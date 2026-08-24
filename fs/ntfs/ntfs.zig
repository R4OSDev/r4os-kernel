// NTFS 3.1 kernel driver (read since 0.60.4, write phase 1 since 0.60.6).
//
// Thin adapter over the shared ntfs_volume logic (E9 "one truth"): this file
// owns mount state, the page-cache device seam and the VFS vocabulary; all
// on-disk parsing, tree walks and write ordering live in ntfs_volume.
//
// Concurrency: every operation runs under the fs-request gate (or the
// single-threaded boot path).  The module-level scratch and volume construction
// rely on that serialization.

const ntfs = @import("ntfs_format");
const nv = @import("ntfs_volume");
const page_cache = @import("../page_cache.zig");
const heap = @import("../../memory/heap.zig");
const timer = @import("../../kernel/timer.zig");
const time_core = @import("../../platform/time.zig");
const k = @import("../../kernel/log.zig");

const SECTOR_SIZE: usize = 512;
// Bound staging ownership while still preserving normal 1/2-MB NTFS extents
// as one or two page-cache/block submissions instead of 2,048/4,096 seam
// calls. The block core still splits each submission at the backend limit.
const MAX_DIRECT_WRITE_SECTORS: u32 = 2048;
const NAME_MAX: usize = 768; // matches vfs.NAME_MAX / nv.NAME_MAX (0.60.19)
const MAX_VOLUMES: usize = 4;
const MAX_MFT_RUNS: usize = nv.MAX_MFT_RUNS;
const METADATA_NEGATIVE_TTL_TICKS: u64 = timer.DEFAULT_HZ;
const METADATA_RECLAIM_ENTRY_BUDGET: u32 = 8;

pub const ATTR_READ_ONLY: u8 = nv.ATTR_READ_ONLY;
pub const ATTR_HIDDEN: u8 = nv.ATTR_HIDDEN;
pub const ATTR_SYSTEM: u8 = nv.ATTR_SYSTEM;
pub const ATTR_DIRECTORY: u8 = nv.ATTR_DIRECTORY;
pub const ATTR_ARCHIVE: u8 = nv.ATTR_ARCHIVE;

pub const WriteStatus = nv.WriteStatus;
pub const LookupStatus = nv.LookupStatus;

pub const RenameStatus = enum(u8) {
    ok,
    not_found,
    not_atomic,
    conflict,
    io,
};

pub const DeleteIfIdentityResult = enum(u8) {
    deleted,
    not_found,
    mismatch,
    io,
};

/// Outcome of the recovery-only compare-and-delete (0.60.21).  `unlinked`
/// reports that only a surplus index alias was detached and the object is
/// still reachable under its canonical name.
pub const RecoveryDeleteResult = enum(u8) {
    deleted,
    unlinked,
    not_found,
    mismatch,
    directory,
    unsupported,
    io,
};

pub const Volume = struct {
    state_slot: u8,
    cluster_bytes: u32,
    total_sectors: u64,
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    reparse: bool = false,
    record: u64 = 0,
    sequence: u16 = 0,
    size: u64 = 0,
    created_time: u16 = 0,
    created_date: u16 = 0,
    access_date: u16 = 0,
    modified_time: u16 = 0,
    modified_date: u16 = 0,

    pub fn isDir(self: Entry) bool {
        return (self.attr & ATTR_DIRECTORY) != 0;
    }
};

const VolumeState = struct {
    in_use: bool = false,
    device_index: usize = 0,
    partition_lba: u32 = 0,
    cluster_bytes: u32 = 0,
    record_bytes: u32 = 0,
    index_block_bytes: u32 = 0,
    total_sectors: u64 = 0,
    mft_runs: [MAX_MFT_RUNS]ntfs.Run = undefined,
    mft_run_count: usize = 0,
    upcase: ?[]u8 = null,
    metadata_cache: nv.MetadataCache = .{},
};

var states: [MAX_VOLUMES]VolumeState = .{VolumeState{}} ** MAX_VOLUMES;
var scratch: nv.Scratch = .{};
var metadata_reclaim_volume_cursor: usize = 0;

// ---------------------------------------------------------------------------
// Page-cache device seam
// ---------------------------------------------------------------------------

const DeviceCtx = struct { device_index: usize = 0 };
var device_ctxs: [MAX_VOLUMES]DeviceCtx = .{DeviceCtx{}} ** MAX_VOLUMES;

