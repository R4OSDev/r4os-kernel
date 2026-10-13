const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const magic = "R4B0";
pub const version: u16 = 1;
pub const width: u32 = 1280;
pub const height: u32 = 720;
pub const header_size: usize = 32;
pub const pixel_format_xrgb32_rgb: u32 = 1;
pub const payload_size: usize = @as(usize, width) * @as(usize, height) * @sizeOf(u32);
pub const total_size: usize = header_size + payload_size;

pub const Header = extern struct {
    width: u32,
    height: u32,
    pixel_format: u32,
    pixel_offset: usize,
    payload_size: usize,
};

extern fn r4os_rust_r4b_read([*]const u8, usize, ?*Header, bool) callconv(.c) bool;
pub fn valid(bytes: []const u8) bool {
    return r4os_rust_r4b_read(bytes.ptr, bytes.len, null, true);
}
pub fn read(bytes: []const u8) ?Header {
    var result: Header = undefined;
    return if (r4os_rust_r4b_read(bytes.ptr, bytes.len, &result, true)) result else null;
}
pub fn readFields(bytes: []const u8) ?Header {
    var result: Header = undefined;
    return if (r4os_rust_r4b_read(bytes.ptr, bytes.len, &result, false)) result else null;
}

pub fn readLe16(bytes: []const u8) u16 {
    return @as(u16, bytes[0]) | (@as(u16, bytes[1]) << 8);
}

pub fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

fn writeLe16(out: []u8, value: u16) void {
    out[0] = @intCast(value & 0xff);
    out[1] = @intCast(value >> 8);
}

fn writeLe32(out: []u8, value: u32) void {
    out[0] = @intCast(value & 0xff);
    out[1] = @intCast((value >> 8) & 0xff);
    out[2] = @intCast((value >> 16) & 0xff);
    out[3] = @intCast(value >> 24);
}

fn writeTestHeader(out: []u8) void {
    @memset(out[0..header_size], 0);
    @memcpy(out[0..4], magic);
    writeLe16(out[4..6], version);
    writeLe16(out[6..8], @intCast(header_size));
    writeLe32(out[8..12], width);
    writeLe32(out[12..16], height);
    writeLe32(out[16..20], pixel_format_xrgb32_rgb);
    writeLe32(out[20..24], @intCast(header_size));
    writeLe32(out[24..28], @intCast(payload_size));
}

test "R4B0 header fields match bootscreen contract" {
    var header_bytes: [header_size]u8 = .{0} ** header_size;
    writeTestHeader(header_bytes[0..]);
    const header = readFields(header_bytes[0..]) orelse return error.BadHeader;
    try std.testing.expectEqual(@as(u32, width), header.width);
    try std.testing.expectEqual(@as(u32, height), header.height);
    try std.testing.expectEqual(@as(u32, pixel_format_xrgb32_rgb), header.pixel_format);
    try std.testing.expectEqual(@as(usize, header_size), header.pixel_offset);
    try std.testing.expectEqual(@as(usize, payload_size), header.payload_size);
    try std.testing.expect(!valid(header_bytes[0..]));
}

test "R4B0 header rejects wrong magic size and format" {
    var header_bytes: [header_size]u8 = .{0} ** header_size;
    writeTestHeader(header_bytes[0..]);

    header_bytes[0] = 'X';
    try std.testing.expect(readFields(header_bytes[0..]) == null);

    writeTestHeader(header_bytes[0..]);
    writeLe32(header_bytes[8..12], 1024);
    try std.testing.expect(readFields(header_bytes[0..]) == null);

    writeTestHeader(header_bytes[0..]);
    writeLe32(header_bytes[16..20], 99);
    try std.testing.expect(readFields(header_bytes[0..]) == null);
}
