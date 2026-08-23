const std = @import("std");

/// One backend-sized part of a logically contiguous block transfer.
pub const Chunk = struct {
    sector_offset: u16,
    sectors: u16,
    byte_offset: usize,
    byte_count: usize,
};

/// Splits a logical transfer without changing its order.  A backend limit of
/// zero means that the complete logical request may be submitted at once.
pub const Iterator = struct {
    total_sectors: u16,
    max_sectors: u16,
    sector_size: usize,
    sector_offset: u16 = 0,

    pub fn init(total_sectors: u16, max_sectors: u16, sector_size: usize) ?Iterator {
        if (total_sectors == 0 or sector_size == 0) return null;
        return .{
            .total_sectors = total_sectors,
            .max_sectors = if (max_sectors == 0) total_sectors else max_sectors,
            .sector_size = sector_size,
        };
    }

    pub fn next(self: *Iterator) ?Chunk {
        if (self.sector_offset >= self.total_sectors) return null;
        const remaining = self.total_sectors - self.sector_offset;
        const sectors = @min(remaining, self.max_sectors);
        const sector_offset = self.sector_offset;
        self.sector_offset += sectors;
        return .{
            .sector_offset = sector_offset,
            .sectors = sectors,
            .byte_offset = @as(usize, sector_offset) * self.sector_size,
            .byte_count = @as(usize, sectors) * self.sector_size,
        };
    }
};

fn expectSplit(total: u16, expected: []const u16) !void {
    var iterator = Iterator.init(total, 8, 512) orelse return error.InvalidIterator;
    var completed: u16 = 0;
    for (expected) |expected_sectors| {
        const chunk = iterator.next() orelse return error.MissingChunk;
        try std.testing.expectEqual(completed, chunk.sector_offset);
        try std.testing.expectEqual(expected_sectors, chunk.sectors);
        try std.testing.expectEqual(@as(usize, completed) * 512, chunk.byte_offset);
        try std.testing.expectEqual(@as(usize, expected_sectors) * 512, chunk.byte_count);
        completed += expected_sectors;
    }
    try std.testing.expectEqual(total, completed);
    try std.testing.expect(iterator.next() == null);
}

test "backend limit preserves exact logical prefix order" {
    try expectSplit(1, &.{1});
    try expectSplit(8, &.{8});
    try expectSplit(9, &.{ 8, 1 });
    try expectSplit(16, &.{ 8, 8 });
    try expectSplit(64, &.{ 8, 8, 8, 8, 8, 8, 8, 8 });
    try expectSplit(128, &.{ 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8 });
}

test "zero backend limit keeps one logical request" {
    var iterator = Iterator.init(128, 0, 4096) orelse return error.InvalidIterator;
    const chunk = iterator.next() orelse return error.MissingChunk;
    try std.testing.expectEqual(@as(u16, 128), chunk.sectors);
    try std.testing.expectEqual(@as(usize, 128 * 4096), chunk.byte_count);
    try std.testing.expect(iterator.next() == null);
}
