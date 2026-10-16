// R4OS kernel font catalog and small bitmap renderer.
//
// This file is kernel code: it contains the kernel-internal 8x8 fallback font
// for early output, plus R4F1 loader/catalog and renderer helpers for loaded
// system fonts. Host font builders must not import this file as a glyph data
// source; R4F system fonts are generated outside the kernel with their own
// builder data.

const r4f = @import("r4f_format");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const GLYPH_W: u8 = 8;
pub const GLYPH_H: u8 = 8;
// Classic Windows bitmap FNT faces such as SCRIPT are up to 37 pixels high
// and 33 pixels wide.  The cache stays deliberately bounded and contains
// rendered glyph data only; the installed R4F file itself remains on C:.
pub const MAX_GLYPH_W: u8 = 40;
pub const MAX_GLYPH_H: u8 = 40;
pub const MAX_GLYPHS: usize = 256;
pub const MAX_BYTES_PER_ROW: usize = 5;
// The installed catalogue remains bounded because every renderable entry owns
// a decoded transient glyph cache.  Sixty-four external R4F files leave room
// for the supplied system/legacy faces and later user imports without moving
// persistent font bytes into the kernel.
pub const MAX_CATALOG_FONTS: usize = 64;
pub const MAX_FONT_PATH: usize = 96;
pub const MAX_FONT_NAME: usize = 40;
pub const MAX_STATUS_TEXT: usize = 48;
pub const BUILTIN_FONT_ID: u32 = 0;
const INVALID_CODEPOINT: u32 = 0xFFFFFFFF;

