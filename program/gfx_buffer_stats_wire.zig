//! Released statistics prefixes; no allocation, budget or residency policy.
const std = @import("std");
const a = @import("r4os_kernel_contract");
pub fn capacity(pointer: *const a.GfxBufferStats) ?u32 {
    if (@intFromPtr(pointer) == 0 or @intFromPtr(pointer) % @alignOf(a.GfxBufferStats) != 0 or
        pointer.version != 1 or pointer.size < 56) return null;
    comptime std.debug.assert(@sizeOf(a.GfxBufferStats) == 136 and @offsetOf(a.GfxBufferStats, "system_bytes") == 56);
    return if (pointer.size >= 136) 136 else 56;
}
pub fn write(pointer: *a.GfxBufferStats, count: u32, value: a.GfxBufferStats) void {
    std.debug.assert(count == 56 or count == 136);
    var snapshot = value;
    snapshot.size = count;
    @memcpy(@as([*]u8, @ptrCast(pointer))[0..count], std.mem.asBytes(&snapshot)[0..count]);
}
