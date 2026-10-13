// Rust owns the intro and CPU fault probes; the display/font owner lends
// only a fixed snapshot without leaking native Zig structs or slices.
const boot_display = @import("../display/boot_display.zig");
const display = @import("../display/display.zig");
const font = @import("font.zig");
const IntroInfo = extern struct { width: u32, height: u32, bpp: u32, cols: u32, rows: u32, scale: u32, glyph_width: u32, glyph_height: u32 };
extern fn r4os_rust_boot_intro() callconv(.c) bool;
pub fn init() bool {
    return r4os_rust_boot_intro();
}
export fn r4os_zig_boot_console(output: *IntroInfo) callconv(.c) i32 {
    const state = boot_display.get() orelse return -1;
    const mode = display.activeMode() orelse return -2;
    output.* = .{ .width = mode.width, .height = mode.height, .bpp = mode.bpp, .cols = state.console_metrics.cols, .rows = state.console_metrics.rows, .scale = state.console_metrics.scale, .glyph_width = font.glyphWidth(), .glyph_height = font.glyphHeight() };
    return 0;
}
