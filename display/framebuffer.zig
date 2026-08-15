// Framebuffer drawing operations.
//
// Public color input is 0xRRGGBB. Output is packed through the boot framebuffer
// masks into the actual 32-bpp pixel format.

pub const Framebuffer = extern struct {
    address: [*]volatile u8,
    width: u64,
    height: u64,
    pitch: u64,
    bpp: u16,
    memory_model: u8,
    red_mask_size: u8,
    red_mask_shift: u8,
    green_mask_size: u8,
    green_mask_shift: u8,
    blue_mask_size: u8,
    blue_mask_shift: u8,
    unused: [5]u8,
    edid_size: u64,
    edid: ?*const anyopaque,
};

pub fn supportsRgb32(fb: *const Framebuffer) bool {
    return fb.bpp == 32 and fb.width <= @divFloor(~@as(u64, 0), 4) and fb.pitch >= fb.width * 4;
}

pub fn isNativeXrgb32(fb: *const Framebuffer) bool {
    if (!supportsRgb32(fb)) return false;
    if (fb.red_mask_size == 0 and fb.green_mask_size == 0 and fb.blue_mask_size == 0) return true;
    return fb.red_mask_size == 8 and fb.red_mask_shift == 16 and
        fb.green_mask_size == 8 and fb.green_mask_shift == 8 and
        fb.blue_mask_size == 8 and fb.blue_mask_shift == 0;
}

pub fn pixel(fb: *Framebuffer, x: u64, y: u64, rgb: u32) void {
    if (!supportsRgb32(fb) or x >= fb.width or y >= fb.height) return;
    writePixel32(fb.address + y * fb.pitch + x * 4, packRgb(fb, rgb));
}

pub fn readPacked32(fb: *Framebuffer, x: u64, y: u64) u32 {
    if (!supportsRgb32(fb) or x >= fb.width or y >= fb.height) return 0;
    return readPixel32(fb.address + y * fb.pitch + x * 4);
}

pub fn rect(fb: *Framebuffer, x0: u64, y0: u64, w: u64, h: u64, rgb: u32) void {
    if (!supportsRgb32(fb) or x0 >= fb.width or y0 >= fb.height) return;

    const x1 = x0 + @min(w, fb.width - x0);
    const y1 = y0 + @min(h, fb.height - y0);
    const color32 = packRgb(fb, rgb);

    var y = y0;
    while (y < y1) : (y += 1) {
        const row = fb.address + y * fb.pitch;
        var x = x0;
        while (x < x1) : (x += 1) {
            writePixel32(row + x * 4, color32);
        }
    }
}

pub fn fill(fb: *Framebuffer, rgb: u32) void {
    rect(fb, 0, 0, fb.width, fb.height, rgb);
}

pub fn putPacked32(fb: *Framebuffer, x: u64, y: u64, color32: u32) void {
    if (!supportsRgb32(fb) or x >= fb.width or y >= fb.height) return;
    writePixel32(fb.address + y * fb.pitch + x * 4, color32);
}

// Verschiebt eine 32-bpp-Framebuffer-Region nach oben. Der Vorwaertslauf ist
// wegen src_y > dst_y ueberlappungssicher. Auf normal ausgerichteten linearen
// Framebuffern werden 64-/32-Bit-Transfers statt einzelner volatiler Bytes
// verwendet; der Bytepfad bleibt nur fuer ungewoehnlich ausgerichtete Modi.
pub fn movePacked32RectUp(
    fb: *Framebuffer,
    x0: u64,
    y0: u64,
    w: u64,
    h: u64,
    rows_up: u64,
) bool {
    if (!supportsRgb32(fb) or w == 0 or h == 0 or rows_up == 0) return false;
    if (x0 >= fb.width or y0 >= fb.height) return false;

    const clipped_w = @min(w, fb.width - x0);
    const clipped_h = @min(h, fb.height - y0);
    if (rows_up >= clipped_h) return false;

    const move_h = clipped_h - rows_up;
    const row_bytes = clipped_w * 4;
    var y: u64 = 0;
    while (y < move_h) : (y += 1) {
        const dst = fb.address + (y0 + y) * fb.pitch + x0 * 4;
        const src = fb.address + (y0 + rows_up + y) * fb.pitch + x0 * 4;
        copyVolatileForward(dst, src, row_bytes);
    }
    return true;
}

