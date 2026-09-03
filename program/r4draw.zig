const r4x_api = @import("r4x_api.zig");
const surface_pipeline = @import("../display/surface_pipeline.zig");
const presenter = @import("../display/presenter.zig");
const surface = @import("../display/surface.zig");
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
pub const GuiGlyphBitmap = r4x_api.GuiGlyphBitmap;

pub const GuiTextMetrics = r4x_api.GuiTextMetrics;
pub const DisplayDamageRect = r4x_api.DisplayDamageRect;
pub const DisplayPresentCapabilities = r4x_api.DisplayPresentCapabilities;
pub const DisplayPresentCompletion = r4x_api.DisplayPresentCompletion;
pub const DisplayPresentRequest = r4x_api.DisplayPresentRequest;
pub const DisplayPresentResult = r4x_api.DisplayPresentResult;

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
    return displayBlitXrgb32Stride(x, y, w, h, pixels, pixel_count, w);
}

pub fn displayBlitXrgb32Stride(x: i32, y: i32, w: u32, h: u32, pixels: [*]const u32, pixel_count: u32, source_stride_pixels: u32) callconv(.c) i32 {
    if (x < 0 or y < 0 or w == 0 or h == 0) return -1;
    if (@intFromPtr(pixels) == 0) return -1;
    if (source_stride_pixels < w) return -1;
    const needed = (@as(u64, h) - 1) * @as(u64, source_stride_pixels) + @as(u64, w);
    if (needed > pixel_count) return -2;
    const bytes_needed = needed * 4;
    if (bytes_needed > @as(u64, ~@as(usize, 0))) return -2;

    const source_view = surface.View{
        .pixels = pixels[0..@intCast(needed)],
        .width = w,
        .height = h,
        .pitch_pixels = source_stride_pixels,
    };
    const region = presenter.Region{
        .destination_x = @intCast(x),
        .destination_y = @intCast(y),
        .w = w,
        .h = h,
    };
    const result = presenter.presentXrgb32(.{
        .source = source_view,
        .regions = (&region)[0..1],
        .reason = .legacy_bridge,
    });
    if (!result.success) return -3;
    markDisplayUsed();
    return 0;
}

pub fn displayPresentRegions(
    request: *const DisplayPresentRequest,
    pixels: [*]const u32,
    pixel_count: u32,
    regions: [*]const DisplayDamageRect,
    region_count: u32,
    out: *DisplayPresentResult,
) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return r4x_api.display_present_error_invalid;
    out.* = .{};
    if (@intFromPtr(request) == 0 or @intFromPtr(pixels) == 0 or @intFromPtr(regions) == 0) return r4x_api.display_present_error_invalid;
    if (request.magic != r4x_api.display_present_magic or request.version != r4x_api.display_present_version or
        request.size < @sizeOf(DisplayPresentRequest) or request.format != r4x_api.display_present_format_xrgb32 or
        request.source_width == 0 or request.source_height == 0 or request.source_stride_pixels < request.source_width or
        region_count == 0 or region_count > r4x_api.display_damage_max_regions)
    {
        return r4x_api.display_present_error_invalid;
    }
    const needed = (@as(u64, request.source_height) - 1) * request.source_stride_pixels + request.source_width;
    if (needed > pixel_count) return r4x_api.display_present_error_out_of_range;

    var normalized: [r4x_api.display_damage_max_regions]presenter.Region =
        .{presenter.Region{}} ** r4x_api.display_damage_max_regions;
    var index: usize = 0;
    while (index < region_count) : (index += 1) {
        const region = regions[index];
        if (region.x < 0 or region.y < 0 or region.w == 0 or region.h == 0) return r4x_api.display_present_error_out_of_range;
        const x: u32 = @intCast(region.x);
        const y: u32 = @intCast(region.y);
        if (x >= request.source_width or y >= request.source_height or
            region.w > request.source_width - x or region.h > request.source_height - y)
        {
            return r4x_api.display_present_error_out_of_range;
        }
        normalized[index] = .{
            .source_x = x,
            .source_y = y,
            .destination_x = x,
            .destination_y = y,
            .w = region.w,
            .h = region.h,
        };
    }

    const source_view = surface.View{
        .pixels = pixels[0..pixel_count],
        .width = request.source_width,
        .height = request.source_height,
        .pitch_pixels = request.source_stride_pixels,
        .revision = request.source_generation,
    };
    const result = presenter.presentXrgb32(.{
        .source = source_view,
        .regions = normalized[0..region_count],
        .reason = .redraw,
        .source_generation = request.source_generation,
        .input_tick = request.input_tick,
        .input_tick_valid = (request.flags & r4x_api.display_present_request_flag_input_tick_valid) != 0,
    });
    fillPresentResult(result, out);
    if (!result.success) return r4x_api.display_present_error_unavailable;
    _ = surface_pipeline.present();
    markDisplayUsed();
    return 0;
}

