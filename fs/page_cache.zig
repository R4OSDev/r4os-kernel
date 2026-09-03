// Page-Cache v2 (0.56.8): page-organisiert + Hash-Index + Cache-Lock.
//
// Ersetzt den Sektor-Cache v1 (64 Eintraege, je EIN 512-B-Sektor in einem
// vollen 4-KB-Frame = 32 KB nutzbar bei 256 KB Frame-Verbrauch, alle
// Zugriffe lineare Scans, KEIN Lock trotz yieldender block.read/write).
//
//   - Cache-Einheit ist die 4-KB-Seite (8 Sektoren, page_lba = lba & ~7).
//     Ein Miss fuellt die ganze Seite mit EINEM block.read(8) und macht
//     die 7 Nachbarsektoren zu kuenftigen Hits.
//   - 512 Eintraege = 2 MB nutzbare Cache-Kapazitaet; Frames kommen wie
//     bisher bedarfsweise aus dem PMM und sind unter Druck reklamierbar.
//   - Hash-Index (1024 Buckets, Ketten ueber next-Indizes) statt
//     Linearsuche; LRU-Scan nur noch bei Eviction.
//   - valid_mask/dirty_mask pro Sektor: Write-Misses brauchen KEIN
//     Read-Modify-Write; ein spaeterer Read merged den Seiten-Fill,
//     ohne gueltige/dirty Sektoren zu ueberschreiben.
//   - Cache-Lock (sync.Mutex, Rank fs_page_cache) schuetzt NUR die
//     Metadaten. Block-I/O laeuft IMMER ohne gehaltenen Lock
//     (MEMSUITE-STRICT-Kriterium sleep_under_lock==0; block.zig gibt
//     seinen Queue-Lock vor dem Warten ebenfalls frei). Waehrend eines
//     entsperrten Fill/Writeback pinnt io_busy den Eintrag: Identitaet
//     und Frame sind stabil, konkurrierende Zugriffe auf DIESE Seite
//     warten kurz, Eviction/Reclaim ueberspringen gepinnte Eintraege.
//     Im fruehen Boot (kein Task-Kontext) laeuft alles single-threaded
//     ohne Lock.
//
// API unveraendert gegenueber v1.

const block = @import("../storage/block.zig");
const diag_screen = @import("../kernel/diag_screen.zig");
const timer = @import("../kernel/timer.zig");
const heap = @import("../memory/heap.zig");
const mem_phys = @import("../memory/phys.zig");
const page_cache_batch = @import("page_cache_batch.zig");
const page_cache_policy = @import("page_cache_policy.zig");
const sync = @import("../sched/sync.zig");
const scheduler = @import("../sched/scheduler.zig");
const sched_task = @import("../sched/task.zig");
const std = @import("std");
const task_context = @import("../sched/task_context.zig");

pub const SECTOR_SIZE: usize = 512;
const PAGE_SECTORS: usize = 8;
const PAGE_BYTES: usize = PAGE_SECTORS * SECTOR_SIZE;
const MAX_ENTRIES: usize = 512;
const MAX_DEVICES: usize = page_cache_policy.max_devices;
const BUCKET_COUNT: usize = 1024;
const MAX_WRITEBACK_RETRIES: usize = 1;
const PAYLOAD_FRAME_BYTES: usize = 4096;
const PAYLOAD_FRAME_BYTES_U32: u32 = 4096;
const PAYLOAD_FRAME_BYTES_U64: u64 = 4096;
const NO_INDEX: u16 = 0xFFFF;
const FULL_MASK: u8 = 0xFF;
// Praktisch "fuer immer": klemmt das Lock, schlaegt die Operation
// kontrolliert fehl statt still ohne Lock zu laufen.
// 0.56.40: hz-neutral (3600 s Wachhund; bei 100 Hz wie zuvor 360000).
const LOCK_TIMEOUT_TICKS: u64 = 3600 * @as(u64, timer.DEFAULT_HZ);
const BUSY_WAIT_LIMIT_TICKS: usize = 5 * @as(usize, timer.DEFAULT_HZ);
const POLICY_VERSION: u32 = 2;
const BACKGROUND_INTERVAL_TICKS: u64 = @max(1, @as(u64, timer.DEFAULT_HZ) / 4);
const BACKGROUND_REQUEUE_TICKS: u64 = @max(1, @as(u64, timer.DEFAULT_HZ) / 100);
const BACKGROUND_FAILURE_BACKOFF_TICKS: u64 = @as(u64, timer.DEFAULT_HZ);
const MAX_DIRTY_AGE_TICKS: u64 = 2 * @as(u64, timer.DEFAULT_HZ);
const BACKGROUND_PAGE_BUDGET: usize = 4;
// 0.75.6 deliberately limits demand coalescing to the two-page shape covered
// by the filesystem diagnostic. Larger runs interacted intermittently with
// the long subsystem/AUTOEXEC workload and are not shipped without proof.
const MAX_CONTIGUOUS_FILL_PAGES: u16 = 2;
// 0.75.6: Every tested speculative variant (adaptive multi-page down to the
// former single-page request) reproducibly stalled the subsystem/AUTOEXEC
// workload once contiguous demand fills were enabled. Keep speculation
// explicitly off; the safe demand coalescing remains independently active.
const SPECULATIVE_READ_AHEAD_ENABLED: bool = false;
const READ_AHEAD_TRIGGER_SECTORS: u32 = PAGE_SECTORS * 2;
const READ_AHEAD_RESIDENT_LIMIT: u16 = 2;
const READ_AHEAD_FREE_FLOOR_MAX: u16 = 32;
const READ_AHEAD_FREE_FLOOR_MIN: u16 = 8;

comptime {
    if (MAX_ENTRIES != page_cache_policy.max_entries) @compileError("page-cache policy entry capacity mismatch");
}

pub const Summary = struct {
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
};

/// Identifies the dirty sectors produced by one filesystem mutation.  Zero is
/// deliberately reserved for legacy/unscoped writes, so a selective commit
/// can never mistake older dirty state for work owned by the caller.
pub const WriteBatch = page_cache_batch.Owner;
pub const NO_WRITE_BATCH: WriteBatch = page_cache_batch.no_owner;

pub const ReclaimResult = struct {
    requested_frames: u32 = 0,
    returned_frames: u32 = 0,
    returned_bytes: u64 = 0,
    dirty_drains: u64 = 0,
    failed_drains: u64 = 0,
};

const Entry = struct {
    valid: bool = false,
    // io_busy pinnt den Eintrag waehrend eines entsperrten Fill/Writeback:
    // Identitaet (device/page_lba) und Frame sind dann stabil.
    io_busy: bool = false,
    device_index: usize = 0,
    page_lba: u64 = 0,
    valid_mask: u8 = 0,
    dirty_mask: u8 = 0,
    last_use: u64 = 0,
    dirty_sequence: u64 = 0,
    dirty_since_tick: u64 = 0,
    dirty_owner: [PAGE_SECTORS]WriteBatch = .{NO_WRITE_BATCH} ** PAGE_SECTORS,
    dirty_write_sequence: [PAGE_SECTORS]u64 = .{0} ** PAGE_SECTORS,
    read_ahead: bool = false,
    phys_addr: u64 = 0,
    next: u16 = NO_INDEX,
};

const BackgroundReason = enum {
    pressure,
    age,
};

const BackgroundSelection = struct {
    index: usize,
    device_index: usize,
    reason: BackgroundReason,
};

var entries: [MAX_ENTRIES]Entry = .{Entry{}} ** MAX_ENTRIES;
var buckets: [BUCKET_COUNT]u16 = .{NO_INDEX} ** BUCKET_COUNT;
var stats: Summary = .{};
var clock: u64 = 0;
var dirty_clock: u64 = 0;
var dirty_write_clock: u64 = 0;
var write_batch_clock: WriteBatch = NO_WRITE_BATCH;
var dirty_entry_count: u32 = 0;
var dirty_sector_count: u64 = 0;
var dirty_sector_count_by_device: [MAX_DEVICES]u64 = .{0} ** MAX_DEVICES;
var payload_frame_count: u32 = 0;
var policy_index: page_cache_policy.Index = page_cache_policy.Index.init();
var policy_event: sync.EventV2 = sync.EventV2.initMode(false, .auto_reset);
var policy_worker_started = false;
var policy_worker_task_id: u32 = 0;
var background_failure_until: [MAX_DEVICES]u64 = .{0} ** MAX_DEVICES;
var read_ahead_states: [MAX_DEVICES]page_cache_policy.ReadAhead =
    .{page_cache_policy.ReadAhead{}} ** MAX_DEVICES;
var read_ahead_device_cursor: u8 = 0;
var capacity_reference_frames: u64 = 0;
var capacity_state: page_cache_policy.Capacity = .{
    .ram_pages = page_cache_policy.min_capacity_pages,
    .active_pages = page_cache_policy.min_capacity_pages,
    .pressure_level = 0,
    .read_ahead_pages = 1,
};
var cache_lock: sync.Mutex = .{};

pub fn init() void {
    // Laeuft in der single-threaded Boot-Phase; kein Lock noetig/moeglich.
    releaseAllPayloads(false);
    entries = .{Entry{}} ** MAX_ENTRIES;
    buckets = .{NO_INDEX} ** BUCKET_COUNT;
    stats = .{
        .enabled = true,
        .sector_bytes = SECTOR_SIZE,
        .capacity = MAX_ENTRIES,
        .payload_frame_bytes = PAYLOAD_FRAME_BYTES_U32,
        .policy_version = POLICY_VERSION,
        .policy_device_capacity = MAX_DEVICES,
        .policy_dirty_high_pages = page_cache_policy.dirty_high_pages,
        .policy_dirty_low_pages = page_cache_policy.dirty_low_pages,
        .policy_max_dirty_age_ticks = MAX_DIRTY_AGE_TICKS,
        .policy_background_page_budget = BACKGROUND_PAGE_BUDGET,
    };
    clock = 0;
    dirty_clock = 0;
    dirty_write_clock = 0;
    write_batch_clock = NO_WRITE_BATCH;
    dirty_entry_count = 0;
    dirty_sector_count = 0;
    dirty_sector_count_by_device = .{0} ** MAX_DEVICES;
    payload_frame_count = 0;
    policy_index = page_cache_policy.Index.init();
    policy_event = sync.EventV2.initMode(false, .auto_reset);
    policy_worker_started = false;
    policy_worker_task_id = 0;
    background_failure_until = .{0} ** MAX_DEVICES;
    read_ahead_states = .{page_cache_policy.ReadAhead{}} ** MAX_DEVICES;
    read_ahead_device_cursor = 0;
    capacity_reference_frames = mem_phys.stats().free_frames;
    capacity_state = page_cache_policy.capacityForMemory(
        capacity_reference_frames,
        capacity_reference_frames,
    );
    stats.capacity_ram_limit_pages = capacity_state.ram_pages;
    stats.capacity_active_limit_pages = capacity_state.active_pages;
    stats.capacity_pressure_level = capacity_state.pressure_level;
    stats.read_ahead_window_max_pages = if (SPECULATIVE_READ_AHEAD_ENABLED) capacity_state.read_ahead_pages else 0;
    cache_lock = sync.Mutex.initClass("fs-page-cache", sync.LockRank.fs_page_cache, .sleepable);
}

