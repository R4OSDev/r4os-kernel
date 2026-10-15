// Stateless facade; native Rust owns the complete NTFS kernel adapter.

const std = @import("std");

const nv = @import("ntfs_volume");

const NAME_MAX: usize = 768;

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
    mount_ref: ?@import("../../storage/access_state.zig").MountRef = null,
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

pub const StorageLocation = struct { device: usize, first: u64, count: u64 };

pub const DirectoryCursor = nv.DirectoryCursor;

pub const DirectoryNextStatus = nv.DirectoryNextStatus;

pub const ReplaceResult = nv.ReplaceResult;

const CVolume = extern struct {
    state_slot: u8,
    cluster_bytes: u32,
    total_sectors: u64,
};

const CEntry = extern struct {
    name: [768]u8,
    name_len: usize,
    attr: u8,
    reparse: bool,
    record: u64,
    sequence: u16,
    size: u64,
    created_time: u16,
    created_date: u16,
    access_date: u16,
    modified_time: u16,
    modified_date: u16,
};

const CArgs = extern struct {
    volume: CVolume,
    entry: ?*const CEntry,
    buffer: CBuffer,
    name: CView,
    other: CView,
    third: CView,
    number: u64,
    second: u64,
    count: u64,
    device: usize,
    state: ?*anyopaque,
    flag: bool,
};

const CView = extern struct {
    data: ?[*]const u8,
    len: usize,
};

const CBuffer = extern struct {
    data: ?[*]u8,
    len: usize,
};

const CSummary = extern struct {
    version: u32,
    active_volumes: u32,
    slot_capacity: u32,
    bytes_per_volume: u32,
    record_capacity: u32,
    attribute_capacity: u32,
    index_capacity: u32,
    path_capacity: u32,
    mount_generation: u64,
    content_generation: u64,
    negative_ttl_ticks: u64,
    record_entries: u32,
    attribute_entries: u32,
    index_entries: u32,
    path_entries: u32,
    record_hits: u64,
    record_misses: u64,
    record_stores: u64,
    record_evictions: u64,
    attribute_hits: u64,
    attribute_misses: u64,
    attribute_stores: u64,
    attribute_evictions: u64,
    index_hits: u64,
    index_misses: u64,
    index_stores: u64,
    index_evictions: u64,
    path_queries: u64,
    path_positive_hits: u64,
    path_negative_hits: u64,
    path_misses: u64,
    path_positive_stores: u64,
    path_negative_stores: u64,
    path_expirations: u64,
    lookup_tree_walks: u64,
    recovery_cache_bypasses: u64,
    mount_invalidations: u64,
    mutation_invalidations: u64,
    external_invalidations: u64,
    invalidated_entries: u64,
    payload_write_retentions: u64,
    system_write_retentions: u64,
    targeted_invalidations: u64,
    targeted_record_invalidations: u64,
    targeted_attribute_invalidations: u64,
    targeted_directory_invalidations: u64,
    global_mutation_invalidations: u64,
    recovery_invalidations: u64,
    mutation_invalidated_record_entries: u64,
    mutation_invalidated_attribute_entries: u64,
    mutation_invalidated_index_entries: u64,
    mutation_invalidated_path_entries: u64,
    reclaim_requests: u64,
    reclaim_scans: u64,
    reclaimed_entries: u64,
};

const CReclaim = extern struct {
    requested_entries: u32,
    inspected_entries: u32,
    reclaimed_entries: u32,
};

const CProgress = extern struct {
    status: u8,
    completed: usize,
    uncertain: bool,
};

const CLocation = extern struct { device: usize, first: u64, count: u64 };

const Result = extern struct { value: u64, status: i32, present: bool };

extern fn r4os_rust_ntfs_adapter(op: u8, input: *const CArgs, out: ?*anyopaque) callconv(.c) Result;