fn seamRead(ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool {
    const c: *DeviceCtx = @ptrCast(@alignCast(ctx));
    return page_cache.readSectors(c.device_index, lba, count, out);
}

fn seamWrite(ctx: *anyopaque, lba: u64, count: u32, data: []const u8) bool {
    const c: *DeviceCtx = @ptrCast(@alignCast(ctx));
    if (count == 0) return true;
    const total_bytes = @as(usize, count) * SECTOR_SIZE;
    if (data.len < total_bytes) return false;
    if (count == 1) return page_cache.writeSector(c.device_index, lba, data[0..SECTOR_SIZE]);

    // writeSectorsDirect owns a staged copy, pins every overlapping cache
    // identity, and invalidates exactly the backend-confirmed prefix. Thus a
    // short/failed write is visible to the shared NTFS recovery logic without
    // leaving stale cache lines, while seamFlush remains the durability and
    // crash-order barrier.
    var done: u32 = 0;
    while (done < count) {
        const chunk = @min(count - done, MAX_DIRECT_WRITE_SECTORS);
        const byte_offset = @as(usize, done) * SECTOR_SIZE;
        const byte_count = @as(usize, chunk) * SECTOR_SIZE;
        if (!page_cache.writeSectorsDirect(
            c.device_index,
            lba + done,
            @intCast(chunk),
            data[byte_offset .. byte_offset + byte_count],
        )) return false;
        done += chunk;
    }
    return true;
}

fn seamFlush(ctx: *anyopaque) bool {
    const c: *DeviceCtx = @ptrCast(@alignCast(ctx));
    return page_cache.flushDevice(c.device_index);
}

fn buildVolume(volume: Volume) nv.Volume {
    const state = &states[volume.state_slot];
    const ctx = &device_ctxs[volume.state_slot];
    ctx.device_index = state.device_index;
    return .{
        .device = .{
            .ctx = ctx,
            .read_sectors = seamRead,
            .write_sectors = seamWrite,
            .flush = seamFlush,
        },
        .partition_lba = state.partition_lba,
        .cluster_bytes = state.cluster_bytes,
        .record_bytes = state.record_bytes,
        .index_block_bytes = state.index_block_bytes,
        .total_sectors = state.total_sectors,
        .mft_runs_buf = state.mft_runs[0..],
        .mft_run_count = &state.mft_run_count,
        .upcase = state.upcase orelse &[_]u8{},
        .scratch = &scratch,
        .now_filetime = nowFiletime(),
        .metadata_cache = &state.metadata_cache,
        .metadata_cache_now_ticks = timer.tickCount(),
    };
}

fn nowFiletime() u64 {
    // Wall-clock date/time -> FILETIME (100 ns since 1601-01-01 UTC).  The
    // RTC is read as UTC; R4OS keeps hardware clock in UTC.
    const now = time_core.wallClock();
    if (!now.valid or now.year < 1601) return 0;
    const days = daysFromCivil(now.year, now.month, now.day);
    // days is counted from 1970-01-01; shift to the 1601 epoch.
    const days_1601: i64 = days + 134774;
    if (days_1601 < 0) return 0;
    const secs: u64 = @as(u64, @intCast(days_1601)) * 86400 +
        @as(u64, now.hour) * 3600 + @as(u64, now.minute) * 60 + @as(u64, now.second);
    return secs * 10_000_000;
}

/// Days from 1970-01-01 to the given civil date (Howard Hinnant's algorithm).
fn daysFromCivil(year: u16, month: u8, day: u8) i64 {
    const y: i64 = @as(i64, year) - @intFromBool(month <= 2);
    const era: i64 = @divFloor(if (y >= 0) y else y - 399, 400);
    const yoe: i64 = y - era * 400;
    const m: i64 = month;
    const doy: i64 = @divFloor(153 * (if (m > 2) m - 3 else m + 9) + 2, 5) + @as(i64, day) - 1;
    const doe: i64 = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    return era * 146097 + doe - 719468;
}

// ---------------------------------------------------------------------------
// Mount / probe
// ---------------------------------------------------------------------------

pub fn inspect(device_index: usize, first_lba: u32) ?Volume {
    var slot_index: usize = MAX_VOLUMES;
    for (states, 0..) |state, i| {
        if (state.in_use and state.device_index == device_index and state.partition_lba == first_lba) {
            slot_index = i;
            break;
        }
        if (!state.in_use and slot_index == MAX_VOLUMES) slot_index = i;
    }
    if (slot_index >= MAX_VOLUMES) {
        k.puts("      NTFS: no free volume slot\r\n");
        return null;
    }

    const ctx = &device_ctxs[slot_index];
    ctx.device_index = device_index;
    const device = nv.Device{
        .ctx = ctx,
        .read_sectors = seamRead,
        .write_sectors = seamWrite,
        .flush = seamFlush,
    };

    const state = &states[slot_index];
    const info = nv.mount(device, first_lba, &scratch, state.mft_runs[0..]) orelse {
        k.puts("      NTFS: mount failed (invalid boot sector or unsupported layout)\r\n");
        return null;
    };
    if (info.cluster_bytes / SECTOR_SIZE == 0) return null;

    state.device_index = device_index;
    state.partition_lba = first_lba;
    state.cluster_bytes = info.cluster_bytes;
    state.record_bytes = info.record_bytes;
    state.index_block_bytes = info.index_block_bytes;
    state.total_sectors = info.total_sectors;
    state.mft_run_count = info.mft_run_count;
    state.metadata_cache.beginMount(METADATA_NEGATIVE_TTL_TICKS);
    state.in_use = true;

    var volume = Volume{
        .state_slot = @intCast(slot_index),
        .cluster_bytes = info.cluster_bytes,
        .total_sectors = info.total_sectors,
    };

    // Load the volume $UpCase table.
    if (state.upcase == null) {
        state.upcase = heap.alloc(ntfs.UPCASE_BYTES, 8) orelse {
            k.puts("      NTFS mount failed: upcase alloc\r\n");
            state.in_use = false;
            return null;
        };
    }
    var probe = buildVolume(volume);
    const got = nv.readFileRange(&probe, ntfs.MFT_RECORD_UPCASE, 0, state.upcase.?) orelse {
        return failInspect(state, "upcase read");
    };
    if (got != ntfs.UPCASE_BYTES) return failInspect(state, "upcase size");

    k.puts("      NTFS 3.1: cluster=");
    k.putDec(info.cluster_bytes);
    k.puts(" record=");
    k.putDec(info.record_bytes);
    k.puts(" mft_runs=");
    k.putDec(info.mft_run_count);
    k.puts(" read-write\r\n");
    _ = &volume;
    return volume;
}

fn failInspect(state: *VolumeState, reason: []const u8) ?Volume {
    k.puts("      NTFS mount failed: ");
    k.puts(reason);
    k.puts("\r\n");
    if (state.upcase) |buf| {
        _ = heap.free(buf);
        state.upcase = null;
    }
    state.in_use = false;
    return null;
}

/// Explicit media-generation boundary for future unmount/removable-device
/// owners. Mounted block devices cannot currently unregister, but callers
/// that replace externally visible media must invalidate before reprobe.
pub fn invalidateMetadataForDevice(device_index: usize) void {
    for (&states) |*state| {
        if (!state.in_use or state.device_index != device_index) continue;
        state.metadata_cache.invalidateExternal();
    }
}

/// Drops a bounded number of decoded cache entries under global memory
/// pressure. The fixed cache control storage stays resident; no sector page
/// or mutable filesystem payload is owned here.
pub fn reclaimMetadataCacheEntries(requested_entries_raw: u32) nv.MetadataReclaimResult {
    const requested = @min(requested_entries_raw, METADATA_RECLAIM_ENTRY_BUDGET);
    var result = nv.MetadataReclaimResult{ .requested_entries = requested };
    if (requested == 0) return result;
    var volume_probes: usize = 0;
    while (volume_probes < states.len and result.reclaimed_entries < requested) : (volume_probes += 1) {
        const index = metadata_reclaim_volume_cursor;
        metadata_reclaim_volume_cursor = (metadata_reclaim_volume_cursor + 1) % states.len;
        const state = &states[index];
        if (!state.in_use) continue;
        const remaining = requested - result.reclaimed_entries;
        const one = state.metadata_cache.reclaim(remaining);
        result.inspected_entries +|= one.inspected_entries;
        result.reclaimed_entries +|= one.reclaimed_entries;
    }
    return result;
}

pub fn metadataCacheSummary() nv.MetadataCacheSummary {
    var out = nv.MetadataCacheSummary{};
    out.active_volumes = 0;
    out.mount_generation = 0;
    out.content_generation = 0;
    out.negative_ttl_ticks = 0;
    for (&states) |*state| {
        if (!state.in_use) continue;
        const one = state.metadata_cache.summary();
        out.active_volumes +|= 1;
        out.bytes_per_volume = @max(out.bytes_per_volume, one.bytes_per_volume);
        out.mount_generation = @max(out.mount_generation, one.mount_generation);
        out.content_generation = @max(out.content_generation, one.content_generation);
        out.negative_ttl_ticks = @max(out.negative_ttl_ticks, one.negative_ttl_ticks);
        out.record_entries +|= one.record_entries;
        out.attribute_entries +|= one.attribute_entries;
        out.index_entries +|= one.index_entries;
        out.path_entries +|= one.path_entries;
        out.record_hits +%= one.record_hits;
        out.record_misses +%= one.record_misses;
        out.record_stores +%= one.record_stores;
        out.record_evictions +%= one.record_evictions;
        out.attribute_hits +%= one.attribute_hits;
        out.attribute_misses +%= one.attribute_misses;
        out.attribute_stores +%= one.attribute_stores;
        out.attribute_evictions +%= one.attribute_evictions;
        out.index_hits +%= one.index_hits;
        out.index_misses +%= one.index_misses;
        out.index_stores +%= one.index_stores;
        out.index_evictions +%= one.index_evictions;
        out.path_queries +%= one.path_queries;
        out.path_positive_hits +%= one.path_positive_hits;
        out.path_negative_hits +%= one.path_negative_hits;
        out.path_misses +%= one.path_misses;
        out.path_positive_stores +%= one.path_positive_stores;
        out.path_negative_stores +%= one.path_negative_stores;
        out.path_expirations +%= one.path_expirations;
        out.lookup_tree_walks +%= one.lookup_tree_walks;
        out.recovery_cache_bypasses +%= one.recovery_cache_bypasses;
        out.mount_invalidations +%= one.mount_invalidations;
        out.mutation_invalidations +%= one.mutation_invalidations;
        out.external_invalidations +%= one.external_invalidations;
        out.invalidated_entries +%= one.invalidated_entries;
        out.reclaim_requests +%= one.reclaim_requests;
        out.reclaim_scans +%= one.reclaim_scans;
        out.reclaimed_entries +%= one.reclaimed_entries;
    }
    return out;
}

// ---------------------------------------------------------------------------
// FILETIME -> DOS date/time for the VFS entry contract
// ---------------------------------------------------------------------------

const DosStamp = struct { date: u16, time: u16 };

fn filetimeToDos(filetime: u64) DosStamp {
    if (filetime == 0) return .{ .date = 0, .time = 0 };
    const total_seconds = filetime / 10_000_000;
    const days_1601: u64 = total_seconds / 86400;
    const day_seconds: u64 = total_seconds % 86400;
    const days_1970: i64 = @as(i64, @intCast(days_1601)) - 134774;
    const z = days_1970 + 719468;
    const era = @divFloor(z, 146097);
    const doe = z - era * 146097;
    const yoe = @divFloor(doe - @divFloor(doe, 1460) + @divFloor(doe, 36524) - @divFloor(doe, 146096), 365);
    const y = yoe + era * 400;
    const doy = doe - (365 * yoe + @divFloor(yoe, 4) - @divFloor(yoe, 100));
    const mp = @divFloor(5 * doy + 2, 153);
    const d = doy - @divFloor(153 * mp + 2, 5) + 1;
    const m = if (mp < 10) mp + 3 else mp - 9;
    const year = if (m <= 2) y + 1 else y;
    if (year < 1980 or year > 2107) return .{ .date = 0, .time = 0 };
    const hours: u16 = @intCast(day_seconds / 3600);
    const minutes: u16 = @intCast((day_seconds % 3600) / 60);
    const seconds: u16 = @intCast(day_seconds % 60);
    return .{
        .date = (@as(u16, @intCast(year - 1980)) << 9) | (@as(u16, @intCast(m)) << 5) | @as(u16, @intCast(d)),
        .time = (hours << 11) | (minutes << 5) | (seconds / 2),
    };
}

fn entryFromShared(e: nv.Entry) Entry {
    var out = Entry{
        .name_len = e.name_len,
        .attr = e.attr,
        .reparse = e.reparse,
        .record = e.record,
        .sequence = e.sequence,
        .size = e.size,
    };
    out.name = e.name;
    const created = filetimeToDos(e.created_time_nt);
    out.created_date = created.date;
    out.created_time = created.time;
    const modified = filetimeToDos(e.modified_time_nt);
    out.modified_date = modified.date;
    out.modified_time = modified.time;
    const accessed = filetimeToDos(e.access_time_nt);
    out.access_date = accessed.date;
    return out;
}

// ---------------------------------------------------------------------------
// Read-side VFS surface
// ---------------------------------------------------------------------------

pub fn resolvePathStatus(volume: Volume, path: []const u8, out: *u64) LookupStatus {
    var v = buildVolume(volume);
    return nv.resolvePathStatus(&v, path, out);
}

pub fn resolvePath(volume: Volume, path: []const u8) ?u64 {
    var record: u64 = undefined;
    return if (resolvePathStatus(volume, path, &record) == .found) record else null;
}

pub fn resolveEntryStatus(volume: Volume, path: []const u8, out: *Entry) LookupStatus {
    var v = buildVolume(volume);
    var resolved: nv.ResolvedEntry = undefined;
    const status = nv.resolveEntryStatus(&v, path, &resolved);
    if (status == .found) out.* = entryFromShared(resolved.entry);
    return status;
}

pub fn resolveEntry(volume: Volume, path: []const u8) ?Entry {
    var entry: Entry = undefined;
    return if (resolveEntryStatus(volume, path, &entry) == .found) entry else null;
}

/// Direct child lookup preserving absence vs metadata/I/O failure and the
/// full {record, sequence} identity.
pub fn lookupEntryStatus(volume: Volume, parent: u64, name: []const u8, out: *Entry) LookupStatus {
    var v = buildVolume(volume);
    var found: nv.LookupResult = undefined;
    const status = nv.lookupInDirectoryStatus(&v, parent, name, &found);
    if (status == .found) out.* = entryFromShared(found.entry);
    return status;
}

/// Recovery-only child lookup.  Atomic alias-first replacement deliberately
/// has short durable windows in which an index alias and the canonical
/// $FILE_NAME disagree.  Normal VFS consumers must reject those windows;
/// the journal recovery primitive must inspect their exact record identity
/// in order to complete or reverse them safely.
pub fn lookupRecoveryEntryStatus(volume: Volume, parent: u64, name: []const u8, out: *Entry) LookupStatus {
    var v = buildVolume(volume);
    var found: nv.LookupResult = undefined;
    const status = nv.lookupInDirectoryStatusTransient(&v, parent, name, &found);
    if (status == .found) out.* = entryFromShared(found.entry);
    return status;
}

pub fn readFile(volume: Volume, entry: Entry, out: []u8) ?usize {
    if (entry.isDir()) return null;
    if (entry.reparse) return null; // reparse points are visible rejections
    if (entry.size > out.len) return null;
    var v = buildVolume(volume);
    if (nv.recordIdentityStatus(&v, entry.record, entry.sequence, false) != .found) return null;
    return nv.readFileRange(&v, entry.record, 0, out[0..@intCast(entry.size)]);
}

pub fn readFileRange(volume: Volume, entry: Entry, offset: usize, out: []u8) ?usize {
    if (entry.isDir()) return null;
    if (entry.reparse) return null; // reparse points are visible rejections
    var v = buildVolume(volume);
    if (nv.recordIdentityStatus(&v, entry.record, entry.sequence, false) != .found) return null;
    return nv.readFileRange(&v, entry.record, offset, out);
}

pub fn readDirectory(volume: Volume, dir_record: u64, out: []u8) ?usize {
    var v = buildVolume(volume);
    var sink = nv.EnumSink{ .out = out, .max_entries = 64 };
    if (!appendBytes(out, &sink.cursor, ".\r\n")) return null;
    if (!appendBytes(out, &sink.cursor, "..\r\n")) return null;
    if (!nv.enumerateDirectory(&v, dir_record, &sink)) return null;
    if (sink.cursor < out.len) out[sink.cursor] = 0;
    return sink.cursor;
}

pub fn readDirectoryEntry(volume: Volume, dir_record: u64, index: usize, out: []u8) ?Entry {
    var entry: Entry = undefined;
    return if (readDirectoryEntryStatus(volume, dir_record, index, out, &entry) == .found)
        entry
    else
        null;
}

pub fn readDirectoryEntryStatus(volume: Volume, dir_record: u64, index: usize, out: []u8, entry_out: *Entry) LookupStatus {
    if (index < 2) {
        if (out.len < 3) return .io;
        var entry = Entry{ .name_len = index + 1, .attr = ATTR_DIRECTORY, .record = dir_record };
        entry.name[0] = '.';
        out[0] = '.';
        if (index == 1) {
            entry.name[1] = '.';
            out[1] = '.';
        }
        out[entry.name_len] = 0;
        entry_out.* = entry;
        return .found;
    }
    var v = buildVolume(volume);
    var sink = nv.EnumSink{ .wanted = index - 2 };
    if (!nv.enumerateDirectory(&v, dir_record, &sink)) return .io;
    const found = sink.found orelse return .not_found;
    if (found.name_len + 1 > out.len) return .io;
    @memcpy(out[0..found.name_len], found.name[0..found.name_len]);
    out[found.name_len] = 0;
    entry_out.* = entryFromShared(found);
    return .found;
}

fn appendBytes(out: []u8, cursor: *usize, bytes: []const u8) bool {
    if (cursor.* + bytes.len >= out.len) return false;
    @memcpy(out[cursor.* .. cursor.* + bytes.len], bytes);
    cursor.* += bytes.len;
    return true;
}

pub fn listRoot(volume: Volume) bool {
    k.puts("      NTFS root:\r\n");
    var buffer: [1024]u8 = undefined;
    const len = readDirectory(volume, ntfs.MFT_RECORD_ROOT, buffer[0..]) orelse return false;
    var start: usize = 0;
    var shown: usize = 0;
    var i: usize = 0;
    while (i < len and shown < 12) : (i += 1) {
        if (buffer[i] == '\n') {
            k.puts("        ");
            k.puts(buffer[start .. i + 1]);
            start = i + 1;
            shown += 1;
        }
    }
    return true;
}

pub fn rootRecord() u64 {
    return ntfs.MFT_RECORD_ROOT;
}

// ---------------------------------------------------------------------------
// Write-side VFS surface (phase 1)
// ---------------------------------------------------------------------------

/// Resolves a parent directory record from an absolute path's parent part.
pub fn resolveParent(volume: Volume, dir_path: []const u8) ?u64 {
    var record: u64 = undefined;
    return if (resolvePathStatus(volume, dir_path, &record) == .found) record else null;
}

pub fn writeFileStatus(volume: Volume, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.writeFile(&v, parent, name, data);
}

pub fn writeFile(volume: Volume, parent: u64, name: []const u8, data: []const u8) bool {
    return writeFileStatus(volume, parent, name, data) == .ok;
}

pub fn createFileStatus(volume: Volume, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.createFile(&v, parent, name, data);
}

pub fn createFile(volume: Volume, parent: u64, name: []const u8, data: []const u8) bool {
    return createFileStatus(volume, parent, name, data) == .ok;
}

pub fn appendFileAtOffset(volume: Volume, parent: u64, name: []const u8, expected_offset: u64, data: []const u8) nv.WriteStatus {
    var v = buildVolume(volume);
    return nv.appendFileAtOffset(&v, parent, name, expected_offset, data);
}

/// Stream-batching append (0.60.14): no durable flush per chunk, the
/// dirty flag spans the stream; flushVolume is the durability point.
pub fn appendFileAtOffsetDeferred(volume: Volume, parent: u64, name: []const u8, expected_offset: u64, data: []const u8) nv.WriteStatus {
    var v = buildVolume(volume);
    return nv.appendFileAtOffsetDeferred(&v, parent, name, expected_offset, data);
}

pub fn appendDiagnosticStage() u32 {
    return nv.appendDiagnosticStage();
}

pub fn lookupDiagnosticStage() u32 {
    return nv.lookupDiagnosticStage();
}

/// Stream-finish durability point: drain buffered writes, then clear the
/// dirty flag with its own flush (the clear can never overtake the data).
pub fn flushVolume(volume: Volume) bool {
    var v = buildVolume(volume);
    return nv.finishDeferred(&v);
}

pub fn deleteFileStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.deleteFile(&v, parent, name);
}

