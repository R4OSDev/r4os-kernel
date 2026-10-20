//! Strict VFAT chain, shared R4STD UTF8/UTF16 and existing ASCII collation.
use super::fat_types::*;
pub fn upper(c: u8) -> u8 {
    if c.is_ascii_lowercase() {
        c - (b'a' - b'A')
    } else {
        c
    }
}
pub fn short_allowed(c: u8) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'$' | b'~')
}
pub fn build_short(name: &[u8], out: &mut [u8; 11]) -> bool {
    out.fill(b' ');
    if name.is_empty() {
        return false;
    }
    let mut dot = None;
    for (i, &v) in name.iter().enumerate() {
        if v == b'.' {
            if dot.is_some() {
                return false;
            }
            dot = Some(i);
        }
    }
    let (base, ext) = dot.map_or((name, &b""[..]), |d| (&name[..d], &name[d + 1..]));
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return false;
    }
    for (i, &ch) in base.iter().enumerate() {
        let c = upper(ch);
        if !short_allowed(c) {
            return false;
        }
        out[i] = c;
    }
    for (i, &ch) in ext.iter().enumerate() {
        let c = upper(ch);
        if !short_allowed(c) {
            return false;
        }
        out[8 + i] = c;
    }
    true
}
pub fn valid_short(name: &[u8]) -> bool {
    build_short(name, &mut [0; 11])
}
pub fn utf8_units(name: &[u8], out: &mut [u16]) -> Option<usize> {
    r4os_text_core::utf16::from_utf8(name, out).ok()
}
pub fn units_utf8(units: &[u16], out: &mut [u8]) -> Option<usize> {
    r4os_text_core::utf16::to_utf8(units, out).ok()
}
pub fn well_formed(units: &[u16]) -> bool {
    char::decode_utf16(units.iter().copied()).all(|scalar| scalar.is_ok())
}
pub fn valid_long(name: &[u8]) -> bool {
    if name.is_empty() || name.len() >= NAME_MAX || name[0] == b'.' || name[name.len() - 1] == b'.'
    {
        return false;
    }
    let mut units = [0u16; UNITS];
    let Some(n) = utf8_units(name, &mut units) else {
        return false;
    };
    let mut text = false;
    for &unit in &units[..n] {
        if unit < 0x20 || unit == 0x7f || unit == 0xffff {
            return false;
        }
        match unit {
            92 | 47 | 58 | 42 | 63 | 34 | 60 | 62 | 124 => return false,
            32 => {}
            _ => text = true,
        }
    }
    text
}
pub fn valid(name: &[u8]) -> bool {
    valid_short(name) || valid_long(name)
}
/// Validate every component before the first lookup/cache or mutation.
pub fn valid_path(path: &[u8]) -> bool {
    path.split(|b| matches!(b, b'/' | b'\\'))
        .filter(|part| !part.is_empty())
        .all(valid)
}
pub fn equal_ascii(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| upper(a) == upper(b))
}
pub fn equal_collated(a: &[u8], b: &[u8]) -> Option<bool> {
    if a.is_empty() || b.is_empty() || !valid(a) || !valid(b) {
        None
    } else {
        Some(equal_ascii(a, b))
    }
}
/// OEM encoding is absent from a FAT short entry. Without an explicit codepage,
/// accept printable ASCII only; never reinterpret arbitrary OEM bytes as UTF8.
pub fn short_to_buffer(raw: &[u8], out: &mut [u8]) -> Option<usize> {
    if raw.len() < 11 || raw[..11].iter().any(|b| !(0x20..=0x7e).contains(b)) {
        return None;
    }
    let mut end = 8;
    while end > 0 && raw[end - 1] == b' ' {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut ext = 11;
    while ext > 8 && raw[ext - 1] == b' ' {
        ext -= 1;
    }
    let len = end + if ext > 8 { 1 + ext - 8 } else { 0 };
    if len > out.len() {
        return None;
    }
    out[..end].copy_from_slice(&raw[..end]);
    if ext > 8 {
        out[end] = b'.';
        out[end + 1..len].copy_from_slice(&raw[8..ext]);
    }
    Some(len)
}
pub fn short_equals(raw: &[u8], name: &[u8]) -> bool {
    let mut out = [0; NAME_MAX];
    short_to_buffer(raw, &mut out).is_some_and(|n| equal_ascii(&out[..n], name))
}
pub fn checksum(short: [u8; 11]) -> u8 {
    let mut sum = 0u8;
    for c in short {
        let low: u8 = if sum & 1 != 0 { 0x80 } else { 0 };
        sum = low.wrapping_add(sum >> 1).wrapping_add(c);
    }
    sum
}
const POS: [usize; 13] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
#[derive(Clone, Copy)]
pub struct Lfn {
    pub units: [u16; UNITS],
    pub len: usize,
    pub active: bool,
    pub total: u8,
    pub expected: u8,
    pub checksum: u8,
}
impl Lfn {
    pub const EMPTY: Self = Self {
        units: [0; UNITS],
        len: 0,
        active: false,
        total: 0,
        expected: 0,
        checksum: 0,
    };
    pub fn reset(&mut self) {
        *self = Self::EMPTY;
    }
    pub fn consume(&mut self, raw: &[u8]) -> bool {
        if raw.len() < 32
            || raw[11] != 15
            || raw[12] != 0
            || le16(&raw[26..28]) != 0
            || raw[0] & 0xa0 != 0
        {
            return false;
        }
        let ordinal = raw[0] & 31;
        if ordinal == 0 || usize::from(ordinal) > LFN_MAX {
            return false;
        }
        let last = raw[0] & 0x40 != 0;
        if !self.active {
            if !last {
                return false;
            }
            self.reset();
            self.active = true;
            self.total = ordinal;
            self.expected = ordinal;
            self.checksum = raw[13];
        } else if last {
            return false;
        }
        if ordinal != self.expected || raw[13] != self.checksum {
            return false;
        }
        let start = usize::from(ordinal - 1) * 13;
        let mut terminated = false;
        for (i, &off) in POS.iter().enumerate() {
            let unit = le16(&raw[off..off + 2]);
            let dst = start + i;
            if !last {
                if dst >= UNITS || unit == 0 || unit == 0xffff {
                    return false;
                }
                self.units[dst] = unit;
                self.len = self.len.max(dst + 1);
                continue;
            }
            if terminated {
                if unit != 0xffff {
                    return false;
                }
            } else if unit == 0 {
                terminated = true;
            } else if unit == 0xffff || dst >= UNITS {
                return false;
            } else {
                self.units[dst] = unit;
                self.len = self.len.max(dst + 1);
            }
        }
        self.expected -= 1;
        true
    }
    pub fn complete(&self, raw: &[u8]) -> bool {
        if !self.active || self.expected != 0 || self.len == 0 || raw.len() < 11 {
            return false;
        }
        let mut short = [0; 11];
        short.copy_from_slice(&raw[..11]);
        checksum(short) == self.checksum && well_formed(&self.units[..self.len])
    }
}
pub fn make_entry(raw: &[u8], lfn: &Lfn, lba: u64, offset: u16) -> Option<Entry> {
    if raw.len() < 32 {
        return None;
    }
    let mut out = Entry {
        attr: raw[11],
        has_directory_lba: true,
        directory_lba: lba,
        directory_offset: offset,
        first_cluster: first_cluster(raw),
        size: le32(&raw[28..32]),
        created_time: le16(&raw[14..16]),
        created_date: le16(&raw[16..18]),
        access_date: le16(&raw[18..20]),
        modified_time: le16(&raw[22..24]),
        modified_date: le16(&raw[24..26]),
        ..Entry::EMPTY
    };
    if lfn.active || lfn.len > 0 {
        if !lfn.complete(raw) || lfn.units[..lfn.len].contains(&0) {
            return None;
        }
        out.name_len = units_utf8(&lfn.units[..lfn.len], &mut out.name)?;
        return Some(out);
    }
    out.name_len = short_to_buffer(&raw[..11], &mut out.name)?;
    Some(out)
}
pub fn write_lfn(out: &mut [u8], units: &[u16], seq: usize, total: usize, sum: u8) {
    out.fill(0);
    let b = u8::try_from(seq).expect("LFN ordinal");
    out[0] = if seq == total { b | 0x40 } else { b };
    out[11] = 15;
    out[13] = sum;
    let start = (seq - 1) * 13;
    let mut ended = false;
    for (i, &off) in POS.iter().enumerate() {
        if start + i < units.len() {
            put16(&mut out[off..off + 2], units[start + i]);
        } else if !ended {
            put16(&mut out[off..off + 2], 0);
            ended = true;
        } else {
            put16(&mut out[off..off + 2], 0xffff);
        }
    }
}
fn decimal(value: u32, out: &mut [u8; 3]) -> usize {
    let mut tmp = [0; 3];
    let mut n = value;
    let mut count = 0;
    while n > 0 && count < 3 {
        tmp[2 - count] = u8::try_from(u32::from(b'0') + n % 10).expect("digit");
        count += 1;
        n /= 10;
    }
    out[..count].copy_from_slice(&tmp[3 - count..]);
    count
}
fn sanitize(c: u8) -> u8 {
    let c = upper(c);
    if short_allowed(c) { c } else { b'_' }
}
pub fn short_alias(name: &[u8], suffix: u32, out: &mut [u8; 11]) -> bool {
    out.fill(b' ');
    if suffix == 0 || suffix > 999 {
        return false;
    }
    let dot = name.iter().rposition(|&c| c == b'.');
    let (base, ext) = dot.map_or((name, &b""[..]), |d| (&name[..d], &name[d + 1..]));
    if base.is_empty() {
        return false;
    }
    let mut digits = [0; 3];
    let n = decimal(suffix, &mut digits);
    let limit = 8 - n - 1;
    let mut used = 0;
    for &c in base.iter().take(limit) {
        out[used] = sanitize(c);
        used += 1;
    }
    if used == 0 {
        out[0] = b'_';
        used = 1;
    }
    out[used] = b'~';
    out[used + 1..used + 1 + n].copy_from_slice(&digits[..n]);
    for (i, &c) in ext.iter().take(3).enumerate() {
        out[8 + i] = sanitize(c);
    }
    true
}
pub fn same_ownership(a: Entry, b: Entry) -> bool {
    !a.is_dir() && !b.is_dir() && a.first_cluster == b.first_cluster && a.size == b.size
}
pub fn same_identity(a: Entry, b: Entry) -> bool {
    a.attr == b.attr
        && a.first_cluster == b.first_cluster
        && a.size == b.size
        && a.created_time == b.created_time
        && a.created_date == b.created_date
        && a.access_date == b.access_date
        && a.modified_time == b.modified_time
        && a.modified_date == b.modified_date
}
pub fn same_copy(a: Entry, b: Entry) -> bool {
    if a.is_dir() || b.is_dir() {
        return false;
    }
    if a.has_directory_lba
        && b.has_directory_lba
        && a.directory_lba == b.directory_lba
        && a.directory_offset == b.directory_offset
    {
        return true;
    }
    a.first_cluster >= 2 && a.first_cluster == b.first_cluster
}
