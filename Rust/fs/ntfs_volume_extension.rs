//! Original single-record DATA spill and exact ATTRIBUTE_LIST identities.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_attributes as validation,
    ntfs_volume_build as build, ntfs_volume_cleanup as cleanup, ntfs_volume_edit as edit,
    ntfs_volume_lookup as lookup, ntfs_volume_mft as mft, ntfs_volume_names as diag,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
#[derive(Clone, Copy)]
pub struct Identity {
    pub record: u64,
    pub sequence: u16,
}
#[inline(never)]
pub(super) unsafe fn identity(v: Volume, base: u64, out: *mut Identity) -> Lookup {
    unsafe {
        let Some(h) = r::load(v, base, v.record()) else {
            return Lookup::Io;
        };
        let record = v.record();
        if !validation::stream_valid(r::view(record), h) {
            return Lookup::Io;
        }
        let Some(list) = r::find(v, record, h, 0x20, View::default()) else {
            return Lookup::NotFound;
        };
        if list.non_resident {
            return Lookup::Io;
        }
        let mut offset = 0;
        let mut extension: Option<Identity> = None;
        let mut saw = false;
        while let Some(e) = fmt::attr_list(list.value.get(), &mut offset) {
            if e.attr_type != 0x80 || e.name.len != 0 {
                continue;
            }
            if saw || e.lowest_vcn != 0 {
                return Lookup::Io;
            }
            saw = true;
            if e.mft_reference.record == base {
                if e.mft_reference.sequence != h.sequence {
                    return Lookup::Io;
                }
                let Some(data) = r::find(v, record, h, 0x80, View::default()) else {
                    return Lookup::Io;
                };
                if !data.non_resident || data.lowest_vcn != 0 || data.instance != e.instance {
                    return Lookup::Io;
                }
                continue;
            }
            let Some(eh) = r::load(v, e.mft_reference.record, v.part_record()) else {
                return Lookup::Io;
            };
            let erecord = v.part_record();
            if eh.sequence != e.mft_reference.sequence
                || eh.base_record.record != base
                || eh.base_record.sequence != h.sequence
                || !validation::stream_valid(r::view(erecord), eh)
            {
                return Lookup::Io;
            }
            let Some(data) = r::find(v, erecord, eh, 0x80, View::default()) else {
                return Lookup::Io;
            };
            if !data.non_resident || data.lowest_vcn != 0 || data.instance != e.instance {
                return Lookup::Io;
            }
            if let Some(old) = extension
                && (old.record != e.mft_reference.record
                    || old.sequence != e.mft_reference.sequence)
            {
                return Lookup::Io;
            }
            extension = Some(Identity {
                record: e.mft_reference.record,
                sequence: e.mft_reference.sequence,
            });
        }
        if offset != list.value.len {
            return Lookup::Io;
        }
        if let Some(e) = extension {
            out.write(e);
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn list_value(
    out: &mut [u8],
    base: u64,
    base_seq: u16,
    si: u16,
    filename: u16,
    ext: u64,
    ext_seq: u16,
    data: u16,
) -> usize {
    let base = (FileReference {
        record: base,
        sequence: base_seq,
    })
    .pack();
    let ext = (FileReference {
        record: ext,
        sequence: ext_seq,
    })
    .pack();
    let mut offset = 0;
    for (kind, reference, instance) in [(0x10, base, si), (0x30, base, filename), (0x80, ext, data)]
    {
        out[offset..offset + 32].fill(0);
        r::put32(out, offset, kind);
        r::put16(out, offset + 4, 32);
        out[offset + 7] = 0x1a;
        r::put64(out, offset + 8, 0);
        r::put64(out, offset + 16, reference);
        r::put16(out, offset + 24, instance);
        offset += 32
    }
    offset
}
fn instance(record: &[u8], h: FileRecordHeader, kind: u32) -> Option<u16> {
    let mut offset = usize::from(h.attrs_offset);
    while offset + 8 <= record.len() {
        let t = le32(record, offset);
        if t == u32::MAX {
            return None;
        }
        let n = le32(record, offset + 4) as usize;
        if n < 0x18 || offset + n > record.len() {
            return None;
        }
        if t == kind && record[offset + 9] == 0 {
            return Some(le16(record, offset + 14));
        }
        offset += n
    }
    None
}
#[inline(never)]
pub(super) unsafe fn spill(
    v: Volume,
    base: u64,
    runs: *const Run,
    count: usize,
    data: u64,
    initialized: u64,
    allocation: u64,
) -> WriteStatus {
    unsafe {
        let Some(base_sequence) = lookup::sequence(v, base) else {
            return WriteStatus::Io;
        };
        let slot = mft::allocate(v);
        if slot.status != WriteStatus::Ok {
            return slot.status;
        }
        let ext = slot.number;
        let sequence = slot.sequence;
        let buffer = v.part_record();
        let b = r::mutable(buffer);
        b.fill(0);
        r::put32(b, 0, 0x454c4946);
        r::put16(b, 4, 0x30);
        r::put16(b, 6, (v.record_bytes / 512 + 1) as u16);
        r::put16(b, 16, sequence);
        r::put16(b, 18, 0);
        r::put16(b, 20, 0x38);
        r::put16(b, 22, 1);
        r::put32(b, 28, v.record_bytes);
        r::put64(
            b,
            32,
            (FileReference {
                record: base,
                sequence: base_sequence,
            })
            .pack(),
        );
        r::put32(b, 44, u32::try_from(ext).expect("NTFS extension record"));
        let mut attr_instance = 0;
        let mut offset = 0x38;
        offset += build::stub(b, offset, data, allocation, &mut attr_instance);
        r::put32(b, offset, u32::MAX);
        r::put32(b, offset + 4, 0);
        offset += 8;
        r::put16(b, 40, attr_instance);
        r::put32(b, 24, offset as u32);
        let Some(mut h) = fmt::file_header(r::view(buffer)) else {
            return WriteStatus::Io;
        };
        if !edit::nonresident(
            r::mutable(buffer),
            &mut h,
            0x80,
            &[],
            core::slice::from_raw_parts(runs, count),
            data,
            initialized,
            allocation,
        ) {
            return cleanup::unpublished(v, ext, false, WriteStatus::RecordFull);
        }
        if !r::store(v, ext, buffer) {
            return WriteStatus::Io;
        }
        if !cleanup::budgeted_flush(v) {
            return cleanup::unpublished(v, ext, true, WriteStatus::Io);
        }
        let Some(h) = r::load(v, base, v.write_record()) else {
            return cleanup::unpublished(v, ext, true, WriteStatus::Io);
        };
        let buffer = v.write_record();
        let Some(si) = instance(r::view(buffer), h, 0x10) else {
            return cleanup::unpublished(v, ext, true, WriteStatus::Io);
        };
        let Some(filename) = instance(r::view(buffer), h, 0x30) else {
            return cleanup::unpublished(v, ext, true, WriteStatus::Io);
        };
        if !edit::remove(r::mutable(buffer), 0x80, &[]) {
            return cleanup::unpublished(v, ext, true, WriteStatus::Io);
        }
        let mut value = [0u8; 96];
        let n = list_value(&mut value, base, h.sequence, si, filename, ext, sequence, 0);
        let mut bytes = [0u8; 120];
        let total = (24 + n + 7) & !7usize;
        bytes[..total].fill(0);
        r::put32(&mut bytes, 0, 0x20);
        r::put32(&mut bytes, 4, total as u32);
        r::put16(&mut bytes, 10, 24);
        r::put32(&mut bytes, 16, n as u32);
        r::put16(&mut bytes, 20, 24);
        bytes[24..24 + n].copy_from_slice(&value[..n]);
        if !edit::insert(r::mutable(buffer), &bytes[..total]) {
            return cleanup::unpublished(v, ext, true, WriteStatus::RecordFull);
        }
        if r::store(v, base, buffer) {
            WriteStatus::Ok
        } else {
            WriteStatus::Io
        }
    }
}
#[inline(never)]
pub(super) unsafe fn commit(
    v: Volume,
    base: u64,
    runs: *const Run,
    count: usize,
    data: u64,
    initialized: u64,
    allocation: u64,
) -> WriteStatus {
    unsafe {
        let mut ext = Identity {
            record: 0,
            sequence: 0,
        };
        diag::append_at(151);
        match identity(v, base, &mut ext) {
            Lookup::Found => {}
            Lookup::Io => return WriteStatus::Io,
            Lookup::NotFound => {
                diag::append_at(152);
                let Some(mut h) = r::load(v, base, v.write_record()) else {
                    return WriteStatus::Io;
                };
                let buffer = v.write_record();
                diag::append_at(153);
                if edit::nonresident(
                    r::mutable(buffer),
                    &mut h,
                    0x80,
                    &[],
                    core::slice::from_raw_parts(runs, count),
                    data,
                    initialized,
                    allocation,
                ) {
                    diag::append_at(154);
                    return if r::store(v, base, buffer) {
                        WriteStatus::Ok
                    } else {
                        WriteStatus::Io
                    };
                }
                diag::append_at(155);
                return spill(v, base, runs, count, data, initialized, allocation);
            }
        }
        diag::append_at(156);
        let Some(mut h) = r::load(v, ext.record, v.part_record()) else {
            return WriteStatus::Io;
        };
        diag::append_at(157);
        if h.sequence != ext.sequence {
            return WriteStatus::Io;
        }
        let buffer = v.part_record();
        diag::append_at(158);
        if !edit::nonresident(
            r::mutable(buffer),
            &mut h,
            0x80,
            &[],
            core::slice::from_raw_parts(runs, count),
            data,
            initialized,
            allocation,
        ) {
            return WriteStatus::RecordFull;
        }
        diag::append_at(159);
        if r::store(v, ext.record, buffer) {
            WriteStatus::Ok
        } else {
            WriteStatus::Io
        }
    }
}
pub(super) unsafe fn resides(v: Volume, parent: u64, name: View) -> bool {
    unsafe {
        let mut found = LookupResult::EMPTY;
        if lookup::lookup(v, parent, name, &mut found, false, 0) != Lookup::Found {
            return false;
        }
        let mut ext = Identity {
            record: 0,
            sequence: 0,
        };
        identity(v, found.record, &mut ext) == Lookup::Found
    }
}
