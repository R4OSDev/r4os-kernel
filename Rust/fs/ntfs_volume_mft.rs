//! Original MFT record bitmap, 64-record growth and mirror-safe rollback.
use super::{
    ntfs_format_types::*, ntfs_volume_cleanup as cleanup, ntfs_volume_clusters as c,
    ntfs_volume_edit as edit, ntfs_volume_io as io, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
pub struct RecordAllocation {
    pub status: WriteStatus,
    pub number: u64,
    pub sequence: u16,
}
impl RecordAllocation {
    fn error(status: WriteStatus) -> Self {
        Self {
            status,
            number: 0,
            sequence: 0,
        }
    }
}
pub(super) unsafe fn copy_resident(a: *const AttrScratch, offset: u64, out: Buffer) -> bool {
    unsafe {
        ptr::write_bytes(out.data, 0, out.len);
        if offset >= (*a).resident_len as u64 {
            return true;
        }
        let n = ((*a).resident_len - offset as usize).min(out.len);
        assert!(offset as usize + n <= 1024);
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*a).resident_copy)
                .cast::<u8>()
                .add(offset as usize),
            out.data,
            n,
        );
        true
    }
}
#[inline(never)]
pub(super) unsafe fn allocate(v: Volume) -> RecordAllocation {
    unsafe {
        if v.record_bytes == 0 {
            return RecordAllocation::error(WriteStatus::Io);
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_a);
        if r::collect(v, 0, 0xb0, View::default(), bitmap) != Lookup::Found {
            return RecordAllocation::error(WriteStatus::Io);
        }
        let data = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_b);
        if r::collect(v, 0, 0x80, View::default(), data) != Lookup::Found {
            return RecordAllocation::error(WriteStatus::Io);
        }
        let records = (*data).data_size / u64::from(v.record_bytes);
        let hint = (*v.scratch()).record_search_start;
        let start = if hint >= 16 && hint < records {
            hint
        } else {
            16
        };
        let mut number = start;
        let mut end = records;
        let mut wrapped = start == 16;
        let mut loaded = u64::MAX;
        let mut sector = [0u8; 512];
        let buffer = Buffer {
            data: sector.as_mut_ptr(),
            len: 512,
        };
        loop {
            if number >= end {
                if wrapped {
                    number = records;
                    break;
                }
                number = 16;
                end = start;
                wrapped = true;
                loaded = u64::MAX
            }
            if v.io.work_counters {
                (*v.scratch()).work.record_bitmap_probes += 1
            }
            let byte = number / 8;
            let index = byte / 512;
            if index != loaded {
                let Some(offset) = index.checked_mul(512) else {
                    return RecordAllocation::error(WriteStatus::Io);
                };
                let ok = if (*bitmap).resident {
                    copy_resident(bitmap, offset, buffer)
                } else {
                    io::runs(
                        v.io,
                        r::attr_runs(bitmap),
                        offset,
                        buffer,
                        None,
                        ptr::null_mut(),
                    )
                };
                if !ok {
                    return RecordAllocation::error(WriteStatus::Io);
                }
                loaded = index
            }
            if (sector[(byte % 512) as usize] >> (number % 8)) & 1 == 0 {
                break;
            }
            number += 1
        }
        if number >= records {
            let status = grow(v);
            if status != WriteStatus::Ok {
                return RecordAllocation::error(status);
            }
            number = records;
            if r::collect(v, 0, 0xb0, View::default(), bitmap) != Lookup::Found {
                return RecordAllocation::error(WriteStatus::Io);
            }
            let Some(offset) = (number / 8 / 512).checked_mul(512) else {
                return RecordAllocation::error(WriteStatus::Io);
            };
            let ok = if (*bitmap).resident {
                copy_resident(bitmap, offset, buffer)
            } else {
                io::runs(
                    v.io,
                    r::attr_runs(bitmap),
                    offset,
                    buffer,
                    None,
                    ptr::null_mut(),
                )
            };
            if !ok {
                return RecordAllocation::error(WriteStatus::Io);
            }
        }
        let mut sequence = 1;
        if !r::load_raw(
            v,
            number,
            Buffer {
                data: ptr::addr_of_mut!((*v.scratch()).write_record).cast(),
                len: 4096,
            },
        ) {
            return RecordAllocation::error(WriteStatus::Io);
        }
        let raw = r::view(v.write_record());
        if le32(raw, 0) == 0x454c4946 {
            let old = le16(raw, 16);
            sequence = if old == u16::MAX { 1 } else { old + 1 }
        }
        if (*bitmap).resident {
            return RecordAllocation::error(WriteStatus::Io);
        }
        sector[(number / 8 % 512) as usize] |= 1u8 << (number % 8);
        let Some(offset) = (number / 8 / 512).checked_mul(512) else {
            return RecordAllocation::error(WriteStatus::Io);
        };
        if !io::runs(
            v.io,
            r::attr_runs(bitmap),
            offset,
            Buffer {
                data: sector.as_mut_ptr(),
                len: 512,
            },
            Some(Mutation::Attribute {
                record: 0,
                kind: 0xb0,
            }),
            ptr::null_mut(),
        ) {
            if !release(v, number) || !io::flush(v.io) {
                let _ = cleanup::uncertain(v);
                return RecordAllocation::error(WriteStatus::CleanupFailed);
            }
            return RecordAllocation::error(WriteStatus::Io);
        }
        (*v.scratch()).record_search_start = number + 1;
        RecordAllocation {
            status: WriteStatus::Ok,
            number,
            sequence,
        }
    }
}
#[inline(never)]
pub(super) unsafe fn grow(v: Volume) -> WriteStatus {
    unsafe {
        let data = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_b);
        if r::collect(v, 0, 0x80, View::default(), data) != Lookup::Found || (*data).resident {
            return WriteStatus::Io;
        }
        let bitmap = ptr::addr_of_mut!((*v.scratch()).attr_mgmt_a);
        if r::collect(v, 0, 0xb0, View::default(), bitmap) != Lookup::Found {
            return WriteStatus::Io;
        }
        let current = (*data).data_size / u64::from(v.record_bytes);
        let Some(grown) = current.checked_add(64) else {
            return WriteStatus::Io;
        };
        let Some(rounded) = grown.checked_add(7) else {
            return WriteStatus::Io;
        };
        if rounded / 8 > (*bitmap).data_size {
            return WriteStatus::NoSpace;
        }
        let Some(bytes) = 64u64.checked_mul(u64::from(v.record_bytes)) else {
            return WriteStatus::Io;
        };
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 || bytes % cluster != 0 {
            return WriteStatus::Io;
        }
        let count = bytes / cluster;
        if count == 0 {
            return WriteStatus::Io;
        }
        if (*data).count + 8 > 256 {
            return WriteStatus::RecordFull;
        }
        let mut runs = [Run::default(); 8];
        let result = c::allocate(
            v,
            count,
            c::RunBuffer {
                data: runs.as_mut_ptr(),
                len: 8,
            },
        );
        if result.status != WriteStatus::Ok {
            if result.status == WriteStatus::CleanupFailed {
                let _ = cleanup::uncertain(v);
            }
            return result.status;
        }
        let span = io::Runs {
            data: runs.as_ptr(),
            len: result.produced,
        };
        if !io::flush(v.io) {
            return rollback(v, span, false, WriteStatus::Io);
        }
        let zeros = [0u8; 512];
        for run in &runs[..result.produced] {
            if !run.has_lcn {
                return rollback(v, span, false, WriteStatus::Io);
            }
            let Some(n) = run.length_clusters.checked_mul(cluster) else {
                return rollback(v, span, false, WriteStatus::Io);
            };
            let mut written = 0;
            while written < n {
                if !io::lcn(
                    v.io,
                    run.lcn,
                    written,
                    Buffer {
                        data: zeros.as_ptr().cast_mut(),
                        len: 512,
                    },
                    Some(Mutation::Payload),
                    ptr::null_mut(),
                ) {
                    return rollback(v, span, false, WriteStatus::Io);
                }
                written += 512
            }
        }
        let mut total = (*data).count;
        for run in &runs[..result.produced] {
            let adjacent = if total > 0 && (*data).runs[total - 1].has_lcn && run.has_lcn {
                let Some(end) = (*data).runs[total - 1]
                    .lcn
                    .checked_add((*data).runs[total - 1].length_clusters)
                else {
                    return rollback(v, span, false, WriteStatus::Io);
                };
                end == run.lcn
            } else {
                false
            };
            if adjacent {
                let Some(n) = (*data).runs[total - 1]
                    .length_clusters
                    .checked_add(run.length_clusters)
                else {
                    return rollback(v, span, false, WriteStatus::Io);
                };
                (*data).runs[total - 1].length_clusters = n
            } else {
                if total >= 256 {
                    return rollback(v, span, false, WriteStatus::Io);
                }
                (*data).runs[total] = *run;
                total += 1
            }
        }
        if total > v.mft_capacity {
            return rollback(v, span, false, WriteStatus::Io);
        }
        let extended = io::Runs {
            data: ptr::addr_of!((*data).runs).cast(),
            len: total,
        };
        if !io::runs_valid(v.io, extended) {
            return rollback(v, span, false, WriteStatus::Io);
        }
        let Some(new_bytes) = (*data).data_size.checked_add(bytes) else {
            return rollback(v, span, false, WriteStatus::Io);
        };
        let Some(mut h) = r::load(v, 0, v.write_record()) else {
            return rollback(v, span, false, WriteStatus::Io);
        };
        let record = v.write_record();
        ptr::copy_nonoverlapping(
            record.data,
            ptr::addr_of_mut!((*v.scratch()).rollback_record).cast(),
            record.len,
        );
        if !edit::nonresident(
            r::mutable(record),
            &mut h,
            0x80,
            &[],
            core::slice::from_raw_parts(extended.data, total),
            new_bytes,
            new_bytes,
            new_bytes,
        ) {
            return rollback(v, span, false, WriteStatus::RecordFull);
        }
        if !edit::filename_dup(r::mutable(record), h, new_bytes, new_bytes, 0) {
            return rollback(v, span, false, WriteStatus::Io);
        }
        if !r::store(v, 0, record) || !io::flush(v.io) {
            return rollback(v, span, true, WriteStatus::Io);
        }
        ptr::copy_nonoverlapping(extended.data, v.mft_runs, total);
        *v.mft_count = total;
        WriteStatus::Ok
    }
}
unsafe fn rollback(v: Volume, runs: io::Runs, published: bool, status: WriteStatus) -> WriteStatus {
    unsafe {
        if !v.cache().is_null() {
            (*v.cache()).invalidate(Mutation::Recovery)
        }
        let old = Buffer {
            data: ptr::addr_of_mut!((*v.scratch()).rollback_record).cast(),
            len: v.record_bytes as usize,
        };
        if published && (!r::store(v, 0, old) || !io::flush(v.io)) {
            let _ = cleanup::uncertain(v);
            return WriteStatus::CleanupFailed;
        }
        if !c::free(v, runs) || !io::flush(v.io) {
            let _ = cleanup::uncertain(v);
            return WriteStatus::CleanupFailed;
        }
        status
    }
}
#[inline(never)]
pub(super) unsafe fn release(v: Volume, number: u64) -> bool {
    unsafe {
        let mut bitmap = AttrScratch::EMPTY;
        if r::collect(v, 0, 0xb0, View::default(), &mut bitmap) != Lookup::Found || bitmap.resident
        {
            return false;
        }
        let Some(offset) = (number / 8 / 512).checked_mul(512) else {
            return false;
        };
        let mut sector = [0u8; 512];
        let buffer = Buffer {
            data: sector.as_mut_ptr(),
            len: 512,
        };
        if !io::runs(
            v.io,
            r::attr_runs(&bitmap),
            offset,
            buffer,
            None,
            ptr::null_mut(),
        ) {
            return false;
        }
        sector[(number / 8 % 512) as usize] &= !(1u8 << (number % 8));
        if !io::runs(
            v.io,
            r::attr_runs(&bitmap),
            offset,
            Buffer {
                data: sector.as_mut_ptr(),
                len: 512,
            },
            Some(Mutation::Attribute {
                record: 0,
                kind: 0xb0,
            }),
            ptr::null_mut(),
        ) {
            return false;
        }
        (*v.scratch()).record_search_start = (*v.scratch()).record_search_start.min(number);
        true
    }
}
pub(super) unsafe fn mark_free(v: Volume, number: u64) -> bool {
    unsafe {
        if !r::load_raw(
            v,
            number,
            Buffer {
                data: ptr::addr_of_mut!((*v.scratch()).write_record).cast(),
                len: 4096,
            },
        ) {
            return false;
        }
        let record = v.write_record();
        if le32(r::view(record), 0) != 0x454c4946 {
            return false;
        }
        let flags = le16(r::view(record), 0x16) & !1;
        r::put16(r::mutable(record), 0x16, flags);
        r::store(v, number, record)
    }
}