pub fn deleteFile(volume: Volume, parent: u64, name: []const u8) bool {
    return deleteFileStatus(volume, parent, name) == .ok;
}

/// Statusful compare-and-delete adapter.  The caller holds the global
/// filesystem-request gate from its content check through this identity
/// recheck.  NTFS record+sequence is the durable identity, so a recycled MFT
/// record or a newly published name can never satisfy a stale delete.
pub fn deleteFileIfIdentity(
    volume: Volume,
    parent: u64,
    name: []const u8,
    expected: Entry,
) DeleteIfIdentityResult {
    var v = buildVolume(volume);
    var found: nv.LookupResult = undefined;
    switch (nv.lookupInDirectoryStatus(&v, parent, name, &found)) {
        .found => {},
        .not_found => return if (nv.finishDeferred(&v)) .not_found else .io,
        .io => return .io,
    }
    if (found.entry.isDir() or
        found.record != expected.record or
        found.sequence != expected.sequence or
        found.entry.size != expected.size)
        return .mismatch;

    const result = nv.deleteFile(&v, parent, name);
    if (result == .ok) return .deleted;
    if (result != .io and result != .not_found) return .mismatch;

    // A lost completion may follow the durable namespace removal.  Absence
    // is accepted only after finishDeferred drains the volume and clears its
    // dirty bracket; otherwise the outcome stays ambiguous.
    switch (nv.lookupInDirectoryStatus(&v, parent, name, &found)) {
        .not_found => return if (nv.finishDeferred(&v)) .deleted else .io,
        .found => return if (found.record == expected.record and
            found.sequence == expected.sequence)
            .io
        else
            .mismatch,
        .io => return .io,
    }
}