pub const FontKind = enum(u16) {
    builtin = 0,
    bitmap = r4f.FONT_KIND_BITMAP,
    windows_vector = r4f.FONT_KIND_WINDOWS_VECTOR,
    sfnt_truetype = r4f.FONT_KIND_SFNT_TRUETYPE,
    unknown = 0xFFFF,
};
const GlyphIndex = extern struct {
    count: u16 = 0,
    codepoints: [MAX_GLYPHS]u32 = .{INVALID_CODEPOINT} ** MAX_GLYPHS,
    glyph_ids: [MAX_GLYPHS]u16 = .{0} ** MAX_GLYPHS,
};
pub const GlyphBitmap = extern struct {
    width: u32 = GLYPH_W,
    height: u32 = GLYPH_H,
    advance: u32 = GLYPH_W,
    line_height: u32 = GLYPH_H,
    baseline: i16 = GLYPH_H - 1,
    fallback: bool = false,
    rows: [MAX_GLYPH_H]u64 = .{0} ** MAX_GLYPH_H,
};
pub const TextMetrics = extern struct {
    width: u32 = 0,
    height: u32 = 0,
    baseline: i16 = 0,
    line_height: u32 = 0,
    visible_bytes: usize = 0,
    clipped: bool = false,
};
pub const CatalogEntry = extern struct {
    used: bool = false,
    path: [MAX_FONT_PATH]u8 = .{0} ** MAX_FONT_PATH,
    path_len: usize = 0,
    family: [MAX_FONT_NAME]u8 = .{0} ** MAX_FONT_NAME,
    family_len: usize = 0,
    face: [MAX_FONT_NAME]u8 = .{0} ** MAX_FONT_NAME,
    face_len: usize = 0,
    style: [MAX_FONT_NAME]u8 = .{0} ** MAX_FONT_NAME,
    style_len: usize = 0,
    kind: FontKind = .unknown,
    weight: u16 = 0,
    style_flags: u32 = 0,
    charset_flags: u32 = 0,
    face_count: u16 = 0,
    strike_count: u16 = 0,
    glyph_count: u32 = 0,
    renderable: bool = false,
    selected: bool = false,
    width: u8 = GLYPH_W,
    height: u8 = GLYPH_H,
    max_advance: u8 = GLYPH_W,
    ascent: i16 = GLYPH_H - 1,
    descent: i16 = 1,
    line_height: u8 = GLYPH_H,
    baseline: i16 = GLYPH_H - 1,
    status: [MAX_STATUS_TEXT]u8 = .{0} ** MAX_STATUS_TEXT,
    status_len: usize = 0,
};
const ActiveFont = extern struct {
    width: u8,
    height: u8,
    bytes_per_row: u8,
    max_advance: u8,
    ascent: i16,
    descent: i16,
    line_height: u8,
    baseline: i16,
    family: [MAX_FONT_NAME]u8 = .{0} ** MAX_FONT_NAME,
    family_len: usize = 0,
    face: [MAX_FONT_NAME]u8 = .{0} ** MAX_FONT_NAME,
    face_len: usize = 0,
    path: [MAX_FONT_PATH]u8 = .{0} ** MAX_FONT_PATH,
    path_len: usize = 0,
};
const CachedFont = extern struct {
    used: bool = false,
    width: u8 = GLYPH_W,
    height: u8 = GLYPH_H,
    bytes_per_row: u8 = 1,
    max_advance: u8 = GLYPH_W,
    ascent: i16 = GLYPH_H - 1,
    descent: i16 = 1,
    line_height: u8 = GLYPH_H,
    baseline: i16 = GLYPH_H - 1,
    glyphs: [MAX_GLYPHS][MAX_GLYPH_H][MAX_BYTES_PER_ROW]u8 = .{.{.{0} ** MAX_BYTES_PER_ROW} ** MAX_GLYPH_H} ** MAX_GLYPHS,
    widths: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS,
    advances: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS,
    codepoints: [MAX_GLYPHS]u32 = .{INVALID_CODEPOINT} ** MAX_GLYPHS,
    index: GlyphIndex = .{},
};
pub const CatalogState = extern struct {
    revision: u32 = 1,
    catalog: [MAX_CATALOG_FONTS]CatalogEntry = .{CatalogEntry{}} ** MAX_CATALOG_FONTS,
    catalog_entries: usize = 0,
    cached_fonts: [*]CachedFont = @ptrFromInt(@alignOf(CachedFont)),
    cached_fonts_len: usize = 0,
    temp_cached_font: CachedFont = .{},
    active_present: bool = false,
    active: ActiveFont = .{ .width = 0, .height = 0, .bytes_per_row = 0, .max_advance = 0, .ascent = 0, .descent = 0, .line_height = 0, .baseline = 0 },
    active_font_id: u32 = BUILTIN_FONT_ID,
    active_glyphs: [MAX_GLYPHS][MAX_GLYPH_H][MAX_BYTES_PER_ROW]u8 = .{.{.{0} ** MAX_BYTES_PER_ROW} ** MAX_GLYPH_H} ** MAX_GLYPHS,
    active_glyph_widths: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS,
    active_glyph_advances: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS,
    active_index: GlyphIndex = .{},

    pub fn reset(self: *CatalogState) void {
        r4os_rust_font_state_reset(self);
    }
    pub fn resetCatalog(self: *CatalogState) void {
        r4os_rust_font_state_reset_catalog(self);
    }
    pub fn glyphWidth(self: *const CatalogState) u32 {
        return r4os_rust_font_state_glyph_width(self);
    }
    pub fn glyphHeight(self: *const CatalogState) u32 {
        return r4os_rust_font_state_glyph_height(self);
    }
    pub fn baseline(self: *const CatalogState) i16 {
        return r4os_rust_font_state_baseline(self);
    }
    pub fn catalogCount(self: *const CatalogState) usize {
        return r4os_rust_font_state_catalog_count(self);
    }
    pub fn catalogEntry(self: *const CatalogState, index: usize) ?CatalogEntry {
        var out: CatalogEntry = undefined;
        return if (r4os_rust_font_state_catalog_entry(self, index, &out)) out else null;
    }
    pub fn fontCount(self: *const CatalogState) usize {
        return r4os_rust_font_state_font_count(self);
    }
    pub fn fontIdForCatalogIndex(self: *const CatalogState, index: usize) u32 {
        return r4os_rust_font_state_font_id_for_catalog_index(self, index);
    }
    pub fn currentFontId(self: *const CatalogState) u32 {
        return r4os_rust_font_state_current_font_id(self);
    }
    pub fn isRenderableFontId(self: *const CatalogState, font_id: u32) bool {
        return r4os_rust_font_state_is_renderable_font_id(self, font_id);
    }
    pub fn normalizeFontId(self: *const CatalogState, font_id: u32) u32 {
        return r4os_rust_font_state_normalize_font_id(self, font_id);
    }
    pub fn catalogEntryForFontId(self: *const CatalogState, font_id: u32) ?CatalogEntry {
        var out: CatalogEntry = undefined;
        return if (r4os_rust_font_state_catalog_entry_for_font_id(self, font_id, &out)) out else null;
    }
    pub fn glyphRow(self: *const CatalogState, c: u8, row: u32) u8 {
        return r4os_rust_font_state_glyph_row(self, c, row);
    }
    pub fn glyphPixel(self: *const CatalogState, c: u8, row: u32, col: u32) bool {
        return r4os_rust_font_state_glyph_pixel(self, c, row, col);
    }
    pub fn glyphBitmapWidth(self: *const CatalogState, c: u8) u32 {
        return r4os_rust_font_state_glyph_bitmap_width(self, c);
    }
    pub fn glyphAdvance(self: *const CatalogState, c: u8) u32 {
        return r4os_rust_font_state_glyph_advance(self, c);
    }
    pub fn glyphHeightForFont(self: *const CatalogState, font_id: u32) u32 {
        return r4os_rust_font_state_glyph_height_for_font(self, font_id);
    }
    pub fn glyphAdvanceForFont(self: *const CatalogState, font_id: u32, codepoint: u32) u32 {
        return r4os_rust_font_state_glyph_advance_for_font(self, font_id, codepoint);
    }
    pub fn glyphBitmapWidthForFont(self: *const CatalogState, font_id: u32, codepoint: u32) u32 {
        return r4os_rust_font_state_glyph_bitmap_width_for_font(self, font_id, codepoint);
    }
    pub fn glyphPixelForFont(self: *const CatalogState, font_id: u32, codepoint: u32, row: u32, col: u32) bool {
        return r4os_rust_font_state_glyph_pixel_for_font(self, font_id, codepoint, row, col);
    }
    pub fn glyphRowMaskForFont(self: *const CatalogState, font_id: u32, codepoint: u32, row: u32) u64 {
        return r4os_rust_font_state_glyph_row_mask_for_font(self, font_id, codepoint, row);
    }
    pub fn glyphBitmapForFont(self: *const CatalogState, font_id: u32, codepoint: u32) GlyphBitmap {
        var out: GlyphBitmap = undefined;
        r4os_rust_font_state_glyph_bitmap_for_font(self, font_id, codepoint, &out);
        return out;
    }
    pub fn measure(self: *const CatalogState, text: []const u8) TextMetrics {
        var out: TextMetrics = undefined;
        r4os_rust_font_state_measure(self, text.ptr, text.len, &out);
        return out;
    }
    pub fn measureZ(self: *const CatalogState, text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
        var out: TextMetrics = undefined;
        r4os_rust_font_state_measure_z(self, text_ptr, max_bytes, &out);
        return out;
    }
    pub fn measureWithFont(self: *const CatalogState, font_id: u32, text: []const u8) TextMetrics {
        var out: TextMetrics = undefined;
        r4os_rust_font_state_measure_with_font(self, font_id, text.ptr, text.len, &out);
        return out;
    }
    pub fn measureZWithFont(self: *const CatalogState, font_id: u32, text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
        var out: TextMetrics = undefined;
        r4os_rust_font_state_measure_z_with_font(self, font_id, text_ptr, max_bytes, &out);
        return out;
    }
    pub fn baselineForFont(self: *const CatalogState, font_id: u32) i16 {
        return r4os_rust_font_state_baseline_for_font(self, font_id);
    }
    pub fn maxAdvanceForFont(self: *const CatalogState, font_id: u32) u32 {
        return r4os_rust_font_state_max_advance_for_font(self, font_id);
    }
    pub fn loadR4F(self: *CatalogState, bytes: []const u8) bool {
        return r4os_rust_font_state_load_r4_f(self, bytes.ptr, bytes.len);
    }
    pub fn loadR4FNamed(self: *CatalogState, path: []const u8, bytes: []const u8) bool {
        return r4os_rust_font_state_load_r4_f_named(self, path.ptr, path.len, bytes.ptr, bytes.len);
    }
    pub fn registerR4F(self: *CatalogState, path: []const u8, bytes: []const u8) bool {
        return r4os_rust_font_state_register_r4_f(self, path.ptr, path.len, bytes.ptr, bytes.len);
    }
};
pub const CatalogView = extern struct {
    bank: u8 = 255,
    unwind: @import("../sched/task_context.zig").UnwindToken = .{},
    pub fn state(self: *const CatalogView) *const CatalogState {
        return r4os_rust_font_view_state(self);
    }
    pub fn release(self: *CatalogView) void {
        r4os_rust_font_view_release(self);
    }
};
pub const CatalogReplacement = extern struct {
    bank: u8,
    unwind: @import("../sched/task_context.zig").UnwindToken,
    owned: bool = true,
    pub fn state(self: *CatalogReplacement) *CatalogState {
        return r4os_rust_font_replacement_state(self);
    }
    pub fn commit(self: *CatalogReplacement) void {
        r4os_rust_font_replacement_commit(self);
    }
    pub fn abort(self: *CatalogReplacement) void {
        r4os_rust_font_replacement_abort(self);
    }
};
pub fn acquireCatalog() CatalogView {
    var out: CatalogView = undefined;
    r4os_rust_font_acquire(&out);
    return out;
}
pub fn beginCatalogReplacement(preserve: bool) ?CatalogReplacement {
    var out: CatalogReplacement = undefined;
    return if (r4os_rust_font_begin(preserve, &out)) out else null;
}
pub const catalog_bank_bytes = @sizeOf(CatalogState) + @sizeOf([MAX_CATALOG_FONTS]CachedFont);