pub fn startPolicyWorker() bool {
    if (policy_worker_started) return true;
    const worker = sched_task.createKernelThreadWithRole("pgcache-wb", policyWorkerMain, .batch) orelse return false;
    policy_worker_started = true;
    policy_worker_task_id = worker.id;
    stats.policy_worker_started = 1;
    stats.policy_worker_task_id = worker.id;
    policy_event.signal();
    return true;
}

/// Starts an operation-scoped dirty set. The token is intentionally explicit:
/// filesystem code must pass it into every cached write that belongs to the
/// mutation and later into flushDeviceBatch().
pub fn beginWriteBatch() ?WriteBatch {
    const guard = acquireLock() orelse return null;
    defer releaseLock(guard);
    write_batch_clock +%= 1;
    if (write_batch_clock == NO_WRITE_BATCH) write_batch_clock = 1;
    return write_batch_clock;
}

pub fn summary() Summary {
    // A runtime lock timeout is not the same as the single-threaded boot
    // bypass.  Never scan mutable entries without the lock merely to produce
    // diagnostics about the lock being stuck.
    const guard = acquireLock() orelse return stats;
    defer releaseLock(guard);
    var out = stats;
    const used: u32 = policy_index.entryCount();
    const dirty_pages: u32 = dirty_entry_count;
    const dirty_sectors = dirty_sector_count;
    const clean_pages: u32 = used - dirty_pages;
    const frames = payload_frame_count;
    out.enabled = true;
    out.sector_bytes = SECTOR_SIZE;
    out.capacity = MAX_ENTRIES;
    out.entries_used = used;
    out.dirty_entries = dirty_pages;
    out.dirty_bytes = dirty_sectors * SECTOR_SIZE;
    out.writeback_queue_depth = dirty_pages;
    out.clean_reclaimable_entries = clean_pages;
    out.dirty_non_reclaimable_entries = dirty_pages;
    out.clean_reclaimable_bytes = @as(u64, clean_pages) * PAYLOAD_FRAME_BYTES_U64;
    out.dirty_non_reclaimable_bytes = @as(u64, dirty_pages) * PAYLOAD_FRAME_BYTES_U64;
    out.payload_frame_bytes = PAYLOAD_FRAME_BYTES_U32;
    out.payload_frames = frames;
    out.payload_bytes = @as(u64, frames) * PAYLOAD_FRAME_BYTES_U64;
    out.pmm_reclaimable_bytes = @as(u64, clean_pages) * PAYLOAD_FRAME_BYTES_U64;
    out.pmm_dirty_bytes = @as(u64, dirty_pages) * PAYLOAD_FRAME_BYTES_U64;
    out.policy_worker_started = if (policy_worker_started) 1 else 0;
    out.policy_worker_task_id = policy_worker_task_id;
    out.policy_clean_device_probes = policy_index.clean_device_probes;
    out.policy_dirty_device_probes = policy_index.dirty_device_probes;
    const capacity = refreshCapacityLocked();
    out.capacity_ram_limit_pages = capacity.ram_pages;
    out.capacity_active_limit_pages = capacity.active_pages;
    out.capacity_pressure_level = capacity.pressure_level;
    out.read_ahead_window_max_pages = if (SPECULATIVE_READ_AHEAD_ENABLED) capacity.read_ahead_pages else 0;
    var current_window: u16 = 0;
    if (SPECULATIVE_READ_AHEAD_ENABLED) {
        for (read_ahead_states) |state| {
            if (state.sequential.window_pages > current_window) current_window = state.sequential.window_pages;
        }
    }
    out.read_ahead_window_pages = current_window;
    var device_high_water: u16 = 0;
    for (policy_index.devices) |device| {
        if (device.dirty_high_water > device_high_water) device_high_water = device.dirty_high_water;
    }
    out.policy_device_dirty_high_water = device_high_water;
    return out;
}

pub fn readSector(device_index: usize, lba: u64, out: []u8) bool {
    if (out.len < SECTOR_SIZE) {
        stats.read_errors +%= 1;
        return false;
    }
    var caller_copy: [SECTOR_SIZE]u8 = undefined;
    const guard = acquireLock() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    defer _ = task_context.leaveUnwind(unwind);

    stats.reads +%= 1;
    const bit = sectorBit(lba);
    const target_page = pageLba(lba);
    noteDemandStartLocked(device_index, target_page);
    var counted_miss = false;
    var busy_guard: usize = 0;
    while (true) {
        const index = findEntry(device_index, target_page) orelse
            createEntry(device_index, target_page) orelse {
            if (!counted_miss) {
                stats.misses +%= 1;
                counted_miss = true;
            }
            // A no-slot read must not bypass a saturated write-back cache.
            // NTFS lookups span multiple metadata pages; reading one page
            // directly from disk while related pages are still dirty can
            // expose an on-disk namespace state that never existed as one
            // coherent cache view. Drain one page, reserve the requested
            // identity and then use the normal fill path.
            if (findOldestDirty() != null) {
                if (!writebackOldestDirty(guard, .pressure)) {
                    stats.read_errors +%= 1;
                    return false;
                }
                busy_guard = 0;
                continue;
            }
            if (hasBusyEntry()) {
                busy_guard += 1;
                if (busy_guard > BUSY_WAIT_LIMIT_TICKS or !waitBusy(guard)) {
                    stats.read_errors +%= 1;
                    return false;
                }
                continue;
            }
            stats.read_errors +%= 1;
            return false;
        };
        if (entries[index].io_busy) {
            // Never bypass an existing busy cache entry.  It may contain
            // dirty sectors or still be owned by a live fill/writeback; an
            // uncached read would break coherence and can later be
            // overwritten by stale writeback.
            busy_guard += 1;
            if (busy_guard > BUSY_WAIT_LIMIT_TICKS) {
                stats.read_errors +%= 1;
                const incident_token = diag_screen.beginResolvableIncident();
                diag_screen.write("[PGCACHE] busy read failed dev=");
                diag_screen.writeDec(device_index);
                diag_screen.write(" page=");
                diag_screen.writeDec(target_page);
                diag_screen.endLine();
                // The failed read is a terminal outcome, not a permanent
                // owner. Keep the captured pixels/evidence, but let a later
                // independent root cause start its own generation.
                _ = diag_screen.resolveIncident(incident_token);
                return false;
            }
            if (!waitBusy(guard)) {
                stats.read_errors +%= 1;
                return false;
            }
            continue;
        }
        if ((entries[index].valid_mask & bit) != 0) {
            const frame = payloadFrame(index) orelse {
                stats.read_errors +%= 1;
                return false;
            };
            const off = sectorOffset(lba);
            entries[index].last_use = nextClock();
            noteDemandHitLocked(index);
            // Snapshot into resident kernel-stack memory while metadata is
            // locked, then drop every cache owner before touching a pageable
            // caller buffer. preTouch was not a pin: the page could be
            // evicted during the preceding cache/backend wait and fault back
            // into this same io_busy entry.
            @memcpy(caller_copy[0..], frame[off .. off + SECTOR_SIZE]);
            stats.read_staging_copy_bytes +%= SECTOR_SIZE;
            stats.read_caller_copy_bytes +%= SECTOR_SIZE;
            stats.read_publish_lock_drops +%= 1;
            if (counted_miss) {
                // Miss + erfolgreicher Fill: bleibt ein Miss.
            } else {
                stats.hits +%= 1;
            }
            releaseLock(guard);
            locked = false;
            @memcpy(out[0..SECTOR_SIZE], caller_copy[0..]);
            return true;
        }
        if (!counted_miss) {
            stats.misses +%= 1;
            counted_miss = true;
        }
        if (!fillEntryWithBackendPolicy(guard, index)) {
            // Never turn a failed fill into an unrelated uncached read.
            // fillEntryWithBackendPolicy already applied the backend-specific
            // rule: no extra USBMSC attempt, one merge-safe retry elsewhere.
            // Partial final device pages are handled by the fill itself.
            if (entries[index].valid and entries[index].valid_mask == 0 and !entries[index].io_busy) {
                clearEntry(index, false);
            }
            stats.read_errors +%= 1;
            return false;
        }
        // Fill erfolgreich -> naechste Runde nimmt den Hit-Pfad (ohne
        // hits-Zaehlung, siehe counted_miss).
    }
}

