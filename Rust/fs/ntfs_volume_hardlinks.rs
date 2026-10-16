//! Original exact parent/name/sequence hard-link detach and recovery delete.
use super::{
    ntfs_format_names as names, ntfs_format_records as fmt, ntfs_format_types::*,
    ntfs_volume_attributes as a, ntfs_volume_cleanup as cleanup,
    ntfs_volume_directory as directory, ntfs_volume_files as files,
    ntfs_volume_index_remove as remove, ntfs_volume_lookup as lookup, ntfs_volume_records as r,
    ntfs_volume_types::*,
};
#[derive(Clone, Copy)]
struct LinkSpan {
    offset: usize,
    length: usize,
}
#[allow(clippy::too_many_arguments)]
unsafe fn find(
    v: Volume,
    record: Buffer,
    h: FileRecordHeader,
    parent: u64,
    sequence: u16,
    target: View,
    out: &mut LinkSpan,
) -> Lookup {
    unsafe {
        let b = r::view(record);
        if !a::stream_valid(b, h) {
            return Lookup::Io;
        }
        let limit = h.bytes_in_use as usize;
        let mut offset = h.attrs_offset as usize;
        let mut found = false;
        while offset + 8 <= limit {
            let kind = le32(b, offset);
            if kind == u32::MAX {
                break;
            }
            let n = le32(b, offset + 4) as usize;
            if n < 24 || offset + n > limit {
                return Lookup::Io;
            }
            if kind == 0x30 {
                if b[offset + 8] != 0 {
                    return Lookup::Io;
                }
                let len = le32(b, offset + 16) as usize;
                let value = le16(b, offset + 20) as usize;
                if value < 24 || value > n || len > n - value {
                    return Lookup::Io;
                }
                let Some(name) = fmt::file_name(&b[offset + value..offset + value + len]) else {
                    return Lookup::Io;
                };
                if name.parent.record == parent
                    && name.parent.sequence == sequence
                    && names::compare(v.upcase.get(), name.name.get(), target.get()) == 0
                {
                    if found {
                        return Lookup::Io;
                    }
                    *out = LinkSpan { offset, length: n };
                    found = true
                }
            }
            offset += n
        }
        if found {
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn delete(
    v: Volume,
    parent: u64,
    name: View,
    number: u64,
    sequence: u16,
) -> WriteStatus {
    unsafe {
        let mut target = [0u8; 510];
        let Some(n) = names::to_utf16(name.get(), &mut target) else {
            return WriteStatus::Invalid;
        };
        let target = View::of(&target[..n]);
        let Some((ps, _)) = lookup::record_link(v, parent) else {
            return WriteStatus::Io;
        };
        let Some((s, count)) = lookup::record_link(v, number) else {
            return WriteStatus::Io;
        };
        if s != sequence || count <= 1 {
            return WriteStatus::Io;
        }
        let Some(h) = r::load(v, number, v.write_record()) else {
            return WriteStatus::Io;
        };
        let mut span = LinkSpan {
            offset: 0,
            length: 0,
        };
        if h.sequence != s
            || find(v, v.write_record(), h, parent, ps, target, &mut span) != Lookup::Found
        {
            return WriteStatus::Io;
        }
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        let status = remove::remove(v, parent, name, None);
        if status != WriteStatus::Ok {
            return status;
        }
        if !cleanup::budgeted_flush(v) {
            return WriteStatus::Io;
        }
        let Some(mut h) = r::load(v, number, v.write_record()) else {
            return WriteStatus::Io;
        };
        if h.sequence != s || h.link_count != count {
            return WriteStatus::Io;
        }
        let record = v.write_record();
        if find(v, record, h, parent, ps, target, &mut span) != Lookup::Found {
            return WriteStatus::Io;
        }
        let tail = span.offset + span.length;
        r::mutable(record).copy_within(tail..h.bytes_in_use as usize, span.offset);
        h.bytes_in_use -= span.length as u32;
        r::put32(r::mutable(record), 24, h.bytes_in_use);
        r::put16(r::mutable(record), 18, h.link_count - 1);
        if !r::store(v, number, record) || !cleanup::budgeted_flush(v) || !r::set_dirty(v, false) {
            return WriteStatus::Io;
        }
        WriteStatus::Ok
    }
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDelete {
    Ok,
    Unlinked,
    NotFound,
    Mismatch,
    Directory,
    Unsupported,
    Io,
}
unsafe fn canonical_parent(
    v: Volume,
    number: u64,
    sequence: u16,
    out: &mut FileReference,
) -> Lookup {
    unsafe {
        let Some(h) = r::load(v, number, v.part_record()) else {
            return Lookup::Io;
        };
        let record = v.part_record();
        if h.sequence != sequence || !a::stream_valid(r::view(record), h) {
            return Lookup::Io;
        }
        if h.link_count != 1 {
            return Lookup::NotFound;
        }
        let mut offset = h.attrs_offset as usize;
        let mut total = 0;
        let mut canonical = 0;
        while let Some(attr) = fmt::attribute(r::view(record), &mut offset) {
            if attr.attr_type != 0x30 {
                continue;
            }
            total += 1;
            let Some(name) = fmt::file_name(attr.value.get()) else {
                return Lookup::Io;
            };
            if name.namespace == 2 {
                continue;
            }
            canonical += 1;
            *out = name.parent
        }
        if total == 1 && canonical == 1 {
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn recover_delete(
    v: Volume,
    parent: u64,
    name: View,
    number: u64,
    sequence: u16,
) -> RecoveryDelete {
    unsafe {
        if name.len == 0 || name.len > 768 {
            return RecoveryDelete::Io;
        }
        let mut found = LookupResult::EMPTY;
        match lookup::lookup(v, parent, name, &mut found, true, 0) {
            Lookup::NotFound => return RecoveryDelete::NotFound,
            Lookup::Io => return RecoveryDelete::Io,
            Lookup::Found => {}
        }
        if found.record != number || found.sequence != sequence {
            return RecoveryDelete::Mismatch;
        }
        if found.entry.is_dir() {
            return RecoveryDelete::Directory;
        }
        if !lookup::live(v, found) {
            return RecoveryDelete::Io;
        }
        match lookup::single_link(v, found.record, found.sequence) {
            WriteStatus::Ok => {}
            WriteStatus::Unsupported => return RecoveryDelete::Unsupported,
            _ => return RecoveryDelete::Io,
        }
        let mut cp = FileReference::default();
        match canonical_parent(v, found.record, found.sequence, &mut cp) {
            Lookup::NotFound => return RecoveryDelete::Unsupported,
            Lookup::Io => return RecoveryDelete::Io,
            Lookup::Found => {}
        }
        if cp.record != parent {
            return RecoveryDelete::Unsupported;
        }
        let reference = (FileReference {
            record: found.record,
            sequence: found.sequence,
        })
        .pack();
        let mut sink = directory::Sink {
            count_ref: Some(reference),
            ..directory::Sink::EMPTY
        };
        if !directory::enumerate(v, parent, &mut sink) || sink.failed || sink.matches == 0 {
            return RecoveryDelete::Io;
        }
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        if sink.matches > 1 {
            if !r::set_dirty(v, true)
                || remove::remove(v, parent, name, Some(reference)) != WriteStatus::Ok
                || !cleanup::budgeted_flush(v)
                || !r::set_dirty(v, false)
            {
                return RecoveryDelete::Io;
            }
            return RecoveryDelete::Unlinked;
        }
        match files::release(v, parent, name, found) {
            WriteStatus::Ok => RecoveryDelete::Ok,
            WriteStatus::NotFound => RecoveryDelete::NotFound,
            WriteStatus::Directory => RecoveryDelete::Directory,
            _ => RecoveryDelete::Io,
        }
    }
}
