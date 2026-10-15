// Private neutral C facade; native Rust owns every VFS state/policy.
const fat32 = @import("fat/fat32.zig");
const ntfs_fs = @import("ntfs/ntfs.zig");
const access = @import("../storage/access_runtime.zig");
pub const NAME_MAX: usize = 768;
pub const MAX_MOUNTED_VOLUMES: usize = 26;

// ABI attribute bits (DOS layout, shared by FAT32 and NTFS file_attributes).
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;

/// Opaque per-filesystem node reference.  FAT32 stores the first cluster,
/// NTFS will store the MFT record reference.  Only the owning filesystem
/// interprets the value; consumers treat it as a directory/file handle.
pub const NodeRef = u64;

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
    io,
};

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

pub const DeleteIfMatchResult = enum(u8) {
    deleted,
    not_found,
    mismatch,
    io,
};

/// Outcome of the recovery-only compare-and-delete (0.60.21).  It extends
/// the plain identity delete by the states that only an alias-first publish
/// window can produce.
pub const RecoveryDeleteResult = enum(u8) {
    deleted,
    /// Only a surplus index alias was detached; the object is still
    /// reachable under its canonical name.
    unlinked,
    not_found,
    mismatch,
    directory,
    /// A half-state the backend deliberately refuses to guess about.
    unsupported,
    io,
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    node: NodeRef = 0,
    // Filesystem-internal generation of `node`. NTFS stores the MFT record
    // sequence here so a recycled record can never satisfy an ownership
    // check. FAT32 has no corresponding generation and leaves it zero.
    node_generation: u16 = 0,
    fat_directory_lba: ?u64 = null,
    fat_directory_offset: u16 = 0,
    // Internal traversal/read guard; not part of any public R4SYS payload.
    reparse: bool = false,
    size: u64 = 0,
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

pub const Volume = union(enum) {
    fat32: fat32.Volume,
    ntfs: ntfs_fs.Volume,
    pub fn accessReference(self: Volume) ?access.MountRef {
        return switch (self) {
            inline else => |v| v.mount_ref,
        };
    }
    pub fn storageRegion(self: Volume) ?access.Region {
        var out: access.Region = undefined;
        const view = cv(self);
        return if (r4os_rust_vfs_mount(6, 0, &view, null, @ptrCast(&out))) out else null;
    }
    pub fn clusterBytes(self: Volume) u32 {
        const v = cv(self);
        return @intCast(r4os_rust_vfs_volume(0, &v));
    }
    pub fn totalClusters(self: Volume) u32 {
        const v = cv(self);
        return @intCast(r4os_rust_vfs_volume(1, &v));
    }
    pub fn rootNode(self: Volume) NodeRef {
        const v = cv(self);
        return r4os_rust_vfs_volume(2, &v);
    }
    pub fn isReadOnly(self: Volume) bool {
        const v = cv(self);
        return r4os_rust_vfs_volume(3, &v) != 0;
    }
};
const Ref = access.MountRef;
const Fat = extern struct {
    device: usize = 0,
    partition: u32 = 0,
    sector_bytes: u16 = 0,
    cluster_sectors: u8 = 0,
    reserved: u16 = 0,
    fats: u8 = 0,
    fat_sectors: u32 = 0,
    sectors: u32 = 0,
    root: u32 = 0,
    fsinfo: u16 = 0,
    backup: u16 = 0,
    write_batch: u64 = 0,
};
const Ntfs = extern struct { slot: u8 = 0, cluster_bytes: u32 = 0, sectors: u64 = 0 };
const CVolume = extern struct {
    kind: u8 = 0,
    has_ref: bool = false,
    reference: Ref = .{ .slot = 0, .generation = 0 },
    fat: Fat = .{},
    ntfs: Ntfs = .{},
};
const CEntry = extern struct {
    name: [768]u8 = .{0} ** 768,
    name_len: usize = 0,
    attr: u8 = 0,
    node: u64 = 0,
    generation: u16 = 0,
    directory_lba: u64 = 0,
    has_lba: bool = false,
    directory_offset: u16 = 0,
    reparse: bool = false,
    size: u64 = 0,
    times: [5]u16 = .{0} ** 5,
};
const Location = extern struct { device: usize, first: u64, count: u64 };
const Bytes = extern struct {
    data: ?[*]const u8 = null,
    len: usize = 0,
    fn text(self: Bytes) []const u8 {
        return if (self.len == 0) "" else self.data.?[0..self.len];
    }
};
const Args = extern struct {
    volume: CVolume = .{},
    other: CVolume = .{},
    entry: ?*const CEntry = null,
    peer: ?*const CEntry = null,
    parent: u64 = 0,
    offset: u64 = 0,
    index: usize = 0,
    name: Bytes = .{},
    second: Bytes = .{},
    third: Bytes = .{},
    data: Bytes = .{},
    buffer: ?[*]u8 = null,
    buffer_len: usize = 0,
    cursor: ?[*]u64 = null,
    progress: ?*CopyProgress = null,
    flag: bool = false,
};
const Result = extern struct { value: u64 = 0, status: i32 = 0, present: bool = false };
extern fn r4os_rust_vfs_mount(u8, u8, ?*const CVolume, ?*const Ref, ?*anyopaque) bool;
extern fn r4os_rust_vfs_volume(u8, *const CVolume) u64;
extern fn r4os_rust_vfs_identity(u8, *const Args) bool;
extern fn r4os_rust_vfs_run(u8, *const Args, ?*CEntry) Result;
extern fn r4os_rust_vfs_attribute(u8, u8) bool;
extern fn r4os_rust_vfs_notify(*const CVolume, u64) void;
fn bytes(value: []const u8) Bytes {
    return .{ .data = value.ptr, .len = value.len };
}
fn cv(value: Volume) CVolume {
    const ref = value.accessReference();
    var out = CVolume{ .has_ref = ref != null, .reference = ref orelse .{ .slot = 0, .generation = 0 } };
    switch (value) {
        .fat32 => |v| out.fat = .{ .device = v.device_index, .partition = v.partition_lba, .sector_bytes = v.bytes_per_sector, .cluster_sectors = v.sectors_per_cluster, .reserved = v.reserved_sectors, .fats = v.fat_count, .fat_sectors = v.sectors_per_fat, .sectors = v.total_sectors, .root = v.root_cluster, .fsinfo = v.fs_info_sector, .backup = v.backup_boot_sector, .write_batch = v.write_batch },
        .ntfs => |v| {
            out.kind = 1;
            out.ntfs = .{ .slot = v.state_slot, .cluster_bytes = v.cluster_bytes, .sectors = v.total_sectors };
        },
    }
    return out;
}
fn volume(value: CVolume) Volume {
    const ref: ?Ref = if (value.has_ref) value.reference else null;
    return switch (value.kind) {
        0 => .{ .fat32 = .{ .mount_ref = ref, .device_index = value.fat.device, .partition_lba = value.fat.partition, .bytes_per_sector = value.fat.sector_bytes, .sectors_per_cluster = value.fat.cluster_sectors, .reserved_sectors = value.fat.reserved, .fat_count = value.fat.fats, .sectors_per_fat = value.fat.fat_sectors, .total_sectors = value.fat.sectors, .root_cluster = value.fat.root, .fs_info_sector = value.fat.fsinfo, .backup_boot_sector = value.fat.backup, .write_batch = value.fat.write_batch } },
        1 => .{ .ntfs = .{ .mount_ref = ref, .state_slot = value.ntfs.slot, .cluster_bytes = value.ntfs.cluster_bytes, .total_sectors = value.ntfs.sectors } },
        else => unreachable,
    };
}
fn ce(e: Entry) CEntry {
    return .{ .name = e.name, .name_len = e.name_len, .attr = e.attr, .node = e.node, .generation = e.node_generation, .directory_lba = e.fat_directory_lba orelse 0, .has_lba = e.fat_directory_lba != null, .directory_offset = e.fat_directory_offset, .reparse = e.reparse, .size = e.size, .times = .{ e.created_time, e.created_date, e.access_date, e.modified_time, e.modified_date } };
}
fn fromCe(e: CEntry) Entry {
    return .{ .name = e.name, .name_len = e.name_len, .attr = e.attr, .node = e.node, .node_generation = e.generation, .fat_directory_lba = if (e.has_lba) e.directory_lba else null, .fat_directory_offset = e.directory_offset, .reparse = e.reparse, .size = e.size, .created_time = e.times[0], .created_date = e.times[1], .access_date = e.times[2], .modified_time = e.times[3], .modified_date = e.times[4] };
}
fn run(op: u8, a: Args) Result {
    return r4os_rust_vfs_run(op, &a, null);
}
fn found(op: u8, a: Args, out: *Entry) LookupStatus {
    var e: CEntry = undefined;
    const result = r4os_rust_vfs_run(op, &a, &e);
    if (result.status == 0) out.* = fromCe(e);
    return @enumFromInt(result.status);
}
pub fn init() void {
    _ = r4os_rust_vfs_mount(0, 0, null, null, null);
}
pub fn mountBootVolume(v: Volume) bool {
    const a = cv(v);
    return r4os_rust_vfs_mount(1, 0, &a, null, null);
}
pub fn mountForDrive(letter: u8, v: Volume) bool {
    const a = cv(v);
    return r4os_rust_vfs_mount(2, letter, &a, null, null);
}
pub fn bootVolume() ?Volume {
    var a: CVolume = undefined;
    return if (r4os_rust_vfs_mount(3, 0, null, null, @ptrCast(&a))) volume(a) else null;
}
pub fn volumeForDrive(letter: u8) ?Volume {
    var a: CVolume = undefined;
    return if (r4os_rust_vfs_mount(4, letter, null, null, @ptrCast(&a))) volume(a) else null;
}
pub fn unmount(ref: Ref) bool {
    return r4os_rust_vfs_mount(5, 0, null, &ref, null);
}
pub fn resolvePathStatus(v: Volume, path: []const u8, out: *NodeRef) LookupStatus {
    const r = run(0, .{ .volume = cv(v), .name = bytes(path) });
    if (r.status == 0) out.* = r.value;
    return @enumFromInt(r.status);
}
pub fn resolvePath(v: Volume, path: []const u8) ?NodeRef {
    var out: NodeRef = undefined;
    return if (resolvePathStatus(v, path, &out) == .found) out else null;
}
pub fn resolveEntryStatus(v: Volume, path: []const u8, out: *Entry) LookupStatus {
    return found(1, .{ .volume = cv(v), .name = bytes(path) }, out);
}
pub fn resolveEntry(v: Volume, path: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (resolveEntryStatus(v, path, &out) == .found) out else null;
}
pub fn lookupDiagnosticStage(v: Volume) u32 {
    return @intCast(run(4, .{ .volume = cv(v) }).value);
}
pub fn lookupEntryStatus(v: Volume, parent: NodeRef, name: []const u8, out: *Entry) LookupStatus {
    return found(2, .{ .volume = cv(v), .parent = parent, .name = bytes(name) }, out);
}
pub fn lookupEntry(v: Volume, parent: NodeRef, name: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (lookupEntryStatus(v, parent, name, &out) == .found) out else null;
}
pub fn lookupRecoveryEntryStatus(v: Volume, parent: NodeRef, name: []const u8, out: *Entry) LookupStatus {
    return found(3, .{ .volume = cv(v), .parent = parent, .name = bytes(name) }, out);
}
pub fn sameVolume(a: Volume, b: Volume) bool {
    return r4os_rust_vfs_identity(0, &.{ .volume = cv(a), .other = cv(b) });
}
fn identity(op: u8, v: Volume, a: Entry, b: Entry) bool {
    const left = ce(a);
    const right = ce(b);
    return r4os_rust_vfs_identity(op, &.{ .volume = cv(v), .entry = &left, .peer = &right });
}
pub fn sameFileIdentity(v: Volume, a: Entry, b: Entry) bool {
    return identity(1, v, a, b);
}
pub fn sameFileForCopy(v: Volume, a: Entry, b: Entry) bool {
    return identity(2, v, a, b);
}
pub fn sameFileRecoveryIdentity(v: Volume, a: Entry, b: Entry) bool {
    return identity(3, v, a, b);
}
fn optional(result: Result) ?usize {
    return if (result.present) @intCast(result.value) else null;
}
pub fn readFile(v: Volume, e: Entry, out: []u8) ?usize {
    const a = ce(e);
    return optional(run(5, .{ .volume = cv(v), .entry = &a, .buffer = out.ptr, .buffer_len = out.len }));
}
pub fn readFileRange(v: Volume, e: Entry, offset: usize, out: []u8) ?usize {
    const a = ce(e);
    return optional(run(6, .{ .volume = cv(v), .entry = &a, .offset = offset, .buffer = out.ptr, .buffer_len = out.len }));
}
pub fn writeFileRange(v: Volume, e: Entry, offset: usize, data: []const u8) ?usize {
    const a = ce(e);
    return optional(run(7, .{ .volume = cv(v), .entry = &a, .offset = offset, .data = bytes(data) }));
}
pub fn notifyDirectoryChanged(v: Volume, parent: NodeRef) void {
    const a = cv(v);
    r4os_rust_vfs_notify(&a, parent);
}
pub fn writeFile(v: Volume, parent: NodeRef, name: []const u8, data: []const u8) bool {
    return run(8, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .data = bytes(data) }).present;
}
pub fn appendFile(v: Volume, parent: NodeRef, name: []const u8, data: []const u8) bool {
    return run(9, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .data = bytes(data) }).present;
}
pub fn appendFileAtOffsetStatus(v: Volume, parent: NodeRef, name: []const u8, expected_size: u64, data: []const u8) AppendStatus {
    return @enumFromInt(run(10, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .offset = expected_size, .data = bytes(data) }).status);
}
pub fn appendFileAtOffsetStatusDeferred(v: Volume, parent: NodeRef, name: []const u8, expected_size: u64, data: []const u8) AppendStatus {
    return @enumFromInt(run(11, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .offset = expected_size, .data = bytes(data) }).status);
}
pub fn publishCopyFile(v: Volume, parent: NodeRef, staged: []const u8, target: []const u8) bool {
    return run(12, .{ .volume = cv(v), .parent = parent, .name = bytes(staged), .second = bytes(target) }).present;
}
pub const CopyProgress = @import("copy_transfer.zig").Progress;
fn copy(src: Volume, dst: Volume, e: Entry, parent: NodeRef, name: []const u8, replace: bool) bool {
    const a = ce(e);
    return run(13, .{ .volume = cv(src), .other = cv(dst), .entry = &a, .parent = parent, .name = bytes(name), .flag = replace }).present;
}
pub fn copyFile(src: Volume, dst: Volume, e: Entry, parent: NodeRef, name: []const u8) bool {
    return copy(src, dst, e, parent, name, true);
}
pub fn copyFileNoReplace(src: Volume, dst: Volume, e: Entry, parent: NodeRef, name: []const u8) bool {
    return copy(src, dst, e, parent, name, false);
}
pub fn copyFileBuffered(src: Volume, dst: Volume, e: Entry, parent: NodeRef, name: []const u8, replace: bool, chunk: []u8, progress: *CopyProgress) bool {
    const a = ce(e);
    return run(14, .{ .volume = cv(src), .other = cv(dst), .entry = &a, .parent = parent, .name = bytes(name), .flag = replace, .buffer = chunk.ptr, .buffer_len = chunk.len, .progress = progress }).present;
}
pub fn makeDirectory(v: Volume, parent: NodeRef, name: []const u8) bool {
    return run(15, .{ .volume = cv(v), .parent = parent, .name = bytes(name) }).present;
}
pub fn deleteFile(v: Volume, parent: NodeRef, name: []const u8) bool {
    return run(16, .{ .volume = cv(v), .parent = parent, .name = bytes(name) }).present;
}
pub fn deleteFileIfIdentity(v: Volume, parent: NodeRef, name: []const u8, expected: Entry) DeleteIfMatchResult {
    const a = ce(expected);
    return @enumFromInt(run(17, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .entry = &a }).status);
}
pub fn deleteRecoveryEntryIfIdentity(v: Volume, parent: NodeRef, name: []const u8, expected: Entry) RecoveryDeleteResult {
    const a = ce(expected);
    return @enumFromInt(run(18, .{ .volume = cv(v), .parent = parent, .name = bytes(name), .entry = &a }).status);
}
pub fn namesEqualCollated(v: Volume, a: []const u8, b: []const u8) ?bool {
    const r = run(19, .{ .volume = cv(v), .name = bytes(a), .second = bytes(b) });
    return if (r.present) r.value != 0 else null;
}
pub fn removeDirectory(v: Volume, parent: NodeRef, name: []const u8) bool {
    return run(20, .{ .volume = cv(v), .parent = parent, .name = bytes(name) }).present;
}
pub fn renameEntryStatus(v: Volume, parent: NodeRef, a: []const u8, b: []const u8) RenameStatus {
    return @enumFromInt(run(21, .{ .volume = cv(v), .parent = parent, .name = bytes(a), .second = bytes(b) }).status);
}
pub fn renameEntry(v: Volume, parent: NodeRef, a: []const u8, b: []const u8) bool {
    return renameEntryStatus(v, parent, a, b) == .ok;
}
pub fn replaceFileAtomic(v: Volume, parent: NodeRef, target: []const u8, staged: []const u8, backup: []const u8, consume_stage: bool) AtomicReplaceResult {
    return @enumFromInt(run(22, .{ .volume = cv(v), .parent = parent, .name = bytes(target), .second = bytes(staged), .third = bytes(backup), .flag = consume_stage }).status);
}
pub fn replaceFileAtomicCreateOnly(v: Volume, parent: NodeRef, target: []const u8, staged: []const u8, backup: []const u8) AtomicReplaceResult {
    return @enumFromInt(run(23, .{ .volume = cv(v), .parent = parent, .name = bytes(target), .second = bytes(staged), .third = bytes(backup) }).status);
}
pub fn flushVolume(v: Volume) bool {
    return run(24, .{ .volume = cv(v) }).present;
}
pub fn readDirectory(v: Volume, dir: NodeRef, out: []u8) ?usize {
    return optional(run(25, .{ .volume = cv(v), .parent = dir, .buffer = out.ptr, .buffer_len = out.len }));
}
pub fn readDirectoryEntryStatus(v: Volume, dir: NodeRef, index: usize, out: []u8, e: *Entry) LookupStatus {
    return found(26, .{ .volume = cv(v), .parent = dir, .index = index, .buffer = out.ptr, .buffer_len = out.len }, e);
}
pub fn readDirectoryEntry(v: Volume, dir: NodeRef, index: usize, out: []u8) ?Entry {
    var e: Entry = undefined;
    return if (readDirectoryEntryStatus(v, dir, index, out, &e) == .found) e else null;
}
pub const DirectoryNextStatus = enum { found, not_found, again, io };
pub fn nextDirectoryEntry(v: Volume, dir: NodeRef, state: *[128]u64, out: *Entry) DirectoryNextStatus {
    var e: CEntry = undefined;
    const result = r4os_rust_vfs_run(27, &.{ .volume = cv(v), .parent = dir, .cursor = state }, &e);
    if (result.status == 0) out.* = fromCe(e);
    return @enumFromInt(result.status);
}
pub fn freeClusterCount(v: Volume) ?u32 {
    const r = run(28, .{ .volume = cv(v) });
    return if (r.present) @intCast(r.value) else null;
}
pub fn listRoot(v: Volume) bool {
    return run(29, .{ .volume = cv(v) }).present;
}
pub fn validateShortName83(name: []const u8) bool {
    return run(30, .{ .name = bytes(name) }).present;
}

