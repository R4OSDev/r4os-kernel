// R4OS kernel font catalog and small bitmap renderer.
//
// This file is kernel code: it contains the kernel-internal 8x8 fallback font
// for early output, plus R4F1 loader/catalog and renderer helpers for loaded
// system fonts. Host font builders must not import this file as a glyph data
// source; R4F system fonts are generated outside the kernel with their own
// builder data.

const r4f = @import("r4f_format");

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

const GlyphIndex = struct {
    count: u16 = 0,
    codepoints: [MAX_GLYPHS]u32 = .{INVALID_CODEPOINT} ** MAX_GLYPHS,
    glyph_ids: [MAX_GLYPHS]u16 = .{0} ** MAX_GLYPHS,
};

pub const GlyphBitmap = struct {
    width: u32 = GLYPH_W,
    height: u32 = GLYPH_H,
    advance: u32 = GLYPH_W,
    line_height: u32 = GLYPH_H,
    baseline: i16 = GLYPH_H - 1,
    fallback: bool = false,
    rows: [MAX_GLYPH_H]u64 = .{0} ** MAX_GLYPH_H,
};

pub const FontKind = enum(u16) {
    builtin = 0,
    bitmap = r4f.FONT_KIND_BITMAP,
    windows_vector = r4f.FONT_KIND_WINDOWS_VECTOR,
    sfnt_truetype = r4f.FONT_KIND_SFNT_TRUETYPE,
    unknown = 0xFFFF,
};

pub const TextMetrics = struct {
    width: u32 = 0,
    height: u32 = 0,
    baseline: i16 = 0,
    line_height: u32 = 0,
    visible_bytes: usize = 0,
    clipped: bool = false,
};