pub fn catalogRevision() u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().revision;
}

pub fn reset() void {
    var transaction = beginCatalogReplacement(true) orelse {
        return;
    };
    defer transaction.abort();
    transaction.state().reset();
    transaction.commit();
}

pub fn resetCatalog() void {
    var transaction = beginCatalogReplacement(false) orelse {
        return;
    };
    defer transaction.abort();
    transaction.commit();
}

pub fn glyphWidth() u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphWidth();
}

pub fn glyphHeight() u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphHeight();
}

pub fn baseline() i16 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().baseline();
}

pub fn catalogCount() usize {
    var view = acquireCatalog();
    defer view.release();
    return view.state().catalogCount();
}

pub fn catalogEntry(index: usize) ?CatalogEntry {
    var view = acquireCatalog();
    defer view.release();
    return view.state().catalogEntry(index);
}

pub fn fontCount() usize {
    var view = acquireCatalog();
    defer view.release();
    return view.state().fontCount();
}

pub fn fontIdForCatalogIndex(index: usize) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().fontIdForCatalogIndex(index);
}

pub fn currentFontId() u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().currentFontId();
}

pub fn isRenderableFontId(font_id: u32) bool {
    var view = acquireCatalog();
    defer view.release();
    return view.state().isRenderableFontId(font_id);
}

