const std = @import("std");

pub const max_extent_pages: u64 = 64;
pub const max_translation_bytes: u64 = 256 * 1024 * 1024;

/// Accumulate fresh translations while the caller retains the virtual range.
/// A leaf covers every byte to its boundary, including huge leaves. This
/// stores only values; no page-table pointer survives an owner handoff.
pub const ContiguousTranslation = struct {
    virtual: u64,
    bytes: u64,
    consumed: u64 = 0,
    physical: ?u64 = null,

    pub fn init(virtual: u64, bytes: u64) ?ContiguousTranslation {
        if (bytes == 0 or bytes > max_translation_bytes or bytes - 1 > std.math.maxInt(u64) - virtual) return null;
        return .{ .virtual = virtual, .bytes = bytes };
    }
    pub fn accept(self: *ContiguousTranslation, address: u64, leaf_remaining: u64) bool {
        if (self.consumed >= self.bytes or leaf_remaining == 0) return false;
        const count = @min(self.bytes - self.consumed, leaf_remaining);
        if (count - 1 > std.math.maxInt(u64) - address) return false;
        if (self.physical) |base| {
            if (self.consumed > std.math.maxInt(u64) - base or address != base + self.consumed) return false;
        } else self.physical = address;
        self.consumed += count;
        return true;
    }
    pub fn result(self: *const ContiguousTranslation) ?u64 {
        return if (self.consumed == self.bytes) self.physical else null;
    }
};

pub fn boundedPageCount(remaining_pages: u64) u64 {
    return @min(remaining_pages, max_extent_pages);
}

/// Counts the naturally contiguous prefix of already acquired frames. It is
/// deliberately not a physical-memory search and therefore cannot turn an
/// ordinary VM commit into an unbounded contiguous-allocation scan.
pub fn contiguousPrefix(frames: []const u64, page_size: u64) usize {
    if (frames.len == 0 or page_size == 0) return 0;
    var count: usize = 1;
    while (count < frames.len) : (count += 1) {
        if (frames[count] != frames[0] +% @as(u64, @intCast(count)) *% page_size) break;
    }
    return count;
}

test "batch size remains stack and latency bounded" {
    try std.testing.expectEqual(@as(u64, 0), boundedPageCount(0));
    try std.testing.expectEqual(@as(u64, 7), boundedPageCount(7));
    try std.testing.expectEqual(max_extent_pages, boundedPageCount(1000));
}

test "natural extent grouping stops at the first discontinuity" {
    const frames = [_]u64{ 0x1000, 0x2000, 0x3000, 0x9000, 0xA000 };
    try std.testing.expectEqual(@as(usize, 3), contiguousPrefix(frames[0..], 4096));
    try std.testing.expectEqual(@as(usize, 2), contiguousPrefix(frames[3..], 4096));
    try std.testing.expectEqual(@as(usize, 0), contiguousPrefix(frames[0..0], 4096));
}

test "fresh contiguous translation covers partial huge leaves and rejects late holes and overflow" {
    const t = std.testing;
    const start: u64 = 0x1ff123;
    var mixed = ContiguousTranslation.init(start, 0x200000 + 0x1000).?;
    try t.expect(mixed.accept(start + 0x40000000, 0x200000 - start));
    try t.expect(mixed.result() == null);
    try t.expect(mixed.accept(0x40200000, 0x200000));
    try t.expect(mixed.result() == null);
    try t.expect(mixed.accept(0x40400000, 0x1000));
    try t.expectEqual(@as(?u64, start + 0x40000000), mixed.result());
    try t.expectEqual(mixed.bytes, mixed.consumed);
    // A bad final 4K leaf cannot be hidden by the previous owner chunk or
    // by checking only the first and last addresses of a larger range.
    for ([_]u64{ 0, 0x80000000 }) |bad_span| {
        var late = ContiguousTranslation.init(0x1000, 65 * 4096 + 1).?;
        for (0..64) |i| try t.expect(late.accept(0x100000 + i * 4096, 4096));
        const consumed = late.consumed;
        try t.expect(!late.accept(bad_span, if (bad_span == 0) 0 else 4096));
        try t.expectEqual(consumed, late.consumed);
        try t.expect(late.result() == null);
        try t.expect(late.accept(0x140000, 4096) and late.result() == null);
        try t.expect(late.accept(0x141000, 4096));
        try t.expectEqual(@as(?u64, 0x100000), late.result());
        try t.expectEqual(@as(u64, 65 * 4096 + 1), late.consumed);
    }
    try t.expect(ContiguousTranslation.init(0, 0) == null);
    try t.expect(ContiguousTranslation.init(0, max_translation_bytes + 1) == null);
    try t.expect(ContiguousTranslation.init(std.math.maxInt(u64), 2) == null);
    var overflow = ContiguousTranslation.init(0, 4096).?;
    try t.expect(!overflow.accept(std.math.maxInt(u64) - 4094, 4096));
    try t.expect(overflow.result() == null and overflow.consumed == 0);
    try t.expect(overflow.accept(std.math.maxInt(u64) - 4095, 4096));
    try t.expectEqual(@as(?u64, std.math.maxInt(u64) - 4095), overflow.result());
}