comptime {
    if (@sizeOf(CVolume) != 16 or @alignOf(CVolume) != 8) @compileError("NTFS adapter AdapterMounted layout");

    if (@offsetOf(CVolume, "state_slot") != 0) @compileError("NTFS adapter AdapterMounted.state_slot offset");

    if (@offsetOf(CVolume, "cluster_bytes") != 4) @compileError("NTFS adapter AdapterMounted.cluster_bytes offset");

    if (@offsetOf(CVolume, "total_sectors") != 8) @compileError("NTFS adapter AdapterMounted.total_sectors offset");

    if (@sizeOf(CEntry) != 824 or @alignOf(CEntry) != 8) @compileError("NTFS adapter AdapterEntry layout");

    if (@offsetOf(CEntry, "name") != 0) @compileError("NTFS adapter AdapterEntry.name offset");

    if (@offsetOf(CEntry, "name_len") != 768) @compileError("NTFS adapter AdapterEntry.name_len offset");

    if (@offsetOf(CEntry, "attr") != 776) @compileError("NTFS adapter AdapterEntry.attr offset");

    if (@offsetOf(CEntry, "reparse") != 777) @compileError("NTFS adapter AdapterEntry.reparse offset");

    if (@offsetOf(CEntry, "record") != 784) @compileError("NTFS adapter AdapterEntry.record offset");

    if (@offsetOf(CEntry, "sequence") != 792) @compileError("NTFS adapter AdapterEntry.sequence offset");

    if (@offsetOf(CEntry, "size") != 800) @compileError("NTFS adapter AdapterEntry.size offset");

    if (@offsetOf(CEntry, "created_time") != 808) @compileError("NTFS adapter AdapterEntry.created_time offset");

    if (@offsetOf(CEntry, "created_date") != 810) @compileError("NTFS adapter AdapterEntry.created_date offset");

    if (@offsetOf(CEntry, "access_date") != 812) @compileError("NTFS adapter AdapterEntry.access_date offset");

    if (@offsetOf(CEntry, "modified_time") != 814) @compileError("NTFS adapter AdapterEntry.modified_time offset");

    if (@offsetOf(CEntry, "modified_date") != 816) @compileError("NTFS adapter AdapterEntry.modified_date offset");

    if (@sizeOf(CArgs) != 136 or @alignOf(CArgs) != 8) @compileError("NTFS adapter AdapterArgs layout");

    if (@offsetOf(CArgs, "volume") != 0) @compileError("NTFS adapter AdapterArgs.volume offset");

    if (@offsetOf(CArgs, "entry") != 16) @compileError("NTFS adapter AdapterArgs.entry offset");

    if (@offsetOf(CArgs, "buffer") != 24) @compileError("NTFS adapter AdapterArgs.buffer offset");

    if (@offsetOf(CArgs, "name") != 40) @compileError("NTFS adapter AdapterArgs.name offset");

    if (@offsetOf(CArgs, "other") != 56) @compileError("NTFS adapter AdapterArgs.other offset");

    if (@offsetOf(CArgs, "third") != 72) @compileError("NTFS adapter AdapterArgs.third offset");

    if (@offsetOf(CArgs, "number") != 88) @compileError("NTFS adapter AdapterArgs.number offset");

    if (@offsetOf(CArgs, "second") != 96) @compileError("NTFS adapter AdapterArgs.second offset");

    if (@offsetOf(CArgs, "count") != 104) @compileError("NTFS adapter AdapterArgs.count offset");

    if (@offsetOf(CArgs, "device") != 112) @compileError("NTFS adapter AdapterArgs.device offset");

    if (@offsetOf(CArgs, "state") != 120) @compileError("NTFS adapter AdapterArgs.state offset");

    if (@offsetOf(CArgs, "flag") != 128) @compileError("NTFS adapter AdapterArgs.flag offset");

    if (@sizeOf(CView) != 16 or @alignOf(CView) != 8) @compileError("NTFS adapter View layout");

    if (@offsetOf(CView, "data") != 0) @compileError("NTFS adapter View.data offset");

    if (@offsetOf(CView, "len") != 8) @compileError("NTFS adapter View.len offset");

    if (@sizeOf(CBuffer) != 16 or @alignOf(CBuffer) != 8) @compileError("NTFS adapter Buffer layout");

    if (@offsetOf(CBuffer, "data") != 0) @compileError("NTFS adapter Buffer.data offset");

    if (@offsetOf(CBuffer, "len") != 8) @compileError("NTFS adapter Buffer.len offset");

    if (@sizeOf(CSummary) != 392 or @alignOf(CSummary) != 8) @compileError("NTFS adapter MetadataCacheSummary layout");

    if (@offsetOf(CSummary, "version") != 0) @compileError("NTFS adapter MetadataCacheSummary.version offset");

    if (@offsetOf(CSummary, "active_volumes") != 4) @compileError("NTFS adapter MetadataCacheSummary.active_volumes offset");

    if (@offsetOf(CSummary, "slot_capacity") != 8) @compileError("NTFS adapter MetadataCacheSummary.slot_capacity offset");

    if (@offsetOf(CSummary, "bytes_per_volume") != 12) @compileError("NTFS adapter MetadataCacheSummary.bytes_per_volume offset");

    if (@offsetOf(CSummary, "record_capacity") != 16) @compileError("NTFS adapter MetadataCacheSummary.record_capacity offset");

    if (@offsetOf(CSummary, "attribute_capacity") != 20) @compileError("NTFS adapter MetadataCacheSummary.attribute_capacity offset");

    if (@offsetOf(CSummary, "index_capacity") != 24) @compileError("NTFS adapter MetadataCacheSummary.index_capacity offset");

    if (@offsetOf(CSummary, "path_capacity") != 28) @compileError("NTFS adapter MetadataCacheSummary.path_capacity offset");

    if (@offsetOf(CSummary, "mount_generation") != 32) @compileError("NTFS adapter MetadataCacheSummary.mount_generation offset");

    if (@offsetOf(CSummary, "content_generation") != 40) @compileError("NTFS adapter MetadataCacheSummary.content_generation offset");

    if (@offsetOf(CSummary, "negative_ttl_ticks") != 48) @compileError("NTFS adapter MetadataCacheSummary.negative_ttl_ticks offset");

    if (@offsetOf(CSummary, "record_entries") != 56) @compileError("NTFS adapter MetadataCacheSummary.record_entries offset");

    if (@offsetOf(CSummary, "attribute_entries") != 60) @compileError("NTFS adapter MetadataCacheSummary.attribute_entries offset");

    if (@offsetOf(CSummary, "index_entries") != 64) @compileError("NTFS adapter MetadataCacheSummary.index_entries offset");

    if (@offsetOf(CSummary, "path_entries") != 68) @compileError("NTFS adapter MetadataCacheSummary.path_entries offset");

    if (@offsetOf(CSummary, "record_hits") != 72) @compileError("NTFS adapter MetadataCacheSummary.record_hits offset");

    if (@offsetOf(CSummary, "record_misses") != 80) @compileError("NTFS adapter MetadataCacheSummary.record_misses offset");

    if (@offsetOf(CSummary, "record_stores") != 88) @compileError("NTFS adapter MetadataCacheSummary.record_stores offset");

    if (@offsetOf(CSummary, "record_evictions") != 96) @compileError("NTFS adapter MetadataCacheSummary.record_evictions offset");

    if (@offsetOf(CSummary, "attribute_hits") != 104) @compileError("NTFS adapter MetadataCacheSummary.attribute_hits offset");

    if (@offsetOf(CSummary, "attribute_misses") != 112) @compileError("NTFS adapter MetadataCacheSummary.attribute_misses offset");

    if (@offsetOf(CSummary, "attribute_stores") != 120) @compileError("NTFS adapter MetadataCacheSummary.attribute_stores offset");

    if (@offsetOf(CSummary, "attribute_evictions") != 128) @compileError("NTFS adapter MetadataCacheSummary.attribute_evictions offset");

    if (@offsetOf(CSummary, "index_hits") != 136) @compileError("NTFS adapter MetadataCacheSummary.index_hits offset");

    if (@offsetOf(CSummary, "index_misses") != 144) @compileError("NTFS adapter MetadataCacheSummary.index_misses offset");

    if (@offsetOf(CSummary, "index_stores") != 152) @compileError("NTFS adapter MetadataCacheSummary.index_stores offset");

    if (@offsetOf(CSummary, "index_evictions") != 160) @compileError("NTFS adapter MetadataCacheSummary.index_evictions offset");

    if (@offsetOf(CSummary, "path_queries") != 168) @compileError("NTFS adapter MetadataCacheSummary.path_queries offset");

    if (@offsetOf(CSummary, "path_positive_hits") != 176) @compileError("NTFS adapter MetadataCacheSummary.path_positive_hits offset");

    if (@offsetOf(CSummary, "path_negative_hits") != 184) @compileError("NTFS adapter MetadataCacheSummary.path_negative_hits offset");

    if (@offsetOf(CSummary, "path_misses") != 192) @compileError("NTFS adapter MetadataCacheSummary.path_misses offset");

    if (@offsetOf(CSummary, "path_positive_stores") != 200) @compileError("NTFS adapter MetadataCacheSummary.path_positive_stores offset");

    if (@offsetOf(CSummary, "path_negative_stores") != 208) @compileError("NTFS adapter MetadataCacheSummary.path_negative_stores offset");

    if (@offsetOf(CSummary, "path_expirations") != 216) @compileError("NTFS adapter MetadataCacheSummary.path_expirations offset");

    if (@offsetOf(CSummary, "lookup_tree_walks") != 224) @compileError("NTFS adapter MetadataCacheSummary.lookup_tree_walks offset");

    if (@offsetOf(CSummary, "recovery_cache_bypasses") != 232) @compileError("NTFS adapter MetadataCacheSummary.recovery_cache_bypasses offset");

    if (@offsetOf(CSummary, "mount_invalidations") != 240) @compileError("NTFS adapter MetadataCacheSummary.mount_invalidations offset");

    if (@offsetOf(CSummary, "mutation_invalidations") != 248) @compileError("NTFS adapter MetadataCacheSummary.mutation_invalidations offset");

    if (@offsetOf(CSummary, "external_invalidations") != 256) @compileError("NTFS adapter MetadataCacheSummary.external_invalidations offset");

    if (@offsetOf(CSummary, "invalidated_entries") != 264) @compileError("NTFS adapter MetadataCacheSummary.invalidated_entries offset");

    if (@offsetOf(CSummary, "payload_write_retentions") != 272) @compileError("NTFS adapter MetadataCacheSummary.payload_write_retentions offset");

    if (@offsetOf(CSummary, "system_write_retentions") != 280) @compileError("NTFS adapter MetadataCacheSummary.system_write_retentions offset");

    if (@offsetOf(CSummary, "targeted_invalidations") != 288) @compileError("NTFS adapter MetadataCacheSummary.targeted_invalidations offset");

    if (@offsetOf(CSummary, "targeted_record_invalidations") != 296) @compileError("NTFS adapter MetadataCacheSummary.targeted_record_invalidations offset");

    if (@offsetOf(CSummary, "targeted_attribute_invalidations") != 304) @compileError("NTFS adapter MetadataCacheSummary.targeted_attribute_invalidations offset");

    if (@offsetOf(CSummary, "targeted_directory_invalidations") != 312) @compileError("NTFS adapter MetadataCacheSummary.targeted_directory_invalidations offset");

    if (@offsetOf(CSummary, "global_mutation_invalidations") != 320) @compileError("NTFS adapter MetadataCacheSummary.global_mutation_invalidations offset");

    if (@offsetOf(CSummary, "recovery_invalidations") != 328) @compileError("NTFS adapter MetadataCacheSummary.recovery_invalidations offset");

    if (@offsetOf(CSummary, "mutation_invalidated_record_entries") != 336) @compileError("NTFS adapter MetadataCacheSummary.mutation_invalidated_record_entries offset");

    if (@offsetOf(CSummary, "mutation_invalidated_attribute_entries") != 344) @compileError("NTFS adapter MetadataCacheSummary.mutation_invalidated_attribute_entries offset");

    if (@offsetOf(CSummary, "mutation_invalidated_index_entries") != 352) @compileError("NTFS adapter MetadataCacheSummary.mutation_invalidated_index_entries offset");

    if (@offsetOf(CSummary, "mutation_invalidated_path_entries") != 360) @compileError("NTFS adapter MetadataCacheSummary.mutation_invalidated_path_entries offset");

    if (@offsetOf(CSummary, "reclaim_requests") != 368) @compileError("NTFS adapter MetadataCacheSummary.reclaim_requests offset");

    if (@offsetOf(CSummary, "reclaim_scans") != 376) @compileError("NTFS adapter MetadataCacheSummary.reclaim_scans offset");

    if (@offsetOf(CSummary, "reclaimed_entries") != 384) @compileError("NTFS adapter MetadataCacheSummary.reclaimed_entries offset");

    if (@sizeOf(CReclaim) != 12 or @alignOf(CReclaim) != 4) @compileError("NTFS adapter MetadataReclaimResult layout");

    if (@offsetOf(CReclaim, "requested_entries") != 0) @compileError("NTFS adapter MetadataReclaimResult.requested_entries offset");

    if (@offsetOf(CReclaim, "inspected_entries") != 4) @compileError("NTFS adapter MetadataReclaimResult.inspected_entries offset");

    if (@offsetOf(CReclaim, "reclaimed_entries") != 8) @compileError("NTFS adapter MetadataReclaimResult.reclaimed_entries offset");

    if (@sizeOf(CProgress) != 24 or @alignOf(CProgress) != 8) @compileError("NTFS adapter WriteProgress layout");

    if (@offsetOf(CProgress, "status") != 0) @compileError("NTFS adapter WriteProgress.status offset");

    if (@offsetOf(CProgress, "completed") != 8) @compileError("NTFS adapter WriteProgress.completed offset");

    if (@offsetOf(CProgress, "uncertain") != 16) @compileError("NTFS adapter WriteProgress.uncertain offset");
}

