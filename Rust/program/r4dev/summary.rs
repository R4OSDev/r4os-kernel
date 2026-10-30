//! Original version1-15 prefix negotiation && complete observational
//! field projection. Every subsystem keeps its native snapshot admission;
//! owned local copies retain no metadata borrow across another provider wait.
use super::snapshot;
use crate::contract::*;
use core::{mem, ptr};
fn clamp_usize(v: usize) -> u32 {
    v.min(u32::MAX as usize) as u32
}
fn clamp_u64(v: u64) -> u32 {
    v.min(u64::from(u32::MAX)) as u32
}
unsafe fn backing_snapshot<T>(fill: unsafe extern "C" fn(*mut T)) -> T {
    let _owner = crate::memory::owner::Guard::acquire(5);
    unsafe { snapshot(fill) }
}
/// # Safety
/// Caller owns the header && exactly its advertised writable prefix. No
/// complete-record reference is constructed over a smaller legacy allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_summary(
    out: *mut ProgramPerformanceSummary,
) -> i32 {
    unsafe {
        let caller_version = (*out).version;
        let caller_size = (*out).size as usize;
        const CAPS: [usize; 15] = [
            mem::offset_of!(ProgramPerformanceSummary, monotonic_clock_flags),
            mem::offset_of!(ProgramPerformanceSummary, service_completion_wait_rounds),
            mem::offset_of!(ProgramPerformanceSummary, service_payload_copy_bytes),
            mem::offset_of!(ProgramPerformanceSummary, service_queue_scan_passes),
            mem::offset_of!(ProgramPerformanceSummary, service_lock_timing_stride),
            mem::offset_of!(ProgramPerformanceSummary, service_registry_index_queries),
            mem::offset_of!(
                ProgramPerformanceSummary,
                hot_path_memory_block_physical_index_entries
            ),
            mem::offset_of!(ProgramPerformanceSummary, fs_drive_gate_count),
            mem::offset_of!(ProgramPerformanceSummary, fs_cache_bulk_write_requests),
            mem::offset_of!(ProgramPerformanceSummary, fs_cache_policy_version),
            mem::offset_of!(ProgramPerformanceSummary, ntfs_metadata_cache_version),
            mem::offset_of!(ProgramPerformanceSummary, fs_cache_capacity_min_pages),
            mem::offset_of!(
                ProgramPerformanceSummary,
                ntfs_metadata_payload_write_retentions
            ),
            mem::offset_of!(ProgramPerformanceSummary, loader_file_range_read_bytes),
            mem::size_of::<ProgramPerformanceSummary>(),
        ];
        if caller_version == 0 || caller_size < mem::offset_of!(ProgramPerformanceSummary, flags) {
            return -1;
        }
        let mut negotiated_version = 1u32;
        for version in (2..=performance_snapshot_version).rev() {
            if caller_version >= version && caller_size >= CAPS[(version - 1) as usize] {
                negotiated_version = version;
                break;
            }
        }
        let copy_size = caller_size.min(CAPS[(negotiated_version - 1) as usize]);
        let sched = snapshot(crate::sched::scheduler::r4os_rust_scheduler_stats);
        let tasks = crate::sched::task::task_snapshots::summary();
        let boot = crate::boot::performance::snapshot();
        let loader = crate::loader::performance::snapshot();
        let monotonic_clock = crate::platform::monotonic::snapshot();
        let display_stats = crate::display::manager::stats();
        let svc = snapshot(crate::service::ffi::r4os_rust_services_performance);
        let audio = crate::audio::core::performance_summary();
        let wait_summary = snapshot(crate::sched::sync_stats::r4os_rust_sync_wait_summary);
        let lock_summary = snapshot(crate::sched::sync_stats::r4os_rust_sync_lock_summary);
        let fs_summary = snapshot(crate::fs::request::r4os_rust_fs_request_summary);
        let fs_cache = snapshot(crate::fs::page_cache::r4os_rust_pagecache_summary);
        let fat32_summary = snapshot(crate::fs::fat::r4os_rust_fat_summary);
        let ntfs_metadata = crate::fs::ntfs_adapter::summary();
        let reclaim_summary = snapshot(crate::memory::reclaim::r4os_rust_reclaim_summary);
        let backing_summary =
            backing_snapshot(crate::memory::backing_store::r4os_rust_backing_summary);
        let backing_slot_summary =
            backing_snapshot(crate::memory::backing_store::r4os_rust_backing_slot_summary);
        let pager_gate_summary =
            backing_snapshot(crate::memory::backing_store::r4os_rust_backing_pager_gate_summary);
        let page_io_summary =
            backing_snapshot(crate::memory::backing_store::r4os_rust_backing_page_io_summary);
        let page_state_summary =
            snapshot(crate::memory::vm_pager_api::r4os_rust_vm_page_state_summary);
        let hot_path_summary = snapshot(crate::memory::vm_pager_api::r4os_rust_vm_hot_stats);
        let block_hot_path_summary = snapshot(crate::memory::blocks::r4os_rust_blocks_stats);
        let loader_file_stats = snapshot(crate::loader::file::r4os_rust_module_file_stats);
        let fpu_status = snapshot(crate::arch::fpu::r4os_rust_fpu_status);
        let work_summary = snapshot(crate::driver::work::r4os_rust_driver_work_summary);
        let block_runtime =
            snapshot(crate::storage::block_workers::r4os_rust_block_workers_summary);
        let fs_diagnostic = snapshot(crate::program::system::io::r4os_rust_sys_failure);
        let tcp_summary = snapshot(crate::net::tcp::r4os_rust_net_tcp_summary);

        let fs_result = fs_diagnostic.result as u32;
        let fs_meta = if fs_diagnostic.sequence == 0 {
            0
        } else {
            0x8000_0000
                | ((fs_result & 0xff) << 20)
                | ((fs_diagnostic.kind & 0xf) << 16)
                | (fs_diagnostic.task_id & 0xffff)
        };
        let fs_lookup = if fs_diagnostic.lookup_stage != 0 {
            fs_diagnostic.lookup_stage
        } else {
            (fs_diagnostic.sequence & 0x000f_ffff) << 12
        };
        let (fpu_lazy_saves, fpu_lazy_skips, fpu_valid_count) =
            crate::sched::task::observer_fpu_counts();
        let mut storage_queue_depth_total = 0u32;
        let mut storage_queue_used_total = 0u32;
        let mut storage_queue_high_water_total = 0u32;
        let mut storage_busy_devices = 0u32;
        let mut storage_failed_devices = 0u32;
        let mut storage_read_ops = 0u64;
        let mut storage_read_sectors = 0u64;
        let mut storage_read_failures = 0u64;
        let mut storage_write_ops = 0u64;
        let mut storage_write_sectors = 0u64;
        let mut storage_write_failures = 0u64;
        let mut storage_flush_ops = 0u64;
        let mut storage_flush_failures = 0u64;
        let mut storage_busy_rejections = 0u64;
        let mut storage_timeout_failures = 0u64;
        let mut storage_completions = 0u64;
        let mut storage_backend_recoveries = 0u64;
        let mut storage_backend_recovery_failures = 0u64;
        let mut storage_queued_requests = 0u64;
        let mut storage_dequeued_requests = 0u64;
        let mut storage_queue_full_waits = 0u64;
        let mut storage_queue_full_rejections = 0u64;
        let mut storage_completion_waits = 0u64;
        let mut storage_completion_timeouts = 0u64;
        let mut storage_completion_total_ticks = 0u64;
        let mut storage_completion_max_ticks = 0u64;
        let mut storage_completion_last_ticks = 0u64;
        for index in 0..crate::storage::block_registry::slot_count() {
            let Some(dev) = crate::storage::block_diagnostics::device_snapshot(index) else {
                continue;
            };
            let storage_stats = dev.stats;
            storage_queue_depth_total = storage_queue_depth_total.wrapping_add(dev.queue_depth);
            storage_queue_used_total = storage_queue_used_total.wrapping_add(dev.queue_used);
            storage_queue_high_water_total =
                storage_queue_high_water_total.wrapping_add(storage_stats.queue_high_water);
            if dev.state == 2 {
                storage_busy_devices += 1
            }
            if dev.state == 4 {
                storage_failed_devices += 1
            }
            storage_read_ops = storage_read_ops.wrapping_add(storage_stats.read_ops);
            storage_read_sectors = storage_read_sectors.wrapping_add(storage_stats.read_sectors);
            storage_read_failures = storage_read_failures.wrapping_add(storage_stats.read_failures);
            storage_write_ops = storage_write_ops.wrapping_add(storage_stats.write_ops);
            storage_write_sectors = storage_write_sectors.wrapping_add(storage_stats.write_sectors);
            storage_write_failures =
                storage_write_failures.wrapping_add(storage_stats.write_failures);
            storage_flush_ops = storage_flush_ops.wrapping_add(storage_stats.flush_ops);
            storage_flush_failures =
                storage_flush_failures.wrapping_add(storage_stats.flush_failures);
            storage_busy_rejections =
                storage_busy_rejections.wrapping_add(storage_stats.busy_rejections);
            storage_timeout_failures =
                storage_timeout_failures.wrapping_add(storage_stats.timeout_failures);
            storage_completions = storage_completions.wrapping_add(storage_stats.completions);
            storage_backend_recoveries =
                storage_backend_recoveries.wrapping_add(storage_stats.backend_recoveries);
            storage_backend_recovery_failures = storage_backend_recovery_failures
                .wrapping_add(storage_stats.backend_recovery_failures);
            storage_queued_requests =
                storage_queued_requests.wrapping_add(storage_stats.queued_requests);
            storage_dequeued_requests =
                storage_dequeued_requests.wrapping_add(storage_stats.dequeued_requests);
            storage_queue_full_waits =
                storage_queue_full_waits.wrapping_add(storage_stats.queue_full_waits);
            storage_queue_full_rejections =
                storage_queue_full_rejections.wrapping_add(storage_stats.queue_full_rejections);
            storage_completion_waits =
                storage_completion_waits.wrapping_add(storage_stats.completion_waits);
            storage_completion_timeouts =
                storage_completion_timeouts.wrapping_add(storage_stats.completion_timeouts);
            storage_completion_total_ticks =
                storage_completion_total_ticks.wrapping_add(storage_stats.completion_total_ticks);
            storage_completion_max_ticks =
                storage_completion_max_ticks.max(storage_stats.completion_max_ticks);
            storage_completion_last_ticks =
                storage_completion_last_ticks.max(storage_stats.completion_last_ticks);
        }
        let mut flags = 0u32;
        if sched.initialized {
            flags |= performance_flag_scheduler_ready;
        }
        if display_stats.registered {
            flags |= performance_flag_display_ready | performance_flag_display_responsiveness_ready;
        }
        if boot.initialized {
            flags |= performance_flag_boot_perf_ready;
        }
        if loader.initialized != 0 {
            flags |= performance_flag_loader_performance_ready;
        }
        if hot_path_summary.range_index_capacity != 0 {
            flags |= performance_flag_hot_path_index_ready;
        }
        if loader.initialized != 0 {
            flags |= performance_flag_loader_memory_ready;
        }
        flags |= performance_flag_wait_objects_ready;
        flags |= performance_flag_lock_diagnostics_ready;
        flags |= performance_flag_storage_request_queue_ready;
        flags |= performance_flag_fs_request_ready;
        flags |= performance_flag_service_queue_ready;
        flags |= performance_flag_preemption_readiness_ready;
        flags |= performance_flag_fs_page_cache_ready;
        flags |= performance_flag_fs_writeback_ready;
        flags |= performance_flag_fs_reclaim_ready;
        flags |= performance_flag_fs_pmm_reclaim_ready;
        flags |= performance_flag_global_reclaim_ready;
        flags |= performance_flag_memory_backing_store_ready;
        flags |= performance_flag_memory_backing_store_slots_ready;
        flags |= performance_flag_memory_pager_gates_ready;
        flags |= performance_flag_memory_page_io_ready;
        flags |= performance_flag_memory_vm_page_state_ready;
        flags |= performance_flag_memory_eviction_ready;
        flags |= performance_flag_memory_pager_error_policy_ready;
        flags |= performance_flag_productive_preemption_ready;
        flags |= performance_flag_scheduler_latency_ready;
        if work_summary.initialized != 0 && work_summary.worker_started != 0 {
            flags |= performance_flag_driver_workqueue_ready;
        }
        if block_runtime.worker_started != 0
            && block_runtime.worker_runtime_requests != 0
            && block_runtime.worker_runtime_completions != 0
            && block_runtime.completion_signals != 0
        {
            flags |= performance_flag_storage_driver_completion_ready;
        }
        if fpu_status.enabled {
            flags |= performance_flag_fpu_state_ready;
        }
        if fpu_status.avx_enabled && fpu_status.avx2_enabled {
            flags |= performance_flag_avx_state_ready;
        }

        let mut preemption_gate_mask = 0u32;
        if sched.preemption_enabled == 0 {
            preemption_gate_mask |= performance_preemption_gate_productive_disabled;
        }
        if sched.preempt_disable_depth != 0 {
            preemption_gate_mask |= performance_preemption_gate_kernel_critical;
        }
        if !fpu_status.enabled {
            preemption_gate_mask |= performance_preemption_gate_fpu_state;
        }

        let result = ProgramPerformanceSummary {
            version: negotiated_version,
            size: (copy_size)
                .try_into()
                .expect("original performance field range"),
            flags,
            missing_flags: performance_missing_fs_latency_histogram
                | performance_missing_service_latency_histogram
                | performance_missing_tcp_latency_histogram,
            ticks: sched.ticks,
            tick_hz: crate::platform::timer::frequency(),
            current_task_index: sched.current_index,
            task_max_count: tasks.total,
            task_count: tasks.total,
            task_ready: tasks.ready,
            task_running: tasks.running,
            task_blocked: tasks.blocked,
            task_dead: tasks.dead,
            task_workers: tasks.workers,
            scheduler_yields: sched.yields,
            scheduler_sleeps: sched.sleeps,
            scheduler_wakes: sched.wakes,
            scheduler_idle_waits: sched.idle_waits,
            boot_phase_count: boot.phase_count,
            boot_current_phase: u32::from(boot.current_phase),
            reserved0: fs_meta,
            boot_transition_count: boot.transition_count,
            boot_total_ticks: boot.total_ticks,
            storage_device_count: clamp_usize(crate::storage::block_registry::count()),
            storage_queue_depth_total,
            storage_queue_used_total,
            storage_queue_high_water_total,
            storage_busy_devices,
            storage_failed_devices,
            storage_read_ops,
            storage_read_sectors,
            storage_read_failures,
            storage_write_ops,
            storage_write_sectors,
            storage_write_failures,
            storage_flush_ops,
            storage_flush_failures,
            storage_busy_rejections,
            storage_timeout_failures,
            storage_completions,
            storage_backend_recoveries,
            storage_backend_recovery_failures,
            storage_queued_requests,
            storage_dequeued_requests,
            storage_queue_full_waits,
            storage_queue_full_rejections,
            storage_completion_waits,
            storage_completion_timeouts,
            storage_completion_total_ticks,
            storage_completion_max_ticks,
            storage_completion_last_ticks,
            fs_requests: fs_summary.requests,
            fs_completed: fs_summary.completed,
            fs_failed: fs_summary.failed,
            fs_read_requests: fs_summary.read_requests,
            fs_write_requests: fs_summary.write_requests,
            fs_metadata_requests: fs_summary.metadata_requests,
            fs_stream_requests: fs_summary.stream_requests,
            fs_lock_acquires: fs_summary.lock_acquires,
            fs_lock_contention_waits: fs_summary.lock_contention_waits,
            fs_lock_timeouts: fs_summary.lock_timeouts,
            fs_boot_bypass: fs_summary.boot_bypass,
            fs_total_ticks: fs_summary.total_ticks,
            fs_max_ticks: fs_summary.max_ticks,
            fs_last_ticks: fs_summary.last_ticks,
            fs_active_kind: fs_summary.active_kind,
            fs_last_kind: fs_summary.last_kind,
            fs_active_drive: fs_summary.active_drive,
            fs_last_drive: fs_summary.last_drive,
            service_max_count: svc.max_services,
            services_used: svc.used_services,
            services_running: svc.running_services,
            service_endpoints: svc.endpoints_used,
            service_request_pending: svc.request_pending,
            service_response_pending: svc.response_pending,
            service_queue_depth_total: svc.queue_depth_total,
            service_queue_used_total: svc.queue_used_total,
            service_queue_high_water_total: svc.queue_high_water_total,
            service_active_workers: svc.active_workers,
            service_max_active_workers: svc.max_active_workers,
            service_open_handles: svc.open_handles,
            service_requests: svc.requests,
            service_responses: svc.responses,
            service_drops: svc.drops,
            service_busy_rejections: svc.busy_rejections,
            service_timeouts: svc.timeouts,
            service_cancellations: svc.cancellations,
            service_completion_waits: svc.completion_waits,
            service_completion_timeouts: svc.completion_timeouts,
            service_completion_wait_rounds: svc.completion_wait_rounds,
            service_targeted_response_wakes: svc.targeted_response_wakes,
            service_targeted_response_wake_misses: svc.targeted_response_wake_misses,
            service_admission_waits: svc.admission_waits,
            service_admission_timeouts: svc.admission_timeouts,
            service_payload_copy_bytes: svc.payload_copy_bytes,
            service_payload_clear_bytes: svc.payload_clear_bytes,
            service_slot_metadata_resets: svc.slot_metadata_resets,
            service_endpoint_metadata_resets: svc.endpoint_metadata_resets,
            service_endpoint_payload_reset_bytes: svc.endpoint_payload_reset_bytes,
            service_queue_scan_passes: svc.queue_scan_passes,
            service_queue_scan_slots: svc.queue_scan_slots,
            service_endpoint_revalidations: svc.endpoint_revalidations,
            service_endpoint_stale_rejections: svc.endpoint_stale_rejections,
            service_lock_family_count: svc.lock_family_count,
            service_lock_reserved0: svc.lock_reserved0,
            service_lock_acquisitions: svc.lock_acquisitions,
            service_lock_contentions: svc.lock_contentions,
            service_lock_wait_ns: svc.lock_wait_ns,
            service_lock_wait_max_ns: svc.lock_wait_max_ns,
            service_lock_hold_ns: svc.lock_hold_ns,
            service_lock_hold_max_ns: svc.lock_hold_max_ns,
            service_lock_timing_unavailable: svc.lock_timing_unavailable,
            service_lock_timing_stride: svc.lock_timing_stride,
            service_lock_timing_reserved0: svc.lock_timing_reserved0,
            service_lock_timing_samples: svc.lock_timing_samples,
            service_registry_index_queries: svc.registry_index_queries,
            service_registry_refresh_requests: svc.registry_refresh_requests,
            service_registry_refresh_visits: svc.registry_refresh_visits,
            service_registry_instance_lookups: svc.registry_instance_lookups,
            service_registry_index_end_markers: svc.registry_index_end_markers,
            tcp_max_connections: tcp_summary.max_connections,
            tcp_active_connections: tcp_summary.active_connections,
            tcp_active_listeners: tcp_summary.active_listeners,
            tcp_buffer_size: tcp_summary.buffer_size,
            tcp_data_tx: tcp_summary.data_tx,
            tcp_data_rx: tcp_summary.data_rx,
            tcp_retransmits: tcp_summary.retransmits,
            tcp_rx_drops: tcp_summary.rx_drops,
            tcp_timeouts: tcp_summary.timeouts,
            tcp_checksum_errors: tcp_summary.checksum_errors,
            display_present_count: display_stats.present_count,
            display_present_bytes_total: display_stats.present_bytes_total,
            display_last_present_bytes: display_stats.last_present_bytes,
            display_full_present_count: display_stats.full_present_count,
            display_partial_present_count: display_stats.partial_present_count,
            display_present_total_ticks: display_stats.present_total_ticks,
            display_present_max_ticks: display_stats.present_max_ticks,
            display_present_last_ticks: display_stats.present_last_ticks,
            display_present_slow_count: display_stats.present_slow_count,
            audio_open_streams: audio.open_streams,
            audio_registered_backends: audio.registered_backends,
            audio_active_backends: audio.active_backends,
            audio_registered_synths: audio.registered_synths,
            audio_stream_writes: audio.total_stream_writes,
            audio_backend_ok: audio.backend_ok,
            audio_backend_fail: audio.backend_fail,
            audio_backend_underruns: audio.backend_status_underruns,
            audio_backend_errors: audio.backend_status_errors,
            audio_stream_ring_bytes: audio.stream_ring_bytes,
            audio_stream_available_bytes: audio.stream_available_bytes,
            audio_stream_high_water_bytes: audio.stream_available_high_water_bytes,
            audio_stream_write_truncations: audio.stream_write_truncations,
            audio_stream_dropped_bytes: audio.stream_dropped_bytes,
            audio_stream_write_total_ticks: audio_duration_ticks_unavailable,
            audio_stream_write_max_ticks: audio_duration_ticks_unavailable,
            audio_stream_write_last_ticks: audio_duration_ticks_unavailable,
            audio_backend_write_calls: audio.backend_write_calls,
            audio_backend_write_total_ticks: audio_duration_ticks_unavailable,
            audio_backend_write_max_ticks: audio_duration_ticks_unavailable,
            audio_backend_write_last_ticks: audio_duration_ticks_unavailable,
            audio_backend_refills: audio.backend_status_refills,
            audio_backend_silence_refills: audio.backend_status_silence_refills,
            audio_backend_buffer_bytes: audio.backend_status_buffer_bytes,
            audio_backend_queued_buffers: audio.backend_status_queued_buffers,
            audio_backend_last_buffer_bytes: audio.backend_status_last_buffer_bytes,
            audio_backend_refill_total_ticks: audio_duration_ticks_unavailable,
            audio_backend_refill_max_ticks: audio_duration_ticks_unavailable,
            audio_backend_refill_last_ticks: audio_duration_ticks_unavailable,
            loader_initialized: loader.initialized,
            loader_started: loader.loader_started,
            loader_completed: loader.loader_completed,
            loader_r4p_runtime_started: loader.r4p_runtime_started,
            loader_r4p_runtime_completed: loader.r4p_runtime_completed,
            loader_service_boot_status: loader.service_boot_status,
            loader_boot_critical_count: loader.boot_critical_count,
            loader_lazy_candidate_count: loader.lazy_candidate_count,
            loader_total_ticks: loader.loader_total_ticks,
            loader_r4p_runtime_total_ticks: loader.r4p_runtime_total_ticks,
            loader_service_boot_ticks: loader.service_boot_ticks,
            loader_config_load_ticks: loader.config_load_ticks,
            loader_config_bytes: loader.config_bytes,
            loader_config_driver_count: loader.config_driver_count,
            loader_config_disabled_count: loader.config_disabled_count,
            loader_config_option_count: loader.config_option_count,
            loader_r4l_scan_entries: loader.r4l_scan_entries,
            loader_r4l_candidates: loader.r4l_candidates,
            loader_r4l_loaded: loader.r4l_loaded,
            loader_r4l_failed: loader.r4l_failed,
            loader_r4l_scan_ticks: loader.r4l_scan_ticks,
            loader_r4l_read_ticks: loader.r4l_read_ticks,
            loader_r4l_resolve_ticks: loader.r4l_resolve_ticks,
            loader_r4l_load_ticks: loader.r4l_load_ticks,
            loader_r4d_scan_entries: loader.r4d_scan_entries,
            loader_r4d_candidates: loader.r4d_candidates,
            loader_r4d_discovered: loader.r4d_discovered,
            loader_r4d_failed: loader.r4d_failed,
            loader_r4d_scan_ticks: loader.r4d_scan_ticks,
            loader_r4d_read_ticks: loader.r4d_read_ticks,
            loader_r4d_probe_ticks: loader.r4d_probe_ticks,
            loader_r4p_scan_entries: loader.r4p_scan_entries,
            loader_r4p_candidates: loader.r4p_candidates,
            loader_r4p_discovered: loader.r4p_discovered,
            loader_r4p_active: loader.r4p_active,
            loader_r4p_blocked: loader.r4p_blocked,
            loader_r4p_failed: loader.r4p_failed,
            loader_r4p_scan_ticks: loader.r4p_scan_ticks,
            loader_r4p_read_ticks: loader.r4p_read_ticks,
            loader_r4p_resolve_ticks: loader.r4p_resolve_ticks,
            loader_r4p_init_ticks: loader.r4p_init_ticks,
            hot_path_vm_range_index_capacity: hot_path_summary.range_index_capacity,
            hot_path_vm_range_index_entries: hot_path_summary.range_index_entries,
            hot_path_vm_range_index_tombstones: hot_path_summary.range_index_tombstones,
            hot_path_vm_range_index_probe_max: hot_path_summary.range_index_probe_max,
            hot_path_vm_range_index_probe_last: hot_path_summary.range_index_probe_last,
            hot_path_vm_range_free_slot_probe_max: hot_path_summary.range_free_slot_probe_max,
            hot_path_vm_range_free_slot_probe_last: hot_path_summary.range_free_slot_probe_last,
            hot_path_bounded_block_device_scan_max: clamp_usize(
                crate::storage::block_registry::max_devices(),
            ),
            hot_path_bounded_tcp_connection_scan_max: (crate::net::tcp::MAX_CONNECTIONS)
                .try_into()
                .expect("original performance field range"),
            hot_path_vm_range_index_lookups: hot_path_summary.range_index_lookups,
            hot_path_vm_range_index_hits: hot_path_summary.range_index_hits,
            hot_path_vm_range_index_misses: hot_path_summary.range_index_misses,
            hot_path_vm_range_index_probe_total: hot_path_summary.range_index_probe_total,
            hot_path_vm_range_index_rebuilds: hot_path_summary.range_index_rebuilds,
            hot_path_vm_range_index_insert_failures: hot_path_summary.range_index_insert_failures,
            hot_path_vm_range_free_slot_lookups: hot_path_summary.range_free_slot_lookups,
            hot_path_vm_range_free_slot_probe_total: hot_path_summary.range_free_slot_probe_total,
            hot_path_memory_block_physical_index_entries: block_hot_path_summary
                .physical_index_entries,
            hot_path_memory_block_physical_step_max: block_hot_path_summary.physical_step_max,
            hot_path_memory_block_id_index_entries: block_hot_path_summary.id_index_entries,
            hot_path_memory_block_id_step_max: block_hot_path_summary.id_index_step_max,
            hot_path_memory_block_free_slot_word_step_max: block_hot_path_summary
                .free_slot_word_step_max,
            hot_path_memory_vm_range_address_entries: hot_path_summary.range_address_entries,
            hot_path_memory_vm_range_address_probe_max: hot_path_summary.range_address_probe_max,
            hot_path_memory_vm_range_address_probe_last: hot_path_summary.range_address_probe_last,
            hot_path_memory_vm_commit_span_active: hot_path_summary.commit_span_active,
            hot_path_memory_vm_commit_span_step_max: hot_path_summary.commit_span_step_max,
            hot_path_memory_vm_page_state_span_active: hot_path_summary.page_state_span_active,
            hot_path_memory_vm_page_state_span_step_max: hot_path_summary.page_state_span_step_max,
            hot_path_memory_block_physical_lookups: block_hot_path_summary.physical_lookups,
            hot_path_memory_block_physical_steps: block_hot_path_summary.physical_steps,
            hot_path_memory_block_physical_mutations: block_hot_path_summary.physical_mutations,
            hot_path_memory_block_physical_rebuilds: block_hot_path_summary.physical_rebuilds,
            hot_path_memory_block_id_lookups: block_hot_path_summary.id_index_lookups,
            hot_path_memory_block_id_steps: block_hot_path_summary.id_index_steps,
            hot_path_memory_block_free_slot_lookups: block_hot_path_summary.free_slot_lookups,
            hot_path_memory_block_free_slot_word_steps: block_hot_path_summary.free_slot_word_steps,
            hot_path_memory_block_claim_transactions: block_hot_path_summary.claim_transactions,
            hot_path_memory_block_claim_rollbacks: block_hot_path_summary.claim_rollbacks,
            hot_path_memory_vm_range_address_lookups: hot_path_summary.range_address_lookups,
            hot_path_memory_vm_range_address_probe_total: hot_path_summary
                .range_address_probe_total,
            hot_path_memory_vm_commit_span_lookups: hot_path_summary.commit_span_lookups,
            hot_path_memory_vm_commit_span_steps: hot_path_summary.commit_span_steps,
            hot_path_memory_vm_page_state_span_lookups: hot_path_summary.page_state_span_lookups,
            hot_path_memory_vm_page_state_span_steps: hot_path_summary.page_state_span_steps,
            hot_path_memory_vm_reclaim_range_steps: hot_path_summary.reclaim_cursor_range_steps,
            hot_path_memory_vm_reclaim_span_steps: hot_path_summary.reclaim_cursor_span_steps,
            hot_path_memory_vm_reclaim_page_steps: hot_path_summary.reclaim_cursor_page_steps,
            hot_path_memory_vm_reclaim_wraps: hot_path_summary.reclaim_cursor_wraps,
            fs_drive_gate_count: clamp_usize(crate::fs::request::DRIVE_GATE_COUNT),
            fs_active_requests: fs_summary.active_requests,
            fs_parallel_active_max: fs_summary.parallel_active_max,
            storage_controller_count: block_runtime.controller_count,
            storage_worker_count: block_runtime.worker_count,
            storage_worker_parallel_active: block_runtime.worker_parallel_active,
            storage_worker_parallel_active_max: block_runtime.worker_parallel_active_max,
            fs_single_drive_requests: fs_summary.single_drive_requests,
            fs_cross_drive_requests: fs_summary.cross_drive_requests,
            fs_global_requests: fs_summary.global_requests,
            storage_worker_start_failures: block_runtime.worker_start_failures,
            storage_direct_requests: block_runtime.direct_requests,
            storage_direct_bytes: block_runtime.direct_bytes,
            storage_bounce_allocations: block_runtime.bounce_allocations,
            storage_bounce_bytes: block_runtime.bounce_bytes,
            storage_bounce_copy_bytes: block_runtime.bounce_copy_bytes,
            storage_direct_timeout_waits: block_runtime.direct_timeout_waits,
            loader_file_active_buffers: loader_file_stats.active_buffers,
            loader_file_reserved_bytes: loader_file_stats.reserved_bytes,
            loader_file_committed_bytes: loader_file_stats.committed_bytes,
            loader_file_peak_reserved_bytes: loader_file_stats.peak_reserved_bytes,
            loader_file_peak_committed_bytes: loader_file_stats.peak_committed_bytes,
            loader_file_full_reads: loader_file_stats.full_reads,
            loader_file_range_reads: loader_file_stats.range_reads,
            loader_file_reserve_failures: loader_file_stats.reserve_failures,
            loader_file_commit_failures: loader_file_stats.commit_failures,
            loader_file_read_failures: loader_file_stats.read_failures,
            loader_file_short_reads: loader_file_stats.short_reads,
            loader_file_release_failures: loader_file_stats.release_failures,
            loader_file_pressure_reclaim_attempts: loader_file_stats.pressure_reclaim_attempts,
            loader_file_pressure_reclaimed_frames: loader_file_stats.pressure_reclaimed_frames,
            loader_file_pressure_failures: loader_file_stats.pressure_failures,
            loader_file_range_read_bytes: loader_file_stats.range_read_bytes,
            loader_metadata_reader_initializations: loader_file_stats
                .metadata_reader_initializations,
            loader_metadata_logical_reads: loader_file_stats.metadata_logical_reads,
            loader_metadata_logical_bytes: loader_file_stats.metadata_logical_bytes,
            loader_metadata_window_hits: loader_file_stats.metadata_window_hits,
            loader_metadata_window_fills: loader_file_stats.metadata_window_fills,
            loader_metadata_window_fill_bytes: loader_file_stats.metadata_window_fill_bytes,
            loader_metadata_direct_reads: loader_file_stats.metadata_direct_reads,
            loader_metadata_direct_bytes: loader_file_stats.metadata_direct_bytes,
            loader_metadata_window_capacity_bytes: ((crate::loader::file::WINDOW_SIZE
                * crate::loader::file::WINDOW_COUNT)
                as u64),
            wait_object_waits: sched.object_waits,
            wait_object_wakes: sched.object_wakes,
            wait_object_timeouts: sched.object_timeouts,
            wait_object_cancellations: sched.object_cancels,
            wait_queue_waits: wait_summary.queue_waits,
            wait_queue_wake_one: wait_summary.wake_one,
            wait_queue_wake_all: wait_summary.wake_all,
            wait_queue_timeouts: wait_summary.timeouts,
            wait_queue_cancellations: wait_summary.cancellations,
            wait_queue_drops: wait_summary.drops,
            lock_acquires: lock_summary.acquires,
            lock_releases: lock_summary.releases,
            lock_recursive_acquires: lock_summary.recursive_acquires,
            lock_contention_waits: lock_summary.contention_waits,
            lock_contention_timeouts: lock_summary.contention_timeouts,
            lock_order_violations: lock_summary.order_violations,
            lock_sleep_checks: lock_summary.sleep_checks,
            lock_sleep_under_lock: lock_summary.sleep_under_lock,
            lock_sleep_under_no_sleep_lock: lock_summary.sleep_under_no_sleep_lock,
            lock_unlock_mismatches: lock_summary.unlock_mismatches,
            lock_held_slots_used: lock_summary.held_slots_used,
            lock_current_depth: lock_summary.current_depth,
            lock_max_depth: lock_summary.max_depth,
            fpu_lazy_saves,
            fpu_lazy_skips,
            lock_tracking_drops: lock_summary.tracking_drops,
            preemption_supported: sched.preemption_supported,
            preemption_enabled: sched.preemption_enabled,
            preemption_test_mode: sched.preemption_test_mode,
            preemption_gate_mask,
            preempt_disable_depth: sched.preempt_disable_depth,
            preempt_disable_max_depth: sched.preempt_disable_max_depth,
            reserved1: fs_lookup,
            preempt_disable_underflows: sched.preempt_disable_underflows,
            preempt_disable_calls: sched.preempt_disable_calls,
            preempt_enable_calls: sched.preempt_enable_calls,
            preemption_simulation_ticks: sched.preemption_simulation_ticks,
            preemption_eligible_ticks: sched.preemption_eligible_ticks,
            preemption_deferred_disabled: sched.preemption_deferred_disabled,
            preemption_deferred_critical: sched.preemption_deferred_critical,
            preemption_deferred_no_task: sched.preemption_deferred_no_task,
            preemption_deferred_no_ready: sched.preemption_deferred_no_ready,
            preemption_switch_ticks: sched.preemption_switch_ticks,
            long_running_task_warnings: sched.long_running_task_warnings,
            starvation_warnings: sched.starvation_warnings,
            fpu_state_supported: if fpu_status.initialized { 1 } else { 0 },
            fpu_state_enabled: if fpu_status.enabled { 1 } else { 0 },
            fpu_state_backend: fpu_status.backend,
            fpu_state_bytes: fpu_status.state_bytes,
            fpu_state_storage_bytes: fpu_status.state_storage_bytes,
            fpu_task_state_count: fpu_valid_count,
            fpu_avx_supported: if fpu_status.avx_supported { 1 } else { 0 },
            fpu_avx_enabled: if fpu_status.avx_enabled { 1 } else { 0 },
            fpu_avx2_supported: if fpu_status.avx2_supported { 1 } else { 0 },
            fpu_avx2_enabled: if fpu_status.avx2_enabled { 1 } else { 0 },
            fpu_simd_abi: fpu_status.simd_abi,
            fpu_xsave_required_bytes: fpu_status.state_bytes,
            fpu_xcr0_mask: fpu_status.xcr0_mask,
            fpu_save_count: fpu_status.save_count,
            fpu_restore_count: fpu_status.restore_count,
            fpu_task_init_count: fpu_status.task_init_count,
            fpu_task_state_bytes: fpu_status.task_state_bytes,
            fs_cache_capacity: fs_cache.capacity,
            fs_cache_sector_bytes: fs_cache.sector_bytes,
            fs_cache_entries_used: fs_cache.entries_used,
            fs_cache_dirty_entries: fs_cache.dirty_entries,
            fs_cache_reads: fs_cache.reads,
            fs_cache_hits: fs_cache.hits,
            fs_cache_misses: fs_cache.misses,
            fs_cache_fills: fs_cache.fills,
            fs_cache_evictions: fs_cache.evictions,
            fs_cache_invalidations: fs_cache.invalidations,
            fs_cache_write_through_requests: fs_cache.write_through_requests,
            fs_cache_write_through_updates: fs_cache.write_through_updates,
            fs_cache_flushes: fs_cache.flushes,
            fs_cache_read_errors: fs_cache.read_errors,
            fs_cache_write_errors: fs_cache.write_errors,
            fs_cache_writeback_waits: fs_cache.writeback_waits,
            fs_cache_writeback_errors: fs_cache.writeback_errors,
            fs_cache_dirty_bytes: fs_cache.dirty_bytes,
            fs_cache_dirty_high_water_entries: fs_cache.dirty_high_water_entries,
            fs_cache_writeback_queue_depth: fs_cache.writeback_queue_depth,
            fs_cache_writeback_queue_high_water: fs_cache.writeback_queue_high_water,
            fs_cache_deferred_write_requests: fs_cache.deferred_write_requests,
            fs_cache_dirty_sector_updates: fs_cache.dirty_sector_updates,
            fs_cache_writeback_drains: fs_cache.writeback_drains,
            fs_cache_writeback_sectors: fs_cache.writeback_sectors,
            fs_cache_writeback_pressure_drains: fs_cache.writeback_pressure_drains,
            fs_cache_writeback_flush_drains: fs_cache.writeback_flush_drains,
            fs_cache_writeback_total_ticks: fs_cache.writeback_total_ticks,
            fs_cache_writeback_max_ticks: fs_cache.writeback_max_ticks,
            fs_cache_writeback_last_ticks: fs_cache.writeback_last_ticks,
            fs_cache_writeback_retries: fs_cache.writeback_retries,
            fs_cache_bulk_write_requests: fs_cache.bulk_write_requests,
            fs_cache_bulk_write_sectors: fs_cache.bulk_write_sectors,
            fs_cache_selective_flushes: fs_cache.selective_flushes,
            fs_cache_selective_writeback_sectors: fs_cache.selective_writeback_sectors,
            fs_cache_selective_foreign_dirty_sectors_skipped: fs_cache
                .selective_foreign_dirty_sectors_skipped,
            fs_cache_policy_version: fs_cache.policy_version,
            fs_cache_policy_device_capacity: fs_cache.policy_device_capacity,
            fs_cache_policy_dirty_high_pages: fs_cache.policy_dirty_high_pages,
            fs_cache_policy_dirty_low_pages: fs_cache.policy_dirty_low_pages,
            fs_cache_policy_max_dirty_age_ticks: fs_cache.policy_max_dirty_age_ticks,
            fs_cache_policy_background_page_budget: fs_cache.policy_background_page_budget,
            fs_cache_policy_worker_started: fs_cache.policy_worker_started,
            fs_cache_policy_worker_task_id: fs_cache.policy_worker_task_id,
            fs_cache_policy_device_dirty_high_water: fs_cache.policy_device_dirty_high_water,
            fs_cache_policy_worker_wakeups: fs_cache.policy_worker_wakeups,
            fs_cache_policy_background_drains: fs_cache.policy_background_drains,
            fs_cache_policy_background_sectors: fs_cache.policy_background_sectors,
            fs_cache_policy_background_pressure_drains: fs_cache.policy_background_pressure_drains,
            fs_cache_policy_background_age_drains: fs_cache.policy_background_age_drains,
            fs_cache_policy_background_errors: fs_cache.policy_background_errors,
            fs_cache_policy_clean_device_probes: fs_cache.policy_clean_device_probes,
            fs_cache_policy_dirty_device_probes: fs_cache.policy_dirty_device_probes,
            fs_cache_policy_full_scan_fallbacks: fs_cache.policy_full_scan_fallbacks,
            fs_cache_read_ahead_requests: fs_cache.read_ahead_requests,
            fs_cache_read_ahead_issued: fs_cache.read_ahead_issued,
            fs_cache_read_ahead_hits: fs_cache.read_ahead_hits,
            fs_cache_read_ahead_cancellations: fs_cache.read_ahead_cancellations,
            fs_cache_read_ahead_budget_skips: fs_cache.read_ahead_budget_skips,
            ntfs_metadata_cache_version: ntfs_metadata.version,
            ntfs_metadata_cache_active_volumes: ntfs_metadata.active_volumes,
            ntfs_metadata_cache_bytes_per_volume: ntfs_metadata.bytes_per_volume,
            ntfs_metadata_cache_slot_capacity: ntfs_metadata.slot_capacity,
            ntfs_metadata_record_capacity: ntfs_metadata.record_capacity,
            ntfs_metadata_attribute_capacity: ntfs_metadata.attribute_capacity,
            ntfs_metadata_index_capacity: ntfs_metadata.index_capacity,
            ntfs_metadata_path_capacity: ntfs_metadata.path_capacity,
            ntfs_metadata_record_entries: ntfs_metadata.record_entries,
            ntfs_metadata_attribute_entries: ntfs_metadata.attribute_entries,
            ntfs_metadata_index_entries: ntfs_metadata.index_entries,
            ntfs_metadata_path_entries: ntfs_metadata.path_entries,
            ntfs_metadata_mount_generation: ntfs_metadata.mount_generation,
            ntfs_metadata_content_generation: ntfs_metadata.content_generation,
            ntfs_metadata_negative_ttl_ticks: ntfs_metadata.negative_ttl_ticks,
            ntfs_metadata_record_hits: ntfs_metadata.record_hits,
            ntfs_metadata_record_misses: ntfs_metadata.record_misses,
            ntfs_metadata_record_stores: ntfs_metadata.record_stores,
            ntfs_metadata_record_evictions: ntfs_metadata.record_evictions,
            ntfs_metadata_attribute_hits: ntfs_metadata.attribute_hits,
            ntfs_metadata_attribute_misses: ntfs_metadata.attribute_misses,
            ntfs_metadata_attribute_stores: ntfs_metadata.attribute_stores,
            ntfs_metadata_attribute_evictions: ntfs_metadata.attribute_evictions,
            ntfs_metadata_index_hits: ntfs_metadata.index_hits,
            ntfs_metadata_index_misses: ntfs_metadata.index_misses,
            ntfs_metadata_index_stores: ntfs_metadata.index_stores,
            ntfs_metadata_index_evictions: ntfs_metadata.index_evictions,
            ntfs_metadata_path_queries: ntfs_metadata.path_queries,
            ntfs_metadata_path_positive_hits: ntfs_metadata.path_positive_hits,
            ntfs_metadata_path_negative_hits: ntfs_metadata.path_negative_hits,
            ntfs_metadata_path_misses: ntfs_metadata.path_misses,
            ntfs_metadata_path_positive_stores: ntfs_metadata.path_positive_stores,
            ntfs_metadata_path_negative_stores: ntfs_metadata.path_negative_stores,
            ntfs_metadata_path_expirations: ntfs_metadata.path_expirations,
            ntfs_metadata_lookup_tree_walks: ntfs_metadata.lookup_tree_walks,
            ntfs_metadata_recovery_cache_bypasses: ntfs_metadata.recovery_cache_bypasses,
            ntfs_metadata_mount_invalidations: ntfs_metadata.mount_invalidations,
            ntfs_metadata_mutation_invalidations: ntfs_metadata.mutation_invalidations,
            ntfs_metadata_external_invalidations: ntfs_metadata.external_invalidations,
            ntfs_metadata_invalidated_entries: ntfs_metadata.invalidated_entries,
            ntfs_metadata_reclaim_requests: ntfs_metadata.reclaim_requests,
            ntfs_metadata_reclaim_scans: ntfs_metadata.reclaim_scans,
            ntfs_metadata_reclaimed_entries: ntfs_metadata.reclaimed_entries,
            fs_cache_capacity_min_pages: fs_cache.capacity_min_pages,
            fs_cache_capacity_max_pages: fs_cache.capacity_max_pages,
            fs_cache_capacity_ram_limit_pages: fs_cache.capacity_ram_limit_pages,
            fs_cache_capacity_active_limit_pages: fs_cache.capacity_active_limit_pages,
            fs_cache_capacity_pressure_level: fs_cache.capacity_pressure_level,
            fs_cache_read_ahead_window_pages: fs_cache.read_ahead_window_pages,
            fs_cache_read_ahead_window_max_pages: fs_cache.read_ahead_window_max_pages,
            fs_cache_capacity_reserved0: fs_cache.capacity_reserved0,
            fs_cache_fill_run_requests: fs_cache.fill_run_requests,
            fs_cache_fill_run_backend_requests: fs_cache.fill_run_backend_requests,
            fs_cache_fill_run_pages: fs_cache.fill_run_pages,
            fs_cache_fill_run_sectors: fs_cache.fill_run_sectors,
            fs_cache_fill_run_bytes: fs_cache.fill_run_bytes,
            fs_cache_fill_run_failures: fs_cache.fill_run_failures,
            fs_cache_fill_run_retries: fs_cache.fill_run_retries,
            fs_cache_fill_run_max_pages: fs_cache.fill_run_max_pages,
            fs_cache_fill_scatter_copy_bytes: fs_cache.fill_scatter_copy_bytes,
            fs_cache_read_staging_copy_bytes: fs_cache.read_staging_copy_bytes,
            fs_cache_read_caller_copy_bytes: fs_cache.read_caller_copy_bytes,
            fs_cache_read_publish_lock_drops: fs_cache.read_publish_lock_drops,
            fs_cache_fill_lock_drops: fs_cache.fill_lock_drops,
            fs_cache_capacity_reductions: fs_cache.capacity_reductions,
            fs_cache_capacity_trimmed_pages: fs_cache.capacity_trimmed_pages,
            fs_cache_read_ahead_pages_scheduled: fs_cache.read_ahead_pages_scheduled,
            fs_cache_read_ahead_pages_issued: fs_cache.read_ahead_pages_issued,
            fs_cache_read_ahead_random_resets: fs_cache.read_ahead_random_resets,
            ntfs_metadata_payload_write_retentions: ntfs_metadata.payload_write_retentions,
            ntfs_metadata_system_write_retentions: ntfs_metadata.system_write_retentions,
            ntfs_metadata_targeted_invalidations: ntfs_metadata.targeted_invalidations,
            ntfs_metadata_targeted_record_invalidations: ntfs_metadata
                .targeted_record_invalidations,
            ntfs_metadata_targeted_attribute_invalidations: ntfs_metadata
                .targeted_attribute_invalidations,
            ntfs_metadata_targeted_directory_invalidations: ntfs_metadata
                .targeted_directory_invalidations,
            ntfs_metadata_global_mutation_invalidations: ntfs_metadata
                .global_mutation_invalidations,
            ntfs_metadata_recovery_invalidations: ntfs_metadata.recovery_invalidations,
            ntfs_metadata_mutation_invalidated_record_entries: ntfs_metadata
                .mutation_invalidated_record_entries,
            ntfs_metadata_mutation_invalidated_attribute_entries: ntfs_metadata
                .mutation_invalidated_attribute_entries,
            ntfs_metadata_mutation_invalidated_index_entries: ntfs_metadata
                .mutation_invalidated_index_entries,
            ntfs_metadata_mutation_invalidated_path_entries: ntfs_metadata
                .mutation_invalidated_path_entries,
            fs_cache_clean_reclaimable_entries: fs_cache.clean_reclaimable_entries,
            fs_cache_dirty_non_reclaimable_entries: fs_cache.dirty_non_reclaimable_entries,
            fs_cache_pagefile_ready: 0,
            fs_cache_pagefile_blockers: fs_cache_pagefile_blocker_no_pagefile
                | fs_cache_pagefile_blocker_no_swap
                | fs_cache_pagefile_blocker_no_pager,
            fs_cache_clean_reclaimable_bytes: fs_cache.clean_reclaimable_bytes,
            fs_cache_dirty_non_reclaimable_bytes: fs_cache.dirty_non_reclaimable_bytes,
            fs_cache_reclaim_scans: fs_cache.reclaim_scans,
            fs_cache_reclaim_clean_entries: fs_cache.reclaim_clean_entries,
            fs_cache_reclaim_dirty_drains: fs_cache.reclaim_dirty_drains,
            fs_cache_reclaim_failed_drains: fs_cache.reclaim_failed_drains,
            fs_cache_payload_frame_bytes: fs_cache.payload_frame_bytes,
            fs_cache_payload_frames: fs_cache.payload_frames,
            fs_cache_payload_bytes: fs_cache.payload_bytes,
            fs_cache_pmm_reclaimable_bytes: fs_cache.pmm_reclaimable_bytes,
            fs_cache_pmm_dirty_bytes: fs_cache.pmm_dirty_bytes,
            fs_cache_payload_allocations: fs_cache.payload_allocations,
            fs_cache_payload_allocation_failures: fs_cache.payload_allocation_failures,
            fs_cache_payload_releases: fs_cache.payload_releases,
            fs_cache_reclaim_returned_frames: fs_cache.reclaim_returned_frames,
            fs_cache_reclaim_returned_bytes: fs_cache.reclaim_returned_bytes,
            global_reclaim_attempts: reclaim_summary.attempts,
            global_reclaim_successes: reclaim_summary.successes,
            global_reclaim_failures: reclaim_summary.failures,
            global_reclaim_requested_frames: reclaim_summary.requested_frames,
            global_reclaim_returned_frames: reclaim_summary.returned_frames,
            global_reclaim_returned_bytes: reclaim_summary.returned_bytes,
            global_reclaim_dirty_drains: reclaim_summary.dirty_drains,
            global_reclaim_failed_drains: reclaim_summary.failed_drains,
            global_reclaim_total_ticks: reclaim_summary.total_ticks,
            global_reclaim_max_ticks: reclaim_summary.max_ticks,
            global_reclaim_last_ticks: reclaim_summary.last_ticks,
            global_reclaim_last_reason: reclaim_summary.last_reason,
            global_reclaim_last_requested_frames: reclaim_summary.last_requested_frames,
            global_reclaim_last_returned_frames: reclaim_summary.last_returned_frames,
            global_reclaim_reserved0: 0,
            global_reclaim_fs_returned_frames: reclaim_summary.fs_returned_frames,
            global_reclaim_fs_returned_bytes: reclaim_summary.fs_returned_bytes,
            global_reclaim_vm_returned_frames: reclaim_summary.vm_returned_frames,
            global_reclaim_vm_returned_bytes: reclaim_summary.vm_returned_bytes,
            global_reclaim_vm_page_outs: reclaim_summary.vm_page_outs,
            global_reclaim_vm_failures: reclaim_summary.vm_failures,
            memory_backing_store_status: backing_summary.last_status,
            memory_backing_store_flags: backing_summary.last_flags,
            memory_backing_store_blockers: backing_summary.last_blockers,
            memory_backing_store_cluster_bytes: backing_summary.last_cluster_bytes,
            memory_backing_store_requested_bytes: backing_summary.last_requested_bytes,
            memory_backing_store_available_bytes: backing_summary.last_available_bytes,
            memory_backing_store_file_size: backing_summary.last_file_size,
            memory_backing_store_probe_count: backing_summary.probes,
            memory_backing_store_ready_count: backing_summary.ready,
            memory_backing_store_failure_count: backing_summary.failures,
            memory_backing_store_first_cluster: backing_summary.last_first_cluster,
            memory_backing_store_pager_enabled: 0,
            memory_backing_store_anonymous_paging_enabled: 0,
            memory_backing_store_reserved0: 0,
            memory_backing_store_slot_status: backing_slot_summary.last_status,
            memory_backing_store_slot_operation: backing_slot_summary.last_operation,
            memory_backing_store_slot_flags: backing_slot_summary.last_flags,
            memory_backing_store_slot_blockers: backing_slot_summary.last_blockers,
            memory_backing_store_slot_bytes: backing_slot_summary.slot_bytes,
            memory_backing_store_slot_max_ranges: backing_slot_summary.max_ranges,
            memory_backing_store_slot_range_count: backing_slot_summary.range_count,
            memory_backing_store_slot_last_owner_kind: backing_slot_summary.last_owner_kind,
            memory_backing_store_slot_capacity: backing_slot_summary.capacity_slots,
            memory_backing_store_slot_reserved: backing_slot_summary.reserved_slots,
            memory_backing_store_slot_free: backing_slot_summary.free_slots,
            memory_backing_store_slot_valid: backing_slot_summary.valid_slots,
            memory_backing_store_slot_dirty: backing_slot_summary.dirty_slots,
            memory_backing_store_slot_error: backing_slot_summary.error_slots,
            memory_backing_store_slot_last_reservation_id: backing_slot_summary.last_reservation_id,
            memory_backing_store_slot_last_owner_id: backing_slot_summary.last_owner_id,
            memory_backing_store_slot_last_region_id: backing_slot_summary.last_region_id,
            memory_backing_store_slot_pager_enabled: 0,
            memory_backing_store_slot_eviction_enabled: 1,
            memory_backing_store_slot_page_in_enabled: 1,
            memory_backing_store_slot_page_out_enabled: 1,
            memory_backing_store_slot_reserved1: 0,
            memory_backing_store_slot_last_first_slot: backing_slot_summary.last_first_slot,
            memory_backing_store_slot_last_slot_count: backing_slot_summary.last_slot_count,
            memory_backing_store_slot_generation: backing_slot_summary.generation,
            memory_backing_store_slot_probe_count: backing_slot_summary.probes,
            memory_backing_store_slot_reserve_count: backing_slot_summary.reserves,
            memory_backing_store_slot_release_count: backing_slot_summary.releases,
            memory_backing_store_slot_error_mark_count: backing_slot_summary.error_marks,
            memory_backing_store_slot_recovery_count: backing_slot_summary.recoveries,
            memory_backing_store_slot_failure_count: backing_slot_summary.failures,
            memory_backing_store_slot_lifecycle_cleanup_count: backing_slot_summary
                .lifecycle_cleanups,
            memory_backing_store_slot_lifecycle_released_ranges: backing_slot_summary
                .lifecycle_released_ranges,
            memory_backing_store_slot_lifecycle_released_slots: backing_slot_summary
                .lifecycle_released_slots,
            memory_pager_gate_status: pager_gate_summary.last_status,
            memory_pager_gate_flags: pager_gate_summary.last_flags,
            memory_pager_gate_blockers: pager_gate_summary.last_blockers,
            memory_pager_gate_region_id: pager_gate_summary.last_region_id,
            memory_pager_gate_owner_id: pager_gate_summary.last_owner_id,
            memory_pager_gate_slot_bytes: 4096,
            memory_pager_gate_rollback_completed: pager_gate_summary.rollback_completed,
            memory_pager_gate_reserved0: 0,
            memory_pager_gate_requested_bytes: pager_gate_summary.requested_bytes,
            memory_pager_gate_committed_bytes: pager_gate_summary.committed_bytes,
            memory_pager_gate_resident_bytes: pager_gate_summary.resident_bytes,
            memory_pager_gate_nonresident_bytes: pager_gate_summary.nonresident_bytes,
            memory_pager_gate_requested_slots: pager_gate_summary.requested_slots,
            memory_pager_gate_prepared_slots: pager_gate_summary.prepared_slots,
            memory_pager_gate_capacity_slots: pager_gate_summary.capacity_slots,
            memory_pager_gate_free_before_slots: pager_gate_summary.free_before_slots,
            memory_pager_gate_free_after_slots: pager_gate_summary.free_after_slots,
            memory_pager_gate_reserved_before_slots: pager_gate_summary.reserved_before_slots,
            memory_pager_gate_reserved_after_slots: pager_gate_summary.reserved_after_slots,
            memory_pager_gate_last_reservation_id: pager_gate_summary.last_reservation_id,
            memory_pager_gate_commit_gate_enabled: pager_gate_summary.commit_gate_enabled,
            memory_pager_gate_fault_gate_enabled: pager_gate_summary.fault_gate_enabled,
            memory_pager_gate_pager_enabled: pager_gate_summary.pager_enabled,
            memory_pager_gate_eviction_enabled: pager_gate_summary.eviction_enabled,
            memory_pager_gate_page_in_enabled: pager_gate_summary.page_in_enabled,
            memory_pager_gate_page_out_enabled: pager_gate_summary.page_out_enabled,
            memory_pager_gate_reserved1: 0,
            memory_pager_gate_slot_generation: pager_gate_summary.slot_generation,
            memory_pager_gate_fault_count: pager_gate_summary.fault_count,
            memory_pager_gate_failed_faults: pager_gate_summary.failed_faults,
            memory_pager_gate_probe_count: pager_gate_summary.probes,
            memory_pager_gate_ready_count: pager_gate_summary.ready,
            memory_pager_gate_rollback_count: pager_gate_summary.rollbacks,
            memory_pager_gate_failure_count: pager_gate_summary.failures,
            memory_page_io_status: page_io_summary.last_status,
            memory_page_io_operation: page_io_summary.last_operation,
            memory_page_io_flags: page_io_summary.last_flags,
            memory_page_io_blockers: page_io_summary.last_blockers,
            memory_page_io_region_id: page_io_summary.last_region_id,
            memory_page_io_reservation_id: page_io_summary.last_reservation_id,
            memory_page_io_owner_kind: page_io_summary.last_owner_kind,
            memory_page_io_owner_id: page_io_summary.last_owner_id,
            memory_page_io_slot_bytes: 4096,
            memory_page_io_io_bytes: page_io_summary.io_bytes,
            memory_page_io_io_status: page_io_summary.io_status,
            memory_page_io_pager_enabled: page_io_summary.pager_enabled,
            memory_page_io_eviction_enabled: page_io_summary.eviction_enabled,
            memory_page_io_page_in_enabled: page_io_summary.page_in_enabled,
            memory_page_io_page_out_enabled: page_io_summary.page_out_enabled,
            memory_page_io_reserved0: 0,
            memory_page_io_region_offset: page_io_summary.region_offset,
            memory_page_io_page_count: page_io_summary.page_count,
            memory_page_io_transfer_bytes: page_io_summary.transfer_bytes,
            memory_page_io_expected_generation: page_io_summary.expected_generation,
            memory_page_io_backing_slot: page_io_summary.backing_slot,
            memory_page_io_backing_offset: page_io_summary.backing_offset,
            memory_page_io_capacity_slots: page_io_summary.capacity_slots,
            memory_page_io_reserved_slots: page_io_summary.reserved_slots,
            memory_page_io_valid_slots: page_io_summary.valid_slots,
            memory_page_io_dirty_slots: page_io_summary.dirty_slots,
            memory_page_io_error_slots: page_io_summary.error_slots,
            memory_page_io_slot_generation: page_io_summary.slot_generation,
            memory_page_io_prepare_count: page_io_summary.prepares,
            memory_page_io_page_out_count: page_io_summary.page_outs,
            memory_page_io_page_in_count: page_io_summary.page_ins,
            memory_page_io_failure_count: page_io_summary.failures,
            memory_page_io_retry_attempt_count: page_io_summary.retry_attempts,
            memory_page_io_retryable_failure_count: page_io_summary.retryable_failures,
            memory_page_io_permanent_failure_count: page_io_summary.permanent_failures,
            memory_page_io_retry_limit_hit_count: page_io_summary.retry_limit_hits,
            memory_page_io_failed_page_out_count: page_io_summary.failed_page_outs,
            memory_page_io_failed_page_in_count: page_io_summary.failed_page_ins,
            memory_page_io_data_preserved_pages: page_io_summary.data_preserved_pages,
            memory_page_io_data_lost_pages: page_io_summary.data_lost_pages,
            memory_vm_page_state_status: page_state_summary.last_status,
            memory_vm_page_state_operation: page_state_summary.last_operation,
            memory_vm_page_state_flags: page_state_summary.last_flags,
            memory_vm_page_state_blockers: page_state_summary.last_blockers,
            memory_vm_page_state_region_id: page_state_summary.last_region_id,
            memory_vm_page_state_page_size: (crate::memory::layout::PAGE_SIZE)
                .try_into()
                .expect("original performance field range"),
            memory_vm_page_state_max_spans: page_state_summary.max_spans,
            memory_vm_page_state_span_count: page_state_summary.span_count,
            memory_vm_page_state_slot_reservation_id: page_state_summary.last_slot_reservation_id,
            memory_vm_page_state_page_count_lo: clamp_u64(page_state_summary.last_page_count),
            memory_vm_page_state_reserved0: [0; 2],
            memory_vm_page_state_region_offset: page_state_summary.last_region_offset,
            memory_vm_page_state_page_count: page_state_summary.last_page_count,
            memory_vm_page_state_committed_pages: page_state_summary.committed_pages,
            memory_vm_page_state_resident_pages: page_state_summary.resident_pages,
            memory_vm_page_state_nonresident_pages: page_state_summary.nonresident_pages,
            memory_vm_page_state_dirty_pages: page_state_summary.dirty_pages,
            memory_vm_page_state_clean_pages: page_state_summary.clean_pages,
            memory_vm_page_state_pinned_pages: page_state_summary.pinned_pages,
            memory_vm_page_state_busy_pages: page_state_summary.busy_pages,
            memory_vm_page_state_error_pages: page_state_summary.error_pages,
            memory_vm_page_state_slot_bound_pages: page_state_summary.slot_bound_pages,
            memory_vm_page_state_slot_index: page_state_summary.last_slot_index,
            memory_vm_page_state_slot_generation: page_state_summary.last_slot_generation,
            memory_vm_page_state_transition_count: page_state_summary.transitions,
            memory_vm_page_state_dirty_mark_count: page_state_summary.dirty_marks,
            memory_vm_page_state_clean_mark_count: page_state_summary.clean_marks,
            memory_vm_page_state_slot_bind_count: page_state_summary.slot_binds,
            memory_vm_page_state_slot_clear_count: page_state_summary.slot_clears,
            memory_vm_page_state_pinned_mark_count: page_state_summary.pinned_marks,
            memory_vm_page_state_pinned_clear_count: page_state_summary.pinned_clears,
            memory_vm_page_state_busy_mark_count: page_state_summary.busy_marks,
            memory_vm_page_state_busy_clear_count: page_state_summary.busy_clears,
            memory_vm_page_state_error_mark_count: page_state_summary.error_marks,
            memory_vm_page_state_error_clear_count: page_state_summary.error_clears,
            memory_vm_page_state_table_full_failures: page_state_summary.table_full_failures,
            memory_vm_page_state_cleanup_pages: page_state_summary.cleanup_pages,
            memory_vm_page_state_fault_page_in_count: page_state_summary.fault_page_ins,
            memory_vm_page_state_fault_page_in_failure_count: page_state_summary
                .fault_page_in_failures,
            memory_vm_page_state_page_out_nonresident_pages: page_state_summary
                .page_out_nonresident_pages,
            memory_vm_eviction_attempt_count: page_state_summary.eviction_attempts,
            memory_vm_eviction_success_count: page_state_summary.eviction_successes,
            memory_vm_eviction_failure_count: page_state_summary.eviction_failures,
            memory_vm_eviction_candidate_count: page_state_summary.eviction_candidates,
            memory_vm_eviction_page_out_count: page_state_summary.eviction_page_outs,
            memory_vm_eviction_clean_page_count: page_state_summary.eviction_clean_pages,
            memory_vm_eviction_dirty_page_count: page_state_summary.eviction_dirty_pages,
            memory_vm_eviction_returned_frames: page_state_summary.eviction_returned_frames,
            memory_vm_eviction_no_backing_count: page_state_summary.eviction_no_backing,
            memory_vm_eviction_no_candidate_count: page_state_summary.eviction_no_candidate,
            memory_vm_eviction_slot_failure_count: page_state_summary.eviction_slot_failures,
            memory_vm_eviction_io_failure_count: page_state_summary.eviction_io_failures,
            memory_vm_eviction_skipped_nonresident: page_state_summary.eviction_skipped_nonresident,
            memory_vm_eviction_skipped_pinned: page_state_summary.eviction_skipped_pinned,
            memory_vm_eviction_skipped_busy: page_state_summary.eviction_skipped_busy,
            memory_vm_eviction_skipped_error: page_state_summary.eviction_skipped_error,
            memory_vm_eviction_skipped_unmapped: page_state_summary.eviction_skipped_unmapped,
            memory_vm_pager_failed_page_out_count: page_state_summary.pager_failed_page_outs,
            memory_vm_pager_failed_page_in_count: page_state_summary.pager_failed_page_ins,
            memory_vm_pager_data_preserved_pages: page_state_summary.pager_data_preserved_pages,
            memory_vm_pager_data_lost_pages: page_state_summary.pager_data_lost_pages,
            memory_vm_pager_dirty_preserved_pages: page_state_summary.pager_dirty_preserved_pages,
            memory_vm_pager_disabled_eviction_gates: page_state_summary
                .pager_disabled_eviction_gates,
            preemption_quantum_ticks: sched.preemption_quantum_ticks,
            preemption_quantum_expired: sched.preemption_quantum_expired,
            preemption_deferred_quantum: sched.preemption_deferred_quantum,
            preemption_deferred_kernel_ip: sched.preemption_deferred_kernel_ip,
            preemption_app_code_ticks: sched.preemption_app_code_ticks,
            scheduler_ready_latency_samples: sched.ready_latency_samples,
            scheduler_ready_latency_total_ticks: sched.ready_latency_total_ticks,
            scheduler_ready_latency_max_ticks: sched.ready_latency_max_ticks,
            scheduler_ready_latency_last_ticks: sched.ready_latency_last_ticks,
            scheduler_ready_waiting_max_ticks: sched.ready_waiting_max_ticks,
            scheduler_run_without_switch_max_ticks: sched.run_without_switch_max_ticks,
            scheduler_quantum_overrun_count: sched.quantum_overrun_count,
            scheduler_quantum_overrun_max_ticks: sched.quantum_overrun_max_ticks,
            scheduler_preemption_deferred_max_ticks: sched.preemption_deferred_max_ticks,
            scheduler_long_running_warn_ticks: sched.long_running_warn_threshold_ticks,
            scheduler_starvation_warn_ticks: sched.starvation_warn_threshold_ticks,
            wait_object_total_ticks: sched.wait_object_total_ticks,
            wait_object_max_ticks: sched.wait_object_max_ticks,
            wait_object_last_ticks: sched.wait_object_last_ticks,
            wait_queue_total_ticks: wait_summary.total_wait_ticks,
            wait_queue_max_ticks: wait_summary.max_wait_ticks,
            wait_queue_last_ticks: wait_summary.last_wait_ticks,
            driver_work_capacity: work_summary.queue_capacity,
            driver_work_depth: work_summary.queue_depth,
            driver_work_high_water: work_summary.queue_high_water,
            driver_work_worker_started: work_summary.worker_started,
            driver_work_submitted: work_summary.submitted,
            driver_work_submitted_from_irq: work_summary.submitted_from_irq,
            driver_work_submitted_from_task: work_summary.submitted_from_task,
            driver_work_started: work_summary.started,
            driver_work_completed: work_summary.completed,
            driver_work_failed: work_summary.failed,
            driver_work_cancelled: work_summary.cancelled,
            driver_work_dropped: work_summary.dropped,
            driver_work_waits: work_summary.waits,
            driver_work_wait_timeouts: work_summary.wait_timeouts,
            driver_work_wait_denied_irq: work_summary.wait_denied_irq,
            driver_work_wait_total_ticks: work_summary.wait_total_ticks,
            driver_work_wait_max_ticks: work_summary.wait_max_ticks,
            driver_work_wait_last_ticks: work_summary.wait_last_ticks,
            driver_work_queue_total_ticks: work_summary.queue_total_ticks,
            driver_work_queue_max_ticks: work_summary.queue_max_ticks,
            driver_work_queue_last_ticks: work_summary.queue_last_ticks,
            driver_work_run_total_ticks: work_summary.run_total_ticks,
            driver_work_run_max_ticks: work_summary.run_max_ticks,
            driver_work_run_last_ticks: work_summary.run_last_ticks,
            driver_work_releases: work_summary.releases,
            driver_work_invalid_handles: work_summary.invalid_handles,
            driver_work_cleanup_cancelled: work_summary.cleanup_cancelled,
            driver_wait_ticks_calls: work_summary.sleep_waits,
            driver_wait_ticks_denied_irq: work_summary.sleep_denied_irq,
            driver_wait_ticks_total: work_summary.sleep_total_ticks,
            storage_worker_started: block_runtime.worker_started,
            storage_worker_task_id: block_runtime.worker_task_id,
            storage_worker_wakeups: block_runtime.worker_wakeups,
            storage_worker_runs: block_runtime.worker_runs,
            storage_worker_idle_waits: block_runtime.worker_idle_waits,
            storage_worker_queue_scans: block_runtime.worker_queue_scans,
            storage_worker_runtime_requests: block_runtime.worker_runtime_requests,
            storage_worker_runtime_completions: block_runtime.worker_runtime_completions,
            storage_boot_inline_requests: block_runtime.boot_inline_requests,
            storage_boot_inline_completions: block_runtime.boot_inline_completions,
            storage_completion_signals: block_runtime.completion_signals,
            fat32_read_sectors: fat32_summary.read_sectors,
            fat32_write_sectors: fat32_summary.write_sectors,
            fat32_read_failures: fat32_summary.read_failures,
            fat32_write_failures: fat32_summary.write_failures,
            fat32_flushes: fat32_summary.flushes,
            fat32_flush_failures: fat32_summary.flush_failures,
            fat32_flush_total_ticks: fat32_summary.flush_total_ticks,
            fat32_flush_max_ticks: fat32_summary.flush_max_ticks,
            fat32_flush_last_ticks: fat32_summary.flush_last_ticks,
            fat32_file_writes: fat32_summary.file_writes,
            fat32_file_appends: fat32_summary.file_appends,
            fat32_file_write_ranges: fat32_summary.file_write_ranges,
            fat32_file_write_bytes: fat32_summary.file_write_bytes,
            fat32_file_append_bytes: fat32_summary.file_append_bytes,
            fat32_dir_scans: fat32_summary.dir_scans,
            fat32_dir_entries_scanned: fat32_summary.dir_entries_scanned,
            fat32_dir_entry_updates: fat32_summary.dir_entry_updates,
            fat32_cluster_walk_steps: fat32_summary.cluster_walk_steps,
            fat32_fat_reads: fat32_summary.fat_reads,
            fat32_fat_writes: fat32_summary.fat_writes,
            fat32_fat_mirror_writes: fat32_summary.fat_mirror_writes,
            fat32_alloc_chain_calls: fat32_summary.alloc_chain_calls,
            fat32_alloc_clusters: fat32_summary.alloc_clusters,
            fat32_alloc_search_steps: fat32_summary.alloc_search_steps,
            fat32_operation_failures: fat32_summary.operation_failures,
            fat32_operation_total_ticks: fat32_summary.operation_total_ticks,
            fat32_operation_max_ticks: fat32_summary.operation_max_ticks,
            fat32_operation_last_ticks: fat32_summary.operation_last_ticks,
            fat32_active_operation: fat32_summary.active_operation,
            fat32_last_operation: fat32_summary.last_operation,
            fat32_yield_points: fat32_summary.yield_points,
            fat32_yields: fat32_summary.yields,
            fat32_yield_skips: fat32_summary.yield_skips,
            fat32_alloc_runs: fat32_summary.alloc_runs,
            fat32_alloc_run_clusters: fat32_summary.alloc_run_clusters,
            fat32_alloc_run_max_clusters: fat32_summary.alloc_run_max_clusters,
            fat32_fat_sector_writes: fat32_summary.fat_sector_writes,
            fat32_read_extent_cache_hits: fat32_summary.read_extent_cache_hits,
            fat32_read_extent_cache_misses: fat32_summary.read_extent_cache_misses,
            fat32_read_extent_cache_stores: fat32_summary.read_extent_cache_stores,
            fat32_read_extent_cache_clusters: fat32_summary.read_extent_cache_clusters,
            fat32_fsinfo_reads: fat32_summary.fsinfo_reads,
            fat32_fsinfo_valid_mounts: fat32_summary.fsinfo_valid_mounts,
            fat32_fsinfo_rebuilds: fat32_summary.fsinfo_rebuilds,
            fat32_fsinfo_writes: fat32_summary.fsinfo_writes,
            fat32_inusemap_builds: fat32_summary.inusemap_builds,
            fat32_inusemap_clusters: fat32_summary.inusemap_clusters,
            fat32_inusemap_alloc_hits: fat32_summary.inusemap_alloc_hits,
            fat32_inusemap_alloc_misses: fat32_summary.inusemap_alloc_misses,
            monotonic_clock_flags: monotonic_clock.flags,
            monotonic_clock_source: monotonic_clock.source,
            monotonic_clock_generation: monotonic_clock.generation,
            monotonic_event_backend: monotonic_clock.event_backend,
            monotonic_clock_resolution_ns: monotonic_clock.resolution_ns,
            monotonic_source_frequency_hz: monotonic_clock.source_frequency_hz,
            monotonic_event_frequency_numerator: monotonic_clock.event.frequency_numerator,
            monotonic_event_frequency_denominator: monotonic_clock.event.frequency_denominator,
            monotonic_event_requested_hz: monotonic_clock.event.requested_hz,
            monotonic_event_effective_hz: monotonic_clock.event.effective_hz,
            boot_timing_valid: if boot.timing_valid { 1 } else { 0 },
            boot_timing_unavailable_spans: boot.timing_unavailable_spans,
            boot_timing_dropped_spans: boot.timing_dropped_spans,
            loader_timing_valid_spans: loader.timing_valid_spans,
            loader_timing_unavailable_spans: loader.timing_unavailable_spans,
            boot_total_ns: boot.total_ns,
            boot_now_ns: boot.now_ns,
            loader_total_ns: loader.loader_total_ns,
            loader_r4p_runtime_total_ns: loader.r4p_runtime_total_ns,
            loader_service_boot_ns: loader.service_boot_ns,
            loader_config_load_ns: loader.config_load_ns,
            ..Default::default()
        };
        ptr::copy_nonoverlapping(
            (&result as *const ProgramPerformanceSummary).cast::<u8>(),
            out.cast::<u8>(),
            copy_size,
        );
        1
    }
}
const _: () = {
    assert!(performance_snapshot_version == 15);
};
