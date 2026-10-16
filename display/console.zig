comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Text console on the framebuffer.
//
// Tracks cursor position, foreground/background color, and scrolling.

const fb = @import("../display/framebuffer.zig");
const font = @import("../kernel/font.zig");
const scroll_buffer = @import("console_scroll_buffer.zig");

// Ein einzelner frueher Framebuffer-ConsoleSink ist aktiv. Das begrenzte
// Zellraster liegt deshalb statisch im Kernel statt als grosse Stackstruktur.
// 65.536 Zellen decken den Default-2x-Textmodus bis einschliesslich 5K ab;
// groessere/ungewoehnliche Modi behalten den ausgerichteten FB-Fallback.

pub const Console = extern struct {
    framebuffer: *fb.Framebuffer,
    cols: u32,
    rows: u32,
    cur_x: u32 = 0,
    cur_y: u32 = 0,
    fg: u32 = 0xE0E0E0,
    bg: u32 = 0x001830,
    scale: u32 = 2,
    margin_left: u32 = 0,
    margin_top: u32 = 0,
    margin_right: u32 = 0,
    margin_bottom: u32 = 0,
    backing_cols: u32 = 0,
    backing_rows: u32 = 0,
    backing_valid: bool = false,
    pixel_backing: [*]u32 = @ptrFromInt(@alignOf(u32)),
    pixel_backing_len: usize = 0,
    has_pixel_backing: bool = false,
    pixel_backing_valid: bool = false,
    pixel_backing_width: u64 = 0,
    pixel_backing_height: u64 = 0,
    pub fn init(framebuffer: *fb.Framebuffer) Console {
        var out: Console = undefined;
        r4os_rust_console_init(framebuffer, &out);
        return out;
    }
    pub fn setFontScale(self: *Console, scale: u32) void {
        r4os_rust_console_set_font_scale(self, scale);
    }
    pub fn attachPixelBacking(self: *Console, pixels: []u32) bool {
        return r4os_rust_console_attach_pixel_backing(self, pixels.ptr, pixels.len);
    }
    pub fn setColors(self: *Console, fg: u32, bg: u32) void {
        r4os_rust_console_set_colors(self, fg, bg);
    }
    pub fn setMargins(self: *Console, left: u32, top: u32, right: u32, bottom: u32) void {
        r4os_rust_console_set_margins(self, left, top, right, bottom);
    }
    pub fn textCols(self: *const Console) u32 {
        return r4os_rust_console_text_cols(self);
    }
    pub fn textRows(self: *const Console) u32 {
        return r4os_rust_console_text_rows(self);
    }
    pub fn setCursor(self: *Console, x: u32, y: u32) void {
        r4os_rust_console_set_cursor(self, x, y);
    }
    pub fn clear(self: *Console) void {
        r4os_rust_console_clear(self);
    }
    pub fn clearFramed(self: *Console, border: u32, inner: u32) void {
        r4os_rust_console_clear_framed(self, border, inner);
    }
    pub fn invalidateBackingForExternalDisplay(self: *Console) void {
        r4os_rust_console_invalidate_backing_for_external_display(self);
    }
    pub fn putc(self: *Console, c: u8) void {
        r4os_rust_console_putc(self, c);
    }
    pub fn puts(self: *Console, s: []const u8) void {
        r4os_rust_console_puts(self, s.ptr, s.len);
    }
    pub fn putHex(self: *Console, value: u64, digits: u8) void {
        r4os_rust_console_put_hex(self, value, digits);
    }
    pub fn putDec(self: *Console, value: u64) void {
        r4os_rust_console_put_dec(self, value);
    }
    fn textBottom(self: *const Console) u32 {
        return self.rows - self.margin_bottom;
    }
};
extern fn r4os_rust_console_init(*fb.Framebuffer, *Console) void;
extern fn r4os_rust_console_set_font_scale(*Console, u32) void;
extern fn r4os_rust_console_attach_pixel_backing(*Console, [*]u32, usize) bool;
extern fn r4os_rust_console_set_colors(*Console, u32, u32) void;
extern fn r4os_rust_console_set_margins(*Console, u32, u32, u32, u32) void;
extern fn r4os_rust_console_text_cols(*const Console) u32;
extern fn r4os_rust_console_text_rows(*const Console) u32;
extern fn r4os_rust_console_set_cursor(*Console, u32, u32) void;
extern fn r4os_rust_console_clear(*Console) void;
extern fn r4os_rust_console_clear_framed(*Console, u32, u32) void;
extern fn r4os_rust_console_invalidate_backing_for_external_display(*Console) void;
extern fn r4os_rust_console_putc(*Console, u8) void;
extern fn r4os_rust_console_puts(*Console, [*]const u8, usize) void;
extern fn r4os_rust_console_put_hex(*Console, u64, u8) void;
extern fn r4os_rust_console_put_dec(*Console, u64) void;