pub const CatalogEntry = struct {
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

const TableSlice = struct {
    data: []const u8 = &[_]u8{},

    fn valid(self: TableSlice) bool {
        return self.data.len != 0;
    }
};

const ParsedFont = struct {
    valid: bool = false,
    kind: FontKind = .unknown,
    weight: u16 = 0,
    style_flags: u32 = 0,
    charset_flags: u32 = 0,
    face_count: u16 = 0,
    strike_count: u16 = 0,
    glyph_count: u32 = 0,
    family: []const u8 = "R4OS",
    face: []const u8 = "Builtin",
    style: []const u8 = "Regular",
    renderable: bool = false,
    reason: []const u8 = "invalid",
    width: u8 = GLYPH_W,
    height: u8 = GLYPH_H,
    bytes_per_row: u8 = 1,
    max_advance: u8 = GLYPH_W,
    ascent: i16 = GLYPH_H - 1,
    descent: i16 = 1,
    line_height: u8 = GLYPH_H,
    baseline: i16 = GLYPH_H - 1,
    strike_id: u16 = 0,
    strike_table: TableSlice = .{},
    map_table: TableSlice = .{},
    metric_table: TableSlice = .{},
    bitmap_table: TableSlice = .{},
};

const ActiveFont = struct {
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

const CachedFont = struct {
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

var catalog: [MAX_CATALOG_FONTS]CatalogEntry = .{CatalogEntry{}} ** MAX_CATALOG_FONTS;
var catalog_entries: usize = 0;
var cached_fonts: [MAX_CATALOG_FONTS]CachedFont = .{CachedFont{}} ** MAX_CATALOG_FONTS;
var temp_cached_font: CachedFont = .{};
var active: ?ActiveFont = null;
var active_font_id: u32 = BUILTIN_FONT_ID;
var active_glyphs: [MAX_GLYPHS][MAX_GLYPH_H][MAX_BYTES_PER_ROW]u8 = .{.{.{0} ** MAX_BYTES_PER_ROW} ** MAX_GLYPH_H} ** MAX_GLYPHS;
var active_glyph_widths: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS;
var active_glyph_advances: [MAX_GLYPHS]u8 = .{GLYPH_W} ** MAX_GLYPHS;
var active_index: GlyphIndex = .{};

pub fn glyph(c: u8) [8]u8 {
    return builtinGlyphForCodepoint(c);
}

pub fn builtinGlyphForCodepoint(codepoint: u32) [8]u8 {
    if (codepoint >= 0x20 and codepoint <= 0x7E) return builtin_ascii_font[codepoint - 0x20];
    return switch (codepoint) {
        0x00A0 => builtin_ascii_font[0],
        0x00C0 => accentedGlyph('A', 0x30, true),
        0x00C1 => accentedGlyph('A', 0x0C, true),
        0x00C2 => accentedGlyph('A', 0x38, true),
        0x00C3 => accentedGlyph('A', 0x76, true),
        0x00C4 => accentedGlyph('A', 0x66, true),
        0x00C5 => accentedGlyph('A', 0x38, true),
        0x00C6 => .{ 0x7E, 0xD8, 0xD8, 0xFE, 0xD8, 0xD8, 0xDE, 0x00 },
        0x00C7 => cedillaGlyph('C'),
        0x00C8 => accentedGlyph('E', 0x30, true),
        0x00C9 => accentedGlyph('E', 0x0C, true),
        0x00CA => accentedGlyph('E', 0x38, true),
        0x00CB => accentedGlyph('E', 0x66, true),
        0x00CC => accentedGlyph('I', 0x30, true),
        0x00CD => accentedGlyph('I', 0x0C, true),
        0x00CE => accentedGlyph('I', 0x38, true),
        0x00CF => accentedGlyph('I', 0x66, true),
        0x00D0 => .{ 0xF8, 0x6C, 0x66, 0xF6, 0x66, 0x6C, 0xF8, 0x00 },
        0x00D1 => accentedGlyph('N', 0x76, true),
        0x00D2 => accentedGlyph('O', 0x30, true),
        0x00D3 => accentedGlyph('O', 0x0C, true),
        0x00D4 => accentedGlyph('O', 0x38, true),
        0x00D5 => accentedGlyph('O', 0x76, true),
        0x00D6 => accentedGlyph('O', 0x66, true),
        0x00D7 => .{ 0x00, 0xC6, 0x6C, 0x38, 0x6C, 0xC6, 0x00, 0x00 },
        0x00D8 => .{ 0x7E, 0xCE, 0xDE, 0xF6, 0xE6, 0xC6, 0xFC, 0x00 },
        0x00D9 => accentedGlyph('U', 0x30, true),
        0x00DA => accentedGlyph('U', 0x0C, true),
        0x00DB => accentedGlyph('U', 0x38, true),
        0x00DC => accentedGlyph('U', 0x66, true),
        0x00DD => accentedGlyph('Y', 0x0C, true),
        0x00DE => .{ 0xF0, 0x60, 0x7C, 0x66, 0x7C, 0x60, 0xF0, 0x00 },
        0x00DF => .{ 0x78, 0xCC, 0xCC, 0xD8, 0xCC, 0xCC, 0xD8, 0xC0 },
        0x00E0 => accentedGlyph('a', 0x30, false),
        0x00E1 => accentedGlyph('a', 0x0C, false),
        0x00E2 => accentedGlyph('a', 0x38, false),
        0x00E3 => accentedGlyph('a', 0x76, false),
        0x00E4 => accentedGlyph('a', 0x66, false),
        0x00E5 => accentedGlyph('a', 0x38, false),
        0x00E6 => .{ 0x00, 0x00, 0xEC, 0x16, 0x7E, 0xD8, 0x76, 0x00 },
        0x00E7 => cedillaGlyph('c'),
        0x00E8 => accentedGlyph('e', 0x30, false),
        0x00E9 => accentedGlyph('e', 0x0C, false),
        0x00EA => accentedGlyph('e', 0x38, false),
        0x00EB => accentedGlyph('e', 0x66, false),
        0x00EC => accentedGlyph('i', 0x30, false),
        0x00ED => accentedGlyph('i', 0x0C, false),
        0x00EE => accentedGlyph('i', 0x38, false),
        0x00EF => accentedGlyph('i', 0x66, false),
        0x00F0 => .{ 0x30, 0x18, 0x7C, 0xCC, 0xCC, 0xCC, 0x78, 0x00 },
        0x00F1 => accentedGlyph('n', 0x76, false),
        0x00F2 => accentedGlyph('o', 0x30, false),
        0x00F3 => accentedGlyph('o', 0x0C, false),
        0x00F4 => accentedGlyph('o', 0x38, false),
        0x00F5 => accentedGlyph('o', 0x76, false),
        0x00F6 => accentedGlyph('o', 0x66, false),
        0x00F7 => .{ 0x00, 0x18, 0x00, 0x7E, 0x00, 0x18, 0x00, 0x00 },
        0x00F8 => .{ 0x00, 0x00, 0x7E, 0xDC, 0xF6, 0x66, 0xFC, 0x00 },
        0x00F9 => accentedGlyph('u', 0x30, false),
        0x00FA => accentedGlyph('u', 0x0C, false),
        0x00FB => accentedGlyph('u', 0x38, false),
        0x00FC => accentedGlyph('u', 0x66, false),
        0x00FD => accentedGlyph('y', 0x0C, false),
        0x00FE => .{ 0xE0, 0x60, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0xF0 },
        0x00FF => accentedGlyph('y', 0x66, false),
        else => builtin_ascii_font[unknown_index],
    };
}

fn accentedGlyph(base: u8, accent: u8, shift_base: bool) [8]u8 {
    const source = builtin_ascii_font[base - 0x20];
    var result: [8]u8 = .{0} ** 8;
    result[0] = accent;
    if (shift_base) {
        var row: usize = 1;
        while (row < result.len) : (row += 1) result[row] = source[row - 1];
    } else {
        var row: usize = 1;
        while (row < result.len) : (row += 1) result[row] = source[row];
    }
    return result;
}

fn cedillaGlyph(base: u8) [8]u8 {
    var result = builtin_ascii_font[base - 0x20];
    result[7] = 0x30;
    return result;
}

pub fn reset() void {
    active = null;
    active_font_id = BUILTIN_FONT_ID;
}

pub fn resetCatalog() void {
    catalog = .{CatalogEntry{}} ** MAX_CATALOG_FONTS;
    cached_fonts = .{CachedFont{}} ** MAX_CATALOG_FONTS;
    catalog_entries = 0;
    active = null;
    active_font_id = BUILTIN_FONT_ID;
}

pub fn glyphWidth() u32 {
    if (active) |f| return f.max_advance;
    return GLYPH_W;
}

pub fn glyphHeight() u32 {
    if (active) |f| return f.line_height;
    return GLYPH_H;
}

pub fn baseline() i16 {
    if (active) |f| return f.baseline;
    return GLYPH_H - 1;
}

pub fn catalogCount() usize {
    return catalog_entries;
}

pub fn catalogEntry(index: usize) ?CatalogEntry {
    if (index >= catalog_entries) return null;
    return catalog[index];
}

pub fn fontCount() usize {
    return 1 + catalog_entries;
}

pub fn fontIdForCatalogIndex(index: usize) u32 {
    if (index >= catalog_entries) return BUILTIN_FONT_ID;
    return @intCast(index + 1);
}

pub fn currentFontId() u32 {
    if (isRenderableFontId(active_font_id)) return active_font_id;
    return BUILTIN_FONT_ID;
}

pub fn isRenderableFontId(font_id: u32) bool {
    if (font_id == BUILTIN_FONT_ID) return true;
    const index = catalogIndexForFontId(font_id) orelse return false;
    return catalog[index].renderable and cached_fonts[index].used;
}

pub fn normalizeFontId(font_id: u32) u32 {
    return if (isRenderableFontId(font_id)) font_id else BUILTIN_FONT_ID;
}

pub fn catalogEntryForFontId(font_id: u32) ?CatalogEntry {
    const index = catalogIndexForFontId(font_id) orelse return null;
    return catalog[index];
}

pub fn glyphRow(c: u8, row: u32) u8 {
    if (active) |f| {
        if (row >= f.height) return 0;
        if (activeGlyphId(c)) |glyph_id| {
            return active_glyphs[glyph_id][row][0];
        }
    }
    if (row >= GLYPH_H) return 0;
    return glyph(c)[row];
}

pub fn glyphPixel(c: u8, row: u32, col: u32) bool {
    if (active) |f| {
        if (row >= f.height or col >= MAX_GLYPH_W) return false;
        if (activeGlyphId(c)) |glyph_id| {
            if (col >= active_glyph_widths[glyph_id]) return false;
            const byte_index: usize = @intCast(col / 8);
            if (byte_index >= f.bytes_per_row or byte_index >= MAX_BYTES_PER_ROW) return false;
            const shift: u3 = @intCast(7 - (col & 7));
            return ((active_glyphs[glyph_id][row][byte_index] >> shift) & 1) == 1;
        }
    }
    if (row >= GLYPH_H or col >= GLYPH_W) return false;
    const shift: u3 = @intCast(GLYPH_W - 1 - col);
    return ((glyph(c)[row] >> shift) & 1) == 1;
}

pub fn glyphBitmapWidth(c: u8) u32 {
    if (active != null) {
        if (activeGlyphId(c)) |glyph_id| return active_glyph_widths[glyph_id];
    }
    return GLYPH_W;
}

pub fn glyphAdvance(c: u8) u32 {
    if (active != null) {
        if (activeGlyphId(c)) |glyph_id| return active_glyph_advances[glyph_id];
    }
    return GLYPH_W;
}

pub fn glyphHeightForFont(font_id: u32) u32 {
    if (cachedFont(font_id)) |f| return f.line_height;
    return GLYPH_H;
}

pub fn glyphAdvanceForFont(font_id: u32, codepoint: u32) u32 {
    if (cachedFont(font_id)) |f| {
        if (cachedGlyphId(f, codepoint)) |glyph_id| return f.advances[glyph_id];
    }
    return GLYPH_W;
}

pub fn glyphBitmapWidthForFont(font_id: u32, codepoint: u32) u32 {
    if (cachedFont(font_id)) |f| {
        if (cachedGlyphId(f, codepoint)) |glyph_id| return f.widths[glyph_id];
    }
    return GLYPH_W;
}

pub fn glyphPixelForFont(font_id: u32, codepoint: u32, row: u32, col: u32) bool {
    if (cachedFont(font_id)) |f| {
        if (row >= f.height or col >= MAX_GLYPH_W) return false;
        if (cachedGlyphId(f, codepoint)) |glyph_id| {
            if (col >= f.widths[glyph_id]) return false;
            const byte_index: usize = @intCast(col / 8);
            if (byte_index >= f.bytes_per_row or byte_index >= MAX_BYTES_PER_ROW) return false;
            const shift: u3 = @intCast(7 - (col & 7));
            return ((f.glyphs[glyph_id][row][byte_index] >> shift) & 1) == 1;
        }
    }
    if (row >= GLYPH_H or col >= GLYPH_W) return false;
    const shift: u3 = @intCast(GLYPH_W - 1 - col);
    return ((builtinGlyphForCodepoint(codepoint)[row] >> shift) & 1) == 1;
}

/// Returns one rendered row as a least-significant-bit-first mask.  The
/// bounded codepoint index is resolved exactly once for the complete row.
pub fn glyphRowMaskForFont(font_id: u32, codepoint: u32, row: u32) u64 {
    if (cachedFont(font_id)) |f| {
        if (row >= f.height) return 0;
        if (cachedGlyphId(f, codepoint)) |glyph_id| return cachedGlyphRowMask(f, glyph_id, row);
    }
    return builtinGlyphRowMask(codepoint, row);
}

/// Materializes every rendered row after one bounded index lookup.  Missing
/// codepoints retain the historical builtin fallback and the selected font's
/// line metrics; no persistent R4F bytes leave the kernel cache.
pub fn glyphBitmapForFont(font_id: u32, codepoint: u32) GlyphBitmap {
    if (cachedFont(font_id)) |f| {
        var result = GlyphBitmap{
            .height = f.height,
            .line_height = f.line_height,
            .baseline = f.baseline,
        };
        if (cachedGlyphId(f, codepoint)) |glyph_id| {
            result.width = f.widths[glyph_id];
            result.advance = f.advances[glyph_id];
            var row: u32 = 0;
            while (row < f.height) : (row += 1) result.rows[row] = cachedGlyphRowMask(f, glyph_id, row);
            return result;
        }
        result.fallback = true;
        var row: u32 = 0;
        while (row < @min(@as(u32, f.height), @as(u32, GLYPH_H))) : (row += 1) {
            result.rows[row] = builtinGlyphRowMask(codepoint, row);
        }
        return result;
    }

    var result = GlyphBitmap{};
    var row: u32 = 0;
    while (row < GLYPH_H) : (row += 1) result.rows[row] = builtinGlyphRowMask(codepoint, row);
    return result;
}

fn cachedGlyphRowMask(cached: *const CachedFont, glyph_id: usize, row: u32) u64 {
    if (glyph_id >= MAX_GLYPHS or row >= cached.height) return 0;
    const width: u32 = @min(@as(u32, cached.widths[glyph_id]), @as(u32, MAX_GLYPH_W));
    var result: u64 = 0;
    var column: u32 = 0;
    while (column < width) : (column += 1) {
        const byte_index: usize = @intCast(column / 8);
        if (byte_index >= cached.bytes_per_row or byte_index >= MAX_BYTES_PER_ROW) break;
        const shift: u3 = @intCast(7 - (column & 7));
        if (((cached.glyphs[glyph_id][row][byte_index] >> shift) & 1) == 1) {
            result |= @as(u64, 1) << @intCast(column);
        }
    }
    return result;
}

fn builtinGlyphRowMask(codepoint: u32, row: u32) u64 {
    if (row >= GLYPH_H) return 0;
    const source = builtinGlyphForCodepoint(codepoint)[row];
    var result: u64 = 0;
    var column: u32 = 0;
    while (column < GLYPH_W) : (column += 1) {
        const shift: u3 = @intCast(GLYPH_W - 1 - column);
        if (((source >> shift) & 1) == 1) result |= @as(u64, 1) << @intCast(column);
    }
    return result;
}

pub fn measure(text: []const u8) TextMetrics {
    return measureWithFont(currentFontId(), text);
}

pub fn measureZ(text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
    var len: usize = 0;
    while (len < max_bytes and text_ptr[len] != 0) : (len += 1) {}
    return measure(text_ptr[0..len]);
}

pub fn measureWithFont(font_id: u32, text: []const u8) TextMetrics {
    var line_w: u32 = 0;
    var max_w: u32 = 0;
    var lines: u32 = 1;
    var visible: usize = 0;
    const safe_font_id = normalizeFontId(font_id);
    var cursor: usize = 0;
    while (cursor < text.len) {
        const decoded = decodeUtf8Scalar(text, cursor);
        cursor += decoded.consumed;
        if (decoded.codepoint == '\r') continue;
        if (decoded.codepoint == '\n') {
            if (line_w > max_w) max_w = line_w;
            line_w = 0;
            lines += 1;
            visible += decoded.consumed;
            continue;
        }
        line_w += glyphAdvanceForFont(safe_font_id, decoded.codepoint);
        visible += decoded.consumed;
    }
    if (line_w > max_w) max_w = line_w;
    const lh = glyphHeightForFont(safe_font_id);
    return .{
        .width = max_w,
        .height = lines * lh,
        .baseline = baselineForFont(safe_font_id),
        .line_height = lh,
        .visible_bytes = visible,
    };
}

pub fn measureZWithFont(font_id: u32, text_ptr: [*:0]const u8, max_bytes: usize) TextMetrics {
    var len: usize = 0;
    while (len < max_bytes and text_ptr[len] != 0) : (len += 1) {}
    return measureWithFont(font_id, text_ptr[0..len]);
}

pub fn baselineForFont(font_id: u32) i16 {
    if (cachedFont(font_id)) |f| return f.baseline;
    return GLYPH_H - 1;
}

pub fn maxAdvanceForFont(font_id: u32) u32 {
    if (cachedFont(font_id)) |f| return f.max_advance;
    return GLYPH_W;
}

pub fn loadR4F(bytes: []const u8) bool {
    return loadR4FNamed("", bytes);
}

pub fn loadR4FNamed(path: []const u8, bytes: []const u8) bool {
    const parsed = parseR4F(bytes) orelse return false;
    if (!parsed.renderable) return false;

    if (!fillCachedFont(parsed, &temp_cached_font)) return false;
    activateFont(&temp_cached_font, parsed.family, parsed.face, path);
    markSelected(path);
    active_font_id = fontIdForPath(path) orelse BUILTIN_FONT_ID;
    return true;
}

pub fn registerR4F(path: []const u8, bytes: []const u8) bool {
    const parsed_opt = parseR4F(bytes);
    var entry = CatalogEntry{};
    copyField(entry.path[0..], &entry.path_len, path);
    if (parsed_opt) |parsed| {
        entry.used = true;
        entry.kind = parsed.kind;
        entry.weight = parsed.weight;
        entry.style_flags = parsed.style_flags;
        entry.charset_flags = parsed.charset_flags;
        entry.face_count = parsed.face_count;
        entry.strike_count = parsed.strike_count;
        entry.glyph_count = parsed.glyph_count;
        entry.renderable = parsed.renderable;
        entry.width = parsed.width;
        entry.height = parsed.height;
        entry.max_advance = parsed.max_advance;
        entry.ascent = parsed.ascent;
        entry.descent = parsed.descent;
        entry.line_height = parsed.line_height;
        entry.baseline = parsed.baseline;
        copyField(entry.family[0..], &entry.family_len, parsed.family);
        copyField(entry.face[0..], &entry.face_len, parsed.face);
        copyField(entry.style[0..], &entry.style_len, parsed.style);
        copyField(entry.status[0..], &entry.status_len, parsed.reason);
    } else {
        entry.used = true;
        entry.kind = .unknown;
        entry.renderable = false;
        copyField(entry.family[0..], &entry.family_len, "Invalid");
        copyField(entry.face[0..], &entry.face_len, "Invalid R4F");
        copyField(entry.style[0..], &entry.style_len, "Error");
        copyField(entry.status[0..], &entry.status_len, "invalid R4F1");
    }
    const slot = addCatalogEntry(entry) orelse return false;
    if (parsed_opt) |parsed| {
        if (parsed.renderable) {
            if (!fillCachedFont(parsed, &cached_fonts[slot])) {
                cached_fonts[slot] = .{};
                catalog[slot].renderable = false;
                copyField(catalog[slot].status[0..], &catalog[slot].status_len, "cache failed");
            }
        }
    }
    return catalog[slot].renderable;
}

fn parseR4F(bytes: []const u8) ?ParsedFont {
    if (bytes.len < r4f.HEADER_SIZE) return null;
    if (!eq(bytes[0..4], &r4f.MAGIC)) return null;
    const version = readLe16(bytes[4..6]);
    const header_size = readLe16(bytes[6..8]);
    const file_flags = readLe32(bytes[8..12]);
    const file_size = readLe32(bytes[12..16]);
    const table_dir_offset = readLe32(bytes[16..20]);
    const table_count = readLe16(bytes[20..22]);
    const face_count = readLe16(bytes[22..24]);
    const strike_count = readLe16(bytes[24..26]);
    const glyph_count = readLe32(bytes[28..32]);
    if (version != r4f.VERSION or header_size < r4f.HEADER_SIZE) return null;
    if (file_size < r4f.HEADER_SIZE or file_size > bytes.len) return null;
    if (table_dir_offset < header_size) return null;
    if (!rangeValid(file_size, table_dir_offset, @as(u32, table_count) * @as(u32, @intCast(r4f.TABLE_ENTRY_SIZE)))) return null;

    const names = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_NAME) orelse &[_]u8{};
    const face_table = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_FACE) orelse &[_]u8{};
    const strike_table = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_STRIKE) orelse &[_]u8{};
    const map_table = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_GLYPH_MAP) orelse &[_]u8{};
    const metric_table = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_GLYPH_METRICS) orelse &[_]u8{};
    const bitmap_table = findTable(bytes, file_size, table_dir_offset, table_count, r4f.TABLE_BITMAP_DATA) orelse &[_]u8{};

    var parsed = ParsedFont{
        .valid = true,
        .face_count = face_count,
        .strike_count = strike_count,
        .glyph_count = glyph_count,
        .renderable = false,
        .reason = "no renderable strike",
        .strike_table = .{ .data = strike_table },
        .map_table = .{ .data = map_table },
        .metric_table = .{ .data = metric_table },
        .bitmap_table = .{ .data = bitmap_table },
    };

    if (face_table.len >= r4f.FACE_RECORD_SIZE) {
        parsed.kind = fontKind(readLe16(face_table[2..4]));
        parsed.style_flags = readLe32(face_table[4..8]);
        parsed.weight = readLe16(face_table[8..10]);
        parsed.charset_flags = readLe32(face_table[12..16]);
        const face_ascent = readLeI16(face_table[18..20]);
        const face_descent = readLeI16(face_table[20..22]);
        const face_line_height = readLeI16(face_table[28..30]);
        parsed.ascent = face_ascent;
        parsed.descent = face_descent;
        if (face_line_height > 0 and face_line_height <= MAX_GLYPH_H) parsed.line_height = @intCast(face_line_height);
        parsed.baseline = if (face_ascent > 0) face_ascent else parsed.baseline;
        parsed.family = nameAt(names, readLe32(face_table[30..34]), "R4OS");
        parsed.face = nameAt(names, readLe32(face_table[34..38]), parsed.family);
        parsed.style = nameAt(names, readLe32(face_table[38..42]), "Regular");
    } else if ((file_flags & r4f.FLAG_HAS_SFNT) != 0) {
        parsed.kind = .sfnt_truetype;
    } else if ((file_flags & r4f.FLAG_HAS_OUTLINE) != 0) {
        parsed.kind = .windows_vector;
    }

    if (strike_table.len >= r4f.STRIKE_RECORD_SIZE and map_table.len >= 4 and metric_table.len >= 4 and bitmap_table.len >= 8) {
        const strike_id = readLe16(strike_table[0..2]);
        const width_u16 = readLe16(strike_table[8..10]);
        const height_u16 = readLe16(strike_table[10..12]);
        const strike_ascent = readLeI16(strike_table[12..14]);
        const strike_descent = readLeI16(strike_table[14..16]);
        const strike_line_height = readLeI16(strike_table[16..18]);
        const strike_baseline = readLeI16(strike_table[18..20]);
        const bitmap_format = readLe16(strike_table[20..22]);
        const bytes_per_row = readLe16(strike_table[22..24]);
        const strike_glyph_count = readLe32(strike_table[24..28]);
        if (width_u16 > 0 and width_u16 <= MAX_GLYPH_W and height_u16 > 0 and height_u16 <= MAX_GLYPH_H and bytes_per_row > 0 and bytes_per_row <= MAX_BYTES_PER_ROW and bitmap_format == r4f.BITMAP_FORMAT_MONO1_MSB and strike_glyph_count > 0 and strike_glyph_count <= MAX_GLYPHS) {
            parsed.strike_id = strike_id;
            parsed.width = @intCast(width_u16);
            parsed.height = @intCast(height_u16);
            parsed.bytes_per_row = @intCast(bytes_per_row);
            parsed.glyph_count = strike_glyph_count;
            parsed.max_advance = @intCast(width_u16);
            parsed.ascent = if (strike_ascent > 0) strike_ascent else parsed.ascent;
            parsed.descent = strike_descent;
            if (strike_line_height > 0 and strike_line_height <= MAX_GLYPH_H) parsed.line_height = @intCast(strike_line_height) else parsed.line_height = @intCast(height_u16);
            parsed.baseline = if (strike_baseline > 0) strike_baseline else parsed.ascent;
            parsed.renderable = true;
            parsed.reason = "bitmap strike";
            parsed.kind = .bitmap;
            return parsed;
        }
        parsed.reason = "unsupported bitmap strike";
        return parsed;
    }

    if ((file_flags & r4f.FLAG_HAS_SFNT) != 0) {
        parsed.kind = .sfnt_truetype;
        parsed.reason = "sfnt catalog only";
    } else if ((file_flags & r4f.FLAG_HAS_OUTLINE) != 0) {
        parsed.kind = .windows_vector;
        parsed.reason = "outline catalog only";
    }
    return parsed;
}

