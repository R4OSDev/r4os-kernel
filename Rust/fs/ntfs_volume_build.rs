//! Original FILE-record and resident/nonresident attribute emitters.
use super::{
    ntfs_format_types::*, ntfs_volume_build_names as names, ntfs_volume_records as r,
    ntfs_volume_types::*,
};
#[allow(clippy::too_many_arguments)]
pub fn emit(
    record: &mut [u8],
    offset: usize,
    kind: u32,
    name: &[u8],
    indexed: bool,
    value: &[u8],
    flags: u8,
    instance: &mut u16,
) -> usize {
    let name_offset = 24;
    let value_offset = (name_offset + name.len() + 7) & !7usize;
    let total = (value_offset + value.len() + 7) & !7usize;
    let a = &mut record[offset..];
    a[..total].fill(0);
    r::put32(a, 0, kind);
    r::put32(a, 4, u32::try_from(total).expect("NTFS attribute size"));
    a[8] = 0;
    a[9] = u8::try_from(name.len() / 2).expect("NTFS attribute name");
    r::put16(a, 10, name_offset as u16);
    r::put16(a, 14, *instance);
    *instance += 1;
    r::put32(
        a,
        16,
        u32::try_from(value.len()).expect("NTFS resident value"),
    );
    r::put16(a, 20, value_offset as u16);
    a[22] = u8::from(indexed || flags != 0);
    a[name_offset..name_offset + name.len()].copy_from_slice(name);
    a[value_offset..value_offset + value.len()].copy_from_slice(value);
    total
}
pub fn stub(
    record: &mut [u8],
    offset: usize,
    data: u64,
    allocation: u64,
    instance: &mut u16,
) -> usize {
    let a = &mut record[offset..];
    a[..0x48].fill(0);
    r::put32(a, 0, 0x80);
    r::put32(a, 4, 0x48);
    a[8] = 1;
    r::put16(a, 10, 0x40);
    r::put16(a, 14, *instance);
    *instance += 1;
    r::put64(a, 0x10, 0);
    r::put64(a, 0x18, 0);
    r::put16(a, 0x20, 0x40);
    r::put64(a, 0x28, allocation);
    r::put64(a, 0x30, data);
    r::put64(a, 0x38, 0);
    0x48
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn file(
    v: Volume,
    number: u64,
    sequence: u16,
    parent: u64,
    parent_sequence: u16,
    name: View,
    resident: Option<View>,
    size: u64,
    allocation: u64,
) -> usize {
    unsafe {
        let buffer = v.write_record();
        let b = r::mutable(buffer);
        b.fill(0);
        r::put32(b, 0, 0x454c4946);
        r::put16(b, 4, 0x30);
        r::put16(
            b,
            6,
            u16::try_from(v.record_bytes / 512 + 1).expect("NTFS record USA"),
        );
        r::put16(b, 16, sequence);
        r::put16(b, 18, 1);
        r::put16(b, 20, 0x38);
        r::put16(b, 22, 1);
        r::put32(b, 28, v.record_bytes);
        r::put32(b, 44, u32::try_from(number).expect("NTFS record number"));
        let mut offset = 0x38;
        let mut instance = 0;
        let mut si = [0u8; 0x48];
        for t in (0..32).step_by(8) {
            r::put64(&mut si, t, v.now_filetime)
        }
        r::put32(&mut si, 32, 0x20);
        r::put32(&mut si, 0x34, v.security_id_file);
        offset += emit(b, offset, 0x10, &[], false, &si, 0, &mut instance);
        let mut filename = [0u8; 0x42 + 510];
        let data = resident.map_or(size, |d| d.len as u64);
        let Some(n) = names::value(
            v,
            &mut filename,
            parent,
            parent_sequence,
            name.get(),
            false,
            allocation,
            data,
            0,
        ) else {
            return 0;
        };
        offset += emit(b, offset, 0x30, &[], true, &filename[..n], 0, &mut instance);
        if let Some(data) = resident {
            offset += emit(b, offset, 0x80, &[], false, data.get(), 0, &mut instance)
        } else {
            offset += stub(b, offset, size, allocation, &mut instance)
        }
        r::put32(b, offset, u32::MAX);
        r::put32(b, offset + 4, 0);
        offset += 8;
        r::put16(b, 0x28, instance);
        r::put32(b, 0x18, offset as u32);
        offset
    }
}