// 0.56.10: Bulk-Lesen ueber Seitengrenzen - ein Lock-/Hash-Zugriff und
// EIN memcpy pro Seite statt pro Sektor. Zaehler bleiben sektorbasiert
// (kompatibel zur bisherigen Semantik: 1 Sektor = 1 read/hit/miss).
pub fn readSectors(device_index: usize, lba: u64, count: u32, out: []u8) bool {
    if (count == 0) return true;
    const total_bytes = @as(usize, count) * SECTOR_SIZE;
    if (out.len < total_bytes) {
        stats.read_errors +%= 1;
        return false;
    }
    const end_lba = lba +% @as(u64, count);
    if (end_lba <= lba) {
        stats.read_errors +%= 1;
        return false;
    }
    const final_page = pageLba(end_lba - 1);
    var caller_copy: [PAGE_BYTES]u8 = undefined;
    // Allocated only after the first multi-page miss is observed. The defer
    // is registered before the cache-lock defer, so resident staging is
    // always freed after metadata ownership has been released.
    var fill_buffer: ?[]u8 = null;
    defer {
        if (fill_buffer) |memory| _ = heap.free(memory);
    }
    var fill_buffer_unavailable = false;
    const guard = acquireLock() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    defer _ = task_context.leaveUnwind(unwind);

    noteDemandStartLocked(device_index, pageLba(lba));
    var done: u32 = 0;
    while (done < count) {
        const cur = lba + done;
        const page = pageLba(cur);
        const first_in_page: usize = @intCast(cur - page);
        const span: usize = @min(@as(usize, count - done), PAGE_SECTORS - first_in_page);
        stats.reads +%= span;

        var served = false;
        var busy_conflict = false;
        var cache_failure = false;
        var busy_guard: usize = 0;
        var miss_counted = false;
        while (!served) {
            const index = findEntry(device_index, page) orelse
                createEntry(device_index, page) orelse {
                // Keep bulk reads coherent with dirty filesystem metadata.
                // This is the same reservation rule as readSector/writeSector:
                // pressure drains one page; a busy-only cache waits for its
                // owner; no successful read bypasses cache identity.
                if (findOldestDirty() != null) {
                    if (!writebackOldestDirty(guard, .pressure)) {
                        cache_failure = true;
                        break;
                    }
                    busy_guard = 0;
                    continue;
                }
                if (hasBusyEntry()) {
                    busy_guard += 1;
                    if (busy_guard > BUSY_WAIT_LIMIT_TICKS or !waitBusy(guard)) {
                        busy_conflict = true;
                        break;
                    }
                    continue;
                }
                cache_failure = true;
                break;
            };
            if (entries[index].io_busy) {
                busy_guard += 1;
                if (busy_guard > BUSY_WAIT_LIMIT_TICKS) {
                    busy_conflict = true;
                    const incident_token = diag_screen.beginResolvableIncident();
                    diag_screen.write("[PGCACHE] busy bulk read failed dev=");
                    diag_screen.writeDec(device_index);
                    diag_screen.write(" page=");
                    diag_screen.writeDec(page);
                    diag_screen.endLine();
                    _ = diag_screen.resolveIncident(incident_token);
                    break;
                }
                if (!waitBusy(guard)) {
                    busy_conflict = true;
                    break;
                }
                continue;
            }
            if (!maskCovers(entries[index].valid_mask, first_in_page, span)) {
                if (!miss_counted) {
                    stats.misses +%= span;
                    miss_counted = true;
                }
                const requested_pages_u64 = (final_page - page) / PAGE_SECTORS + 1;
                const requested_pages: u16 = @intCast(@min(
                    @as(u64, MAX_ENTRIES),
                    requested_pages_u64,
                ));
                const run_pages = fillRunPageLimitLocked(device_index, requested_pages);
                if (run_pages >= 2 and fill_buffer == null and !fill_buffer_unavailable) {
                    // Heap growth can commit/reclaim and must never run under
                    // cache_lock. No identity is pinned yet, so re-evaluate
                    // the page after reacquiring the lock.
                    releaseLock(guard);
                    locked = false;
                    fill_buffer = heap.alloc(@as(usize, run_pages) * PAGE_BYTES, 16);
                    if (fill_buffer == null) fill_buffer_unavailable = true;
                    relock(guard);
                    locked = true;
                    continue;
                }
                const fill_ok = if (run_pages >= 2 and fill_buffer != null)
                    fillContiguousRunLocked(
                        guard,
                        device_index,
                        page,
                        run_pages,
                        fill_buffer.?,
                        false,
                    ).ok
                else
                    fillEntryWithBackendPolicy(guard, index);
                if (!fill_ok) {
                    cache_failure = true;
                    break;
                }
                // nach Fill: naechste Runde prueft erneut (Eintrag stabil,
                // Lock seit relock gehalten)
                if (!maskCovers(entries[index].valid_mask, first_in_page, span)) {
                    cache_failure = true;
                    break;
                }
                const frame_f = payloadFrame(index) orelse {
                    cache_failure = true;
                    break;
                };
                const off_f = first_in_page * SECTOR_SIZE;
                const copy_bytes = span * SECTOR_SIZE;
                @memcpy(caller_copy[0..copy_bytes], frame_f[off_f .. off_f + copy_bytes]);
                stats.read_staging_copy_bytes +%= copy_bytes;
                stats.read_caller_copy_bytes +%= copy_bytes;
                stats.read_publish_lock_drops +%= 1;
                entries[index].last_use = nextClock();
                noteDemandHitLocked(index);
                releaseLock(guard);
                locked = false;
                @memcpy(out[@as(usize, done) * SECTOR_SIZE ..][0..copy_bytes], caller_copy[0..copy_bytes]);
                relock(guard);
                locked = true;
                served = true;
                break;
            }
            const frame = payloadFrame(index) orelse {
                cache_failure = true;
                break;
            };
            const off = first_in_page * SECTOR_SIZE;
            const copy_bytes = span * SECTOR_SIZE;
            @memcpy(caller_copy[0..copy_bytes], frame[off .. off + copy_bytes]);
            stats.read_staging_copy_bytes +%= copy_bytes;
            stats.read_caller_copy_bytes +%= copy_bytes;
            stats.read_publish_lock_drops +%= 1;
            entries[index].last_use = nextClock();
            noteDemandHitLocked(index);
            if (!miss_counted) stats.hits +%= span;
            releaseLock(guard);
            locked = false;
            @memcpy(out[@as(usize, done) * SECTOR_SIZE ..][0..copy_bytes], caller_copy[0..copy_bytes]);
            relock(guard);
            locked = true;
            served = true;
        }
        if (busy_conflict or cache_failure) {
            stats.read_errors +%= 1;
            return false;
        }
        if (!served) {
            stats.read_errors +%= 1;
            return false;
        }
        done += @intCast(span);
    }
    scheduleReadAheadLocked(device_index, lba, count);
    return true;
}

fn maskCovers(mask: u8, first: usize, len: usize) bool {
    var i: usize = 0;
    while (i < len) : (i += 1) {
        if ((mask >> @as(u3, @intCast(first + i))) & 1 == 0) return false;
    }
    return true;
}

pub fn writeSector(device_index: usize, lba: u64, data: []const u8) bool {
    return writeSectorInBatch(device_index, lba, data, NO_WRITE_BATCH);
}

pub fn writeSectorInBatch(device_index: usize, lba: u64, data: []const u8, batch: WriteBatch) bool {
    if (data.len < SECTOR_SIZE) {
        stats.write_errors +%= 1;
        return false;
    }
    // Copy from the pageable caller before taking cache ownership. A fault
    // here may re-enter storage, but no cache lock or io_busy pin exists yet.
    var caller_copy: [SECTOR_SIZE]u8 = undefined;
    @memcpy(caller_copy[0..], data[0..SECTOR_SIZE]);
    const guard = acquireLock() orelse {
        stats.write_errors +%= 1;
        return false;
    };
    defer releaseLock(guard);
    const unwind = enterOperation() orelse {
        stats.write_errors +%= 1;
        return false;
    };
    defer _ = task_context.leaveUnwind(unwind);

    const bit = sectorBit(lba);
    const target_page = pageLba(lba);
    var busy_guard: usize = 0;
    while (true) {
        const index = findEntry(device_index, target_page) orelse
            createEntry(device_index, target_page) orelse {
            // A direct write without a cache reservation is incoherent:
            // while block I/O runs without cache_lock, a reclaimed slot
            // can fill the old sector and retain it after the write. This
            // happened once the 512-page cache was saturated by a large
            // deferred update stream. Drain one dirty page and retry so
            // createEntry can reserve the target identity before data is
            // accepted. If every remaining slot is merely busy, wait for
            // its owner; never report an unreserved backend write as a
            // successful cache update.
            if (findOldestDirty() != null) {
                if (!writebackOldestDirty(guard, .pressure)) {
                    stats.write_errors +%= 1;
                    return false;
                }
                busy_guard = 0;
                continue;
            }
            if (hasBusyEntry()) {
                busy_guard += 1;
                if (busy_guard > BUSY_WAIT_LIMIT_TICKS or !waitBusy(guard)) {
                    stats.write_errors +%= 1;
                    return false;
                }
                continue;
            }
            stats.write_errors +%= 1;
            return false;
        };
        if (entries[index].io_busy) {
            if (!waitBusy(guard)) {
                stats.write_errors +%= 1;
                return false;
            }
            continue;
        }
        const frame = payloadFrame(index) orelse {
            stats.write_errors +%= 1;
            return false;
        };
        const off = sectorOffset(lba);
        dropReadAheadResidencyLocked(index);
        if (!markDirty(index, bit, batch)) {
            stats.write_errors +%= 1;
            return false;
        }
        @memcpy(frame[off .. off + SECTOR_SIZE], caller_copy[0..]);
        entries[index].valid_mask |= bit;
        entries[index].last_use = nextClock();
        stats.dirty_sector_updates +%= 1;
        stats.deferred_write_requests +%= 1;
        return true;
    }
}

pub fn writeSectorsDirect(device_index: usize, lba: u64, sectors: u16, data: []const u8) bool {
    if (sectors == 0) {
        stats.write_errors +%= 1;
        return false;
    }
    const sector_count: usize = @intCast(sectors);
    const byte_count = sector_count * SECTOR_SIZE;
    if (data.len < byte_count) {
        stats.write_errors +%= 1;
        return false;
    }
    if (sector_count > 1) {
        stats.bulk_write_requests +%= 1;
        stats.bulk_write_sectors +%= @intCast(sector_count);
    }
    // preTouch is not residency ownership. Stage the complete pageable
    // caller range into kernel heap before reserving cache pages; otherwise
    // block.write's bounce copy could fault while those same pages are
    // io_busy and recursively wait on this operation.
    const staging_unwind = enterOperation() orelse {
        stats.write_errors +%= 1;
        return false;
    };
    defer _ = task_context.leaveUnwind(staging_unwind);
    // This path is also reached from the comparatively deep R4X async-I/O
    // call chain. A 4-KB stack fallback crossed that task's guard page as
    // soon as NTFS started preserving multi-sector requests. Kernel-heap
    // staging is resident and keeps the task stack bounded for every size.
    const staged_data = heap.alloc(byte_count, 16) orelse {
        stats.write_errors +%= 1;
        return false;
    };
    defer _ = heap.free(staged_data);
    @memcpy(staged_data[0..byte_count], data[0..byte_count]);
    const guard = acquireLock() orelse {
        stats.write_errors +%= 1;
        return false;
    };
    var locked = true;
    defer if (locked) releaseLock(guard);
    var owned_entries: [MAX_ENTRIES / 64]u64 = .{0} ** (MAX_ENTRIES / 64);
    var reserved_entries: [MAX_ENTRIES / 64]u64 = .{0} ** (MAX_ENTRIES / 64);

    // Pin every overlapping cache page, creating an empty reservation for a
    // page that is not cached yet.  Merely pinning existing pages leaves a
    // coherence hole: while the backend write runs without the cache lock, a
    // reader can otherwise fill a previously absent page with the old disk
    // contents and keep that stale fill after the write completes.
    //
    // Existing valid/dirty data remains intact until the backend has
    // acknowledged the direct write. Invalidating before I/O lost the only
    // copy of dirty new data when the backend returned an error.
    var offset: usize = 0;
    while (offset < sector_count) {
        const cur = lba + offset;
        const page = pageLba(cur);
        var created = false;
        const index = findEntry(device_index, page) orelse blk: {
            const reserved = createEntry(device_index, page) orelse {
                // Release every reservation acquired so far. Existing cache
                // contents were not changed; newly created empty entries can
                // be discarded without I/O.
                releaseDirectWritePins(&owned_entries, &reserved_entries);
                stats.write_errors +%= 1;
                return false;
            };
            created = true;
            break :blk reserved;
        };
        if (entries[index].io_busy) {
            if (!waitBusy(guard)) {
                releaseDirectWritePins(&owned_entries, &reserved_entries);
                stats.write_errors +%= 1;
                return false;
            }
            continue;
        }
        dropReadAheadResidencyLocked(index);
        if (!policy_index.pin(index)) {
            releaseDirectWritePins(&owned_entries, &reserved_entries);
            stats.write_errors +%= 1;
            return false;
        }
        entries[index].io_busy = true;
        const ownership_bit = @as(u64, 1) << @as(u6, @intCast(index % 64));
        owned_entries[index / 64] |= ownership_bit;
        if (created) reserved_entries[index / 64] |= ownership_bit;
        const page_remaining = PAGE_SECTORS - @as(usize, @intCast(cur - pageLba(cur)));
        offset += @min(page_remaining, sector_count - offset);
    }

    stats.write_through_requests +%= 1;
    releaseLock(guard);
    locked = false;
    const write_result = block.writeDirectWithProgress(device_index, lba, sectors, staged_data[0..byte_count]);
    const completed_sectors: usize = @min(@as(usize, write_result.sectors_completed), sector_count);
    const write_ok = write_result.err == .none and completed_sectors == sector_count;

    // Every pinned page must be released under the cache lock on both success
    // and failure. relock() is deliberately non-abandoning.
    relock(guard);
    locked = true;
    offset = 0;
    while (offset < sector_count) {
        const cur = lba + offset;
        const page = pageLba(cur);
        const first = @as(usize, @intCast(cur - page));
        const span = @min(PAGE_SECTORS - first, sector_count - offset);
        if (findEntry(device_index, page)) |index| {
            const ownership_bit = @as(u64, 1) << @as(u6, @intCast(index % 64));
            const owns_entry = (owned_entries[index / 64] & ownership_bit) != 0 and
                entries[index].valid and
                entries[index].device_index == device_index and
                entries[index].page_lba == page and
                entries[index].io_busy;
            const completed_in_span = if (completed_sectors > offset)
                @min(span, completed_sectors - offset)
            else
                0;
            if (owns_entry and completed_in_span != 0) {
                var page_offset: usize = 0;
                while (page_offset < completed_in_span) : (page_offset += 1) {
                    const bit = @as(u8, 1) << @as(u3, @intCast(first + page_offset));
                    entries[index].valid_mask &= ~bit;
                    clearDirtyBits(index, bit);
                    stats.invalidations +%= 1;
                }
            }
            if (owns_entry) {
                entries[index].io_busy = false;
                _ = policy_index.unpin(index, entries[index].dirty_mask != 0);
                owned_entries[index / 64] &= ~ownership_bit;
                const was_reserved = (reserved_entries[index / 64] & ownership_bit) != 0;
                reserved_entries[index / 64] &= ~ownership_bit;
                if ((completed_in_span != 0 or was_reserved) and
                    entries[index].valid_mask == 0 and
                    entries[index].dirty_mask == 0)
                {
                    clearEntry(index, false);
                }
            }
        }
        offset += span;
    }
    // io_busy makes an owned slot non-evictable, so every bit must have been
    // found under the same device/page identity above.  On an invariant
    // violation, fail closed without unpinning an entry that may now belong to
    // another operation.
    for (owned_entries) |word| {
        if (word != 0) {
            stats.write_errors +%= 1;
            return false;
        }
    }
    for (reserved_entries) |word| {
        if (word != 0) {
            stats.write_errors +%= 1;
            return false;
        }
    }
    stats.write_through_updates +%= @intCast(completed_sectors);
    if (!write_ok) {
        stats.write_errors +%= 1;
        return false;
    }
    return true;
}

