//! Host checks compile the actual private decoder/types and read built assets.
use super::*;
use std::{boxed::Box, fs, path::PathBuf, vec::Vec};

fn assets() -> PathBuf {
    PathBuf::from(std::env::var_os("R4OS_FONT_ARTIFACTS").expect("built font artifacts"))
}
fn font(name: &str) -> Vec<u8> {
    fs::read(assets().join(name)).expect("real generated R4F")
}
fn table(bytes: &[u8], tag: u32) -> usize {
    let dir = f::read_u32(&bytes[16..]) as usize;
    let count = f::read_u16(&bytes[20..]) as usize;
    for i in 0..count {
        let off = dir + i * f::TABLE_ENTRY_SIZE;
        if f::read_u32(&bytes[off..]) == tag {
            return f::read_u32(&bytes[off + 4..]) as usize;
        }
    }
    panic!("required table absent");
}
fn cache(bytes: &[u8]) -> Box<CachedFont> {
    let parsed = parse(bytes).expect("valid asset");
    assert!(parsed.renderable);
    let mut out = Box::<CachedFont>::new_uninit();
    assert!(unsafe { fill_cached(parsed, out.as_mut_ptr()) });
    unsafe { out.assume_init() }
}

#[test]
fn unicode82_endash_coverage_and_missing_glyph_are_only_rendering_decisions() {
    static EMPTY: CatalogState = CatalogState::EMPTY;
    let stock = font("TERMINAL8.R4F");
    let capable = include_bytes!("../../Tests/Fixture/Unicode82/Z82EN.R4F");
    for (bytes, has_endash) in [(&stock[..], false), (&capable[..], true)] {
        let mut state = Box::<CatalogState>::new_uninit();
        unsafe { core::ptr::copy_nonoverlapping(&EMPTY, state.as_mut_ptr(), 1) };
        let mut state = unsafe { state.assume_init() };
        let mut storage = Box::<CachedFont>::new_uninit();
        state.cached_fonts = storage.as_mut_ptr();
        state.cached_fonts_len = 1;
        assert!(state.register(b"/R4OS/FONTS/UNICODE82.R4F", bytes));
        let cached = unsafe { storage.assume_init() };
        assert_eq!(
            indexed_id(&cached.index, 0x2013, None).is_some(),
            has_endash
        );
        assert!(indexed_id(&cached.index, u32::from(b'-'), None).is_some());
        assert!(indexed_id(&cached.index, 0xe4, None).is_some());
        let glyph = state.glyph_bitmap_for_font(1, 0x2013);
        assert_eq!(glyph.fallback, !has_endash);
        for row in 0..8 {
            let expected = if has_endash {
                if row == 3 { 0x7e } else { 0 }
            } else {
                super::super::builtin::row_mask(u32::from(b'?'), row)
            };
            assert_eq!(glyph.rows[row as usize], expected);
            for col in 0..8 {
                assert_eq!(
                    state.glyph_pixel_for_font(1, 0x2013, row, col),
                    expected & (1 << col) != 0
                );
            }
        }
        assert!(state.glyph_bitmap_for_font(1, 0x10fffd).fallback);
    }
}

#[test]
fn all_stock_faces_load_real_maps_and_bitmap_rows() {
    let mut count = 0;
    for file in fs::read_dir(assets()).unwrap() {
        let file = file.unwrap();
        if file.path().extension().and_then(|x| x.to_str()) != Some("R4F") {
            continue;
        }
        let bytes = fs::read(file.path()).unwrap();
        assert!(bytes.len() <= 128 * 1024);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.glyph_count, 257);
        let c = cache(&bytes);
        assert_eq!(c.index.count, 257);
        for &cp in &[
            0xfc, 0x2500, 0x2502, 0x253c, 0x2550, 0x2588, 0x2591, 0x20ac, 0xfffd, 0x263a, 0x2302,
            0x3a9, 0x20a7,
        ] {
            let id = indexed_id(&c.index, cp, None).expect("planned glyph is mapped");
            assert!(c.glyphs[id].iter().any(|row| row.iter().any(|&x| x != 0)));
        }
        assert_eq!(indexed_id(&c.index, 0x1f600, None), None);
        assert_eq!(indexed_id(&c.index, 1, None), None);
        assert_eq!(indexed_id(&c.index, 0x7f, None), None);
        for byte in 32u8..=126 {
            assert!(indexed_id(&c.index, u32::from(byte), None).is_some());
        }
        count += 1;
    }
    assert_eq!(count, 14);
    for (name, scale) in [("TERMINAL8.R4F", 1), ("TERMINAL16.R4F", 2)] {
        let c = cache(&font(name));
        let block = indexed_id(&c.index, 0x2588, None).unwrap();
        for y in 0..8 * scale {
            assert_eq!(c.glyphs[block][y][0], 0xff);
        }
        let euro = indexed_id(&c.index, 0x20ac, None).unwrap();
        for (y, row) in [0x1e, 0x30, 0x7c, 0x30, 0x7c, 0x30, 0x1e, 0]
            .iter()
            .enumerate()
        {
            for n in 0..scale {
                assert_eq!(c.glyphs[euro][y * scale + n][0], *row);
            }
        }
    }
}

