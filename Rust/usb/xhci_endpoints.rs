//! Original endpoint context construction and configure ownership. Unknown
//! command completion faults the exact endpoint; Add-Configure is not retried.
use super::xhci_state::Context;
use super::{
    endpoint_context::{self, BulkFields},
    endpoint_recovery::{self, EndpointState},
    xhci_constants::*,
    xhci_handles::*,
    xhci_hardware::{dma_fence, trb_type, volatile_read32},
    xhci_owner::selected_allows_mutation,
    xhci_state::{current, state},
    xhci_types::{EndpointHandle, Trb},
};
use crate::sched::sync_types::Text;
use core::ptr;
pub(super) fn endpoint_dci(_ctx: Context, address: u8) -> u8 {
    let number = address & 15;
    if number == 0 {
        0
    } else {
        number * 2 + u8::from(address & 0x80 != 0)
    }
}
fn floor_log2(_ctx: Context, mut value: u16) -> u8 {
    let mut result = 0;
    while value > 1 {
        value >>= 1;
        result += 1;
    }
    result
}
pub(super) fn encode_interrupt_interval(ctx: Context, speed: u8, descriptor: u8) -> u8 {
    let raw = if descriptor == 0 { 1 } else { descriptor };
    match speed {
        1 | 2 => floor_log2(ctx, u16::from(raw) * 8).clamp(3, 10),
        _ => (raw - 1).clamp(0, 15),
    }
}
pub(super) unsafe fn endpoint_context(
    _ctx: Context,
    input: *mut u32,
    words: usize,
    dci: u8,
) -> *mut u32 {
    unsafe { input.add(words * (usize::from(dci) + 1)) }
}
pub(super) unsafe fn setup_bulk_ep_context(
    ctx: Context,
    input: *mut u32,
    words: usize,
    dci: u8,
    input_direction: bool,
    dequeue: u64,
    packet_burst: (u16, u8),
) -> BulkFields {
    let (packet, burst) = packet_burst;
    unsafe {
        let ep = endpoint_context(ctx, input, words, dci);
        let fields = endpoint_context::bulk_fields(
            (*current(ctx)).addressed_speed,
            input_direction,
            packet,
            burst,
        );
        ep.write(fields.dword0);
        ep.add(1).write(fields.dword1);
        ep.add(2).write(dequeue as u32);
        ep.add(3).write((dequeue >> 32) as u32);
        ep.add(4).write(fields.dword4);
        fields
    }
}
pub(super) unsafe fn configure_first_interrupt_in_endpoint(
    ctx: Context,
    address: u8,
    packet: u16,
    interval: u8,
) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        let c = current(ctx);
        let s = state(ctx);
        if !(*c).get_config_ok
            || (*c).addressed_slot_id == 0
            || (*s).first_input_virt == 0
            || (*c).interrupt_endpoint_faulted
            || address & 0x80 == 0
        {
            return false;
        }
        let number = address & 15;
        if number == 0 {
            return false;
        }
        let dci = number * 2 + 1;
        if dci >= 32 {
            return false;
        }
        if !super::xhci_runtime::ensure_interrupt_storage(ctx) {
            (*c).failures += 1;
            (*c).reason = Text::literal(
                b"xHCI interrupt DMA allocation unavailable; retained owner unchanged",
            );
            return false;
        }
        if let Some(owner) = super::xhci::current_interrupt_owner_match(ctx) {
            (*s).deferred_events.purge(owner);
        }
        super::xhci::release_interrupt_transfer(ctx, true);
        (*c).interrupt_pending = false;
        (*c).interrupt_pending_trb_phys = 0;
        (*c).interrupt_pending_streak = 0;
        let input = ptr::with_exposed_provenance_mut::<u32>((*s).first_input_virt as usize);
        ptr::write_bytes(input, 0, INPUT_CONTEXT_BYTES / 4);
        let words = usize::from((*c).context_size) / 4;
        input.add(1).write((1u32 << dci) | 1);
        let slot = input.add(words);
        slot.write((u32::from((*c).addressed_speed) << 20) | (u32::from(dci) << 27));
        slot.add(1).write(u32::from((*c).addressed_port) << 16);
        let ep = endpoint_context(ctx, input, words, dci);
        let packet = if packet == 0 { 8 } else { packet };
        let interval_context = encode_interrupt_interval(ctx, (*c).addressed_speed, interval);
        (*c).interrupt_endpoint_id = dci;
        (*c).interrupt_endpoint_address = address;
        (*c).interrupt_endpoint_max_packet = packet;
        (*c).interrupt_endpoint_interval_raw = interval;
        (*c).interrupt_endpoint_interval_context = interval_context;
        ep.write(u32::from(interval_context) << 16);
        ep.add(1)
            .write((3 << 1) | (7 << 3) | (u32::from(packet) << 16));
        let dequeue = ((*c).interrupt_ring_phys
            + u64::from((*c).interrupt_enqueue) * core::mem::size_of::<Trb>() as u64)
            | u64::from((*c).interrupt_cycle != 0);
        ep.add(2).write(dequeue as u32);
        ep.add(3).write((dequeue >> 32) as u32);
        ep.add(4)
            .write(u32::from(packet) | (u32::from(packet) << 16));
        let control =
            trb_type(TRB_TYPE_CONFIGURE_ENDPOINT) | (u32::from((*c).addressed_slot_id) << 24);
        let Some(completion) =
            super::xhci::submit_command(ctx, (*c).input_context_phys, 0, control)
        else {
            (*c).interrupt_endpoint_faulted = true;
            return false;
        };
        (*c).last_completion_code = completion.code;
        (*c).last_slot_id = completion.slot_id;
        (*c).interrupt_endpoint_configured = completion.code == COMPLETION_SUCCESS;
        if (*c).interrupt_endpoint_configured {
            (*c).interrupt_endpoint_faulted = false;
            return true;
        }
        (*c).interrupt_endpoint_faulted = true;
        (*c).failures += 1;
        false
    }
}
pub(super) unsafe fn configure_interrupt_in_endpoint_handle(
    ctx: Context,
    handle: *mut EndpointHandle,
) -> bool {
    unsafe {
        if (*handle).kind != 1 || !select_device_handle(ctx, ptr::addr_of_mut!((*handle).device)) {
            return false;
        }
        if !configure_first_interrupt_in_endpoint(
            ctx,
            (*handle).address,
            (*handle).max_packet,
            (*handle).interval,
        ) {
            return false;
        }
        refresh_handle_from_current(ctx, ptr::addr_of_mut!((*handle).device));
        (*handle).endpoint_id = (*current(ctx)).interrupt_endpoint_id;
        true
    }
}
pub(super) unsafe fn configure_first_bulk_endpoints(
    ctx: Context,
    in_address: u8,
    in_packet: u16,
    in_burst: u8,
    out_address: u8,
    out_packet: u16,
    out_burst: u8,
) -> bool {
    unsafe {
        if !selected_allows_mutation(ctx) {
            return false;
        }
        let c = current(ctx);
        let s = state(ctx);
        if !(*c).get_config_ok
            || (*c).addressed_slot_id == 0
            || (*s).first_input_virt == 0
            || (*c).bulk_endpoints_faulted
            || in_address & 0x80 == 0
            || out_address & 0x80 != 0
        {
            return false;
        }
        let in_dci = endpoint_dci(ctx, in_address);
        let out_dci = endpoint_dci(ctx, out_address);
        if in_dci == 0 || out_dci == 0 || in_dci >= 32 || out_dci >= 32 {
            return false;
        }
        if !super::xhci_runtime::ensure_bulk_storage(ctx) {
            (*c).failures += 1;
            (*c).reason =
                Text::literal(b"xHCI bulk DMA allocation unavailable; retained owner unchanged");
            return false;
        }
        let max_dci = in_dci.max(out_dci);
        let input = ptr::with_exposed_provenance_mut::<u32>((*s).first_input_virt as usize);
        ptr::write_bytes(input, 0, INPUT_CONTEXT_BYTES / 4);
        let words = usize::from((*c).context_size) / 4;
        input.add(1).write((1u32 << in_dci) | (1u32 << out_dci) | 1);
        let slot = input.add(words);
        slot.write((u32::from((*c).addressed_speed) << 20) | (u32::from(max_dci) << 27));
        slot.add(1).write(u32::from((*c).addressed_port) << 16);
        let input_dequeue = ((*c).bulk_in_ring_phys
            + u64::from((*c).bulk_in_enqueue) * core::mem::size_of::<Trb>() as u64)
            | u64::from((*c).bulk_in_cycle != 0);
        let output_dequeue = ((*c).bulk_out_ring_phys
            + u64::from((*c).bulk_out_enqueue) * core::mem::size_of::<Trb>() as u64)
            | u64::from((*c).bulk_out_cycle != 0);
        let in_fields = setup_bulk_ep_context(
            ctx,
            input,
            words,
            in_dci,
            true,
            input_dequeue,
            (in_packet, in_burst),
        );
        let out_fields = setup_bulk_ep_context(
            ctx,
            input,
            words,
            out_dci,
            false,
            output_dequeue,
            (out_packet, out_burst),
        );
        let control =
            trb_type(TRB_TYPE_CONFIGURE_ENDPOINT) | (u32::from((*c).addressed_slot_id) << 24);
        let Some(completion) =
            super::xhci::submit_command(ctx, (*c).input_context_phys, 0, control)
        else {
            (*c).bulk_endpoints_faulted = true;
            return false;
        };
        (*c).last_completion_code = completion.code;
        (*c).last_slot_id = completion.slot_id;
        (*c).bulk_endpoints_configured = completion.code == COMPLETION_SUCCESS;
        if (*c).bulk_endpoints_configured {
            (*c).bulk_endpoints_faulted = false;
            (*c).bulk_in_endpoint_id = in_dci;
            (*c).bulk_out_endpoint_id = out_dci;
            (*c).bulk_in_endpoint_address = in_address;
            (*c).bulk_in_endpoint_max_packet = in_fields.max_packet;
            (*c).bulk_in_endpoint_max_burst = in_fields.max_burst;
            (*c).bulk_out_endpoint_address = out_address;
            (*c).bulk_out_endpoint_max_packet = out_fields.max_packet;
            (*c).bulk_out_endpoint_max_burst = out_fields.max_burst;
            return true;
        }
        (*c).bulk_endpoints_faulted = true;
        (*c).failures += 1;
        false
    }
}
pub(super) unsafe fn configure_bulk_endpoint_handles(
    ctx: Context,
    input: *mut EndpointHandle,
    output: *mut EndpointHandle,
) -> bool {
    unsafe {
        if (*input).kind != 2
            || (*output).kind != 3
            || !same_device_target(ctx, (*input).device, (*output).device)
            || !select_device_handle(ctx, ptr::addr_of_mut!((*input).device))
        {
            return false;
        }
        (*output).device = (*input).device;
        if !configure_first_bulk_endpoints(
            ctx,
            (*input).address,
            (*input).max_packet,
            (*input).max_burst,
            (*output).address,
            (*output).max_packet,
            (*output).max_burst,
        ) {
            return false;
        }
        refresh_handle_from_current(ctx, ptr::addr_of_mut!((*input).device));
        (*output).device = (*input).device;
        (*input).endpoint_id = (*current(ctx)).bulk_in_endpoint_id;
        (*input).max_packet = (*current(ctx)).bulk_in_endpoint_max_packet;
        (*input).max_burst = (*current(ctx)).bulk_in_endpoint_max_burst;
        (*output).endpoint_id = (*current(ctx)).bulk_out_endpoint_id;
        (*output).max_packet = (*current(ctx)).bulk_out_endpoint_max_packet;
        (*output).max_burst = (*current(ctx)).bulk_out_endpoint_max_burst;
        true
    }
}
pub(super) unsafe fn endpoint_ring_base(ctx: Context, address: u8) -> Option<u64> {
    unsafe {
        let c = current(ctx);
        if address == (*c).bulk_in_endpoint_address {
            Some((*c).bulk_in_ring_phys)
        } else if address == (*c).bulk_out_endpoint_address {
            Some((*c).bulk_out_ring_phys)
        } else if address == (*c).interrupt_endpoint_address {
            Some((*c).interrupt_ring_phys)
        } else {
            None
        }
    }
}
pub(super) unsafe fn endpoint_ring_dequeue(ctx: Context, address: u8) -> Option<u64> {
    unsafe {
        let base = endpoint_ring_base(ctx, address)?;
        let c = current(ctx);
        let (index, cycle) = if address == (*c).bulk_in_endpoint_address {
            ((*c).bulk_in_enqueue, (*c).bulk_in_cycle)
        } else if address == (*c).bulk_out_endpoint_address {
            ((*c).bulk_out_enqueue, (*c).bulk_out_cycle)
        } else if address == (*c).interrupt_endpoint_address {
            ((*c).interrupt_enqueue, (*c).interrupt_cycle)
        } else {
            return None;
        };
        Some((base + u64::from(index) * core::mem::size_of::<Trb>() as u64) | u64::from(cycle != 0))
    }
}
pub(super) unsafe fn control_ring_dequeue(ctx: Context) -> Option<u64> {
    unsafe {
        if (*current(ctx)).ep0_ring_phys == 0 || (*state(ctx)).first_ep0_ring_virt == 0 {
            return None;
        }
        Some(
            ((*current(ctx)).ep0_ring_phys
                + u64::from((*current(ctx)).ep0_enqueue) * core::mem::size_of::<Trb>() as u64)
                | u64::from((*current(ctx)).ep0_cycle != 0),
        )
    }
}
pub(super) unsafe fn endpoint_state(ctx: Context, dci: u8) -> Option<EndpointState> {
    unsafe {
        if (*state(ctx)).first_device_virt == 0 || dci == 0 {
            return None;
        }
        dma_fence();
        let address = (*state(ctx)).first_device_virt
            + u64::from((*current(ctx)).context_size) * u64::from(dci);
        endpoint_recovery::state_from_raw((volatile_read32(address) & 7) as u8)
    }
}
