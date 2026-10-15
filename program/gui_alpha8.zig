const std = @import("std");

extern fn r4os_rust_program_alpha8_validate(u32, u32, u32, usize, u32, u32, usize, *Geometry) u8;
extern fn r4os_rust_program_alpha8_pack([*]u32, usize, [*]const u8, usize, u32, u32, u32, *usize) u8;
extern fn r4os_rust_program_alpha8_byte([*]const u32, usize, usize, *u8) bool;
fn nativeError(status: u8) Error {
    return switch (status) {
        1 => error.InvalidDimensions,
        2 => error.InvalidStride,
        3 => error.SourceTooSmall,
        4 => error.DestinationTooSmall,
        5 => error.Overflow,
        else => unreachable,
    };
}
comptime {
    if (@sizeOf(Geometry) != 24 or @alignOf(Geometry) != 8 or @offsetOf(Geometry, "pixel_count") != 0 or @offsetOf(Geometry, "source_bytes") != 8 or @offsetOf(Geometry, "packed_words") != 16)
        @compileError("Native Alpha8 original geometry drift");
}


pub const Error = error{
    InvalidDimensions,
    InvalidStride,
    SourceTooSmall,
    DestinationTooSmall,
    Overflow,
};

pub const Geometry = extern struct {
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
    var result: Geometry = undefined;
    const status = r4os_rust_program_alpha8_validate(width, height, stride, source_len, max_width, max_height, max_pixels, &result);
    if (status != 0) return nativeError(status);
    return result;
}

/// Copies a potentially padded Alpha8 source into tightly packed little-endian
/// words.  Padding bytes never cross the transport boundary.
pub fn packCompact(destination: []u32, source: []const u8, width: u32, height: u32, stride: u32) Error!usize {
    var result: usize = undefined;
    const status = r4os_rust_program_alpha8_pack(destination.ptr, destination.len, source.ptr, source.len, width, height, stride, &result);
    if (status != 0) return nativeError(status);
    return result;
}

pub fn packedByte(words: []const u32, byte_index: usize) ?u8 {
    var result: u8 = undefined;
    return if (r4os_rust_program_alpha8_byte(words.ptr, words.len, byte_index, &result)) result else null;
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
