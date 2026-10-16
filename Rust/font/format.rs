//! Kernel-private original R4F1 wire contract, independent of font host tools.
pub const MAGIC: [u8; 4] = *b"R4F1";
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 32;
pub const TABLE_ENTRY_SIZE: usize = 16;
pub const FLAG_HAS_BITMAP: u32 = 1;
pub const FLAG_HAS_OUTLINE: u32 = 2;
pub const FLAG_HAS_SFNT: u32 = 4;
pub const FLAG_HAS_KERNING: u32 = 8;
pub const FACE_RECORD_SIZE: usize = 64;
pub const STRIKE_RECORD_SIZE: usize = 40;
pub const GLYPH_MAP_RECORD_SIZE: usize = 16;
pub const GLYPH_METRIC_RECORD_SIZE: usize = 24;
pub const BITMAP_GLYPH_RECORD_SIZE: usize = 16;
pub const KERN_PAIR_RECORD_SIZE: usize = 12;
pub const OUTLINE_RECORD_SIZE: usize = 24;
pub const SFNT_RECORD_SIZE: usize = 24;
pub const fn tag(b: [u8; 4]) -> u32 {
    u32::from_le_bytes(b)
}
pub const TABLE_NAME: u32 = tag(*b"NAME");
pub const TABLE_FACE: u32 = tag(*b"FACE");
pub const TABLE_STRIKE: u32 = tag(*b"STRK");
pub const TABLE_GLYPH_MAP: u32 = tag(*b"GMAP");
pub const TABLE_GLYPH_METRICS: u32 = tag(*b"GMET");
pub const TABLE_BITMAP_DATA: u32 = tag(*b"BDAT");
pub const TABLE_KERNING: u32 = tag(*b"KERN");
pub const TABLE_OUTLINE: u32 = tag(*b"OUTL");
pub const TABLE_SFNT: u32 = tag(*b"SFNT");
pub const TABLE_META: u32 = tag(*b"META");
pub const FONT_KIND_BITMAP: u16 = 1;
pub const FONT_KIND_WINDOWS_VECTOR: u16 = 2;
pub const FONT_KIND_SFNT_TRUETYPE: u16 = 3;
pub const STYLE_MONOSPACE: u32 = 1;
pub const STYLE_ITALIC: u32 = 2;
pub const STYLE_BOLD: u32 = 4;
pub const STYLE_UNDERLINE: u32 = 8;
pub const STYLE_STRIKEOUT: u32 = 16;
pub const CHARSET_CP437: u16 = 437;
pub const CHARSET_WINDOWS_1252: u16 = 1252;
pub const CHARSET_UNICODE: u16 = 0xffff;
pub const CHARSET_FLAG_CP437: u32 = 1;
pub const CHARSET_FLAG_WINDOWS_1252: u32 = 2;
pub const CHARSET_FLAG_UNICODE: u32 = 4;
pub const BITMAP_FORMAT_MONO1_MSB: u16 = 1;
pub const OUTLINE_KIND_WINDOWS_VECTOR_FNT: u16 = 1;
pub const OUTLINE_KIND_TRUETYPE_GLYF: u16 = 2;
pub const SFNT_KIND_TRUETYPE: u16 = 1;
pub const REQUIRED_SFNT_CMAP: u32 = 1;
pub const REQUIRED_SFNT_NAME: u32 = 2;
pub const REQUIRED_SFNT_HEAD: u32 = 4;
pub const REQUIRED_SFNT_HHEA: u32 = 8;
pub const REQUIRED_SFNT_HMTX: u32 = 16;
pub const REQUIRED_SFNT_MAXP: u32 = 32;
pub const REQUIRED_SFNT_LOCA: u32 = 64;
pub const REQUIRED_SFNT_GLYF: u32 = 128;
pub const REQUIRED_SFNT_OS2: u32 = 256;
pub fn read_u16(s: &[u8]) -> u16 {
    u16::from_le_bytes([s[0], s[1]])
}
pub fn read_i16(s: &[u8]) -> i16 {
    i16::from_le_bytes([s[0], s[1]])
}
pub fn read_u32(s: &[u8]) -> u32 {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}