fn project(comptime T: type, value: anytype) T {
    return switch (@typeInfo(T)) {
        .@"struct" => blk: {
            var out: T = undefined;
            inline for (std.meta.fields(T)) |f| @field(out, f.name) = project(f.type, @field(value, f.name));
            break :blk out;
        },
        .@"enum" => @enumFromInt(value),
        else => @as(T, value),
    };
}
fn view(b: []const u8) CView {
    return .{ .data = b.ptr, .len = b.len };
}
fn buffer(b: []u8) CBuffer {
    return .{ .data = b.ptr, .len = b.len };
}
fn args() CArgs {
    return std.mem.zeroes(CArgs);
}
fn run(volume: Volume, op: u8, input: CArgs, out: ?*anyopaque) Result {
    var a = input;
    a.volume = .{ .state_slot = volume.state_slot, .cluster_bytes = volume.cluster_bytes, .total_sectors = volume.total_sectors };
    return r4os_rust_ntfs_adapter(op, &a, out);
}
fn global(op: u8, a: CArgs, out: ?*anyopaque) Result {
    return r4os_rust_ntfs_adapter(op, &a, out);
}
fn optional(r: Result) ?usize {
    return if (r.present) @intCast(r.value) else null;
}
pub fn inspect(device_index: usize, first_lba: u32) ?Volume {
    return inspectMode(device_index, first_lba, 0, false);
}
pub fn inspectBounded(device_index: usize, first_lba: u32, partition_sectors: u64) ?Volume {
    return inspectMode(device_index, first_lba, partition_sectors, true);
}
fn inspectMode(device: usize, first: u32, count: u64, bounded: bool) ?Volume {
    var a = args();
    a.device = device;
    a.number = first;
    a.count = count;
    var out: CVolume = undefined;
    if (!global(if (bounded) 1 else 0, a, &out).present) return null;
    return .{ .state_slot = out.state_slot, .cluster_bytes = out.cluster_bytes, .total_sectors = out.total_sectors };
}
pub fn storageLocation(volume: Volume) ?StorageLocation {
    var out: CLocation = undefined;
    return if (run(volume, 2, args(), &out).present) project(StorageLocation, out) else null;
}
pub fn forgetStorage(device: usize, first: u64, count: u64) void {
    var a = args();
    a.device = device;
    a.number = first;
    a.count = count;
    _ = global(3, a, null);
}
pub fn invalidateMetadataForDevice(device_index: usize) void {
    var a = args();
    a.device = device_index;
    _ = global(4, a, null);
}
pub fn reclaimMetadataCacheEntries(requested_entries_raw: u32) nv.MetadataReclaimResult {
    var a = args();
    a.count = requested_entries_raw;
    var out: CReclaim = undefined;
    _ = global(5, a, &out);
    return project(nv.MetadataReclaimResult, out);
}
pub fn metadataCacheSummary() nv.MetadataCacheSummary {
    var out: CSummary = undefined;
    _ = global(6, args(), &out);
    return project(nv.MetadataCacheSummary, out);
}
pub fn resolvePathStatus(volume: Volume, path: []const u8, out: *u64) LookupStatus {
    var a = args();
    a.name = view(path);
    const r = run(volume, 7, a, null);
    if (r.status == 0) out.* = r.value;
    return @enumFromInt(r.status);
}
pub fn resolvePath(volume: Volume, path: []const u8) ?u64 {
    var n: u64 = 0;
    return if (resolvePathStatus(volume, path, &n) == .found) n else null;
}
pub fn resolveParent(volume: Volume, dir_path: []const u8) ?u64 {
    return resolvePath(volume, dir_path);
}
pub fn resolveEntryStatus(volume: Volume, path: []const u8, out: *Entry) LookupStatus {
    var a = args();
    a.name = view(path);
    return entryResult(volume, 8, a, out);
}
pub fn resolveEntry(volume: Volume, path: []const u8) ?Entry {
    var e: Entry = undefined;
    return if (resolveEntryStatus(volume, path, &e) == .found) e else null;
}
fn entryResult(volume: Volume, op: u8, a: CArgs, out: *Entry) LookupStatus {
    var e: CEntry = undefined;
    const r = run(volume, op, a, &e);
    if (r.status == 0) out.* = project(Entry, e);
    return @enumFromInt(r.status);
}
pub fn lookupEntryStatus(volume: Volume, parent: u64, name: []const u8, out: *Entry) LookupStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    return entryResult(volume, 9, a, out);
}
pub fn lookupRecoveryEntryStatus(volume: Volume, parent: u64, name: []const u8, out: *Entry) LookupStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    return entryResult(volume, 10, a, out);
}
pub fn readFile(volume: Volume, entry: Entry, out: []u8) ?usize {
    return readMode(volume, entry, 0, out, true);
}
pub fn readFileRange(volume: Volume, entry: Entry, offset: usize, out: []u8) ?usize {
    return readMode(volume, entry, offset, out, false);
}
fn readMode(volume: Volume, entry: Entry, offset: usize, out: []u8, whole: bool) ?usize {
    var e = project(CEntry, entry);
    var a = args();
    a.entry = &e;
    a.number = offset;
    a.buffer = buffer(out);
    return optional(run(volume, if (whole) 11 else 12, a, null));
}
pub fn readDirectory(volume: Volume, dir_record: u64, out: []u8) ?usize {
    var a = args();
    a.number = dir_record;
    a.buffer = buffer(out);
    return optional(run(volume, 13, a, null));
}
pub fn readDirectoryEntryStatus(volume: Volume, dir_record: u64, index: usize, out: []u8, entry_out: *Entry) LookupStatus {
    var a = args();
    a.number = dir_record;
    a.second = index;
    a.buffer = buffer(out);
    return entryResult(volume, 14, a, entry_out);
}
pub fn readDirectoryEntry(volume: Volume, dir_record: u64, index: usize, out: []u8) ?Entry {
    var e: Entry = undefined;
    return if (readDirectoryEntryStatus(volume, dir_record, index, out, &e) == .found) e else null;
}
pub fn nextDirectoryEntry(volume: Volume, dir: u64, cursor: *DirectoryCursor, out: *Entry) DirectoryNextStatus {
    var a = args();
    a.number = dir;
    a.state = cursor;
    var e: CEntry = undefined;
    const r = run(volume, 15, a, &e);
    if (r.value == 0) out.* = project(Entry, e);
    return @enumFromInt(r.value);
}
pub fn listRoot(volume: Volume) bool {
    return run(volume, 16, args(), null).present;
}
pub fn rootRecord() u64 {
    return global(38, args(), null).value;
}
fn writeName(volume: Volume, op: u8, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    a.other = view(data);
    return @enumFromInt(run(volume, op, a, null).status);
}
pub fn writeFileStatus(volume: Volume, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    return writeName(volume, 17, parent, name, data);
}
pub fn writeFile(volume: Volume, parent: u64, name: []const u8, data: []const u8) bool {
    return writeFileStatus(volume, parent, name, data) == .ok;
}
pub fn createFileStatus(volume: Volume, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    return writeName(volume, 18, parent, name, data);
}
pub fn createFile(volume: Volume, parent: u64, name: []const u8, data: []const u8) bool {
    return createFileStatus(volume, parent, name, data) == .ok;
}
fn appendMode(volume: Volume, parent: u64, name: []const u8, offset: u64, data: []const u8, durable: bool) WriteStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    a.second = offset;
    a.other = view(data);
    return @enumFromInt(run(volume, if (durable) 19 else 20, a, null).status);
}
pub fn appendFileAtOffset(volume: Volume, parent: u64, name: []const u8, expected_offset: u64, data: []const u8) nv.WriteStatus {
    return appendMode(volume, parent, name, expected_offset, data, true);
}
pub fn appendFileAtOffsetDeferred(volume: Volume, parent: u64, name: []const u8, expected_offset: u64, data: []const u8) nv.WriteStatus {
    return appendMode(volume, parent, name, expected_offset, data, false);
}
pub fn appendDiagnosticStage() u32 {
    return @intCast(global(21, args(), null).value);
}
pub fn lookupDiagnosticStage() u32 {
    return @intCast(global(22, args(), null).value);
}
pub fn flushVolume(volume: Volume) bool {
    return run(volume, 23, args(), null).present;
}
pub fn deleteFileStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    return writeName(volume, 24, parent, name, &.{});
}
pub fn deleteFile(volume: Volume, parent: u64, name: []const u8) bool {
    return deleteFileStatus(volume, parent, name) == .ok;
}
fn deleteIdentity(volume: Volume, op: u8, parent: u64, name: []const u8, expected: Entry) Result {
    var e = project(CEntry, expected);
    var a = args();
    a.entry = &e;
    a.number = parent;
    a.name = view(name);
    return run(volume, op, a, null);
}
pub fn deleteFileIfIdentity(volume: Volume, parent: u64, name: []const u8, expected: Entry) DeleteIfIdentityResult {
    return @enumFromInt(deleteIdentity(volume, 25, parent, name, expected).status);
}
pub fn deleteRecoveryEntryIfIdentity(volume: Volume, parent: u64, name: []const u8, expected: Entry) RecoveryDeleteResult {
    return @enumFromInt(deleteIdentity(volume, 26, parent, name, expected).status);
}
pub fn namesEqualCollated(volume: Volume, a: []const u8, b: []const u8) ?bool {
    var x = args();
    x.name = view(a);
    x.other = view(b);
    const r = run(volume, 27, x, null);
    return if (r.present) r.value != 0 else null;
}
pub fn makeDirectoryStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    return writeName(volume, 28, parent, name, &.{});
}
pub fn makeDirectory(volume: Volume, parent: u64, name: []const u8) bool {
    return makeDirectoryStatus(volume, parent, name) == .ok;
}
pub fn removeDirectoryStatus(volume: Volume, parent: u64, name: []const u8) WriteStatus {
    return writeName(volume, 29, parent, name, &.{});
}
pub fn removeDirectory(volume: Volume, parent: u64, name: []const u8) bool {
    return removeDirectoryStatus(volume, parent, name) == .ok;
}
pub fn renameEntryStatus(volume: Volume, parent: u64, old_name: []const u8, new_name: []const u8) RenameStatus {
    var a = args();
    a.number = parent;
    a.name = view(old_name);
    a.other = view(new_name);
    return @enumFromInt(run(volume, 30, a, null).status);
}
pub fn renameEntry(volume: Volume, parent: u64, old_name: []const u8, new_name: []const u8) bool {
    return renameEntryStatus(volume, parent, old_name, new_name) == .ok;
}
pub fn moveEntryStatus(volume: Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) WriteStatus {
    var a = args();
    a.number = old_parent;
    a.name = view(old_name);
    a.second = new_parent;
    a.other = view(new_name);
    return @enumFromInt(run(volume, 31, a, null).status);
}
pub fn moveEntry(volume: Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) bool {
    return moveEntryStatus(volume, old_parent, old_name, new_parent, new_name) == .ok;
}
pub fn freeClusterCount(volume: Volume) ?u32 {
    const r = run(volume, 32, args(), null);
    return if (r.present) @intCast(r.value) else null;
}
fn replace(volume: Volume, op: u8, parent: u64, target: []const u8, stage: []const u8, backup: []const u8, consume: bool) ReplaceResult {
    var a = args();
    a.number = parent;
    a.name = view(target);
    a.other = view(stage);
    a.third = view(backup);
    a.flag = consume;
    return @enumFromInt(run(volume, op, a, null).status);
}
pub fn replaceFileAtomic(volume: Volume, parent: u64, target_name: []const u8, staged_name: []const u8, backup_name: []const u8, consume_stage: bool) ReplaceResult {
    return replace(volume, 33, parent, target_name, staged_name, backup_name, consume_stage);
}
pub fn publishFileCreateOnly(volume: Volume, parent: u64, target_name: []const u8, staged_name: []const u8) ReplaceResult {
    return replace(volume, 34, parent, target_name, staged_name, &.{}, false);
}
pub fn replaceFileAtomicCreateOnly(volume: Volume, parent: u64, target_name: []const u8, staged_name: []const u8, backup_name: []const u8) ReplaceResult {
    return replace(volume, 35, parent, target_name, staged_name, backup_name, false);
}
pub fn childSizeStatus(volume: Volume, parent: u64, name: []const u8, out: *u64) LookupStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    const r = run(volume, 36, a, null);
    if (r.status == 0) out.* = r.value;
    return @enumFromInt(r.status);
}
pub fn childSize(volume: Volume, parent: u64, name: []const u8) u64 {
    var out: u64 = 0;
    return if (childSizeStatus(volume, parent, name, &out) == .found) out else 0;
}
fn writeFileRangeProgress(volume: Volume, entry: Entry, offset: usize, data: []const u8) nv.WriteProgress {
    var e = project(CEntry, entry);
    var a = args();
    a.entry = &e;
    a.number = offset;
    a.name = view(data);
    var out: CProgress = undefined;
    _ = run(volume, 37, a, &out);
    return project(nv.WriteProgress, out);
}
pub fn writeFileRangeStatus(volume: Volume, entry: Entry, offset: usize, data: []const u8) WriteStatus {
    return writeFileRangeProgress(volume, entry, offset, data).status;
}
pub fn writeFileRange(volume: Volume, entry: Entry, offset: usize, data: []const u8) ?usize {
    const r = writeFileRangeProgress(volume, entry, offset, data);
    return if (r.status == .ok or (r.completed != 0 and r.completed < data.len)) r.completed else null;
}