pub fn normalizeFontId(font_id: u32) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().normalizeFontId(font_id);
}

pub fn catalogEntryForFontId(font_id: u32) ?CatalogEntry {
    var view = acquireCatalog();
    defer view.release();
    return view.state().catalogEntryForFontId(font_id);
}

pub fn glyphRow(c: u8, row: u32) u8 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphRow(c, row);
}

pub fn glyphPixel(c: u8, row: u32, col: u32) bool {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphPixel(c, row, col);
}

pub fn glyphBitmapWidth(c: u8) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphBitmapWidth(c);
}

pub fn glyphAdvance(c: u8) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphAdvance(c);
}

pub fn glyphHeightForFont(font_id: u32) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphHeightForFont(font_id);
}

pub fn glyphAdvanceForFont(font_id: u32, codepoint: u32) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphAdvanceForFont(font_id, codepoint);
}

pub fn glyphBitmapWidthForFont(font_id: u32, codepoint: u32) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphBitmapWidthForFont(font_id, codepoint);
}

pub fn glyphPixelForFont(font_id: u32, codepoint: u32, row: u32, col: u32) bool {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphPixelForFont(font_id, codepoint, row, col);
}

pub fn glyphRowMaskForFont(font_id: u32, codepoint: u32, row: u32) u64 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphRowMaskForFont(font_id, codepoint, row);
}

pub fn glyphBitmapForFont(font_id: u32, codepoint: u32) GlyphBitmap {
    var view = acquireCatalog();
    defer view.release();
    return view.state().glyphBitmapForFont(font_id, codepoint);
}

pub fn measure(text: []const u8) TextMetrics {
    var view = acquireCatalog();
    defer view.release();
    return view.state().measure(text);
}

pub fn measureZ(text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
    var view = acquireCatalog();
    defer view.release();
    return view.state().measureZ(text_ptr, max_bytes);
}

pub fn measureWithFont(font_id: u32, text: []const u8) TextMetrics {
    var view = acquireCatalog();
    defer view.release();
    return view.state().measureWithFont(font_id, text);
}

pub fn measureZWithFont(font_id: u32, text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
    var view = acquireCatalog();
    defer view.release();
    return view.state().measureZWithFont(font_id, text_ptr, max_bytes);
}

pub fn baselineForFont(font_id: u32) i16 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().baselineForFont(font_id);
}

pub fn maxAdvanceForFont(font_id: u32) u32 {
    var view = acquireCatalog();
    defer view.release();
    return view.state().maxAdvanceForFont(font_id);
}

pub fn loadR4F(bytes: []const u8) bool {
    var transaction = beginCatalogReplacement(true) orelse {
        return false;
    };
    defer transaction.abort();
    if (!transaction.state().loadR4F(bytes)) return false;
    transaction.commit();
    return true;
}

pub fn loadR4FNamed(path: []const u8, bytes: []const u8) bool {
    var transaction = beginCatalogReplacement(true) orelse {
        return false;
    };
    defer transaction.abort();
    if (!transaction.state().loadR4FNamed(path, bytes)) return false;
    transaction.commit();
    return true;
}

pub fn registerR4F(path: []const u8, bytes: []const u8) bool {
    var transaction = beginCatalogReplacement(true) orelse {
        return false;
    };
    defer transaction.abort();
    const renderable = transaction.state().registerR4F(path, bytes);
    transaction.commit();
    return renderable;
}

