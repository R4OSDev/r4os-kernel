//! Temporary fixed projection to the native R4DRAW facade.
const r4x_api = @import("r4x_api.zig");
pub const name = "R4DRAW";
pub const gui_font_builtin_id = r4x_api.gui_font_builtin_id;
pub const gui_font_flag_renderable = r4x_api.gui_font_flag_renderable;
pub const gui_font_flag_selected = r4x_api.gui_font_flag_selected;
pub const gui_font_flag_builtin = r4x_api.gui_font_flag_builtin;
pub const MarkDisplayUsedFn = *const fn () void;
pub const FontCatalogChangedFn = *const fn () void;
pub const GuiFontInfo = r4x_api.GuiFontInfo;
pub const GuiGlyphBitmap = r4x_api.GuiGlyphBitmap;
pub const GuiTextMetrics = r4x_api.GuiTextMetrics;
pub const DisplayDamageRect = r4x_api.DisplayDamageRect;
pub const DisplayPresentCapabilities = r4x_api.DisplayPresentCapabilities;
pub const DisplayPresentCompletion = r4x_api.DisplayPresentCompletion;
pub const DisplayPresentRequest = r4x_api.DisplayPresentRequest;
pub const DisplayPresentResult = r4x_api.DisplayPresentResult;

const Args = extern struct { a: u64 = 0, b: u64 = 0, x: i32 = 0, y: i32 = 0, w: u32 = 0, h: u32 = 0, color: u32 = 0, bg: u32 = 0, input: ?*const anyopaque = null, second: ?*const anyopaque = null, third: ?*const anyopaque = null };
extern fn r4os_rust_r4draw(u32, *const Args, ?*anyopaque) u64;
fn signed(op: u32, args: Args, out: ?*anyopaque) i32 {
    return @bitCast(@as(u32, @truncate(r4os_rust_r4draw(op, &args, out))));
}
pub fn setDisplayUsedHook(hook: MarkDisplayUsedFn) void {
    _ = r4os_rust_r4draw(0, &Args{ .a = @intFromPtr(hook) }, null);
}
pub fn setFontCatalogChangedHook(hook: FontCatalogChangedFn) void {
    _ = r4os_rust_r4draw(1, &Args{ .a = @intFromPtr(hook) }, null);
}
pub fn markDisplayUsed() void {
    _ = r4os_rust_r4draw(2, &Args{}, null);
}
pub fn screenWidth() callconv(.c) u32 {
    return @intCast(r4os_rust_r4draw(3, &Args{}, null));
}
pub fn screenHeight() callconv(.c) u32 {
    return @intCast(r4os_rust_r4draw(4, &Args{}, null));
}
pub fn clear(rgb: u32) callconv(.c) void {
    _ = r4os_rust_r4draw(5, &Args{ .color = rgb }, null);
}
pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) callconv(.c) void {
    _ = r4os_rust_r4draw(6, &Args{ .x = x, .y = y, .w = w, .h = h, .color = rgb }, null);
}
pub fn text(x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) callconv(.c) void {
    _ = r4os_rust_r4draw(7, &Args{ .x = x, .y = y, .input = value, .color = fg, .bg = bg }, null);
}
pub fn displayRevision() callconv(.c) u32 {
    return @intCast(r4os_rust_r4draw(8, &Args{}, null));
}
pub fn displayBeginFrame() callconv(.c) i32 {
    return signed(9, .{}, null);
}
pub fn displayBeginFrameRect(x: i32, y: i32, w: u32, h: u32) callconv(.c) i32 {
    return signed(10, .{ .x = x, .y = y, .w = w, .h = h }, null);
}
pub fn displayPresent() callconv(.c) i32 {
    return signed(11, .{}, null);
}
pub fn displayBlitXrgb32(x: i32, y: i32, w: u32, h: u32, pixels: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    return signed(12, .{ .x = x, .y = y, .w = w, .h = h, .input = pixels, .b = pixel_count, .a = w }, null);
}
pub fn displayBlitXrgb32Stride(x: i32, y: i32, w: u32, h: u32, pixels: [*]const u32, pixel_count: u32, source_stride_pixels: u32) callconv(.c) i32 {
    return signed(12, .{ .x = x, .y = y, .w = w, .h = h, .input = pixels, .b = pixel_count, .a = source_stride_pixels }, null);
}
pub fn displayPresentRegions(
    request: *const DisplayPresentRequest,
    pixels: [*]const u32,
    pixel_count: u32,
    regions: [*]const DisplayDamageRect,
    region_count: u32,
    out: *DisplayPresentResult,
) callconv(.c) i32 {
    return signed(13, .{ .input = request, .second = pixels, .third = regions, .b = pixel_count, .a = region_count }, out);
}
pub fn displayPresentCapabilities(out: *DisplayPresentCapabilities) callconv(.c) i32 {
    return signed(14, .{}, out);
}
pub fn displayPresentCompletion(fence: u64, out: *DisplayPresentCompletion) callconv(.c) i32 {
    return signed(15, .{ .a = fence }, out);
}
pub fn displayOutputTarget(adapter_id: u32, head_id: u32, out: *@import("r4os_kernel_contract").GfxOutputTarget) callconv(.c) i32 {
    return signed(16, .{ .w = adapter_id, .h = head_id }, out);
}
pub fn displayOutputPresentationInfo(input: *const @import("r4os_kernel_contract").GfxOutputTarget, out: *@import("r4os_kernel_contract").DisplayPresentationInfo) callconv(.c) i32 {
    return signed(17, .{ .input = input }, out);
}
pub fn displayOutputPresentationFeedback(input: *const @import("r4os_kernel_contract").GfxOutputTarget, source: *const @import("r4os_kernel_contract").GfxFence, out: *@import("r4os_kernel_contract").DisplayPresentationStats) callconv(.c) i32 {
    return signed(18, .{ .input = input, .second = source }, out);
}
pub fn displayPresentationStats(head_id: u32, out: *@import("r4os_kernel_contract").DisplayPresentationStats) callconv(.c) i32 {
    return signed(19, .{ .w = head_id }, out);
}
pub fn displayPresentationInfo(head_id: u32, out: *@import("r4os_kernel_contract").DisplayPresentationInfo) callconv(.c) i32 {
    return signed(20, .{ .w = head_id }, out);
}
pub fn displayPresentationFeedback(head_id: u32, source: *const @import("r4os_kernel_contract").GfxFence, out: *@import("r4os_kernel_contract").DisplayPresentationStats) callconv(.c) i32 {
    return signed(21, .{ .w = head_id, .input = source }, out);
}
pub fn fontCount() callconv(.c) u32 {
    return @intCast(r4os_rust_r4draw(22, &Args{}, null));
}
pub fn fontInfo(font_id: u32, out: *GuiFontInfo) callconv(.c) i32 {
    return signed(23, .{ .w = font_id }, out);
}
pub fn fontMeasure(font_id: u32, value: [*:0]const u8, out: *GuiTextMetrics) callconv(.c) i32 {
    return signed(24, .{ .w = font_id, .input = value }, out);
}
pub fn fontGlyphRow(font_id: u32, codepoint: u32, row: u32) callconv(.c) u64 {
    return r4os_rust_r4draw(25, &Args{ .w = font_id, .h = codepoint, .a = row }, null);
}
pub fn fontGlyphBitmap(font_id: u32, codepoint: u32, out: *GuiGlyphBitmap) callconv(.c) i32 {
    return signed(26, .{ .w = font_id, .h = codepoint }, out);
}
pub fn fontReload() callconv(.c) i32 {
    return signed(27, .{}, null);
}
pub fn fontRevision() callconv(.c) u32 {
    return @intCast(r4os_rust_r4draw(28, &Args{}, null));
}
pub fn textFont(font_id: u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) callconv(.c) void {
    _ = r4os_rust_r4draw(29, &Args{ .w = font_id, .x = x, .y = y, .input = value, .color = fg, .bg = bg }, null);
}
export fn r4os_zig_r4draw_hook(code: usize) callconv(.c) void {
    const hook: *const fn () void = @ptrFromInt(code);
    hook();
}
comptime {
    if (@sizeOf(Args) != 64 or @offsetOf(Args, "input") != 40) @compileError("R4DRAW C args layout");
}