#[test]
fn native_catalogue_samplers_use_all_planned_glyphs_without_builtin_fallback() {
    static EMPTY_STATE: CatalogState = CatalogState::EMPTY;
    for file in fs::read_dir(assets()).unwrap() {
        let file = file.unwrap();
        if file.path().extension().and_then(|x| x.to_str()) != Some("R4F") {
            continue;
        }
        let bytes = fs::read(file.path()).unwrap();
        let mut state = Box::<CatalogState>::new_uninit();
        unsafe { core::ptr::copy_nonoverlapping(&EMPTY_STATE, state.as_mut_ptr(), 1) };
        let mut state = unsafe { state.assume_init() };
        let mut storage = Box::<CachedFont>::new_uninit();
        state.cached_fonts = storage.as_mut_ptr();
        state.cached_fonts_len = 1;
        assert!(state.register(b"/R4OS/FONTS/TEST.R4F", &bytes));
        let cached = unsafe { storage.assume_init() };
        for &cp in &cached.index.codepoints[..usize::from(cached.index.count)] {
            let bitmap = state.glyph_bitmap_for_font(1, cp);
            assert!(!bitmap.fallback, "planned scalar {cp:x}");
            let id = indexed_id(&cached.index, cp, None).unwrap();
            for row in 0..bitmap.height {
                assert_eq!(
                    state.glyph_row_mask_for_font(1, cp, row),
                    bitmap.rows[row as usize]
                );
                for col in 0..bitmap.width {
                    let expected = cached.glyphs[id][row as usize][col as usize / 8]
                        & (0x80 >> (col & 7))
                        != 0;
                    assert_eq!(state.glyph_pixel_for_font(1, cp, row, col), expected);
                    assert_eq!(bitmap.rows[row as usize] & (1 << col) != 0, expected);
                }
            }
        }
        assert!(state.glyph_bitmap_for_font(1, 0x1f600).fallback);
    }
}

#[test]
fn invalid_scalars_metadata_and_conflicting_glyph_ids_are_rejected() {
    let original = font("TERMINAL16.R4F");
    let gm = table(&original, f::TABLE_GLYPH_MAP);
    let face = table(&original, f::TABLE_FACE);
    for bad in [0xd800u32, 0xdfff, 0x110000, 0xffffffff] {
        let mut bytes = original.clone();
        bytes[gm + 8..gm + 12].copy_from_slice(&bad.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }
    for (offset, value) in [(gm + 6, 850u16), (gm + 4, 1)] {
        let mut bytes = original.clone();
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }
    for (offset, value) in [
        (gm + 16, 1u32),
        (gm, u32::MAX),
        (face + 12, 0),
        (face + 12, 12),
    ] {
        let mut bytes = original.clone();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }
    let mut bytes = original.clone();
    bytes[gm + 28..gm + 32].copy_from_slice(&0u32.to_le_bytes());
    assert!(parse(&bytes).is_none());
    let mut bytes = original.clone();
    let bitmap = table(&bytes, f::TABLE_BITMAP_DATA);
    bytes[bitmap + 24..bitmap + 28].copy_from_slice(&0u32.to_le_bytes());
    let mut out = Box::<CachedFont>::new_uninit();
    assert!(!unsafe { fill_cached(parse(&bytes).unwrap(), out.as_mut_ptr()) });
}

#[test]
fn duplicate_scalars_keep_the_lowest_glyph_id_and_legacy_tags_are_provenance() {
    let mut bytes = font("TERMINAL16.R4F");
    let gm = table(&bytes, f::TABLE_GLYPH_MAP);
    let face = table(&bytes, f::TABLE_FACE);
    bytes[face + 12..face + 16].copy_from_slice(&5u32.to_le_bytes());
    bytes[gm + 6..gm + 8].copy_from_slice(&437u16.to_le_bytes());
    bytes[gm + 8..gm + 12].copy_from_slice(&0x2500u32.to_le_bytes());
    bytes[gm + 24..gm + 28].copy_from_slice(&0x2500u32.to_le_bytes());
    let c = cache(&bytes);
    assert_eq!(indexed_id(&c.index, 0x2500, None), Some(0));
    assert_eq!(indexed_id(&c.index, 0xc4, None).is_some(), true);
    bytes[face + 12..face + 16].copy_from_slice(&4u32.to_le_bytes());
    assert!(parse(&bytes).is_none());
}
