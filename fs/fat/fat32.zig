// Stateless private C facade. Native FAT owns all state, parsing, namespace,
// chain allocation, I/O, recovery and durability; raw logging stays here.
const k = @import("../../kernel/log.zig");
const page_cache = @import("../page_cache.zig");
const SECTOR_SIZE: usize = 512;
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_ARCHIVE: u8 = 0x20;
// Windows-parity name limits (0.60.19): FAT LFN carries at most 255 UTF-16
// units (20 LFN entries); the UTF-8 (BMP) worst case is 765 bytes, buffered
// as 768.  NAME_MAX stays the BYTE limit for UTF-8 name buffers; unit
// buffers use NAME_UNITS_MAX.
const NAME_UNITS_MAX: usize = 255;
const NAME_MAX: usize = 768;

pub const Volume = struct {
    mount_ref: ?@import("../../storage/access_state.zig").MountRef = null,
    device_index: usize,
    partition_lba: u32,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_count: u8,
    sectors_per_fat: u32,
    total_sectors: u32,
    root_cluster: u32,
    fs_info_sector: u16,
    backup_boot_sector: u16,
    // Non-zero only in a regular mutation. Recovery/ownership operations
    // deliberately retain zero and therefore keep their full-device flush
    // boundary for lost-completion reconciliation.
    write_batch: page_cache.WriteBatch = page_cache.NO_WRITE_BATCH,

    pub fn clusterBytes(self: Volume) u32 {
        const c = cv(self);
        return @intCast(r4os_rust_fat_geometry(1, &c));
    }
    pub fn totalClusters(self: Volume) u32 {
        const c = cv(self);
        return @intCast(r4os_rust_fat_geometry(2, &c));
    }
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    first_cluster: u32 = 0,
    // Short directory-entry location observed under the filesystem gate.
    // Unlike cluster zero, it distinguishes empty files and their aliases.
    directory_lba: ?u64 = null,
    directory_offset: u16 = 0,
    size: u32 = 0,
    created_time: u16 = 0,
    created_date: u16 = 0,
    access_date: u16 = 0,
    modified_time: u16 = 0,
    modified_date: u16 = 0,

    pub fn isDir(self: Entry) bool {
        return r4os_rust_vfs_attribute(0, self.attr);
    }

    pub fn isReadOnly(self: Entry) bool {
        return r4os_rust_vfs_attribute(1, self.attr);
    }

    pub fn isHiddenOrSystem(self: Entry) bool {
        return r4os_rust_vfs_attribute(2, self.attr);
    }
};

pub const AppendStatus = enum(u8) {
    ok,
    invalid,
    not_found,
    offset_mismatch,
    too_large,
    io,
};

pub const LookupStatus = enum(u8) {
    found,
    not_found,
    io,
};

pub const RenameStatus = enum(u8) {
    ok,
    not_found,
    not_atomic,
    conflict,
    read_only,
    io,
};

/// Result of the update-only same-directory ownership transfer.  This
/// primitive never copies payload bytes and never frees a cluster chain.  A
/// caller-owned durable journal decides whether the retained backup is later
/// kept, restored, or deleted.
pub const AtomicReplaceResult = enum(i32) {
    ok = 0,
    invalid = -1,
    not_found = -2,
    alias = -3,
    conflict = -4,
    read_only = -5,
    io = -6,
    not_atomic = -7,
};

pub const DeleteIfIdentityResult = enum(u8) {
    deleted,
    not_found,
    mismatch,
    io,
};

pub const Operation = enum(u32) {
    none = 0,
    resolve_entry = 1,
    resolve_path = 2,
    read_file = 3,
    read_range = 4,
    write_range = 5,
    write_file = 6,
    append_file = 7,
    copy_file = 8,
    make_directory = 9,
    delete_file = 10,
    remove_directory = 11,
    rename_entry = 12,
    set_attributes = 13,
};

