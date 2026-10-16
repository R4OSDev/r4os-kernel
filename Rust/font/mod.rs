pub mod builtin;
pub mod catalog_io;
pub mod decode;
pub mod ffi;
pub mod format;
pub mod owner;
pub mod state;
pub mod types;

// Native early rendering shares the exact catalogue read lease and old queries.
pub fn glyph_width() -> u32 {
    owner::with_catalog(types::CatalogState::glyph_width)
}
pub fn glyph_height() -> u32 {
    owner::with_catalog(types::CatalogState::glyph_height)
}
pub fn glyph_row(c: u8, row: u32) -> u8 {
    owner::with_catalog(|s| s.glyph_row(c, row))
}
