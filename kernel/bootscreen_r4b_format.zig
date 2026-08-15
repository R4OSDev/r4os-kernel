const std = @import("std");

pub const magic = "R4B0";
pub const version: u16 = 1;
pub const width: u32 = 1280;
pub const height: u32 = 720;
pub const header_size: usize = 32;
pub const pixel_format_xrgb32_rgb: u32 = 1;
pub const payload_size: usize = @as(usize, width) * @as(usize, height) * @sizeOf(u32);
pub const total_size: usize = header_size + payload_size;

pub const Header = struct {
    width: u32,
    height: u32,
    pixel_format: u32,
    pixel_offset: usize,
    payload_size: usize,
};

pub fn valid(bytes: []const u8) bool {
    return read(bytes) != null;
}

pub fn read(bytes: []const u8) ?Header {
    const header = readFields(bytes) orelse return null;
    if (header.pixel_offset != header_size) return null;
    if (header.payload_size != payload_size) return null;
    if (bytes.len < header.pixel_offset + header.payload_size) return null;
    return header;
}

pub fn readFields(bytes: []const u8) ?Header {
    if (bytes.len < header_size) return null;
    if (!std.mem.eql(u8, bytes[0..4], magic)) return null;
    if (readLe16(bytes[4..6]) != version) return null;
    if (readLe16(bytes[6..8]) != header_size) return null;
    const raw_width = readLe32(bytes[8..12]);
    const raw_height = readLe32(bytes[12..16]);
    const pixel_format = readLe32(bytes[16..20]);
    const pixel_offset = readLe32(bytes[20..24]);
    const raw_payload_size = readLe32(bytes[24..28]);
    if (raw_width != width or raw_height != height) return null;
    if (pixel_format != pixel_format_xrgb32_rgb) return null;
    return .{
        .width = raw_width,
        .height = raw_height,
        .pixel_format = pixel_format,
        .pixel_offset = @intCast(pixel_offset),
        .payload_size = @intCast(raw_payload_size),
    };
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
