pub const PAGE_BYTES: u32 = 4096;
pub const MAX_SEGMENTS: usize = 16;
pub const MAX_TRANSFER_BYTES: u32 = PAGE_BYTES * MAX_SEGMENTS;

pub const Segment = struct {
    phys: u64 = 0,
    len: u32 = 0,
};

pub const Plan = struct {
    segments: [MAX_SEGMENTS]Segment = .{Segment{}} ** MAX_SEGMENTS,
    count: u8 = 0,
    bytes: u32 = 0,
};

pub fn build(phys: u64, bytes: u32) ?Plan {
    if (phys == 0 or bytes == 0 or bytes > MAX_TRANSFER_BYTES) return null;
    var out: Plan = .{ .bytes = bytes };
    var cursor = phys;
    var remaining = bytes;
    while (remaining != 0) {
        if (out.count >= MAX_SEGMENTS) return null;
        const page_offset: u32 = @intCast(cursor & (PAGE_BYTES - 1));
        const available = PAGE_BYTES - page_offset;
        const length = @min(remaining, available);
        out.segments[out.count] = .{ .phys = cursor, .len = length };
        out.count += 1;
        cursor += length;
        remaining -= length;
    }
    return out;
}

test "64 KiB transfer becomes a bounded page chain" {
    const testing = @import("std").testing;
    const plan = build(0x10000, MAX_TRANSFER_BYTES) orelse return error.NoPlan;
    try testing.expectEqual(@as(u8, 16), plan.count);
    try testing.expectEqual(@as(u32, MAX_TRANSFER_BYTES), plan.bytes);
    for (plan.segments[0..plan.count]) |segment| {
        try testing.expectEqual(@as(u32, PAGE_BYTES), segment.len);
    }
}

test "unaligned chain never crosses a page boundary" {
    const testing = @import("std").testing;
    const plan = build(0x12ff0, 8192) orelse return error.NoPlan;
    try testing.expectEqual(@as(u8, 3), plan.count);
    try testing.expectEqual(@as(u32, 16), plan.segments[0].len);
    try testing.expectEqual(@as(u32, 4096), plan.segments[1].len);
    try testing.expectEqual(@as(u32, 4080), plan.segments[2].len);
}

test "zero and oversized transfers are rejected" {
    const testing = @import("std").testing;
    try testing.expect(build(0x1000, 0) == null);
    try testing.expect(build(0x1000, MAX_TRANSFER_BYTES + 1) == null);
}