pub fn glyph(c: u8) [8]u8 {
    return builtinGlyphForCodepoint(c);
}
pub fn builtinGlyphForCodepoint(cp: u32) [8]u8 {
    var out: [8]u8 = undefined;
    r4os_rust_font_builtin(cp, &out);
    return out;
}
fn buildGlyphIndex(codepoints: *const [MAX_GLYPHS]u32, out: *GlyphIndex) bool {
    return r4os_rust_font_build_index(codepoints, out);
}
fn indexedGlyphId(index: *const GlyphIndex, cp: u32, probes: ?*u16) ?usize {
    const value = r4os_rust_font_indexed_id(index, cp, probes);
    return if (value == ~@as(usize, 0)) null else value;
}
const DecodedScalar = extern struct { codepoint: u32, consumed: usize };
pub fn decodeUtf8Scalar(value: []const u8, start: usize) DecodedScalar {
    var out: DecodedScalar = undefined;
    r4os_rust_font_decode(value.ptr, value.len, start, &out);
    return out;
}

pub fn kindName(kind: FontKind) []const u8 {
    var len: usize = 0;
    const p = r4os_rust_font_kind_name(@intFromEnum(kind), &len);
    return p[0..len];
}
extern fn r4os_rust_font_state_reset(*CatalogState) void;
extern fn r4os_rust_font_state_reset_catalog(*CatalogState) void;
extern fn r4os_rust_font_state_glyph_width(*const CatalogState) u32;
extern fn r4os_rust_font_state_glyph_height(*const CatalogState) u32;
extern fn r4os_rust_font_state_baseline(*const CatalogState) i16;
extern fn r4os_rust_font_state_catalog_count(*const CatalogState) usize;
extern fn r4os_rust_font_state_catalog_entry(*const CatalogState, usize, *CatalogEntry) bool;
extern fn r4os_rust_font_state_font_count(*const CatalogState) usize;
extern fn r4os_rust_font_state_font_id_for_catalog_index(*const CatalogState, usize) u32;
extern fn r4os_rust_font_state_current_font_id(*const CatalogState) u32;
extern fn r4os_rust_font_state_is_renderable_font_id(*const CatalogState, u32) bool;
extern fn r4os_rust_font_state_normalize_font_id(*const CatalogState, u32) u32;
extern fn r4os_rust_font_state_catalog_entry_for_font_id(*const CatalogState, u32, *CatalogEntry) bool;
extern fn r4os_rust_font_state_glyph_row(*const CatalogState, u8, u32) u8;
extern fn r4os_rust_font_state_glyph_pixel(*const CatalogState, u8, u32, u32) bool;
extern fn r4os_rust_font_state_glyph_bitmap_width(*const CatalogState, u8) u32;
extern fn r4os_rust_font_state_glyph_advance(*const CatalogState, u8) u32;
extern fn r4os_rust_font_state_glyph_height_for_font(*const CatalogState, u32) u32;
extern fn r4os_rust_font_state_glyph_advance_for_font(*const CatalogState, u32, u32) u32;
extern fn r4os_rust_font_state_glyph_bitmap_width_for_font(*const CatalogState, u32, u32) u32;
extern fn r4os_rust_font_state_glyph_pixel_for_font(*const CatalogState, u32, u32, u32, u32) bool;
extern fn r4os_rust_font_state_glyph_row_mask_for_font(*const CatalogState, u32, u32, u32) u64;
extern fn r4os_rust_font_state_glyph_bitmap_for_font(*const CatalogState, u32, u32, *GlyphBitmap) void;
extern fn r4os_rust_font_state_measure(*const CatalogState, [*]const u8, usize, *TextMetrics) void;
extern fn r4os_rust_font_state_measure_z(*const CatalogState, [*]const u8, usize, *TextMetrics) void;
extern fn r4os_rust_font_state_measure_with_font(*const CatalogState, u32, [*]const u8, usize, *TextMetrics) void;
extern fn r4os_rust_font_state_measure_z_with_font(*const CatalogState, u32, [*]const u8, usize, *TextMetrics) void;
extern fn r4os_rust_font_state_baseline_for_font(*const CatalogState, u32) i16;
extern fn r4os_rust_font_state_max_advance_for_font(*const CatalogState, u32) u32;
extern fn r4os_rust_font_state_load_r4_f(*CatalogState, [*]const u8, usize) bool;
extern fn r4os_rust_font_state_load_r4_f_named(*CatalogState, [*]const u8, usize, [*]const u8, usize) bool;
extern fn r4os_rust_font_state_register_r4_f(*CatalogState, [*]const u8, usize, [*]const u8, usize) bool;
extern fn r4os_rust_font_acquire(*CatalogView) void;
extern fn r4os_rust_font_view_state(*const CatalogView) *const CatalogState;
extern fn r4os_rust_font_view_release(*CatalogView) void;
extern fn r4os_rust_font_begin(bool, *CatalogReplacement) bool;
extern fn r4os_rust_font_replacement_state(*const CatalogReplacement) *CatalogState;
extern fn r4os_rust_font_replacement_commit(*CatalogReplacement) void;
extern fn r4os_rust_font_replacement_abort(*CatalogReplacement) void;
extern fn r4os_rust_font_builtin(u32, *[8]u8) void;
extern fn r4os_rust_font_build_index(*const [MAX_GLYPHS]u32, *GlyphIndex) bool;
extern fn r4os_rust_font_indexed_id(*const GlyphIndex, u32, ?*u16) usize;
extern fn r4os_rust_font_decode([*]const u8, usize, usize, *DecodedScalar) void;
extern fn r4os_rust_font_kind_name(u16, *usize) [*]const u8;

