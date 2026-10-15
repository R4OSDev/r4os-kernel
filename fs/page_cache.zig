// Stateless private C facade. Native Rust owns the one page cache, policy
// worker, metadata, buffers, dirty sets, backend I/O and durability decisions.
const policy = @import("page_cache_policy.zig");
const page_cache_batch = @import("page_cache_batch.zig");
const page_cache_policy = policy;
const diag = @import("../kernel/diag_screen.zig");
const hz: u64 = @import("../kernel/timer.zig").DEFAULT_HZ;
pub const SECTOR_SIZE: usize = 512;
const MAX_ENTRIES: usize = 512;
const MAX_DEVICES: usize = 8;
const PAYLOAD_FRAME_BYTES_U32: u32 = 4096;
const POLICY_VERSION: u32 = 2;
const MAX_DIRTY_AGE_TICKS: u64 = 2 * hz;
const BACKGROUND_PAGE_BUDGET: u32 = 4;
pub const WriteBatch = page_cache_batch.Owner;
pub const NO_WRITE_BATCH: WriteBatch = page_cache_batch.no_owner;

pub const Summary = extern struct {
    enabled: bool = false,
    sector_bytes: u32 = SECTOR_SIZE,
    capacity: u32 = MAX_ENTRIES,
    entries_used: u32 = 0,
    dirty_entries: u32 = 0,
    reads: u64 = 0,
    hits: u64 = 0,
    misses: u64 = 0,
    fills: u64 = 0,
    evictions: u64 = 0,
    invalidations: u64 = 0,
    write_through_requests: u64 = 0,
    write_through_updates: u64 = 0,
    flushes: u64 = 0,
    read_errors: u64 = 0,
    write_errors: u64 = 0,
    writeback_waits: u64 = 0,
    writeback_errors: u64 = 0,
    dirty_bytes: u64 = 0,
    dirty_high_water_entries: u32 = 0,
    writeback_queue_depth: u32 = 0,
    writeback_queue_high_water: u32 = 0,
    deferred_write_requests: u64 = 0,
    dirty_sector_updates: u64 = 0,
    writeback_drains: u64 = 0,
    writeback_sectors: u64 = 0,
    writeback_pressure_drains: u64 = 0,
    writeback_flush_drains: u64 = 0,
    writeback_total_ticks: u64 = 0,
    writeback_max_ticks: u64 = 0,
    writeback_last_ticks: u64 = 0,
    writeback_retries: u64 = 0,
    clean_reclaimable_entries: u32 = 0,
    dirty_non_reclaimable_entries: u32 = 0,
    clean_reclaimable_bytes: u64 = 0,
    dirty_non_reclaimable_bytes: u64 = 0,
    reclaim_scans: u64 = 0,
    reclaim_clean_entries: u64 = 0,
    reclaim_dirty_drains: u64 = 0,
    reclaim_failed_drains: u64 = 0,
    payload_frame_bytes: u32 = PAYLOAD_FRAME_BYTES_U32,
    payload_frames: u32 = 0,
    payload_bytes: u64 = 0,
    pmm_reclaimable_bytes: u64 = 0,
    pmm_dirty_bytes: u64 = 0,
    payload_allocations: u64 = 0,
    payload_allocation_failures: u64 = 0,
    payload_releases: u64 = 0,
    reclaim_returned_frames: u64 = 0,
    reclaim_returned_bytes: u64 = 0,
    lock_timeouts: u64 = 0,
    busy_waits: u64 = 0,
    bulk_write_requests: u64 = 0,
    bulk_write_sectors: u64 = 0,
    selective_flushes: u64 = 0,
    selective_writeback_sectors: u64 = 0,
    selective_foreign_dirty_sectors_skipped: u64 = 0,
    policy_version: u32 = POLICY_VERSION,
    policy_device_capacity: u32 = MAX_DEVICES,
    policy_dirty_high_pages: u32 = page_cache_policy.dirty_high_pages,
    policy_dirty_low_pages: u32 = page_cache_policy.dirty_low_pages,
    policy_max_dirty_age_ticks: u64 = MAX_DIRTY_AGE_TICKS,
    policy_background_page_budget: u32 = BACKGROUND_PAGE_BUDGET,
    policy_worker_started: u32 = 0,
    policy_worker_task_id: u32 = 0,
    policy_worker_wakeups: u64 = 0,
    policy_background_drains: u64 = 0,
    policy_background_sectors: u64 = 0,
    policy_background_pressure_drains: u64 = 0,
    policy_background_age_drains: u64 = 0,
    policy_background_errors: u64 = 0,
    policy_clean_device_probes: u64 = 0,
    policy_dirty_device_probes: u64 = 0,
    policy_full_scan_fallbacks: u64 = 0,
    policy_device_dirty_high_water: u32 = 0,
    read_ahead_requests: u64 = 0,
    read_ahead_issued: u64 = 0,
    read_ahead_hits: u64 = 0,
    read_ahead_cancellations: u64 = 0,
    read_ahead_budget_skips: u64 = 0,
    capacity_min_pages: u32 = page_cache_policy.min_capacity_pages,
    capacity_max_pages: u32 = page_cache_policy.max_capacity_pages,
    capacity_ram_limit_pages: u32 = page_cache_policy.min_capacity_pages,
    capacity_active_limit_pages: u32 = page_cache_policy.min_capacity_pages,
    capacity_pressure_level: u32 = 0,
    read_ahead_window_pages: u32 = 0,
    read_ahead_window_max_pages: u32 = 0,
    capacity_reserved0: u32 = 0,
    fill_run_requests: u64 = 0,
    fill_run_backend_requests: u64 = 0,
    fill_run_pages: u64 = 0,
    fill_run_sectors: u64 = 0,
    fill_run_bytes: u64 = 0,
    fill_run_failures: u64 = 0,
    fill_run_retries: u64 = 0,
    fill_run_max_pages: u64 = 0,
    fill_scatter_copy_bytes: u64 = 0,
    read_staging_copy_bytes: u64 = 0,
    read_caller_copy_bytes: u64 = 0,
    read_publish_lock_drops: u64 = 0,
    fill_lock_drops: u64 = 0,
    capacity_reductions: u64 = 0,
    capacity_trimmed_pages: u64 = 0,
    read_ahead_pages_scheduled: u64 = 0,
    read_ahead_pages_issued: u64 = 0,
    read_ahead_random_resets: u64 = 0,
    writeback_windows: u64 = 0,
    writeback_window_pages: u64 = 0,
    writeback_window_requests: u64 = 0,
    writeback_window_max_sectors: u64 = 0,
    writeback_window_pending_max: u64 = 0,
};

