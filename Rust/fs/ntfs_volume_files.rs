//! Original file create/overwrite/delete, durable publication and ownership.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_build as build,
    ntfs_volume_build_names as names, ntfs_volume_cleanup as cleanup,
    ntfs_volume_clusters as clusters, ntfs_volume_edit as edit, ntfs_volume_extension as extension,
    ntfs_volume_hardlinks as links, ntfs_volume_index_insert as insert,
    ntfs_volume_index_remove as remove, ntfs_volume_io as io, ntfs_volume_lookup as lookup,
    ntfs_volume_mft as mft, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;
#[inline(never)]
pub(super) unsafe fn create(v: Volume, parent: u64, name: View, data: View) -> WriteStatus {
    unsafe {
        if !names::valid(name.get()) {
            return WriteStatus::Invalid;
        }
        let mut existing = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut existing, false, 0) {
            Lookup::Found => return WriteStatus::Exists,
            Lookup::Io => return WriteStatus::Io,
            Lookup::NotFound => {}
        }
        let Some(sequence) = lookup::sequence(v, parent) else {
            return WriteStatus::Io;
        };
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        let resident = data.len <= 0x2a0;
        let mut runs = [Run::default(); 24];
        let mut count = 0;
        let mut allocation = (data.len as u64 + 7) & !7u64;
        if !resident && data.len > 0 {
            let cluster = u64::from(v.geometry().cluster_bytes);
            if cluster == 0 {
                return cleanup::abort(v, WriteStatus::Io);
            }
            let Some(rounded) = (data.len as u64).checked_add(cluster - 1) else {
                return cleanup::abort(v, WriteStatus::Invalid);
            };
            let n = rounded / cluster;
            let result = clusters::allocate(
                v,
                n,
                clusters::RunBuffer {
                    data: runs.as_mut_ptr(),
                    len: 24,
                },
            );
            if result.status != WriteStatus::Ok {
                return cleanup::abort(v, result.status);
            }
            count = result.produced;
            let span = io::Runs {
                data: runs.as_ptr(),
                len: count,
            };
            let Some(bytes) = n.checked_mul(cluster) else {
                return cleanup::freeing(v, span, WriteStatus::Io);
            };
            allocation = bytes;
            if !cleanup::budgeted_flush(v) {
                return cleanup::freeing(v, span, WriteStatus::Io);
            }
        }
        let span = io::Runs {
            data: runs.as_ptr(),
            len: count,
        };
        let slot = mft::allocate(v);
        if slot.status != WriteStatus::Ok {
            return cleanup::freeing(v, span, slot.status);
        }
        if !cleanup::budgeted_flush(v) {
            return cleanup::releasing(v, slot.number, false, span, WriteStatus::Io);
        }
        if !resident && data.len > 0 {
            if !io::runs(
                v.io,
                span,
                0,
                Buffer {
                    data: data.data.cast_mut(),
                    len: data.len,
                },
                Some(Mutation::Payload),
                ptr::null_mut(),
            ) {
                return cleanup::releasing(v, slot.number, false, span, WriteStatus::Io);
            }
            let tail = allocation - data.len as u64;
            let zeros = [0u8; 512];
            let mut written = 0;
            while written < tail {
                let n = (tail - written).min(512);
                if !io::runs(
                    v.io,
                    span,
                    data.len as u64 + written,
                    Buffer {
                        data: zeros.as_ptr().cast_mut(),
                        len: n as usize,
                    },
                    Some(Mutation::Payload),
                    ptr::null_mut(),
                ) {
                    return cleanup::releasing(v, slot.number, false, span, WriteStatus::Io);
                }
                written += n
            }
            if !cleanup::budgeted_flush(v) {
                return cleanup::releasing(v, slot.number, false, span, WriteStatus::Io);
            }
        }
        if build::file(
            v,
            slot.number,
            slot.sequence,
            parent,
            sequence,
            name,
            if resident { Some(data) } else { None },
            data.len as u64,
            allocation,
        ) == 0
        {
            return cleanup::releasing(v, slot.number, false, span, WriteStatus::Invalid);
        }
        if !resident && data.len > 0 {
            let Some(mut h) = fmt::file_header(r::view(v.write_record())) else {
                return cleanup::releasing(v, slot.number, false, span, WriteStatus::Io);
            };
            if !edit::nonresident(
                r::mutable(v.write_record()),
                &mut h,
                0x80,
                &[],
                &runs[..count],
                data.len as u64,
                data.len as u64,
                allocation,
            ) {
                return cleanup::releasing(v, slot.number, false, span, WriteStatus::RecordFull);
            }
        }
        if !r::store(v, slot.number, v.write_record()) || !cleanup::budgeted_flush(v) {
            return WriteStatus::Io;
        }
        let mut key = [0u8; 16 + 0x42 + 510];
        let Some(n) = names::key(
            v,
            &mut key,
            parent,
            sequence,
            name.get(),
            false,
            allocation,
            data.len as u64,
        ) else {
            return cleanup::releasing(v, slot.number, true, span, WriteStatus::Invalid);
        };
        let reference = (FileReference {
            record: slot.number,
            sequence: slot.sequence,
        })
        .pack();
        let status = insert::insert(v, parent, reference, View::of(&key[..n]));
        if status != WriteStatus::Ok {
            if matches!(status, WriteStatus::Io | WriteStatus::CleanupFailed) {
                return status;
            }
            return cleanup::releasing(v, slot.number, true, span, status);
        }
        if !cleanup::budgeted_flush(v) || !r::set_dirty(v, false) {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
#[inline(never)]
pub(super) unsafe fn write(v: Volume, parent: u64, name: View, data: View) -> WriteStatus {
    unsafe {
        if !names::valid(name.get()) {
            return WriteStatus::Invalid;
        }
        let mut found = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut found, false, 0) {
            Lookup::NotFound => return create(v, parent, name, data),
            Lookup::Io => return WriteStatus::Io,
            Lookup::Found => {}
        }
        if found.entry.is_dir() {
            return WriteStatus::Directory;
        }
        if !lookup::live(v, found) {
            return WriteStatus::Io;
        }
        let status = lookup::single_link(v, found.record, found.sequence);
        if status != WriteStatus::Ok {
            return status;
        }
        let status = delete(v, parent, name);
        if status != WriteStatus::Ok {
            status
        } else {
            create(v, parent, name, data)
        }
    }
}
#[inline(never)]
pub(super) unsafe fn delete(v: Volume, parent: u64, name: View) -> WriteStatus {
    unsafe {
        if !names::valid(name.get()) {
            return WriteStatus::Invalid;
        }
        let mut found = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut found, false, 0) {
            Lookup::NotFound => return WriteStatus::NotFound,
            Lookup::Io => return WriteStatus::Io,
            Lookup::Found => {}
        }
        if found.entry.is_dir() {
            return WriteStatus::Directory;
        }
        if !lookup::live(v, found) {
            return WriteStatus::Io;
        }
        let Some(h) = r::load(v, found.record, v.part_record()) else {
            return WriteStatus::Io;
        };
        if h.sequence != found.sequence {
            return WriteStatus::Io;
        }
        if h.link_count > 1 {
            return links::delete(v, parent, name, found.record, found.sequence);
        }
        release(v, parent, name, found)
    }
}
#[inline(never)]
pub(super) unsafe fn release(
    v: Volume,
    parent: u64,
    name: View,
    found: LookupResult,
) -> WriteStatus {
    unsafe {
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        let data = ptr::addr_of_mut!((*v.scratch()).attr_op);
        if r::collect(v, found.record, 0x80, View::default(), data) != Lookup::Found {
            return WriteStatus::Io;
        }
        let mut identity = extension::Identity {
            record: 0,
            sequence: 0,
        };
        let status = extension::identity(v, found.record, &mut identity);
        if status == Lookup::Io {
            return WriteStatus::Io;
        }
        let ext = if status == Lookup::Found {
            Some(identity)
        } else {
            None
        };
        let reference = (FileReference {
            record: found.record,
            sequence: found.sequence,
        })
        .pack();
        let status = remove::remove(v, parent, name, Some(reference));
        if status != WriteStatus::Ok {
            return status;
        }
        if !cleanup::budgeted_flush(v) {
            return WriteStatus::Io;
        }
        if !retire_record(v, found.record, found.sequence)
            || !mft::release(v, found.record)
            || !cleanup::budgeted_flush(v)
        {
            return WriteStatus::Io;
        }
        if let Some(ext) = ext
            && (!retire_record(v, ext.record, ext.sequence)
                || !mft::release(v, ext.record)
                || !cleanup::budgeted_flush(v))
        {
            return WriteStatus::Io;
        }
        if !(*data).resident
            && (!clusters::free(v, r::attr_runs(data)) || !cleanup::budgeted_flush(v))
        {
            return WriteStatus::Io;
        }
        if !r::set_dirty(v, false) {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
pub(super) unsafe fn retire_record(v: Volume, number: u64, sequence: u16) -> bool {
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
        if le32(r::view(record), 0) != 0x454c4946
            || le16(r::view(record), 16) != sequence
            || le16(r::view(record), 22) & 1 == 0
        {
            return false;
        }
        let flags = le16(r::view(record), 22) & !1;
        r::put16(r::mutable(record), 22, flags);
        r::put16(
            r::mutable(record),
            16,
            if sequence == u16::MAX {
                1
            } else {
                sequence + 1
            },
        );
        r::store(v, number, record)
    }
}