test "font measurement advances once per utf8 scalar" {
    const std = @import("std");
    const metrics = measureWithFont(BUILTIN_FONT_ID, "A\xc3\xa4B\n\xc3\x9c");
    try std.testing.expectEqual(@as(u32, 24), metrics.width);
    try std.testing.expectEqual(@as(u32, 16), metrics.height);
    try std.testing.expectEqual(@as(usize, 7), metrics.visible_bytes);

    const malformed = measureWithFont(BUILTIN_FONT_ID, "\xc3(");
    try std.testing.expectEqual(@as(u32, 16), malformed.width);
    try std.testing.expectEqual(@as(usize, 2), malformed.visible_bytes);
}

test "builtin fallback contains western glyphs" {
    const std = @import("std");
    try std.testing.expect(!std.mem.eql(u8, &builtinGlyphForCodepoint(0xE4), &builtinGlyphForCodepoint('?')));
    try std.testing.expect(!std.mem.eql(u8, &builtinGlyphForCodepoint(0xDF), &builtinGlyphForCodepoint('?')));
}

test "bounded glyph index matches the former linear lookup" {
    const std = @import("std");
    var codepoints: [MAX_GLYPHS]u32 = .{INVALID_CODEPOINT} ** MAX_GLYPHS;
    var glyph_id: usize = 0;
    while (glyph_id < codepoints.len) : (glyph_id += 1) {
        codepoints[glyph_id] = if (glyph_id == 255)
            0x1F642
        else
            @as(u32, 0x1000) + @as(u32, @intCast(254 - @min(glyph_id, 254)));
    }
    // The old scan selected the lowest glyph ID for duplicate codepoints.
    codepoints[17] = codepoints[93];

    var index: GlyphIndex = .{};
    try std.testing.expect(buildGlyphIndex(&codepoints, &index));
    try std.testing.expectEqual(@as(u16, MAX_GLYPHS), index.count);

    for (codepoints) |codepoint| {
        var expected: ?usize = null;
        for (codepoints, 0..) |candidate, candidate_id| {
            if (candidate == codepoint) {
                expected = candidate_id;
                break;
            }
        }
        var probes: u16 = 0;
        try std.testing.expectEqual(expected, indexedGlyphId(&index, codepoint, &probes));
        try std.testing.expect(probes <= 9);
    }

    var missing_probes: u16 = 0;
    try std.testing.expectEqual(@as(?usize, null), indexedGlyphId(&index, 0x10FFFF, &missing_probes));
    try std.testing.expect(missing_probes <= 9);
    try std.testing.expectEqual(@as(?usize, 255), indexedGlyphId(&index, 0x1F642, null));

    const invalid_map: [MAX_GLYPHS]u32 = .{INVALID_CODEPOINT} ** MAX_GLYPHS;
    try std.testing.expect(!buildGlyphIndex(&invalid_map, &index));
    try std.testing.expectEqual(@as(u16, 0), index.count);
}