fn eq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn readLe16(s: []const u8) u16 {
    return @as(u16, s[0]) | (@as(u16, s[1]) << 8);
}

fn readLeI16(s: []const u8) i16 {
    return @as(i16, @bitCast(readLe16(s)));
}

fn readLe32(s: []const u8) u32 {
    return @as(u32, s[0]) |
        (@as(u32, s[1]) << 8) |
        (@as(u32, s[2]) << 16) |
        (@as(u32, s[3]) << 24);
}

fn findTable(bytes: []const u8, file_size: u32, dir_offset: u32, table_count: u16, tag: u32) ?[]const u8 {
    var i: u16 = 0;
    while (i < table_count) : (i += 1) {
        const off: usize = @as(usize, dir_offset) + @as(usize, i) * r4f.TABLE_ENTRY_SIZE;
        const entry = bytes[off .. off + r4f.TABLE_ENTRY_SIZE];
        const entry_tag = readLe32(entry[0..4]);
        const table_offset = readLe32(entry[4..8]);
        const table_size = readLe32(entry[8..12]);
        if (entry_tag == tag) {
            if (!rangeValid(file_size, table_offset, table_size)) return null;
            return bytes[table_offset .. table_offset + table_size];
        }
    }
    return null;
}

fn loadBitmapGlyphs(table: []const u8, strike_id: u16, height: u8, bytes_per_row: u8, glyph_count: u32, out: *[MAX_GLYPHS][MAX_GLYPH_H][MAX_BYTES_PER_ROW]u8) bool {
    if (table.len < 8) return false;
    const record_count = readLe32(table[0..4]);
    const payload_start = readLe32(table[4..8]);
    if (record_count < glyph_count) return false;
    if (!rangeValid(@intCast(table.len), 8, record_count * @as(u32, @intCast(r4f.BITMAP_GLYPH_RECORD_SIZE)))) return false;
    if (payload_start > table.len) return false;
    var loaded: u32 = 0;
    var i: u32 = 0;
    while (i < record_count) : (i += 1) {
        const off: usize = 8 + @as(usize, i) * r4f.BITMAP_GLYPH_RECORD_SIZE;
        const glyph_id = readLe32(table[off .. off + 4]);
        const rec_strike_id = readLe16(table[off + 4 .. off + 6]);
        const format = readLe16(table[off + 6 .. off + 8]);
        const data_offset = readLe32(table[off + 8 .. off + 12]);
        const data_size = readLe32(table[off + 12 .. off + 16]);
        if (rec_strike_id != strike_id) continue;
        if (format != r4f.BITMAP_FORMAT_MONO1_MSB) return false;
        const needed = @as(u32, height) * @as(u32, bytes_per_row);
        if (glyph_id >= MAX_GLYPHS or data_size < needed) return false;
        if (data_offset < payload_start or !rangeValid(@intCast(table.len), data_offset, data_size)) return false;
        var row: u8 = 0;
        while (row < height) : (row += 1) {
            var b: u8 = 0;
            while (b < bytes_per_row) : (b += 1) {
                out[glyph_id][row][b] = table[data_offset + @as(u32, row) * bytes_per_row + b];
            }
        }
        loaded += 1;
    }
    return loaded >= glyph_count;
}

