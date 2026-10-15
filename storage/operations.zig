// Stateless C facade for the native kernel storage maintenance owner.
// Filesystem and drive data remain at their current owners until their port.
const std = @import("std");
const api = @import("r4os_kernel_contract");
const access = @import("access_runtime.zig");
const vfs = @import("../fs/vfs.zig");
const drive = @import("../fs/drive.zig");
const fat = @import("../fs/fat/fat32.zig");
const ntfs = @import("../fs/ntfs/ntfs.zig");
const cache = @import("../fs/page_cache.zig");
const task = @import("../sched/task.zig");
const Label = extern struct { data: [*]const u8, len: usize };
const Args = extern struct {
    generation: u64 = 0,
    id: u64 = 0,
    relative: u64 = 0,
    slot: u32 = 0,
    flags: u32 = 0,
    sectors: u32 = 0,
    len: u32 = 0,
    target: ?*const api.StorageTarget = null,
    device: ?*const api.StorageDeviceRef = null,
    data: ?[*]u8 = null,
    program: u32 = 0,
    program_generation: u64 = 0,
    task: u32 = 0,
    task_generation: u64 = 0,
};
const DriveMetadata = extern struct { present: bool, has_vfs: bool, kind: u8, role: u8, bytes: usize, index: usize, name: [32]u8 };
const VolumeMetadata = extern struct { filesystem: u32, role: u32 };
const Probe = extern struct { index: usize, lba: u32, sectors: u64, filesystem: u32 };
extern fn r4os_rust_storage_operations(operation: u8, input: *const Args, out: ?*anyopaque) i32;
fn run(operation: u8, input: Args, out: ?*anyopaque) i32 {
    return r4os_rust_storage_operations(operation, &input, out);
}
pub fn reserveRecoveryCacheBoot(index: usize) bool {
    return run(0, .{ .id = index }, null) != 0;
}
pub fn inventory(out: *api.StorageInventory) callconv(.c) i32 {
    return run(1, .{}, out);
}
pub fn deviceInfo(generation: u64, slot: u32, out: *api.StorageDeviceInfo) callconv(.c) i32 {
    return run(2, .{ .generation = generation, .slot = slot }, out);
}
pub fn partitionInfo(generation: u64, device: *const api.StorageDeviceRef, slot: u32, out: *api.StoragePartitionInfo) callconv(.c) i32 {
    return run(3, .{ .generation = generation, .device = device, .slot = slot }, out);
}
pub fn volumeInfo(generation: u64, slot: u32, out: *api.StorageVolumeInfo) callconv(.c) i32 {
    return run(4, .{ .generation = generation, .slot = slot }, out);
}
pub fn claimBegin(target: *const api.StorageTarget, out: *u64) callconv(.c) i32 {
    return run(5, .{ .target = target }, out);
}
pub fn claimEnd(id: u64, flags: u32) callconv(.c) i32 {
    return run(6, .{ .id = id, .flags = flags }, null);
}
pub fn read(target: *const api.StorageTarget, relative: u64, sectors: u32, out: [*]u8, len: u32) callconv(.c) i32 {
    return run(7, .{ .target = target, .relative = relative, .sectors = sectors, .data = out, .len = len }, null);
}
pub fn claimRead(id: u64, relative: u64, sectors: u32, data: [*]u8, len: u32) callconv(.c) i32 {
    return run(8, .{ .id = id, .relative = relative, .sectors = sectors, .data = data, .len = len }, null);
}
pub fn claimWrite(id: u64, relative: u64, sectors: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    return run(9, .{ .id = id, .relative = relative, .sectors = sectors, .data = @constCast(data), .len = len }, null);
}
pub fn claimFlush(id: u64) callconv(.c) i32 {
    return run(10, .{ .id = id }, null);
}
pub fn rescan(device: *const api.StorageDeviceRef) callconv(.c) i32 {
    return run(11, .{ .device = device }, null);
}
pub fn mount(target: *const api.StorageTarget, letter_arg: u32, out: *api.StorageVolumeRef) callconv(.c) i32 {
    return run(12, .{ .target = target, .slot = letter_arg }, out);
}
pub fn unmount(volume: *const api.StorageVolumeRef) callconv(.c) i32 {
    return run(13, .{}, @constCast(volume));
}
pub fn useEnd(id: u64) callconv(.c) i32 {
    return run(14, .{ .id = id }, null);
}
pub fn releaseOwner(program: u32, generation: u64, task_id: u32, task_generation: u64) bool {
    return run(15, .{ .program = program, .program_generation = generation, .task = task_id, .task_generation = task_generation }, null) != 0;
}