/// Recovery-only compare-and-delete adapter (0.60.21).  Journal/claim replay
/// resolves the name through the transient view, so it can also reverse a
/// publish that stopped between the canonical $FILE_NAME rewrite and the
/// target index insert.  The exact {record, sequence} is still mandatory, so
/// a merely equal name or a replaced object is never removed.
pub fn deleteRecoveryEntryIfIdentity(
    volume: Volume,
    parent: u64,
    name: []const u8,
    expected: Entry,
) RecoveryDeleteResult {
    var v = buildVolume(volume);
    const result = nv.deleteRecoveryEntryIfIdentity(
        &v,
        parent,
        name,
        expected.record,
        expected.sequence,
    );
    return switch (result) {
        // The namespace change is only reported once it is durable, matching
        // the plain identity delete above.
        .ok => if (nv.finishDeferred(&v)) .deleted else .io,
        .unlinked => if (nv.finishDeferred(&v)) .unlinked else .io,
        .not_found => if (nv.finishDeferred(&v)) .not_found else .io,
        .mismatch => .mismatch,
        .directory => .directory,
        .unsupported => .unsupported,
        .io => .io,
    };
}

/// Backend-exact name comparison over `$UpCase` (0.60.24).
pub fn namesEqualCollated(volume: Volume, a: []const u8, b: []const u8) ?bool {
    var v = buildVolume(volume);
    return nv.namesEqualCollated(&v, a, b);
}

