//! Original FILE_NAME/index-key preparation and namespace classification.
use super::{
    ntfs_format_names as names, ntfs_format_records as fmt,
    ntfs_format_types::*,
    ntfs_volume_records::{put16, put32, put64},
    ntfs_volume_types::Volume,
};
pub fn safe83(name: &[u8]) -> bool {
    let mut dot = None;
    for (i, c) in name.iter().copied().enumerate() {
        if c == b'.' {
            if dot.is_some() {
                return false;
            }
            dot = Some(i);
            continue;
        }
        if !(c.is_ascii_uppercase() || c.is_ascii_digit() || b"_-~$@!#%&'(){}^`".contains(&c)) {
            return false;
        }
    }
    let base = dot.unwrap_or(name.len());
    if base == 0 || base > 8 {
        return false;
    }
    if let Some(dot) = dot {
        let ext = name.len() - dot - 1;
        if ext == 0 || ext > 3 {
            return false;
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub fn value(
    v: Volume,
    out: &mut [u8],
    parent: u64,
    sequence: u16,
    name: &[u8],
    directory: bool,
    allocation: u64,
    data: u64,
    created: u64,
) -> Option<usize> {
    let namespace = if safe83(name) { 3 } else { 0 };
    let flags = if directory { 0x1000_0000 } else { 0x20 };
    if 0x42 > out.len() {
        return None;
    }
    let n = names::to_utf16(name, &mut out[0x42..])?;
    let len = 0x42 + n;
    out[..0x42].fill(0);
    put64(
        out,
        0,
        (FileReference {
            record: parent,
            sequence,
        })
        .pack(),
    );
    put64(
        out,
        8,
        if created != 0 {
            created
        } else {
            v.now_filetime
        },
    );
    put64(out, 16, v.now_filetime);
    put64(out, 24, v.now_filetime);
    put64(out, 32, v.now_filetime);
    put64(out, 40, allocation);
    put64(out, 48, data);
    put32(out, 56, flags);
    out[64] = u8::try_from(n / 2).expect("NTFS filename units");
    out[65] = namespace;
    Some(len)
}
#[allow(clippy::too_many_arguments)]
pub fn key(
    v: Volume,
    out: &mut [u8],
    parent: u64,
    sequence: u16,
    name: &[u8],
    directory: bool,
    allocation: u64,
    data: u64,
) -> Option<usize> {
    if out.len() < 16 {
        return None;
    }
    let n = value(
        v,
        &mut out[16..],
        parent,
        sequence,
        name,
        directory,
        allocation,
        data,
        0,
    )?;
    let len = (16 + n + 7) & !7usize;
    if len > out.len() {
        return None;
    }
    out[..16].fill(0);
    out[16 + n..len].fill(0);
    put16(out, 8, u16::try_from(len).expect("NTFS index entry length"));
    put16(out, 10, u16::try_from(n).expect("NTFS filename key length"));
    Some(len)
}
pub fn filename(key: &[u8]) -> Option<FileName> {
    if key.len() < 16 + 0x42 {
        None
    } else {
        fmt::file_name(&key[16..])
    }
}