fn releaseDirectWritePins(
    owned_entries: *[MAX_ENTRIES / 64]u64,
    reserved_entries: *[MAX_ENTRIES / 64]u64,
) void {
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        const bit = @as(u64, 1) << @as(u6, @intCast(index % 64));
        if ((owned_entries[index / 64] & bit) == 0) continue;
        if (entries[index].io_busy) {
            entries[index].io_busy = false;
            _ = policy_index.unpin(index, entries[index].dirty_mask != 0);
        }
        if ((reserved_entries[index / 64] & bit) != 0 and
            entries[index].valid and
            entries[index].valid_mask == 0 and
            entries[index].dirty_mask == 0)
        {
            clearEntry(index, false);
        }
        owned_entries[index / 64] &= ~bit;
        reserved_entries[index / 64] &= ~bit;
    }
}

pub fn flushDevice(device_index: usize) bool {
    const guard = acquireLock() orelse return false;
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse return false;
    defer _ = task_context.leaveUnwind(unwind);
    stats.flushes +%= 1;
    if (!drainDevice(guard, device_index, .flush)) return false;
    releaseLock(guard);
    locked = false;
    if (!block.flush(device_index)) {
        stats.writeback_errors +%= 1;
        return false;
    }
    return true;
}

/// Commits only dirty sectors tagged with `batch`, then issues the same
/// backend durability barrier as flushDevice(). Dirty sectors from earlier or
/// concurrent operations remain cached, including when they share a 4-KB
/// cache page with the committed mutation.
pub fn flushDeviceBatch(device_index: usize, batch: WriteBatch) bool {
    if (batch == NO_WRITE_BATCH) return flushDevice(device_index);
    const guard = acquireLock() orelse return false;
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse return false;
    defer _ = task_context.leaveUnwind(unwind);
    stats.flushes +%= 1;
    stats.selective_flushes +%= 1;
    // A background writeback temporarily detaches its page from the dirty
    // FIFO. Wait for target-device dirty pins before taking the ownership
    // snapshot so an explicit durability boundary cannot overlook them.
    if (!waitForDeviceDirtyIdle(guard, device_index)) return false;
    const all_dirty = dirtySectorsForDevice(device_index);
    const owned_dirty = dirtySectorsForDeviceBatch(device_index, batch);
    if (all_dirty > owned_dirty) {
        stats.selective_foreign_dirty_sectors_skipped +%= all_dirty - owned_dirty;
    }
    if (!drainDeviceBatch(guard, device_index, batch, .flush)) return false;
    releaseLock(guard);
    locked = false;
    if (!block.flush(device_index)) {
        stats.writeback_errors +%= 1;
        return false;
    }
    return true;
}

pub fn flushAll() bool {
    const guard = acquireLock() orelse return false;
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse return false;
    defer _ = task_context.leaveUnwind(unwind);
    if (!drainAll(guard, .flush)) return false;
    releaseLock(guard);
    locked = false;
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        if (block.get(index) == null) continue;
        stats.flushes +%= 1;
        if (!block.flush(index)) {
            stats.writeback_errors +%= 1;
            return false;
        }
    }
    return true;
}

pub fn reclaimPayloadFrames(target_frames: u32, allow_dirty_drain: bool) ReclaimResult {
    const guard = acquireLock() orelse return .{ .requested_frames = target_frames };
    defer releaseLock(guard);
    const unwind = enterOperation() orelse return .{ .requested_frames = target_frames };
    defer _ = task_context.leaveUnwind(unwind);
    return reclaimLocked(guard, target_frames, allow_dirty_drain);
}

pub fn invalidateSector(device_index: usize, lba: u64) void {
    const guard = acquireLock() orelse return;
    defer releaseLock(guard);
    const unwind = enterOperation() orelse return;
    defer _ = task_context.leaveUnwind(unwind);
    const bit = sectorBit(lba);
    while (true) {
        const index = findEntry(device_index, pageLba(lba)) orelse return;
        if (entries[index].io_busy) {
            if (!waitBusy(guard)) return;
            continue;
        }
        if ((entries[index].dirty_mask & bit) != 0) {
            if (!writebackEntryUnlocked(guard, index)) return;
            continue;
        }
        entries[index].valid_mask &= ~bit;
        clearDirtyBits(index, bit);
        stats.invalidations +%= 1;
        if (entries[index].valid_mask == 0) clearEntry(index, false);
        return;
    }
}

pub fn invalidateDevice(device_index: usize) void {
    const guard = acquireLock() orelse return;
    defer releaseLock(guard);
    const unwind = enterOperation() orelse return;
    defer _ = task_context.leaveUnwind(unwind);
    if (device_index < MAX_DEVICES and read_ahead_states[device_index].cancelAll()) {
        stats.read_ahead_cancellations +%= 1;
    }
    while (true) {
        var found: ?usize = null;
        var index: usize = 0;
        while (index < entries.len) : (index += 1) {
            if (entries[index].valid and entries[index].device_index == device_index) {
                found = index;
                break;
            }
        }
        const target = found orelse return;
        if (entries[target].io_busy) {
            if (!waitBusy(guard)) return;
            continue;
        }
        if (entries[target].dirty_mask != 0) {
            if (!writebackEntryUnlocked(guard, target)) return;
            continue;
        }
        clearEntry(target, false);
        stats.invalidations +%= 1;
    }
}

// ---------------------------------------------------------------------------
// Lock-Helfer. Ergebnis von acquireLock: null = Lock-Timeout (Fehler),
// true = Lock gehalten, false = fruehe Boot-Phase (single-threaded,
// kein Lock noetig).
// ---------------------------------------------------------------------------

// Every public operation that can drop cache_lock while io_busy pins mutable
// metadata carries a task-local unwind claim. Hard kill cannot skip the Zig
// defers that relock, clear those pins and close any diagnostic generation.
// Boot code has no task context and is admitted without incrementing a count.
fn enterOperation() ?task_context.UnwindToken {
    const token = task_context.enterUnwind();
    return if (token.admitted()) token else null;
}

fn acquireLock() ?bool {
    if (scheduler.currentId() == null) return false;
    if (!cache_lock.lock(LOCK_TIMEOUT_TICKS)) {
        stats.lock_timeouts +%= 1;
        return null;
    }
    return true;
}

fn releaseLock(owned: bool) void {
    if (owned) _ = cache_lock.unlock();
}

// Nach einem entsperrten I/O den Lock zwingend zurueckholen.  Ohne Besitz
// weiterzumachen korrumpiert Hash/LRU und ein fremdes io_busy; andererseits
// darf die gepinnte Seite nicht herrenlos bleiben. Deshalb bounded wait +
// sichtbare Diagnose, aber niemals unlocked weiterlaufen.
fn relock(guard: bool) void {
    if (!guard) return;
    const diagnostic_slice_ticks: u64 = 5 * @as(u64, timer.DEFAULT_HZ);
    var slices: u64 = 0;
    var incident_token: diag_screen.IncidentToken = .{};
    while (!cache_lock.lock(diagnostic_slice_ticks)) {
        stats.lock_timeouts +%= 1;
        slices +%= 1;
        if (!incident_token.valid()) {
            incident_token = diag_screen.beginResolvableIncident();
        }
        diag_screen.write("[PGCACHE] relock stalled slice=");
        diag_screen.writeDec(slices);
        diag_screen.endLine();
    }
    if (incident_token.valid()) _ = diag_screen.resolveIncident(incident_token);
}

// Kurz warten, bis ein fremder Fill/Writeback fertig ist; Lock wird
// dabei freigegeben (kein sleep_under_lock) und wieder geholt.
fn waitBusy(guard: bool) bool {
    stats.busy_waits +%= 1;
    if (!guard) return true;
    _ = cache_lock.unlock();
    scheduler.sleepTicksWithReason(1, "pgcache-busy");
    // Every caller continues to touch protected entry metadata.  Therefore
    // it must either reacquire the lock or stay in the visible relock loop;
    // returning without ownership made the callers' deferred unlock and
    // metadata access corrupt a foreign owner.
    relock(guard);
    return true;
}

// ---------------------------------------------------------------------------
// Interna (Lock wird vom oeffentlichen Einstieg gehalten, sofern nicht
// anders vermerkt)
// ---------------------------------------------------------------------------

fn pageLba(lba: u64) u64 {
    return lba & ~@as(u64, PAGE_SECTORS - 1);
}

fn sectorBit(lba: u64) u8 {
    return @as(u8, 1) << @as(u3, @intCast(lba & (PAGE_SECTORS - 1)));
}

fn sectorOffset(lba: u64) usize {
    return @as(usize, @intCast(lba & (PAGE_SECTORS - 1))) * SECTOR_SIZE;
}

fn bucketOf(device_index: usize, page_lba: u64) usize {
    var h: u64 = page_lba *% 0x9E3779B97F4A7C15;
    h ^= (@as(u64, device_index) +% 1) *% 0xC2B2AE3D27D4EB4F;
    h ^= h >> 29;
    return @intCast(h & (BUCKET_COUNT - 1));
}