test "console pixel scrolling matches cell redraw through geometry and display changes" {
    const testing = @import("std").testing;
    for ([_]usize{ 0, 1 }) |alignment_offset| {
        const reference = try consoleScrollSnapshots(false, alignment_offset);
        const accelerated = try consoleScrollSnapshots(true, alignment_offset);
        for (reference, accelerated) |expected, actual| try testing.expectEqualSlices(u8, &expected, &actual);
    }
}

fn consoleScrollSnapshots(use_shadow: bool, alignment_offset: usize) ![5][404 * 82]u8 {
    const std = @import("std");
    var memory: [404 * 82 + 1]u8 align(8) = .{0xA5} ** (404 * 82 + 1);
    var frame: fb.Framebuffer = .{
        .address = memory[alignment_offset..].ptr,
        .width = 98,
        .height = 82,
        .pitch = 404,
        .bpp = 32,
        .memory_model = 1,
        .red_mask_size = 8,
        .red_mask_shift = 0,
        .green_mask_size = 8,
        .green_mask_shift = 8,
        .blue_mask_size = 8,
        .blue_mask_shift = 16,
        .unused = .{0} ** 5,
        .edid_size = 0,
        .edid = null,
    };
    var shadow: [98 * 82]u32 = undefined;
    var console = Console.init(&frame);
    try std.testing.expect(!console.attachPixelBacking(shadow[0..1]));
    if (use_shadow) try std.testing.expect(console.attachPixelBacking(&shadow));
    var snapshots: [5][404 * 82]u8 = undefined;
    console.setMargins(1, 1, 1, 1);
    console.clearFramed(0x125678, 0x234567);
    for (0..9) |i| {
        console.setColors(0xABCDEF + @as(u32, @intCast(i)), 0x112200 + @as(u32, @intCast(i)));
        console.puts("Ab\tC\x08D\n");
    }
    @memcpy(&snapshots[0], memory[alignment_offset..][0..snapshots[0].len]);
    try std.testing.expectEqual(console.margin_left, console.cur_x);
    try std.testing.expectEqual(console.textBottom() - 1, console.cur_y);
    console.setMargins(0, 0, 0, 0);
    console.puts("a\nb\nc\nd\ne\nf\ng\n");
    @memcpy(&snapshots[1], memory[alignment_offset..][0..snapshots[1].len]);
    console.setFontScale(1);
    try std.testing.expect(!console.backing_valid);
    try std.testing.expectEqual(use_shadow, console.pixel_backing_valid);
    console.puts("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n");
    @memcpy(&snapshots[2], memory[alignment_offset..][0..snapshots[2].len]);
    console.clear();
    console.puts("A\nB\nC\nD\nE\nF\nG\nH\nI\nJ\nK\nL\n");
    @memcpy(&snapshots[3], memory[alignment_offset..][0..snapshots[3].len]);
    fb.rect(&frame, 1, 1, 35, 48, 0x56789A);
    console.invalidateBackingForExternalDisplay();
    try std.testing.expect(!console.pixel_backing_valid and !console.backing_valid);
    console.puts("external\nsecond\nthird\n");
    @memcpy(&snapshots[4], memory[alignment_offset..][0..snapshots[4].len]);
    return snapshots;
}

comptime {
    if (@sizeOf(Console) != 104 or @alignOf(Console) != 8 or @offsetOf(Console, "framebuffer") != 0 or @offsetOf(Console, "cols") != 8 or @offsetOf(Console, "rows") != 12 or @offsetOf(Console, "cur_x") != 16 or @offsetOf(Console, "cur_y") != 20 or @offsetOf(Console, "fg") != 24 or @offsetOf(Console, "bg") != 28 or @offsetOf(Console, "scale") != 32 or @offsetOf(Console, "margin_left") != 36 or @offsetOf(Console, "margin_top") != 40 or @offsetOf(Console, "margin_right") != 44 or @offsetOf(Console, "margin_bottom") != 48 or @offsetOf(Console, "backing_cols") != 52 or @offsetOf(Console, "backing_rows") != 56 or @offsetOf(Console, "backing_valid") != 60 or @offsetOf(Console, "pixel_backing") != 64 or @offsetOf(Console, "pixel_backing_len") != 72 or @offsetOf(Console, "has_pixel_backing") != 80 or @offsetOf(Console, "pixel_backing_valid") != 81 or @offsetOf(Console, "pixel_backing_width") != 88 or @offsetOf(Console, "pixel_backing_height") != 96) @compileError("Native display Console layout");
}