pub fn makeDirectoryStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.createDirectory(&v, parent, name);
}

pub fn makeDirectory(volume: Volume, parent: u64, name: []const u8) bool {
    return makeDirectoryStatus(volume, parent, name) == .ok;
}

pub fn removeDirectoryStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.deleteDirectory(&v, parent, name);
}

pub fn removeDirectory(volume: Volume, parent: u64, name: []const u8) bool {
    return removeDirectoryStatus(volume, parent, name) == .ok;
}

pub fn renameEntryStatus(volume: Volume, parent: u64, old_name: []const u8, new_name: []const u8) RenameStatus {
    var v = buildVolume(volume);
    return switch (nv.renameEntry(&v, parent, old_name, parent, new_name)) {
        .ok => .ok,
        .not_found => .not_found,
        .exists, .invalid => .conflict,
        // NTFS implements rename. Any other backend result can arise after
        // its dirty bracket or a durable sub-step has begun, so even a
        // low-level "unsupported" is not proof that Copy/Delete is safe.
        .directory, .not_directory, .not_empty, .read_only_target, .no_space, .dir_full, .record_full, .unsupported, .offset_mismatch, .io, .cleanup_failed => .io,
    };
}

pub fn renameEntry(volume: Volume, parent: u64, old_name: []const u8, new_name: []const u8) bool {
    return renameEntryStatus(volume, parent, old_name, new_name) == .ok;
}