export fn r4os_zig_ops_drive(letter: u8, out: *DriveMetadata) void {
    const d = drive.get(letter);
    out.* = .{ .present = d != null, .has_vfs = vfs.volumeForDrive(letter) != null, .kind = if (d) |value| @intFromEnum(value.kind) else 0, .role = if (d) |value| @intFromEnum(value.role) else 0, .bytes = if (d) |value| value.bytes else 0, .index = if (d) |value| value.block_device_index orelse std.math.maxInt(usize) else std.math.maxInt(usize), .name = .{0} ** 32 };
    if (d) |value| {
        const len = @min(value.name.len, out.name.len - 1);
        @memcpy(out.name[0..len], value.name[0..len]);
    }
}
export fn r4os_zig_ops_volume(slot: u32, out: *VolumeMetadata) bool {
    const volume = (if (slot == 26) vfs.bootVolume() else vfs.volumeForDrive(@intCast('A' + slot))) orelse return false;
    const d = if (slot == 26) null else drive.get(@intCast('A' + slot));
    out.* = .{ .filesystem = switch (volume) {
        .fat32 => api.storage_filesystem_fat32,
        .ntfs => api.storage_filesystem_ntfs,
    }, .role = if (d) |value| @intFromEnum(value.role) else 0 };
    return true;
}
export fn r4os_zig_ops_mount_reference(letter: u8, out: *access.MountRef) bool {
    const volume = vfs.volumeForDrive(letter) orelse return false;
    out.* = volume.accessReference() orelse return false;
    return true;
}
export fn r4os_zig_ops_unmount(reference: access.MountRef) bool {
    return vfs.unmount(reference);
}
export fn r4os_zig_ops_cache(index: usize, first: u64, count: u64) bool {
    return cache.flushInvalidateRange(index, first, count);
}
export fn r4os_zig_ops_forget(filesystem: u8, index: usize, first: u64, count: u64) void {
    switch (filesystem) {
        0 => fat.forgetStorage(index, first, count),
        1 => ntfs.forgetStorage(index, first, count),
        else => unreachable,
    }
}
export fn r4os_zig_ops_probe(input: *const Probe, context: *anyopaque, consume: *const fn (*anyopaque, *anyopaque) callconv(.c) i32) i32 {
    var volume: ?vfs.Volume = switch (input.filesystem) {
        api.storage_filesystem_fat32 => .{ .fat32 = fat.parseBounded(input.index, input.lba, input.sectors) orelse return api.storage_error_remount },
        api.storage_filesystem_ntfs => .{ .ntfs = ntfs.inspectBounded(input.index, input.lba, input.sectors) orelse return api.storage_error_remount },
        else => return api.storage_error_unsupported,
    };
    return consume(context, &volume);
}
export fn r4os_zig_ops_create_cleanup(entry: *const fn () callconv(.c) noreturn) bool {
    return task.createKernelThreadCriticalWithRole("storage-cleanup", entry, .batch) != null;
}
comptime {
    _ = @import("normal_boot.zig");
    if (@sizeOf(Args) != 96 or @offsetOf(Args, "target") != 40 or @offsetOf(Args, "program_generation") != 72 or @sizeOf(DriveMetadata) != 56 or @sizeOf(VolumeMetadata) != 8 or @sizeOf(Probe) != 32) @compileError("Rust storage operations transport differs");
    if (@intFromEnum(drive.Kind.fat32) != 2 or @intFromEnum(drive.Kind.ntfs) != 3 or @intFromEnum(drive.Role.ram) != 3) @compileError("Rust storage operations drive vocabulary differs");
}
