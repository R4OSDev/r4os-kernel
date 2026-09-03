const std = @import("std");

pub const max_extent_pages: u64 = 64;

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