test "bulk glyph rows equal indexed row and pixel references" {
    const std = @import("std");
    resetCatalog();
    defer resetCatalog();

    var preparation = beginCatalogReplacement(false).?;
    defer preparation.abort();
    const state = preparation.state();
    state.catalog_entries = 1;
    state.catalog[0] = .{ .used = true, .renderable = true, .width = 5, .height = 3, .max_advance = 6, .line_height = 4, .baseline = 2 };
    state.cached_fonts[0] = .{
        .used = true,
        .width = 5,
        .height = 3,
        .bytes_per_row = 1,
        .max_advance = 6,
        .line_height = 4,
        .baseline = 2,
    };
    const cached = &state.cached_fonts[0];
    cached.codepoints[7] = 0x1F642;
    cached.widths[7] = 5;
    cached.advances[7] = 6;
    cached.glyphs[7][0][0] = 0b10101000;
    cached.glyphs[7][1][0] = 0b01010000;
    cached.glyphs[7][2][0] = 0b11111000;
    try std.testing.expect(buildGlyphIndex(&cached.codepoints, &cached.index));

    preparation.commit();
    const bitmap = glyphBitmapForFont(1, 0x1F642);
    try std.testing.expectEqual(@as(u32, 5), bitmap.width);
    try std.testing.expectEqual(@as(u32, 3), bitmap.height);
    try std.testing.expectEqual(@as(u32, 6), bitmap.advance);
    try std.testing.expectEqual(@as(u32, 4), bitmap.line_height);
    try std.testing.expectEqual(@as(i16, 2), bitmap.baseline);
    try std.testing.expect(!bitmap.fallback);
    try std.testing.expectEqual(@as(u32, 6), measureWithFont(1, "\xF0\x9F\x99\x82").width);

    var row: u32 = 0;
    while (row < MAX_GLYPH_H) : (row += 1) {
        const row_mask = glyphRowMaskForFont(1, 0x1F642, row);
        try std.testing.expectEqual(row_mask, bitmap.rows[row]);
        var column: u32 = 0;
        while (column < MAX_GLYPH_W) : (column += 1) {
            const expected = (row_mask & (@as(u64, 1) << @intCast(column))) != 0;
            try std.testing.expectEqual(expected, glyphPixelForFont(1, 0x1F642, row, column));
        }
    }

    const fallback = glyphBitmapForFont(1, 0x10FFFF);
    try std.testing.expect(fallback.fallback);
    try std.testing.expectEqual(@as(u32, 3), fallback.height);
    row = 0;
    while (row < MAX_GLYPH_H) : (row += 1) {
        try std.testing.expectEqual(glyphRowMaskForFont(1, 0x10FFFF, row), fallback.rows[row]);
    }

    resetCatalog();
    try std.testing.expect(!isRenderableFontId(1));
}

comptime {
    if (@sizeOf(GlyphIndex) != 1540 or @alignOf(GlyphIndex) != 4 or @offsetOf(GlyphIndex, "count") != 0 or @offsetOf(GlyphIndex, "codepoints") != 4 or @offsetOf(GlyphIndex, "glyph_ids") != 1028) @compileError("Native font GlyphIndex layout");
}

comptime {
    if (@sizeOf(GlyphBitmap) != 344 or @alignOf(GlyphBitmap) != 8 or @offsetOf(GlyphBitmap, "width") != 0 or @offsetOf(GlyphBitmap, "height") != 4 or @offsetOf(GlyphBitmap, "advance") != 8 or @offsetOf(GlyphBitmap, "line_height") != 12 or @offsetOf(GlyphBitmap, "baseline") != 16 or @offsetOf(GlyphBitmap, "fallback") != 18 or @offsetOf(GlyphBitmap, "rows") != 24) @compileError("Native font GlyphBitmap layout");
}

comptime {
    if (@sizeOf(TextMetrics) != 32 or @alignOf(TextMetrics) != 8 or @offsetOf(TextMetrics, "width") != 0 or @offsetOf(TextMetrics, "height") != 4 or @offsetOf(TextMetrics, "baseline") != 8 or @offsetOf(TextMetrics, "line_height") != 12 or @offsetOf(TextMetrics, "visible_bytes") != 16 or @offsetOf(TextMetrics, "clipped") != 24) @compileError("Native font TextMetrics layout");
}

comptime {
    if (@sizeOf(CatalogEntry) != 352 or @alignOf(CatalogEntry) != 8 or @offsetOf(CatalogEntry, "used") != 0 or @offsetOf(CatalogEntry, "path") != 1 or @offsetOf(CatalogEntry, "path_len") != 104 or @offsetOf(CatalogEntry, "family") != 112 or @offsetOf(CatalogEntry, "family_len") != 152 or @offsetOf(CatalogEntry, "face") != 160 or @offsetOf(CatalogEntry, "face_len") != 200 or @offsetOf(CatalogEntry, "style") != 208 or @offsetOf(CatalogEntry, "style_len") != 248 or @offsetOf(CatalogEntry, "kind") != 256 or @offsetOf(CatalogEntry, "weight") != 258 or @offsetOf(CatalogEntry, "style_flags") != 260 or @offsetOf(CatalogEntry, "charset_flags") != 264 or @offsetOf(CatalogEntry, "face_count") != 268 or @offsetOf(CatalogEntry, "strike_count") != 270 or @offsetOf(CatalogEntry, "glyph_count") != 272 or @offsetOf(CatalogEntry, "renderable") != 276 or @offsetOf(CatalogEntry, "selected") != 277 or @offsetOf(CatalogEntry, "width") != 278 or @offsetOf(CatalogEntry, "height") != 279 or @offsetOf(CatalogEntry, "max_advance") != 280 or @offsetOf(CatalogEntry, "ascent") != 282 or @offsetOf(CatalogEntry, "descent") != 284 or @offsetOf(CatalogEntry, "line_height") != 286 or @offsetOf(CatalogEntry, "baseline") != 288 or @offsetOf(CatalogEntry, "status") != 290 or @offsetOf(CatalogEntry, "status_len") != 344) @compileError("Native font CatalogEntry layout");
}

