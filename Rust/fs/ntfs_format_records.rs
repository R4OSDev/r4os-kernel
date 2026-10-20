//! Exact original NTFS BPB/FILE/attribute/index byte codecs. No I/O/state.
use super::ntfs_format_types::*;
fn size_byte(raw: u8, bytes: u32) -> Option<u32> {
    let signed = raw as i8;
    if signed < 0 {
        let n = (-i16::from(signed)) as u8;
        if n > 30 {
            return None;
        }
        return Some(1u32 << n);
    }
    if signed == 0 || bytes == 0 {
        return None;
    }
    Some((signed as u32) * bytes)
}
fn sectors(raw: u8) -> Option<u32> {
    if raw == 0 {
        return None;
    }
    if raw <= 0x80 {
        return raw.is_power_of_two().then_some(u32::from(raw));
    }
    let shift = raw & 15;
    if shift > 12 {
        None
    } else {
        Some(1u32 << shift)
    }
}
pub fn boot(s: &[u8], out: &mut BootSector) -> u8 {
    if s.len() < 512 {
        return 1;
    }
    if &s[3..11] != b"NTFS    " {
        return 2;
    }
    if s[510] != 0x55 || s[511] != 0xaa {
        return 3;
    }
    let bps = le16(s, 11);
    if !(256..=4096).contains(&bps) || !bps.is_power_of_two() {
        return 4;
    }
    let Some(spc) = sectors(s[13]) else { return 5 };
    let bytes = u32::from(bps) * spc;
    if bytes == 0 || bytes > 2 * 1024 * 1024 {
        return 5;
    }
    let total = le64(s, 0x28);
    let mft = le64(s, 0x30);
    if total == 0 || mft == 0 {
        return 8;
    }
    let Some(record) = size_byte(s[0x40], bytes) else {
        return 6;
    };
    if !(256..=4096).contains(&record) || !record.is_power_of_two() {
        return 6;
    }
    let Some(index) = size_byte(s[0x44], bytes) else {
        return 7;
    };
    if !(512..=65536).contains(&index) || !index.is_power_of_two() {
        return 7;
    }
    *out = BootSector {
        bytes_per_sector: bps,
        sectors_per_cluster_raw: s[13],
        cluster_bytes: bytes,
        total_sectors: total,
        mft_lcn: mft,
        mftmirr_lcn: le64(s, 0x38),
        file_record_bytes: record,
        index_block_bytes: index,
        serial_number: le64(s, 0x48),
    };
    0
}
pub fn fixup(record: &mut [u8], install: Option<u16>) -> u8 {
    if record.len() < 10 {
        return 1;
    }
    let ofs = le16(record, 4) as usize;
    let count = le16(record, 6) as usize;
    if count < 2 {
        return 2;
    }
    let sectors = count - 1;
    if record.len() < sectors * 512 {
        return 1;
    }
    if record.len() != sectors * 512 || ofs + count * 2 > record.len() {
        return 2;
    }
    if let Some(previous) = install {
        let mut usn = previous.wrapping_add(1);
        if usn == 0 || usn == u16::MAX {
            usn = 1
        }
        record[ofs..ofs + 2].copy_from_slice(&usn.to_le_bytes());
        for i in 0..sectors {
            let end = (i + 1) * 512 - 2;
            let original = le16(record, end);
            record[ofs + 2 + i * 2..ofs + 4 + i * 2].copy_from_slice(&original.to_le_bytes());
            record[end..end + 2].copy_from_slice(&usn.to_le_bytes())
        }
    } else {
        let usn = le16(record, ofs);
        for i in 0..sectors {
            let end = (i + 1) * 512 - 2;
            if le16(record, end) != usn {
                return 3;
            }
            let original = le16(record, ofs + 2 + i * 2);
            record[end..end + 2].copy_from_slice(&original.to_le_bytes())
        }
    }
    0
}
pub fn file_header(s: &[u8]) -> Option<FileRecordHeader> {
    if s.len() < 0x30 || le32(s, 0) != 0x454c4946 {
        return None;
    }
    let h = FileRecordHeader {
        magic: le32(s, 0),
        lsn: le64(s, 8),
        sequence: le16(s, 0x10),
        link_count: le16(s, 0x12),
        attrs_offset: le16(s, 0x14),
        flags: le16(s, 0x16),
        bytes_in_use: le32(s, 0x18),
        bytes_allocated: le32(s, 0x1c),
        base_record: FileReference::parse(le64(s, 0x20)),
        next_attr_instance: le16(s, 0x28),
        record_number: le32(s, 0x2c),
    };
    if h.attrs_offset < 0x30
        || h.attrs_offset as usize >= s.len()
        || h.bytes_in_use < u32::from(h.attrs_offset)
        || h.bytes_in_use as usize > s.len()
    {
        None
    } else {
        Some(h)
    }
}
pub fn attribute(s: &[u8], offset: &mut usize) -> Option<Attribute> {
    if *offset + 8 > s.len() {
        return None;
    }
    let kind = le32(s, *offset);
    if kind == u32::MAX {
        return None;
    }
    let length = le32(s, *offset + 4);
    if length < 0x18 || !length.is_multiple_of(8) || *offset + length as usize > s.len() {
        return None;
    }
    let base = *offset;
    *offset += length as usize;
    let nr = s[base + 8] != 0;
    let n = s[base + 9] as usize;
    let no = le16(s, base + 10) as usize;
    let name = if n > 0 {
        let end = no + n * 2;
        if end > length as usize {
            return None;
        }
        View::of(&s[base + no..base + end])
    } else {
        View::default()
    };
    let mut a = Attribute {
        attr_type: kind,
        length,
        non_resident: nr,
        name,
        flags: le16(s, base + 12),
        instance: le16(s, base + 14),
        ..Attribute::default()
    };
    if !nr {
        if base + 0x18 > s.len() {
            return None;
        }
        let n = le32(s, base + 0x10) as usize;
        let o = le16(s, base + 0x14) as usize;
        let end = o + n;
        if end > length as usize {
            return None;
        }
        a.value = View::of(&s[base + o..base + end])
    } else {
        if base + 0x40 > s.len() {
            return None;
        }
        a.lowest_vcn = le64(s, base + 0x10);
        a.highest_vcn = le64(s, base + 0x18);
        let o = le16(s, base + 0x20) as usize;
        a.compression_unit = s[base + 0x22];
        a.allocated_size = le64(s, base + 0x28);
        a.data_size = le64(s, base + 0x30);
        a.initialized_size = le64(s, base + 0x38);
        if o > length as usize {
            return None;
        }
        a.mapping_pairs = View::of(&s[base + o..base + length as usize])
    }
    Some(a)
}
pub fn standard(s: &[u8]) -> Option<StandardInformation> {
    if s.len() < 0x24 {
        return None;
    }
    Some(StandardInformation {
        creation_time: le64(s, 0),
        data_change_time: le64(s, 8),
        mft_change_time: le64(s, 16),
        access_time: le64(s, 24),
        file_attributes: le32(s, 32),
    })
}
pub fn file_name(s: &[u8]) -> Option<FileName> {
    if s.len() < 0x42 {
        return None;
    }
    let n = s[0x40];
    let end = 0x42 + usize::from(n) * 2;
    if s.len() < end || !super::ntfs_format_names::valid_utf16(&s[0x42..end]) {
        return None;
    }
    Some(FileName {
        parent: FileReference::parse(le64(s, 0)),
        creation_time: le64(s, 8),
        data_change_time: le64(s, 16),
        mft_change_time: le64(s, 24),
        access_time: le64(s, 32),
        allocated_size: le64(s, 40),
        data_size: le64(s, 48),
        flags: le32(s, 56),
        name_length: n,
        namespace: s[0x41],
        name: View::of(&s[0x42..end]),
    })
}
pub fn attr_list(s: &[u8], offset: &mut usize) -> Option<AttributeListEntry> {
    if *offset + 0x1a > s.len() {
        return None;
    }
    let base = *offset;
    let length = le16(s, base + 4);
    if length < 0x1a || base + usize::from(length) > s.len() {
        return None;
    }
    let n = s[base + 6] as usize;
    let o = s[base + 7] as usize;
    let name = if n > 0 {
        let end = o + n * 2;
        if end > length as usize {
            return None;
        }
        View::of(&s[base + o..base + end])
    } else {
        View::default()
    };
    *offset += usize::from(length);
    Some(AttributeListEntry {
        attr_type: le32(s, base),
        entry_length: length,
        lowest_vcn: le64(s, base + 8),
        mft_reference: FileReference::parse(le64(s, base + 16)),
        instance: le16(s, base + 24),
        name,
    })
}
pub fn index_header(s: &[u8]) -> Option<IndexHeader> {
    if s.len() < 16 {
        return None;
    }
    let h = IndexHeader {
        entries_offset: le32(s, 0),
        index_length: le32(s, 4),
        allocated_size: le32(s, 8),
        flags: s[12],
    };
    if h.entries_offset < 16 || h.index_length < h.entries_offset {
        None
    } else {
        Some(h)
    }
}
pub fn index_root(s: &[u8]) -> Option<IndexRoot> {
    if s.len() < 32 {
        return None;
    }
    let h = index_header(&s[16..])?;
    let start = 16 + h.entries_offset as usize;
    let end = 16 + h.index_length as usize;
    if start > s.len() || end > s.len() || start > end {
        return None;
    }
    Some(IndexRoot {
        indexed_attr_type: le32(s, 0),
        collation_rule: le32(s, 4),
        index_block_bytes: le32(s, 8),
        header: h,
        entries: View::of(&s[start..end]),
    })
}
pub fn index_block(s: &[u8]) -> Option<IndexBlock> {
    if s.len() < 0x28 || le32(s, 0) != 0x58444e49 {
        return None;
    }
    let h = index_header(&s[24..])?;
    let start = 24 + h.entries_offset as usize;
    let end = 24 + h.index_length as usize;
    if start > s.len() || end > s.len() || start > end {
        return None;
    }
    Some(IndexBlock {
        lsn: le64(s, 8),
        vcn: le64(s, 16),
        entries: View::of(&s[start..end]),
    })
}
pub fn index_entry(s: &[u8], state: &mut IteratorState) -> Option<IndexEntry> {
    if state.done {
        return None;
    }
    let base = state.offset;
    if base + 16 > s.len() {
        state.done = true;
        return None;
    }
    let length = le16(s, base + 8);
    if length < 16 || base + usize::from(length) > s.len() {
        state.done = true;
        return None;
    }
    let key_length = le16(s, base + 10);
    let flags = le16(s, base + 12);
    let key = if key_length > 0 {
        if 16 + usize::from(key_length) > usize::from(length) {
            state.done = true;
            return None;
        }
        View::of(&s[base + 16..base + 16 + usize::from(key_length)])
    } else {
        View::default()
    };
    let mut node = 0;
    if flags & 1 != 0 {
        if length < 24 {
            state.done = true;
            return None;
        }
        node = le64(s, base + usize::from(length) - 8)
    }
    state.offset += usize::from(length);
    if flags & 2 != 0 {
        state.done = true
    }
    Some(IndexEntry {
        file_reference: le64(s, base),
        entry_length: length,
        key_length,
        flags,
        key,
        has_sub_node_vcn: flags & 1 != 0,
        sub_node_vcn: node,
    })
}
pub fn volume_info(s: &[u8]) -> Option<VolumeInformation> {
    if s.len() < 12 {
        None
    } else {
        Some(VolumeInformation {
            major: s[8],
            minor: s[9],
            flags: le16(s, 10),
        })
    }
}
