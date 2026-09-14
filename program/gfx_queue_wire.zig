// Complete extensible payload prefixes. Capacity between two revisions does
// not authorize reading or writing a partial new field group.
const std = @import("std");
const abi = @import("r4os_kernel_contract");

pub fn prefix(comptime T: type, available: u32) ?u32 {
    const sizes = comptime if (T == abi.GfxBackendRegistration) &[_]u32{ 32, 40, 48 }
        else if (T == abi.GfxBackendInfo) &[_]u32{ 136, 144, 152 }
        else if (T == abi.GfxSubmission) &[_]u32{ 408, 432, 512 }
        else if (T == abi.GfxDriverJob) &[_]u32{ 112, 136, 224, 272 }
        else @compileError("Unsupported graphics queue payload");
    comptime std.debug.assert(sizes[sizes.len - 1] == @sizeOf(T));
    var size: ?u32 = null;
    for (sizes) |n| if (n <= available) { size = n; };
    return size;
}

pub fn read(comptime T: type, input: *const T) ?T {
    if (@intFromPtr(input) == 0 or input.version != 1) return null;
    const size = prefix(T, input.size) orelse return null;
    var value: T = .{};
    @memcpy(std.mem.asBytes(&value)[0..size], std.mem.asBytes(input)[0..size]);
    value.size = size;
    return value;
}

pub fn capacity(comptime T: type, output: *T) ?u32 {
    if (@intFromPtr(output) == 0 or output.version != 1) return null;
    return prefix(T, output.size);
}

pub fn write(comptime T: type, output: *T, size: u32, value: T) void {
    std.debug.assert(prefix(T, size) == size);
    var snapshot = value;
    snapshot.size = size;
    @memcpy(std.mem.asBytes(output)[0..size], std.mem.asBytes(&snapshot)[0..size]);
}