pub fn displayPresentCapabilities(out: *DisplayPresentCapabilities) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return r4x_api.display_present_error_invalid;
    const capabilities = @import("../display/display.zig").presentCapabilities();
    out.* = .{
        .flags = capabilities.flags,
        .formats = capabilities.formats,
        .max_regions = capabilities.max_regions,
        .backend_kind = capabilities.backend_kind,
        .backend_name = capabilities.backend_name,
        .fallback_name = capabilities.fallback_name,
    };
    return 0;
}

pub fn displayPresentCompletion(fence: u64, out: *DisplayPresentCompletion) callconv(.c) i32 {
    if (@intFromPtr(out) == 0 or fence == 0) return r4x_api.display_present_error_invalid;
    const display = @import("../display/display.zig");
    const complete = display.presentFenceCompleted(fence);
    out.* = .{
        .flags = if (complete) r4x_api.display_present_completion_complete else 0,
        .fence = fence,
        .completed_fence = display.highestCompletedFence(),
        .result = if (complete) 0 else r4x_api.display_present_error_unavailable,
    };
    return out.result;
}

fn fillPresentResult(result: presenter.PresentResult, out: *DisplayPresentResult) void {
    out.* = .{
        .flags = (if (result.success) r4x_api.display_present_result_success else 0) |
            (if (result.success and result.fence == result.completed_fence) r4x_api.display_present_result_completed else 0) |
            (if (result.accelerated) r4x_api.display_present_result_accelerated else 0) |
            (if (result.fallback) r4x_api.display_present_result_fallback else 0),
        .source_generation = result.source_generation,
        .present_generation = result.present_generation,
        .fence = result.fence,
        .completed_fence = result.completed_fence,
        .region_count = result.region_count,
        .pixel_count = result.pixel_count,
        .fallback_regions = result.fallback_regions,
        .backend_error = result.backend_error,
        .present_tick = result.present_tick,
        .elapsed_ticks = result.elapsed_ticks,
        .backend_name = result.backend_name,
    };
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
    return font.glyphRowMaskForFont(font_id, codepoint, row);
}

/// Copies one complete rendered glyph after a single bounded codepoint-index
/// lookup.  This append-only bulk operation retains fontGlyphRow for older
/// callers while avoiding one ABI call and one glyph lookup per bitmap row.
pub fn fontGlyphBitmap(font_id: u32, codepoint: u32, out: *GuiGlyphBitmap) callconv(.c) i32 {
    if (@intFromPtr(out) == 0 or codepoint > 0x10FFFF) return -1;
    if (!font.isRenderableFontId(font_id)) return -2;

    const bitmap = font.glyphBitmapForFont(font_id, codepoint);
    out.* = .{
        .width = bitmap.width,
        .height = bitmap.height,
        .advance = bitmap.advance,
        .line_height = bitmap.line_height,
        .baseline = bitmap.baseline,
        .reserved0 = 0,
        .rows = bitmap.rows,
    };
    return 0;
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