pub fn packRgb(fb: *const Framebuffer, rgb: u32) u32 {
    const r: u8 = @truncate(rgb >> 16);
    const g: u8 = @truncate(rgb >> 8);
    const b: u8 = @truncate(rgb);

    if (fb.red_mask_size == 0 and fb.green_mask_size == 0 and fb.blue_mask_size == 0) {
        return rgb;
    }

    return packChannel(r, fb.red_mask_size, fb.red_mask_shift) |
        packChannel(g, fb.green_mask_size, fb.green_mask_shift) |
        packChannel(b, fb.blue_mask_size, fb.blue_mask_shift);
}

pub fn unpackRgb(fb: *const Framebuffer, packed_pixel: u32) u32 {
    if (fb.red_mask_size == 0 and fb.green_mask_size == 0 and fb.blue_mask_size == 0) {
        return packed_pixel & 0x00FF_FFFF;
    }
    const r: u32 = unpackChannel(packed_pixel, fb.red_mask_size, fb.red_mask_shift);
    const g: u32 = unpackChannel(packed_pixel, fb.green_mask_size, fb.green_mask_shift);
    const b: u32 = unpackChannel(packed_pixel, fb.blue_mask_size, fb.blue_mask_shift);
    return (r << 16) | (g << 8) | b;
}

fn packChannel(value: u8, size: u8, shift: u8) u32 {
    if (size == 0) return 0;
    const scaled: u32 = if (size >= 8)
        @as(u32, value) << @intCast(size - 8)
    else
        @as(u32, value) >> @intCast(8 - size);
    return scaled << @intCast(shift);
}

fn unpackChannel(value: u32, size: u8, shift: u8) u8 {
    if (size == 0) return 0;
    const max = (@as(u32, 1) << @intCast(size)) - 1;
    const raw = (value >> @intCast(shift)) & max;
    return @intCast((raw * 255 + max / 2) / max);
}

fn writePixel32(ptr: [*]volatile u8, value: u32) void {
    if ((@intFromPtr(ptr) & 3) == 0) {
        const word: *volatile u32 = @ptrCast(@alignCast(ptr));
        word.* = value;
        return;
    }
    ptr[0] = @truncate(value);
    ptr[1] = @truncate(value >> 8);
    ptr[2] = @truncate(value >> 16);
    ptr[3] = @truncate(value >> 24);
}

fn readPixel32(ptr: [*]volatile u8) u32 {
    if ((@intFromPtr(ptr) & 3) == 0) {
        const word: *volatile u32 = @ptrCast(@alignCast(ptr));
        return word.*;
    }
    return @as(u32, ptr[0]) |
        (@as(u32, ptr[1]) << 8) |
        (@as(u32, ptr[2]) << 16) |
        (@as(u32, ptr[3]) << 24);
}

fn copyVolatileForward(dst: [*]volatile u8, src: [*]volatile u8, byte_len: u64) void {
    var offset: u64 = 0;

    if (((@intFromPtr(dst) | @intFromPtr(src)) & 7) == 0) {
        const dst_words: [*]volatile u64 = @ptrCast(@alignCast(dst));
        const src_words: [*]volatile u64 = @ptrCast(@alignCast(src));
        const count = byte_len / 8;
        var i: u64 = 0;
        while (i < count) : (i += 1) dst_words[i] = src_words[i];
        offset = count * 8;
    }

    if (((@intFromPtr(dst + offset) | @intFromPtr(src + offset)) & 3) == 0) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst + offset));
        const src_words: [*]volatile u32 = @ptrCast(@alignCast(src + offset));
        const count = (byte_len - offset) / 4;
        var i: u64 = 0;
        while (i < count) : (i += 1) dst_words[i] = src_words[i];
        offset += count * 4;
    }

    while (offset < byte_len) : (offset += 1) dst[offset] = src[offset];
}