comptime {
    if (@sizeOf(ActiveFont) != 216 or @alignOf(ActiveFont) != 8 or @offsetOf(ActiveFont, "width") != 0 or @offsetOf(ActiveFont, "height") != 1 or @offsetOf(ActiveFont, "bytes_per_row") != 2 or @offsetOf(ActiveFont, "max_advance") != 3 or @offsetOf(ActiveFont, "ascent") != 4 or @offsetOf(ActiveFont, "descent") != 6 or @offsetOf(ActiveFont, "line_height") != 8 or @offsetOf(ActiveFont, "baseline") != 10 or @offsetOf(ActiveFont, "family") != 12 or @offsetOf(ActiveFont, "family_len") != 56 or @offsetOf(ActiveFont, "face") != 64 or @offsetOf(ActiveFont, "face_len") != 104 or @offsetOf(ActiveFont, "path") != 112 or @offsetOf(ActiveFont, "path_len") != 208) @compileError("Native font ActiveFont layout");
}

comptime {
    if (@sizeOf(CachedFont) != 54292 or @alignOf(CachedFont) != 4 or @offsetOf(CachedFont, "used") != 0 or @offsetOf(CachedFont, "width") != 1 or @offsetOf(CachedFont, "height") != 2 or @offsetOf(CachedFont, "bytes_per_row") != 3 or @offsetOf(CachedFont, "max_advance") != 4 or @offsetOf(CachedFont, "ascent") != 6 or @offsetOf(CachedFont, "descent") != 8 or @offsetOf(CachedFont, "line_height") != 10 or @offsetOf(CachedFont, "baseline") != 12 or @offsetOf(CachedFont, "glyphs") != 14 or @offsetOf(CachedFont, "widths") != 51214 or @offsetOf(CachedFont, "advances") != 51470 or @offsetOf(CachedFont, "codepoints") != 51728 or @offsetOf(CachedFont, "index") != 52752) @compileError("Native font CachedFont layout");
}

comptime {
    if (@sizeOf(CatalogState) != 130328 or @alignOf(CatalogState) != 8 or @offsetOf(CatalogState, "revision") != 0 or @offsetOf(CatalogState, "catalog") != 8 or @offsetOf(CatalogState, "catalog_entries") != 22536 or @offsetOf(CatalogState, "cached_fonts") != 22544 or @offsetOf(CatalogState, "cached_fonts_len") != 22552 or @offsetOf(CatalogState, "temp_cached_font") != 22560 or @offsetOf(CatalogState, "active_present") != 76852 or @offsetOf(CatalogState, "active") != 76856 or @offsetOf(CatalogState, "active_font_id") != 77072 or @offsetOf(CatalogState, "active_glyphs") != 77076 or @offsetOf(CatalogState, "active_glyph_widths") != 128276 or @offsetOf(CatalogState, "active_glyph_advances") != 128532 or @offsetOf(CatalogState, "active_index") != 128788) @compileError("Native font CatalogState layout");
}

comptime {
    if (@sizeOf(CatalogView) != 24 or @alignOf(CatalogView) != 8 or @offsetOf(CatalogView, "bank") != 0 or @offsetOf(CatalogView, "unwind") != 8) @compileError("Native font CatalogView layout");
}

comptime {
    if (@sizeOf(CatalogReplacement) != 32 or @alignOf(CatalogReplacement) != 8 or @offsetOf(CatalogReplacement, "bank") != 0 or @offsetOf(CatalogReplacement, "unwind") != 8 or @offsetOf(CatalogReplacement, "owned") != 24) @compileError("Native font CatalogReplacement layout");
}

comptime {
    if (@sizeOf(DecodedScalar) != 16 or @alignOf(DecodedScalar) != 8 or @offsetOf(DecodedScalar, "codepoint") != 0 or @offsetOf(DecodedScalar, "consumed") != 8) @compileError("Native font DecodedScalar layout");
}