pub const Summary = extern struct {
    read_sectors: u64 = 0,
    write_sectors: u64 = 0,
    read_failures: u64 = 0,
    write_failures: u64 = 0,
    flushes: u64 = 0,
    flush_failures: u64 = 0,
    flush_total_ticks: u64 = 0,
    flush_max_ticks: u64 = 0,
    flush_last_ticks: u64 = 0,
    file_writes: u64 = 0,
    file_appends: u64 = 0,
    file_write_ranges: u64 = 0,
    file_write_bytes: u64 = 0,
    file_append_bytes: u64 = 0,
    dir_scans: u64 = 0,
    dir_entries_scanned: u64 = 0,
    dir_entry_updates: u64 = 0,
    cluster_walk_steps: u64 = 0,
    fat_reads: u64 = 0,
    fat_writes: u64 = 0,
    fat_mirror_writes: u64 = 0,
    alloc_chain_calls: u64 = 0,
    alloc_clusters: u64 = 0,
    alloc_search_steps: u64 = 0,
    alloc_runs: u64 = 0,
    alloc_run_clusters: u64 = 0,
    alloc_run_max_clusters: u64 = 0,
    fat_sector_writes: u64 = 0,
    read_extent_cache_hits: u64 = 0,
    read_extent_cache_misses: u64 = 0,
    read_extent_cache_stores: u64 = 0,
    read_extent_cache_clusters: u64 = 0,
    fsinfo_reads: u64 = 0,
    fsinfo_valid_mounts: u64 = 0,
    fsinfo_rebuilds: u64 = 0,
    fsinfo_writes: u64 = 0,
    inusemap_builds: u64 = 0,
    inusemap_deferred_builds: u64 = 0,
    inusemap_clusters: u64 = 0,
    inusemap_alloc_hits: u64 = 0,
    inusemap_alloc_misses: u64 = 0,
    operation_failures: u64 = 0,
    operation_total_ticks: u64 = 0,
    operation_max_ticks: u64 = 0,
    operation_last_ticks: u64 = 0,
    active_operation: u32 = @intFromEnum(Operation.none),
    last_operation: u32 = @intFromEnum(Operation.none),
    yield_points: u64 = 0,
    yields: u64 = 0,
    yield_skips: u64 = 0,
};

pub const DIR_CHAIN_MAX_CLUSTERS: usize = 4096;
pub const DirectoryCursor = extern struct {
    cluster: u32 = 0,
    successor: u32 = 0,
    steps: u32 = 0,
    offset: u32 = 0,
    state: u32 = 0,
    lfn_len: u32 = 0,
    lfn_active: u8 = 0,
    lfn_total: u8 = 0,
    lfn_expected: u8 = 0,
    lfn_checksum: u8 = 0,
    units: [NAME_UNITS_MAX]u16 = .{0} ** NAME_UNITS_MAX,
};
pub const DirectoryNextStatus = enum { found, not_found, again, io };

