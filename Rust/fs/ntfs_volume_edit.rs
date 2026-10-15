//! Original pure record attribute editing and mapping-pair publication.
use super::{
    ntfs_format_records as fmt, ntfs_format_runs as pairs,
    ntfs_format_types::*,
    ntfs_volume_records::{put16, put32, put64},
};
#[derive(Clone, Copy)]
pub struct Span {
    pub offset: usize,
    pub length: usize,
}
pub fn find(record: &[u8], header: FileRecordHeader, kind: u32, name: &[u8]) -> Option<Span> {
    let mut offset = usize::from(header.attrs_offset);
    while offset + 8 <= record.len() {
        let t = le32(record, offset);
        if t == u32::MAX {
            return None;
        }
        let length = le32(record, offset + 4) as usize;
        if length < 0x18 || offset + length > record.len() {
            return None;
        }
        if t == kind {
            let n = record[offset + 9] as usize;
            let o = le16(record, offset + 10) as usize;
            let found = if n > 0 {
                &record[offset + o..offset + o + n * 2]
            } else {
                &record[..0]
            };
            if found == name {
                return Some(Span { offset, length });
            }
        }
        offset += length
    }
    None
}
pub fn resize(
    record: &mut [u8],
    header: &mut FileRecordHeader,
    span: &mut Span,
    new_length: usize,
) -> bool {
    let aligned = (new_length + 7) & !7usize;
    if aligned == span.length {
        return true;
    }
    let tail = span.offset + span.length;
    let n = header.bytes_in_use as usize - tail;
    let end = span.offset + aligned + n;
    if end > record.len() {
        return false;
    }
    record.copy_within(tail..tail + n, span.offset + aligned);
    put32(
        record,
        span.offset + 4,
        u32::try_from(aligned).expect("NTFS attribute length"),
    );
    header.bytes_in_use = u32::try_from(end).expect("NTFS record bytes");
    put32(record, 0x18, header.bytes_in_use);
    span.length = aligned;
    true
}
#[inline(never)]
#[allow(clippy::too_many_arguments)] // Preserve original explicit descriptor/size inputs.
pub fn nonresident(
    record: &mut [u8],
    header: &mut FileRecordHeader,
    kind: u32,
    name: &[u8],
    runs: &[Run],
    data: u64,
    initialized: u64,
    allocation: u64,
) -> bool {
    let Some(mut span) = find(record, *header, kind, name) else {
        return false;
    };
    if record[span.offset + 8] != 1 {
        return false;
    }
    let mut mapping = [0u8; 4096];
    let mut used = 0;
    let mut previous = 0i64;
    let mut total = 0u64;
    for run in runs {
        if used >= mapping.len() {
            return false;
        }
        let delta = if run.has_lcn {
            if run.lcn > i64::MAX as u64 {
                return false;
            }
            Some(run.lcn as i64 - previous)
        } else {
            None
        };
        let Some(n) = pairs::encode(&mut mapping[used..], run.length_clusters, delta) else {
            return false;
        };
        used += n;
        if run.has_lcn {
            previous = run.lcn as i64
        }
        let Some(next) = total.checked_add(run.length_clusters) else {
            return false;
        };
        total = next
    }
    if used >= mapping.len() {
        return false;
    }
    mapping[used] = 0;
    used += 1;
    let offset = le16(record, span.offset + 0x20) as usize;
    if offset > span.length || !resize(record, header, &mut span, offset + used) {
        return false;
    }
    put64(
        record,
        span.offset + 0x18,
        if total == 0 { 0 } else { total - 1 },
    );
    put64(record, span.offset + 0x28, allocation);
    put64(record, span.offset + 0x30, data);
    put64(record, span.offset + 0x38, initialized);
    let flags = le16(record, span.offset + 12);
    if flags & (0x8000 | 1) != 0 && offset >= 0x48 && total > 0 && allocation.is_multiple_of(total)
    {
        let cluster = allocation / total;
        let mut mapped = 0u64;
        for run in runs {
            if run.has_lcn {
                let Some(next) = mapped.checked_add(run.length_clusters) else {
                    return false;
                };
                mapped = next
            }
        }
        let Some(actual) = mapped.checked_mul(cluster) else {
            return false;
        };
        put64(record, span.offset + 0x40, actual)
    }
    record[span.offset + offset..span.offset + offset + used].copy_from_slice(&mapping[..used]);
    true
}
pub fn resident(
    record: &mut [u8],
    header: &mut FileRecordHeader,
    kind: u32,
    name: &[u8],
    value: &[u8],
) -> bool {
    let Some(mut span) = find(record, *header, kind, name) else {
        return false;
    };
    if record[span.offset + 8] != 0 {
        return false;
    }
    let offset = le16(record, span.offset + 0x14) as usize;
    if !resize(record, header, &mut span, offset + value.len()) {
        return false;
    }
    put32(
        record,
        span.offset + 0x10,
        u32::try_from(value.len()).expect("NTFS resident length"),
    );
    record[span.offset + offset..span.offset + offset + value.len()].copy_from_slice(value);
    true
}
pub fn filename_dup(
    record: &mut [u8],
    header: FileRecordHeader,
    allocation: u64,
    data: u64,
    mtime: u64,
) -> bool {
    let Some(span) = find(record, header, 0x30, &[]) else {
        return false;
    };
    let value = span.offset + le16(record, span.offset + 0x14) as usize;
    if mtime != 0 {
        put64(record, value + 0x10, mtime);
        put64(record, value + 0x18, mtime)
    }
    put64(record, value + 0x28, allocation);
    put64(record, value + 0x30, data);
    true
}
pub fn insert(record: &mut [u8], bytes: &[u8]) -> bool {
    let Some(mut h) = fmt::file_header(record) else {
        return false;
    };
    let kind = le32(bytes, 0);
    let mut offset = usize::from(h.attrs_offset);
    while offset + 8 <= record.len() {
        let t = le32(record, offset);
        if t == u32::MAX || t > kind {
            break;
        }
        let n = le32(record, offset + 4) as usize;
        if n < 0x18 || offset + n > record.len() {
            return false;
        }
        offset += n
    }
    let add = bytes.len();
    let tail = h.bytes_in_use as usize - offset;
    if h.bytes_in_use as usize + add > record.len() {
        return false;
    }
    record.copy_within(offset..offset + tail, offset + add);
    record[offset..offset + add].copy_from_slice(bytes);
    let instance = le16(record, 0x28);
    put16(record, offset + 14, instance);
    put16(record, 0x28, instance + 1);
    h.bytes_in_use += u32::try_from(add).expect("NTFS insertion length");
    put32(record, 0x18, h.bytes_in_use);
    true
}
pub fn remove(record: &mut [u8], kind: u32, name: &[u8]) -> bool {
    let Some(mut h) = fmt::file_header(record) else {
        return false;
    };
    let Some(span) = find(record, h, kind, name) else {
        return false;
    };
    let tail = span.offset + span.length;
    let n = h.bytes_in_use as usize - tail;
    record.copy_within(tail..tail + n, span.offset);
    h.bytes_in_use -= u32::try_from(span.length).expect("NTFS removal length");
    put32(record, 0x18, h.bytes_in_use);
    true
}
