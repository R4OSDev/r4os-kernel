//! Original NTFS collation and BMP-only conversion acceptance boundaries.
use super::ntfs_format_types::le16;
pub fn upcase(table: &[u8], unit: u16) -> u16 {
    let i = usize::from(unit) * 2;
    if i + 2 > table.len() {
        unit
    } else {
        le16(table, i)
    }
}
pub fn compare(table: &[u8], left: &[u8], right: &[u8]) -> i8 {
    let ln = left.len() / 2;
    let rn = right.len() / 2;
    for i in 0..ln.min(rn) {
        let l = upcase(table, le16(left, i * 2));
        let r = upcase(table, le16(right, i * 2));
        if l < r {
            return -1;
        }
        if l > r {
            return 1;
        }
    }
    if ln < rn {
        -1
    } else if ln > rn {
        1
    } else {
        0
    }
}
pub fn ascii_matches(table: &[u8], ascii: &[u8], utf16: &[u8]) -> bool {
    if utf16.len() != ascii.len() * 2 {
        return false;
    }
    for (i, b) in ascii.iter().enumerate() {
        if upcase(table, u16::from(*b)) != upcase(table, le16(utf16, i * 2)) {
            return false;
        }
    }
    true
}
pub fn to_ascii(s: &[u8], out: &mut [u8]) -> Option<usize> {
    let n = s.len() / 2;
    if n > out.len() {
        return None;
    }
    for (i, byte) in out.iter_mut().take(n).enumerate() {
        let c = le16(s, i * 2);
        if !(0x20..=0x7e).contains(&c) {
            return None;
        }
        *byte = c as u8
    }
    Some(n)
}
pub fn from_ascii(s: &[u8], out: &mut [u8]) -> Option<usize> {
    if out.len() < s.len() * 2 {
        return None;
    }
    for (i, c) in s.iter().enumerate() {
        out[i * 2] = *c;
        out[i * 2 + 1] = 0
    }
    Some(s.len() * 2)
}
pub fn to_utf16(s: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut i = 0;
    let mut o = 0;
    while i < s.len() {
        let b = s[i];
        let (mut code, extra) = if b < 0x80 {
            (u32::from(b), 0)
        } else if b & 0xe0 == 0xc0 {
            (u32::from(b & 0x1f), 1)
        } else if b & 0xf0 == 0xe0 {
            (u32::from(b & 15), 2)
        } else {
            return None;
        };
        if i + extra >= s.len() {
            return None;
        }
        for k in 0..extra {
            let c = s[i + 1 + k];
            if c & 0xc0 != 0x80 {
                return None;
            }
            code = (code << 6) | u32::from(c & 0x3f)
        }
        i += 1 + extra;
        if code == 0 || (0xd800..=0xdfff).contains(&code) || code > 0xffff {
            return None;
        }
        if o + 2 > out.len() {
            return None;
        }
        out[o] = code as u8;
        out[o + 1] = (code >> 8) as u8;
        o += 2
    }
    Some(o)
}
pub fn to_utf8(s: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut o = 0;
    for i in 0..s.len() / 2 {
        let c = le16(s, i * 2);
        if c == 0 || (0xd800..=0xdfff).contains(&c) {
            return None;
        }
        if c < 0x80 {
            if o + 1 > out.len() {
                return None;
            }
            out[o] = c as u8;
            o += 1
        } else if c < 0x800 {
            if o + 2 > out.len() {
                return None;
            }
            out[o] = (0xc0 | (c >> 6)) as u8;
            out[o + 1] = (0x80 | (c & 0x3f)) as u8;
            o += 2
        } else {
            if o + 3 > out.len() {
                return None;
            }
            out[o] = (0xe0 | (c >> 12)) as u8;
            out[o + 1] = (0x80 | ((c >> 6) & 0x3f)) as u8;
            out[o + 2] = (0x80 | (c & 0x3f)) as u8;
            o += 3
        }
    }
    Some(o)
}
