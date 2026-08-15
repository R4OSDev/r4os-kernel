const r4x_api = @import("r4x_api.zig");
const surface_pipeline = @import("../display/surface_pipeline.zig");
const display = @import("../display/display.zig");
const font = @import("../kernel/font.zig");
const font_catalog = @import("../kernel/font_catalog.zig");
const mouse = @import("../driver/input/mouse.zig");

pub const name = "R4DRAW";
pub const gui_font_builtin_id = r4x_api.gui_font_builtin_id;
pub const gui_font_flag_renderable = r4x_api.gui_font_flag_renderable;
pub const gui_font_flag_selected = r4x_api.gui_font_flag_selected;
pub const gui_font_flag_builtin = r4x_api.gui_font_flag_builtin;

pub const MarkDisplayUsedFn = *const fn () void;
pub const FontCatalogChangedFn = *const fn () void;

pub const GuiFontInfo = r4x_api.GuiFontInfo;

pub const GuiTextMetrics = r4x_api.GuiTextMetrics;

var display_used_hook: ?MarkDisplayUsedFn = null;
var font_catalog_changed_hook: ?FontCatalogChangedFn = null;
var display_revision: u32 = 0;

pub fn setDisplayUsedHook(hook: MarkDisplayUsedFn) void {
    display_used_hook = hook;
}

pub fn setFontCatalogChangedHook(hook: FontCatalogChangedFn) void {
    font_catalog_changed_hook = hook;
}

pub fn markDisplayUsed() void {
    display_revision +%= 1;
    if (display_revision == 0) display_revision = 1;
    if (display_used_hook) |hook| hook();
}

pub fn screenWidth() callconv(.c) u32 {
    return surface_pipeline.width();
}

pub fn screenHeight() callconv(.c) u32 {
    return surface_pipeline.height();
}

pub fn clear(rgb: u32) callconv(.c) void {
    _ = rgb;
}

pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) callconv(.c) void {
    _ = x;
    _ = y;
    _ = w;
    _ = h;
    _ = rgb;
}

pub fn text(x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) callconv(.c) void {
    _ = x;
    _ = y;
    _ = value;
    _ = fg;
    _ = bg;
}

pub fn displayRevision() callconv(.c) u32 {
    return display_revision;
}

pub fn displayBeginFrame() callconv(.c) i32 {
    markDisplayUsed();
    mouse.disableCursor();
    return surface_pipeline.beginFrame();
}

pub fn displayBeginFrameRect(x: i32, y: i32, w: u32, h: u32) callconv(.c) i32 {
    markDisplayUsed();
    mouse.disableCursor();
    return surface_pipeline.beginFrameRect(x, y, w, h);
}

pub fn displayPresent() callconv(.c) i32 {
    markDisplayUsed();
    return surface_pipeline.present();
}

pub fn displayBlitXrgb32(x: i32, y: i32, w: u32, h: u32, pixels: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    if (x < 0 or y < 0 or w == 0 or h == 0) return -1;
    if (@intFromPtr(pixels) == 0) return -1;
    const needed = @as(u64, w) * @as(u64, h);
    if (needed > pixel_count) return -2;
    const bytes_needed = needed * 4;
    if (bytes_needed > @as(u64, ~@as(usize, 0))) return -2;

    const raw: [*]const u8 = @ptrCast(pixels);
    const ok = display.presentXrgb32Rect(
        @intCast(x),
        @intCast(y),
        w,
        h,
        raw[0..@intCast(bytes_needed)],
        w,
    );
    if (!ok) return -3;
    markDisplayUsed();
    return 0;
}

pub fn fontCount() callconv(.c) u32 {
    return @intCast(font.fontCount());
}

pub fn fontInfo(font_id: u32, out: *GuiFontInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return -1;
    return fillGuiFontInfo(font_id, false, out);
}

pub fn fontMeasure(font_id: u32, value: [*:0]const u8, out: *GuiTextMetrics) callconv(.c) i32 {
    if (@intFromPtr(value) == 0 or @intFromPtr(out) == 0) return -1;
    if (!font.isRenderableFontId(font_id)) return -2;
    const metrics = font.measureZWithFont(font_id, value, 4096);
    out.* = guiMetrics(metrics);
    return 0;
}

