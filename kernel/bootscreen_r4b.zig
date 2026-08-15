const std = @import("std");
const fb = @import("../display/framebuffer.zig");
const format = @import("bootscreen_r4b_format.zig");

pub fn valid(bytes: []const u8) bool {
    return format.valid(bytes);
}

pub fn draw(framebuf: *fb.Framebuffer, bytes: []const u8) bool {
    if (!fb.supportsRgb32(framebuf)) return false;
    const header = format.read(bytes) orelse return false;
    return drawHeader(framebuf, bytes, header);
}

const Origin = struct {
    x: u64,
    y: u64,
};

fn centeredOrigin(frame_width: u64, frame_height: u64, image_width: u32, image_height: u32) ?Origin {
    if (frame_width < image_width or frame_height < image_height) return null;
    return .{
        .x = (frame_width - image_width) / 2,
        .y = (frame_height - image_height) / 2,
    };
}

fn drawHeader(framebuf: *fb.Framebuffer, bytes: []const u8, header: format.Header) bool {
    const origin = centeredOrigin(framebuf.width, framebuf.height, header.width, header.height) orelse return false;
    const pixel_count = std.math.mul(usize, header.width, header.height) catch return false;
    const pixel_bytes = std.math.mul(usize, pixel_count, @sizeOf(u32)) catch return false;
    if (header.payload_size != pixel_bytes or header.pixel_offset > bytes.len or pixel_bytes > bytes.len - header.pixel_offset) return false;

    var y: u32 = 0;
    while (y < header.height) : (y += 1) {
        var x: u32 = 0;
        while (x < header.width) : (x += 1) {
            const offset = header.pixel_offset + (@as(usize, y) * @as(usize, header.width) + @as(usize, x)) * @sizeOf(u32);
            const rgb = format.readLe32(bytes[offset .. offset + 4]) & 0x00FF_FFFF;
            fb.putPacked32(framebuf, origin.x + x, origin.y + y, fb.packRgb(framebuf, rgb));
        }
    }
    return true;
}

test "720p bootscreen is unchanged and larger framebuffers center it" {
    const testing = std.testing;
    const exact = centeredOrigin(1280, 720, 1280, 720) orelse return error.MissingExactOrigin;
    try testing.expectEqual(@as(u64, 0), exact.x);
    try testing.expectEqual(@as(u64, 0), exact.y);

    const full_hd = centeredOrigin(1920, 1080, 1280, 720) orelse return error.MissingFullHdOrigin;
    try testing.expectEqual(@as(u64, 320), full_hd.x);
    try testing.expectEqual(@as(u64, 180), full_hd.y);

    try testing.expect(centeredOrigin(1279, 720, 1280, 720) == null);
    try testing.expect(centeredOrigin(1280, 719, 1280, 720) == null);
}

test "centered draw honors pitch and surrounding pixels" {
    const testing = std.testing;
    const header_size = 32;
    var image: [header_size + 16]u8 = .{0} ** (header_size + 16);
    writeLe32(image[header_size..][0..4], 0x0011_2233);
    writeLe32(image[header_size..][4..8], 0x0044_5566);
    writeLe32(image[header_size..][8..12], 0x0077_8899);
    writeLe32(image[header_size..][12..16], 0x00AA_BBCC);

    const pitch = 32;
    var storage: [8 + pitch * 4 + 8]u8 align(8) = .{0xCC} ** (8 + pitch * 4 + 8);
    var frame = testFramebuffer(storage[8..].ptr, 6, 4, pitch);
    const header: format.Header = .{
        .width = 2,
        .height = 2,
        .pixel_format = format.pixel_format_xrgb32_rgb,
        .pixel_offset = header_size,
        .payload_size = 16,
    };

    try testing.expect(drawHeader(&frame, image[0..], header));
    try testing.expectEqual(@as(u32, 0x0011_2233), fb.readPacked32(&frame, 2, 1));
    try testing.expectEqual(@as(u32, 0x0044_5566), fb.readPacked32(&frame, 3, 1));
    try testing.expectEqual(@as(u32, 0x0077_8899), fb.readPacked32(&frame, 2, 2));
    try testing.expectEqual(@as(u32, 0x00AA_BBCC), fb.readPacked32(&frame, 3, 2));
    try testing.expectEqual(@as(u32, 0xCCCC_CCCC), fb.readPacked32(&frame, 1, 1));
    try testing.expectEqualSlices(u8, &(.{0xCC} ** 8), storage[0..8]);
    try testing.expectEqualSlices(u8, &(.{0xCC} ** 8), storage[storage.len - 8 ..]);
    try testing.expectEqualSlices(u8, &(.{0xCC} ** 8), storage[8 + 24 .. 8 + pitch]);
}

fn testFramebuffer(address: [*]u8, width: u64, height: u64, pitch: u64) fb.Framebuffer {
    return .{
        .address = address,
        .width = width,
        .height = height,
        .pitch = pitch,
        .bpp = 32,
        .memory_model = 1,
        .red_mask_size = 8,
        .red_mask_shift = 16,
        .green_mask_size = 8,
        .green_mask_shift = 8,
        .blue_mask_size = 8,
        .blue_mask_shift = 0,
        .unused = .{0} ** 5,
        .edid_size = 0,
        .edid = null,
    };
}

fn writeLe32(out: []u8, value: u32) void {
    out[0] = @truncate(value);
    out[1] = @truncate(value >> 8);
    out[2] = @truncate(value >> 16);
    out[3] = @truncate(value >> 24);
}