fn loadGlyphMetrics(table: []const u8, glyph_count: u32, fallback_width: u8, fallback_advance: u8, widths: *[MAX_GLYPHS]u8, advances: *[MAX_GLYPHS]u8) bool {
    if (table.len < 4) return false;
    const record_count = readLe32(table[0..4]);
    if (!rangeValid(@intCast(table.len), 4, record_count * @as(u32, @intCast(r4f.GLYPH_METRIC_RECORD_SIZE)))) return false;
    var i: u32 = 0;
    while (i < record_count) : (i += 1) {
        const off: usize = 4 + @as(usize, i) * r4f.GLYPH_METRIC_RECORD_SIZE;
        const glyph_id = readLe32(table[off .. off + 4]);
        if (glyph_id >= glyph_count or glyph_id >= MAX_GLYPHS) continue;
        const advance = readLeI16(table[off + 4 .. off + 6]);
        const x0 = readLeI16(table[off + 12 .. off + 14]);
        const x1 = readLeI16(table[off + 16 .. off + 18]);
        const width_i = x1 - x0;
        widths[glyph_id] = clampMetric(width_i, fallback_width);
        advances[glyph_id] = clampMetric(advance, fallback_advance);
    }
    return true;
}

fn loadGlyphMap(table: []const u8, glyph_count: u32, out: *[MAX_GLYPHS]u32) bool {
    if (table.len < 4) return false;
    const record_count = readLe32(table[0..4]);
    if (!rangeValid(@intCast(table.len), 4, record_count * @as(u32, @intCast(r4f.GLYPH_MAP_RECORD_SIZE)))) return false;
    var mapped: u32 = 0;
    var i: u32 = 0;
    while (i < record_count) : (i += 1) {
        const off: usize = 4 + @as(usize, i) * r4f.GLYPH_MAP_RECORD_SIZE;
        const codepoint = readLe32(table[off + 4 .. off + 8]);
        const glyph_id = readLe32(table[off + 8 .. off + 12]);
        if (codepoint <= 0x10FFFF and glyph_id < glyph_count and glyph_id < MAX_GLYPHS) {
            out[glyph_id] = codepoint;
            mapped += 1;
        }
    }
    return mapped > 0;
}

