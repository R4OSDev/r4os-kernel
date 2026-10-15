//! Original sparse extent expansion, zeroing and confirmed write progress.
use super::{
    ntfs_format_types::*, ntfs_volume_cleanup as cleanup, ntfs_volume_clusters as clusters,
    ntfs_volume_edit as edit, ntfs_volume_extension as extension, ntfs_volume_io as io,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
static ZERO: [u8; 4096] = [0; 4096];
fn bytes(n: usize) -> Buffer {
    assert!(n <= 4096);
    Buffer {
        data: ZERO.as_ptr().cast_mut(),
        len: n,
    }
}
pub fn push(out: &mut [Run], count: &mut usize, run: Run) -> bool {
    if run.length_clusters == 0 {
        return true;
    }
    if *count > 0 {
        let old = &mut out[*count - 1];
        if !old.has_lcn && !run.has_lcn {
            let Some(n) = old.length_clusters.checked_add(run.length_clusters) else {
                return false;
            };
            old.length_clusters = n;
            return true;
        }
        if old.has_lcn && run.has_lcn {
            let Some(end) = old.lcn.checked_add(old.length_clusters) else {
                return false;
            };
            if end == run.lcn {
                let Some(n) = old.length_clusters.checked_add(run.length_clusters) else {
                    return false;
                };
                old.length_clusters = n;
                return true;
            }
        }
    }
    if *count >= out.len() {
        return false;
    }
    out[*count] = run;
    *count += 1;
    true
}
pub(super) unsafe fn zero_run(v: Volume, run: Run) -> bool {
    unsafe {
        if !run.has_lcn {
            return true;
        }
        if !io::runs_valid(v.io, io::Runs { data: &run, len: 1 }) {
            return false;
        }
        let Some(total) = run
            .length_clusters
            .checked_mul(u64::from(v.geometry().cluster_bytes))
        else {
            return false;
        };
        let mut pos = 0;
        while pos < total {
            let n = (total - pos).min(4096);
            if !io::lcn(
                v.io,
                run.lcn,
                pos,
                bytes(n as usize),
                Some(Mutation::Payload),
                ptr::null_mut(),
            ) {
                return false;
            }
            pos += n
        }
        true
    }
}
pub(super) unsafe fn zero_mapped(v: Volume, runs: io::Runs, from: u64, to: u64) -> bool {
    unsafe {
        if to <= from {
            return true;
        }
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 || !io::runs_valid(v.io, runs) {
            return false;
        }
        let mut start = 0u64;
        for i in 0..runs.len {
            let run = runs.get(i);
            let Some(n) = run.length_clusters.checked_mul(cluster) else {
                return false;
            };
            let Some(end) = start.checked_add(n) else {
                return false;
            };
            if start >= to {
                break;
            }
            if run.has_lcn && end > from {
                let mut pos = start.max(from);
                let stop = end.min(to);
                while pos < stop {
                    let n = (stop - pos).min(4096);
                    if !io::lcn(
                        v.io,
                        run.lcn,
                        pos - start,
                        bytes(n as usize),
                        Some(Mutation::Payload),
                        ptr::null_mut(),
                    ) {
                        return false;
                    }
                    pos += n
                }
            }
            start = end
        }
        true
    }
}
pub(super) unsafe fn overlaps(v: Volume, runs: io::Runs, offset: u64, len: u64) -> Option<bool> {
    unsafe {
        if len == 0 {
            return Some(false);
        }
        let end = offset.checked_add(len)?;
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 || !io::runs_valid(v.io, runs) {
            return None;
        }
        let mut start = 0u64;
        for i in 0..runs.len {
            let run = runs.get(i);
            let bytes = run.length_clusters.checked_mul(cluster)?;
            let re = start.checked_add(bytes)?;
            if start >= end {
                break;
            }
            if !run.has_lcn && re > offset {
                return Some(true);
            }
            start = re
        }
        Some(false)
    }
}
#[inline(never)]
pub(super) unsafe fn fill(v: Volume, attr: *mut AttrScratch, offset: u64, len: u64) -> WriteStatus {
    unsafe {
        if len == 0 {
            return WriteStatus::Ok;
        }
        let cluster = u64::from(v.geometry().cluster_bytes);
        if cluster == 0 || !io::runs_valid(v.io, r::attr_runs(attr)) {
            return WriteStatus::Io;
        }
        let Some(end) = offset.checked_add(len) else {
            return WriteStatus::Io;
        };
        let first = offset / cluster;
        let last = (end - 1) / cluster;
        let mut out = [Run::default(); 256];
        let mut out_count = 0;
        let mut fresh = [Run::default(); 32];
        let mut fresh_count = 0;
        let mut status = WriteStatus::Ok;
        let mut changed = false;
        let mut vcn = 0u64;
        'build: for i in 0..(*attr).count {
            let run = (*attr).runs[i];
            let Some(end) = vcn.checked_add(run.length_clusters) else {
                status = WriteStatus::Io;
                break;
            };
            if run.has_lcn || end <= first || vcn > last {
                if !push(&mut out, &mut out_count, run) {
                    status = WriteStatus::RecordFull;
                    break;
                }
                vcn = end;
                continue;
            }
            let from = vcn.max(first);
            let to = (end - 1).min(last);
            if !push(
                &mut out,
                &mut out_count,
                Run {
                    length_clusters: from - vcn,
                    has_lcn: false,
                    lcn: 0,
                },
            ) {
                status = WriteStatus::RecordFull;
                break;
            }
            let mut need = to - from + 1;
            while need > 0 {
                if fresh_count + 8 > 32 {
                    status = WriteStatus::RecordFull;
                    break 'build;
                }
                let mut runs = [Run::default(); 8];
                let result = clusters::allocate(
                    v,
                    need,
                    clusters::RunBuffer {
                        data: runs.as_mut_ptr(),
                        len: 8,
                    },
                );
                if result.status != WriteStatus::Ok {
                    status = result.status;
                    break 'build;
                }
                for run in &runs[..result.produced] {
                    fresh[fresh_count] = *run;
                    fresh_count += 1;
                    need -= run.length_clusters;
                    if !zero_run(v, *run) {
                        status = WriteStatus::Io;
                        break 'build;
                    }
                    if !push(&mut out, &mut out_count, *run) {
                        status = WriteStatus::RecordFull;
                        break 'build;
                    }
                }
            }
            if !push(
                &mut out,
                &mut out_count,
                Run {
                    length_clusters: (end - 1) - to,
                    has_lcn: false,
                    lcn: 0,
                },
            ) {
                status = WriteStatus::RecordFull;
                break;
            }
            changed = true;
            vcn = end
        }
        if status != WriteStatus::Ok {
            if fresh_count != 0
                && !clusters::free(
                    v,
                    io::Runs {
                        data: fresh.as_ptr(),
                        len: fresh_count,
                    },
                )
            {
                return WriteStatus::CleanupFailed;
            }
            return status;
        }
        if !changed {
            return WriteStatus::Ok;
        }
        ptr::copy_nonoverlapping(
            out.as_ptr(),
            ptr::addr_of_mut!((*attr).runs).cast::<Run>(),
            out_count,
        );
        (*attr).count = out_count;
        WriteStatus::Ok
    }
}
fn progress(status: WriteStatus, completed: usize, uncertain: bool) -> WriteProgress {
    WriteProgress {
        status,
        completed,
        uncertain,
    }
}
#[inline(never)]
pub(super) unsafe fn write_at(v: Volume, number: u64, offset: u64, data: View) -> WriteProgress {
    unsafe {
        if (*v.scratch()).has_mirror_bootstrap_lcn {
            return progress(WriteStatus::Unsupported, 0, false);
        }
        if (*v.scratch()).repair_required {
            return progress(WriteStatus::CleanupFailed, 0, true);
        }
        if data.len == 0 {
            return progress(WriteStatus::Ok, 0, false);
        }
        let Some(end) = offset.checked_add(data.len as u64) else {
            return progress(WriteStatus::OffsetMismatch, 0, false);
        };
        let attr = ptr::addr_of_mut!((*v.scratch()).attr_op);
        if r::collect(v, number, 0x80, View::default(), attr) != Lookup::Found {
            return progress(WriteStatus::Io, 0, false);
        }
        if (*attr).flags & (1 | 0x4000) != 0 {
            return progress(WriteStatus::Unsupported, 0, false);
        }
        if end > (*attr).data_size {
            return progress(WriteStatus::OffsetMismatch, 0, false);
        }
        if (*attr).resident {
            let Some(previously_dirty) = r::is_dirty(v) else {
                return progress(WriteStatus::Io, 0, false);
            };
            if !r::ensure_dirty(v) {
                return progress(cleanup::uncertain(v), 0, true);
            }
            let Some(mut h) = r::load(v, number, v.write_record()) else {
                return progress(WriteStatus::Io, 0, false);
            };
            let record = v.write_record();
            let saved = Buffer {
                data: ptr::addr_of_mut!((*v.scratch()).rollback_record).cast::<u8>(),
                len: record.len,
            };
            ptr::copy_nonoverlapping(record.data, saved.data, record.len);
            let mut value = [0u8; 0x2a0];
            let n = (*attr).resident_len;
            if n > value.len() {
                return progress(WriteStatus::Io, 0, false);
            }
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*attr).resident_copy).cast::<u8>(),
                value.as_mut_ptr(),
                n,
            );
            assert!(offset as usize <= value.len() && end as usize <= value.len());
            ptr::copy_nonoverlapping(data.data, value.as_mut_ptr().add(offset as usize), data.len);
            if !edit::resident(r::mutable(record), &mut h, 0x80, &[], &value[..n]) {
                return progress(WriteStatus::Io, 0, false);
            }
            if !r::store(v, number, record) || !io::flush(v.io) {
                if !r::store(v, number, saved) || !io::flush(v.io) {
                    let _ = cleanup::uncertain(v);
                    return progress(WriteStatus::CleanupFailed, 0, true);
                }
                if !previously_dirty && !r::set_dirty(v, false) {
                    return progress(cleanup::uncertain(v), 0, true);
                }
                return progress(WriteStatus::Io, 0, false);
            }
            if !previously_dirty && !r::set_dirty(v, false) {
                return progress(cleanup::uncertain(v), data.len, true);
            }
            return progress(WriteStatus::Ok, data.len, false);
        }
        let sparse = (*attr).flags & 0x8000 != 0;
        if end > (*attr).initialized_size && !sparse {
            return progress(WriteStatus::OffsetMismatch, 0, false);
        }
        let Some(hole) = overlaps(v, r::attr_runs(attr), offset, data.len as u64) else {
            return progress(WriteStatus::Io, 0, false);
        };
        let needs = sparse && (hole || end > (*attr).initialized_size);
        let mutation = if number == 6 {
            Mutation::Attribute {
                record: 6,
                kind: 0x80,
            }
        } else {
            Mutation::Payload
        };
        let mut done = 0;
        let buffer = Buffer {
            data: data.data.cast_mut(),
            len: data.len,
        };
        if !needs {
            if !io::runs(
                v.io,
                r::attr_runs(attr),
                offset,
                buffer,
                Some(mutation),
                &mut done,
            ) {
                return progress(cleanup::uncertain(v), done, true);
            }
            return progress(WriteStatus::Ok, done, false);
        }
        if !r::ensure_dirty(v) {
            return progress(cleanup::uncertain(v), 0, true);
        }
        let status = fill(v, attr, offset, data.len as u64);
        if status != WriteStatus::Ok {
            return progress(cleanup::abort(v, status), 0, false);
        }
        if !cleanup::budgeted_flush(v) {
            return progress(cleanup::uncertain(v), 0, true);
        }
        if offset > (*attr).initialized_size
            && !zero_mapped(v, r::attr_runs(attr), (*attr).initialized_size, offset)
        {
            return progress(cleanup::uncertain(v), 0, true);
        }
        if !io::runs(
            v.io,
            r::attr_runs(attr),
            offset,
            buffer,
            Some(mutation),
            &mut done,
        ) || !cleanup::budgeted_flush(v)
        {
            return progress(cleanup::uncertain(v), 0, true);
        }
        let initialized = end.max((*attr).initialized_size);
        let runs = r::attr_runs(attr);
        let status = extension::commit(
            v,
            number,
            runs.data,
            runs.len,
            (*attr).data_size,
            initialized,
            (*attr).alloc_size,
        );
        if status != WriteStatus::Ok {
            let _ = cleanup::uncertain(v);
            return progress(status, 0, true);
        }
        if !cleanup::budgeted_flush(v) || !r::set_dirty(v, false) {
            return progress(cleanup::uncertain(v), 0, true);
        }
        progress(WriteStatus::Ok, data.len, false)
    }
}
