const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const page_size: u64 = 4096;
pub const max_segments: usize = 64;

pub const Segment = extern struct {
    phys_addr: u64 = 0,
    bytes: u32 = 0,
    reserved: u32 = 0,
};

pub const SegmentList = extern struct {
    count: u16 = 0,
    segments: [max_segments]Segment = .{Segment{}} ** max_segments,
};

pub const Constraints = struct {
    dma_mask: u64 = std.math.maxInt(u64),
    boundary: u64 = 0,
    max_segment_bytes: u32 = 0,
    alignment: u32 = 1,
    max_segment_count: u16 = max_segments,
};

pub const Error = error{
    InvalidArgument,
    AddressLimit,
    Alignment,
    TooManySegments,
    Overflow,
};

const RawConstraints = extern struct {
    dma_mask: u64,
    boundary: u64,
    max_segment_bytes: u32,
    alignment: u32,
    max_segment_count: u16,
};
comptime {
    if (@sizeOf(Segment) != 16 or @sizeOf(SegmentList) != 1032 or @offsetOf(SegmentList, "segments") != 8 or
        @sizeOf(RawConstraints) != 32 or @offsetOf(RawConstraints, "max_segment_count") != 24)
        @compileError("Native DMA geometry C layout drift");
}
extern fn r4os_rust_dma_from_frames(frames: [*]const u64, len: usize, first_offset: u32, bytes: u32, constraints: *const RawConstraints, output: *SegmentList) u8;
extern fn r4os_rust_dma_constrain(source: *const SegmentList, constraints: *const RawConstraints, output: *SegmentList) u8;
extern fn r4os_rust_dma_range(output: *SegmentList, start: u64, bytes: u32, constraints: *const RawConstraints, append_existing: bool) u8;
extern fn r4os_rust_dma_handle(operation: u8, a: u64, b: u64) u64;
fn raw(c: Constraints) RawConstraints {
    return .{ .dma_mask = c.dma_mask, .boundary = c.boundary, .max_segment_bytes = c.max_segment_bytes, .alignment = c.alignment, .max_segment_count = c.max_segment_count };
}
fn checked(code: u8) Error!void {
    return switch (code) {
        0 => {},
        1 => error.InvalidArgument,
        2 => error.AddressLimit,
        3 => error.Alignment,
        4 => error.TooManySegments,
        5 => error.Overflow,
        else => @panic("Invalid native DMA geometry status"),
    };
}
pub fn buildFromFrames(frames: []const u64, first_offset: u32, bytes: u32, constraints: Constraints) Error!SegmentList {
    const c = raw(constraints);
    var output: SegmentList = undefined;
    try checked(r4os_rust_dma_from_frames(frames.ptr, frames.len, first_offset, bytes, &c, &output));
    return output;
}
pub fn constrain(source: *const SegmentList, constraints: Constraints) Error!SegmentList {
    const c = raw(constraints);
    var output: SegmentList = undefined;
    try checked(r4os_rust_dma_constrain(source, &c, &output));
    return output;
}
pub fn singleRange(phys_addr: u64, bytes: u32, constraints: Constraints) Error!SegmentList {
    const c = raw(constraints);
    var output: SegmentList = undefined;
    try checked(r4os_rust_dma_range(&output, phys_addr, bytes, &c, false));
    return output;
}
pub fn appendRange(list: *SegmentList, phys_addr: u64, bytes: u32, constraints: Constraints) Error!void {
    const c = raw(constraints);
    try checked(r4os_rust_dma_range(list, phys_addr, bytes, &c, true));
}
pub fn nextGeneration(previous: u64) u64 {
    return r4os_rust_dma_handle(0, previous, 0);
}
pub fn makeHandle(slot: usize, generation: u64) ?u64 {
    const value = r4os_rust_dma_handle(1, slot, generation);
    return if (value == 0) null else value;
}
pub fn handleSlot(handle: u64, capacity: usize) ?usize {
    const value = r4os_rust_dma_handle(2, handle, capacity);
    return if (value == std.math.maxInt(u64)) null else @intCast(value);
}
pub fn handleGeneration(handle: u64) u64 {
    return r4os_rust_dma_handle(3, handle, 0);
}

test "non-contiguous frames merge only adjacent physical runs" {
    const frames = [_]u64{ 0x1000, 0x2000, 0x9000, 0xA000 };
    const list = try buildFromFrames(frames[0..], 128, 4 * 4096 - 256, .{});
    try std.testing.expectEqual(@as(u16, 2), list.count);
    try std.testing.expectEqual(@as(u64, 0x1080), list.segments[0].phys_addr);
    try std.testing.expectEqual(@as(u32, 8064), list.segments[0].bytes);
    try std.testing.expectEqual(@as(u64, 0x9000), list.segments[1].phys_addr);
    try std.testing.expectEqual(@as(u32, 8064), list.segments[1].bytes);
}

test "mask boundary and maximum segment size are hard limits" {
    const source = try singleRange(0x00FF_E000, 0x2000, .{});
    const split = try constrain(&source, .{
        .dma_mask = 0x00FF_FFFF,
        .boundary = 0x1000,
        .max_segment_bytes = 0x800,
        .alignment = 0x100,
        .max_segment_count = 4,
    });
    try std.testing.expectEqual(@as(u16, 4), split.count);
    try std.testing.expectEqual(@as(u32, 0x800), split.segments[0].bytes);
    try std.testing.expectError(error.AddressLimit, singleRange(0xFFFF_F000, 0x2000, .{
        .dma_mask = 0xFFFF_FFFF,
    }));
}

test "segment overflow leaves no partial public result" {
    const frames = [_]u64{ 0x1000, 0x3000, 0x5000 };
    try std.testing.expectError(error.TooManySegments, buildFromFrames(frames[0..], 0, 3 * 4096, .{
        .max_segment_count = 2,
    }));
}

test "every emitted segment start satisfies alignment" {
    const split = try singleRange(0x1000, 7000, .{
        .max_segment_bytes = 3000,
        .alignment = 256,
    });
    try std.testing.expectEqual(@as(u16, 3), split.count);
    for (split.segments[0..split.count]) |segment| {
        try std.testing.expectEqual(@as(u64, 0), segment.phys_addr & 255);
    }
    try std.testing.expectError(error.InvalidArgument, singleRange(0x1000, 4096, .{
        .boundary = 128,
        .alignment = 256,
    }));
}

test "generation handles reject zero and stale generations" {
    const first_generation = nextGeneration(0);
    const first = makeHandle(3, first_generation) orelse return error.BadHandle;
    const second_generation = nextGeneration(first_generation);
    const second = makeHandle(3, second_generation) orelse return error.BadHandle;
    try std.testing.expectEqual(@as(usize, 3), handleSlot(first, 8).?);
    try std.testing.expectEqual(first_generation, handleGeneration(first));
    try std.testing.expect(first != second);
    try std.testing.expect(handleSlot(0, 8) == null);
}