fn fillCachedFont(parsed: ParsedFont, cached: *CachedFont) bool {
    if (!parsed.renderable) return false;
    cached.* = CachedFont{
        .used = true,
        .width = parsed.width,
        .height = parsed.height,
        .bytes_per_row = parsed.bytes_per_row,
        .max_advance = parsed.max_advance,
        .ascent = parsed.ascent,
        .descent = parsed.descent,
        .line_height = parsed.line_height,
        .baseline = parsed.baseline,
        .widths = .{parsed.width} ** MAX_GLYPHS,
        .advances = .{parsed.max_advance} ** MAX_GLYPHS,
    };
    if (!loadGlyphMetrics(parsed.metric_table.data, parsed.glyph_count, parsed.width, parsed.max_advance, &cached.widths, &cached.advances)) return false;
    if (!loadBitmapGlyphs(parsed.bitmap_table.data, parsed.strike_id, parsed.height, parsed.bytes_per_row, parsed.glyph_count, &cached.glyphs)) return false;
    if (!loadGlyphMap(parsed.map_table.data, parsed.glyph_count, &cached.codepoints)) return false;
    return buildGlyphIndex(&cached.codepoints, &cached.index);
}

fn activateFont(cached: *const CachedFont, family: []const u8, face: []const u8, path: []const u8) void {
    active_glyphs = cached.glyphs;
    active_glyph_widths = cached.widths;
    active_glyph_advances = cached.advances;
    active_index = cached.index;
    var next_active = ActiveFont{
        .width = cached.width,
        .height = cached.height,
        .bytes_per_row = cached.bytes_per_row,
        .max_advance = cached.max_advance,
        .ascent = cached.ascent,
        .descent = cached.descent,
        .line_height = cached.line_height,
        .baseline = cached.baseline,
    };
    copyField(next_active.family[0..], &next_active.family_len, family);
    copyField(next_active.face[0..], &next_active.face_len, face);
    copyField(next_active.path[0..], &next_active.path_len, path);
    active = next_active;
}