pub fn moveEntryStatus(volume: Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) WriteStatus {
    var v = buildVolume(volume);
    return nv.renameEntry(&v, old_parent, old_name, new_parent, new_name);
}

pub fn moveEntry(volume: Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) bool {
    return moveEntryStatus(volume, old_parent, old_name, new_parent, new_name) == .ok;
}

pub const ReplaceResult = nv.ReplaceResult;

pub fn freeClusterCount(volume: Volume) ?u32 {
    var v = buildVolume(volume);
    const free = nv.freeClusterCount(&v) orelse return null;
    const max_u32: u64 = 0xFFFF_FFFF;
    return if (free > max_u32) 0xFFFF_FFFF else @intCast(free);
}

pub fn replaceFileAtomic(volume: Volume, parent: u64, target_name: []const u8, staged_name: []const u8, backup_name: []const u8, consume_stage: bool) ReplaceResult {
    var v = buildVolume(volume);
    return nv.replaceFileAtomic(&v, parent, target_name, staged_name, backup_name, consume_stage);
}

pub fn publishFileCreateOnly(volume: Volume, parent: u64, target_name: []const u8, staged_name: []const u8) ReplaceResult {
    var v = buildVolume(volume);
    return nv.publishFileCreateOnly(&v, parent, target_name, staged_name);
}