test "aligned packed pixels use exact 32-bit values" {
    const testing = @import("std").testing;
    var storage: [32]u8 align(8) = .{0} ** 32;
    var frame = testFramebuffer(storage[0..].ptr, 4, 2, 16);
    putPacked32(&frame, 2, 1, 0xA1B2C3D4);
    try testing.expectEqual(@as(u32, 0xA1B2C3D4), readPacked32(&frame, 2, 1));
}

test "packed rect move is overlap-safe and keeps outside pixels" {
    const testing = @import("std").testing;
    var storage: [96]u8 align(8) = .{0} ** 96;
    var frame = testFramebuffer(storage[0..].ptr, 6, 4, 24);
    var before: [24]u32 = undefined;

    var y: u64 = 0;
    while (y < frame.height) : (y += 1) {
        var x: u64 = 0;
        while (x < frame.width) : (x += 1) {
            const value: u32 = @intCast(100 + y * 10 + x);
            putPacked32(&frame, x, y, value);
            before[@as(usize, @intCast(y * frame.width + x))] = value;
        }
    }

    try testing.expect(movePacked32RectUp(&frame, 1, 0, 4, 4, 1));
    y = 0;
    while (y < 3) : (y += 1) {
        var x: u64 = 1;
        while (x < 5) : (x += 1) {
            try testing.expectEqual(
                before[@as(usize, @intCast((y + 1) * frame.width + x))],
                readPacked32(&frame, x, y),
            );
        }
    }
    // Die Funktion bewegt nur die Zielregion; Rand und freigewordene Zeile
    // werden vom Console-Caller bewusst separat behandelt.
    try testing.expectEqual(before[0], readPacked32(&frame, 0, 0));
    try testing.expectEqual(before[18 + 2], readPacked32(&frame, 2, 3));
}

test "unaligned packed pixel fallback remains byte-correct" {
    const testing = @import("std").testing;
    var storage: [18]u8 = .{0} ** 18;
    var frame = testFramebuffer(storage[1..].ptr, 2, 2, 8);
    putPacked32(&frame, 1, 1, 0x10203040);
    try testing.expectEqual(@as(u32, 0x10203040), readPacked32(&frame, 1, 1));
}

test "unaligned padded rect move preserves pixels and padding" {
    const testing = @import("std").testing;
    var storage: [53]u8 = .{0xCC} ** 53;
    var frame = testFramebuffer(storage[1..].ptr, 3, 4, 13);
    var before: [12]u32 = undefined;

    var y: u64 = 0;
    while (y < frame.height) : (y += 1) {
        var x: u64 = 0;
        while (x < frame.width) : (x += 1) {
            const value: u32 = @intCast(0x1000 + y * 16 + x);
            putPacked32(&frame, x, y, value);
            before[@as(usize, @intCast(y * frame.width + x))] = value;
        }
    }

    try testing.expect(movePacked32RectUp(&frame, 0, 0, 3, 4, 1));
    y = 0;
    while (y < 3) : (y += 1) {
        var x: u64 = 0;
        while (x < 3) : (x += 1) {
            try testing.expectEqual(
                before[@as(usize, @intCast((y + 1) * frame.width + x))],
                readPacked32(&frame, x, y),
            );
        }
    }
    // One padding byte follows each 12-byte pixel row and must never move.
    try testing.expectEqual(@as(u8, 0xCC), storage[13]);
    try testing.expectEqual(@as(u8, 0xCC), storage[26]);
    try testing.expectEqual(@as(u8, 0xCC), storage[39]);
    try testing.expectEqual(@as(u8, 0xCC), storage[52]);
}

test "invalid and single-row moves leave the framebuffer untouched" {
    const testing = @import("std").testing;
    var storage: [32]u8 align(8) = .{0x5A} ** 32;
    var frame = testFramebuffer(storage[0..].ptr, 4, 2, 16);
    const before = storage;

    try testing.expect(!movePacked32RectUp(&frame, 0, 0, 4, 1, 1));
    try testing.expect(!movePacked32RectUp(&frame, 0, 0, 4, 2, 2));
    try testing.expect(!movePacked32RectUp(&frame, 0, 0, 0, 2, 1));
    try testing.expectEqualSlices(u8, before[0..], storage[0..]);
}

fn testFramebuffer(address: [*]u8, width: u64, height: u64, pitch: u64) Framebuffer {
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