fn rangeValid(total: u32, offset: u32, size: u32) bool {
    if (offset > total) return false;
    if (size > total - offset) return false;
    return true;
}

fn addCatalogEntry(entry: CatalogEntry) ?usize {
    if (catalog_entries >= catalog.len) return null;
    const slot = catalog_entries;
    catalog[catalog_entries] = entry;
    catalog_entries += 1;
    return slot;
}

fn markSelected(path: []const u8) void {
    var i: usize = 0;
    while (i < catalog_entries) : (i += 1) {
        catalog[i].selected = path.len != 0 and pathEquals(catalog[i].path[0..catalog[i].path_len], path);
    }
}

fn pathEquals(a: []const u8, b: []const u8) bool {
    return normalizedPathEquals(a, b) or normalizedPathEquals(stripDrive(a), stripDrive(b));
}

fn normalizedPathEquals(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        var ca = a[i];
        var cb = b[i];
        if (ca == '/') ca = '\\';
        if (cb == '/') cb = '\\';
        if (ca >= 'a' and ca <= 'z') ca -= 32;
        if (cb >= 'a' and cb <= 'z') cb -= 32;
        if (ca != cb) return false;
    }
    return true;
}

fn stripDrive(path: []const u8) []const u8 {
    if (path.len >= 2 and path[1] == ':') return path[2..];
    return path;
}

fn fontIdForPath(path: []const u8) ?u32 {
    if (path.len == 0) return null;
    var i: usize = 0;
    while (i < catalog_entries) : (i += 1) {
        if (pathEquals(catalog[i].path[0..catalog[i].path_len], path)) return fontIdForCatalogIndex(i);
    }
    return null;
}

fn catalogIndexForFontId(font_id: u32) ?usize {
    if (font_id == BUILTIN_FONT_ID) return null;
    const raw = font_id - 1;
    if (raw >= catalog_entries) return null;
    return @intCast(raw);
}

fn cachedFont(font_id: u32) ?*const CachedFont {
    const index = catalogIndexForFontId(font_id) orelse return null;
    if (!cached_fonts[index].used) return null;
    return &cached_fonts[index];
}

fn activeGlyphId(codepoint: u32) ?usize {
    return indexedGlyphId(&active_index, codepoint, null);
}

fn cachedGlyphId(cached: *const CachedFont, codepoint: u32) ?usize {
    return indexedGlyphId(&cached.index, codepoint, null);
}

fn buildGlyphIndex(codepoints: *const [MAX_GLYPHS]u32, out: *GlyphIndex) bool {
    out.* = .{};
    var glyph_id: usize = 0;
    while (glyph_id < codepoints.len) : (glyph_id += 1) {
        const codepoint = codepoints[glyph_id];
        if (codepoint == INVALID_CODEPOINT) continue;
        var insertion: usize = @intCast(out.count);
        while (insertion > 0) {
            const previous = insertion - 1;
            const previous_codepoint = out.codepoints[previous];
            const previous_glyph_id = out.glyph_ids[previous];
            if (previous_codepoint < codepoint or
                (previous_codepoint == codepoint and @as(usize, previous_glyph_id) <= glyph_id)) break;
            out.codepoints[insertion] = previous_codepoint;
            out.glyph_ids[insertion] = previous_glyph_id;
            insertion = previous;
        }
        out.codepoints[insertion] = codepoint;
        out.glyph_ids[insertion] = @intCast(glyph_id);
        out.count += 1;
    }
    return out.count != 0;
}