fn entryFromFat32(entry: fat32.Entry) Entry {
    var out = Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .node = entry.first_cluster,
        .fat_directory_lba = entry.directory_lba,
        .fat_directory_offset = entry.directory_offset,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryToFat32(entry: Entry) fat32.Entry {
    var out = fat32.Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .first_cluster = @intCast(entry.node),
        .directory_lba = entry.fat_directory_lba,
        .directory_offset = entry.fat_directory_offset,
        .size = @intCast(entry.size),
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryFromNtfs(entry: ntfs_fs.Entry) Entry {
    var out = Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .node = entry.record,
        .node_generation = entry.sequence,
        .reparse = entry.reparse,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

fn entryToNtfs(entry: Entry) ntfs_fs.Entry {
    var out = ntfs_fs.Entry{
        .name_len = entry.name_len,
        .attr = entry.attr,
        .record = entry.node,
        .sequence = entry.node_generation,
        .reparse = entry.reparse,
        .size = entry.size,
        .created_time = entry.created_time,
        .created_date = entry.created_date,
        .access_date = entry.access_date,
        .modified_time = entry.modified_time,
        .modified_date = entry.modified_date,
    };
    out.name = entry.name;
    return out;
}

// Raw backend leaves. All neutral routing, identity, status and lifecycle
// decisions belong to Rust. These functions only project values and call
// the existing filesystem owner; no mount table or generic policy lives here.
fn boolean(value: bool) Result {
    return .{ .present = value };
}
fn code(value: anytype) Result {
    return .{ .status = @intFromEnum(value) };
}
fn number(value: ?usize) Result {
    return if (value) |n| .{ .value = n, .present = true } else .{};
}
fn outputBuffer(a: *const Args) []u8 {
    return a.buffer.?[0..a.buffer_len];
}
fn fatEntry(a: *const CEntry) fat32.Entry {
    return entryToFat32(fromCe(a.*));
}
fn ntfsEntry(a: *const CEntry) ntfs_fs.Entry {
    return entryToNtfs(fromCe(a.*));
}

export fn r4os_zig_vfs_fat(operation: u8, a: *const Args, out: ?*CEntry) Result {
    // Each raw operation keeps its own stack frame. Inlining all backend
    // branches into this dispatcher exhausts the shared 64-KB task stack.
    return switch (operation) {
        inline 0, 1, 2, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 34 => |selected| fatLeaf(selected, a, out),
        else => unreachable,
    };
}
noinline fn fatLeaf(comptime operation: u8, a: *const Args, out: ?*CEntry) Result {
    const v = volume(a.volume).fat32;
    return switch (operation) {
        0 => blk: {
            var node: u32 = undefined;
            const status = fat32.resolvePathStatus(v, a.name.text(), &node);
            var r = code(status);
            if (status == .found) r.value = node;
            break :blk r;
        },
        1 => blk: {
            var e: fat32.Entry = undefined;
            const status = fat32.resolveEntryStatus(v, a.name.text(), &e);
            if (status == .found) out.?.* = ce(entryFromFat32(e));
            break :blk code(status);
        },
        2 => blk: {
            var e: fat32.Entry = undefined;
            const status = fat32.findEntryStatus(v, @intCast(a.parent), a.name.text(), &e);
            if (status == .found) out.?.* = ce(entryFromFat32(e));
            break :blk code(status);
        },
        5 => number(fat32.readFile(v, fatEntry(a.entry.?), outputBuffer(a))),
        6 => number(fat32.readFileRange(v, fatEntry(a.entry.?), @intCast(a.offset), outputBuffer(a))),
        7 => number(fat32.writeFileRange(v, fatEntry(a.entry.?), @intCast(a.offset), a.data.text())),
        8 => boolean(fat32.writeFile(v, @intCast(a.parent), a.name.text(), a.data.text())),
        9 => boolean(fat32.appendFile(v, @intCast(a.parent), a.name.text(), a.data.text())),
        10 => code(fat32.appendFileAtOffsetStatus(v, @intCast(a.parent), a.name.text(), @intCast(a.offset), a.data.text())),
        11 => code(fat32.appendFileAtOffsetStatusDeferred(v, @intCast(a.parent), a.name.text(), @intCast(a.offset), a.data.text())),
        12 => boolean(fat32.publishCopyFile(v, @intCast(a.parent), a.name.text(), a.second.text())),
        13 => boolean(fat32.copyFile(v, volume(a.other).fat32, fatEntry(a.entry.?), @intCast(a.parent), a.name.text())),
        15 => boolean(fat32.makeDirectory(v, @intCast(a.parent), a.name.text())),
        16 => boolean(fat32.deleteFile(v, @intCast(a.parent), a.name.text())),
        17 => code(fat32.deleteFileIfIdentity(v, @intCast(a.parent), a.name.text(), fatEntry(a.entry.?))),
        19 => blk: {
            const answer = fat32.namesEqualCollated(v, a.name.text(), a.second.text()) orelse break :blk Result{};
            break :blk .{ .value = @intFromBool(answer), .present = true };
        },
        20 => boolean(fat32.removeDirectory(v, @intCast(a.parent), a.name.text())),
        21 => code(fat32.renameEntryStatus(v, @intCast(a.parent), a.name.text(), a.second.text())),
        22 => code(fat32.replaceFileAtomic(v, @intCast(a.parent), a.name.text(), a.second.text(), a.third.text(), a.flag)),
        23 => code(fat32.replaceFileAtomicCreateOnly(v, @intCast(a.parent), a.name.text(), a.second.text(), a.third.text())),
        24 => boolean(fat32.flushVolume(v)),
        25 => number(fat32.readDirectory(v, @intCast(a.parent), outputBuffer(a))),
        26 => blk: {
            var e: fat32.Entry = undefined;
            const status = fat32.readDirectoryEntryStatus(v, @intCast(a.parent), a.index, outputBuffer(a), &e);
            if (status == .found) out.?.* = ce(entryFromFat32(e));
            break :blk code(status);
        },
        27 => blk: {
            var e: fat32.Entry = undefined;
            const cursor: *fat32.DirectoryCursor = @ptrCast(@alignCast(a.cursor.?));
            const status = fat32.nextDirectoryEntry(v, @intCast(a.parent), cursor, &e);
            if (status == .found) out.?.* = ce(entryFromFat32(e));
            break :blk code(status);
        },
        28 => blk: {
            const free = fat32.freeClusterCount(v) orelse break :blk Result{};
            break :blk .{ .value = free, .present = true };
        },
        29 => boolean(fat32.listRoot(v)),
        30 => boolean(fat32.validateShortName83(a.name.text())),
        31 => boolean(fat32.sameFileForCopy(fatEntry(a.entry.?), fatEntry(a.peer.?))),
        34 => boolean(fat32.copyFileNoReplace(v, volume(a.other).fat32, fatEntry(a.entry.?), @intCast(a.parent), a.name.text())),
        else => unreachable,
    };
}
export fn r4os_zig_vfs_ntfs(operation: u8, a: *const Args, out: ?*CEntry) Result {
    // Each raw operation keeps its own stack frame. Inlining all backend
    // branches into this dispatcher exhausts the shared 64-KB task stack.
    return switch (operation) {
        inline 0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 32 => |selected| ntfsLeaf(selected, a, out),
        else => unreachable,
    };
}
noinline fn ntfsLeaf(comptime operation: u8, a: *const Args, out: ?*CEntry) Result {
    const v = volume(a.volume).ntfs;
    return switch (operation) {
        0 => blk: {
            var node: u64 = undefined;
            const status = ntfs_fs.resolvePathStatus(v, a.name.text(), &node);
            var r = code(status);
            if (status == .found) r.value = node;
            break :blk r;
        },
        1 => blk: {
            var e: ntfs_fs.Entry = undefined;
            const status = ntfs_fs.resolveEntryStatus(v, a.name.text(), &e);
            if (status == .found) out.?.* = ce(entryFromNtfs(e));
            break :blk code(status);
        },
        2 => blk: {
            var e: ntfs_fs.Entry = undefined;
            const status = ntfs_fs.lookupEntryStatus(v, a.parent, a.name.text(), &e);
            if (status == .found) out.?.* = ce(entryFromNtfs(e));
            break :blk code(status);
        },
        3 => blk: {
            var e: ntfs_fs.Entry = undefined;
            const status = ntfs_fs.lookupRecoveryEntryStatus(v, a.parent, a.name.text(), &e);
            if (status == .found) out.?.* = ce(entryFromNtfs(e));
            break :blk code(status);
        },
        4 => .{ .value = ntfs_fs.lookupDiagnosticStage(), .present = true },
        5 => number(ntfs_fs.readFile(v, ntfsEntry(a.entry.?), outputBuffer(a))),
        6 => number(ntfs_fs.readFileRange(v, ntfsEntry(a.entry.?), @intCast(a.offset), outputBuffer(a))),
        7 => number(ntfs_fs.writeFileRange(v, ntfsEntry(a.entry.?), @intCast(a.offset), a.data.text())),
        8 => boolean(ntfs_fs.writeFile(v, a.parent, a.name.text(), a.data.text())),
        10 => code(ntfs_fs.appendFileAtOffset(v, a.parent, a.name.text(), a.offset, a.data.text())),
        11 => code(ntfs_fs.appendFileAtOffsetDeferred(v, a.parent, a.name.text(), a.offset, a.data.text())),
        12 => code(ntfs_fs.renameEntryStatus(v, a.parent, a.name.text(), a.second.text())),
        15 => boolean(ntfs_fs.makeDirectory(v, a.parent, a.name.text())),
        16 => boolean(ntfs_fs.deleteFile(v, a.parent, a.name.text())),
        17 => code(ntfs_fs.deleteFileIfIdentity(v, a.parent, a.name.text(), ntfsEntry(a.entry.?))),
        18 => code(ntfs_fs.deleteRecoveryEntryIfIdentity(v, a.parent, a.name.text(), ntfsEntry(a.entry.?))),
        19 => blk: {
            const answer = ntfs_fs.namesEqualCollated(v, a.name.text(), a.second.text()) orelse break :blk Result{};
            break :blk .{ .value = @intFromBool(answer), .present = true };
        },
        20 => boolean(ntfs_fs.removeDirectory(v, a.parent, a.name.text())),
        21 => code(ntfs_fs.renameEntryStatus(v, a.parent, a.name.text(), a.second.text())),
        22 => code(ntfs_fs.replaceFileAtomic(v, a.parent, a.name.text(), a.second.text(), a.third.text(), a.flag)),
        23 => code(ntfs_fs.replaceFileAtomicCreateOnly(v, a.parent, a.name.text(), a.second.text(), a.third.text())),
        24 => boolean(ntfs_fs.flushVolume(v)),
        25 => number(ntfs_fs.readDirectory(v, a.parent, outputBuffer(a))),
        26 => blk: {
            var e: ntfs_fs.Entry = undefined;
            const status = ntfs_fs.readDirectoryEntryStatus(v, a.parent, a.index, outputBuffer(a), &e);
            if (status == .found) out.?.* = ce(entryFromNtfs(e));
            break :blk code(status);
        },
        27 => blk: {
            var e: ntfs_fs.Entry = undefined;
            const cursor: *ntfs_fs.DirectoryCursor = @ptrCast(@alignCast(a.cursor.?));
            const status = ntfs_fs.nextDirectoryEntry(v, a.parent, cursor, &e);
            if (status == .found) out.?.* = ce(entryFromNtfs(e));
            break :blk code(status);
        },
        28 => blk: {
            const free = ntfs_fs.freeClusterCount(v) orelse break :blk Result{};
            break :blk .{ .value = free, .present = true };
        },
        29 => boolean(ntfs_fs.listRoot(v)),
        32 => .{ .value = ntfs_fs.childSize(v, a.parent, a.name.text()), .present = true },
        else => unreachable,
    };
}
export fn r4os_zig_vfs_location(v: *const CVolume, out: *Location) bool {
    const value = ntfs_fs.storageLocation(volume(v.*).ntfs) orelse return false;
    out.* = .{ .device = value.device, .first = value.first, .count = value.count };
    return true;
}
export fn r4os_zig_vfs_activity() void {
    @import("../kernel/desktop_events.zig").signal();
}
comptime {
    if (@sizeOf(Fat) != 48 or @sizeOf(Ntfs) != 16 or @sizeOf(CVolume) != 88 or @sizeOf(CEntry) != 840 or @sizeOf(Result) != 16)
        @compileError("Native VFS private C layout changed");
    if (@sizeOf(fat32.DirectoryCursor) > 1024 or @sizeOf(ntfs_fs.DirectoryCursor) > 1024)
        @compileError("directory cursor exceeds contract");
    if (@import("ntfs_format").MFT_RECORD_ROOT != 5)
        @compileError("Native VFS NTFS root record changed");
    if (@intFromEnum(fat32.RenameStatus.read_only) != 4 or @intFromEnum(fat32.RenameStatus.io) != 5 or
        @intFromEnum(ntfs_fs.WriteStatus.offset_mismatch) != 12 or @intFromEnum(ntfs_fs.WriteStatus.cleanup_failed) != 14 or
        @intFromEnum(ntfs_fs.ReplaceResult.io) != 6)
        @compileError("Native VFS raw backend status mapping changed");
}