fn findEntry(device_index: usize, page_lba: u64) ?usize {
    var cursor = buckets[bucketOf(device_index, page_lba)];
    while (cursor != NO_INDEX) {
        const idx: usize = cursor;
        const e = entries[idx];
        if (e.valid and e.device_index == device_index and e.page_lba == page_lba) return idx;
        cursor = e.next;
    }
    return null;
}

fn linkEntry(index: usize) void {
    const b = bucketOf(entries[index].device_index, entries[index].page_lba);
    entries[index].next = buckets[b];
    buckets[b] = @intCast(index);
}

fn unlinkEntry(index: usize) void {
    const b = bucketOf(entries[index].device_index, entries[index].page_lba);
    var cursor = buckets[b];
    var prev: u16 = NO_INDEX;
    while (cursor != NO_INDEX) {
        if (cursor == @as(u16, @intCast(index))) {
            if (prev == NO_INDEX) {
                buckets[b] = entries[index].next;
            } else {
                entries[@as(usize, prev)].next = entries[index].next;
            }
            entries[index].next = NO_INDEX;
            return;
        }
        prev = cursor;
        cursor = entries[@as(usize, cursor)].next;
    }
}

// Legt einen leeren Eintrag (valid, ohne gueltige Sektoren) samt Frame an.
fn createEntry(device_index: usize, page_lba: u64) ?usize {
    if (device_index >= MAX_DEVICES) return null;
    const index = selectWritableSlot(device_index) orelse return null;
    const kept_phys = entries[index].phys_addr;
    entries[index] = .{
        .valid = true,
        .device_index = device_index,
        .page_lba = page_lba,
        .last_use = nextClock(),
        .phys_addr = kept_phys,
    };
    if (ensurePayload(index) == null) {
        const phys = entries[index].phys_addr;
        entries[index] = .{ .phys_addr = phys };
        _ = policy_index.release(index);
        return null;
    }
    if (!policy_index.attachClean(index, device_index)) {
        const phys = entries[index].phys_addr;
        entries[index] = .{ .phys_addr = phys };
        _ = policy_index.release(index);
        return null;
    }
    linkEntry(index);
    return index;
}

// Fuellt fehlende Sektoren der Seite von der Disk nach. I/O laeuft OHNE
// Lock; io_busy pinnt den Eintrag solange. Gueltige (auch dirty)
// Sektoren bleiben unangetastet.
fn fillEntryWithBackendPolicy(guard: bool, index: usize) bool {
    if (fillEntryUnlocked(guard, index)) return true;
    const device = entries[index].device_index;
    const backend = block.get(device) orelse return false;
    // USBMSC performs its own bounded wire-level recovery/retry and must not
    // be multiplied by the cache. AHCI/NVMe and other backends retain the
    // historical single post-fill retry, still merging around dirty sectors.
    if (backend.owns_transport_retry) return false;
    return fillEntryUnlocked(guard, index);
}

fn fillEntryUnlocked(guard: bool, index: usize) bool {
    if (entries[index].valid_mask == FULL_MASK) return true;
    const device = entries[index].device_index;
    const page = entries[index].page_lba;
    const mask_before = entries[index].valid_mask;
    const frame = payloadFrame(index) orelse return false;
    const device_info = block.get(device) orelse return false;
    if (device_info.sector_size != SECTOR_SIZE) return false;

    var readable_sectors: usize = PAGE_SECTORS;
    if (device_info.sector_count != 0) {
        if (page >= device_info.sector_count) return false;
        readable_sectors = @intCast(@min(
            @as(u64, PAGE_SECTORS),
            device_info.sector_count - page,
        ));
    }
    const readable_mask: u8 = if (readable_sectors == PAGE_SECTORS)
        FULL_MASK
    else
        (@as(u8, 1) << @as(u3, @intCast(readable_sectors))) - 1;
    if ((readable_mask & ~mask_before) == 0) return false;

    if (!policy_index.pin(index)) return false;
    entries[index].io_busy = true;
    releaseLock(guard);

    var ok = true;
    if (mask_before == 0) {
        // Ganze Seite mit EINEM Read direkt in den Frame - der Kern des
        // page-organisierten Designs. Die letzte Geraeteseite darf kuerzer
        // sein; das ist kein Transportfehler und benoetigt keinen Fallback.
        // io_busy haelt Schreiber fern.
        const byte_count = readable_sectors * SECTOR_SIZE;
        ok = block.readDirect(
            device,
            page,
            @intCast(readable_sectors),
            frame[0..byte_count],
        );
    } else {
        // Seltener Merge-Fall (write-first-Seite): nur fehlende Sektoren
        // einzeln nachladen, dirty Daten nicht ueberschreiben.
        var sector: usize = 0;
        var buf: [SECTOR_SIZE]u8 = undefined;
        while (sector < readable_sectors) : (sector += 1) {
            const bit = @as(u8, 1) << @as(u3, @intCast(sector));
            if ((mask_before & bit) != 0) continue;
            if (!block.readDirect(device, page + sector, 1, buf[0..])) {
                ok = false;
                break;
            }
            const off = sector * SECTOR_SIZE;
            @memcpy(frame[off .. off + SECTOR_SIZE], buf[0..SECTOR_SIZE]);
        }
    }

    relock(guard);
    entries[index].io_busy = false;
    _ = policy_index.unpin(index, entries[index].dirty_mask != 0);
    if (ok) {
        entries[index].valid_mask = mask_before | readable_mask;
        entries[index].last_use = nextClock();
        stats.fills +%= 1;
    }
    return ok;
}

const FillRunOutcome = struct {
    ok: bool = false,
    pages: u16 = 0,
    sectors: u16 = 0,
};

fn fillRunPageLimitLocked(device_index: usize, requested_pages: u16) u16 {
    const device = block.get(device_index) orelse return 0;
    if (device.sector_size != SECTOR_SIZE) return 0;
    const capacity = refreshCapacityLocked();
    return page_cache_policy.fillRunPageLimit(
        @min(requested_pages, MAX_CONTIGUOUS_FILL_PAGES),
        device.max_sectors_per_request,
        PAGE_SECTORS,
        capacity.active_pages,
    );
}

/// Pins a contiguous set of absent or partially valid cache pages, submits
/// one backend-sized read into resident heap staging, and scatters only the
/// sectors that were missing at reservation time. Dirty/valid bytes are
/// therefore never replaced by stale media contents. Publication is
/// all-or-nothing because readDirect has no partial-read success contract.
fn fillContiguousRunLocked(
    guard: bool,
    device_index: usize,
    first_page: u64,
    requested_pages: u16,
    staging: []u8,
    speculative: bool,
) FillRunOutcome {
    const device = block.get(device_index) orelse return .{};
    if (device.sector_size != SECTOR_SIZE) return .{};
    const device_sectors = device.sector_count;
    const owns_transport_retry = device.owns_transport_retry;
    const staging_pages: u16 = @intCast(@min(
        @as(usize, MAX_ENTRIES),
        staging.len / PAGE_BYTES,
    ));
    const page_limit = @min(
        staging_pages,
        fillRunPageLimitLocked(device_index, requested_pages),
    );
    if (page_limit < 2) return .{};

    var indices: [MAX_ENTRIES]u16 = undefined;
    var masks_before: [MAX_ENTRIES]u8 = undefined;
    var reserved: usize = 0;
    var total_sectors: u16 = 0;
    var page = first_page;
    while (reserved < page_limit) {
        if (device_sectors != 0 and page >= device_sectors) break;
        const readable: usize = if (device_sectors == 0)
            PAGE_SECTORS
        else
            @intCast(@min(@as(u64, PAGE_SECTORS), device_sectors - page));
        const readable_mask = readableMask(readable);
        const existing = findEntry(device_index, page);
        if (speculative and existing != null) break;
        const index = existing orelse if (speculative)
            createReadAheadEntryLocked(device_index, page) orelse break
        else
            createEntry(device_index, page) orelse break;
        if (entries[index].io_busy or
            (entries[index].valid_mask & readable_mask) == readable_mask)
        {
            break;
        }
        if (!policy_index.pin(index)) break;
        entries[index].io_busy = true;
        if (!speculative) dropReadAheadResidencyLocked(index);
        indices[reserved] = @intCast(index);
        masks_before[reserved] = entries[index].valid_mask;
        reserved += 1;
        total_sectors += @intCast(readable);
        if (readable != PAGE_SECTORS or page > std.math.maxInt(u64) - PAGE_SECTORS) break;
        page += PAGE_SECTORS;
    }

    // If contention or capacity left only one page, retain the established
    // single-frame direct path: it avoids allocating/scattering for no I/O
    // reduction and preserves the existing retry rule.
    if (reserved < 2) {
        if (reserved == 0) return .{};
        const index: usize = indices[0];
        entries[index].io_busy = false;
        _ = policy_index.unpin(index, entries[index].dirty_mask != 0);
        return .{
            .ok = fillEntryWithBackendPolicy(guard, index),
            .pages = 1,
            .sectors = @intCast(@min(@as(usize, PAGE_SECTORS), total_sectors)),
        };
    }

    const byte_count = @as(usize, total_sectors) * SECTOR_SIZE;
    stats.fill_run_requests +%= 1;
    stats.fill_run_pages +%= reserved;
    stats.fill_run_sectors +%= total_sectors;
    stats.fill_run_bytes +%= byte_count;
    if (reserved > stats.fill_run_max_pages) stats.fill_run_max_pages = reserved;
    stats.fill_lock_drops +%= 1;
    releaseLock(guard);

    var ok = false;
    var attempts: usize = 0;
    var backend_requests: u64 = 0;
    var retries: u64 = 0;
    const max_attempts: usize = page_cache_policy.fillAttemptLimit(owns_transport_retry);
    while (attempts < max_attempts) : (attempts += 1) {
        backend_requests +%= 1;
        if (block.readDirect(
            device_index,
            first_page,
            total_sectors,
            staging[0..byte_count],
        )) {
            ok = true;
            break;
        }
        if (attempts + 1 < max_attempts) retries +%= 1;
    }

    relock(guard);
    stats.fill_run_backend_requests +%= backend_requests;
    stats.fill_run_retries +%= retries;
    var slot: usize = 0;
    while (slot < reserved) : (slot += 1) {
        const index: usize = indices[slot];
        const expected_page = first_page + @as(u64, slot) * PAGE_SECTORS;
        if (!entries[index].valid or
            entries[index].device_index != device_index or
            entries[index].page_lba != expected_page or
            !entries[index].io_busy or
            payloadFrame(index) == null)
        {
            ok = false;
        }
    }
    if (!ok) stats.fill_run_failures +%= 1;
    var offset_sectors: usize = 0;
    slot = 0;
    while (slot < reserved) : (slot += 1) {
        const index: usize = indices[slot];
        const expected_page = first_page + @as(u64, slot) * PAGE_SECTORS;
        const identity_ok = entries[index].valid and
            entries[index].device_index == device_index and
            entries[index].page_lba == expected_page and
            entries[index].io_busy;
        if (!identity_ok) {
            ok = false;
            continue;
        }
        const readable: usize = if (device_sectors == 0)
            PAGE_SECTORS
        else
            @intCast(@min(@as(u64, PAGE_SECTORS), device_sectors - expected_page));
        const readable_mask = readableMask(readable);
        if (ok) {
            const frame = payloadFrame(index).?;
            var sector: usize = 0;
            while (sector < readable) : (sector += 1) {
                const bit = @as(u8, 1) << @as(u3, @intCast(sector));
                if ((page_cache_policy.fillMissingMask(masks_before[slot], readable_mask) & bit) == 0) continue;
                const source_offset = (offset_sectors + sector) * SECTOR_SIZE;
                const target_offset = sector * SECTOR_SIZE;
                @memcpy(
                    frame[target_offset .. target_offset + SECTOR_SIZE],
                    staging[source_offset .. source_offset + SECTOR_SIZE],
                );
                stats.fill_scatter_copy_bytes +%= SECTOR_SIZE;
            }
            entries[index].valid_mask = page_cache_policy.fillPublishedMask(
                masks_before[slot],
                readable_mask,
                true,
            );
            entries[index].last_use = nextClock();
            stats.fills +%= 1;
        }
        offset_sectors += readable;
        entries[index].io_busy = false;
        _ = policy_index.unpin(index, !ok or entries[index].dirty_mask != 0);
        if (!ok and entries[index].valid_mask == 0 and entries[index].dirty_mask == 0) {
            clearEntry(index, false);
        }
    }
    return .{
        .ok = ok,
        .pages = @intCast(reserved),
        .sectors = total_sectors,
    };
}

