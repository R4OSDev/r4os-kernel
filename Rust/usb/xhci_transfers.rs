//! Control, bulk and interrupt TD submission. A timed-out TD remains owned
//! until exact endpoint recovery/cancellation; no late completion is reused.
use super::{
    bulk_completion,
    event_router::{self, Event, Match},
    wait_types::Deadline,
    xhci::*,
    xhci_constants::*,
    xhci_diag as diag,
    xhci_state::{current, state},
    xhci_types::{ControlRequest, EndpointHandle, IncidentToken, InterruptSubmission},
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
    handle: u32,
    retain: bool,
}
impl Drop for TransferLease {
    fn drop(&mut self) {
        if !self.retain {
            unsafe {
                (*state()).transfer_objects.release(self.handle);
            }
        }
    }
}
pub(super) unsafe fn take_last_sync_transfer_incident_token() -> IncidentToken {
    unsafe {
        let token = (*state()).last_sync_transfer_incident;
        (*state()).last_sync_transfer_incident = IncidentToken::EMPTY;
        token
    }
}
pub(super) unsafe fn close_last_sync_transfer_incident() {
    unsafe {
        if (*state()).last_sync_transfer_incident.valid() {
            diag::resolve((*state()).last_sync_transfer_incident);
            (*state()).last_sync_transfer_incident = IncidentToken::EMPTY;
        }
    }
}
pub(super) unsafe fn submit_control_within(req: ControlRequest, budget: Option<&Deadline>) -> bool {
    unsafe {
        if !selected_allows_mutation() {
            return false;
        }
        close_last_sync_transfer_incident();
        if (*current()).addressed_slot_id == 0
            || (*state()).first_ep0_ring_virt == 0
            || (*current()).control_endpoint_faulted
            || (req.direction != 0 && (req.length == 0 || req.data_phys == 0))
            || (*state())
                .transfer_objects
                .timed_out_handle((*current()).addressed_slot_id, 1)
                .is_some()
            || budget.is_some_and(Deadline::expired_any)
        {
            return false;
        }
        record_control_request(req);
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
            setup,
            8,
            trb_type(TRB_TYPE_SETUP_STAGE) | TRB_IDT | setup_type,
        );
        let mut count = 1usize;
        if req.direction != 0 {
            pointers[count] = write_transfer_trb(
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
            Match::transfer_td((*current()).addressed_slot_id, 1, &pointers, count as u8);
        let Some(handle) = (*state())
            .transfer_objects
            .begin(0, expected, timer::tick_count())
        else {
            return false;
        };
        let mut lease = TransferLease {
            handle,
            retain: false,
        };
        dma_fence();
        (*current()).control_transfers += 1;
        write_doorbell((*current()).addressed_slot_id, 1);
        let Some(event) = wait_transfer_completion_within(expected, budget) else {
            (*state()).transfer_objects.mark_timeout(handle);
            lease.retain = true;
            (*current()).last_control_completion_code = 0xff;
            if !(*current()).last_wait_cancelled {
                (*current()).control_timeouts += 1;
            }
            (*current()).control_failures += 1;
            if !(*current()).last_wait_cancelled {
                recover_control_endpoint(budget);
            }
            close_last_sync_transfer_incident();
            return false;
        };
        (*state()).transfer_objects.complete(handle, event);
        (*current()).last_transfer_completion_code = event.code;
        (*current()).last_control_completion_code = event.code;
        (*current()).last_control_residue = event.length;
        (*current()).last_control_ok = (event.code == COMPLETION_SUCCESS && event.length == 0)
            || (req.direction == 1
                && event.code == COMPLETION_SHORT_PACKET
                && event.length <= u32::from(req.length));
        if (*current()).last_control_ok {
            if req.length != 0 && event.length != 0 {
                (*current()).control_short_packets += 1;
            }
            return true;
        }
        (*current()).control_failures += 1;
        if event.code == COMPLETION_STALL_ERROR {
            (*current()).control_stalls += 1;
        }
        (*current()).reason = Text::literal(b"xHCI control transfer failed");
        recover_control_endpoint(budget);
        false
    }
}
unsafe fn record_control_request(req: ControlRequest) {
    unsafe {
        (*current()).last_control_request_type = req.request_type;
        (*current()).last_control_request = req.request;
        (*current()).last_control_value = req.value;
        (*current()).last_control_index = req.index_value;
        (*current()).last_control_length = req.length;
        (*current()).last_control_direction = Text::literal(match req.direction {
            0 => b"none",
            1 => b"in",
            2 => b"out",
            _ => panic!("Typed control direction"),
        });
        (*current()).last_control_completion_code = 0;
        (*current()).last_control_residue = 0;
        (*current()).last_control_ok = false;
    }
}
pub(super) unsafe fn submit_control_no_data(kind: u8, request: u8, value: u16, index: u16) -> bool {
    unsafe { submit_control_no_data_within(kind, request, value, index, None) }
}
pub(super) unsafe fn submit_control_no_data_within(
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        submit_control_within(
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
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    physical: u64,
) -> bool {
    unsafe { submit_control_in_typed(0x80, request, value, index, length, physical) }
}
pub(super) unsafe fn submit_control_in_typed(
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    physical: u64,
) -> bool {
    unsafe {
        submit_control_within(
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
pub(super) unsafe fn bulk_out(data: &[u8]) -> bool {
    unsafe {
        if !selected_allows_mutation() {
            return false;
        }
        begin_bulk_record(false, data.len());
        if !(*current()).bulk_endpoints_configured
            || (*current()).bulk_endpoints_faulted
            || (*state()).first_bulk_out_ring_virt == 0
            || (*state()).first_bulk_buffer_virt == 0
        {
            (*current()).last_bulk_result = Text::literal(b"not-ready");
            return false;
        }
        if data.is_empty() || data.len() > BULK_BUFFER_BYTES {
            (*current()).last_bulk_result = Text::literal(b"bad-length");
            return false;
        }
        if (*state())
            .transfer_objects
            .timed_out_handle(
                (*current()).addressed_slot_id,
                (*current()).bulk_out_endpoint_id,
            )
            .is_some()
        {
            (*current()).last_bulk_result = Text::literal(b"recovery-required");
            return false;
        }
        ptr::copy_nonoverlapping(
            data.as_ptr(),
            (*state()).first_bulk_buffer_virt as *mut u8,
            data.len(),
        );
        bulk_transfer(false, data.len() as u32)
    }
}
pub(super) unsafe fn bulk_in(out: &mut [u8]) -> bool {
    unsafe {
        if !selected_allows_mutation() {
            return false;
        }
        begin_bulk_record(true, out.len());
        if !(*current()).bulk_endpoints_configured
            || (*current()).bulk_endpoints_faulted
            || (*state()).first_bulk_in_ring_virt == 0
            || (*state()).first_bulk_buffer_virt == 0
        {
            (*current()).last_bulk_result = Text::literal(b"not-ready");
            return false;
        }
        if out.is_empty() || out.len() > BULK_BUFFER_BYTES {
            (*current()).last_bulk_result = Text::literal(b"bad-length");
            return false;
        }
        if (*state())
            .transfer_objects
            .timed_out_handle(
                (*current()).addressed_slot_id,
                (*current()).bulk_in_endpoint_id,
            )
            .is_some()
        {
            (*current()).last_bulk_result = Text::literal(b"recovery-required");
            return false;
        }
        ptr::write_bytes((*state()).first_bulk_buffer_virt as *mut u8, 0, out.len());
        if !bulk_transfer(true, out.len() as u32) {
            return false;
        }
        ptr::copy_nonoverlapping(
            (*state()).first_bulk_buffer_virt as *const u8,
            out.as_mut_ptr(),
            (*current()).last_bulk_actual_len as usize,
        );
        true
    }
}
// The original IN/OUT paths share only their identical submission/retention
// sequence; direction-specific preconditions and copies remain above.
unsafe fn bulk_transfer(input: bool, bytes: u32) -> bool {
    unsafe {
        let Some(submission) = write_bulk_trb_chain(input, (*current()).bulk_buffer_phys, bytes)
        else {
            (*current()).last_bulk_result = Text::literal(b"chain-failed");
            return false;
        };
        let endpoint = if input {
            (*current()).bulk_in_endpoint_id
        } else {
            (*current()).bulk_out_endpoint_id
        };
        let expected = Match::transfer_td(
            (*current()).addressed_slot_id,
            endpoint,
            &submission.trb_phys,
            submission.trb_count,
        );
        let Some(handle) = (*state()).transfer_objects.begin(
            if input { 1 } else { 2 },
            expected,
            timer::tick_count(),
        ) else {
            (*current()).last_bulk_result = Text::literal(b"transfer-table-full");
            return false;
        };
        let mut lease = TransferLease {
            handle,
            retain: false,
        };
        dma_fence();
        (*current()).bulk_transfers += 1;
        write_doorbell((*current()).addressed_slot_id, u32::from(endpoint));
        let Some(event) = wait_transfer_completion_within(expected, None) else {
            (*state()).transfer_objects.mark_timeout(handle);
            lease.retain = true;
            (*current()).bulk_failures += 1;
            (*current()).last_bulk_result = Text::literal(if (*current()).last_wait_cancelled {
                b"device-detached"
            } else {
                b"timeout"
            });
            return false;
        };
        (*state()).transfer_objects.complete(handle, event);
        (*current()).last_transfer_completion_code = event.code;
        record_bulk_completion(input, bytes, event);
        let evaluated = bulk_completion::evaluate(input, bytes, event.code, event.length);
        if !evaluated.accepted {
            (*current()).bulk_failures += 1;
            return false;
        }
        true
    }
}
pub(super) unsafe fn bulk_out_for_handle(handle: *mut EndpointHandle, data: &[u8]) -> bool {
    unsafe {
        (*handle).kind == 3
            && select_device_handle(ptr::addr_of_mut!((*handle).device))
            && (*current()).bulk_endpoints_configured
            && (*current()).bulk_out_endpoint_address == (*handle).address
            && bulk_out(data)
    }
}
pub(super) unsafe fn bulk_in_for_handle(handle: *mut EndpointHandle, out: &mut [u8]) -> bool {
    unsafe {
        (*handle).kind == 2
            && select_device_handle(ptr::addr_of_mut!((*handle).device))
            && (*current()).bulk_endpoints_configured
            && (*current()).bulk_in_endpoint_address == (*handle).address
            && bulk_in(out)
    }
}
unsafe fn begin_bulk_record(input: bool, requested: usize) {
    unsafe {
        close_last_sync_transfer_incident();
        (*current()).last_bulk_direction = Text::literal(if input { b"in" } else { b"out" });
        (*current()).last_bulk_result = Text::literal(b"submitted");
        (*current()).last_bulk_request_len = requested.min(u32::MAX as usize) as u32;
        (*current()).last_bulk_residue = 0;
        (*current()).last_bulk_actual_len = 0;
        (*current()).last_bulk_completion_code = 0;
    }
}
unsafe fn record_bulk_completion(input: bool, requested: u32, event: Event) {
    unsafe {
        (*current()).last_bulk_completion_code = event.code;
        (*current()).last_bulk_residue = event.length;
        let eval = bulk_completion::evaluate(input, requested, event.code, event.length);
        (*current()).last_bulk_actual_len = eval.actual_len;
        (*current()).last_bulk_result = Text::literal(if eval.accepted {
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
    handle: *mut EndpointHandle,
    out: &mut [u8],
) -> InterruptPollResult {
    unsafe {
        if (*handle).kind != 1
            || !select_device_handle(ptr::addr_of_mut!((*handle).device))
            || (*current()).interrupt_endpoint_faulted
        {
            return InterruptPollResult::Failed;
        }
        if (!(*current()).interrupt_endpoint_configured
            || (*current()).interrupt_endpoint_address != (*handle).address)
            && !configure_interrupt_in_endpoint_handle(handle)
        {
            return InterruptPollResult::Failed;
        }
        poll_first_interrupt_in_report_status(out)
    }
}
pub(super) unsafe fn poll_first_interrupt_in_report_status(out: &mut [u8]) -> InterruptPollResult {
    unsafe {
        if !(*current()).interrupt_endpoint_configured
            || (*state()).first_interrupt_ring_virt == 0
            || (*state()).first_interrupt_buffer_virt == 0
            || out.is_empty()
            || out.len() > INTERRUPT_BUFFER_BYTES
        {
            return InterruptPollResult::Failed;
        }
        (*current()).interrupt_polls += 1;
        if (*current()).interrupt_pending && (*current()).interrupt_pending_trb_phys == 0 {
            release_interrupt_transfer(true);
            (*current()).interrupt_pending = false;
            (*current()).interrupt_pending_streak = 0;
        }
        if !(*current()).interrupt_pending {
            ptr::write_bytes(
                (*state()).first_interrupt_buffer_virt as *mut u8,
                0,
                out.len(),
            );
            (*current()).last_interrupt_request_len = out.len() as u32;
            (*current()).last_interrupt_residue = 0;
            (*current()).last_interrupt_actual_len = 0;
            (*current()).last_interrupt_completion_code = 0;
            let submission = write_interrupt_trb(
                (*current()).interrupt_buffer_phys,
                out.len() as u32,
                trb_type(TRB_TYPE_NORMAL) | TRB_IOC,
            );
            (*current()).interrupt_pending_trb_phys = submission.trb_phys;
            let expected = Match::transfer(
                (*current()).addressed_slot_id,
                (*current()).interrupt_endpoint_id,
                submission.trb_phys,
            );
            let Some(handle) = (*state())
                .transfer_objects
                .begin(3, expected, timer::tick_count())
            else {
                (*current()).interrupt_pending_trb_phys = 0;
                (*current()).failures += 1;
                return InterruptPollResult::Failed;
            };
            (*current()).interrupt_transfer_handle = handle;
            dma_fence();
            (*current()).interrupt_pending = true;
            (*current()).interrupt_pending_streak = 0;
            write_doorbell(
                (*current()).addressed_slot_id,
                u32::from((*current()).interrupt_endpoint_id),
            );
            if submission.advance.wrapped {
                (*current()).interrupt_ring_wraps += 1;
                emit_interrupt_wrap_marker(submission);
            }
        }
        let expected = Match::transfer(
            (*current()).addressed_slot_id,
            (*current()).interrupt_endpoint_id,
            (*current()).interrupt_pending_trb_phys,
        );
        let Some(event) = poll_matching_event(expected) else {
            (*current()).interrupt_no_report += 1;
            (*current()).interrupt_pending_streak =
                (*current()).interrupt_pending_streak.wrapping_add(1);
            if (*current()).interrupt_pending_streak & PENDING_HALT_CHECK_MASK == 0
                && let Some(s) = interrupt_endpoint_state()
            {
                (*current()).last_interrupt_ep_state = s;
                if s == EP_STATE_HALTED || s == EP_STATE_ERROR {
                    recover_interrupt_endpoint(true);
                }
            }
            return InterruptPollResult::NoReport;
        };
        if (*current()).interrupt_transfer_handle != 0 {
            (*state())
                .transfer_objects
                .complete((*current()).interrupt_transfer_handle, event);
            (*state())
                .transfer_objects
                .release((*current()).interrupt_transfer_handle);
            (*current()).interrupt_transfer_handle = 0;
        }
        (*current()).last_transfer_completion_code = event.code;
        (*current()).last_interrupt_completion_code = event.code;
        (*current()).last_interrupt_residue = event.length;
        (*current()).interrupt_pending = false;
        (*current()).interrupt_pending_trb_phys = 0;
        (*current()).interrupt_pending_streak = 0;
        let requested = (*current()).last_interrupt_request_len;
        let ok = (event.code == COMPLETION_SUCCESS && event.length == 0)
            || (event.code == COMPLETION_SHORT_PACKET && event.length <= requested);
        if !ok {
            (*current()).last_interrupt_actual_len = 0;
            (*current()).failures += 1;
            recover_interrupt_endpoint(false);
            return InterruptPollResult::NoReport;
        }
        (*current()).last_interrupt_actual_len = requested - event.length;
        if (*current()).last_interrupt_actual_len == 0 {
            (*current()).interrupt_no_report += 1;
            return InterruptPollResult::NoReport;
        }
        let count = out
            .len()
            .min((*current()).last_interrupt_actual_len as usize);
        ptr::copy_nonoverlapping(
            (*state()).first_interrupt_buffer_virt as *const u8,
            out.as_mut_ptr(),
            count,
        );
        (*current()).interrupt_reports += 1;
        InterruptPollResult::Report
    }
}
unsafe fn interrupt_endpoint_state() -> Option<u8> {
    unsafe {
        if (*current()).interrupt_endpoint_id == 0 {
            return None;
        }
        (*current()).interrupt_halted_checks = (*current()).interrupt_halted_checks.wrapping_add(1);
        endpoint_state((*current()).interrupt_endpoint_id).map(|s| s as u8)
    }
}
pub(super) unsafe fn read_interrupt_hardware_dequeue() -> u64 {
    unsafe {
        if (*state()).first_device_virt == 0 || (*current()).interrupt_endpoint_id == 0 {
            return 0;
        }
        let address = (*state()).first_device_virt
            + u64::from((*current()).context_size) * u64::from((*current()).interrupt_endpoint_id);
        (u64::from(volatile_read32(address + 12)) << 32) | u64::from(volatile_read32(address + 8))
    }
}
unsafe fn recover_interrupt_endpoint(from_pending: bool) -> bool {
    unsafe {
        let slot = (*current()).addressed_slot_id;
        let ep = (*current()).interrupt_endpoint_id;
        if slot == 0 || ep == 0 {
            return false;
        }
        if (*current()).interrupt_recoveries < 4 {
            puts(b"[USBHIDRECOVERY] slot=");
            diag::console_dec(slot);
            puts(b" endpoint=");
            diag::console_dec(ep);
            puts(b" port=");
            diag::console_dec((*current()).addressed_port);
            puts(b" vid=");
            diag::console_hex((*current()).device_vendor_id, 4);
            puts(b" pid=");
            diag::console_hex((*current()).device_product_id, 4);
            puts(b" from=");
            puts(if from_pending {
                b"pending-state"
            } else {
                b"completion"
            });
            puts(b" code=");
            diag::console_dec((*current()).last_interrupt_completion_code);
            puts(b" request=");
            diag::console_dec((*current()).last_interrupt_request_len);
            puts(b" residue=");
            diag::console_dec((*current()).last_interrupt_residue);
            puts(b" last-state=");
            diag::console_dec((*current()).last_interrupt_ep_state);
            puts(b"\r\n");
        }
        let pending = current_interrupt_owner_match();
        if let Some(m) = pending {
            (*state()).deferred_events.purge(m);
        }
        release_interrupt_transfer(true);
        (*current()).interrupt_recoveries = (*current()).interrupt_recoveries.wrapping_add(1);
        if from_pending {
            (*current()).interrupt_pending_timeouts =
                (*current()).interrupt_pending_timeouts.wrapping_add(1);
        }
        let ok = reset_first_endpoint_state((*current()).interrupt_endpoint_address, None);
        if let Some(m) = pending {
            (*state()).deferred_events.purge(m);
        }
        (*current()).interrupt_pending = false;
        (*current()).interrupt_pending_trb_phys = 0;
        (*current()).interrupt_pending_streak = 0;
        dma_fence();
        ok
    }
}
pub(super) unsafe fn release_interrupt_transfer(cancel: bool) {
    unsafe {
        let handle = (*current()).interrupt_transfer_handle;
        if handle == 0 {
            return;
        }
        if cancel {
            (*state()).transfer_objects.cancel(handle);
        }
        (*state()).transfer_objects.release(handle);
        (*current()).interrupt_transfer_handle = 0;
    }
}
unsafe fn emit_interrupt_wrap_marker(submission: InterruptSubmission) {
    unsafe {
        let deferred = (*state()).deferred_events.snapshot();
        let advance = submission.advance;
        puts(b"[USBHIDWRAP] wraps=");
        diag::console_dec((*current()).interrupt_ring_wraps);
        puts(b" submitted_enqueue=");
        diag::console_dec(submission.enqueue);
        puts(b" enqueue=");
        diag::console_dec((*current()).interrupt_enqueue);
        puts(b" next_enqueue=");
        diag::console_dec(advance.next_enqueue);
        puts(b" submitted_pcs=");
        diag::console_dec(submission.producer_cycle);
        puts(b" pcs=");
        diag::console_dec((*current()).interrupt_cycle);
        puts(b" next_producer_cycle=");
        diag::console_dec(advance.next_producer_cycle);
        puts(b" link_c=");
        diag::console_dec(advance.link_cycle);
        puts(b" reports=");
        diag::console_dec((*current()).interrupt_reports);
        puts(b" recoveries=");
        diag::console_dec((*current()).interrupt_recoveries);
        puts(b" pending_timeouts=");
        diag::console_dec((*current()).interrupt_pending_timeouts);
        puts(b" deferred_overflows=");
        diag::console_dec(deferred.overflows);
        puts(b" event_wraps=");
        diag::console_dec((*current()).event_ring_wraps);
        puts(b" event_commits=");
        diag::console_dec((*current()).event_erdp_commits);
        puts(b" ring_full=");
        diag::console_dec((*current()).ring_full);
        puts(b" stale_events=");
        diag::console_dec((*current()).stale_events);
        puts(b"\r\n");
    }
}