pub extern var dir_chain_truncations: u64;
const Ref = @import("../../storage/access_state.zig").MountRef;
const CVolume = extern struct {
    has_ref: bool = false,
    reference: Ref = .{ .slot = 0, .generation = 0 },
    device_index: usize = 0,
    partition_lba: u32 = 0,
    bytes_per_sector: u16 = 0,
    sectors_per_cluster: u8 = 0,
    reserved_sectors: u16 = 0,
    fat_count: u8 = 0,
    sectors_per_fat: u32 = 0,
    total_sectors: u32 = 0,
    root_cluster: u32 = 0,
    fs_info_sector: u16 = 0,
    backup_boot_sector: u16 = 0,
    write_batch: u64 = 0,
};
const CEntry = extern struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    first_cluster: u32 = 0,
    has_directory_lba: bool = false,
    directory_lba: u64 = 0,
    directory_offset: u16 = 0,
    size: u32 = 0,
    created_time: u16 = 0,
    created_date: u16 = 0,
    access_date: u16 = 0,
    modified_time: u16 = 0,
    modified_date: u16 = 0,
};
const Bytes = extern struct { data: ?[*]const u8 = null, len: usize = 0 };
const Buffer = extern struct { data: ?[*]u8 = null, len: usize = 0 };
const Args = extern struct { volume: CVolume = .{}, other: CVolume = .{}, entry: ?*const CEntry = null, peer: ?*const CEntry = null, parent: u32 = 0, offset: usize = 0, index: usize = 0, name: Bytes = .{}, second: Bytes = .{}, third: Bytes = .{}, data: Bytes = .{}, buffer: Buffer = .{}, cursor: ?*DirectoryCursor = null, first: u64 = 0, count: u64 = 0, flag: bool = false, set: u8 = 0, clear: u8 = 0 };
const Result = extern struct { value: u64, status: i32, present: bool };
extern fn r4os_rust_fat_run(u8, *const Args, ?*CEntry, ?*CVolume) Result;
extern fn r4os_rust_fat_geometry(u8, *const CVolume) u64;
extern fn r4os_rust_fat_summary(*Summary) void;
extern fn r4os_rust_vfs_attribute(u8, u8) bool;
fn bytes(b: []const u8) Bytes {
    return .{ .data = b.ptr, .len = b.len };
}
fn buffer(b: []u8) Buffer {
    return .{ .data = b.ptr, .len = b.len };
}
fn cv(v: Volume) CVolume {
    return .{ .has_ref = v.mount_ref != null, .reference = v.mount_ref orelse .{ .slot = 0, .generation = 0 }, .device_index = v.device_index, .partition_lba = v.partition_lba, .bytes_per_sector = v.bytes_per_sector, .sectors_per_cluster = v.sectors_per_cluster, .reserved_sectors = v.reserved_sectors, .fat_count = v.fat_count, .sectors_per_fat = v.sectors_per_fat, .total_sectors = v.total_sectors, .root_cluster = v.root_cluster, .fs_info_sector = v.fs_info_sector, .backup_boot_sector = v.backup_boot_sector, .write_batch = v.write_batch };
}
fn fromCv(v: CVolume) Volume {
    return .{ .mount_ref = if (v.has_ref) v.reference else null, .device_index = v.device_index, .partition_lba = v.partition_lba, .bytes_per_sector = v.bytes_per_sector, .sectors_per_cluster = v.sectors_per_cluster, .reserved_sectors = v.reserved_sectors, .fat_count = v.fat_count, .sectors_per_fat = v.sectors_per_fat, .total_sectors = v.total_sectors, .root_cluster = v.root_cluster, .fs_info_sector = v.fs_info_sector, .backup_boot_sector = v.backup_boot_sector, .write_batch = v.write_batch };
}
fn ce(e: Entry) CEntry {
    return .{ .name = e.name, .name_len = e.name_len, .attr = e.attr, .first_cluster = e.first_cluster, .has_directory_lba = e.directory_lba != null, .directory_lba = e.directory_lba orelse 0, .directory_offset = e.directory_offset, .size = e.size, .created_time = e.created_time, .created_date = e.created_date, .access_date = e.access_date, .modified_time = e.modified_time, .modified_date = e.modified_date };
}
fn fromCe(e: CEntry) Entry {
    return .{ .name = e.name, .name_len = e.name_len, .attr = e.attr, .first_cluster = e.first_cluster, .directory_lba = if (e.has_directory_lba) e.directory_lba else null, .directory_offset = e.directory_offset, .size = e.size, .created_time = e.created_time, .created_date = e.created_date, .access_date = e.access_date, .modified_time = e.modified_time, .modified_date = e.modified_date };
}
fn run(op: u8, args: Args) Result {
    return r4os_rust_fat_run(op, &args, null, null);
}
fn found(op: u8, args: Args, out: *Entry) LookupStatus {
    var e: CEntry = undefined;
    const r = r4os_rust_fat_run(op, &args, &e, null);
    const status: LookupStatus = @enumFromInt(r.status);
    if (status == .found) out.* = fromCe(e);
    return status;
}
fn optional(r: Result) ?usize {
    return if (r.present) @intCast(r.value) else null;
}
fn parseNative(op: u8, device_index: usize, first_lba: u32, count: u64, print: bool) ?Volume {
    var out: CVolume = undefined;
    const r = r4os_rust_fat_run(op, &.{ .volume = .{ .device_index = device_index, .partition_lba = first_lba }, .count = count, .flag = print }, null, &out);
    return if (r.present) fromCv(out) else null;
}
pub fn inspect(device_index: usize, first_lba: u32) ?Volume {
    return parseNative(35, device_index, first_lba, 0, true);
}
pub fn inspectBounded(device_index: usize, first_lba: u32, partition_sectors: u64) ?Volume {
    return parseNative(36, device_index, first_lba, partition_sectors, true);
}
pub fn parse(device_index: usize, first_lba: u32) ?Volume {
    return parseNative(35, device_index, first_lba, 0, false);
}
pub fn parseBounded(device_index: usize, first_lba: u32, partition_sectors: u64) ?Volume {
    return parseNative(36, device_index, first_lba, partition_sectors, false);
}
pub fn forgetStorage(device: usize, first: u64, count: u64) void {
    _ = run(42, .{ .volume = .{ .device_index = device }, .first = first, .count = count });
}
pub fn summary() Summary {
    var out: Summary = undefined;
    r4os_rust_fat_summary(&out);
    return out;
}
pub fn freeClusterCount(volume: Volume) ?u32 {
    const r = run(28, .{ .volume = cv(volume) });
    return if (r.present) @intCast(r.value) else null;
}
pub fn printInfo(volume: Volume) void {
    _ = run(37, .{ .volume = cv(volume) });
}
pub fn listRoot(volume: Volume) bool {
    return run(29, .{ .volume = cv(volume) }).present;
}
pub fn listDirectory(volume: Volume, start_cluster: u32) bool {
    return run(38, .{ .volume = cv(volume), .parent = start_cluster }).present;
}
pub fn readDirectory(volume: Volume, start_cluster: u32, out: []u8) ?usize {
    return optional(run(25, .{ .volume = cv(volume), .parent = start_cluster, .buffer = buffer(out) }));
}
pub fn readDirectoryEntry(volume: Volume, start_cluster: u32, index: usize, out: []u8) ?Entry {
    var e: Entry = undefined;
    return if (readDirectoryEntryStatus(volume, start_cluster, index, out, &e) == .found) e else null;
}
pub fn readDirectoryEntryStatus(volume: Volume, start_cluster: u32, index: usize, out: []u8, entry_out: *Entry) LookupStatus {
    return found(26, .{ .volume = cv(volume), .parent = start_cluster, .index = index, .buffer = buffer(out) }, entry_out);
}
pub fn resolveEntryStatus(volume: Volume, path: []const u8, out: *Entry) LookupStatus {
    return found(1, .{ .volume = cv(volume), .name = bytes(path) }, out);
}
pub fn resolveEntry(volume: Volume, path: []const u8) ?Entry {
    var e: Entry = undefined;
    return if (resolveEntryStatus(volume, path, &e) == .found) e else null;
}
pub fn resolvePathStatus(volume: Volume, path: []const u8, out: *u32) LookupStatus {
    const r = run(0, .{ .volume = cv(volume), .name = bytes(path) });
    const status: LookupStatus = @enumFromInt(r.status);
    if (status == .found) out.* = @intCast(r.value);
    return status;
}
pub fn resolvePath(volume: Volume, path: []const u8) ?u32 {
    var node: u32 = undefined;
    return if (resolvePathStatus(volume, path, &node) == .found) node else null;
}
pub fn findDirectory(volume: Volume, start_cluster: u32, name: []const u8) ?u32 {
    const r = run(40, .{ .volume = cv(volume), .parent = start_cluster, .name = bytes(name) });
    return if (r.present) @intCast(r.value) else null;
}
pub fn findEntry(volume: Volume, start_cluster: u32, name: []const u8) ?Entry {
    var e: Entry = undefined;
    return if (findEntryStatus(volume, start_cluster, name, &e) == .found) e else null;
}
pub fn findEntryStatus(volume: Volume, start_cluster: u32, name: []const u8, out: *Entry) LookupStatus {
    return found(2, .{ .volume = cv(volume), .parent = start_cluster, .name = bytes(name) }, out);
}
pub fn printFile(volume: Volume, entry: Entry, max_bytes: usize) bool {
    const e = ce(entry);
    return run(39, .{ .volume = cv(volume), .entry = &e, .index = max_bytes }).present;
}
pub fn readFile(volume: Volume, entry: Entry, out: []u8) ?usize {
    const e = ce(entry);
    return optional(run(5, .{ .volume = cv(volume), .entry = &e, .buffer = buffer(out) }));
}
pub fn readFileRange(volume: Volume, entry: Entry, offset: usize, out: []u8) ?usize {
    const e = ce(entry);
    return optional(run(6, .{ .volume = cv(volume), .entry = &e, .offset = offset, .buffer = buffer(out) }));
}
pub fn writeFileRange(original_volume: Volume, entry: Entry, offset: usize, data: []const u8) ?usize {
    const e = ce(entry);
    return optional(run(7, .{ .volume = cv(original_volume), .entry = &e, .offset = offset, .data = bytes(data) }));
}
pub fn writeFile(original_volume: Volume, parent_cluster: u32, name: []const u8, data: []const u8) bool {
    return run(8, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(name), .data = bytes(data) }).present;
}
pub fn appendFile(volume: Volume, parent_cluster: u32, name: []const u8, data: []const u8) bool {
    return run(9, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(name), .data = bytes(data) }).present;
}
pub fn appendFileAtOffset(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) bool {
    return appendFileAtOffsetStatus(volume, parent_cluster, name, expected_size, data) == .ok;
}
pub fn appendFileAtOffsetStatus(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) AppendStatus {
    return @enumFromInt(run(10, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(name), .offset = expected_size, .data = bytes(data) }).status);
}
pub fn appendFileAtOffsetStatusDeferred(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) AppendStatus {
    return @enumFromInt(run(11, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(name), .offset = expected_size, .data = bytes(data) }).status);
}
pub fn publishCopyFile(original: Volume, parent: u32, staged: []const u8, target: []const u8) bool {
    return run(12, .{ .volume = cv(original), .parent = parent, .name = bytes(staged), .second = bytes(target) }).present;
}
pub fn copyFile(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent_cluster: u32, dst_name: []const u8) bool {
    const e = ce(src_entry);
    return run(13, .{ .volume = cv(src_volume), .other = cv(dst_volume), .entry = &e, .parent = dst_parent_cluster, .name = bytes(dst_name), .flag = true }).present;
}
pub fn copyFileNoReplace(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent_cluster: u32, dst_name: []const u8) bool {
    const e = ce(src_entry);
    return run(13, .{ .volume = cv(src_volume), .other = cv(dst_volume), .entry = &e, .parent = dst_parent_cluster, .name = bytes(dst_name), .flag = false }).present;
}
pub fn sameFileForCopy(left: Entry, right: Entry) bool {
    const l = ce(left);
    const r = ce(right);
    return run(31, .{ .entry = &l, .peer = &r }).present;
}
pub fn makeDirectory(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    return run(15, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(name) }).present;
}
pub fn deleteFile(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    return run(16, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(name) }).present;
}
pub fn deleteFileIfIdentity(volume: Volume, parent_cluster: u32, name: []const u8, expected: Entry) DeleteIfIdentityResult {
    const e = ce(expected);
    return @enumFromInt(run(17, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(name), .entry = &e }).status);
}
pub fn removeDirectory(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    return run(20, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(name) }).present;
}
pub fn validateShortName83(name: []const u8) bool {
    return run(30, .{ .name = bytes(name) }).present;
}
pub fn replaceFileAtomic(volume: Volume, parent_cluster: u32, target_name: []const u8, staged_name: []const u8, backup_name: []const u8, consume_stage: bool) AtomicReplaceResult {
    return @enumFromInt(run(22, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(target_name), .second = bytes(staged_name), .third = bytes(backup_name), .flag = consume_stage }).status);
}
pub fn replaceFileAtomicCreateOnly(volume: Volume, parent_cluster: u32, target_name: []const u8, staged_name: []const u8, backup_name: []const u8) AtomicReplaceResult {
    return @enumFromInt(run(23, .{ .volume = cv(volume), .parent = parent_cluster, .name = bytes(target_name), .second = bytes(staged_name), .third = bytes(backup_name) }).status);
}
pub fn renameEntry(volume: Volume, parent_cluster: u32, old_name: []const u8, new_name: []const u8) bool {
    return renameEntryStatus(volume, parent_cluster, old_name, new_name) == .ok;
}
pub fn renameEntryStatus(original_volume: Volume, parent_cluster: u32, old_name: []const u8, new_name: []const u8) RenameStatus {
    return @enumFromInt(run(21, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(old_name), .second = bytes(new_name) }).status);
}
pub fn setAttributes(original_volume: Volume, parent_cluster: u32, name: []const u8, set_mask: u8, clear_mask: u8) bool {
    return run(41, .{ .volume = cv(original_volume), .parent = parent_cluster, .name = bytes(name), .set = set_mask, .clear = clear_mask }).present;
}
pub fn nextDirectoryEntry(volume: Volume, start: u32, cursor: *DirectoryCursor, out: *Entry) DirectoryNextStatus {
    var e: CEntry = undefined;
    const r = r4os_rust_fat_run(27, &.{ .volume = cv(volume), .parent = start, .cursor = cursor }, &e, null);
    const status: DirectoryNextStatus = @enumFromInt(r.status);
    if (status == .found) out.* = fromCe(e);
    return status;
}
pub fn flushVolume(volume: Volume) bool {
    return run(24, .{ .volume = cv(volume) }).present;
}
pub fn namesEqualCollated(volume: Volume, a: []const u8, b: []const u8) ?bool {
    const r = run(19, .{ .volume = cv(volume), .name = bytes(a), .second = bytes(b) });
    return if (r.present) r.value != 0 else null;
}
export fn r4os_zig_fat_text(data: [*]const u8, len: usize) void {
    k.puts(data[0..len]);
}
export fn r4os_zig_fat_decimal(value: u64) void {
    k.putDec(value);
}
export fn r4os_zig_fat_char(value: u8) void {
    k.putc(value);
}
comptime {
    _ = @import("../../storage/block.zig");
    _ = @import("directory_growth.zig");
}

