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
const sync = @import("../sched/sync.zig");
const scheduler = @import("../sched/scheduler.zig");
const task_context = @import("../sched/task_context.zig");

pub const SECTOR_SIZE: usize = 512;
const PAGE_SECTORS: usize = 8;
const PAGE_BYTES: usize = PAGE_SECTORS * SECTOR_SIZE;
const MAX_ENTRIES: usize = 512;
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
    dirty_owner: [PAGE_SECTORS]WriteBatch = .{NO_WRITE_BATCH} ** PAGE_SECTORS,
    dirty_write_sequence: [PAGE_SECTORS]u64 = .{0} ** PAGE_SECTORS,
    phys_addr: u64 = 0,
    next: u16 = NO_INDEX,
};

var entries: [MAX_ENTRIES]Entry = .{Entry{}} ** MAX_ENTRIES;
var buckets: [BUCKET_COUNT]u16 = .{NO_INDEX} ** BUCKET_COUNT;
var stats: Summary = .{};
var clock: u64 = 0;
var dirty_clock: u64 = 0;
var dirty_write_clock: u64 = 0;
var write_batch_clock: WriteBatch = NO_WRITE_BATCH;
var dirty_entry_count: u32 = 0;
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
    };
    clock = 0;
    dirty_clock = 0;
    dirty_write_clock = 0;
    write_batch_clock = NO_WRITE_BATCH;
    dirty_entry_count = 0;
    cache_lock = sync.Mutex.initClass("fs-page-cache", sync.LockRank.fs_page_cache, .sleepable);
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
    var used: u32 = 0;
    var dirty_pages: u32 = 0;
    var dirty_sectors: u64 = 0;
    var clean_pages: u32 = 0;
    var frames: u32 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        const e = entries[index];
        if (e.phys_addr != 0) frames += 1;
        if (!e.valid) continue;
        used += 1;
        if (e.dirty_mask != 0) {
            dirty_pages += 1;
            dirty_sectors += @popCount(e.dirty_mask);
        } else {
            clean_pages += 1;
        }
    }
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
            // Snapshot into resident kernel-stack memory while metadata is
            // locked, then drop every cache owner before touching a pageable
            // caller buffer. preTouch was not a pin: the page could be
            // evicted during the preceding cache/backend wait and fault back
            // into this same io_busy entry.
            @memcpy(caller_copy[0..], frame[off .. off + SECTOR_SIZE]);
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
    var caller_copy: [PAGE_BYTES]u8 = undefined;
    const guard = acquireLock() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    defer releaseLock(guard);
    const unwind = enterOperation() orelse {
        stats.read_errors +%= 1;
        return false;
    };
    defer _ = task_context.leaveUnwind(unwind);

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
                stats.misses +%= span;
                if (!fillEntryWithBackendPolicy(guard, index)) {
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
                entries[index].last_use = nextClock();
                releaseLock(guard);
                @memcpy(out[@as(usize, done) * SECTOR_SIZE ..][0..copy_bytes], caller_copy[0..copy_bytes]);
                relock(guard);
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
            entries[index].last_use = nextClock();
            stats.hits +%= span;
            releaseLock(guard);
            @memcpy(out[@as(usize, done) * SECTOR_SIZE ..][0..copy_bytes], caller_copy[0..copy_bytes]);
            relock(guard);
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
        @memcpy(frame[off .. off + SECTOR_SIZE], caller_copy[0..]);
        entries[index].valid_mask |= bit;
        entries[index].last_use = nextClock();
        markDirty(index, bit, batch);
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
        if (entries[index].io_busy) entries[index].io_busy = false;
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
    const index = selectWritableSlot() orelse return null;
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
    if (ok) {
        entries[index].valid_mask = mask_before | readable_mask;
        entries[index].last_use = nextClock();
        stats.fills +%= 1;
    }
    return ok;
}

fn selectWritableSlot() ?usize {
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (!entries[index].valid) return index;
    }

    stats.reclaim_scans +%= 1;
    const slot = selectCleanSlot() orelse {
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

fn selectCleanSlot() ?usize {
    var best: ?usize = null;
    var best_use: u64 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (!entries[index].valid or entries[index].dirty_mask != 0 or entries[index].io_busy) continue;
        if (best == null or entries[index].last_use < best_use) {
            best = index;
            best_use = entries[index].last_use;
        }
    }
    return best;
}

fn selectCleanPayloadSlot() ?usize {
    var best: ?usize = null;
    var best_use: u64 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (!entries[index].valid or entries[index].dirty_mask != 0 or entries[index].io_busy or entries[index].phys_addr == 0) continue;
        if (best == null or entries[index].last_use < best_use) {
            best = index;
            best_use = entries[index].last_use;
        }
    }
    return best;
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
};

fn drainDevice(guard: bool, device_index: usize, reason: DrainReason) bool {
    if (dirtyEntriesForDevice(device_index) == 0) return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    var written: u64 = 0;
    while (findOldestDirtyForDevice(device_index)) |index| {
        const before = stats.writeback_sectors;
        if (!writebackEntryUnlocked(guard, index)) return false;
        written +%= stats.writeback_sectors - before;
    }
    recordDrain(reason, written, start);
    return true;
}