fn readableMask(sectors: usize) u8 {
    if (sectors >= PAGE_SECTORS) return FULL_MASK;
    if (sectors == 0) return 0;
    return (@as(u8, 1) << @as(u3, @intCast(sectors))) - 1;
}

fn refreshCapacityLocked() page_cache_policy.Capacity {
    const next = page_cache_policy.capacityForMemory(
        capacity_reference_frames,
        mem_phys.stats().free_frames,
    );
    if (next.active_pages < capacity_state.active_pages) {
        stats.capacity_reductions +%= 1;
    }
    if (next.read_ahead_pages < capacity_state.read_ahead_pages) {
        for (&read_ahead_states) |*state| {
            if (state.pending or state.inflight) {
                if (state.cancelAll()) stats.read_ahead_cancellations +%= 1;
            } else if (next.read_ahead_pages == 0) {
                state.sequential.reset();
            }
        }
    }
    capacity_state = next;
    stats.capacity_ram_limit_pages = next.ram_pages;
    stats.capacity_active_limit_pages = next.active_pages;
    stats.capacity_pressure_level = next.pressure_level;
    stats.read_ahead_window_max_pages = if (SPECULATIVE_READ_AHEAD_ENABLED) next.read_ahead_pages else 0;
    return next;
}

fn capacityOverLimitLocked() bool {
    return policy_index.entryCount() > refreshCapacityLocked().active_pages;
}

fn trimCapacityOne() bool {
    const guard = acquireLock() orelse return false;
    defer releaseLock(guard);
    const unwind = enterOperation() orelse return false;
    defer _ = task_context.leaveUnwind(unwind);
    if (!capacityOverLimitLocked()) return false;
    const slot = selectCleanPayloadSlot() orelse return false;
    stats.evictions +%= 1;
    stats.reclaim_clean_entries +%= 1;
    stats.capacity_trimmed_pages +%= 1;
    clearEntry(slot, true);
    return true;
}

fn selectWritableSlot(preferred_device: usize) ?usize {
    const capacity = refreshCapacityLocked();
    if (policy_index.entryCount() < capacity.active_pages) {
        if (policy_index.claimFree()) |index| return index;
    }
    stats.reclaim_scans +%= 1;
    const slot = policy_index.cleanVictim(preferred_device) orelse {
        // Alles dirty oder gepinnt: KEIN Writeback hier - der wuerde
        // den Lock innerhalb eines verschachtelten Ablaufs freigeben.
        // Der Aufrufer faellt auf unkached/Write-Through zurueck;
        // Draenage besorgen Flush-Pfade und der PMM-Reclaim.
        stats.reclaim_failed_drains +%= 1;
        return null;
    };
    evictForReplacement(slot);
    return slot;
}

fn selectCleanPayloadSlot() ?usize {
    return policy_index.cleanVictim(null);
}

fn reclaimLocked(guard: bool, target_frames: u32, allow_dirty_drain: bool) ReclaimResult {
    var result = ReclaimResult{
        .requested_frames = target_frames,
    };
    if (target_frames == 0) return result;

    stats.reclaim_scans +%= 1;
    while (result.returned_frames < target_frames) {
        if (selectCleanPayloadSlot()) |slot| {
            stats.evictions +%= 1;
            stats.reclaim_clean_entries +%= 1;
            clearEntry(slot, true);
            result.returned_frames += 1;
            result.returned_bytes +%= PAYLOAD_FRAME_BYTES_U64;
            continue;
        }

        if (!allow_dirty_drain) break;
        const before_drains = stats.reclaim_dirty_drains;
        const before_failures = stats.reclaim_failed_drains;
        if (!writebackOldestDirty(guard, .pressure)) {
            result.failed_drains +%= stats.reclaim_failed_drains - before_failures;
            break;
        }
        const drained = stats.reclaim_dirty_drains - before_drains;
        if (drained == 0) break;
        result.dirty_drains +%= drained;
    }

    return result;
}

const DrainReason = enum {
    flush,
    pressure,
    background_pressure,
    background_age,
};

fn drainDevice(guard: bool, device_index: usize, reason: DrainReason) bool {
    if (dirtyEntriesForDevice(device_index) == 0) return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    var written: u64 = 0;
    while (dirtyEntriesForDevice(device_index) != 0) {
        const index = findOldestDirtyForDevice(device_index) orelse {
            if (device_index < MAX_DEVICES and policy_index.devices[device_index].busy_dirty != 0) {
                if (!waitForDeviceDirtyIdle(guard, device_index)) return false;
                continue;
            }
            recordDirtyIndexFallback(device_index);
            return false;
        };
        const before = stats.writeback_sectors;
        if (!writebackEntryUnlocked(guard, index)) return false;
        written +%= stats.writeback_sectors - before;
    }
    recordDrain(reason, written, start);
    return true;
}

fn drainDeviceBatch(guard: bool, device_index: usize, batch: WriteBatch, reason: DrainReason) bool {
    if (!waitForDeviceDirtyIdle(guard, device_index)) return false;
    if (dirtySectorsForDeviceBatch(device_index, batch) == 0) return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    var written: u64 = 0;
    while (true) {
        const index = findOldestDirtyForDeviceBatch(device_index, batch) orelse {
            if (device_index < MAX_DEVICES and policy_index.devices[device_index].busy_dirty != 0) {
                if (!waitForDeviceDirtyIdle(guard, device_index)) return false;
                continue;
            }
            break;
        };
        const before = stats.writeback_sectors;
        if (!writebackEntryBatchUnlocked(guard, index, batch)) return false;
        written +%= stats.writeback_sectors - before;
    }
    stats.selective_writeback_sectors +%= written;
    recordDrain(reason, written, start);
    return true;
}

fn drainAll(guard: bool, reason: DrainReason) bool {
    if (dirtyEntries() == 0) return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    var written: u64 = 0;
    while (dirtyEntries() != 0) {
        const index = findOldestDirty() orelse {
            if (hasBusyDirty()) {
                if (!waitForAnyDirtyIdle(guard)) return false;
                continue;
            }
            recordDirtyIndexFallback(null);
            return false;
        };
        const before = stats.writeback_sectors;
        if (!writebackEntryUnlocked(guard, index)) return false;
        written +%= stats.writeback_sectors - before;
    }
    recordDrain(reason, written, start);
    return true;
}

fn writebackOldestDirty(guard: bool, reason: DrainReason) bool {
    const index = findOldestDirty() orelse return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    const before = stats.writeback_sectors;
    if (!writebackEntryUnlocked(guard, index)) {
        if (reason == .pressure) stats.reclaim_failed_drains +%= 1;
        return false;
    }
    recordDrain(reason, stats.writeback_sectors - before, start);
    return true;
}

// Schreibt alle dirty Sektoren des Eintrags zurueck (sektorweise;
// Run-Coalescing kommt in 0.56.9). I/O laeuft OHNE Lock unter io_busy;
// Schreiber auf dieselbe Seite warten solange (waitBusy), daher ist der
// Frame waehrend des I/O stabil.
fn writebackEntryUnlocked(guard: bool, index: usize) bool {
    return writebackEntrySelectedUnlocked(guard, index, null);
}

fn writebackEntryBatchUnlocked(guard: bool, index: usize, batch: WriteBatch) bool {
    return writebackEntrySelectedUnlocked(guard, index, batch);
}

fn writebackEntrySelectedUnlocked(guard: bool, index: usize, batch: ?WriteBatch) bool {
    if (index >= entries.len) return true;
    while (entries[index].valid and entries[index].io_busy) {
        if (!waitBusy(guard)) return false;
        // Nach dem Schlaf kann der Slot eine ANDERE Seite tragen -
        // egal: irgendeinen dirty Zustand dieses Slots zu flushen ist
        // immer korrekt, die Drain-Schleifen re-scannen ohnehin.
    }
    if (!entries[index].valid) return true;
    const dirty_snapshot = if (batch) |owner|
        dirtyMaskForBatch(&entries[index], owner)
    else
        entries[index].dirty_mask;
    if (dirty_snapshot == 0) return true;
    const device = entries[index].device_index;
    const page = entries[index].page_lba;
    const frame = payloadFrame(index) orelse {
        stats.writeback_errors +%= 1;
        return false;
    };
    if (!policy_index.pin(index)) {
        stats.writeback_errors +%= 1;
        return false;
    }
    entries[index].io_busy = true;
    releaseLock(guard);

    // 0.56.9 (vorgezogen): zusammenhaengende dirty Runs mit EINEM
    // block.write schreiben statt sektorweise. Ohne Coalescing entlud
    // ein Flush des groesseren v2-Caches hunderte 512-B-Einzelwrites in
    // die Block-Queue, hinter denen alle FS-Reads anstanden (Gate-
    // Befund: Sessions starben mitten in der Auth, sobald ein Flush-
    // Burst lief).
    var ok = true;
    var written_bits: u8 = 0;
    var sector: usize = 0;
    while (sector < PAGE_SECTORS) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((dirty_snapshot & bit) == 0) {
            sector += 1;
            continue;
        }
        var run_len: usize = 1;
        while (sector + run_len < PAGE_SECTORS) {
            const next_bit = @as(u8, 1) << @as(u3, @intCast(sector + run_len));
            if ((dirty_snapshot & next_bit) == 0) break;
            run_len += 1;
        }
        const off = sector * SECTOR_SIZE;
        const run_bytes = run_len * SECTOR_SIZE;
        // USBMSC owns its transport recovery and exact-one READ/WRITE retry.
        // Replaying again here multiplied a single WRITE10 failure into up to
        // four wire attempts and immediately re-entered the just-reset BOT
        // session. Other backends retain the historical cache retry.
        const max_retries: usize = if (block.get(device)) |backend|
            if (backend.owns_transport_retry) 0 else MAX_WRITEBACK_RETRIES
        else
            0;
        var retries: usize = 0;
        while (true) {
            if (block.writeDirect(device, page + sector, @intCast(run_len), frame[off .. off + run_bytes])) break;
            if (retries >= max_retries) {
                ok = false;
                break;
            }
            retries += 1;
            stats.writeback_retries +%= 1;
        }
        if (!ok) break;
        var mark: usize = 0;
        while (mark < run_len) : (mark += 1) {
            written_bits |= @as(u8, 1) << @as(u3, @intCast(sector + mark));
        }
        sector += run_len;
    }

    relock(guard);
    entries[index].io_busy = false;
    clearDirtyBits(index, written_bits);
    _ = policy_index.unpin(index, !ok or entries[index].dirty_mask != 0);
    stats.writeback_sectors +%= @popCount(written_bits);
    if (!ok) stats.writeback_errors +%= 1;
    return ok;
}

