//! Complete color payload prefixes. No display policy or hardware access.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const Color = abi.GfxOutputColorState;
pub fn capacity(pointer: *const Color) ?u32 {
    if (@intFromPtr(pointer) == 0 or @intFromPtr(pointer) % @alignOf(Color) != 0 or pointer.version != 1 or pointer.size < 128) return null;
    comptime std.debug.assert(@sizeOf(Color) == 192 and @offsetOf(Color, "link_kind") == 128);
    return if (pointer.size >= 192) 192 else 128;
}
pub fn read(pointer: *const Color) ?Color {
    const count = capacity(pointer) orelse return null;
    var value: Color = .{};
    @memcpy(std.mem.asBytes(&value)[0..count], @as([*]const u8, @ptrCast(pointer))[0..count]);
    // The owner receives the current complete representation. Missing optional
    // fields are zero; no byte beyond the released caller prefix was read.
    value.size = @sizeOf(Color);
    return value;
}
pub fn write(pointer: *Color, count: u32, value: Color) void {
    std.debug.assert(count == 128 or count == 192);
    var snapshot = value;
    snapshot.size = count;
    @memcpy(@as([*]u8, @ptrCast(pointer))[0..count], std.mem.asBytes(&snapshot)[0..count]);
}