pub const ReclaimResult = extern struct {
    requested_frames: u32 = 0,
    returned_frames: u32 = 0,
    returned_bytes: u64 = 0,
    dirty_drains: u64 = 0,
    failed_drains: u64 = 0,
};
extern fn r4os_rust_pagecache_init() void;
extern fn r4os_rust_pagecache_start() bool;
extern fn r4os_rust_pagecache_batch(*u64) bool;
extern fn r4os_rust_pagecache_summary(*Summary) void;
extern fn r4os_rust_pagecache_read_sector(usize, u64, [*]u8, usize) bool;
extern fn r4os_rust_pagecache_read_sectors(usize, u64, u32, [*]u8, usize) bool;
extern fn r4os_rust_pagecache_write_sector(usize, u64, [*]const u8, usize, u64) bool;
extern fn r4os_rust_pagecache_write_direct(usize, u64, u16, [*]const u8, usize) bool;
extern fn r4os_rust_pagecache_flush_device(usize, u64) bool;
extern fn r4os_rust_pagecache_flush_all() bool;
extern fn r4os_rust_pagecache_flush_range(usize, u64, u64) bool;
extern fn r4os_rust_pagecache_reclaim(u32, bool, *ReclaimResult) void;
extern fn r4os_rust_pagecache_invalidate_sector(usize, u64) void;
extern fn r4os_rust_pagecache_invalidate_device(usize) void;
pub fn init() void {
    r4os_rust_pagecache_init();
}
pub fn startPolicyWorker() bool {
    return r4os_rust_pagecache_start();
}
pub fn beginWriteBatch() ?WriteBatch {
    var out: u64 = undefined;
    return if (r4os_rust_pagecache_batch(&out)) out else null;
}
pub fn summary() Summary {
    var out: Summary = undefined;
    r4os_rust_pagecache_summary(&out);
    return out;
}
pub fn readSector(device_index: usize, lba: u64, out: []u8) bool {
    return r4os_rust_pagecache_read_sector(device_index, lba, out.ptr, out.len);
}
pub fn readSectors(device_index: usize, lba: u64, count: u32, out: []u8) bool {
    return r4os_rust_pagecache_read_sectors(device_index, lba, count, out.ptr, out.len);
}
pub fn writeSector(device_index: usize, lba: u64, data: []const u8) bool {
    return writeSectorInBatch(device_index, lba, data, NO_WRITE_BATCH);
}
pub fn writeSectorInBatch(device_index: usize, lba: u64, data: []const u8, batch: WriteBatch) bool {
    return r4os_rust_pagecache_write_sector(device_index, lba, data.ptr, data.len, batch);
}
pub fn writeSectorsDirect(device_index: usize, lba: u64, sectors: u16, data: []const u8) bool {
    return r4os_rust_pagecache_write_direct(device_index, lba, sectors, data.ptr, data.len);
}
pub fn flushDevice(device_index: usize) bool {
    return r4os_rust_pagecache_flush_device(device_index, 0);
}
pub fn flushDeviceBatch(device_index: usize, batch: WriteBatch) bool {
    return r4os_rust_pagecache_flush_device(device_index, batch);
}
pub fn flushAll() bool {
    return r4os_rust_pagecache_flush_all();
}
pub fn flushInvalidateRange(device_index: usize, first: u64, count: u64) bool {
    return r4os_rust_pagecache_flush_range(device_index, first, count);
}
pub fn reclaimPayloadFrames(target_frames: u32, allow_dirty_drain: bool) ReclaimResult {
    var out: ReclaimResult = undefined;
    r4os_rust_pagecache_reclaim(target_frames, allow_dirty_drain, &out);
    return out;
}
pub fn invalidateSector(device_index: usize, lba: u64) void {
    r4os_rust_pagecache_invalidate_sector(device_index, lba);
}
pub fn invalidateDevice(device_index: usize) void {
    r4os_rust_pagecache_invalidate_device(device_index);
}
// Only rendering remains with the existing diagnostic-screen owner. Tokens
// retain its exact generation; this leaf owns no cache decisions or state.
export fn r4os_zig_pgcache_diagnostic(op: u8, token: u64, value: u64, extra: u64) u64 {
    switch (op) {
        0 => return diag.beginResolvableIncident().generation,
        1 => return @intFromBool(diag.resolveIncident(.{ .generation = token })),
        2 => {
            diag.write("[PGCACHE] relock stalled slice=");
            diag.writeDec(value);
            diag.endLine();
            return 0;
        },
        3 => {
            const incident = diag.beginResolvableIncident();
            diag.write("[PGCACHE] dirty index fallback dev=");
            if (value == @import("std").math.maxInt(u64)) diag.write("all") else diag.writeDec(value);
            diag.write(" observed=");
            diag.writeDec(extra);
            diag.endLine();
            _ = diag.resolveIncident(incident);
            return 0;
        },
        4, 5 => {
            const incident = diag.beginResolvableIncident();
            diag.write(if (op == 4) "[PGCACHE] busy read failed dev=" else "[PGCACHE] busy bulk read failed dev=");
            diag.writeDec(value);
            diag.write(" page=");
            diag.writeDec(extra);
            diag.endLine();
            _ = diag.resolveIncident(incident);
            return 0;
        },
        else => unreachable,
    }
}
comptime {
    if (@sizeOf(Summary) != 792 or @alignOf(Summary) != 8 or @sizeOf(ReclaimResult) != 32)
        @compileError("Native page-cache private C layout changed");
}

// Native block I/O still uses these exact C execution/diagnostic leaves.
// Register their real exports also when a host root reaches only the cache.
comptime {
    _ = @import("../storage/block.zig");
}