fn findOldestDirty() ?usize {
    const device_index = policy_index.nextDirtyDevice(false) orelse return null;
    return policy_index.dirtyHead(device_index);
}

fn findOldestDirtyForDevice(device_index: usize) ?usize {
    return policy_index.dirtyHead(device_index);
}

fn findOldestDirtyForDeviceBatch(device_index: usize, batch: WriteBatch) ?usize {
    var best: ?usize = null;
    var best_sequence: u64 = 0;
    var cursor = policy_index.dirtyHead(device_index);
    while (cursor) |index| {
        const entry = &entries[index];
        if (oldestDirtySequenceForBatch(entry, batch)) |sequence| {
            if (best == null or sequence < best_sequence) {
                best = index;
                best_sequence = sequence;
            }
        }
        cursor = policy_index.nextDirty(index);
    }
    return best;
}

fn markDirty(index: usize, bits: u8, owner: WriteBatch) bool {
    if (index >= entries.len or !entries[index].valid or entries[index].device_index >= MAX_DEVICES) return false;
    const newly_dirty = bits & ~entries[index].dirty_mask;
    if (entries[index].dirty_mask == 0) {
        if (!policy_index.markDirty(index)) return false;
        entries[index].dirty_sequence = nextDirtySequence();
        entries[index].dirty_since_tick = @max(1, timer.tickCount());
        dirty_entry_count +%= 1;
    }
    entries[index].dirty_mask |= bits;
    dirty_sector_count +%= @popCount(newly_dirty);
    dirty_sector_count_by_device[entries[index].device_index] +%= @popCount(newly_dirty);
    var sector: usize = 0;
    while (sector < PAGE_SECTORS) : (sector += 1) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((bits & bit) == 0) continue;
        entries[index].dirty_owner[sector] = owner;
        entries[index].dirty_write_sequence[sector] = nextDirtyWriteSequence();
    }
    updateDirtyHighWater();
    if (policy_index.devices[entries[index].device_index].pressure_active) policy_event.signal();
    return true;
}

fn clearDirtyBits(index: usize, bits: u8) void {
    if (index >= entries.len or bits == 0) return;
    const cleared = entries[index].dirty_mask & bits;
    if (cleared == 0) return;
    entries[index].dirty_mask &= ~cleared;
    const cleared_count: u64 = @popCount(cleared);
    dirty_sector_count -|= cleared_count;
    if (entries[index].device_index < MAX_DEVICES) {
        dirty_sector_count_by_device[entries[index].device_index] -|= cleared_count;
    }
    page_cache_batch.clearOwnership(
        &entries[index].dirty_owner,
        &entries[index].dirty_write_sequence,
        cleared,
    );
    if (entries[index].dirty_mask == 0) {
        entries[index].dirty_sequence = 0;
        entries[index].dirty_since_tick = 0;
        if (dirty_entry_count != 0) dirty_entry_count -= 1;
        _ = policy_index.clearDirty(index);
    }
}

fn dirtyMaskForBatch(entry: *const Entry, batch: WriteBatch) u8 {
    return page_cache_batch.maskForOwner(entry.dirty_mask, &entry.dirty_owner, batch);
}

fn oldestDirtySequenceForBatch(entry: *const Entry, batch: WriteBatch) ?u64 {
    return page_cache_batch.oldestSequenceForOwner(
        entry.dirty_mask,
        &entry.dirty_owner,
        &entry.dirty_write_sequence,
        batch,
    );
}

fn recordDrain(reason: DrainReason, written: u64, start_tick: u64) void {
    if (written == 0) return;
    const elapsed = elapsedTicks(start_tick);
    stats.writeback_drains +%= 1;
    switch (reason) {
        .flush => stats.writeback_flush_drains +%= 1,
        .pressure => {
            stats.writeback_pressure_drains +%= 1;
            stats.reclaim_dirty_drains +%= written;
        },
        .background_pressure, .background_age => {},
    }
    stats.writeback_last_ticks = elapsed;
    stats.writeback_total_ticks +%= elapsed;
    if (elapsed > stats.writeback_max_ticks) stats.writeback_max_ticks = elapsed;
}

fn elapsedTicks(start_tick: u64) u64 {
    const now = timer.tickCount();
    return if (now >= start_tick) now - start_tick else 0;
}

fn dirtyEntries() u32 {
    return dirty_entry_count;
}

fn dirtyEntriesForDevice(device_index: usize) u32 {
    if (device_index >= MAX_DEVICES) return 0;
    return policy_index.devices[device_index].dirty;
}

fn dirtySectorsForDevice(device_index: usize) u64 {
    if (device_index >= MAX_DEVICES) return 0;
    return dirty_sector_count_by_device[device_index];
}

fn dirtySectorsForDeviceBatch(device_index: usize, batch: WriteBatch) u64 {
    var dirty: u64 = 0;
    var cursor = policy_index.dirtyHead(device_index);
    while (cursor) |index| {
        dirty +%= @popCount(dirtyMaskForBatch(&entries[index], batch));
        cursor = policy_index.nextDirty(index);
    }
    return dirty;
}

fn hasBusyEntry() bool {
    return policy_index.busy_count != 0;
}

fn hasBusyDirty() bool {
    for (policy_index.devices) |device| {
        if (device.busy_dirty != 0) return true;
    }
    return false;
}

fn waitForDeviceDirtyIdle(guard: bool, device_index: usize) bool {
    if (device_index >= MAX_DEVICES) return false;
    while (policy_index.devices[device_index].busy_dirty != 0) {
        // A false guard denotes single-threaded boot, where a busy owner
        // cannot make progress independently and therefore signals an
        // invariant violation instead of entering a spin loop.
        if (!guard or !waitBusy(guard)) return false;
    }
    return true;
}

fn waitForAnyDirtyIdle(guard: bool) bool {
    while (hasBusyDirty()) {
        if (!guard or !waitBusy(guard)) return false;
    }
    return true;
}

fn recordDirtyIndexFallback(device_filter: ?usize) void {
    // This is deliberately an exceptional diagnostic full scan. Routine
    // selection stays on the per-device queues; the counter makes any index
    // divergence visible instead of silently treating dirty data as clean.
    stats.policy_full_scan_fallbacks +%= 1;
    var observed: u64 = 0;
    for (&entries) |*entry| {
        if (!entry.valid or entry.dirty_mask == 0) continue;
        if (device_filter) |device_index| {
            if (entry.device_index != device_index) continue;
        }
        observed +%= 1;
    }
    const incident_token = diag_screen.beginResolvableIncident();
    diag_screen.write("[PGCACHE] dirty index fallback dev=");
    if (device_filter) |device_index| {
        diag_screen.writeDec(device_index);
    } else {
        diag_screen.write("all");
    }
    diag_screen.write(" observed=");
    diag_screen.writeDec(observed);
    diag_screen.endLine();
    _ = diag_screen.resolveIncident(incident_token);
}

fn evictForReplacement(index: usize) void {
    if (index >= entries.len or !entries[index].valid) return;
    stats.evictions +%= 1;
    if (entries[index].dirty_mask == 0) stats.reclaim_clean_entries +%= 1;
    // Frame behalten: der Slot wird sofort wiederbelegt (createEntry
    // uebernimmt phys_addr), das spart Free+Alloc pro Eviction.
    dropReadAheadResidencyLocked(index);
    unlinkEntry(index);
    _ = policy_index.detach(index);
    const phys = entries[index].phys_addr;
    entries[index] = .{ .phys_addr = phys };
}

fn clearEntry(index: usize, reclaim: bool) void {
    if (index >= entries.len) return;
    if (entries[index].valid) {
        dropReadAheadResidencyLocked(index);
        unlinkEntry(index);
        _ = policy_index.detach(index);
    }
    releasePayload(index, reclaim);
    entries[index] = .{};
    _ = policy_index.release(index);
}

fn ensurePayload(index: usize) ?[]u8 {
    if (index >= entries.len) return null;
    if (entries[index].phys_addr == 0) {
        const phys_addr = mem_phys.allocFrame() orelse blk: {
            // Nur saubere Frames reklamieren (kein Dirty-Drain: der
            // wuerde den Lock in einem verschachtelten Ablauf freigeben).
            _ = reclaimLocked(false, 1, false);
            break :blk mem_phys.allocFrame() orelse {
                stats.payload_allocation_failures +%= 1;
                return null;
            };
        };
        entries[index].phys_addr = phys_addr;
        payload_frame_count +%= 1;
        stats.payload_allocations +%= 1;
    }
    return payloadFrame(index);
}

fn payloadFrame(index: usize) ?[]u8 {
    if (index >= entries.len or entries[index].phys_addr == 0) return null;
    const virt_addr = mem_phys.physToVirt(entries[index].phys_addr);
    if (virt_addr == 0) return null;
    const ptr: [*]u8 = @ptrFromInt(virt_addr);
    return ptr[0..PAYLOAD_FRAME_BYTES];
}

fn releasePayload(index: usize, reclaim: bool) void {
    if (index >= entries.len or entries[index].phys_addr == 0) return;
    mem_phys.freeFrame(entries[index].phys_addr);
    entries[index].phys_addr = 0;
    if (payload_frame_count != 0) payload_frame_count -= 1;
    stats.payload_releases +%= 1;
    if (reclaim) {
        stats.reclaim_returned_frames +%= 1;
        stats.reclaim_returned_bytes +%= PAYLOAD_FRAME_BYTES_U64;
    }
}

fn releaseAllPayloads(reclaim: bool) void {
    var index: usize = 0;
    while (index < entries.len) : (index += 1) releasePayload(index, reclaim);
}

const BackgroundOutcome = enum {
    none,
    wrote,
    failed,
};

fn policyWorkerMain() callconv(.c) void {
    while (true) {
        _ = policy_event.waitResult(BACKGROUND_INTERVAL_TICKS);
        stats.policy_worker_wakeups +%= 1;

        var pages: usize = 0;
        var failed = false;
        while (pages < BACKGROUND_PAGE_BUDGET) {
            switch (backgroundWritebackOne()) {
                .none => break,
                .wrote => pages += 1,
                .failed => {
                    failed = true;
                    break;
                },
            }
        }

        // Dirty progress owns priority. With speculation disabled, an idle
        // pass performs at most one bounded capacity trim.
        if (pages == 0 and !failed) _ = trimCapacityOne();

        if (pages == BACKGROUND_PAGE_BUDGET and backgroundNeedsPromptWake()) {
            scheduler.sleepTicksWithReason(BACKGROUND_REQUEUE_TICKS, "pgcache-policy-yield");
            policy_event.signal();
        }
    }
}