/// VFS create-only replace surface.  The compatibility backup argument is
/// never consumed; it must be proven absent through the same statusful NTFS
/// lookup before the publish-before-detach primitive may run.
pub fn replaceFileAtomicCreateOnly(
    volume: Volume,
    parent: u64,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
) ReplaceResult {
    var v = buildVolume(volume);
    var backup: nv.LookupResult = undefined;
    switch (nv.lookupInDirectoryStatus(&v, parent, backup_name, &backup)) {
        .found => return .conflict,
        .not_found => {},
        .io => return .io,
    }
    return nv.publishFileCreateOnly(&v, parent, target_name, staged_name);
}

pub fn childSizeStatus(volume: Volume, parent: u64, name: []const u8, out: *u64) LookupStatus {
    var v = buildVolume(volume);
    var found: nv.LookupResult = undefined;
    const status = nv.lookupInDirectoryStatus(&v, parent, name, &found);
    if (status != .found) return status;
    if (found.entry.isDir()) return .io;
    var attr = nv.AttrScratch{};
    if (nv.collectAttributeStatus(&v, found.record, .data, &[_]u8{}, &attr) != .found) return .io;
    out.* = attr.data_size;
    return .found;
}

/// Current data size of a child, or 0 when absent (used by append-at-end).
pub fn childSize(volume: Volume, parent: u64, name: []const u8) u64 {
    var size: u64 = 0;
    return if (childSizeStatus(volume, parent, name, &size) == .found) size else 0;
}

