const std = @import("std");

pub const Error = error{
    InvalidDimensions,
    InvalidStride,
    SourceTooSmall,
    DestinationTooSmall,
    Overflow,
};

pub const Geometry = struct {
    pixel_count: usize,
    source_bytes: usize,
    packed_words: usize,
};

pub fn validate(
    width: u32,
    height: u32,
    stride: u32,
    source_len: usize,
    max_width: u32,
    max_height: u32,
    max_pixels: usize,
) Error!Geometry {
    if (width == 0 or height == 0 or width > max_width or height > max_height) return error.InvalidDimensions;
    if (stride < width) return error.InvalidStride;

    const pixel_count = std.math.mul(usize, @as(usize, width), @as(usize, height)) catch return error.Overflow;
    if (pixel_count > max_pixels) return error.InvalidDimensions;
    const preceding_rows = std.math.mul(usize, @as(usize, height - 1), @as(usize, stride)) catch return error.Overflow;
    const source_bytes = std.math.add(usize, preceding_rows, @as(usize, width)) catch return error.Overflow;
    if (source_len < source_bytes) return error.SourceTooSmall;
    const rounded = std.math.add(usize, pixel_count, 3) catch return error.Overflow;
    return .{
        .pixel_count = pixel_count,
        .source_bytes = source_bytes,
        .packed_words = rounded / 4,
    };
}

/// Copies a potentially padded Alpha8 source into tightly packed little-endian
/// words.  Padding bytes never cross the transport boundary.
pub fn packCompact(destination: []u32, source: []const u8, width: u32, height: u32, stride: u32) Error!usize {
    const geometry = try validate(width, height, stride, source.len, width, height, std.math.maxInt(usize));
    if (destination.len < geometry.packed_words) return error.DestinationTooSmall;
    @memset(destination[0..geometry.packed_words], 0);

    const row_width: usize = @intCast(width);
    const row_stride: usize = @intCast(stride);
    var output_index: usize = 0;
    var row: usize = 0;
    while (row < @as(usize, height)) : (row += 1) {
        const source_row = row * row_stride;
        var column: usize = 0;
        while (column < row_width) : (column += 1) {
            const shift: u5 = @intCast((output_index & 3) * 8);
            destination[output_index / 4] |= @as(u32, source[source_row + column]) << shift;
            output_index += 1;
        }
    }
    return geometry.packed_words;
}

pub fn packedByte(words: []const u32, byte_index: usize) ?u8 {
    if (byte_index / 4 >= words.len) return null;
    const shift: u5 = @intCast((byte_index & 3) * 8);
    return @truncate(words[byte_index / 4] >> shift);
}

test "alpha8 transport validates padded sources and packs only visible bytes" {
    const source = [_]u8{
        0, 64, 255, 0xEE, 0xEE,
        1, 2,  3,   0xDD, 0xDD,
    };
    const geometry = try validate(3, 2, 5, source.len, 512, 512, 512 * 512);
    try std.testing.expectEqual(@as(usize, 6), geometry.pixel_count);
    try std.testing.expectEqual(@as(usize, 8), geometry.source_bytes);
    try std.testing.expectEqual(@as(usize, 2), geometry.packed_words);

    var words: [2]u32 = undefined;
    try std.testing.expectEqual(@as(usize, 2), try packCompact(words[0..], source[0..], 3, 2, 5));
    const expected = [_]u8{ 0, 64, 255, 1, 2, 3 };
    for (expected, 0..) |value, index| try std.testing.expectEqual(value, packedByte(words[0..], index).?);
}

test "alpha8 transport rejects dimensions stride length and capacity before copy" {
    var source: [16]u8 = .{0} ** 16;
    var words: [1]u32 = .{0};
    try std.testing.expectError(error.InvalidDimensions, validate(0, 1, 1, source.len, 512, 512, 512 * 512));
    try std.testing.expectError(error.InvalidDimensions, validate(513, 1, 513, source.len, 512, 512, 512 * 512));
    try std.testing.expectError(error.InvalidStride, validate(4, 2, 3, source.len, 512, 512, 512 * 512));
    try std.testing.expectError(error.SourceTooSmall, validate(4, 2, 8, 11, 512, 512, 512 * 512));
    try std.testing.expectError(error.DestinationTooSmall, packCompact(words[0..], source[0..], 4, 2, 4));
}
