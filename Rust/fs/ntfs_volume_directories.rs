//! Original directory record construction and ordered create/delete.
use super::{
    ntfs_format_types::*, ntfs_volume_build as build, ntfs_volume_build_names as names,
    ntfs_volume_cleanup as cleanup, ntfs_volume_clusters as clusters,
    ntfs_volume_directory as directory, ntfs_volume_files as files,
    ntfs_volume_index_insert as insert, ntfs_volume_index_io as idx,
    ntfs_volume_index_remove as remove, ntfs_volume_io as io, ntfs_volume_lookup as lookup,
    ntfs_volume_mft as mft, ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::ptr;

#[inline(never)]
unsafe fn build_directory(
    v: Volume,
    number: u64,
    sequence: u16,
    parent: u64,
    parent_sequence: u16,
    name: View,
) -> usize {
    unsafe {
        let b = r::mutable(v.write_record());
        b.fill(0);
        r::put32(b, 0, 0x454c4946);
        r::put16(b, 4, 0x30);
        r::put16(
            b,
            6,
            u16::try_from(v.record_bytes / 512 + 1).expect("NTFS USA"),
        );
        r::put16(b, 16, sequence);
        r::put16(b, 18, 1);
        r::put16(b, 20, 0x38);
        r::put16(b, 22, 3);
        r::put32(b, 28, v.record_bytes);
        r::put32(b, 44, u32::try_from(number).expect("NTFS record number"));
        let mut offset = 0x38;
        let mut instance = 0;
        let mut si = [0u8; 0x48];
        for t in (0..32).step_by(8) {
            r::put64(&mut si, t, v.now_filetime);
        }
        r::put32(&mut si, 32, 0);
        r::put32(&mut si, 0x34, v.security_id_dir);
        offset += build::emit(b, offset, 0x10, &[], false, &si, 0, &mut instance);
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = names::value(
            v,
            &mut filename,
            parent,
            parent_sequence,
            name.get(),
            true,
            0,
            0,
            0,
        ) else {
            return 0;
        };
        offset += build::emit(b, offset, 0x30, &[], true, &filename[..n], 0, &mut instance);
        let mut root = [0u8; 0x30];
        r::put32(&mut root, 0, 0x30);
        r::put32(&mut root, 4, 1);
        r::put32(&mut root, 8, v.index_block_bytes);
        root[12] = 1;
        r::put32(&mut root, 16, 16);
        r::put32(&mut root, 20, 32);
        r::put32(&mut root, 24, 32);
        r::put16(&mut root, 40, 16);
        r::put16(&mut root, 44, 2);
        offset += build::emit(b, offset, 0x90, &idx::I30, false, &root, 0, &mut instance);
        r::put32(b, offset, u32::MAX);
        r::put32(b, offset + 4, 0);
        offset += 8;
        r::put16(b, 0x28, instance);
        r::put32(b, 0x18, offset as u32);
        offset
    }
}

#[inline(never)]
pub(super) unsafe fn create(v: Volume, parent: u64, name: View) -> WriteStatus {
    unsafe {
        if name.len == 0 || name.len > NAME_MAX {
            return WriteStatus::Invalid;
        }
        if v.geometry().cluster_bytes != v.index_block_bytes {
            return WriteStatus::Unsupported;
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
        let slot = mft::allocate(v);
        if slot.status != WriteStatus::Ok {
            return cleanup::abort(v, slot.status);
        }
        let empty = io::Runs {
            data: ptr::null(),
            len: 0,
        };
        if !cleanup::budgeted_flush(v) {
            return cleanup::releasing(v, slot.number, false, empty, WriteStatus::Io);
        }
        if build_directory(v, slot.number, slot.sequence, parent, sequence, name) == 0 {
            return cleanup::releasing(v, slot.number, false, empty, WriteStatus::Invalid);
        }
        if !r::store(v, slot.number, v.write_record()) || !cleanup::budgeted_flush(v) {
            return WriteStatus::Io;
        }
        let mut key = [0u8; 16 + 0x42 + 510];
        let Some(n) = names::key(v, &mut key, parent, sequence, name.get(), true, 0, 0) else {
            return cleanup::releasing(v, slot.number, true, empty, WriteStatus::Invalid);
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
            return cleanup::releasing(v, slot.number, true, empty, status);
        }
        if !cleanup::budgeted_flush(v) || !r::set_dirty(v, false) {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}

#[inline(never)]
pub(super) unsafe fn delete(v: Volume, parent: u64, name: View) -> WriteStatus {
    unsafe {
        if name.len == 0 || name.len > NAME_MAX {
            return WriteStatus::Invalid;
        }
        let mut found = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut found, false, 0) {
            Lookup::NotFound => return WriteStatus::NotFound,
            Lookup::Io => return WriteStatus::Io,
            Lookup::Found => {}
        }
        if !found.entry.is_dir() {
            return WriteStatus::NotDirectory;
        }
        if !lookup::live(v, found) {
            return WriteStatus::Io;
        }
        let mut sink = directory::Sink {
            wanted: 0,
            ..directory::Sink::EMPTY
        };
        if !directory::enumerate(v, found.record, &mut sink) {
            return WriteStatus::Io;
        }
        if sink.found.is_some() {
            return WriteStatus::NotEmpty;
        }
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        let alloc = ptr::addr_of_mut!((*v.scratch()).attr_op);
        let alloc_status = r::collect(v, found.record, 0xa0, View::of(&idx::I30), alloc);
        if alloc_status == Lookup::Io {
            return WriteStatus::Io;
        }
        let status = remove::remove(v, parent, name, None);
        if status != WriteStatus::Ok {
            return status;
        }
        if !cleanup::budgeted_flush(v)
            || !files::retire_record(v, found.record, found.sequence)
            || !mft::release(v, found.record)
            || !cleanup::budgeted_flush(v)
        {
            return WriteStatus::Io;
        }
        if alloc_status == Lookup::Found
            && !(*alloc).resident
            && (!clusters::free(v, r::attr_runs(alloc)) || !cleanup::budgeted_flush(v))
        {
            return WriteStatus::Io;
        }
        if !r::set_dirty(v, false) {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