comptime {
    if (@sizeOf(CVolume) != 72 or @alignOf(CVolume) != 8) @compileError("Native FAT Volume layout changed");
    if (@offsetOf(CVolume, "has_ref") != 0) @compileError("Native FAT Volume.has_ref offset changed");
    if (@offsetOf(CVolume, "reference") != 8) @compileError("Native FAT Volume.reference offset changed");
    if (@offsetOf(CVolume, "device_index") != 24) @compileError("Native FAT Volume.device_index offset changed");
    if (@offsetOf(CVolume, "partition_lba") != 32) @compileError("Native FAT Volume.partition_lba offset changed");
    if (@offsetOf(CVolume, "bytes_per_sector") != 36) @compileError("Native FAT Volume.bytes_per_sector offset changed");
    if (@offsetOf(CVolume, "sectors_per_cluster") != 38) @compileError("Native FAT Volume.sectors_per_cluster offset changed");
    if (@offsetOf(CVolume, "reserved_sectors") != 40) @compileError("Native FAT Volume.reserved_sectors offset changed");
    if (@offsetOf(CVolume, "fat_count") != 42) @compileError("Native FAT Volume.fat_count offset changed");
    if (@offsetOf(CVolume, "sectors_per_fat") != 44) @compileError("Native FAT Volume.sectors_per_fat offset changed");
    if (@offsetOf(CVolume, "total_sectors") != 48) @compileError("Native FAT Volume.total_sectors offset changed");
    if (@offsetOf(CVolume, "root_cluster") != 52) @compileError("Native FAT Volume.root_cluster offset changed");
    if (@offsetOf(CVolume, "fs_info_sector") != 56) @compileError("Native FAT Volume.fs_info_sector offset changed");
    if (@offsetOf(CVolume, "backup_boot_sector") != 58) @compileError("Native FAT Volume.backup_boot_sector offset changed");
    if (@offsetOf(CVolume, "write_batch") != 64) @compileError("Native FAT Volume.write_batch offset changed");
    if (@sizeOf(CEntry) != 824 or @alignOf(CEntry) != 8) @compileError("Native FAT Entry layout changed");
    if (@offsetOf(CEntry, "name") != 0) @compileError("Native FAT Entry.name offset changed");
    if (@offsetOf(CEntry, "name_len") != 768) @compileError("Native FAT Entry.name_len offset changed");
    if (@offsetOf(CEntry, "attr") != 776) @compileError("Native FAT Entry.attr offset changed");
    if (@offsetOf(CEntry, "first_cluster") != 780) @compileError("Native FAT Entry.first_cluster offset changed");
    if (@offsetOf(CEntry, "has_directory_lba") != 784) @compileError("Native FAT Entry.has_directory_lba offset changed");
    if (@offsetOf(CEntry, "directory_lba") != 792) @compileError("Native FAT Entry.directory_lba offset changed");
    if (@offsetOf(CEntry, "directory_offset") != 800) @compileError("Native FAT Entry.directory_offset offset changed");
    if (@offsetOf(CEntry, "size") != 804) @compileError("Native FAT Entry.size offset changed");
    if (@offsetOf(CEntry, "created_time") != 808) @compileError("Native FAT Entry.created_time offset changed");
    if (@offsetOf(CEntry, "created_date") != 810) @compileError("Native FAT Entry.created_date offset changed");
    if (@offsetOf(CEntry, "access_date") != 812) @compileError("Native FAT Entry.access_date offset changed");
    if (@offsetOf(CEntry, "modified_time") != 814) @compileError("Native FAT Entry.modified_time offset changed");
    if (@offsetOf(CEntry, "modified_date") != 816) @compileError("Native FAT Entry.modified_date offset changed");
    if (@sizeOf(Bytes) != 16 or @alignOf(Bytes) != 8) @compileError("Native FAT Bytes layout changed");
    if (@offsetOf(Bytes, "data") != 0) @compileError("Native FAT Bytes.data offset changed");
    if (@offsetOf(Bytes, "len") != 8) @compileError("Native FAT Bytes.len offset changed");
    if (@sizeOf(Buffer) != 16 or @alignOf(Buffer) != 8) @compileError("Native FAT Buffer layout changed");
    if (@offsetOf(Buffer, "data") != 0) @compileError("Native FAT Buffer.data offset changed");
    if (@offsetOf(Buffer, "len") != 8) @compileError("Native FAT Buffer.len offset changed");
    if (@sizeOf(Args) != 296 or @alignOf(Args) != 8) @compileError("Native FAT Args layout changed");
    if (@offsetOf(Args, "volume") != 0) @compileError("Native FAT Args.volume offset changed");
    if (@offsetOf(Args, "other") != 72) @compileError("Native FAT Args.other offset changed");
    if (@offsetOf(Args, "entry") != 144) @compileError("Native FAT Args.entry offset changed");
    if (@offsetOf(Args, "peer") != 152) @compileError("Native FAT Args.peer offset changed");
    if (@offsetOf(Args, "parent") != 160) @compileError("Native FAT Args.parent offset changed");
    if (@offsetOf(Args, "offset") != 168) @compileError("Native FAT Args.offset offset changed");
    if (@offsetOf(Args, "index") != 176) @compileError("Native FAT Args.index offset changed");
    if (@offsetOf(Args, "name") != 184) @compileError("Native FAT Args.name offset changed");
    if (@offsetOf(Args, "second") != 200) @compileError("Native FAT Args.second offset changed");
    if (@offsetOf(Args, "third") != 216) @compileError("Native FAT Args.third offset changed");
    if (@offsetOf(Args, "data") != 232) @compileError("Native FAT Args.data offset changed");
    if (@offsetOf(Args, "buffer") != 248) @compileError("Native FAT Args.buffer offset changed");
    if (@offsetOf(Args, "cursor") != 264) @compileError("Native FAT Args.cursor offset changed");
    if (@offsetOf(Args, "first") != 272) @compileError("Native FAT Args.first offset changed");
    if (@offsetOf(Args, "count") != 280) @compileError("Native FAT Args.count offset changed");
    if (@offsetOf(Args, "flag") != 288) @compileError("Native FAT Args.flag offset changed");
    if (@offsetOf(Args, "set") != 289) @compileError("Native FAT Args.set offset changed");
    if (@offsetOf(Args, "clear") != 290) @compileError("Native FAT Args.clear offset changed");
    if (@sizeOf(Summary) != 392 or @alignOf(Summary) != 8 or @sizeOf(DirectoryCursor) != 540) @compileError("Native FAT summary/cursor layout changed");
}
