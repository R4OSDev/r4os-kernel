//! Original bounded passive service snapshots. No endpoint borrow crosses
//! registry release, lock acquisition or a foreign operation.
use super::{endpoint, registry as reg, registry_types::*};
use core::ptr;
fn add_timing(out: &mut PerformanceSummary, index: usize, t: LockTiming) {
    out.lock_acquisitions[index] = out.lock_acquisitions[index].wrapping_add(t.acquisitions);
    out.lock_contentions[index] = out.lock_contentions[index].wrapping_add(t.contentions);
    out.lock_wait_ns[index] = out.lock_wait_ns[index].wrapping_add(t.wait_ns);
    out.lock_wait_max_ns[index] = out.lock_wait_max_ns[index].max(t.wait_max_ns);
    out.lock_hold_ns[index] = out.lock_hold_ns[index].wrapping_add(t.hold_ns);
    out.lock_hold_max_ns[index] = out.lock_hold_max_ns[index].max(t.hold_max_ns);
    out.lock_timing_unavailable[index] =
        out.lock_timing_unavailable[index].wrapping_add(t.timing_unavailable);
    out.lock_timing_samples[index] = out.lock_timing_samples[index].wrapping_add(t.timing_samples);
}
pub(super) unsafe fn snapshot() -> PerformanceSummary {
    unsafe {
        let mut out = PerformanceSummary::EMPTY;
        let Some(mut guard) = reg::lock_registry(2) else {
            return out;
        };
        for i in 0..MAX_SERVICES {
            let e = reg::entry(i);
            if !(*e).used {
                continue;
            }
            out.used_services += 1;
            if (*e).state == STATE_RUNNING {
                out.running_services += 1;
            }
        }
        let r = reg::raw();
        let e = (*r).enumeration;
        out.registry_index_queries = e.index_queries;
        out.registry_refresh_requests = e.refresh_requests;
        out.registry_refresh_visits = e.refresh_visits;
        out.registry_instance_lookups = e.instance_lookups;
        out.registry_index_end_markers = e.index_end_markers;
        for i in 0..3 {
            add_timing(
                &mut out,
                i,
                reg::timing_snapshot(ptr::addr_of_mut!((*r).timing).cast::<LockTiming>().add(i)),
            );
        }
        guard.release();
        for i in 0..MAX_ENDPOINTS {
            let ep = reg::endpoint(i);
            let Some(_guard) = reg::endpoint_lock(ep, 6, false) else {
                continue;
            };
            let t = (*ep).lifetime;
            out.payload_copy_bytes = out.payload_copy_bytes.wrapping_add(t.payload_copy_bytes);
            out.payload_clear_bytes = out.payload_clear_bytes.wrapping_add(t.payload_clear_bytes);
            out.slot_metadata_resets = out
                .slot_metadata_resets
                .wrapping_add(t.slot_metadata_resets);
            out.endpoint_metadata_resets = out
                .endpoint_metadata_resets
                .wrapping_add(t.endpoint_metadata_resets);
            out.endpoint_payload_reset_bytes = out
                .endpoint_payload_reset_bytes
                .wrapping_add(t.endpoint_payload_reset_bytes);
            out.queue_scan_passes = out.queue_scan_passes.wrapping_add(t.queue_scan_passes);
            out.queue_scan_slots = out.queue_scan_slots.wrapping_add(t.queue_scan_slots);
            out.endpoint_revalidations = out.endpoint_revalidations.wrapping_add(t.revalidations);
            out.endpoint_stale_rejections = out
                .endpoint_stale_rejections
                .wrapping_add(t.stale_rejections);
            for family in 3..FAMILIES {
                add_timing(
                    &mut out,
                    family,
                    reg::timing_snapshot(
                        ptr::addr_of_mut!((*ep).lock_timing)
                            .cast::<LockTiming>()
                            .add(family),
                    ),
                );
            }
            if !(*ep).used {
                continue;
            }
            let counts = endpoint::counts(ep);
            out.endpoints_used += 1;
            out.request_pending = out
                .request_pending
                .wrapping_add(counts.queued + counts.delivered);
            out.response_pending = out.response_pending.wrapping_add(counts.responded);
            out.queue_depth_total = out.queue_depth_total.wrapping_add(DEPTH as u32);
            out.queue_used_total = out.queue_used_total.wrapping_add(counts.used);
            out.queue_high_water_total = out
                .queue_high_water_total
                .wrapping_add((*ep).queue_high_water);
            out.active_workers = out.active_workers.wrapping_add(counts.delivered);
            out.max_active_workers = out.max_active_workers.max((*ep).max_active_workers);
            out.open_handles = out.open_handles.wrapping_add((*ep).open_handles);
            out.requests = out.requests.wrapping_add((*ep).requests);
            out.responses = out.responses.wrapping_add((*ep).responses);
            out.drops = out.drops.wrapping_add((*ep).drops);
            out.busy_rejections = out.busy_rejections.wrapping_add((*ep).busy_rejections);
            out.timeouts = out.timeouts.wrapping_add((*ep).timeouts);
            out.cancellations = out.cancellations.wrapping_add((*ep).cancellations);
            out.completion_waits = out.completion_waits.wrapping_add((*ep).completion_waits);
            out.completion_timeouts = out
                .completion_timeouts
                .wrapping_add((*ep).completion_timeouts);
            out.completion_wait_rounds = out
                .completion_wait_rounds
                .wrapping_add((*ep).completion_wait_rounds);
            out.targeted_response_wakes = out
                .targeted_response_wakes
                .wrapping_add((*ep).targeted_response_wakes);
            out.targeted_response_wake_misses = out
                .targeted_response_wake_misses
                .wrapping_add((*ep).targeted_response_wake_misses);
            out.admission_waits = out.admission_waits.wrapping_add((*ep).admission_waits);
            out.admission_timeouts = out
                .admission_timeouts
                .wrapping_add((*ep).admission_timeouts);
        }
        out
    }
}
