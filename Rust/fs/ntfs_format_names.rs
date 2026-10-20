//! NTFS $UpCase collation plus strict R4STD UTF8/UTF16 scalar conversions.
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
    if !ascii.is_ascii() || utf16.len() != ascii.len() * 2 || !valid_utf16(utf16) {
        return false;
    }
    for (i, b) in ascii.iter().enumerate() {
        if upcase(table, u16::from(*b)) != upcase(table, le16(utf16, i * 2)) {
            return false;
        }
    }
    true
}
/// File/stream names use strict Unicode scalars; on-disk LE counts are bytes.
/// NUL is excluded by this filesystem boundary, not by the shared codec.
pub fn valid_utf16(s: &[u8]) -> bool {
    r4os_text_core::utf16::validate_le(s).is_ok() && !s.as_chunks::<2>().0.contains(&[0, 0])
}
pub fn valid_name(s: &[u8]) -> bool {
    !s.is_empty()
        && !s.contains(&0)
        && r4os_text_core::utf16::units_required(s).is_ok_and(|n| n <= 255)
}
/// Preflight the complete path before traversal can populate metadata caches.
pub fn valid_path(path: &[u8]) -> bool {
    path.split(|b| matches!(b, b'/' | b'\\'))
        .filter(|part| !part.is_empty())
        .all(valid_name)
}
pub fn to_ascii(s: &[u8], out: &mut [u8]) -> Option<usize> {
    if !s.len().is_multiple_of(2) || s.len() / 2 > out.len() {
        return None;
    }
    if s.as_chunks::<2>()
        .0
        .iter()
        .any(|u| !(0x20..=0x7e).contains(&u16::from_le_bytes(*u)))
    {
        return None;
    }
    let n = s.len() / 2;
    for (dst, unit) in out[..n].iter_mut().zip(s.as_chunks::<2>().0) {
        *dst = unit[0];
    }
    Some(n)
}
pub fn from_ascii(s: &[u8], out: &mut [u8]) -> Option<usize> {
    if s.iter().any(|b| !(0x20..=0x7e).contains(b)) {
        return None;
    }
    let n = s.len().checked_mul(2)?;
    if n > out.len() {
        return None;
    }
    for (dst, &byte) in out[..n].as_chunks_mut::<2>().0.iter_mut().zip(s) {
        *dst = [byte, 0];
    }
    Some(n)
}
pub fn to_utf16(s: &[u8], out: &mut [u8]) -> Option<usize> {
    if s.contains(&0) {
        return None;
    }
    r4os_text_core::utf16::from_utf8_le(s, out).ok()
}
pub fn to_utf8(s: &[u8], out: &mut [u8]) -> Option<usize> {
    if !valid_utf16(s) {
        return None;
    }
    r4os_text_core::utf16::le_to_utf8(s, out).ok()
}
