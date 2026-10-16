//! Passive original block-device performance projection, including owned
//! fixed labels. The storage owner releases before the caller output write.
use crate::{contract as c, storage::block_diagnostics};
/// # Safety
/// Exclusive aligned canonical result retained for this bounded observation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_storage(
    index: u32,
    out: *mut c::ProgramStoragePerformanceInfo,
) -> i32 {
    unsafe {
        let Some(device) = block_diagnostics::device_snapshot(index as usize) else {
            out.write(Default::default());
            return 0;
        };
        let storage_stats = device.stats;
        let mut value = c::ProgramStoragePerformanceInfo {
            index,
            bus: u32::from(device.bus),
            state: u32::from(device.state),
            source: u32::from(device.source),
            flags: u32::from(device.removable) | (u32::from(device.writable) << 1),
            port: device.port,
            sector_size: device.sector_size,
            max_sectors_per_request: device.max_sectors,
            queue_depth: device.queue_depth,
            queue_used: device.queue_used,
            queue_high_water: storage_stats.queue_high_water,
            timeout_ticks: device.timeout_ticks,
            sector_count: device.sector_count,
            active_request_id: storage_stats.active_request.id,
            active_request_kind: u32::from(storage_stats.active_request.kind),
            active_request_sectors: u32::from(storage_stats.active_request.sectors),
            active_request_lba: storage_stats.active_request.lba,
            last_request_id: storage_stats.last_request.id,
            last_request_kind: u32::from(storage_stats.last_request.kind),
            last_request_sectors: u32::from(storage_stats.last_request.sectors),
            last_request_lba: storage_stats.last_request.lba,
            read_ops: storage_stats.read_ops,
            read_sectors: storage_stats.read_sectors,
            read_failures: storage_stats.read_failures,
            write_ops: storage_stats.write_ops,
            write_sectors: storage_stats.write_sectors,
            write_failures: storage_stats.write_failures,
            flush_ops: storage_stats.flush_ops,
            flush_failures: storage_stats.flush_failures,
            busy_rejections: storage_stats.busy_rejections,
            timeout_failures: storage_stats.timeout_failures,
            completions: storage_stats.completions,
            backend_recoveries: storage_stats.backend_recoveries,
            backend_recovery_failures: storage_stats.backend_recovery_failures,
            queued_requests: storage_stats.queued_requests,
            dequeued_requests: storage_stats.dequeued_requests,
            queue_full_waits: storage_stats.queue_full_waits,
            queue_full_rejections: storage_stats.queue_full_rejections,
            completion_waits: storage_stats.completion_waits,
            completion_timeouts: storage_stats.completion_timeouts,
            completion_total_ticks: storage_stats.completion_total_ticks,
            completion_max_ticks: storage_stats.completion_max_ticks,
            completion_last_ticks: storage_stats.completion_last_ticks,
            last_error: u32::from(storage_stats.last_error),
            last_sense_valid: u32::from(storage_stats.last_sense.valid),
            last_sense: if storage_stats.last_sense.valid {
                u32::from(storage_stats.last_sense.opcode) << 24
                    | u32::from(storage_stats.last_sense.key) << 16
                    | u32::from(storage_stats.last_sense.asc) << 8
                    | u32::from(storage_stats.last_sense.ascq)
            } else {
                0
            },
            completion_signals: storage_stats.completion_signals,
            worker_requests: storage_stats.worker_requests,
            worker_completions: storage_stats.worker_completions,
            boot_inline_requests: storage_stats.boot_inline_requests,
            boot_inline_completions: storage_stats.boot_inline_completions,
            ..Default::default()
        };
        value.name = device.name;
        value.driver = device.driver;
        value.controller = device.controller;
        out.write(value);
        1
    }
}
