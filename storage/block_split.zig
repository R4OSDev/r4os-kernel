const std = @import("std");
const owner_test = if (@import("builtin").is_test) @import("rust_owner_test") else struct {};
comptime {
    if (@import("builtin").is_test) _ = owner_test;
}
pub const Chunk = extern struct { sector_offset: u16, sectors: u16, byte_offset: usize, byte_count: usize };
pub const Iterator = extern struct {
    total_sectors: u16,
    max_sectors: u16,
    sector_size: usize,
    sector_offset: u16 = 0,
    pub fn init(total: u16, max: u16, size: usize) ?Iterator {
        var out: Iterator = undefined;
        return if (r4os_rust_storage_split_init(total, max, size, &out)) out else null;
    }
    pub fn next(self: *Iterator) ?Chunk {
        var out: Chunk = undefined;
        return if (r4os_rust_storage_split_next(self, &out)) out else null;
    }
};
extern fn r4os_rust_storage_split_init(total: u16, max: u16, size: usize, out: *Iterator) bool;
extern fn r4os_rust_storage_split_next(state: *Iterator, out: *Chunk) bool;
comptime {
    if (@sizeOf(Iterator) != 24 or @offsetOf(Iterator, "sector_size") != 8 or @sizeOf(Chunk) != 24) @compileError("private block split C layout drift");
}

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
