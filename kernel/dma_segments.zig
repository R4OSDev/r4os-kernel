const std = @import("std");

pub const page_size: u64 = 4096;
pub const max_segments: usize = 64;

pub const Segment = extern struct {
    phys_addr: u64 = 0,
    bytes: u32 = 0,
    reserved: u32 = 0,
};

pub const SegmentList = struct {
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

pub fn buildFromFrames(
    frames: []const u64,
    first_offset: u32,
    bytes: u32,
    constraints: Constraints,
) Error!SegmentList {
    try validateConstraints(constraints);
    if (bytes == 0 or first_offset >= page_size) return error.InvalidArgument;

    const covered = std.math.add(u64, first_offset, bytes) catch return error.Overflow;
    const required_frames = (covered + page_size - 1) / page_size;
    if (required_frames != frames.len) return error.InvalidArgument;

    var result: SegmentList = .{};
    var remaining: u64 = bytes;
    var index: usize = 0;
    while (index < frames.len) : (index += 1) {
        const frame = frames[index];
        if ((frame & (page_size - 1)) != 0) return error.InvalidArgument;
        const offset: u64 = if (index == 0) first_offset else 0;
        const available = page_size - offset;
        const take = @min(remaining, available);
        try appendPhysicalRange(&result, frame + offset, take, constraints);
        remaining -= take;
    }
    if (remaining != 0) return error.InvalidArgument;
    return result;
}

pub fn constrain(source: *const SegmentList, constraints: Constraints) Error!SegmentList {
    try validateConstraints(constraints);
    if (source.count == 0 or source.count > max_segments) return error.InvalidArgument;
    var result: SegmentList = .{};
    var index: usize = 0;
    while (index < source.count) : (index += 1) {
        const segment = source.segments[index];
        if (segment.bytes == 0) return error.InvalidArgument;
        try appendPhysicalRange(&result, segment.phys_addr, segment.bytes, constraints);
    }
    return result;
}

pub fn singleRange(phys_addr: u64, bytes: u32, constraints: Constraints) Error!SegmentList {
    try validateConstraints(constraints);
    if (bytes == 0) return error.InvalidArgument;
    var result: SegmentList = .{};
    try appendPhysicalRange(&result, phys_addr, bytes, constraints);
    return result;
}

pub fn appendRange(list: *SegmentList, phys_addr: u64, bytes: u32, constraints: Constraints) Error!void {
    try validateConstraints(constraints);
    if (bytes == 0) return error.InvalidArgument;
    try appendPhysicalRange(list, phys_addr, bytes, constraints);
}

fn validateConstraints(constraints: Constraints) Error!void {
    if (constraints.max_segment_count == 0 or constraints.max_segment_count > max_segments) {
        return error.InvalidArgument;
    }
    if (!isPowerOfTwoOrZero(constraints.boundary)) return error.InvalidArgument;
    if (!isPowerOfTwo(constraints.alignment)) return error.InvalidArgument;
    if (constraints.boundary != 0 and constraints.boundary < constraints.alignment) {
        return error.InvalidArgument;
    }
}

fn appendPhysicalRange(
    list: *SegmentList,
    start: u64,
    byte_count: u64,
    constraints: Constraints,
) Error!void {
    if (byte_count == 0) return;
    const last = std.math.add(u64, start, byte_count - 1) catch return error.Overflow;
    if (last > constraints.dma_mask) return error.AddressLimit;
    const max_segment_bytes: u64 = if (constraints.max_segment_bytes == 0)
        std.math.maxInt(u32)
    else
        constraints.max_segment_bytes;
    var current = start;
    var remaining = byte_count;
    while (remaining != 0) {
        var take = @min(remaining, max_segment_bytes);
        if (constraints.boundary != 0) {
            const boundary_offset = current & (constraints.boundary - 1);
            take = @min(take, constraints.boundary - boundary_offset);
        }
        // If this physical run needs another descriptor, keep its next start
        // aligned as well. The final descriptor may have any byte length.
        if (take < remaining and constraints.alignment > 1) {
            take &= ~(@as(u64, constraints.alignment) - 1);
        }
        if (take == 0 or take > std.math.maxInt(u32)) return error.Overflow;

        if (list.count != 0) {
            const previous_index: usize = list.count - 1;
            var previous = &list.segments[previous_index];
            const previous_end = std.math.add(u64, previous.phys_addr, previous.bytes) catch return error.Overflow;
            const merged_bytes = @as(u64, previous.bytes) + take;
            const same_boundary = constraints.boundary == 0 or
                (previous.phys_addr / constraints.boundary == current / constraints.boundary);
            if (previous_end == current and merged_bytes <= max_segment_bytes and same_boundary) {
                previous.bytes = @intCast(merged_bytes);
                current += take;
                remaining -= take;
                continue;
            }
        }

        if ((current & (@as(u64, constraints.alignment) - 1)) != 0) return error.Alignment;
        if (list.count >= constraints.max_segment_count) return error.TooManySegments;
        const target: usize = list.count;
        list.segments[target] = .{
            .phys_addr = current,
            .bytes = @intCast(take),
        };
        list.count += 1;
        current += take;
        remaining -= take;
    }
}

pub fn nextGeneration(previous: u64) u64 {
    const generation = (previous +% 1) & 0x00FF_FFFF_FFFF_FFFF;
    return if (generation == 0) 1 else generation;
}

pub fn makeHandle(slot: usize, generation: u64) ?u64 {
    if (slot >= 255 or generation == 0 or generation > 0x00FF_FFFF_FFFF_FFFF) return null;
    return (generation << 8) | @as(u64, @intCast(slot + 1));
}

pub fn handleSlot(handle: u64, capacity: usize) ?usize {
    const encoded = handle & 0xFF;
    if (encoded == 0) return null;
    const slot: usize = @intCast(encoded - 1);
    return if (slot < capacity) slot else null;
}

pub fn handleGeneration(handle: u64) u64 {
    return handle >> 8;
}

fn isPowerOfTwo(value: u32) bool {
    return value != 0 and (value & (value - 1)) == 0;
}

fn isPowerOfTwoOrZero(value: u64) bool {
    return value == 0 or (value & (value - 1)) == 0;
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
