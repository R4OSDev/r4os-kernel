//! Bounded copying and x86 ordering for an already owned resident DMA mapping.
//! Admission/lifetime belong to driver_api; this helper allocates nothing and
//! cannot grant concurrent ownership of the same bytes to CPU and device.
const std = @import("std");
pub const Direction = enum(u32) { bidirectional = 0, to_device = 1, from_device = 2 };
pub const Phase = enum { device, cpu };

pub fn synchronize(original: []u8, bounce: ?[]u8, direction: Direction, phase: Phase, offset: u32, bytes: u32) bool {
    if (bytes == 0 or offset > original.len or bytes > original.len - offset) return false;
    if (bounce) |area| {
        if (area.len != original.len) return false;
    }
    if (phase == .cpu) asm volatile ("mfence" ::: .{ .memory = true });
    if (bounce) |area| {
        if (phase == .device and direction != .from_device)
            @memcpy(area[offset..][0..bytes], original[offset..][0..bytes]);
        if (phase == .cpu and direction != .to_device)
            @memcpy(original[offset..][0..bytes], area[offset..][0..bytes]);
    }
    if (phase == .device) asm volatile ("mfence" ::: .{ .memory = true });
    return true;
}

test "DMA range synchronization preserves independently updated peer fields and rejects invalid extents atomically" {
    const t = std.testing;
    var original: [256]u8 = @splat(0x11);
    var device: [256]u8 = @splat(0x22);
    try t.expect(synchronize(&original, &device, .bidirectional, .device, 0, 256));
    try t.expectEqualSlices(u8, &original, &device);
    // CPU owns a command field; the device independently publishes a status.
    @memset(original[16..20], 0x44);
    @memset(device[48..52], 0x88);
    try t.expect(synchronize(&original, &device, .bidirectional, .device, 16, 4));
    try t.expect(std.mem.allEqual(u8, device[16..20], 0x44));
    try t.expect(std.mem.allEqual(u8, device[48..52], 0x88));
    // A second CPU field has not been published; receiving status must keep it.
    original[32] = 0x66;
    try t.expect(synchronize(&original, &device, .bidirectional, .cpu, 48, 4));
    try t.expect(std.mem.allEqual(u8, original[48..52], 0x88));
    try t.expectEqual(@as(u8, 0x66), original[32]);
    const before_cpu = original;
    const before_device = device;
    for ([_][2]u32{ .{ 0, 0 }, .{ 256, 1 }, .{ 255, 2 }, .{ 0xffffffff, 1 }, .{ 1, 0xffffffff } }) |range| {
        for ([_]Phase{ .device, .cpu }) |phase| {
            try t.expect(!synchronize(&original, &device, .bidirectional, phase, range[0], range[1]));
            try t.expectEqualSlices(u8, &before_cpu, &original);
            try t.expectEqualSlices(u8, &before_device, &device);
        }
    }
    try t.expect(!synchronize(&original, device[0..255], .bidirectional, .cpu, 0, 1));
    for ([_]Direction{ .to_device, .from_device }) |direction| {
        @memset(&original, 0x33);
        @memset(&device, 0x77);
        try t.expect(synchronize(&original, &device, direction, .device, 255, 1));
        try t.expectEqual(@as(u8, if (direction == .to_device) 0x33 else 0x77), device[255]);
        try t.expect(synchronize(&original, &device, direction, .cpu, 0, 1));
        try t.expectEqual(@as(u8, if (direction == .from_device) 0x77 else 0x33), original[0]);
        try t.expect(std.mem.allEqual(u8, original[1..255], 0x33));
        try t.expect(std.mem.allEqual(u8, device[0..255], 0x77));
    }
    const direct = original;
    try t.expect(synchronize(&original, null, .bidirectional, .device, 17, 23));
    try t.expect(synchronize(&original, null, .bidirectional, .cpu, 17, 23));
    try t.expectEqualSlices(u8, &direct, &original);
}