/// Writes `data` at `offset` within an existing file: in place inside the
/// current content (FAT32-equivalent semantics, no truncate), appending at
/// the exact end, or replacing from offset 0 when the write extends past
/// the end.
pub fn writeFileRangeStatus(volume: Volume, entry: Entry, offset: usize, data: []const u8) WriteStatus {
    if (entry.isDir()) return .directory;
    var v = buildVolume(volume);
    if (nv.recordIdentityStatus(&v, entry.record, entry.sequence, false) != .found) return .io;
    const offset_u64: u64 = @intCast(offset);
    const data_len_u64: u64 = @intCast(data.len);
    if (data_len_u64 > ~@as(u64, 0) - offset_u64) return .offset_mismatch;
    if (offset_u64 + data_len_u64 <= entry.size) {
        // A hard link intentionally shares this data stream.  Pure in-place
        // writes which do not change its size therefore remain supported.
        return nv.writeFileAt(&v, entry.record, offset_u64, data);
    }

    // Append/growth and whole-object replacement need one canonical
    // parent/name.  Preserve `.unsupported` at this adapter boundary rather
    // than silently choosing one name of a hard-linked record.
    const link_status = nv.requireSingleLinkStatus(&v, entry.record, entry.sequence);
    if (link_status != .ok) return link_status;
    const loc = nv.recordParentAndName(&v, entry.record) orelse return .io;
    if (offset_u64 == entry.size) {
        return nv.appendFileAtOffset(&v, loc.parent, loc.name[0..loc.name_len], offset_u64, data);
    }
    if (offset == 0) return nv.writeFile(&v, loc.parent, loc.name[0..loc.name_len], data);
    return .offset_mismatch;
}

pub fn writeFileRange(volume: Volume, entry: Entry, offset: usize, data: []const u8) ?usize {
    // The legacy VFS surface has no status channel.  All non-OK results,
    // including the explicit hard-link `.unsupported`, fail closed here.
    return if (writeFileRangeStatus(volume, entry, offset, data) == .ok) data.len else null;
}