fn indexedGlyphId(index: *const GlyphIndex, codepoint: u32, probe_count: ?*u16) ?usize {
    var low: usize = 0;
    var high: usize = @intCast(index.count);
    while (low < high) {
        if (probe_count) |count| count.* +%= 1;
        const middle = low + (high - low) / 2;
        if (index.codepoints[middle] < codepoint) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if (low >= @as(usize, index.count) or index.codepoints[low] != codepoint) return null;
    return index.glyph_ids[low];
}

const DecodedScalar = struct {
    codepoint: u32,
    consumed: usize,
};

fn decodeUtf8Scalar(value: []const u8, start: usize) DecodedScalar {
    if (start >= value.len) return .{ .codepoint = 0xFFFD, .consumed = 0 };
    const first = value[start];
    if (first < 0x80) return .{ .codepoint = first, .consumed = 1 };
    const expected: usize = if (first >= 0xC2 and first <= 0xDF)
        2
    else if (first >= 0xE0 and first <= 0xEF)
        3
    else if (first >= 0xF0 and first <= 0xF4)
        4
    else
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    if (expected > value.len - start) return .{ .codepoint = 0xFFFD, .consumed = 1 };
    var codepoint: u32 = first & (@as(u8, 0x7F) >> @intCast(expected));
    var offset: usize = 1;
    while (offset < expected) : (offset += 1) {
        const byte = value[start + offset];
        if ((byte & 0xC0) != 0x80) return .{ .codepoint = 0xFFFD, .consumed = 1 };
        codepoint = (codepoint << 6) | (byte & 0x3F);
    }
    if ((expected == 3 and codepoint < 0x800) or
        (expected == 4 and codepoint < 0x10000) or
        codepoint > 0x10FFFF or
        (codepoint >= 0xD800 and codepoint <= 0xDFFF))
    {
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    }
    return .{ .codepoint = codepoint, .consumed = expected };
}

fn nameAt(names: []const u8, offset: u32, fallback: []const u8) []const u8 {
    if (offset >= names.len) return fallback;
    var end: usize = @intCast(offset);
    while (end < names.len and names[end] != 0) : (end += 1) {}
    if (end == offset) return fallback;
    return names[@intCast(offset)..end];
}

fn copyField(out: []u8, len: *usize, value: []const u8) void {
    const count = if (value.len < out.len) value.len else out.len;
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    if (count < out.len) out[count] = 0;
    len.* = count;
}

fn fontKind(value: u16) FontKind {
    return switch (value) {
        r4f.FONT_KIND_BITMAP => .bitmap,
        r4f.FONT_KIND_WINDOWS_VECTOR => .windows_vector,
        r4f.FONT_KIND_SFNT_TRUETYPE => .sfnt_truetype,
        else => .unknown,
    };
}

pub fn kindName(kind: FontKind) []const u8 {
    return switch (kind) {
        .builtin => "builtin",
        .bitmap => "bitmap",
        .windows_vector => "win-vector",
        .sfnt_truetype => "sfnt",
        .unknown => "unknown",
    };
}

fn clampMetric(value: i16, fallback: u8) u8 {
    if (value <= 0) return fallback;
    if (value > MAX_GLYPH_W) return MAX_GLYPH_W;
    return @intCast(value);
}

const unknown_index: usize = '?' - 0x20;

// zig fmt: off
pub const builtin_ascii_font = [_][8]u8{
    // 0x20 ' '
    .{ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 },
    // 0x21 '!'
    .{ 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00 },
    // 0x22 '"'
    .{ 0x6C, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00, 0x00 },
    // 0x23 '#'
    .{ 0x6C, 0x6C, 0xFE, 0x6C, 0xFE, 0x6C, 0x6C, 0x00 },
    // 0x24 '$'
    .{ 0x18, 0x7E, 0xC0, 0x7C, 0x06, 0xFC, 0x18, 0x00 },
    // 0x25 '%'
    .{ 0x00, 0xC6, 0xCC, 0x18, 0x30, 0x66, 0xC6, 0x00 },
    // 0x26 '&'
    .{ 0x38, 0x6C, 0x38, 0x76, 0xDC, 0xCC, 0x76, 0x00 },
    // 0x27 '''
    .{ 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00 },
    // 0x28 '('
    .{ 0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00 },
    // 0x29 ')'
    .{ 0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00 },
    // 0x2A '*'
    .{ 0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00 },
    // 0x2B '+'
    .{ 0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00 },
    // 0x2C ','
    .{ 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30 },
    // 0x2D '-'
    .{ 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00 },
    // 0x2E '.'
    .{ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00 },
    // 0x2F '/'
    .{ 0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0x80, 0x00 },
    // 0x30 '0'
    .{ 0x7C, 0xC6, 0xCE, 0xDE, 0xF6, 0xE6, 0x7C, 0x00 },
    // 0x31 '1'
    .{ 0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00 },
    // 0x32 '2'
    .{ 0x7C, 0xC6, 0x06, 0x1C, 0x30, 0x66, 0xFE, 0x00 },
    // 0x33 '3'
    .{ 0x7C, 0xC6, 0x06, 0x3C, 0x06, 0xC6, 0x7C, 0x00 },
    // 0x34 '4'
    .{ 0x0C, 0x1C, 0x3C, 0x6C, 0xFE, 0x0C, 0x0C, 0x00 },
    // 0x35 '5'
    .{ 0xFE, 0xC0, 0xC0, 0xFC, 0x06, 0xC6, 0x7C, 0x00 },
    // 0x36 '6'
    .{ 0x38, 0x60, 0xC0, 0xFC, 0xC6, 0xC6, 0x7C, 0x00 },
    // 0x37 '7'
    .{ 0xFE, 0xC6, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00 },
    // 0x38 '8'
    .{ 0x7C, 0xC6, 0xC6, 0x7C, 0xC6, 0xC6, 0x7C, 0x00 },
    // 0x39 '9'
    .{ 0x7C, 0xC6, 0xC6, 0x7E, 0x06, 0x0C, 0x78, 0x00 },
    // 0x3A ':'
    .{ 0x00, 0x18, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00 },
    // 0x3B ';'
    .{ 0x00, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30 },
    // 0x3C '<'
    .{ 0x06, 0x0C, 0x18, 0x30, 0x18, 0x0C, 0x06, 0x00 },
    // 0x3D '='
    .{ 0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00 },
    // 0x3E '>'
    .{ 0x60, 0x30, 0x18, 0x0C, 0x18, 0x30, 0x60, 0x00 },
    // 0x3F '?'
    .{ 0x7C, 0xC6, 0x0C, 0x18, 0x18, 0x00, 0x18, 0x00 },
    // 0x40 '@'
    .{ 0x7C, 0xC6, 0xDE, 0xDE, 0xDE, 0xC0, 0x7C, 0x00 },
    // 0x41 'A'
    .{ 0x38, 0x6C, 0xC6, 0xC6, 0xFE, 0xC6, 0xC6, 0x00 },
    // 0x42 'B'
    .{ 0xFC, 0x66, 0x66, 0x7C, 0x66, 0x66, 0xFC, 0x00 },
    // 0x43 'C'
    .{ 0x3C, 0x66, 0xC0, 0xC0, 0xC0, 0x66, 0x3C, 0x00 },
    // 0x44 'D'
    .{ 0xF8, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0xF8, 0x00 },
    // 0x45 'E'
    .{ 0xFE, 0x62, 0x68, 0x78, 0x68, 0x62, 0xFE, 0x00 },
    // 0x46 'F'
    .{ 0xFE, 0x62, 0x68, 0x78, 0x68, 0x60, 0xF0, 0x00 },
    // 0x47 'G'
    .{ 0x3C, 0x66, 0xC0, 0xC0, 0xCE, 0x66, 0x3E, 0x00 },
    // 0x48 'H'
    .{ 0xC6, 0xC6, 0xC6, 0xFE, 0xC6, 0xC6, 0xC6, 0x00 },
    // 0x49 'I'
    .{ 0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00 },
    // 0x4A 'J'
    .{ 0x1E, 0x0C, 0x0C, 0x0C, 0xCC, 0xCC, 0x78, 0x00 },
    // 0x4B 'K'
    .{ 0xE6, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0xE6, 0x00 },
    // 0x4C 'L'
    .{ 0xF0, 0x60, 0x60, 0x60, 0x62, 0x66, 0xFE, 0x00 },
    // 0x4D 'M'
    .{ 0xC6, 0xEE, 0xFE, 0xFE, 0xD6, 0xC6, 0xC6, 0x00 },
    // 0x4E 'N'
    .{ 0xC6, 0xE6, 0xF6, 0xDE, 0xCE, 0xC6, 0xC6, 0x00 },
    // 0x4F 'O'
    .{ 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00 },
    // 0x50 'P'
    .{ 0xFC, 0x66, 0x66, 0x7C, 0x60, 0x60, 0xF0, 0x00 },
    // 0x51 'Q'
    .{ 0x7C, 0xC6, 0xC6, 0xC6, 0xD6, 0x7C, 0x0E, 0x00 },
    // 0x52 'R'
    .{ 0xFC, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0xE6, 0x00 },
    // 0x53 'S'
    .{ 0x7C, 0xC6, 0xE0, 0x78, 0x0E, 0xC6, 0x7C, 0x00 },
    // 0x54 'T'
    .{ 0x7E, 0x7E, 0x5A, 0x18, 0x18, 0x18, 0x3C, 0x00 },
    // 0x55 'U'
    .{ 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00 },
    // 0x56 'V'
    .{ 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x6C, 0x38, 0x00 },
    // 0x57 'W'
    .{ 0xC6, 0xC6, 0xC6, 0xD6, 0xFE, 0xEE, 0xC6, 0x00 },
    // 0x58 'X'
    .{ 0xC6, 0xC6, 0x6C, 0x38, 0x6C, 0xC6, 0xC6, 0x00 },
    // 0x59 'Y'
    .{ 0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x3C, 0x00 },
    // 0x5A 'Z'
    .{ 0xFE, 0xC6, 0x8C, 0x18, 0x32, 0x66, 0xFE, 0x00 },
    // 0x5B '['
    .{ 0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00 },
    // 0x5C '\\'
    .{ 0xC0, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x02, 0x00 },
    // 0x5D ']'
    .{ 0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00 },
    // 0x5E '^'
    .{ 0x10, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00 },
    // 0x5F '_'
    .{ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF },
    // 0x60 '`'
    .{ 0x30, 0x30, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00 },
    // 0x61 'a'
    .{ 0x00, 0x00, 0x78, 0x0C, 0x7C, 0xCC, 0x76, 0x00 },
    // 0x62 'b'
    .{ 0xE0, 0x60, 0x7C, 0x66, 0x66, 0x66, 0xDC, 0x00 },
    // 0x63 'c'
    .{ 0x00, 0x00, 0x7C, 0xC6, 0xC0, 0xC6, 0x7C, 0x00 },
    // 0x64 'd'
    .{ 0x1C, 0x0C, 0x7C, 0xCC, 0xCC, 0xCC, 0x76, 0x00 },
    // 0x65 'e'
    .{ 0x00, 0x00, 0x7C, 0xC6, 0xFE, 0xC0, 0x7C, 0x00 },
    // 0x66 'f'
    .{ 0x3C, 0x66, 0x60, 0xF8, 0x60, 0x60, 0xF0, 0x00 },
    // 0x67 'g'
    .{ 0x00, 0x00, 0x76, 0xCC, 0xCC, 0x7C, 0x0C, 0xF8 },
    // 0x68 'h'
    .{ 0xE0, 0x60, 0x6C, 0x76, 0x66, 0x66, 0xE6, 0x00 },
    // 0x69 'i'
    .{ 0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00 },
    // 0x6A 'j'
    .{ 0x06, 0x00, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3C },
    // 0x6B 'k'
    .{ 0xE0, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0xE6, 0x00 },
    // 0x6C 'l'
    .{ 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00 },
    // 0x6D 'm'
    .{ 0x00, 0x00, 0xEC, 0xFE, 0xD6, 0xD6, 0xD6, 0x00 },
    // 0x6E 'n'
    .{ 0x00, 0x00, 0xDC, 0x66, 0x66, 0x66, 0x66, 0x00 },
    // 0x6F 'o'
    .{ 0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0x7C, 0x00 },
    // 0x70 'p'
    .{ 0x00, 0x00, 0xDC, 0x66, 0x66, 0x7C, 0x60, 0xF0 },
    // 0x71 'q'
    .{ 0x00, 0x00, 0x76, 0xCC, 0xCC, 0x7C, 0x0C, 0x1E },
    // 0x72 'r'
    .{ 0x00, 0x00, 0xDC, 0x76, 0x60, 0x60, 0xF0, 0x00 },
    // 0x73 's'
    .{ 0x00, 0x00, 0x7E, 0xC0, 0x7C, 0x06, 0xFC, 0x00 },
    // 0x74 't'
    .{ 0x30, 0x30, 0xFC, 0x30, 0x30, 0x36, 0x1C, 0x00 },
    // 0x75 'u'
    .{ 0x00, 0x00, 0xCC, 0xCC, 0xCC, 0xCC, 0x76, 0x00 },
    // 0x76 'v'
    .{ 0x00, 0x00, 0xC6, 0xC6, 0xC6, 0x6C, 0x38, 0x00 },
    // 0x77 'w'
    .{ 0x00, 0x00, 0xC6, 0xD6, 0xD6, 0xFE, 0x6C, 0x00 },
    // 0x78 'x'
    .{ 0x00, 0x00, 0xC6, 0x6C, 0x38, 0x6C, 0xC6, 0x00 },
    // 0x79 'y'
    .{ 0x00, 0x00, 0xC6, 0xC6, 0xC6, 0x7E, 0x06, 0xFC },
    // 0x7A 'z'
    .{ 0x00, 0x00, 0xFE, 0x4C, 0x18, 0x32, 0xFE, 0x00 },
    // 0x7B '{'
    .{ 0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00 },
    // 0x7C '|'
    .{ 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00 },
    // 0x7D '}'
    .{ 0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00 },
    // 0x7E '~'
    .{ 0x76, 0xDC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 },
};
// zig fmt: on

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

    catalog_entries = 1;
    catalog[0] = .{ .used = true, .renderable = true, .width = 5, .height = 3, .max_advance = 6, .line_height = 4, .baseline = 2 };
    cached_fonts[0] = .{
        .used = true,
        .width = 5,
        .height = 3,
        .bytes_per_row = 1,
        .max_advance = 6,
        .line_height = 4,
        .baseline = 2,
    };
    const cached = &cached_fonts[0];
    cached.codepoints[7] = 0x1F642;
    cached.widths[7] = 5;
    cached.advances[7] = 6;
    cached.glyphs[7][0][0] = 0b10101000;
    cached.glyphs[7][1][0] = 0b01010000;
    cached.glyphs[7][2][0] = 0b11111000;
    try std.testing.expect(buildGlyphIndex(&cached.codepoints, &cached.index));

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
