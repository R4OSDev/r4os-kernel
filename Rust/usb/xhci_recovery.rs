//! Generation-bound endpoint recovery. One original budget spans every step;
//! recovering a failed slot never enumerates its replacement on the same port.
use super::xhci_state::Context;
use super::{
    endpoint_recovery::{self, EndpointState, PrepareAction},
    wait_types::Deadline,
    xhci::*,
    xhci_constants::*,
    xhci_diag as diag,
    xhci_state::{current, state},
    xhci_types::DeviceHandle,
};
use crate::{boot::foundation::puts, sched::sync_types::Text};
use core::ptr;
#[derive(Clone, Copy)]
pub(super) enum EndpointFaultClass {
    Control,
    Interrupt,
    Bulk,
}
unsafe fn set_fault(ctx: Context, class: EndpointFaultClass, value: bool) {
    unsafe {
        match class {
            EndpointFaultClass::Control => (*current(ctx)).control_endpoint_faulted = value,
            EndpointFaultClass::Interrupt => (*current(ctx)).interrupt_endpoint_faulted = value,
            EndpointFaultClass::Bulk => (*current(ctx)).bulk_endpoints_faulted = value,
        }
    }
}
pub(super) unsafe fn mark_dci_recovery_failed(ctx: Context, dci: u8, class: EndpointFaultClass) {
    unsafe {
        (*current(ctx)).failures = (*current(ctx)).failures.wrapping_add(1);
        (*current(ctx)).reason = Text::literal(b"xHCI endpoint recovery failed");
        let token = diag::begin();
        diag::write(b"[XHCI] endpoint recovery failed slot=");
        diag::dec((*current(ctx)).addressed_slot_id);
        diag::write(b" ep=");
        diag::dec(dci);
        diag::write(b" state=");
        if let Some(s) = endpoint_state(ctx, dci) {
            diag::dec(s as u8);
        } else {
            diag::write(b"unknown");
        }
        diag::write(b" cc=");
        diag::dec((*current(ctx)).last_completion_code);
        diag::end();
        diag::resolve(token);
        set_fault(ctx, class, true);
    }
}
unsafe fn fault_class(ctx: Context, address: u8) -> EndpointFaultClass {
    if address == unsafe { (*current(ctx)).interrupt_endpoint_address } {
        EndpointFaultClass::Interrupt
    } else {
        EndpointFaultClass::Bulk
    }
}
pub(super) unsafe fn endpoint_recovery_command_within(
    ctx: Context,
    control: u32,
    parameter: u64,
    budget: Option<&Deadline>,
) -> Option<u8> {
    unsafe {
        (*current(ctx)).last_completion_code = 0xff;
        let event = submit_command_within(ctx, parameter, 0, control, budget)?;
        (*current(ctx)).last_completion_code = event.code;
        (*current(ctx)).last_slot_id = event.slot_id;
        Some(event.code)
    }
}
unsafe fn drain_unresolved_transfer_dci(ctx: Context, dci: u8) {
    unsafe {
        let Some(handle) = (*state(ctx))
            .transfer_objects
            .timed_out_handle((*current(ctx)).addressed_slot_id, dci)
        else {
            return;
        };
        let Some(owner) = (*state(ctx)).transfer_objects.match_for_handle(handle) else {
            return;
        };
        let _ = drain_event_batch(ctx, owner);
        (*state(ctx)).deferred_events.purge(owner);
        (*state(ctx)).transfer_objects.cancel(handle);
        (*state(ctx)).transfer_objects.release(handle);
        close_last_sync_transfer_incident(ctx);
    }
}
pub(super) unsafe fn set_configuration_for_handle(ctx: Context, handle: *mut DeviceHandle) -> bool {
    unsafe {
        if !select_device_handle(ctx, handle) {
            return false;
        }
        if (*current(ctx)).set_configuration_ok
            && (*current(ctx)).config_value == (*handle).config_value
        {
            return true;
        }
        set_first_configuration(ctx)
    }
}
pub(super) unsafe fn set_first_configuration(ctx: Context) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        (*current(ctx)).set_configuration_attempted = true;
        (*current(ctx)).set_configuration_ok = submit_control_no_data(
            ctx,
            0,
            USB_REQ_SET_CONFIGURATION,
            u16::from((*current(ctx)).config_value),
            0,
        );
        puts(b"[USBCONFIG] slot=");
        diag::console_dec((*current(ctx)).addressed_slot_id);
        puts(b" port=");
        diag::console_dec((*current(ctx)).addressed_port);
        puts(b" vid=");
        diag::console_hex((*current(ctx)).device_vendor_id, 4);
        puts(b" pid=");
        diag::console_hex((*current(ctx)).device_product_id, 4);
        puts(b" value=");
        diag::console_dec((*current(ctx)).config_value);
        puts(b" result=");
        puts(if (*current(ctx)).set_configuration_ok {
            b"OK"
        } else {
            b"FAILED"
        });
        puts(b" code=");
        diag::console_dec((*current(ctx)).last_control_completion_code);
        puts(b"\r\n");
        (*current(ctx)).set_configuration_ok
    }
}
unsafe fn recovery_port_ready(ctx: Context, port: u8) -> bool {
    unsafe {
        if !(*current(ctx)).present
            || !(*current(ctx)).controller_running
            || (*current(ctx)).mmio_virt == 0
            || port == 0
            || port > (*current(ctx)).port_count_seen
        {
            return false;
        }
        let value = read_op32(ctx, OP_PORT_BASE + u64::from(port - 1) * OP_PORT_STRIDE);
        value & PORTSC_CCS != 0
            && value & PORTSC_PED != 0
            && port_link_state(ctx, value) == PORTSC_PLS_U0
    }
}
pub(super) unsafe fn select_existing_runtime_for_recovery(
    ctx: Context,
    handle: *mut DeviceHandle,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        if budget.is_some_and(Deadline::expired_any) {
            return false;
        }
        let Some(index) = runtime_index_for_identity(ctx, (*handle).slot_id, (*handle).port) else {
            return false;
        };
        let rt = ptr::addr_of!((*state(ctx)).runtimes[index]).read();
        if rt.detach_pending || ((*handle).generation != 0 && rt.generation != (*handle).generation)
        {
            return false;
        }
        if rt.transaction_active && !super::xhci_owner::transaction_owned(ctx, rt) {
            return false;
        }
        if !recovery_port_ready(ctx, (*handle).port) || budget.is_some_and(Deadline::expired_any) {
            return false;
        }
        if !activate_runtime(ctx, index)
            || (*current(ctx)).addressed_slot_id != (*handle).slot_id
            || (*current(ctx)).addressed_port != (*handle).port
        {
            return false;
        }
        if budget.is_some_and(Deadline::expired_any) {
            return false;
        }
        refresh_handle_from_current(ctx, handle);
        true
    }
}
pub(super) unsafe fn reset_mass_storage_interface_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    interface: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        select_existing_runtime_for_recovery(ctx, handle, budget)
            && reset_first_mass_storage_interface(ctx, interface, budget)
    }
}
pub(super) unsafe fn reset_first_mass_storage_interface(
    ctx: Context,
    interface: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        submit_control_no_data_within(
            ctx,
            0x21,
            MSC_REQ_BULK_ONLY_RESET,
            0,
            u16::from(interface),
            budget,
        )
    }
}
pub(super) unsafe fn clear_endpoint_halt_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        select_existing_runtime_for_recovery(ctx, handle, budget)
            && clear_first_endpoint_halt(ctx, address, budget)
    }
}
pub(super) unsafe fn clear_first_endpoint_halt(
    ctx: Context,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        submit_control_no_data_within(
            ctx,
            0x02,
            USB_REQ_CLEAR_FEATURE,
            USB_FEATURE_ENDPOINT_HALT,
            u16::from(address),
            budget,
        )
    }
}
pub(super) unsafe fn reconfigure_mass_storage_bulk_endpoints_for_handle_within(
    ctx: Context,
    handle: *mut DeviceHandle,
    input: u8,
    output: u8,
    budget: &Deadline,
) -> bool {
    unsafe {
        if !select_existing_runtime_for_recovery(ctx, handle, Some(budget))
            || !reconfigure_stopped_bulk_endpoints_within(ctx, input, output, budget)
        {
            return false;
        }
        refresh_handle_from_current(ctx, handle);
        true
    }
}
unsafe fn fail_bulk_reconfigure(ctx: Context, reason: &'static [u8]) -> bool {
    unsafe {
        (*current(ctx)).failures = (*current(ctx)).failures.wrapping_add(1);
        (*current(ctx)).reason = Text::literal(reason);
        (*current(ctx)).bulk_endpoints_configured = false;
        (*current(ctx)).bulk_endpoints_faulted = true;
        let token = diag::begin();
        diag::write(b"[XHCI] BOT bulk reconfigure failed slot=");
        diag::dec((*current(ctx)).addressed_slot_id);
        diag::write(b" in=");
        diag::dec((*current(ctx)).bulk_in_endpoint_id);
        diag::write(b" out=");
        diag::dec((*current(ctx)).bulk_out_endpoint_id);
        diag::write(b" cc=");
        diag::dec((*current(ctx)).last_completion_code);
        diag::end();
        diag::resolve(token);
        false
    }
}
unsafe fn reconfigure_stopped_bulk_endpoints_within(
    ctx: Context,
    input: u8,
    output: u8,
    budget: &Deadline,
) -> bool {
    unsafe {
        (*current(ctx)).last_completion_code = 0xff;
        let i = endpoint_dci(ctx, input);
        let o = endpoint_dci(ctx, output);
        if (*current(ctx)).addressed_slot_id == 0
            || (*state(ctx)).first_input_virt == 0
            || (*state(ctx)).first_device_virt == 0
            || i == 0
            || o == 0
            || i >= 32
            || o >= 32
            || input & 0x80 == 0
            || output & 0x80 != 0
            || input != (*current(ctx)).bulk_in_endpoint_address
            || output != (*current(ctx)).bulk_out_endpoint_address
        {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk identity invalid");
        }
        if budget.expired_any() {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk reconfigure budget expired");
        }
        let Some(ins) = endpoint_state(ctx, i) else {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk IN state missing");
        };
        let Some(outs) = endpoint_state(ctx, o) else {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk OUT state missing");
        };
        if !matches!(ins, EndpointState::Stopped) || !matches!(outs, EndpointState::Stopped) {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk endpoint not stopped");
        }
        let Some(ideq) = endpoint_ring_dequeue(ctx, input) else {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk IN dequeue missing");
        };
        let Some(odeq) = endpoint_ring_dequeue(ctx, output) else {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk OUT dequeue missing");
        };
        let input_context = (*state(ctx)).first_input_virt as *mut u32;
        ptr::write_bytes(input_context, 0, INPUT_CONTEXT_BYTES / 4);
        let words = usize::from((*current(ctx)).context_size) / 4;
        input_context.write((1 << i) | (1 << o));
        input_context.add(1).write(1 | (1 << i) | (1 << o));
        dma_fence();
        copy_output_slot_context_to_input(ctx, input_context.add(words));
        let ic = endpoint_context(ctx, input_context, words, i);
        let oc = endpoint_context(ctx, input_context, words, o);
        copy_output_endpoint_context_to_input(ctx, ic, i);
        copy_output_endpoint_context_to_input(ctx, oc, o);
        ic.add(2).write(ideq as u32);
        ic.add(3).write((ideq >> 32) as u32);
        oc.add(2).write(odeq as u32);
        oc.add(3).write((odeq >> 32) as u32);
        (*current(ctx)).bulk_endpoints_configured = false;
        (*current(ctx)).bulk_endpoints_faulted = true;
        let command = trb_type(TRB_TYPE_CONFIGURE_ENDPOINT)
            | (u32::from((*current(ctx)).addressed_slot_id) << 24);
        let Some(event) = submit_command_within(
            ctx,
            (*current(ctx)).input_context_phys,
            0,
            command,
            Some(budget),
        ) else {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk reconfigure timeout");
        };
        (*current(ctx)).last_completion_code = event.code;
        (*current(ctx)).last_slot_id = event.slot_id;
        if event.code != COMPLETION_SUCCESS || event.slot_id != (*current(ctx)).addressed_slot_id {
            return fail_bulk_reconfigure(ctx, b"xHCI BOT bulk reconfigure rejected");
        }
        (*current(ctx)).bulk_endpoints_configured = true;
        (*current(ctx)).bulk_endpoints_faulted = false;
        true
    }
}
unsafe fn copy_output_slot_context_to_input(ctx: Context, dest: *mut u32) {
    unsafe {
        let source = (*state(ctx)).first_device_virt;
        dest.write(volatile_read32(source) & !(1 << 24));
        dest.add(1).write(volatile_read32(source + 4));
        dest.add(2)
            .write(volatile_read32(source + 8) & !(0xf << 18));
        dest.add(3).write(0);
    }
}
unsafe fn copy_output_endpoint_context_to_input(ctx: Context, dest: *mut u32, dci: u8) {
    unsafe {
        let source = (*state(ctx)).first_device_virt
            + u64::from((*current(ctx)).context_size) * u64::from(dci);
        dest.write(volatile_read32(source) & 0xffff_ff00);
        dest.add(1).write(volatile_read32(source + 4) & !0x41);
        dest.add(4).write(volatile_read32(source + 16));
    }
}
pub(super) unsafe fn reset_endpoint_state_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        select_existing_runtime_for_recovery(ctx, handle, budget)
            && reset_first_endpoint_state(ctx, address, budget)
    }
}
pub(super) unsafe fn reset_first_endpoint_state(
    ctx: Context,
    address: u8,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        if (*current(ctx)).addressed_slot_id == 0 {
            return false;
        }
        let dci = endpoint_dci(ctx, address);
        if dci == 0 || dci >= 32 {
            return false;
        }
        let class = fault_class(ctx, address);
        let Some(dequeue) = endpoint_ring_dequeue(ctx, address) else {
            mark_dci_recovery_failed(ctx, dci, class);
            return false;
        };
        recover_endpoint_dci(ctx, dci, dequeue, class, budget)
    }
}
pub(super) unsafe fn recover_control_endpoint(ctx: Context, budget: Option<&Deadline>) -> bool {
    unsafe {
        if (*current(ctx)).addressed_slot_id == 0 {
            return false;
        }
        let Some(dequeue) = control_ring_dequeue(ctx) else {
            mark_dci_recovery_failed(ctx, 1, EndpointFaultClass::Control);
            return false;
        };
        recover_endpoint_dci(ctx, 1, dequeue, EndpointFaultClass::Control, budget)
    }
}
unsafe fn recover_endpoint_dci(
    ctx: Context,
    dci: u8,
    dequeue: u64,
    class: EndpointFaultClass,
    budget: Option<&Deadline>,
) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) || dci == 0 || dci >= 32 {
            return false;
        }
        let fields = (u32::from(dci) << 16) | (u32::from((*current(ctx)).addressed_slot_id) << 24);
        for _ in 0..ENDPOINT_RECOVERY_COMMAND_ATTEMPTS {
            if budget.is_some_and(Deadline::expired_any) {
                break;
            }
            let Some(s) = endpoint_state(ctx, dci) else {
                break;
            };
            let action = endpoint_recovery::prepare_action(s);
            let kind = match action {
                PrepareAction::Stop => TRB_TYPE_STOP_ENDPOINT,
                PrepareAction::Reset => TRB_TYPE_RESET_ENDPOINT,
                PrepareAction::SetDequeue => TRB_TYPE_SET_TR_DEQUEUE_POINTER,
                PrepareAction::Reconfigure => break,
            };
            let parameter = if matches!(action, PrepareAction::SetDequeue) {
                dequeue
            } else {
                0
            };
            let Some(code) =
                endpoint_recovery_command_within(ctx, trb_type(kind) | fields, parameter, budget)
            else {
                break;
            };
            if endpoint_recovery::completion_needs_state_refresh(code) {
                continue;
            }
            if code != COMPLETION_SUCCESS {
                break;
            }
            if matches!(action, PrepareAction::SetDequeue) {
                drain_unresolved_transfer_dci(ctx, dci);
                set_fault(ctx, class, false);
                return true;
            }
        }
        mark_dci_recovery_failed(ctx, dci, class);
        false
    }
}
pub(super) unsafe fn set_hid_boot_protocol_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    interface: u8,
) -> bool {
    unsafe { select_device_handle(ctx, handle) && set_first_hid_boot_protocol(ctx, interface) }
}
pub(super) unsafe fn set_first_hid_boot_protocol(ctx: Context, interface: u8) -> bool {
    unsafe {
        (*current(ctx)).hid_set_protocol_attempted = true;
        (*current(ctx)).hid_set_protocol_ok =
            submit_control_no_data(ctx, 0x21, HID_REQ_SET_PROTOCOL, 0, u16::from(interface));
        (*current(ctx)).hid_set_protocol_ok
    }
}
pub(super) unsafe fn set_hid_idle_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    interface: u8,
) -> bool {
    unsafe { select_device_handle(ctx, handle) && set_first_hid_idle(ctx, interface) }
}
pub(super) unsafe fn set_first_hid_idle(ctx: Context, interface: u8) -> bool {
    unsafe {
        (*current(ctx)).hid_set_idle_attempted = true;
        (*current(ctx)).hid_set_idle_ok =
            submit_control_no_data(ctx, 0x21, HID_REQ_SET_IDLE, 0, u16::from(interface));
        (*current(ctx)).hid_set_idle_ok
    }
}
pub(super) unsafe fn get_hid_report_descriptor_for_handle(
    ctx: Context,
    handle: *mut DeviceHandle,
    interface: u8,
    expected: u16,
    out: &mut [u8],
) -> Option<usize> {
    unsafe {
        if !select_device_handle(ctx, handle)
            || !selected_allows_mutation(ctx)
            || expected == 0
            || out.is_empty()
            || (*state(ctx)).first_descriptor_virt == 0
        {
            return None;
        }
        let len = usize::from(expected).min(DESCRIPTOR_BYTES).min(out.len()) as u16;
        let desc = (*state(ctx)).first_descriptor_virt as *mut u8;
        ptr::write_bytes(desc, 0, usize::from(len));
        if !submit_control_in_typed(
            ctx,
            0x81,
            USB_REQ_GET_DESCRIPTOR,
            u16::from(USB_DESC_REPORT) << 8,
            u16::from(interface),
            len,
            (*current(ctx)).descriptor_phys,
        ) {
            return None;
        }
        let actual = (u32::from(len) - (*current(ctx)).last_control_residue) as usize;
        ptr::copy_nonoverlapping(desc, out.as_mut_ptr(), actual);
        refresh_handle_from_current(ctx, handle);
        Some(actual)
    }
}
