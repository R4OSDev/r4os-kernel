comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native USB owner: stateless compatibility facade.
extern fn r4os_rust_usb_trb_chain(u64, u32, *Plan) bool;
pub const PAGE_BYTES: u32 = 4096;
pub const MAX_SEGMENTS: usize = 16;
pub const MAX_TRANSFER_BYTES: u32 = PAGE_BYTES * MAX_SEGMENTS;

pub const Segment = extern struct {
    phys: u64 = 0,
    len: u32 = 0,
};

pub const Plan = extern struct {
    segments: [MAX_SEGMENTS]Segment = .{Segment{}} ** MAX_SEGMENTS,
    count: u8 = 0,
    bytes: u32 = 0,
};

pub fn build(phys: u64, bytes: u32) ?Plan {
    var out: Plan = undefined;
    return if (r4os_rust_usb_trb_chain(phys, bytes, &out)) out else null;
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

comptime {
    if (@sizeOf(Segment) != 16 or
        @alignOf(Segment) != 8 or
        @offsetOf(Segment, "phys") != 0 or
        @offsetOf(Segment, "len") != 8) @compileError("USB Segment private C layout");
}

comptime {
    if (@sizeOf(Plan) != 264 or
        @alignOf(Plan) != 8 or
        @offsetOf(Plan, "segments") != 0 or
        @offsetOf(Plan, "count") != 256 or
        @offsetOf(Plan, "bytes") != 260) @compileError("USB Plan private C layout");
}
