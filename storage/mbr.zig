// Native legacy mount scanner owns table validation, marker policy and reports.
// Temporary leaf adapters only project current FS/backend call shapes.
const drive = @import("../fs/drive.zig");
const fat32 = @import("../fs/fat/fat32.zig");
const ntfs_fs = @import("../fs/ntfs/ntfs.zig");
const vfs = @import("../fs/vfs.zig");
const inventory = @import("media_inventory.zig");
const k = @import("../kernel/log.zig");
pub const Partition = struct {
    bootable: bool,
    type_id: u8,
    first_lba: u64,
    sector_count: u64,
};

pub const TableKind = enum(u8) {
    none,
    mbr,
    gpt_primary,
    gpt_backup,
};

pub const ScanResult = enum(u8) {
    device_missing,
    logical_sector_unsupported,
    capacity_unavailable,
    sector_zero_read_failed,
    mbr_signature_invalid,
    no_mountable_partition,
    unsupported_partition,
    partition_invalid,
    gpt_invalid,
    gpt_entry_read_failed,
    partition_lba_unsupported,
    filesystem_probe_read_failed,
    filesystem_lookup_failed,
    filesystem_unknown,
    fat32_invalid,
    ntfs_invalid,
    boot_volume_only,
    data_volume_only,
    partition_size_unsupported,
    drive_letters_exhausted,
    mount_rejected,
    system_mounted,
};

pub const ScanReport = struct {
    table: TableKind = .none,
    result: ScanResult = .device_missing,
    detail: []const u8 = "",
    table_valid: bool = false,
    partition_count: u16 = 0,
    mountable_count: u16 = 0,
    filesystem_count: u16 = 0,
    best_system_score: u8 = 0,
    marker_found_mask: u8 = 0,
    marker_io_mask: u8 = 0,
    mbr_type: u8 = 0,
};

const Label = extern struct { data: [*]const u8, len: usize };
const NativeReport = extern struct { table: u8, result: u8, detail: Label, table_valid: bool, partition_count: u16, mountable_count: u16, filesystem_count: u16, best_system_score: u8, marker_found_mask: u8, marker_io_mask: u8, mbr_type: u8 };
const Probe = extern struct { index: usize, lba: u32, sectors: u64, kind: u8 };
const Consumer = *const fn (*anyopaque, *anyopaque) callconv(.c) void;
extern fn r4os_rust_mbr_scan(usize, *NativeReport) callconv(.c) void;
pub fn scan(index: usize) ScanReport {
    var out: NativeReport = undefined;
    r4os_rust_mbr_scan(index, &out);
    return .{ .table = @enumFromInt(out.table), .result = @enumFromInt(out.result), .detail = out.detail.data[0..out.detail.len], .table_valid = out.table_valid, .partition_count = out.partition_count, .mountable_count = out.mountable_count, .filesystem_count = out.filesystem_count, .best_system_score = out.best_system_score, .marker_found_mask = out.marker_found_mask, .marker_io_mask = out.marker_io_mask, .mbr_type = out.mbr_type };
}
export fn r4os_zig_mbr_probe(input: *const Probe, ctx: *anyopaque, consume: Consumer) callconv(.c) bool {
    var volume: ?vfs.Volume = switch (input.kind) {
        2 => .{ .fat32 = fat32.inspectBounded(input.index, input.lba, input.sectors) orelse return false },
        3 => .{ .ntfs = ntfs_fs.inspectBounded(input.index, input.lba, input.sectors) orelse return false },
        else => unreachable,
    };
    consume(ctx, &volume);
    return true;
}
export fn r4os_zig_mbr_lookup(token: *anyopaque, path: [*]const u8, len: usize, attr: *u8) callconv(.c) u8 {
    var entry: vfs.Entry = undefined;
    const status = vfs.resolveEntryStatus(inventory.volume(token), path[0..len], &entry);
    if (status == .found) attr.* = entry.attr;
    return @intFromEnum(status);
}
export fn r4os_zig_mbr_list(token: *anyopaque) callconv(.c) void {
    _ = vfs.listRoot(inventory.volume(token));
}
export fn r4os_zig_mbr_hex(value: u64, digits: u8) callconv(.c) void {
    k.putHex(value, digits);
}
comptime {
    if (@sizeOf(NativeReport) != 40 or @sizeOf(Probe) != 32 or @intFromEnum(vfs.LookupStatus.found) != 0 or @intFromEnum(vfs.LookupStatus.not_found) != 1 or @intFromEnum(vfs.LookupStatus.io) != 2 or @intFromEnum(drive.Role.system) != 1 or @intFromEnum(drive.Role.data) != 2) @compileError("Native legacy scan report/lookup/role layout differs");
}