/// Returns one monochrome glyph row from the bounded runtime R4F cache. The
/// bits are packed least-significant-bit first by pixel column. It deliberately
/// exposes rendered cache pixels only; installed font files stay on C:.
pub fn fontGlyphRow(font_id: u32, codepoint: u32, row: u32) callconv(.c) u64 {
    if (codepoint > 0x10FFFF or row >= font.MAX_GLYPH_H or !font.isRenderableFontId(font_id)) return 0;
    const glyph_width = @min(font.glyphBitmapWidthForFont(font_id, codepoint), @as(u32, 64));
    var result: u64 = 0;
    var column: u32 = 0;
    while (column < glyph_width) : (column += 1) {
        if (font.glyphPixelForFont(font_id, codepoint, row, column)) {
            result |= @as(u64, 1) << @intCast(column);
        }
    }
    return result;
}

/// Rescans C:\R4OS\FONTS.  R4F files are read only long enough to populate
/// the bounded runtime glyph cache; their persistent owner remains the file
/// system.  A non-negative result is the number of renderable faces.
pub fn fontReload() callconv(.c) i32 {
    const result = font_catalog.reloadInstalled();
    if (result.unavailable) return -1;
    if (font_catalog_changed_hook) |hook| hook();
    return @intCast(result.registered);
}

pub fn textFont(font_id: u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) callconv(.c) void {
    _ = font_id;
    _ = x;
    _ = y;
    _ = value;
    _ = fg;
    _ = bg;
}

fn fillGuiFontInfo(font_id: u32, force_selected: bool, out: *GuiFontInfo) i32 {
    out.* = .{};
    if (font_id == gui_font_builtin_id) {
        out.* = .{
            .id = gui_font_builtin_id,
            .kind = 0,
            .flags = gui_font_flag_renderable | gui_font_flag_builtin | (if (force_selected or font.currentFontId() == gui_font_builtin_id) gui_font_flag_selected else 0),
            .weight = 400,
            .style_flags = 0,
            .charset_flags = 0,
            .width = 8,
            .height = 8,
            .max_advance = 8,
            .line_height = 8,
            .baseline = 7,
            .glyph_count = 95,
            .strike_count = 1,
        };
        copyFixedZ(out.family[0..], "R4OS");
        copyFixedZ(out.face[0..], "Builtin 8x8");
        copyFixedZ(out.style[0..], "Regular");
        copyFixedZ(out.status[0..], "builtin fallback");
        return 1;
    }
    const entry = font.catalogEntryForFontId(font_id) orelse return 0;
    out.* = .{
        .id = font_id,
        .kind = @intFromEnum(entry.kind),
        .flags = (if (entry.renderable) gui_font_flag_renderable else 0) | (if (force_selected or entry.selected) gui_font_flag_selected else 0),
        .weight = entry.weight,
        .style_flags = entry.style_flags,
        .charset_flags = entry.charset_flags,
        .width = entry.width,
        .height = entry.height,
        .max_advance = entry.max_advance,
        .line_height = entry.line_height,
        .baseline = entry.baseline,
        .glyph_count = entry.glyph_count,
        .strike_count = entry.strike_count,
    };
    copyFixedZ(out.path[0..], entry.path[0..entry.path_len]);
    copyFixedZ(out.family[0..], entry.family[0..entry.family_len]);
    copyFixedZ(out.face[0..], entry.face[0..entry.face_len]);
    copyFixedZ(out.style[0..], entry.style[0..entry.style_len]);
    copyFixedZ(out.status[0..], entry.status[0..entry.status_len]);
    return 1;
}

fn guiMetrics(metrics: font.TextMetrics) GuiTextMetrics {
    return .{
        .width = metrics.width,
        .height = metrics.height,
        .line_height = metrics.line_height,
        .baseline = metrics.baseline,
        .visible_bytes = @intCast(@min(metrics.visible_bytes, @as(usize, ~@as(u32, 0)))),
        .flags = if (metrics.clipped) 1 else 0,
    };
}

fn copyFixedZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
}
