//! Typed temporary C cuts. Every state pointer retains its exact read lease
//! or unpublished writer and no buffer ownership is transferred by an address.
use super::{builtin, decode, owner, types::*};
use core::slice;
unsafe fn span<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if n == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, n) }
    }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_reset(state: *mut CatalogState) {
    unsafe { (*state).reset() };
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_reset_catalog(state: *mut CatalogState) {
    unsafe { (*state).reset_catalog() };
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_width(state: *const CatalogState) -> u32 {
    unsafe { (*state).glyph_width() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_height(state: *const CatalogState) -> u32 {
    unsafe { (*state).glyph_height() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_baseline(state: *const CatalogState) -> i16 {
    unsafe { (*state).baseline() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_catalog_count(state: *const CatalogState) -> usize {
    unsafe { (*state).catalog_count() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_catalog_entry(
    state: *const CatalogState,
    index: usize,
    out: *mut CatalogEntry,
) -> bool {
    let Some(value) = (unsafe { (*state).catalog_entry(index) }) else {
        return false;
    };
    unsafe { out.write(value) }
    true
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_font_count(state: *const CatalogState) -> usize {
    unsafe { (*state).font_count() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_font_id_for_catalog_index(
    state: *const CatalogState,
    index: usize,
) -> u32 {
    unsafe { (*state).font_id_for_catalog_index(index) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_current_font_id(state: *const CatalogState) -> u32 {
    unsafe { (*state).current_font_id() }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_is_renderable_font_id(
    state: *const CatalogState,
    font_id: u32,
) -> bool {
    unsafe { (*state).is_renderable(font_id) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_normalize_font_id(
    state: *const CatalogState,
    font_id: u32,
) -> u32 {
    unsafe { (*state).normalize_id(font_id) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_catalog_entry_for_font_id(
    state: *const CatalogState,
    font_id: u32,
    out: *mut CatalogEntry,
) -> bool {
    let Some(value) = (unsafe { (*state).catalog_entry_for_id(font_id) }) else {
        return false;
    };
    unsafe { out.write(value) }
    true
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_row(
    state: *const CatalogState,
    c: u8,
    row: u32,
) -> u8 {
    unsafe { (*state).glyph_row(c, row) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_pixel(
    state: *const CatalogState,
    c: u8,
    row: u32,
    col: u32,
) -> bool {
    unsafe { (*state).glyph_pixel(c, row, col) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_bitmap_width(
    state: *const CatalogState,
    c: u8,
) -> u32 {
    unsafe { (*state).glyph_bitmap_width(c) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_advance(
    state: *const CatalogState,
    c: u8,
) -> u32 {
    unsafe { (*state).glyph_advance(c) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_height_for_font(
    state: *const CatalogState,
    font_id: u32,
) -> u32 {
    unsafe { (*state).glyph_height_for_font(font_id) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_advance_for_font(
    state: *const CatalogState,
    font_id: u32,
    codepoint: u32,
) -> u32 {
    unsafe { (*state).glyph_advance_for_font(font_id, codepoint) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_bitmap_width_for_font(
    state: *const CatalogState,
    font_id: u32,
    codepoint: u32,
) -> u32 {
    unsafe { (*state).glyph_bitmap_width_for_font(font_id, codepoint) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_pixel_for_font(
    state: *const CatalogState,
    font_id: u32,
    codepoint: u32,
    row: u32,
    col: u32,
) -> bool {
    unsafe { (*state).glyph_pixel_for_font(font_id, codepoint, row, col) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_row_mask_for_font(
    state: *const CatalogState,
    font_id: u32,
    codepoint: u32,
    row: u32,
) -> u64 {
    unsafe { (*state).glyph_row_mask_for_font(font_id, codepoint, row) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_glyph_bitmap_for_font(
    state: *const CatalogState,
    font_id: u32,
    codepoint: u32,
    out: *mut GlyphBitmap,
) {
    let value = unsafe { (*state).glyph_bitmap_for_font(font_id, codepoint) };
    unsafe { out.write(value) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_measure(
    state: *const CatalogState,
    text: *const u8,
    text_len: usize,
    out: *mut TextMetrics,
) {
    let value = unsafe { (*state).measure_raw((*state).current_font_id(), text, text_len) };
    unsafe { out.write(value) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_measure_z(
    state: *const CatalogState,
    text_ptr: *const u8,
    max_bytes: usize,
    out: *mut TextMetrics,
) {
    let mut n = 0;
    while n < max_bytes && unsafe { text_ptr.add(n).read() } != 0 {
        n += 1;
    }
    let value = unsafe { (*state).measure_raw((*state).current_font_id(), text_ptr, n) };
    unsafe { out.write(value) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_measure_with_font(
    state: *const CatalogState,
    font_id: u32,
    text: *const u8,
    text_len: usize,
    out: *mut TextMetrics,
) {
    let value = unsafe { (*state).measure_raw(font_id, text, text_len) };
    unsafe { out.write(value) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_measure_z_with_font(
    state: *const CatalogState,
    font_id: u32,
    text_ptr: *const u8,
    max_bytes: usize,
    out: *mut TextMetrics,
) {
    let mut n = 0;
    while n < max_bytes && unsafe { text_ptr.add(n).read() } != 0 {
        n += 1;
    }
    let value = unsafe { (*state).measure_raw(font_id, text_ptr, n) };
    unsafe { out.write(value) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_baseline_for_font(
    state: *const CatalogState,
    font_id: u32,
) -> i16 {
    unsafe { (*state).baseline_for_font(font_id) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_max_advance_for_font(
    state: *const CatalogState,
    font_id: u32,
) -> u32 {
    unsafe { (*state).max_advance_for_font(font_id) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_load_r4_f(
    state: *mut CatalogState,
    bytes: *const u8,
    bytes_len: usize,
) -> bool {
    unsafe { (*state).load_named(&[], span(bytes, bytes_len)) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_load_r4_f_named(
    state: *mut CatalogState,
    path: *const u8,
    path_len: usize,
    bytes: *const u8,
    bytes_len: usize,
) -> bool {
    unsafe { (*state).load_named(span(path, path_len), span(bytes, bytes_len)) }
}

/// # Safety
/// State belongs to the exact live catalog lease/writer; input bytes stay valid
/// and outputs are exclusive/aligned until this synchronous operation finishes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_state_register_r4_f(
    state: *mut CatalogState,
    path: *const u8,
    path_len: usize,
    bytes: *const u8,
    bytes_len: usize,
) -> bool {
    unsafe { (*state).register(span(path, path_len), span(bytes, bytes_len)) }
}

/// # Safety
/// Caller owns an aligned output View and releases its exact lease once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_acquire(out: *mut owner::View) {
    unsafe { out.write(owner::acquire()) }
}
/// # Safety
/// View retains its exact bank lease until all state access has completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_view_state(
    view: *const owner::View,
) -> *const CatalogState {
    unsafe { (*view).state() }
}
/// # Safety
/// Caller owns the exact live View and performs no subsequent bank access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_view_release(view: *mut owner::View) {
    unsafe { (*view).release() }
}
/// # Safety
/// Exclusive aligned output retains the exact unpublished writer until abort/commit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_begin(
    preserve: bool,
    out: *mut owner::Replacement,
) -> bool {
    let Some(value) = owner::begin(preserve) else {
        return false;
    };
    unsafe { out.write(value) }
    true
}
/// # Safety
/// The exact replacement remains owned and unpublished during state mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_replacement_state(
    value: *const owner::Replacement,
) -> *mut CatalogState {
    unsafe { (*value).state() }
}
/// # Safety
/// Caller owns the exact replacement and stops mutating its bank before publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_replacement_commit(value: *mut owner::Replacement) {
    unsafe { (*value).commit() }
}
/// # Safety
/// Caller owns the exact replacement, with no remaining unpublished bank borrow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_replacement_abort(value: *mut owner::Replacement) {
    unsafe { (*value).abort() }
}
/// # Safety
/// Caller owns one writable eight-byte output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_builtin(cp: u32, out: *mut [u8; 8]) {
    unsafe { out.write(builtin::glyph(cp)) }
}
/// # Safety
/// Exact resident codepoint input and nonoverlapping exclusive index output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_build_index(
    cp: *const [u32; MAX_GLYPHS],
    out: *mut GlyphIndex,
) -> bool {
    unsafe { decode::build_index(&*cp, &mut *out) }
}
/// # Safety
/// Index is initialized and retained; optional probe output is exclusive/aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_indexed_id(
    index: *const GlyphIndex,
    cp: u32,
    probes: *mut u16,
) -> usize {
    unsafe { decode::indexed_id(&*index, cp, probes.as_mut()) }.unwrap_or(usize::MAX)
}
/// # Safety
/// Raw caller bytes retain len and output is exclusively writable to completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_decode(
    p: *const u8,
    len: usize,
    start: usize,
    out: *mut decode::DecodedScalar,
) {
    let value = unsafe { decode::decode_raw(p, len, start) };
    unsafe { out.write(value) }
}
/// # Safety
/// The output length is exclusive; returned text is immutable static storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_kind_name(kind: u16, len: *mut usize) -> *const u8 {
    let kind = match kind {
        0 => FontKind::Builtin,
        1 => FontKind::Bitmap,
        2 => FontKind::WindowsVector,
        3 => FontKind::SfntTruetype,
        0xffff => FontKind::Unknown,
        _ => panic!("Original font kind enum"),
    };
    let value = kind_name(kind);
    unsafe { len.write(value.len()) }
    value.as_ptr()
}

/// # Safety
/// Input retains at least the supplied readable span for the original LE read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_wire_u16(p: *const u8, len: usize) -> u16 {
    super::format::read_u16(unsafe { span(p, len) })
}
/// # Safety
/// Input retains at least the supplied readable span for the original LE read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_wire_i16(p: *const u8, len: usize) -> i16 {
    super::format::read_i16(unsafe { span(p, len) })
}
/// # Safety
/// Input retains at least the supplied readable span for the original LE read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_wire_u32(p: *const u8, len: usize) -> u32 {
    super::format::read_u32(unsafe { span(p, len) })
}
