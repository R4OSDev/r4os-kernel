//! Original durable/deferred append phases, including resident conversion.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_cleanup as cleanup,
    ntfs_volume_clusters as clusters, ntfs_volume_edit as edit, ntfs_volume_extension as extension,
    ntfs_volume_index_update as index, ntfs_volume_io as io, ntfs_volume_lookup as lookup,
    ntfs_volume_names as diag, ntfs_volume_records as r, ntfs_volume_sparse as sparse,
    ntfs_volume_types::*,
};
use core::ptr;
pub(super) unsafe fn finish(v: Volume) -> bool {
    unsafe {
        if !io::flush(v.io) {
            return false;
        }
        r::set_dirty(v, false)
    }
}
#[inline(never)]
pub(super) unsafe fn append(
    v: Volume,
    parent: u64,
    name: View,
    expected: u64,
    data: View,
    durable: bool,
) -> WriteStatus {
    unsafe {
        diag::append_at(1);
        if !super::ntfs_format_names::valid_name(name.get()) {
            return WriteStatus::Invalid;
        }
        let mut found = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut found, false, 0) {
            Lookup::NotFound => return WriteStatus::NotFound,
            Lookup::Io => return WriteStatus::Io,
            Lookup::Found => {}
        }
        diag::append_at(2);
        if found.entry.is_dir() {
            return WriteStatus::Directory;
        }
        if !lookup::live(v, found) {
            return WriteStatus::Io;
        }
        diag::append_at(3);
        let attr = ptr::addr_of_mut!((*v.scratch()).attr_op);
        if r::collect(v, found.record, 0x80, View::default(), attr) != Lookup::Found {
            return WriteStatus::Io;
        }
        if (*attr).data_size != expected {
            return WriteStatus::OffsetMismatch;
        }
        if (*attr).flags & (1 | 0x4000) != 0 {
            return WriteStatus::Unsupported;
        }
        if data.len == 0 {
            diag::append_at(0);
            return WriteStatus::Ok;
        }
        diag::append_at(4);
        let status = lookup::single_link(v, found.record, found.sequence);
        if status != WriteStatus::Ok {
            return status;
        }
        let Some(total) = (*attr).data_size.checked_add(data.len as u64) else {
            return WriteStatus::Invalid;
        };
        diag::append_at(5);
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        if (*attr).resident {
            if total <= 0x2a0 {
                diag::append_at(6);
                let mut value = [0u8; 0x2a0];
                let n = (*attr).resident_len;
                assert!(n <= value.len());
                ptr::copy_nonoverlapping(
                    ptr::addr_of!((*attr).resident_copy).cast::<u8>(),
                    value.as_mut_ptr(),
                    n,
                );
                assert!(n + data.len <= value.len());
                ptr::copy_nonoverlapping(data.data, value.as_mut_ptr().add(n), data.len);
                let Some(mut h) = r::load(v, found.record, v.write_record()) else {
                    return WriteStatus::Io;
                };
                let record = v.write_record();
                if edit::resident(
                    r::mutable(record),
                    &mut h,
                    0x80,
                    &[],
                    &value[..total as usize],
                ) {
                    let allocation = (total + 7) & !7u64;
                    let _ = edit::filename_dup(
                        r::mutable(record),
                        h,
                        allocation,
                        total,
                        v.now_filetime,
                    );
                    if !r::store(v, found.record, record)
                        || (durable && !cleanup::budgeted_flush(v))
                        || !index::update(v, parent, name, allocation, total, v.now_filetime)
                        || (durable && !cleanup::budgeted_flush(v))
                        || (durable && !r::set_dirty(v, false))
                    {
                        return WriteStatus::Io;
                    }
                    diag::append_at(0);
                    return WriteStatus::Ok;
                }
            }
            diag::append_at(7);
            let status = convert(v, parent, name, found.record, attr, data, total, durable);
            if status == WriteStatus::Ok {
                diag::append_at(0)
            }
            return status;
        }
        if (*attr).flags & 0x8000 != 0 {
            diag::append_at(8);
            if (*attr).initialized_size < (*attr).data_size
                && !sparse::zero_mapped(
                    v,
                    r::attr_runs(attr),
                    (*attr).initialized_size,
                    (*attr).data_size,
                )
            {
                return WriteStatus::Io;
            }
            let end = total.min((*attr).alloc_size);
            if end > (*attr).data_size {
                let status = sparse::fill(v, attr, (*attr).data_size, end - (*attr).data_size);
                if status != WriteStatus::Ok {
                    return cleanup::abort(v, status);
                }
            }
        }
        if total <= (*attr).alloc_size {
            diag::append_at(9);
            if !io::runs(
                v.io,
                r::attr_runs(attr),
                (*attr).data_size,
                Buffer {
                    data: data.data.cast_mut(),
                    len: data.len,
                },
                Some(Mutation::Payload),
                ptr::null_mut(),
            ) || (durable && !cleanup::budgeted_flush(v))
            {
                return WriteStatus::Io;
            }
            diag::append_at(10);
            let runs = r::attr_runs(attr);
            let status = extension::commit(
                v,
                found.record,
                runs.data,
                runs.len,
                total,
                total,
                (*attr).alloc_size,
            );
            if status != WriteStatus::Ok {
                return cleanup::after_commit(v, status);
            }
            if durable && !cleanup::budgeted_flush(v) {
                return WriteStatus::Io;
            }
            diag::append_at(11);
            if !filename_on_base(v, found.record, (*attr).alloc_size, total)
                || (durable && !cleanup::budgeted_flush(v))
            {
                return WriteStatus::Io;
            }
            diag::append_at(12);
            if !index::update(v, parent, name, (*attr).alloc_size, total, v.now_filetime)
                || (durable && !cleanup::budgeted_flush(v))
                || (durable && !r::set_dirty(v, false))
            {
                return WriteStatus::Io;
            }
            diag::append_at(0);
            return WriteStatus::Ok;
        }
        diag::append_at(13);
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 {
            return cleanup::abort(v, WriteStatus::Io);
        }
        let Some(rounded) = total.checked_add(cluster - 1) else {
            return cleanup::abort(v, WriteStatus::Invalid);
        };
        let required = rounded / cluster;
        let allocated = (*attr).alloc_size / cluster;
        if required <= allocated {
            return cleanup::abort(v, WriteStatus::Io);
        }
        let needed = required - allocated;
        if (*attr).count + 8 > 256 {
            return cleanup::abort(v, WriteStatus::RecordFull);
        }
        let mut runs = [Run::default(); 8];
        let result = clusters::allocate(
            v,
            needed,
            clusters::RunBuffer {
                data: runs.as_mut_ptr(),
                len: 8,
            },
        );
        if result.status != WriteStatus::Ok {
            return cleanup::abort(v, result.status);
        }
        let fresh = io::Runs {
            data: runs.as_ptr(),
            len: result.produced,
        };
        if durable && !cleanup::budgeted_flush(v) {
            return cleanup::freeing(v, fresh, WriteStatus::Io);
        }
        let mut count = (*attr).count;
        for run in &runs[..result.produced] {
            let adjacent = if count > 0 && (*attr).runs[count - 1].has_lcn && run.has_lcn {
                let Some(end) = (*attr).runs[count - 1]
                    .lcn
                    .checked_add((*attr).runs[count - 1].length_clusters)
                else {
                    return cleanup::freeing(v, fresh, WriteStatus::Io);
                };
                end == run.lcn
            } else {
                false
            };
            if adjacent {
                let Some(n) = (*attr).runs[count - 1]
                    .length_clusters
                    .checked_add(run.length_clusters)
                else {
                    return cleanup::freeing(v, fresh, WriteStatus::Io);
                };
                (*attr).runs[count - 1].length_clusters = n
            } else {
                if count >= 256 {
                    return cleanup::freeing(v, fresh, WriteStatus::RecordFull);
                }
                (*attr).runs[count] = *run;
                count += 1
            }
        }
        let all = io::Runs {
            data: ptr::addr_of!((*attr).runs).cast::<Run>(),
            len: count,
        };
        if !io::runs_valid(v.io, all) {
            return cleanup::freeing(v, fresh, WriteStatus::Io);
        }
        let Some(bytes) = needed.checked_mul(cluster) else {
            return cleanup::freeing(v, fresh, WriteStatus::Io);
        };
        let Some(allocation) = (*attr).alloc_size.checked_add(bytes) else {
            return cleanup::freeing(v, fresh, WriteStatus::Io);
        };
        diag::append_at(14);
        if !io::runs(
            v.io,
            all,
            (*attr).data_size,
            Buffer {
                data: data.data.cast_mut(),
                len: data.len,
            },
            Some(Mutation::Payload),
            ptr::null_mut(),
        ) || (durable && !cleanup::budgeted_flush(v))
        {
            return cleanup::freeing(v, fresh, WriteStatus::Io);
        }
        diag::append_at(15);
        let status =
            extension::commit(v, found.record, all.data, all.len, total, total, allocation);
        if status != WriteStatus::Ok {
            if matches!(status, WriteStatus::RecordFull | WriteStatus::NoSpace) {
                return cleanup::freeing(v, fresh, status);
            }
            return status;
        }
        if durable && !cleanup::budgeted_flush(v) {
            return WriteStatus::Io;
        }
        diag::append_at(16);
        if !filename_on_base(v, found.record, allocation, total)
            || (durable && !cleanup::budgeted_flush(v))
        {
            return WriteStatus::Io;
        }
        diag::append_at(17);
        if !index::update(v, parent, name, allocation, total, v.now_filetime)
            || (durable && !cleanup::budgeted_flush(v))
            || (durable && !r::set_dirty(v, false))
        {
            return WriteStatus::Io;
        }
        diag::append_at(0);
        WriteStatus::Ok
    }
}
unsafe fn filename_on_base(v: Volume, base: u64, allocation: u64, data: u64) -> bool {
    unsafe {
        diag::append_at(161);
        let Some(h) = r::load(v, base, v.write_record()) else {
            return false;
        };
        let record = v.write_record();
        let _ = edit::filename_dup(r::mutable(record), h, allocation, data, v.now_filetime);
        diag::append_at(162);
        r::store(v, base, record)
    }
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
unsafe fn convert(
    v: Volume,
    parent: u64,
    name: View,
    number: u64,
    attr: *const AttrScratch,
    data: View,
    total: u64,
    durable: bool,
) -> WriteStatus {
    unsafe {
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 {
            return cleanup::abort(v, WriteStatus::Io);
        }
        let Some(rounded) = total.checked_add(cluster - 1) else {
            return cleanup::abort(v, WriteStatus::Invalid);
        };
        let count = rounded / cluster;
        let mut runs = [Run::default(); 24];
        let result = clusters::allocate(
            v,
            count,
            clusters::RunBuffer {
                data: runs.as_mut_ptr(),
                len: 24,
            },
        );
        if result.status != WriteStatus::Ok {
            return cleanup::abort(v, result.status);
        }
        let span = io::Runs {
            data: runs.as_ptr(),
            len: result.produced,
        };
        let Some(bytes) = count.checked_mul(cluster) else {
            return cleanup::freeing(v, span, WriteStatus::Io);
        };
        if durable && !cleanup::budgeted_flush(v) {
            return cleanup::freeing(v, span, WriteStatus::Io);
        }
        if (*attr).resident_len > 0
            && !io::runs(
                v.io,
                span,
                0,
                Buffer {
                    data: ptr::addr_of!((*attr).resident_copy).cast::<u8>().cast_mut(),
                    len: (*attr).resident_len,
                },
                Some(Mutation::Payload),
                ptr::null_mut(),
            )
        {
            return cleanup::freeing(v, span, WriteStatus::Io);
        }
        if !io::runs(
            v.io,
            span,
            (*attr).resident_len as u64,
            Buffer {
                data: data.data.cast_mut(),
                len: data.len,
            },
            Some(Mutation::Payload),
            ptr::null_mut(),
        ) || (durable && !cleanup::budgeted_flush(v))
        {
            return cleanup::freeing(v, span, WriteStatus::Io);
        }
        let Some(_h) = r::load(v, number, v.write_record()) else {
            return cleanup::freeing(v, span, WriteStatus::Io);
        };
        let record = v.write_record();
        if !edit::remove(r::mutable(record), 0x80, &[]) {
            return cleanup::freeing(v, span, WriteStatus::Io);
        }
        let mut stub = [0u8; 0x48];
        r::put32(&mut stub, 0, 0x80);
        r::put32(&mut stub, 4, 0x48);
        stub[8] = 1;
        r::put16(&mut stub, 0x20, 0x40);
        r::put64(&mut stub, 0x28, bytes);
        r::put64(&mut stub, 0x30, total);
        r::put64(&mut stub, 0x38, total);
        if !edit::insert(r::mutable(record), &stub) {
            return cleanup::freeing(v, span, WriteStatus::Io);
        }
        let Some(mut h) = fmt::file_header(r::view(record)) else {
            return cleanup::freeing(v, span, WriteStatus::Io);
        };
        if !edit::nonresident(
            r::mutable(record),
            &mut h,
            0x80,
            &[],
            &runs[..result.produced],
            total,
            total,
            bytes,
        ) {
            return cleanup::freeing(v, span, WriteStatus::RecordFull);
        }
        let _ = edit::filename_dup(r::mutable(record), h, bytes, total, v.now_filetime);
        if !r::store(v, number, record)
            || (durable && !cleanup::budgeted_flush(v))
            || !index::update(v, parent, name, bytes, total, v.now_filetime)
            || (durable && !cleanup::budgeted_flush(v))
            || (durable && !r::set_dirty(v, false))
        {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
