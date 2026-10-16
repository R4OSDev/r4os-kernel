comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
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
    return r4os_rust_fb_supports(fb);
}

pub fn isNativeXrgb32(fb: *const Framebuffer) bool {
    return r4os_rust_fb_native(fb);
}

pub fn pixel(fb: *Framebuffer, x: u64, y: u64, rgb: u32) void {
    r4os_rust_fb_pixel(fb, x, y, rgb);
}

pub fn readPacked32(fb: *Framebuffer, x: u64, y: u64) u32 {
    return r4os_rust_fb_read(fb, x, y);
}

pub fn rect(fb: *Framebuffer, x0: u64, y0: u64, w: u64, h: u64, rgb: u32) void {
    r4os_rust_fb_rect(fb, x0, y0, w, h, rgb);
}

pub fn fill(fb: *Framebuffer, rgb: u32) void {
    r4os_rust_fb_fill(fb, rgb);
}

pub fn putPacked32(fb: *Framebuffer, x: u64, y: u64, color32: u32) void {
    r4os_rust_fb_put(fb, x, y, color32);
}

// Present already packed RAM pixels without reading the device framebuffer.
// Source rows may be wider than the copied rectangle; device pitch and unusual
// destination alignment remain properties of the framebuffer.
pub fn blitPacked32(fb: *Framebuffer, x: u64, y: u64, w: u64, h: u64, source: []const u32, stride: usize) bool {
    return r4os_rust_fb_blit(fb, x, y, w, h, source.ptr, source.len, stride);
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
    return r4os_rust_fb_move(fb, x0, y0, w, h, rows_up);
}

pub fn packRgb(fb: *const Framebuffer, rgb: u32) u32 {
    return r4os_rust_fb_pack(fb, rgb);
}

pub fn unpackRgb(fb: *const Framebuffer, packed_pixel: u32) u32 {
    return r4os_rust_fb_unpack(fb, packed_pixel);
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

comptime {
    if (@sizeOf(Framebuffer) != 64 or @alignOf(Framebuffer) != 8 or @offsetOf(Framebuffer, "address") != 0 or @offsetOf(Framebuffer, "width") != 8 or @offsetOf(Framebuffer, "height") != 16 or @offsetOf(Framebuffer, "pitch") != 24 or @offsetOf(Framebuffer, "bpp") != 32 or @offsetOf(Framebuffer, "memory_model") != 34 or @offsetOf(Framebuffer, "red_mask_size") != 35 or @offsetOf(Framebuffer, "red_mask_shift") != 36 or @offsetOf(Framebuffer, "green_mask_size") != 37 or @offsetOf(Framebuffer, "green_mask_shift") != 38 or @offsetOf(Framebuffer, "blue_mask_size") != 39 or @offsetOf(Framebuffer, "blue_mask_shift") != 40 or @offsetOf(Framebuffer, "unused") != 41 or @offsetOf(Framebuffer, "edid_size") != 48 or @offsetOf(Framebuffer, "edid") != 56) @compileError("Native display Framebuffer layout");
}

extern fn r4os_rust_fb_supports(*const Framebuffer) bool;
extern fn r4os_rust_fb_native(*const Framebuffer) bool;
extern fn r4os_rust_fb_pixel(*const Framebuffer, u64, u64, u32) void;
extern fn r4os_rust_fb_read(*const Framebuffer, u64, u64) u32;
extern fn r4os_rust_fb_rect(*const Framebuffer, u64, u64, u64, u64, u32) void;
extern fn r4os_rust_fb_fill(*const Framebuffer, u32) void;
extern fn r4os_rust_fb_put(*const Framebuffer, u64, u64, u32) void;
extern fn r4os_rust_fb_blit(*const Framebuffer, u64, u64, u64, u64, [*]const u32, usize, usize) bool;
extern fn r4os_rust_fb_move(*const Framebuffer, u64, u64, u64, u64, u64) bool;
extern fn r4os_rust_fb_pack(*const Framebuffer, u32) u32;
extern fn r4os_rust_fb_unpack(*const Framebuffer, u32) u32;
