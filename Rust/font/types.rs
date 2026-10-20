//! Fixed private font records; published cache bytes remain at the font owner.
use core::{mem, ptr};
pub const GLYPH_W: u8 = 8;
pub const GLYPH_H: u8 = 8;
pub const MAX_GLYPH_W: u8 = 40;
pub const MAX_GLYPH_H: usize = 40;
pub const MAX_GLYPHS: usize = 512;
pub const MAX_BYTES_PER_ROW: usize = 5;
pub const MAX_CATALOG_FONTS: usize = 64;
pub const MAX_FONT_PATH: usize = 96;
pub const MAX_FONT_NAME: usize = 40;
pub const MAX_STATUS_TEXT: usize = 48;
pub const BUILTIN_FONT_ID: u32 = 0;
pub const INVALID_CODEPOINT: u32 = u32::MAX;
pub type Glyphs = [[[u8; MAX_BYTES_PER_ROW]; MAX_GLYPH_H]; MAX_GLYPHS];
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Builtin = 0,
    Bitmap = 1,
    WindowsVector = 2,
    SfntTruetype = 3,
    Unknown = 0xffff,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GlyphIndex {
    pub count: u16,
    pub codepoints: [u32; MAX_GLYPHS],
    pub glyph_ids: [u16; MAX_GLYPHS],
}
impl GlyphIndex {
    pub const EMPTY: Self = Self {
        count: 0,
        codepoints: [INVALID_CODEPOINT; MAX_GLYPHS],
        glyph_ids: [0; MAX_GLYPHS],
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub advance: u32,
    pub line_height: u32,
    pub baseline: i16,
    pub fallback: bool,
    pub rows: [u64; MAX_GLYPH_H],
}
impl GlyphBitmap {
    pub const EMPTY: Self = Self {
        width: 8,
        height: 8,
        advance: 8,
        line_height: 8,
        baseline: 7,
        fallback: false,
        rows: [0; MAX_GLYPH_H],
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TextMetrics {
    pub width: u32,
    pub height: u32,
    pub baseline: i16,
    pub line_height: u32,
    pub visible_bytes: usize,
    pub clipped: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CatalogEntry {
    pub used: bool,
    pub path: [u8; MAX_FONT_PATH],
    pub path_len: usize,
    pub family: [u8; MAX_FONT_NAME],
    pub family_len: usize,
    pub face: [u8; MAX_FONT_NAME],
    pub face_len: usize,
    pub style: [u8; MAX_FONT_NAME],
    pub style_len: usize,
    pub kind: FontKind,
    pub weight: u16,
    pub style_flags: u32,
    pub charset_flags: u32,
    pub face_count: u16,
    pub strike_count: u16,
    pub glyph_count: u32,
    pub renderable: bool,
    pub selected: bool,
    pub width: u8,
    pub height: u8,
    pub max_advance: u8,
    pub ascent: i16,
    pub descent: i16,
    pub line_height: u8,
    pub baseline: i16,
    pub status: [u8; MAX_STATUS_TEXT],
    pub status_len: usize,
}
impl CatalogEntry {
    pub const EMPTY: Self = Self {
        used: false,
        path: [0; MAX_FONT_PATH],
        path_len: 0,
        family: [0; MAX_FONT_NAME],
        family_len: 0,
        face: [0; MAX_FONT_NAME],
        face_len: 0,
        style: [0; MAX_FONT_NAME],
        style_len: 0,
        kind: FontKind::Unknown,
        weight: 0,
        style_flags: 0,
        charset_flags: 0,
        face_count: 0,
        strike_count: 0,
        glyph_count: 0,
        renderable: false,
        selected: false,
        width: 8,
        height: 8,
        max_advance: 8,
        ascent: 7,
        descent: 1,
        line_height: 8,
        baseline: 7,
        status: [0; MAX_STATUS_TEXT],
        status_len: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActiveFont {
    pub width: u8,
    pub height: u8,
    pub bytes_per_row: u8,
    pub max_advance: u8,
    pub ascent: i16,
    pub descent: i16,
    pub line_height: u8,
    pub baseline: i16,
    pub family: [u8; MAX_FONT_NAME],
    pub family_len: usize,
    pub face: [u8; MAX_FONT_NAME],
    pub face_len: usize,
    pub path: [u8; MAX_FONT_PATH],
    pub path_len: usize,
}
impl ActiveFont {
    pub const EMPTY: Self = Self {
        width: 0,
        height: 0,
        bytes_per_row: 0,
        max_advance: 0,
        ascent: 0,
        descent: 0,
        line_height: 0,
        baseline: 0,
        family: [0; MAX_FONT_NAME],
        family_len: 0,
        face: [0; MAX_FONT_NAME],
        face_len: 0,
        path: [0; MAX_FONT_PATH],
        path_len: 0,
    };
}
#[repr(C)]
pub struct CachedFont {
    pub used: bool,
    pub width: u8,
    pub height: u8,
    pub bytes_per_row: u8,
    pub max_advance: u8,
    pub ascent: i16,
    pub descent: i16,
    pub line_height: u8,
    pub baseline: i16,
    pub glyphs: Glyphs,
    pub widths: [u8; MAX_GLYPHS],
    pub advances: [u8; MAX_GLYPHS],
    pub codepoints: [u32; MAX_GLYPHS],
    pub index: GlyphIndex,
}
impl CachedFont {
    pub const EMPTY: Self = Self {
        used: false,
        width: 8,
        height: 8,
        bytes_per_row: 1,
        max_advance: 8,
        ascent: 7,
        descent: 1,
        line_height: 8,
        baseline: 7,
        glyphs: [[[0; MAX_BYTES_PER_ROW]; MAX_GLYPH_H]; MAX_GLYPHS],
        widths: [8; MAX_GLYPHS],
        advances: [8; MAX_GLYPHS],
        codepoints: [INVALID_CODEPOINT; MAX_GLYPHS],
        index: GlyphIndex::EMPTY,
    };
}
/// Initialize resident cache storage in place, without a106KB stack temporary.
/// # Safety
/// Caller owns resident aligned cache storage of the complete CachedFont size; it does not alias the immutable initialization template.
pub unsafe fn reset_cached(out: *mut CachedFont) {
    unsafe { ptr::copy_nonoverlapping(ptr::addr_of!(EMPTY_CACHE), out, 1) }
}
static EMPTY_CACHE: CachedFont = CachedFont::EMPTY;
pub fn copy_field(out: &mut [u8], value: &[u8]) -> usize {
    let count = value.len().min(out.len());
    out[..count].copy_from_slice(&value[..count]);
    if count < out.len() {
        out[count] = 0;
    }
    count
}
pub fn kind_name(kind: FontKind) -> &'static [u8] {
    match kind {
        FontKind::Builtin => b"builtin",
        FontKind::Bitmap => b"bitmap",
        FontKind::WindowsVector => b"win-vector",
        FontKind::SfntTruetype => b"sfnt",
        FontKind::Unknown => b"unknown",
    }
}
#[repr(C)]
pub struct CatalogState {
    pub revision: u32,
    pub catalog: [CatalogEntry; MAX_CATALOG_FONTS],
    pub catalog_entries: usize,
    pub cached_fonts: *mut CachedFont,
    pub cached_fonts_len: usize,
    pub temp_cached_font: CachedFont,
    pub active_present: bool,
    pub active: ActiveFont,
    pub active_font_id: u32,
    pub active_glyphs: Glyphs,
    pub active_glyph_widths: [u8; MAX_GLYPHS],
    pub active_glyph_advances: [u8; MAX_GLYPHS],
    pub active_index: GlyphIndex,
}
impl CatalogState {
    pub const EMPTY: Self = Self {
        revision: 1,
        catalog: [CatalogEntry::EMPTY; MAX_CATALOG_FONTS],
        catalog_entries: 0,
        cached_fonts: ptr::NonNull::<CachedFont>::dangling().as_ptr(),
        cached_fonts_len: 0,
        temp_cached_font: CachedFont::EMPTY,
        active_present: false,
        active: ActiveFont::EMPTY,
        active_font_id: 0,
        active_glyphs: [[[0; MAX_BYTES_PER_ROW]; MAX_GLYPH_H]; MAX_GLYPHS],
        active_glyph_widths: [8; MAX_GLYPHS],
        active_glyph_advances: [8; MAX_GLYPHS],
        active_index: GlyphIndex::EMPTY,
    };
}
// SAFETY: the empty constant is never mutated. Nonempty states require an exact
// catalog read lease or exclusive unpublished writer at every API boundary.
unsafe impl Sync for CatalogState {}
pub const CATALOG_BANK_BYTES: usize =
    mem::size_of::<CatalogState>() + mem::size_of::<[CachedFont; MAX_CATALOG_FONTS]>();

const _: () = {
    assert!(core::mem::size_of::<GlyphIndex>() == 3076);
    assert!(core::mem::align_of::<GlyphIndex>() == 4);
    assert!(core::mem::offset_of!(GlyphIndex, count) == 0);
    assert!(core::mem::offset_of!(GlyphIndex, codepoints) == 4);
    assert!(core::mem::offset_of!(GlyphIndex, glyph_ids) == 2052);
};
const _: () = {
    assert!(core::mem::size_of::<GlyphBitmap>() == 344);
    assert!(core::mem::align_of::<GlyphBitmap>() == 8);
    assert!(core::mem::offset_of!(GlyphBitmap, width) == 0);
    assert!(core::mem::offset_of!(GlyphBitmap, height) == 4);
    assert!(core::mem::offset_of!(GlyphBitmap, advance) == 8);
    assert!(core::mem::offset_of!(GlyphBitmap, line_height) == 12);
    assert!(core::mem::offset_of!(GlyphBitmap, baseline) == 16);
    assert!(core::mem::offset_of!(GlyphBitmap, fallback) == 18);
    assert!(core::mem::offset_of!(GlyphBitmap, rows) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<TextMetrics>() == 32);
    assert!(core::mem::align_of::<TextMetrics>() == 8);
    assert!(core::mem::offset_of!(TextMetrics, width) == 0);
    assert!(core::mem::offset_of!(TextMetrics, height) == 4);
    assert!(core::mem::offset_of!(TextMetrics, baseline) == 8);
    assert!(core::mem::offset_of!(TextMetrics, line_height) == 12);
    assert!(core::mem::offset_of!(TextMetrics, visible_bytes) == 16);
    assert!(core::mem::offset_of!(TextMetrics, clipped) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<CatalogEntry>() == 352);
    assert!(core::mem::align_of::<CatalogEntry>() == 8);
    assert!(core::mem::offset_of!(CatalogEntry, used) == 0);
    assert!(core::mem::offset_of!(CatalogEntry, path) == 1);
    assert!(core::mem::offset_of!(CatalogEntry, path_len) == 104);
    assert!(core::mem::offset_of!(CatalogEntry, family) == 112);
    assert!(core::mem::offset_of!(CatalogEntry, family_len) == 152);
    assert!(core::mem::offset_of!(CatalogEntry, face) == 160);
    assert!(core::mem::offset_of!(CatalogEntry, face_len) == 200);
    assert!(core::mem::offset_of!(CatalogEntry, style) == 208);
    assert!(core::mem::offset_of!(CatalogEntry, style_len) == 248);
    assert!(core::mem::offset_of!(CatalogEntry, kind) == 256);
    assert!(core::mem::offset_of!(CatalogEntry, weight) == 258);
    assert!(core::mem::offset_of!(CatalogEntry, style_flags) == 260);
    assert!(core::mem::offset_of!(CatalogEntry, charset_flags) == 264);
    assert!(core::mem::offset_of!(CatalogEntry, face_count) == 268);
    assert!(core::mem::offset_of!(CatalogEntry, strike_count) == 270);
    assert!(core::mem::offset_of!(CatalogEntry, glyph_count) == 272);
    assert!(core::mem::offset_of!(CatalogEntry, renderable) == 276);
    assert!(core::mem::offset_of!(CatalogEntry, selected) == 277);
    assert!(core::mem::offset_of!(CatalogEntry, width) == 278);
    assert!(core::mem::offset_of!(CatalogEntry, height) == 279);
    assert!(core::mem::offset_of!(CatalogEntry, max_advance) == 280);
    assert!(core::mem::offset_of!(CatalogEntry, ascent) == 282);
    assert!(core::mem::offset_of!(CatalogEntry, descent) == 284);
    assert!(core::mem::offset_of!(CatalogEntry, line_height) == 286);
    assert!(core::mem::offset_of!(CatalogEntry, baseline) == 288);
    assert!(core::mem::offset_of!(CatalogEntry, status) == 290);
    assert!(core::mem::offset_of!(CatalogEntry, status_len) == 344);
};
const _: () = {
    assert!(core::mem::size_of::<ActiveFont>() == 216);
    assert!(core::mem::align_of::<ActiveFont>() == 8);
    assert!(core::mem::offset_of!(ActiveFont, width) == 0);
    assert!(core::mem::offset_of!(ActiveFont, height) == 1);
    assert!(core::mem::offset_of!(ActiveFont, bytes_per_row) == 2);
    assert!(core::mem::offset_of!(ActiveFont, max_advance) == 3);
    assert!(core::mem::offset_of!(ActiveFont, ascent) == 4);
    assert!(core::mem::offset_of!(ActiveFont, descent) == 6);
    assert!(core::mem::offset_of!(ActiveFont, line_height) == 8);
    assert!(core::mem::offset_of!(ActiveFont, baseline) == 10);
    assert!(core::mem::offset_of!(ActiveFont, family) == 12);
    assert!(core::mem::offset_of!(ActiveFont, family_len) == 56);
    assert!(core::mem::offset_of!(ActiveFont, face) == 64);
    assert!(core::mem::offset_of!(ActiveFont, face_len) == 104);
    assert!(core::mem::offset_of!(ActiveFont, path) == 112);
    assert!(core::mem::offset_of!(ActiveFont, path_len) == 208);
};
const _: () = {
    assert!(core::mem::size_of::<CachedFont>() == 108564);
    assert!(core::mem::align_of::<CachedFont>() == 4);
    assert!(core::mem::offset_of!(CachedFont, used) == 0);
    assert!(core::mem::offset_of!(CachedFont, width) == 1);
    assert!(core::mem::offset_of!(CachedFont, height) == 2);
    assert!(core::mem::offset_of!(CachedFont, bytes_per_row) == 3);
    assert!(core::mem::offset_of!(CachedFont, max_advance) == 4);
    assert!(core::mem::offset_of!(CachedFont, ascent) == 6);
    assert!(core::mem::offset_of!(CachedFont, descent) == 8);
    assert!(core::mem::offset_of!(CachedFont, line_height) == 10);
    assert!(core::mem::offset_of!(CachedFont, baseline) == 12);
    assert!(core::mem::offset_of!(CachedFont, glyphs) == 14);
    assert!(core::mem::offset_of!(CachedFont, widths) == 102414);
    assert!(core::mem::offset_of!(CachedFont, advances) == 102926);
    assert!(core::mem::offset_of!(CachedFont, codepoints) == 103440);
    assert!(core::mem::offset_of!(CachedFont, index) == 105488);
};
const _: () = {
    assert!(core::mem::size_of::<CatalogState>() == 237848);
    assert!(core::mem::align_of::<CatalogState>() == 8);
    assert!(core::mem::offset_of!(CatalogState, revision) == 0);
    assert!(core::mem::offset_of!(CatalogState, catalog) == 8);
    assert!(core::mem::offset_of!(CatalogState, catalog_entries) == 22536);
    assert!(core::mem::offset_of!(CatalogState, cached_fonts) == 22544);
    assert!(core::mem::offset_of!(CatalogState, cached_fonts_len) == 22552);
    assert!(core::mem::offset_of!(CatalogState, temp_cached_font) == 22560);
    assert!(core::mem::offset_of!(CatalogState, active_present) == 131124);
    assert!(core::mem::offset_of!(CatalogState, active) == 131128);
    assert!(core::mem::offset_of!(CatalogState, active_font_id) == 131344);
    assert!(core::mem::offset_of!(CatalogState, active_glyphs) == 131348);
    assert!(core::mem::offset_of!(CatalogState, active_glyph_widths) == 233748);
    assert!(core::mem::offset_of!(CatalogState, active_glyph_advances) == 234260);
    assert!(core::mem::offset_of!(CatalogState, active_index) == 234772);
};
