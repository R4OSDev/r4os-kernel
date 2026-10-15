//! Native normal installation mount policy; temporary stateless FS/boot/log adapters.
const boot_info = @import("../bootloader/boot_info.zig");
const inventory = @import("media_inventory.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const log = @import("../kernel/log.zig");
const Label = extern struct { data: [*]const u8, len: usize };
const Identity = extern struct { present: bool, generic_media: bool, path: Label, path_truncated: bool, disk_guid: [16]u8, partition_guid: [16]u8 };
const BootIdentity = extern struct { loaded: Identity, mbr_disk_id: u32, partition_index: u32 };
const NativeResult = extern struct { kind: u8, index: usize, reason: Label };
const DriveRequest = extern struct { letter: u8, kind: u8, role: u8, bytes: usize, index: usize, name: Label, volume: ?*anyopaque };
pub const Result = union(enum) { managed, legacy: usize, rejected: []const u8 };
extern fn r4os_rust_normal_mount(*NativeResult) callconv(.c) void;
pub fn mount() Result {
    var out: NativeResult = undefined;
    r4os_rust_normal_mount(&out);
    return switch (out.kind) {
        0 => .managed,
        1 => .{ .legacy = out.index },
        2 => .{ .rejected = out.reason.data[0..out.reason.len] },
        else => unreachable,
    };
}
export fn r4os_zig_normal_identity(out: *BootIdentity) callconv(.c) void {
    const identity = &boot_info.get().executable_source;
    const path = identity.path();
    out.* = .{ .loaded = .{ .present = identity.present, .generic_media = identity.media_type == 0, .path = .{ .data = path.ptr, .len = path.len }, .path_truncated = identity.path_truncated, .disk_guid = identity.disk_guid, .partition_guid = identity.partition_guid }, .mbr_disk_id = identity.mbr_disk_id, .partition_index = identity.partition_index };
}
export fn r4os_zig_normal_drive(operation: u8, input: *const DriveRequest) callconv(.c) bool {
    return switch (operation) {
        0 => drive.get(input.letter) != null,
        1 => drive.mountBlockRole(input.letter, @enumFromInt(input.kind), @enumFromInt(input.role), input.name.data[0..input.name.len], input.bytes, input.index),
        2 => vfs.mountForDrive(input.letter, inventory.volume(input.volume.?)),
        3 => blk: {
            drive.unmountLocked(input.letter);
            break :blk true;
        },
        4 => drive.setCurrent(input.letter),
        5 => vfs.mountBootVolume(inventory.volume(input.volume.?)),
        else => unreachable,
    };
}
export fn r4os_zig_normal_log(data: [*]const u8, len: usize) callconv(.c) void {
    log.puts(data[0..len]);
}
export fn r4os_zig_normal_decimal(value: u64) callconv(.c) void {
    log.putDec(value);
}
comptime {
    if (@sizeOf(BootIdentity) != 72 or @sizeOf(NativeResult) != 32 or @sizeOf(DriveRequest) != 48 or @intFromEnum(drive.Kind.fat32) != 2 or @intFromEnum(drive.Kind.ntfs) != 3 or @intFromEnum(drive.Role.system) != 1 or @intFromEnum(drive.Role.data) != 2) @compileError("Native normal boot C/drive role layout differs");
}