fn drainDeviceBatch(guard: bool, device_index: usize, batch: WriteBatch, reason: DrainReason) bool {
    if (dirtySectorsForDeviceBatch(device_index, batch) == 0) return true;
    const start = timer.tickCount();
    stats.writeback_waits +%= 1;
    var written: u64 = 0;
    while (findOldestDirtyForDeviceBatch(device_index, batch)) |index| {
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
    while (findOldestDirty()) |index| {
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
    stats.writeback_sectors +%= @popCount(written_bits);
    if (!ok) stats.writeback_errors +%= 1;
    return ok;
}

fn findOldestDirty() ?usize {
    var best: ?usize = null;
    var best_sequence: u64 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (!entries[index].valid or entries[index].dirty_mask == 0) continue;
        if (best == null or entries[index].dirty_sequence < best_sequence) {
            best = index;
            best_sequence = entries[index].dirty_sequence;
        }
    }
    return best;
}

fn findOldestDirtyForDevice(device_index: usize) ?usize {
    var best: ?usize = null;
    var best_sequence: u64 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (!entries[index].valid or entries[index].dirty_mask == 0 or entries[index].device_index != device_index) continue;
        if (best == null or entries[index].dirty_sequence < best_sequence) {
            best = index;
            best_sequence = entries[index].dirty_sequence;
        }
    }
    return best;
}

fn findOldestDirtyForDeviceBatch(device_index: usize, batch: WriteBatch) ?usize {
    var best: ?usize = null;
    var best_sequence: u64 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        const entry = &entries[index];
        if (!entry.valid or entry.device_index != device_index) continue;
        const sequence = oldestDirtySequenceForBatch(entry, batch) orelse continue;
        if (best == null or sequence < best_sequence) {
            best = index;
            best_sequence = sequence;
        }
    }
    return best;
}

fn markDirty(index: usize, bits: u8, owner: WriteBatch) void {
    if (index >= entries.len or !entries[index].valid) return;
    if (entries[index].dirty_mask == 0) {
        entries[index].dirty_sequence = nextDirtySequence();
        dirty_entry_count +%= 1;
    }
    entries[index].dirty_mask |= bits;
    var sector: usize = 0;
    while (sector < PAGE_SECTORS) : (sector += 1) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((bits & bit) == 0) continue;
        entries[index].dirty_owner[sector] = owner;
        entries[index].dirty_write_sequence[sector] = nextDirtyWriteSequence();
    }
    updateDirtyHighWater();
}

fn clearDirtyBits(index: usize, bits: u8) void {
    if (index >= entries.len or bits == 0) return;
    const cleared = entries[index].dirty_mask & bits;
    if (cleared == 0) return;
    entries[index].dirty_mask &= ~cleared;
    page_cache_batch.clearOwnership(
        &entries[index].dirty_owner,
        &entries[index].dirty_write_sequence,
        cleared,
    );
    if (entries[index].dirty_mask == 0) {
        entries[index].dirty_sequence = 0;
        if (dirty_entry_count != 0) dirty_entry_count -= 1;
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
    var dirty: u32 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (entries[index].valid and entries[index].dirty_mask != 0) dirty += 1;
    }
    return dirty;
}

fn dirtyEntriesForDevice(device_index: usize) u32 {
    var dirty: u32 = 0;
    var index: usize = 0;
    while (index < entries.len) : (index += 1) {
        if (entries[index].valid and entries[index].dirty_mask != 0 and entries[index].device_index == device_index) dirty += 1;
    }
    return dirty;
}

fn dirtySectorsForDevice(device_index: usize) u64 {
    var dirty: u64 = 0;
    // Iterate the global array by reference. Entry now carries per-sector
    // ownership metadata; value iteration materializes the complete cache
    // array as a roughly 88-KB stack temporary when this helper is inlined.
    for (&entries) |*entry| {
        if (entry.valid and entry.device_index == device_index) dirty +%= @popCount(entry.dirty_mask);
    }
    return dirty;
}

fn dirtySectorsForDeviceBatch(device_index: usize, batch: WriteBatch) u64 {
    var dirty: u64 = 0;
    for (&entries) |*entry| {
        if (entry.valid and entry.device_index == device_index) dirty +%= @popCount(dirtyMaskForBatch(entry, batch));
    }
    return dirty;
}

fn hasBusyEntry() bool {
    for (&entries) |*entry| {
        if (entry.valid and entry.io_busy) return true;
    }
    return false;
}

fn evictForReplacement(index: usize) void {
    if (index >= entries.len or !entries[index].valid) return;
    stats.evictions +%= 1;
    if (entries[index].dirty_mask == 0) stats.reclaim_clean_entries +%= 1;
    // Frame behalten: der Slot wird sofort wiederbelegt (createEntry
    // uebernimmt phys_addr), das spart Free+Alloc pro Eviction.
    unlinkEntry(index);
    const phys = entries[index].phys_addr;
    entries[index] = .{ .phys_addr = phys };
}

fn clearEntry(index: usize, reclaim: bool) void {
    if (index >= entries.len) return;
    if (entries[index].valid) unlinkEntry(index);
    releasePayload(index, reclaim);
    entries[index] = .{};
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
