// Native boot inventory owns records, probes and manifest admission. This
// temporary facade only projects legacy device/FS/JSON call shapes.
const std = @import("std");
const block = @import("block.zig");
const tables = @import("partition_table.zig");
const installation = @import("installation.zig");
const fat = @import("../fs/fat/fat32.zig");
const ntfs = @import("../fs/ntfs/ntfs.zig");
const vfs = @import("../fs/vfs.zig");
pub const maximum_devices = 16;
const Label = extern struct { data: [*]const u8, len: usize };
const DeviceInfo = extern struct { index: usize, bus: u8, name: Label, model: Label, driver: Label, sector_bytes: u32, sectors: u64, writable: bool };
const Entry = extern struct {
    name: [768]u8,
    name_len: usize,
    attr: u8,
    node: u64,
    node_generation: u16,
    fat_lba: u64,
    has_fat_lba: bool,
    fat_offset: u16,
    reparse: bool,
    size: u64,
    created_time: u16,
    created_date: u16,
    access_date: u16,
    modified_time: u16,
    modified_date: u16,
};
// Opaque storage belongs to the current FS owner. Native inventory retains
// only each exact token; no second inventory/selection state lives here.
var volumes: [maximum_devices][tables.max_partitions]?vfs.Volume = .{.{null} ** tables.max_partitions} ** maximum_devices;
extern fn r4os_rust_media_scan() callconv(.c) void;
pub fn scan() void {
    r4os_rust_media_scan();
}
fn label(text: []const u8) Label {
    return .{ .data = text.ptr, .len = text.len };
}
export fn r4os_zig_media_device(index: usize, out: *DeviceInfo) callconv(.c) bool {
    const device = block.get(index) orelse return false;
    out.* = .{ .index = index, .bus = @intFromEnum(device.bus), .name = label(device.name), .model = label(device.model), .driver = label(device.driver), .sector_bytes = device.sector_size, .sectors = device.sector_count, .writable = device.writable };
    return true;
}
export fn r4os_zig_media_volumes_reset() callconv(.c) void {
    for (&volumes) |*row| for (row) |*cell| {
        cell.* = null;
    };
}
export fn r4os_zig_media_probe(index: usize, partition: usize, kind: u8, lba: u32, sectors: u64) callconv(.c) ?*anyopaque {
    const value: vfs.Volume = switch (kind) {
        3 => .{ .fat32 = fat.parseBounded(index, lba, sectors) orelse return null },
        4 => .{ .ntfs = ntfs.inspectBounded(index, lba, sectors) orelse return null },
        else => unreachable,
    };
    volumes[index][partition] = value;
    return &volumes[index][partition];
}
pub fn volume(raw: *anyopaque) vfs.Volume {
    const cell: *const ?vfs.Volume = @ptrCast(@alignCast(raw));
    return cell.*.?;
}
export fn r4os_zig_media_entry(raw: *anyopaque, path: [*]const u8, len: usize, out: *Entry) callconv(.c) bool {
    const entry = vfs.resolveEntry(volume(raw), path[0..len]) orelse return false;
    out.* = .{ .name = entry.name, .name_len = entry.name_len, .attr = entry.attr, .node = entry.node, .node_generation = entry.node_generation, .fat_lba = entry.fat_directory_lba orelse 0, .has_fat_lba = entry.fat_directory_lba != null, .fat_offset = entry.fat_directory_offset, .reparse = entry.reparse, .size = entry.size, .created_time = entry.created_time, .created_date = entry.created_date, .access_date = entry.access_date, .modified_time = entry.modified_time, .modified_date = entry.modified_date };
    return true;
}
export fn r4os_zig_media_read(raw: *anyopaque, entry: *const Entry, offset: usize, data: [*]u8, len: usize) callconv(.c) usize {
    const value = vfs.Entry{ .name = entry.name, .name_len = entry.name_len, .attr = entry.attr, .node = entry.node, .node_generation = entry.node_generation, .fat_directory_lba = if (entry.has_fat_lba) entry.fat_lba else null, .fat_directory_offset = entry.fat_offset, .reparse = entry.reparse, .size = entry.size, .created_time = entry.created_time, .created_date = entry.created_date, .access_date = entry.access_date, .modified_time = entry.modified_time, .modified_date = entry.modified_date };
    return vfs.readFileRange(volume(raw), value, offset, data[0..len]) orelse std.math.maxInt(usize);
}
export fn r4os_zig_media_manifest(data: [*]const u8, len: usize, scratch: [*]u8, scratch_len: usize, out: *installation.Manifest, err: *Label) callconv(.c) bool {
    var allocator = std.heap.FixedBufferAllocator.init(scratch[0..scratch_len]);
    out.* = installation.parse(allocator.allocator(), data[0..len]) catch |failure| {
        err.* = label(@errorName(failure));
        return false;
    };
    return true;
}
comptime {
    if (@sizeOf(DeviceInfo) != 88 or @sizeOf(Entry) != 840 or vfs.NAME_MAX != 768 or tables.max_partitions != 128) @compileError("Native media C device/entry/token bounds differ");
}
