//! Control, bulk and interrupt TD submission. A timed-out TD remains owned
//! until exact endpoint recovery/cancellation; no late completion is reused.
use super::xhci_state::Context;
use super::{
    bulk_completion,
    event_router::{self, Event, Match},
    wait_types::Deadline,
    xhci::*,
    xhci_constants::*,
    xhci_diag as diag,
    xhci_state::{current, state},
    xhci_types::{ControlRequest, EndpointHandle, IncidentToken, InterruptSubmission, Trb},
};
use crate::{boot::foundation::puts, platform::timer, sched::sync_types::Text};
use core::ptr;
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InterruptPollResult {
    Report = 0,
    NoReport = 1,
    Failed = 2,
}
struct TransferLease {
    ctx: Context,
    handle: u32,
    retain: bool,
}
impl Drop for TransferLease {
    fn drop(&mut self) {
        let ctx = self.ctx;
        if !self.retain {
            unsafe {
                (*state(ctx)).transfer_objects.release(self.handle);
            }
        }
    }
}
pub(super) unsafe fn take_last_sync_transfer_incident_token(ctx: Context) -> IncidentToken {
    unsafe {
        let token = (*state(ctx)).last_sync_transfer_incident;
        (*state(ctx)).last_sync_transfer_incident = IncidentToken::EMPTY;
        token
    }
}
pub(super) unsafe fn close_last_sync_transfer_incident(ctx: Context) {
    unsafe {
        if (*state(ctx)).last_sync_transfer_incident.valid() {
            diag::resolve((*state(ctx)).last_sync_transfer_incident);
            (*state(ctx)).last_sync_transfer_incident = IncidentToken::EMPTY;
        }
    }
}
pub(super) unsafe fn submit_control_within(
    ctx: Context,
    req: ControlRequest,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        close_last_sync_transfer_incident(ctx);
        if (*current(ctx)).addressed_slot_id == 0
            || (*state(ctx)).first_ep0_ring_virt == 0
            || (*current(ctx)).control_endpoint_faulted
            || (req.direction != 0 && (req.length == 0 || req.data_phys == 0))
            || (*state(ctx))
                .transfer_objects
                .timed_out_handle((*current(ctx)).addressed_slot_id, 1)
                .is_some()
            || budget.is_some_and(Deadline::expired_any)
        {
            return false;
        }
        record_control_request(ctx, req);
        let setup_type = match req.direction {
            0 => 0,
            1 => SETUP_TRT_IN_DATA,
            2 => SETUP_TRT_OUT_DATA,
            _ => panic!("Typed control direction"),
        };
        let setup = u64::from(req.request_type)
            | (u64::from(req.request) << 8)
            | (u64::from(req.value) << 16)
            | (u64::from(req.index_value) << 32)
            | (u64::from(req.length) << 48);
        let mut pointers = [0; event_router::MAX_TRANSFER_TRB_POINTERS];
        pointers[0] = write_transfer_trb(
            ctx,
            setup,
            8,
            trb_type(TRB_TYPE_SETUP_STAGE) | TRB_IDT | setup_type,
        );
        let mut count = 1usize;
        if req.direction != 0 {
            pointers[count] = write_transfer_trb(
                ctx,
                req.data_phys,
                u32::from(req.length),
                trb_type(TRB_TYPE_DATA_STAGE)
                    | if req.direction == 1 {
                        DATA_STAGE_DIR_IN
                    } else {
                        0
                    },
            );
            count += 1;
        }
        pointers[count] = write_transfer_trb(
            ctx,
            0,
            0,
            trb_type(TRB_TYPE_STATUS_STAGE)
                | TRB_IOC
                | if req.direction != 1 {
                    STATUS_STAGE_DIR_IN
                } else {
                    0
                },
        );
        count += 1;
        let expected =
            Match::transfer_td((*current(ctx)).addressed_slot_id, 1, &pointers, count as u8);
        let Some(handle) = (*state(ctx))
            .transfer_objects
            .begin(0, expected, timer::tick_count())
        else {
            return false;
        };
        let mut lease = TransferLease {
            ctx,
            handle,
            retain: false,
        };
        dma_fence();
        (*current(ctx)).control_transfers += 1;
        write_doorbell(ctx, (*current(ctx)).addressed_slot_id, 1);
        let Some(event) = wait_transfer_completion_within(ctx, expected, budget) else {
            (*state(ctx)).transfer_objects.mark_timeout(handle);
            lease.retain = true;
            (*current(ctx)).last_control_completion_code = 0xff;
            if !(*current(ctx)).last_wait_cancelled {
                (*current(ctx)).control_timeouts += 1;
            }
            (*current(ctx)).control_failures += 1;
            if !(*current(ctx)).last_wait_cancelled {
                recover_control_endpoint(ctx, budget);
            }
            close_last_sync_transfer_incident(ctx);
            return false;
        };
        (*state(ctx)).transfer_objects.complete(handle, event);
        (*current(ctx)).last_transfer_completion_code = event.code;
        (*current(ctx)).last_control_completion_code = event.code;
        (*current(ctx)).last_control_residue = event.length;
        (*current(ctx)).last_control_ok = (event.code == COMPLETION_SUCCESS && event.length == 0)
            || (req.direction == 1
                && event.code == COMPLETION_SHORT_PACKET
                && event.length <= u32::from(req.length));
        if (*current(ctx)).last_control_ok {
            if req.length != 0 && event.length != 0 {
                (*current(ctx)).control_short_packets += 1;
            }
            return true;
        }
        (*current(ctx)).control_failures += 1;
        if event.code == COMPLETION_STALL_ERROR {
            (*current(ctx)).control_stalls += 1;
        }
        (*current(ctx)).reason = Text::literal(b"xHCI control transfer failed");
        recover_control_endpoint(ctx, budget);
        false
    }
}
unsafe fn record_control_request(ctx: Context, req: ControlRequest) {
    unsafe {
        (*current(ctx)).last_control_request_type = req.request_type;
        (*current(ctx)).last_control_request = req.request;
        (*current(ctx)).last_control_value = req.value;
        (*current(ctx)).last_control_index = req.index_value;
        (*current(ctx)).last_control_length = req.length;
        (*current(ctx)).last_control_direction = Text::literal(match req.direction {
            0 => b"none",
            1 => b"in",
            2 => b"out",
            _ => panic!("Typed control direction"),
        });
        (*current(ctx)).last_control_completion_code = 0;
        (*current(ctx)).last_control_residue = 0;
        (*current(ctx)).last_control_ok = false;
    }
}
pub(super) unsafe fn submit_control_no_data(
    ctx: Context,
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
) -> bool {
    unsafe { submit_control_no_data_within(ctx, kind, request, value, index, None) }
}
pub(super) unsafe fn submit_control_no_data_within(
    ctx: Context,
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        submit_control_within(
            ctx,
            ControlRequest {
                request_type: kind,
                request,
                value,
                index_value: index,
                ..ControlRequest::EMPTY
            },
            budget,
        )
    }
}
pub(super) unsafe fn submit_control_in(
    ctx: Context,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    physical: u64,
) -> bool {
    unsafe { submit_control_in_typed(ctx, 0x80, request, value, index, length, physical) }
}
pub(super) unsafe fn submit_control_in_typed(
    ctx: Context,
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    physical: u64,
) -> bool {
    unsafe {
        submit_control_within(
            ctx,
            ControlRequest {
                request_type: kind,
                request,
                value,
                index_value: index,
                length,
                data_phys: physical,
                direction: 1,
            },
            None,
        )
    }
}
pub(super) unsafe fn bulk_out(ctx: Context, data: &[u8]) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        begin_bulk_record(ctx, false, data.len());
        if !(*current(ctx)).bulk_endpoints_configured
            || (*current(ctx)).bulk_endpoints_faulted
            || (*state(ctx)).first_bulk_out_ring_virt == 0
            || (*state(ctx)).first_bulk_buffer_virt == 0
        {
            (*current(ctx)).last_bulk_result = Text::literal(b"not-ready");
            return false;
        }
        if data.is_empty() || data.len() > BULK_BUFFER_BYTES {
            (*current(ctx)).last_bulk_result = Text::literal(b"bad-length");
            return false;
        }
        if (*state(ctx))
            .transfer_objects
            .timed_out_handle(
                (*current(ctx)).addressed_slot_id,
                (*current(ctx)).bulk_out_endpoint_id,
            )
            .is_some()
        {
            (*current(ctx)).last_bulk_result = Text::literal(b"recovery-required");
            return false;
        }
        (*state(ctx)).bulk_out_requested_bytes = (*state(ctx))
            .bulk_out_requested_bytes
            .wrapping_add(data.len() as u64);
        ptr::copy_nonoverlapping(
            data.as_ptr(),
            (*state(ctx)).first_bulk_buffer_virt as *mut u8,
            data.len(),
        );
        bulk_transfer(ctx, false, data.len() as u32)
    }
}
pub(super) unsafe fn bulk_in(ctx: Context, out: &mut [u8]) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        begin_bulk_record(ctx, true, out.len());
        if !(*current(ctx)).bulk_endpoints_configured
            || (*current(ctx)).bulk_endpoints_faulted
            || (*state(ctx)).first_bulk_in_ring_virt == 0
            || (*state(ctx)).first_bulk_buffer_virt == 0
        {
            (*current(ctx)).last_bulk_result = Text::literal(b"not-ready");
            return false;
        }
        if out.is_empty() || out.len() > BULK_BUFFER_BYTES {
            (*current(ctx)).last_bulk_result = Text::literal(b"bad-length");
            return false;
        }
        if (*state(ctx))
            .transfer_objects
            .timed_out_handle(
                (*current(ctx)).addressed_slot_id,
                (*current(ctx)).bulk_in_endpoint_id,
            )
            .is_some()
        {
            (*current(ctx)).last_bulk_result = Text::literal(b"recovery-required");
            return false;
        }
        (*state(ctx)).bulk_in_requested_bytes = (*state(ctx))
            .bulk_in_requested_bytes
            .wrapping_add(out.len() as u64);
        (*state(ctx)).bulk_cleared_bytes = (*state(ctx))
            .bulk_cleared_bytes
            .wrapping_add(out.len() as u64);
        ptr::write_bytes(
            (*state(ctx)).first_bulk_buffer_virt as *mut u8,
            0,
            out.len(),
        );
        if !bulk_transfer(ctx, true, out.len() as u32) {
            return false;
        }
        ptr::copy_nonoverlapping(
            (*state(ctx)).first_bulk_buffer_virt as *const u8,
            out.as_mut_ptr(),
            (*current(ctx)).last_bulk_actual_len as usize,
        );
        true
    }
}
// The original IN/OUT paths share only their identical submission/retention
// sequence; direction-specific preconditions and copies remain above.
unsafe fn bulk_transfer(ctx: Context, input: bool, bytes: u32) -> bool {
    unsafe {
        let Some(submission) =
            write_bulk_trb_chain(ctx, input, (*current(ctx)).bulk_buffer_phys, bytes)
        else {
            (*current(ctx)).last_bulk_result = Text::literal(b"chain-failed");
            return false;
        };
        let endpoint = if input {
            (*current(ctx)).bulk_in_endpoint_id
        } else {
            (*current(ctx)).bulk_out_endpoint_id
        };
        let expected = Match::transfer_td(
            (*current(ctx)).addressed_slot_id,
            endpoint,
            &submission.trb_phys,
            submission.trb_count,
        );
        let Some(handle) = (*state(ctx)).transfer_objects.begin(
            if input { 1 } else { 2 },
            expected,
            timer::tick_count(),
        ) else {
            (*current(ctx)).last_bulk_result = Text::literal(b"transfer-table-full");
            return false;
        };
        let mut lease = TransferLease {
            ctx,
            handle,
            retain: false,
        };
        dma_fence();
        (*current(ctx)).bulk_transfers += 1;
        write_doorbell(ctx, (*current(ctx)).addressed_slot_id, u32::from(endpoint));
        let Some(event) = wait_transfer_completion_within(ctx, expected, None) else {
            (*state(ctx)).transfer_objects.mark_timeout(handle);
            lease.retain = true;
            (*current(ctx)).bulk_failures += 1;
            (*current(ctx)).last_bulk_result =
                Text::literal(if (*current(ctx)).last_wait_cancelled {
                    b"device-detached"
                } else {
                    b"timeout"
                });
            return false;
        };
        (*state(ctx)).transfer_objects.complete(handle, event);
        (*current(ctx)).last_transfer_completion_code = event.code;
        record_bulk_completion(ctx, input, bytes, event);
        let evaluated = bulk_completion::evaluate(input, bytes, event.code, event.length);
        if !evaluated.accepted {
            (*current(ctx)).bulk_failures += 1;
            return false;
        }
        true
    }
}
pub(super) unsafe fn bulk_out_for_handle(
    ctx: Context,
    handle: *mut EndpointHandle,
    data: &[u8],
) -> bool {
    unsafe {
        (*handle).kind == 3
            && select_device_handle(ctx, ptr::addr_of_mut!((*handle).device))
            && (*current(ctx)).bulk_endpoints_configured
            && (*current(ctx)).bulk_out_endpoint_address == (*handle).address
            && bulk_out(ctx, data)
    }
}
pub(super) unsafe fn bulk_in_for_handle(
    ctx: Context,
    handle: *mut EndpointHandle,
    out: &mut [u8],
) -> bool {
    unsafe {
        (*handle).kind == 2
            && select_device_handle(ctx, ptr::addr_of_mut!((*handle).device))
            && (*current(ctx)).bulk_endpoints_configured
            && (*current(ctx)).bulk_in_endpoint_address == (*handle).address
            && bulk_in(ctx, out)
    }
}
unsafe fn begin_bulk_record(ctx: Context, input: bool, requested: usize) {
    unsafe {
        close_last_sync_transfer_incident(ctx);
        (*current(ctx)).last_bulk_direction = Text::literal(if input { b"in" } else { b"out" });
        (*current(ctx)).last_bulk_result = Text::literal(b"submitted");
        (*current(ctx)).last_bulk_request_len = requested.min(u32::MAX as usize) as u32;
        (*current(ctx)).last_bulk_residue = 0;
        (*current(ctx)).last_bulk_actual_len = 0;
        (*current(ctx)).last_bulk_completion_code = 0;
    }
}
unsafe fn record_bulk_completion(ctx: Context, input: bool, requested: u32, event: Event) {
    unsafe {
        (*current(ctx)).last_bulk_completion_code = event.code;
        (*current(ctx)).last_bulk_residue = event.length;
        let eval = bulk_completion::evaluate(input, requested, event.code, event.length);
        (*current(ctx)).last_bulk_actual_len = eval.actual_len;
        (*current(ctx)).last_bulk_result = Text::literal(if eval.accepted {
            if event.code == COMPLETION_SHORT_PACKET {
                b"short"
            } else {
                b"ok"
            }
        } else if event.length > requested
            || (event.code == COMPLETION_SUCCESS && event.length != 0)
        {
            b"bad-residue"
        } else {
            b"completion-error"
        });
    }
}
pub(super) unsafe fn poll_interrupt_in_report_status(
    ctx: Context,
    handle: *mut EndpointHandle,
    out: &mut [u8],
) -> InterruptPollResult {
    unsafe {
        if (*handle).kind != 1
            || !select_device_handle(ctx, ptr::addr_of_mut!((*handle).device))
            || (*current(ctx)).interrupt_endpoint_faulted
        {
            return InterruptPollResult::Failed;
        }
        if (!(*current(ctx)).interrupt_endpoint_configured
            || (*current(ctx)).interrupt_endpoint_address != (*handle).address)
            && !configure_interrupt_in_endpoint_handle(ctx, handle)
        {
            return InterruptPollResult::Failed;
        }
        poll_first_interrupt_in_report_status(ctx, out)
    }
}
pub(super) unsafe fn poll_first_interrupt_in_report_status(
    ctx: Context,
    out: &mut [u8],
) -> InterruptPollResult {
    unsafe {
        if !(*current(ctx)).interrupt_endpoint_configured
            || (*state(ctx)).first_interrupt_ring_virt == 0
            || (*state(ctx)).first_interrupt_buffer_virt == 0
            || out.is_empty()
            || out.len() > INTERRUPT_BUFFER_BYTES
        {
            return InterruptPollResult::Failed;
        }
        (*current(ctx)).interrupt_polls += 1;
        if (*current(ctx)).interrupt_pending && (*current(ctx)).interrupt_pending_trb_phys == 0 {
            release_interrupt_transfer(ctx, true);
            (*current(ctx)).interrupt_pending = false;
            (*current(ctx)).interrupt_pending_streak = 0;
        }
        if !(*current(ctx)).interrupt_pending && !arm_interrupt_report(ctx, out.len()) {
            return InterruptPollResult::Failed;
        }
        let expected = Match::transfer(
            (*current(ctx)).addressed_slot_id,
            (*current(ctx)).interrupt_endpoint_id,
            (*current(ctx)).interrupt_pending_trb_phys,
        );
        let Some(event) = poll_matching_event(ctx, expected) else {
            (*current(ctx)).interrupt_no_report += 1;
            (*current(ctx)).interrupt_pending_streak =
                (*current(ctx)).interrupt_pending_streak.wrapping_add(1);
            if (*current(ctx)).interrupt_pending_streak & PENDING_HALT_CHECK_MASK == 0
                && let Some(s) = interrupt_endpoint_state(ctx)
            {
                (*current(ctx)).last_interrupt_ep_state = s;
                if s == EP_STATE_HALTED || s == EP_STATE_ERROR {
                    recover_interrupt_endpoint(ctx, true);
                }
            }
            return InterruptPollResult::NoReport;
        };
        if (*current(ctx)).interrupt_transfer_handle != 0 {
            (*state(ctx))
                .transfer_objects
                .complete((*current(ctx)).interrupt_transfer_handle, event);
            (*state(ctx))
                .transfer_objects
                .release((*current(ctx)).interrupt_transfer_handle);
            (*current(ctx)).interrupt_transfer_handle = 0;
        }
        (*current(ctx)).last_transfer_completion_code = event.code;
        (*current(ctx)).last_interrupt_completion_code = event.code;
        (*current(ctx)).last_interrupt_residue = event.length;
        (*current(ctx)).interrupt_pending = false;
        (*current(ctx)).interrupt_pending_trb_phys = 0;
        (*current(ctx)).interrupt_pending_streak = 0;
        let requested = (*current(ctx)).last_interrupt_request_len;
        let ok = (event.code == COMPLETION_SUCCESS && event.length == 0)
            || (event.code == COMPLETION_SHORT_PACKET && event.length <= requested);
        if !ok {
            (*current(ctx)).last_interrupt_actual_len = 0;
            (*current(ctx)).failures += 1;
            recover_interrupt_endpoint(ctx, false);
            return InterruptPollResult::NoReport;
        }
        (*current(ctx)).last_interrupt_actual_len = requested - event.length;
        if (*current(ctx)).last_interrupt_actual_len == 0 {
            (*current(ctx)).interrupt_no_report += 1;
            return InterruptPollResult::NoReport;
        }
        let count = out
            .len()
            .min((*current(ctx)).last_interrupt_actual_len as usize);
        ptr::copy_nonoverlapping(
            (*state(ctx)).first_interrupt_buffer_virt as *const u8,
            out.as_mut_ptr(),
            count,
        );
        (*current(ctx)).interrupt_reports += 1;
        // The previous report is in caller storage and its exact ledger entry
        // is retired. Keep its public length/residue metadata while preparing
        // the next one; DMA may only overwrite the private native buffer.
        let subscribed = (*state(ctx)).active_runtime_index.is_some_and(|i| {
            (*state(ctx)).input_generations[i] == (*state(ctx)).runtimes[i].generation
                && (*state(ctx)).input_endpoints[i] == (*current(ctx)).interrupt_endpoint_id
        });
        if subscribed {
            let last = (
                (*current(ctx)).last_interrupt_request_len,
                (*current(ctx)).last_interrupt_residue,
                (*current(ctx)).last_interrupt_actual_len,
                (*current(ctx)).last_interrupt_completion_code,
            );
            let _ = arm_interrupt_report(ctx, out.len());
            (*current(ctx)).last_interrupt_request_len = last.0;
            (*current(ctx)).last_interrupt_residue = last.1;
            (*current(ctx)).last_interrupt_actual_len = last.2;
            (*current(ctx)).last_interrupt_completion_code = last.3;
        }
        InterruptPollResult::Report
    }
}
// Admission/reservation precedes ring publication. A failed immediate
// resubmit leaves the completed report valid and retries on a later poll.
unsafe fn arm_interrupt_report(ctx: Context, bytes: usize) -> bool {
    unsafe {
        let c = current(ctx);
        if (*c).interrupt_pending || (*c).interrupt_endpoint_faulted {
            return false;
        }
        let physical = (*c).interrupt_ring_phys
            + u64::from((*c).interrupt_enqueue) * core::mem::size_of::<Trb>() as u64;
        let expected =
            Match::transfer((*c).addressed_slot_id, (*c).interrupt_endpoint_id, physical);
        let Some(handle) = (*state(ctx))
            .transfer_objects
            .begin(3, expected, timer::tick_count())
        else {
            (*c).failures += 1;
            return false;
        };
        ptr::write_bytes(
            (*state(ctx)).first_interrupt_buffer_virt as *mut u8,
            0,
            bytes,
        );
        (*c).last_interrupt_request_len = bytes as u32;
        (*c).last_interrupt_residue = 0;
        (*c).last_interrupt_actual_len = 0;
        (*c).last_interrupt_completion_code = 0;
        let submission = write_interrupt_trb(
            ctx,
            (*c).interrupt_buffer_phys,
            bytes as u32,
            trb_type(TRB_TYPE_NORMAL) | TRB_IOC,
        );
        assert_eq!(
            submission.trb_phys, physical,
            "interrupt reservation/ring mismatch"
        );
        (*c).interrupt_pending_trb_phys = physical;
        (*c).interrupt_transfer_handle = handle;
        dma_fence();
        (*c).interrupt_pending = true;
        (*c).interrupt_pending_streak = 0;
        write_doorbell(
            ctx,
            (*c).addressed_slot_id,
            u32::from((*c).interrupt_endpoint_id),
        );
        if submission.advance.wrapped {
            (*c).interrupt_ring_wraps += 1;
            emit_interrupt_wrap_marker(ctx, submission);
        }
        true
    }
}
unsafe fn interrupt_endpoint_state(ctx: Context) -> Option<u8> {
    unsafe {
        if (*current(ctx)).interrupt_endpoint_id == 0 {
            return None;
        }
        (*current(ctx)).interrupt_halted_checks =
            (*current(ctx)).interrupt_halted_checks.wrapping_add(1);
        endpoint_state(ctx, (*current(ctx)).interrupt_endpoint_id).map(|s| s as u8)
    }
}
pub(super) unsafe fn read_interrupt_hardware_dequeue(ctx: Context) -> u64 {
    unsafe {
        if (*state(ctx)).first_device_virt == 0 || (*current(ctx)).interrupt_endpoint_id == 0 {
            return 0;
        }
        let address = (*state(ctx)).first_device_virt
            + u64::from((*current(ctx)).context_size)
                * u64::from((*current(ctx)).interrupt_endpoint_id);
        (u64::from(volatile_read32(address + 12)) << 32) | u64::from(volatile_read32(address + 8))
    }
}
unsafe fn recover_interrupt_endpoint(ctx: Context, from_pending: bool) -> bool {
    unsafe {
        let slot = (*current(ctx)).addressed_slot_id;
        let ep = (*current(ctx)).interrupt_endpoint_id;
        if slot == 0 || ep == 0 {
            return false;
        }
        if (*current(ctx)).interrupt_recoveries < 4 {
            puts(b"[USBHIDRECOVERY] slot=");
            diag::console_dec(slot);
            puts(b" endpoint=");
            diag::console_dec(ep);
            puts(b" port=");
            diag::console_dec((*current(ctx)).addressed_port);
            puts(b" vid=");
            diag::console_hex((*current(ctx)).device_vendor_id, 4);
            puts(b" pid=");
            diag::console_hex((*current(ctx)).device_product_id, 4);
            puts(b" from=");
            puts(if from_pending {
                b"pending-state"
            } else {
                b"completion"
            });
            puts(b" code=");
            diag::console_dec((*current(ctx)).last_interrupt_completion_code);
            puts(b" request=");
            diag::console_dec((*current(ctx)).last_interrupt_request_len);
            puts(b" residue=");
            diag::console_dec((*current(ctx)).last_interrupt_residue);
            puts(b" last-state=");
            diag::console_dec((*current(ctx)).last_interrupt_ep_state);
            puts(b"\r\n");
        }
        let pending = current_interrupt_owner_match(ctx);
        if let Some(m) = pending {
            (*state(ctx)).deferred_events.purge(m);
        }
        release_interrupt_transfer(ctx, true);
        (*current(ctx)).interrupt_recoveries = (*current(ctx)).interrupt_recoveries.wrapping_add(1);
        if from_pending {
            (*current(ctx)).interrupt_pending_timeouts =
                (*current(ctx)).interrupt_pending_timeouts.wrapping_add(1);
        }
        let ok = reset_first_endpoint_state(ctx, (*current(ctx)).interrupt_endpoint_address, None);
        if let Some(m) = pending {
            (*state(ctx)).deferred_events.purge(m);
        }
        (*current(ctx)).interrupt_pending = false;
        (*current(ctx)).interrupt_pending_trb_phys = 0;
        (*current(ctx)).interrupt_pending_streak = 0;
        dma_fence();
        ok
    }
}
pub(super) unsafe fn release_interrupt_transfer(ctx: Context, cancel: bool) {
    unsafe {
        let handle = (*current(ctx)).interrupt_transfer_handle;
        if handle == 0 {
            return;
        }
        if cancel {
            (*state(ctx)).transfer_objects.cancel(handle);
        }
        (*state(ctx)).transfer_objects.release(handle);
        (*current(ctx)).interrupt_transfer_handle = 0;
    }
}
unsafe fn emit_interrupt_wrap_marker(ctx: Context, submission: InterruptSubmission) {
    unsafe {
        let deferred = (*state(ctx)).deferred_events.snapshot();
        let advance = submission.advance;
        puts(b"[USBHIDWRAP] wraps=");
        diag::console_dec((*current(ctx)).interrupt_ring_wraps);
        puts(b" submitted_enqueue=");
        diag::console_dec(submission.enqueue);
        puts(b" enqueue=");
        diag::console_dec((*current(ctx)).interrupt_enqueue);
        puts(b" next_enqueue=");
        diag::console_dec(advance.next_enqueue);
        puts(b" submitted_pcs=");
        diag::console_dec(submission.producer_cycle);
        puts(b" pcs=");
        diag::console_dec((*current(ctx)).interrupt_cycle);
        puts(b" next_producer_cycle=");
        diag::console_dec(advance.next_producer_cycle);
        puts(b" link_c=");
        diag::console_dec(advance.link_cycle);
        puts(b" reports=");
        diag::console_dec((*current(ctx)).interrupt_reports);
        puts(b" recoveries=");
        diag::console_dec((*current(ctx)).interrupt_recoveries);
        puts(b" pending_timeouts=");
        diag::console_dec((*current(ctx)).interrupt_pending_timeouts);
        puts(b" deferred_overflows=");
        diag::console_dec(deferred.overflows);
        puts(b" event_wraps=");
        diag::console_dec((*current(ctx)).event_ring_wraps);
        puts(b" event_commits=");
        diag::console_dec((*current(ctx)).event_erdp_commits);
        puts(b" ring_full=");
        diag::console_dec((*current(ctx)).ring_full);
        puts(b" stale_events=");
        diag::console_dec((*current(ctx)).stale_events);
        puts(b"\r\n");
    }
}