fn backgroundWritebackOne() BackgroundOutcome {
    const guard = acquireLock() orelse return .failed;
    defer releaseLock(guard);
    const unwind = enterOperation() orelse return .failed;
    defer _ = task_context.leaveUnwind(unwind);

    const now = timer.tickCount();
    const selection = selectBackgroundDirtyLocked(now) orelse return .none;
    const start = now;
    const before = stats.writeback_sectors;
    stats.writeback_waits +%= 1;
    if (!writebackEntryUnlocked(guard, selection.index)) {
        background_failure_until[selection.device_index] = timer.deadlineAfter(
            timer.tickCount(),
            BACKGROUND_FAILURE_BACKOFF_TICKS,
        );
        stats.policy_background_errors +%= 1;
        return .failed;
    }
    background_failure_until[selection.device_index] = 0;
    const written = stats.writeback_sectors - before;
    if (written == 0) return .none;
    stats.policy_background_drains +%= 1;
    stats.policy_background_sectors +%= written;
    switch (selection.reason) {
        .pressure => {
            stats.policy_background_pressure_drains +%= 1;
            recordDrain(.background_pressure, written, start);
        },
        .age => {
            stats.policy_background_age_drains +%= 1;
            recordDrain(.background_age, written, start);
        },
    }
    return .wrote;
}

fn selectBackgroundDirtyLocked(now: u64) ?BackgroundSelection {
    const over_capacity = capacityOverLimitLocked();
    var visited: u8 = 0;
    var probes: usize = 0;
    while (probes < MAX_DEVICES) : (probes += 1) {
        const device_index = policy_index.nextDirtyDevice(false) orelse break;
        const bit = @as(u8, 1) << @as(u3, @intCast(device_index));
        if ((visited & bit) != 0) break;
        visited |= bit;
        if (now < background_failure_until[device_index]) continue;
        const index = policy_index.dirtyHead(device_index) orelse continue;
        const reason: BackgroundReason = if (policy_index.devices[device_index].pressure_active or over_capacity)
            .pressure
        else if (page_cache_policy.ageDue(now, entries[index].dirty_since_tick, MAX_DIRTY_AGE_TICKS))
            .age
        else
            continue;
        return .{ .index = index, .device_index = device_index, .reason = reason };
    }
    return null;
}

fn backgroundNeedsPromptWake() bool {
    const guard = acquireLock() orelse return false;
    defer releaseLock(guard);
    if (capacityOverLimitLocked()) return true;
    const now = timer.tickCount();
    var device_index: usize = 0;
    while (device_index < MAX_DEVICES) : (device_index += 1) {
        if (now < background_failure_until[device_index]) continue;
        const index = policy_index.dirtyHead(device_index) orelse continue;
        if (policy_index.devices[device_index].pressure_active or
            page_cache_policy.ageDue(now, entries[index].dirty_since_tick, MAX_DIRTY_AGE_TICKS)) return true;
    }
    return false;
}

fn noteDemandStartLocked(device_index: usize, page_lba: u64) void {
    if (device_index >= MAX_DEVICES) return;
    if (read_ahead_states[device_index].demand(page_lba / PAGE_SECTORS)) {
        stats.read_ahead_cancellations +%= 1;
    }
}

fn noteDemandHitLocked(index: usize) void {
    if (index >= entries.len or !entries[index].valid) return;
    policy_index.touchClean(index);
    if (!entries[index].read_ahead or entries[index].device_index >= MAX_DEVICES) return;
    entries[index].read_ahead = false;
    read_ahead_states[entries[index].device_index].consumeResident();
    stats.read_ahead_hits +%= 1;
}

fn scheduleReadAheadLocked(device_index: usize, lba: u64, count: u32) void {
    if (!SPECULATIVE_READ_AHEAD_ENABLED or
        !policy_worker_started or
        device_index >= MAX_DEVICES or
        count < READ_AHEAD_TRIGGER_SECTORS) return;
    const count_u64: u64 = count;
    const end_lba = lba +% count_u64;
    if (end_lba < lba or end_lba == 0) return;
    const last_page = pageLba(end_lba - 1);
    const next_page_lba = last_page +% PAGE_SECTORS;
    if (next_page_lba <= last_page) return;
    const device = block.get(device_index) orelse return;
    if (device.sector_size != SECTOR_SIZE) return;
    const capacity = refreshCapacityLocked();
    if (capacity.read_ahead_pages == 0) return;
    if (device.sector_count != 0 and next_page_lba >= device.sector_count) return;
    const resident_budget = readAheadResidentBudget(capacity);
    if (resident_budget == 0 or
        read_ahead_states[device_index].resident_pages >= resident_budget)
    {
        stats.read_ahead_budget_skips +%= 1;
        return;
    }

    stats.read_ahead_requests +%= 1;
    if (findEntry(device_index, next_page_lba) != null) {
        stats.read_ahead_budget_skips +%= 1;
        return;
    }
    if (read_ahead_states[device_index].schedule(next_page_lba / PAGE_SECTORS, 1)) {
        stats.read_ahead_cancellations +%= 1;
    }
    stats.read_ahead_pages_scheduled +%= 1;
    policy_event.signal();
}

fn readAheadResidentBudget(capacity: page_cache_policy.Capacity) u16 {
    if (capacity.read_ahead_pages == 0) return 0;
    return @min(READ_AHEAD_RESIDENT_LIMIT, capacity.read_ahead_pages);
}

fn readAheadFreeFloor(capacity: page_cache_policy.Capacity) u16 {
    return @max(
        READ_AHEAD_FREE_FLOOR_MIN,
        @min(READ_AHEAD_FREE_FLOOR_MAX, capacity.active_pages / 8),
    );
}

fn readAheadOne() bool {
    const guard = acquireLock() orelse return false;
    var locked = true;
    defer if (locked) releaseLock(guard);
    const unwind = enterOperation() orelse return false;
    defer _ = task_context.leaveUnwind(unwind);

    var probes: usize = 0;
    while (probes < MAX_DEVICES) : (probes += 1) {
        const device_index = (@as(usize, read_ahead_device_cursor) + probes) % MAX_DEVICES;
        var state = &read_ahead_states[device_index];
        if (!state.pending or state.inflight) continue;
        read_ahead_device_cursor = @intCast((device_index + 1) % MAX_DEVICES);
        const request = state.begin() orelse continue;
        const capacity = refreshCapacityLocked();
        const resident_budget = readAheadResidentBudget(capacity);
        if (resident_budget == 0 or
            request.pages == 0 or
            request.pages > resident_budget -| state.resident_pages or
            policy_index.entryCount() + request.pages > capacity.active_pages or
            policy_index.free_count <= readAheadFreeFloor(capacity))
        {
            _ = state.complete(request, false);
            stats.read_ahead_budget_skips +%= 1;
            continue;
        }
        const first_page_lba = request.page * PAGE_SECTORS;
        if (findEntry(device_index, first_page_lba) != null) {
            _ = state.complete(request, false);
            stats.read_ahead_budget_skips +%= 1;
            continue;
        }

        var staging: ?[]u8 = null;
        if (request.pages >= 2) {
            releaseLock(guard);
            locked = false;
            staging = heap.alloc(@as(usize, request.pages) * PAGE_BYTES, 16);
            relock(guard);
            locked = true;
        }
        var outcome: FillRunOutcome = .{};
        if (request.pages >= 2 and staging != null) {
            outcome = fillContiguousRunLocked(
                guard,
                device_index,
                first_page_lba,
                request.pages,
                staging.?,
                true,
            );
        } else if (request.pages == 1) {
            const index = createReadAheadEntryLocked(device_index, first_page_lba);
            if (index) |slot| {
                outcome = .{
                    .ok = fillEntryWithBackendPolicy(guard, slot),
                    .pages = 1,
                    .sectors = PAGE_SECTORS,
                };
            }
        }
        const publish = state.complete(request, outcome.ok);
        if (publish) {
            var page_offset: u16 = 0;
            while (page_offset < outcome.pages) : (page_offset += 1) {
                const page_lba = first_page_lba + @as(u64, page_offset) * PAGE_SECTORS;
                const index = findEntry(device_index, page_lba) orelse continue;
                if (!entries[index].read_ahead) {
                    entries[index].read_ahead = true;
                    state.resident_pages += 1;
                    stats.read_ahead_issued +%= 1;
                    stats.read_ahead_pages_issued +%= 1;
                }
            }
        } else {
            discardReadAheadRunLocked(device_index, first_page_lba, outcome.pages);
            if (staging == null and request.pages >= 2) stats.read_ahead_budget_skips +%= 1;
        }
        if (staging) |memory| {
            releaseLock(guard);
            locked = false;
            _ = heap.free(memory);
            relock(guard);
            locked = true;
        }
        return true;
    }
    return false;
}

fn createReadAheadEntryLocked(device_index: usize, page_lba_value: u64) ?usize {
    const capacity = refreshCapacityLocked();
    if (policy_index.entryCount() >= capacity.active_pages or
        policy_index.free_count <= readAheadFreeFloor(capacity)) return null;
    const index = policy_index.claimFree() orelse return null;
    const kept_phys = entries[index].phys_addr;
    entries[index] = .{
        .valid = true,
        .device_index = device_index,
        .page_lba = page_lba_value,
        .last_use = nextClock(),
        .phys_addr = kept_phys,
    };
    if (entries[index].phys_addr == 0) {
        const phys_addr = mem_phys.allocFrame() orelse {
            entries[index] = .{};
            _ = policy_index.release(index);
            return null;
        };
        entries[index].phys_addr = phys_addr;
        payload_frame_count +%= 1;
        stats.payload_allocations +%= 1;
    }
    if (payloadFrame(index) == null or !policy_index.attachClean(index, device_index)) {
        releasePayload(index, false);
        entries[index] = .{};
        _ = policy_index.release(index);
        return null;
    }
    linkEntry(index);
    return index;
}

fn discardReadAheadRunLocked(device_index: usize, first_page_lba: u64, pages: u16) void {
    var page_offset: u16 = 0;
    while (page_offset < pages) : (page_offset += 1) {
        const page_lba = first_page_lba + @as(u64, page_offset) * PAGE_SECTORS;
        const index = findEntry(device_index, page_lba) orelse continue;
        if (!entries[index].io_busy and entries[index].dirty_mask == 0) clearEntry(index, false);
    }
}

fn dropReadAheadResidencyLocked(index: usize) void {
    if (index >= entries.len or !entries[index].read_ahead) return;
    entries[index].read_ahead = false;
    if (entries[index].device_index < MAX_DEVICES) {
        read_ahead_states[entries[index].device_index].consumeResident();
    }
}

fn updateDirtyHighWater() void {
    const dirty = dirty_entry_count;
    if (dirty > stats.dirty_high_water_entries) stats.dirty_high_water_entries = dirty;
    if (dirty > stats.writeback_queue_high_water) stats.writeback_queue_high_water = dirty;
}

fn nextClock() u64 {
    clock +%= 1;
    return clock;
}

fn nextDirtySequence() u64 {
    dirty_clock +%= 1;
    if (dirty_clock == 0) dirty_clock = 1;
    return dirty_clock;
}

fn nextDirtyWriteSequence() u64 {
    dirty_write_clock +%= 1;
    if (dirty_write_clock == 0) dirty_write_clock = 1;
    return dirty_write_clock;
}
