// Kernel-local R4F v1 container contract for the built-in font loader.
// Multi-byte fields are little-endian. Tags are stored as four ASCII bytes.

pub const MAGIC = [_]u8{ 'R', '4', 'F', '1' };
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 32;
pub const TABLE_ENTRY_SIZE: usize = 16;

pub const FLAG_HAS_BITMAP: u32 = 0x0000_0001;
pub const FLAG_HAS_OUTLINE: u32 = 0x0000_0002;
pub const FLAG_HAS_SFNT: u32 = 0x0000_0004;
pub const FLAG_HAS_KERNING: u32 = 0x0000_0008;

pub const FACE_RECORD_SIZE: usize = 64;
pub const STRIKE_RECORD_SIZE: usize = 40;
pub const GLYPH_MAP_RECORD_SIZE: usize = 16;
pub const GLYPH_METRIC_RECORD_SIZE: usize = 24;
pub const BITMAP_GLYPH_RECORD_SIZE: usize = 16;
pub const KERN_PAIR_RECORD_SIZE: usize = 12;
pub const OUTLINE_RECORD_SIZE: usize = 24;
pub const SFNT_RECORD_SIZE: usize = 24;

pub const TABLE_NAME: u32 = tag("NAME");
pub const TABLE_FACE: u32 = tag("FACE");
pub const TABLE_STRIKE: u32 = tag("STRK");
pub const TABLE_GLYPH_MAP: u32 = tag("GMAP");
pub const TABLE_GLYPH_METRICS: u32 = tag("GMET");
pub const TABLE_BITMAP_DATA: u32 = tag("BDAT");
pub const TABLE_KERNING: u32 = tag("KERN");
pub const TABLE_OUTLINE: u32 = tag("OUTL");
pub const TABLE_SFNT: u32 = tag("SFNT");
pub const TABLE_META: u32 = tag("META");

pub const FONT_KIND_BITMAP: u16 = 1;
pub const FONT_KIND_WINDOWS_VECTOR: u16 = 2;
pub const FONT_KIND_SFNT_TRUETYPE: u16 = 3;

pub const STYLE_MONOSPACE: u32 = 0x0000_0001;
pub const STYLE_ITALIC: u32 = 0x0000_0002;
pub const STYLE_BOLD: u32 = 0x0000_0004;
pub const STYLE_UNDERLINE: u32 = 0x0000_0008;
pub const STYLE_STRIKEOUT: u32 = 0x0000_0010;

pub const CHARSET_CP437: u16 = 437;
pub const CHARSET_WINDOWS_1252: u16 = 1252;
pub const CHARSET_UNICODE: u16 = 0xFFFF;

pub const CHARSET_FLAG_CP437: u32 = 0x0000_0001;
pub const CHARSET_FLAG_WINDOWS_1252: u32 = 0x0000_0002;
pub const CHARSET_FLAG_UNICODE: u32 = 0x0000_0004;

pub const BITMAP_FORMAT_MONO1_MSB: u16 = 1;
pub const OUTLINE_KIND_WINDOWS_VECTOR_FNT: u16 = 1;
pub const OUTLINE_KIND_TRUETYPE_GLYF: u16 = 2;
pub const SFNT_KIND_TRUETYPE: u16 = 1;

pub const REQUIRED_SFNT_CMAP: u32 = 0x0000_0001;
pub const REQUIRED_SFNT_NAME: u32 = 0x0000_0002;
pub const REQUIRED_SFNT_HEAD: u32 = 0x0000_0004;
pub const REQUIRED_SFNT_HHEA: u32 = 0x0000_0008;
pub const REQUIRED_SFNT_HMTX: u32 = 0x0000_0010;
pub const REQUIRED_SFNT_MAXP: u32 = 0x0000_0020;
pub const REQUIRED_SFNT_LOCA: u32 = 0x0000_0040;
pub const REQUIRED_SFNT_GLYF: u32 = 0x0000_0080;
pub const REQUIRED_SFNT_OS2: u32 = 0x0000_0100;

pub const INVALID_GLYPH_ID: u16 = 0xFFFF;

pub fn tag(comptime bytes: *const [4]u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

pub fn readU16(s: []const u8) u16 {
    return @as(u16, s[0]) | (@as(u16, s[1]) << 8);
}

pub fn readI16(s: []const u8) i16 {
    return @as(i16, @bitCast(readU16(s)));
}

pub fn readU32(s: []const u8) u32 {
    return @as(u32, s[0]) |
        (@as(u32, s[1]) << 8) |
        (@as(u32, s[2]) << 16) |
        (@as(u32, s[3]) << 24);
}

pub fn readTag(s: []const u8) u32 {
    return readU32(s[0..4]);
}
