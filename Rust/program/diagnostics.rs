//! Passive program summaries and original explicit one-shot diagnostic controls.
use super::{
    console, console_input, entry, failure, gui_events, launch, payload, payload_types as t,
    registry, registry_types::ProgramRegistryStats,
};
use crate::{contract as c, memory::heap};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
unsafe fn storage() -> t::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        payload::r4os_rust_program_payload_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn stats() -> ProgramRegistryStats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        registry::r4os_rust_program_registry_snapshot(out.as_mut_ptr());
        out.assume_init()
    }
}
/// # Safety
/// Original resident canonical passive output; snapshots retain their owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_diagnostic_storage(
    out: *mut c::ProgramInstanceStorageSummary,
) {
    unsafe {
        let storage = storage();
        out.write(c::ProgramInstanceStorageSummary {
            core_bytes_per_instance: storage.core_bytes_per_instance,
            registry_reserved_core_bytes: storage.registry_reserved_core_bytes,
            live_core_bytes: storage.live_core_bytes,
            active_instance_bytes: storage.active_instance_bytes,
            peak_active_instance_bytes: storage.peak_active_instance_bytes,
            reserved_instance_bytes: storage.reserved_instance_bytes,
            peak_reserved_instance_bytes: storage.peak_reserved_instance_bytes,
            current_payload_bytes: storage.current_payload_bytes,
            peak_payload_bytes: storage.peak_payload_bytes,
            current_runtime_bytes: storage.current_runtime_bytes,
            peak_runtime_bytes: storage.peak_runtime_bytes,
            current_console_bytes: storage.current_console_bytes,
            peak_console_bytes: storage.peak_console_bytes,
            current_gui_bytes: storage.current_gui_bytes,
            peak_gui_bytes: storage.peak_gui_bytes,
            active_instances: storage.active_instances,
            active_service_instances: storage.active_service_instances,
            active_console_instances: storage.active_console_instances,
            active_gui_instances: storage.active_gui_instances,
            runtime_payloads: storage.active_runtime_payloads,
            process_payloads: storage.active_process_payloads,
            environment_payloads: storage.active_environment_payloads,
            console_payloads: storage.active_console_payloads,
            console_output_payloads: storage.active_console_output_payloads,
            gui_payloads: storage.active_gui_payloads,
            gui_command_payloads: storage.active_gui_command_payloads,
            gui_raster_payloads: storage.active_gui_raster_payloads,
            allocation_attempts: storage.allocation_attempts,
            payload_allocations: storage.payload_allocations,
            payload_releases: storage.payload_releases,
            allocation_failures: storage.allocation_failures,
            transaction_rollbacks: storage.transaction_rollbacks,
            owner_mismatches: storage.owner_mismatches,
            header_errors: storage.header_errors,
            free_failures: storage.free_failures,
            quarantined_payloads: storage.quarantined_payloads,
            quarantined_bytes: storage.quarantined_bytes,
            current_gui_frame_bytes: storage.current_gui_frame_bytes,
            peak_gui_frame_bytes: storage.peak_gui_frame_bytes,
            current_gui_frame_commands: storage.current_gui_frame_commands,
            peak_gui_frame_commands: storage.peak_gui_frame_commands,
            current_gui_frame_nodes: storage.current_gui_frame_nodes,
            peak_gui_frame_nodes: storage.peak_gui_frame_nodes,
            gui_frame_commits: storage.gui_frame_commits,
            gui_frame_cancels: storage.gui_frame_cancels,
            gui_frame_oom_failures: storage.gui_frame_oom_failures,
            gui_frame_snapshot_reads: storage.gui_frame_snapshot_reads,
            ..Default::default()
        });
    }
}
/// # Safety
/// Original resident canonical passive output; snapshots retain their owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_diagnostic_registry(
    out: *mut c::ProgramRegistrySummaryV2,
) {
    unsafe {
        let registry = stats();
        out.write(c::ProgramRegistrySummaryV2 {
            chunk_slots: registry.chunk_slots,
            chunk_count: registry.chunk_count,
            slot_capacity: registry.slot_capacity,
            free_slots: registry.free_slots,
            reserved_slots: registry.reserved_slots,
            live_slots: registry.live_slots,
            done_slots: registry.done_slots,
            retiring_slots: registry.retiring_slots,
            pinned_slots: registry.pinned_slots,
            warm_chunks: 2,
            last_admission_error: registry.last_admission_error,
            flags: if registry.failure_armed {
                c::program_registry_summary_flag_failure_armed
            } else {
                0
            },
            peak_chunks: registry.peak_chunks,
            peak_live: registry.peak_live,
            growth_attempts: registry.growth_attempts,
            growth_failures: registry.growth_failures,
            forced_failures: registry.forced_failures,
            publish_count: registry.publish_count,
            rollback_count: registry.rollback_count,
            shrink_count: registry.shrink_count,
            id_collisions: registry.id_collisions,
            id_wraps: registry.id_wraps,
            live_id_hash: registry.live_id_hash,
            live_address_hash: registry.live_address_hash,
            next_generation: registry.next_generation,
            completion_pending: registry.completion_pending.min(u32::MAX.into()) as u32,
            completion_ready: registry.completion_ready.min(u32::MAX.into()) as u32,
            completion_output_bytes: registry.completion_output_bytes,
            retire_queued: registry.retire_queued.min(u32::MAX.into()) as u32,
            retire_deferred: registry.retire_deferred.min(u32::MAX.into()) as u32,
            history_head: registry.history_head.min(u32::MAX.into()) as u32,
            history_count: registry.history_count.min(u32::MAX.into()) as u32,
            history_sequence: registry.history_sequence,
            completion_peak: registry.completion_peak,
            retire_retries: registry.retire_retries,
            ..Default::default()
        });
    }
}
/// # Safety
/// Caller owns the resident result; queue scan retains the original registry lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_diagnostic_input(
    out: *mut c::ProgramInputPerformanceInfo,
) {
    unsafe {
        let mut gui_pending = 0u64;
        let mut console_pending = 0u64;
        let mut gui_active = 0u32;
        let mut console_active = 0u32;
        let mut gui_high_water = 0;
        let mut console_high_water = 0;
        let mut attach_pending = 0u32;
        if registry::lock() {
            let mut iterator = MaybeUninit::uninit();
            registry::r4os_rust_program_registry_iterator_new(false, iterator.as_mut_ptr());
            let mut iterator = iterator.assume_init();
            loop {
                let instance = registry::r4os_rust_program_registry_iterator(&mut iterator);
                if instance.is_null() {
                    break;
                }
                let gui = (*instance).gui_payload.cast::<t::Gui>();
                if !gui.is_null() {
                    gui_active = gui_active.saturating_add(1);
                    gui_pending = gui_pending.saturating_add(gui_events::pending(gui).into());
                    gui_high_water = gui_high_water.max((*gui).event_high_water);
                    if (*gui).start_attach_pending {
                        attach_pending = attach_pending.saturating_add(1);
                    }
                }
                let console = (*instance).console_payload.cast::<t::Console>();
                if !console.is_null() {
                    console_active = console_active.saturating_add(1);
                    console_pending =
                        console_pending.saturating_add((*console).state.stdin_pending.into());
                    console_high_water = console_high_water.max((*console).input_high_water);
                }
            }
            registry::unlock();
        }
        (*out).gui_queue_capacity = 63;
        (*out).gui_queue_pending = gui_pending.min(u32::MAX.into()) as u32;
        (*out).gui_queue_high_water = gui_high_water;
        (*out).gui_queue_active = gui_active;
        (*out).console_queue_capacity = 4095;
        (*out).console_queue_pending = console_pending.min(u32::MAX.into()) as u32;
        (*out).console_queue_high_water = console_high_water;
        (*out).console_queue_active = console_active;
        (*out).program_start_attach_pending = attach_pending;
        (*out).gui_push_attempts =
            (*gui_events::r4os_rust_program_gui_events_state()).push_attempts;
        (*out).gui_accepted = (*gui_events::r4os_rust_program_gui_events_state()).accepted;
        (*out).gui_mouse_move_coalesced =
            (*gui_events::r4os_rust_program_gui_events_state()).mouse_move_coalesced;
        (*out).gui_mouse_move_evicted =
            (*gui_events::r4os_rust_program_gui_events_state()).mouse_move_evicted;
        (*out).gui_rejected = (*gui_events::r4os_rust_program_gui_events_state()).rejected;
        (*out).console_push_calls = (*console_input::r4os_rust_program_console_input_state())
            .push_calls
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_batch_calls = (*console_input::r4os_rust_program_console_input_state())
            .batch_calls
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_bytes_attempted = (*console_input::r4os_rust_program_console_input_state())
            .bytes_attempted
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_bytes_accepted = (*console_input::r4os_rust_program_console_input_state())
            .bytes_accepted
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_full_events = (*console_input::r4os_rust_program_console_input_state())
            .full_events
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_read_calls = (*console_input::r4os_rust_program_console_input_state())
            .read_calls
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_read_empty = (*console_input::r4os_rust_program_console_input_state())
            .read_empty
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_read_bytes = (*console_input::r4os_rust_program_console_input_state())
            .read_bytes
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_calls = (*console_input::r4os_rust_program_console_input_state())
            .wait_calls
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_blocks = (*console_input::r4os_rust_program_console_input_state())
            .wait_blocks
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_immediate = (*console_input::r4os_rust_program_console_input_state())
            .wait_immediate
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_wakes = (*console_input::r4os_rust_program_console_input_state())
            .wait_wakes
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_timeouts = (*console_input::r4os_rust_program_console_input_state())
            .wait_timeouts
            .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_wait_cancellations =
            (*console_input::r4os_rust_program_console_input_state())
                .wait_cancellations
                .load(core::sync::atomic::Ordering::Relaxed);
        (*out).console_output_write_calls =
            (*console::r4os_rust_program_console_state()).write_calls;
        (*out).console_output_source_bytes =
            (*console::r4os_rust_program_console_state()).source_bytes;
        (*out).console_output_visible_append_bytes =
            (*console::r4os_rust_program_console_state()).visible_append_bytes;
        (*out).console_output_capture_append_bytes =
            (*console::r4os_rust_program_console_state()).capture_append_bytes;
        (*out).console_output_shared_bytes =
            (*console::r4os_rust_program_console_state()).shared_bytes;
        (*out).console_output_revision_batches =
            (*console::r4os_rust_program_console_state()).revision_batches;
        (*out).console_output_desktop_signals =
            (*console::r4os_rust_program_console_state()).desktop_signals;
        (*out).console_output_compactions =
            (*console::r4os_rust_program_console_state()).compactions;
        (*out).console_output_compaction_bytes =
            (*console::r4os_rust_program_console_state()).compaction_bytes;
        (*out).console_output_segment_drops =
            (*console::r4os_rust_program_console_state()).segment_drops;
        (*out).console_output_segment_drop_bytes =
            (*console::r4os_rust_program_console_state()).segment_drop_bytes;
        (*out).program_launch_attempts = launch::r4os_rust_program_launch_attempts();
        (*out).program_entries_started = AtomicU64::from_ptr(ptr::addr_of_mut!(
            (*entry::r4os_rust_program_entry_state()).entries_started
        ))
        .load(Ordering::Relaxed);
        (*out).program_attach_wait_events = AtomicU64::from_ptr(ptr::addr_of_mut!(
            (*entry::r4os_rust_program_entry_state()).attach_wait_events
        ))
        .load(Ordering::Relaxed);
    }
}
/// # Safety
/// The original admitted diagnostic caller retains exclusive result storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_diagnostic_registry_test(
    out: *mut c::ProgramRegistrySelfTestResultV2,
) -> i32 {
    unsafe {
        let operation = (*out).operation;
        let phase = (*out).lifecycle_phase;
        let requested_id = (*out).requested_next_id;
        let before = stats();
        let mut flags = 0;
        let mut result = 1;
        let mut lifecycle_result = 0;
        let mut applied_id = 0;
        match operation {
            c::program_registry_self_test_operation_reset => {
                registry::r4os_rust_program_registry_control(11, 0);
                failure::r4os_rust_program_failure_control(0, 0);
                flags |= c::program_registry_self_test_flag_reset;
                if failure::allowed(1) || failure::allowed(2) {
                    flags |= c::program_registry_self_test_flag_allowed;
                }
            }
            c::program_registry_self_test_operation_arm_next_growth => {
                if !failure::allowed(1) {
                    flags |= c::program_registry_self_test_flag_denied;
                    result = -1;
                } else {
                    flags |= c::program_registry_self_test_flag_allowed
                        | c::program_registry_self_test_flag_one_shot;
                    if registry::r4os_rust_program_registry_control(10, 0) != 0 {
                        flags |= c::program_registry_self_test_flag_armed;
                    } else {
                        flags |= c::program_registry_self_test_flag_busy;
                        result = -2;
                    }
                }
            }
            c::program_registry_self_test_operation_arm_lifecycle_failure => {
                if !failure::allowed(2)
                    || phase == 0
                    || phase > c::program_registry_self_test_phase_slot_reclaim
                {
                    flags |= c::program_registry_self_test_flag_denied;
                    lifecycle_result = -1;
                    result = -1;
                } else {
                    flags |= c::program_registry_self_test_flag_allowed
                        | c::program_registry_self_test_flag_one_shot;
                    if failure::r4os_rust_program_failure_control(1, phase as u8) {
                        flags |= c::program_registry_self_test_flag_lifecycle_armed;
                    } else {
                        flags |= c::program_registry_self_test_flag_busy;
                        lifecycle_result = -5;
                        result = -2;
                    }
                }
            }
            c::program_registry_self_test_operation_signal_reaper => {
                if !failure::allowed(2) {
                    flags |= c::program_registry_self_test_flag_denied;
                    lifecycle_result = -1;
                    result = -1;
                } else {
                    flags |= c::program_registry_self_test_flag_allowed;
                    failure::r4os_rust_program_failure_control(2, 0);
                }
            }
            c::program_registry_self_test_operation_force_next_id => {
                if !failure::allowed(2) || requested_id == 0 {
                    flags |= c::program_registry_self_test_flag_denied;
                    lifecycle_result = -1;
                    result = -1;
                } else {
                    flags |= c::program_registry_self_test_flag_allowed;
                    registry::r4os_rust_program_registry_control(8, requested_id.into());
                    applied_id = requested_id;
                }
            }
            _ => result = -3,
        }
        let after = stats();
        let mut lifecycle = MaybeUninit::uninit();
        failure::r4os_rust_program_failure_snapshot(lifecycle.as_mut_ptr());
        let lifecycle = lifecycle.assume_init();
        if lifecycle.armed {
            flags |= c::program_registry_self_test_flag_lifecycle_armed;
        }
        if lifecycle.consumed {
            flags |= c::program_registry_self_test_flag_lifecycle_consumed;
        }
        if lifecycle.reaper_signalled {
            flags |= c::program_registry_self_test_flag_reaper_signalled;
        }
        if lifecycle.retried {
            flags |= c::program_registry_self_test_flag_lifecycle_retried;
        }
        if lifecycle.recovered {
            flags |= c::program_registry_self_test_flag_lifecycle_recovered;
        }
        out.write(c::ProgramRegistrySelfTestResultV2 {
            operation,
            flags,
            chunk_count_before: before.chunk_count,
            slot_capacity_before: before.slot_capacity,
            free_slots_before: before.free_slots,
            growth_failures_before: before.growth_failures,
            growth_failures_after: after.growth_failures,
            forced_failures_before: before.forced_failures,
            forced_failures_after: after.forced_failures,
            lifecycle_phase: if phase != 0 { phase } else { lifecycle.phase },
            lifecycle_result,
            retire_queued_before: before.retire_queued.min(u32::MAX.into()) as u32,
            retire_queued_after: after.retire_queued.min(u32::MAX.into()) as u32,
            completion_pending_before: before.completion_pending.min(u32::MAX.into()) as u32,
            completion_pending_after: after.completion_pending.min(u32::MAX.into()) as u32,
            completion_ready_before: before.completion_ready.min(u32::MAX.into()) as u32,
            completion_ready_after: after.completion_ready.min(u32::MAX.into()) as u32,
            retire_retries_before: before.retire_retries,
            retire_retries_after: after.retire_retries,
            history_sequence_before: before.history_sequence,
            history_sequence_after: after.history_sequence,
            requested_next_id: requested_id,
            applied_next_id: applied_id,
            ..Default::default()
        });
        result
    }
}
/// # Safety
/// Original single-test admission and resident output; no state borrow spans work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_diagnostic_storage_test(
    out: *mut c::ProgramInstanceStorageSelfTestResult,
) -> i32 {
    unsafe {
        let state = payload::r4os_rust_program_payload_state();
        if (*state).self_test_running {
            out.write(c::ProgramInstanceStorageSelfTestResult::default());
            return -2;
        }
        (*state).self_test_running = true;
        struct Running;
        impl Drop for Running {
            fn drop(&mut self) {
                unsafe {
                    (*payload::r4os_rust_program_payload_state()).self_test_running = false;
                }
            }
        }
        let _running = Running;
        let mut heap_before = MaybeUninit::uninit();
        heap::r4os_rust_heap_stats(heap_before.as_mut_ptr());
        let heap_before = heap_before.assume_init();
        let before = storage();
        let passed = super::self_test::run();
        let report = ptr::addr_of!((*state).report).read();
        let after = storage();
        let allocations = after
            .payload_allocations
            .saturating_sub(before.payload_allocations);
        let releases = after
            .payload_releases
            .saturating_sub(before.payload_releases);
        let mut flags = 0;
        if heap_before.capacity_bytes != 0 {
            flags |= c::program_instance_storage_self_test_flag_heap_ready;
        }
        if report.storage_baseline_ok {
            flags |= c::program_instance_storage_self_test_flag_storage_baseline_restored;
        }
        if allocations == releases {
            flags |= c::program_instance_storage_self_test_flag_payload_balance;
        }
        if after.transaction_rollbacks > before.transaction_rollbacks {
            flags |= c::program_instance_storage_self_test_flag_rollback_path;
        }
        out.write(c::ProgramInstanceStorageSelfTestResult {
            cases: report.cases,
            passed_cases: if passed {
                report.cases
            } else {
                report.failed_case.saturating_sub(1)
            },
            failed_case: report.failed_case,
            flags,
            baseline_payload_reserved_bytes: before.current_payload_bytes,
            final_payload_reserved_bytes: after.current_payload_bytes,
            peak_payload_reserved_bytes: report.peak_payload_bytes,
            allocation_failures_before: before.allocation_failures,
            allocation_failures_after: after.allocation_failures,
            ..Default::default()
        });
        if passed { 1 } else { -1 }
    }
}
