//! Original bounded R4F table decoding and stable lowest-ID glyph index.
use super::{format as f, types::*};
#[derive(Clone, Copy)]
pub struct Parsed<'a> {
    pub kind: FontKind,
    pub weight: u16,
    pub style_flags: u32,
    pub charset_flags: u32,
    pub face_count: u16,
    pub strike_count: u16,
    pub glyph_count: u32,
    pub family: &'a [u8],
    pub face: &'a [u8],
    pub style: &'a [u8],
    pub renderable: bool,
    pub reason: &'static [u8],
    pub width: u8,
    pub height: u8,
    pub bytes_per_row: u8,
    pub max_advance: u8,
    pub ascent: i16,
    pub descent: i16,
    pub line_height: u8,
    pub baseline: i16,
    pub strike_id: u16,
    pub strike_table: &'a [u8],
    pub map_table: &'a [u8],
    pub metric_table: &'a [u8],
    pub bitmap_table: &'a [u8],
}
fn range_valid(total: u32, offset: u32, size: u32) -> bool {
    offset <= total && size <= total - offset
}
fn records_fit(total: usize, offset: usize, count: u32, stride: usize) -> bool {
    offset <= total && stride != 0 && count as usize <= (total - offset) / stride
}
fn find_table(bytes: &[u8], file_size: u32, dir: u32, count: u16, tag: u32) -> Option<&[u8]> {
    for i in 0..usize::from(count) {
        let off = dir as usize + i * f::TABLE_ENTRY_SIZE;
        let record = &bytes[off..off + f::TABLE_ENTRY_SIZE];
        if f::read_u32(record) != tag {
            continue;
        }
        let offset = f::read_u32(&record[4..]);
        let size = f::read_u32(&record[8..]);
        if !range_valid(file_size, offset, size) {
            return None;
        }
        return Some(&bytes[offset as usize..(offset + size) as usize]);
    }
    None
}
fn name_at<'a>(names: &'a [u8], offset: u32, fallback: &'a [u8]) -> &'a [u8] {
    let offset = offset as usize;
    if offset >= names.len() {
        return fallback;
    }
    let mut end = offset;
    while end < names.len() && names[end] != 0 {
        end += 1
    }
    if end == offset {
        fallback
    } else {
        &names[offset..end]
    }
}
pub fn parse(bytes: &[u8]) -> Option<Parsed<'_>> {
    if bytes.len() < f::HEADER_SIZE || bytes[..4] != f::MAGIC {
        return None;
    }
    let version = f::read_u16(&bytes[4..]);
    let header = f::read_u16(&bytes[6..]);
    let flags = f::read_u32(&bytes[8..]);
    let size = f::read_u32(&bytes[12..]);
    let dir = f::read_u32(&bytes[16..]);
    let count = f::read_u16(&bytes[20..]);
    let face_count = f::read_u16(&bytes[22..]);
    let strike_count = f::read_u16(&bytes[24..]);
    let glyph_count = f::read_u32(&bytes[28..]);
    if version != f::VERSION
        || usize::from(header) < f::HEADER_SIZE
        || size < u32::from(header)
        || size as usize > bytes.len()
        || dir < u32::from(header)
        || !range_valid(size, dir, u32::from(count) * f::TABLE_ENTRY_SIZE as u32)
    {
        return None;
    }
    let names = find_table(bytes, size, dir, count, f::TABLE_NAME).unwrap_or(&[]);
    let face = find_table(bytes, size, dir, count, f::TABLE_FACE).unwrap_or(&[]);
    let strike = find_table(bytes, size, dir, count, f::TABLE_STRIKE).unwrap_or(&[]);
    let map = find_table(bytes, size, dir, count, f::TABLE_GLYPH_MAP).unwrap_or(&[]);
    let metric = find_table(bytes, size, dir, count, f::TABLE_GLYPH_METRICS).unwrap_or(&[]);
    let bitmap = find_table(bytes, size, dir, count, f::TABLE_BITMAP_DATA).unwrap_or(&[]);
    let mut p = Parsed {
        kind: FontKind::Unknown,
        weight: 0,
        style_flags: 0,
        charset_flags: 0,
        face_count,
        strike_count,
        glyph_count,
        family: b"R4OS",
        face: b"Builtin",
        style: b"Regular",
        renderable: false,
        reason: b"no renderable strike",
        width: 8,
        height: 8,
        bytes_per_row: 1,
        max_advance: 8,
        ascent: 7,
        descent: 1,
        line_height: 8,
        baseline: 7,
        strike_id: 0,
        strike_table: strike,
        map_table: map,
        metric_table: metric,
        bitmap_table: bitmap,
    };
    if face.len() >= f::FACE_RECORD_SIZE {
        p.kind = match f::read_u16(&face[2..]) {
            1 => FontKind::Bitmap,
            2 => FontKind::WindowsVector,
            3 => FontKind::SfntTruetype,
            _ => FontKind::Unknown,
        };
        p.style_flags = f::read_u32(&face[4..]);
        p.weight = f::read_u16(&face[8..]);
        p.charset_flags = f::read_u32(&face[12..]);
        p.ascent = f::read_i16(&face[18..]);
        p.descent = f::read_i16(&face[20..]);
        let lh = f::read_i16(&face[28..]);
        if lh > 0 && lh <= MAX_GLYPH_H as i16 {
            p.line_height = lh as u8
        }
        if p.ascent > 0 {
            p.baseline = p.ascent
        }
        p.family = name_at(names, f::read_u32(&face[30..]), b"R4OS");
        p.face = name_at(names, f::read_u32(&face[34..]), p.family);
        p.style = name_at(names, f::read_u32(&face[38..]), b"Regular");
    } else if flags & f::FLAG_HAS_SFNT != 0 {
        p.kind = FontKind::SfntTruetype
    } else if flags & f::FLAG_HAS_OUTLINE != 0 {
        p.kind = FontKind::WindowsVector
    }
    if strike.len() >= f::STRIKE_RECORD_SIZE
        && map.len() >= 4
        && metric.len() >= 4
        && bitmap.len() >= 8
    {
        let id = f::read_u16(strike);
        let width = f::read_u16(&strike[8..]);
        let height = f::read_u16(&strike[10..]);
        let ascent = f::read_i16(&strike[12..]);
        let descent = f::read_i16(&strike[14..]);
        let lh = f::read_i16(&strike[16..]);
        let baseline = f::read_i16(&strike[18..]);
        let format = f::read_u16(&strike[20..]);
        let row = f::read_u16(&strike[22..]);
        let glyphs = f::read_u32(&strike[24..]);
        if width > 0
            && width <= u16::from(MAX_GLYPH_W)
            && height > 0
            && usize::from(height) <= MAX_GLYPH_H
            && row > 0
            && usize::from(row) <= MAX_BYTES_PER_ROW
            && format == f::BITMAP_FORMAT_MONO1_MSB
            && glyphs > 0
            && glyphs as usize <= MAX_GLYPHS
        {
            // Validate scalar/provenance records before catalogue publication.
            // Legacy tags describe provenance, never byte-valued codepoints.
            let mut mapped = [INVALID_CODEPOINT; MAX_GLYPHS];
            if p.charset_flags & !7 != 0
                || p.charset_flags & 4 == 0
                || !map_metadata_valid(map, p.charset_flags)
                || !load_map(map, glyphs, &mut mapped)
            {
                return None;
            }
            p.strike_id = id;
            p.width = width as u8;
            p.height = height as u8;
            p.bytes_per_row = row as u8;
            p.glyph_count = glyphs;
            p.max_advance = width as u8;
            if ascent > 0 {
                p.ascent = ascent
            }
            p.descent = descent;
            p.line_height = if lh > 0 && lh <= MAX_GLYPH_H as i16 {
                lh as u8
            } else {
                height as u8
            };
            p.baseline = if baseline > 0 { baseline } else { p.ascent };
            p.renderable = true;
            p.reason = b"bitmap strike";
            p.kind = FontKind::Bitmap;
            return Some(p);
        }
        p.reason = b"unsupported bitmap strike";
        return Some(p);
    }
    if flags & f::FLAG_HAS_SFNT != 0 {
        p.kind = FontKind::SfntTruetype;
        p.reason = b"sfnt catalog only";
    } else if flags & f::FLAG_HAS_OUTLINE != 0 {
        p.kind = FontKind::WindowsVector;
        p.reason = b"outline catalog only";
    }
    Some(p)
}
fn clamp_metric(value: i32, fallback: u8) -> u8 {
    if value <= 0 {
        fallback
    } else {
        value.min(i32::from(MAX_GLYPH_W)) as u8
    }
}
fn load_metrics(
    table: &[u8],
    glyphs: u32,
    width: u8,
    advance: u8,
    widths: &mut [u8; MAX_GLYPHS],
    advances: &mut [u8; MAX_GLYPHS],
) -> bool {
    if table.len() < 4 {
        return false;
    }
    let count = f::read_u32(table);
    if !records_fit(table.len(), 4, count, f::GLYPH_METRIC_RECORD_SIZE) {
        return false;
    }
    for i in 0..count as usize {
        let off = 4 + i * f::GLYPH_METRIC_RECORD_SIZE;
        let id = f::read_u32(&table[off..]);
        if id >= glyphs || id as usize >= MAX_GLYPHS {
            continue;
        }
        let adv = f::read_i16(&table[off + 4..]);
        let x0 = f::read_i16(&table[off + 12..]);
        let x1 = f::read_i16(&table[off + 16..]);
        let w = i32::from(x1) - i32::from(x0);
        if w < 0 || w > i32::from(MAX_GLYPH_W) {
            return false;
        }
        widths[id as usize] = clamp_metric(w, width);
        advances[id as usize] = clamp_metric(i32::from(adv), advance);
    }
    true
}
fn load_bitmap(
    table: &[u8],
    strike: u16,
    height: u8,
    row: u8,
    glyphs: u32,
    out: &mut Glyphs,
) -> bool {
    if table.len() < 8 {
        return false;
    }
    let count = f::read_u32(table);
    let payload = f::read_u32(&table[4..]);
    if count < glyphs || !records_fit(table.len(), 8, count, f::BITMAP_GLYPH_RECORD_SIZE) {
        return false;
    }
    let end = 8 + count as usize * f::BITMAP_GLYPH_RECORD_SIZE;
    if (payload as usize) < end || payload as usize > table.len() {
        return false;
    }
    let mut loaded = 0u32;
    let mut seen = [false; MAX_GLYPHS];
    for i in 0..count as usize {
        let off = 8 + i * f::BITMAP_GLYPH_RECORD_SIZE;
        let id = f::read_u32(&table[off..]);
        let rec = f::read_u16(&table[off + 4..]);
        let format = f::read_u16(&table[off + 6..]);
        let offset = f::read_u32(&table[off + 8..]);
        let size = f::read_u32(&table[off + 12..]);
        if rec != strike {
            continue;
        }
        if format != f::BITMAP_FORMAT_MONO1_MSB {
            return false;
        }
        let needed = u32::from(height) * u32::from(row);
        if id >= glyphs
            || id as usize >= MAX_GLYPHS
            || size < needed
            || offset < payload
            || !range_valid(
                u32::try_from(table.len()).expect("Original R4F table length"),
                offset,
                size,
            )
        {
            return false;
        }
        if seen[id as usize] {
            return false;
        }
        seen[id as usize] = true;
        for y in 0..usize::from(height) {
            for x in 0..usize::from(row) {
                out[id as usize][y][x] = table[offset as usize + y * usize::from(row) + x];
            }
        }
        loaded += 1;
    }
    loaded >= glyphs
}
fn map_metadata_valid(table: &[u8], flags: u32) -> bool {
    if table.len() < 4 {
        return false;
    }
    let count = f::read_u32(table);
    if count as usize > MAX_GLYPHS * 2
        || !records_fit(table.len(), 4, count, f::GLYPH_MAP_RECORD_SIZE)
    {
        return false;
    }
    for record in table[4..4 + count as usize * f::GLYPH_MAP_RECORD_SIZE]
        .as_chunks::<{ f::GLYPH_MAP_RECORD_SIZE }>()
        .0
    {
        let charset = f::read_u16(&record[2..]);
        if f::read_u16(record) != 0
            || f::read_u32(&record[12..]) != 0
            || !match charset {
                437 => flags & 1 != 0,
                1252 => flags & 2 != 0,
                0xffff => true,
                _ => false,
            }
        {
            return false;
        }
    }
    true
}
fn load_map(table: &[u8], glyphs: u32, out: &mut [u32; MAX_GLYPHS]) -> bool {
    if table.len() < 4 {
        return false;
    }
    let count = f::read_u32(table);
    if count as usize > MAX_GLYPHS * 2
        || !records_fit(table.len(), 4, count, f::GLYPH_MAP_RECORD_SIZE)
    {
        return false;
    }
    let mut mapped = 0;
    for i in 0..count as usize {
        let off = 4 + i * f::GLYPH_MAP_RECORD_SIZE;
        let cp = f::read_u32(&table[off + 4..]);
        let id = f::read_u32(&table[off + 8..]);
        if char::from_u32(cp).is_none() || id >= glyphs || id as usize >= MAX_GLYPHS {
            return false;
        }
        let slot = &mut out[id as usize];
        if *slot != INVALID_CODEPOINT && *slot != cp {
            return false;
        }
        *slot = cp;
        mapped += 1;
    }
    mapped > 0
}
/// # Safety
/// Caller retains the complete readable input span and aligned exclusive initialized cache output, where present, through this bounded operation.
pub unsafe fn fill_cached(p: Parsed<'_>, out: *mut CachedFont) -> bool {
    if !p.renderable {
        return false;
    }
    unsafe {
        reset_cached(out);
        let c = &mut *out;
        c.used = true;
        c.width = p.width;
        c.height = p.height;
        c.bytes_per_row = p.bytes_per_row;
        c.max_advance = p.max_advance;
        c.ascent = p.ascent;
        c.descent = p.descent;
        c.line_height = p.line_height;
        c.baseline = p.baseline;
        c.widths.fill(p.width);
        c.advances.fill(p.max_advance);
        if !load_metrics(
            p.metric_table,
            p.glyph_count,
            p.width,
            p.max_advance,
            &mut c.widths,
            &mut c.advances,
        ) {
            return false;
        }
        if !load_bitmap(
            p.bitmap_table,
            p.strike_id,
            p.height,
            p.bytes_per_row,
            p.glyph_count,
            &mut c.glyphs,
        ) {
            return false;
        }
        if !load_map(p.map_table, p.glyph_count, &mut c.codepoints) {
            return false;
        }
        build_index(&c.codepoints, &mut c.index)
    }
}
pub fn build_index(codepoints: &[u32; MAX_GLYPHS], out: &mut GlyphIndex) -> bool {
    *out = GlyphIndex::EMPTY;
    for (id, &cp) in codepoints.iter().enumerate() {
        if cp == INVALID_CODEPOINT {
            continue;
        }
        let mut insertion = usize::from(out.count);
        while insertion > 0 {
            let previous = insertion - 1;
            let old = out.codepoints[previous];
            let old_id = out.glyph_ids[previous];
            if old < cp || old == cp && usize::from(old_id) <= id {
                break;
            }
            out.codepoints[insertion] = old;
            out.glyph_ids[insertion] = old_id;
            insertion = previous;
        }
        out.codepoints[insertion] = cp;
        out.glyph_ids[insertion] = id as u16;
        out.count += 1;
    }
    out.count != 0
}
pub fn indexed_id(index: &GlyphIndex, cp: u32, probes: Option<&mut u16>) -> Option<usize> {
    let mut probes = probes;
    let mut low = 0;
    let mut high = usize::from(index.count);
    while low < high {
        if let Some(n) = probes.as_deref_mut() {
            *n = n.wrapping_add(1)
        }
        let middle = low + (high - low) / 2;
        if index.codepoints[middle] < cp {
            low = middle + 1
        } else {
            high = middle
        }
    }
    if low >= usize::from(index.count) || index.codepoints[low] != cp {
        None
    } else {
        Some(usize::from(index.glyph_ids[low]))
    }
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
pub fn path_equals(a: &[u8], b: &[u8]) -> bool {
    fn normalize(b: u8) -> u8 {
        if b == b'/' {
            b'\\'
        } else {
            b.to_ascii_uppercase()
        }
    }
    fn equal(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len()
            && a.iter()
                .copied()
                .map(normalize)
                .eq(b.iter().copied().map(normalize))
    }
    fn strip(p: &[u8]) -> &[u8] {
        if p.len() >= 2 && p[1] == b':' {
            &p[2..]
        } else {
            p
        }
    }
    equal(a, b) || equal(strip(a), strip(b))
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DecodedScalar {
    pub codepoint: u32,
    pub consumed: usize,
}
/// # Safety
/// Caller retains the complete readable input span and aligned exclusive initialized cache output, where present, through this bounded operation.
pub unsafe fn decode_raw(p: *const u8, len: usize, start: usize) -> DecodedScalar {
    fn invalid(n: usize) -> DecodedScalar {
        DecodedScalar {
            codepoint: 0xfffd,
            consumed: n,
        }
    }
    if start >= len {
        return invalid(0);
    }
    let first = unsafe { p.add(start).read() };
    if first < 0x80 {
        return DecodedScalar {
            codepoint: u32::from(first),
            consumed: 1,
        };
    }
    let expected = if (0xc2..=0xdf).contains(&first) {
        2
    } else if (0xe0..=0xef).contains(&first) {
        3
    } else if (0xf0..=0xf4).contains(&first) {
        4
    } else {
        return invalid(1);
    };
    if expected > len - start {
        return invalid(1);
    }
    let mut cp = u32::from(first & (0x7f >> expected));
    for offset in 1..expected {
        let byte = unsafe { p.add(start + offset).read() };
        if byte & 0xc0 != 0x80 {
            return invalid(1);
        }
        cp = (cp << 6) | u32::from(byte & 0x3f);
    }
    if expected == 3 && cp < 0x800
        || expected == 4 && cp < 0x10000
        || cp > 0x10ffff
        || (0xd800..=0xdfff).contains(&cp)
    {
        return invalid(1);
    }
    DecodedScalar {
        codepoint: cp,
        consumed: expected,
    }
}

const _: () = {
    assert!(core::mem::size_of::<DecodedScalar>() == 16);
    assert!(core::mem::align_of::<DecodedScalar>() == 8);
    assert!(core::mem::offset_of!(DecodedScalar, codepoint) == 0);
    assert!(core::mem::offset_of!(DecodedScalar, consumed) == 8);
};
