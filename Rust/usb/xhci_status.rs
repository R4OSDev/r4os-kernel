//! Original copied controller diagnostic view. The caller MUST own controller
//! admission; IRQ/worker counters will be sampled from their atomic producers
//! when the controller entrypoints are switched. This is never an unlocked
//! read of the mutable controller catalogue.
use super::{
    xhci_constants::*,
    xhci_hardware::{read_op32, read_rt32, read_rt64, volatile_read32},
    xhci_state::state,
    xhci_types::{Status, Trb},
};
use core::ptr;
pub(super) unsafe fn status_locked() -> Status {
    unsafe {
        let s = state();
        let mut result = ptr::addr_of!((*s).current).read();
        let waits = super::wait::metrics();
        result.wait_calls = waits.calls;
        result.wait_successes = waits.successes;
        result.wait_timeouts = waits.timeouts;
        result.wait_early_calls = waits.early_calls;
        result.wait_runtime_calls = waits.runtime_calls;
        result.wait_requested_ms = waits.requested_ms;
        result.wait_elapsed_ns = waits.elapsed_ns;
        result.wait_max_elapsed_ns = waits.max_elapsed_ns;
        result.wait_iterations = waits.iterations;
        result.wait_blocked_ticks = waits.blocked_ticks;
        result.last_wait_reason = waits.last_reason;
        result.last_wait_phase = waits.last_phase;
        result.last_wait_requested_ms = waits.last_requested_ms;
        result.last_wait_elapsed_ns = waits.last_elapsed_ns;
        result.last_wait_iterations = waits.last_iterations;
        result.last_wait_blocked_ticks = waits.last_blocked_ticks;
        result.last_wait_retry = waits.last_retry;
        result.last_wait_success = waits.last_success;
        if (*s).first_interrupt_ring_virt != 0 {
            let ring = ptr::with_exposed_provenance::<Trb>((*s).first_interrupt_ring_virt as usize);
            result.interrupt_link_cycle = u8::from(
                ptr::addr_of!((*ring.add(TRANSFER_TRB_COUNT - 1)).control).read() & TRB_CYCLE != 0,
            );
        }
        if result.interrupt_pending
            && result.interrupt_pending_trb_phys >= result.interrupt_ring_phys
        {
            let offset = result.interrupt_pending_trb_phys - result.interrupt_ring_phys;
            if offset.is_multiple_of(core::mem::size_of::<Trb>() as u64) {
                let index = offset / core::mem::size_of::<Trb>() as u64;
                if index < (TRANSFER_TRB_COUNT - 1) as u64 {
                    result.interrupt_pending_index = index as u16;
                }
            }
        }
        if (*s).event_ring_virt != 0 && usize::from(result.event_dequeue) < EVENT_TRB_COUNT {
            let address = (*s).event_ring_virt
                + u64::from(result.event_dequeue) * core::mem::size_of::<Trb>() as u64;
            let control = volatile_read32(address + 12);
            result.event_raw_cycle = u8::from(control & TRB_CYCLE != 0);
            result.event_raw_type = ((control >> TRB_TYPE_SHIFT) & 0x3f) as u8;
            result.event_raw_code = (volatile_read32(address + 8) >> 24) as u8;
        }
        result.interrupt_hw_dequeue = super::xhci::read_interrupt_hardware_dequeue();
        if result.mapped && result.mmio_virt != 0 {
            result.usbsts = read_op32(ptr::addr_of!((*s).current), OP_USBSTS);
            result.iman0 = read_rt32(ptr::addr_of!((*s).current), RT_IR0_IMAN);
            result.erdp0 = read_rt64(ptr::addr_of!((*s).current), RT_IR0_ERDP);
        }
        let deferred = (*s).deferred_events.snapshot();
        result.deferred_events_pending = deferred.pending;
        result.deferred_events_queued = deferred.queued;
        result.deferred_events_delivered = deferred.delivered;
        result.deferred_events_overflows = deferred.overflows;
        result.deferred_events_purged = deferred.purged;
        result.deferred_events_high_water = deferred.high_water;
        let ports = (*s).pending_port_changes.snapshot();
        result.port_change_events = ports.events;
        result.port_change_queued = ports.queued;
        result.port_change_coalesced = ports.coalesced;
        result.port_change_invalid = ports.invalid;
        result.port_change_taken = ports.taken;
        result.port_change_retries = ports.retries;
        result.port_change_pending_mask = ports.pending_mask;
        result.port_change_pending = ports.pending;
        result.port_change_high_water = ports.high_water;
        let transfers = (*s).transfer_objects.snapshot();
        result.transfer_objects_active = transfers.active;
        result.transfer_objects_high_water = transfers.high_water;
        result.transfer_objects_submitted = transfers.submitted;
        result.transfer_objects_completed = transfers.completed;
        result.transfer_objects_timed_out = transfers.timed_out;
        result.transfer_objects_cancelled = transfers.cancelled;
        result.transfer_objects_failed = transfers.failed;
        super::xhci_async::project_counters(&mut result);
        result
    }
}
