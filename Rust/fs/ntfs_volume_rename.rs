//! Original alias-first, exact-identity, independently durable rename phases.
use super::{
    ntfs_format_names as text, ntfs_format_records as fmt, ntfs_format_types::*,
    ntfs_volume_attributes as a, ntfs_volume_build_names as names,
    ntfs_volume_canonical as canonical, ntfs_volume_cleanup as cleanup, ntfs_volume_edit as edit,
    ntfs_volume_index_insert as insert, ntfs_volume_index_io as idx,
    ntfs_volume_index_remove as remove, ntfs_volume_lookup as lookup, ntfs_volume_records as r,
    ntfs_volume_types::*,
};
use core::ptr;

enum Phase {
    Initial,
    AliasPublished,
    CanonicalRewritten,
    Complete,
    Missing,
    Conflict,
    Unsupported,
    Io,
}

#[inline(never)]
unsafe fn inspect(
    v: Volume,
    old_parent: u64,
    old_name: View,
    new_parent: u64,
    new_name: View,
    subject: *mut LookupResult,
) -> Phase {
    unsafe {
        let Some(old_seq) = lookup::sequence(v, old_parent) else {
            return Phase::Io;
        };
        let Some(new_seq) = lookup::sequence(v, new_parent) else {
            return Phase::Io;
        };
        let mut old = LookupResult::EMPTY;
        let old_status = lookup::lookup(v, old_parent, old_name, &mut old, true, 0);
        if old_status == Lookup::Io {
            return Phase::Io;
        }
        let mut new = LookupResult::EMPTY;
        let new_status = lookup::lookup(v, new_parent, new_name, &mut new, true, 0);
        if new_status == Lookup::Io {
            return Phase::Io;
        }
        if old_status == Lookup::NotFound && new_status == Lookup::NotFound {
            return Phase::Missing;
        }
        if (old_status == Lookup::Found && !lookup::live(v, old))
            || (new_status == Lookup::Found && !lookup::live(v, new))
        {
            return Phase::Io;
        }
        if old_status == Lookup::Found && new_status == Lookup::Found {
            if old.record != new.record || old.sequence != new.sequence {
                return Phase::Conflict;
            }
            if old.entry.is_dir() != new.entry.is_dir() {
                return Phase::Io;
            }
        }
        subject.write(if old_status == Lookup::Found {
            old
        } else {
            new
        });
        let status = lookup::single_link(v, (*subject).record, (*subject).sequence);
        if status == WriteStatus::Unsupported {
            return Phase::Unsupported;
        }
        if status != WriteStatus::Ok {
            return Phase::Io;
        }
        let old_canonical = canonical::status(
            v,
            (*subject).record,
            (*subject).sequence,
            old_parent,
            old_seq,
            old_name,
        );
        if old_canonical == Lookup::Io {
            return Phase::Io;
        }
        let new_canonical = canonical::status(
            v,
            (*subject).record,
            (*subject).sequence,
            new_parent,
            new_seq,
            new_name,
        );
        if new_canonical == Lookup::Io {
            return Phase::Io;
        }
        if old_canonical == Lookup::Found && new_canonical == Lookup::Found {
            return Phase::Conflict;
        }
        if old_canonical == Lookup::NotFound && new_canonical == Lookup::NotFound {
            return Phase::Io;
        }
        if old_status == Lookup::Found && new_status == Lookup::NotFound {
            return if old_canonical == Lookup::Found {
                Phase::Initial
            } else {
                Phase::Io
            };
        }
        if old_status == Lookup::Found && new_status == Lookup::Found {
            return if old_canonical == Lookup::Found {
                Phase::AliasPublished
            } else {
                Phase::CanonicalRewritten
            };
        }
        if new_canonical == Lookup::Found {
            Phase::Complete
        } else {
            Phase::Io
        }
    }
}

#[inline(never)]
unsafe fn alias_value(
    v: Volume,
    parent: u64,
    name: View,
    location: LookupResult,
    out: &mut [u8],
) -> Option<usize> {
    unsafe {
        let parent_sequence = lookup::sequence(v, parent)?;
        let target = ptr::addr_of_mut!((*v.scratch()).name_utf16).cast::<u8>();
        let n = text::to_utf16(name.get(), core::slice::from_raw_parts_mut(target, 510))?;
        let entries = if location.in_root {
            let h = r::load(v, parent, v.record())?;
            if h.sequence != parent_sequence
                || h.flags & 2 == 0
                || !a::stream_valid(r::view(v.record()), h)
            {
                return None;
            }
            let attr = r::find(v, v.record(), h, 0x90, View::of(&idx::I30))?;
            fmt::index_root(attr.value.get())?.entries
        } else {
            let alloc = ptr::addr_of_mut!((*v.scratch()).attr_index);
            if r::collect(v, parent, 0xa0, View::of(&idx::I30), alloc) != Lookup::Found
                || (*alloc).resident
                || (*alloc).count == 0
                || !idx::load(v, r::attr_runs(alloc), location.block_vcn)
            {
                return None;
            }
            let block = fmt::index_block(r::view(idx::block(v)))?;
            if block.vcn != location.block_vcn {
                return None;
            }
            block.entries
        };
        if location.entry_offset >= entries.len {
            return None;
        }
        let e = fmt::index_entry(
            &entries.get()[location.entry_offset..],
            &mut IteratorState::default(),
        )?;
        let reference = (FileReference {
            record: location.record,
            sequence: location.sequence,
        })
        .pack();
        if e.flags & 2 != 0 || e.file_reference != reference {
            return None;
        }
        let filename = fmt::file_name(e.key.get())?;
        if filename.parent.record != parent
            || filename.parent.sequence != parent_sequence
            || text::compare(
                v.upcase.get(),
                core::slice::from_raw_parts(target, n),
                filename.name.get(),
            ) != 0
            || e.key.len > out.len()
        {
            return None;
        }
        out[..e.key.len].copy_from_slice(e.key.get());
        Some(e.key.len)
    }
}

#[inline(never)]
unsafe fn publish(v: Volume, source: LookupResult, parent: u64, name: View) -> WriteStatus {
    unsafe {
        let Some(sequence) = lookup::sequence(v, parent) else {
            return WriteStatus::Io;
        };
        let dir = source.entry.is_dir();
        let mut allocation = 0;
        let mut size = 0;
        if !dir {
            let data = ptr::addr_of_mut!((*v.scratch()).attr_op);
            if r::collect(v, source.record, 0x80, View::default(), data) != Lookup::Found {
                return WriteStatus::Io;
            }
            allocation = (*data).alloc_size;
            size = (*data).data_size;
        }
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = names::value(
            v,
            &mut filename,
            parent,
            sequence,
            name.get(),
            dir,
            allocation,
            size,
            source.entry.created_time_nt,
        ) else {
            return cleanup::abort(v, WriteStatus::Invalid);
        };
        let Some(mut h) = r::load(v, source.record, v.write_record()) else {
            return WriteStatus::Io;
        };
        if h.sequence != source.sequence
            || h.link_count != 1
            || !a::stream_valid(r::view(v.write_record()), h)
        {
            return WriteStatus::Io;
        }
        if !edit::resident(
            r::mutable(v.write_record()),
            &mut h,
            0x30,
            &[],
            &filename[..n],
        ) {
            return cleanup::abort(v, WriteStatus::RecordFull);
        }
        let mut key = [0u8; 16 + 0x42 + 510];
        let len = (16 + n + 7) & !7usize;
        r::put16(&mut key, 8, len as u16);
        r::put16(&mut key, 10, n as u16);
        key[16..16 + n].copy_from_slice(&filename[..n]);
        let reference = (FileReference {
            record: source.record,
            sequence: source.sequence,
        })
        .pack();
        insert::insert(v, parent, reference, View::of(&key[..len]))
    }
}

#[inline(never)]
unsafe fn rewrite(v: Volume, subject: LookupResult, parent: u64, name: View) -> WriteStatus {
    unsafe {
        let mut target = LookupResult::EMPTY;
        if lookup::lookup(v, parent, name, &mut target, true, 0) != Lookup::Found
            || target.record != subject.record
            || target.sequence != subject.sequence
        {
            return WriteStatus::Io;
        }
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = alias_value(v, parent, name, target, &mut filename) else {
            return WriteStatus::Io;
        };
        let Some(parsed) = fmt::file_name(&filename[..n]) else {
            return WriteStatus::Io;
        };
        if (parsed.flags & 0x10000000 != 0) != subject.entry.is_dir() {
            return WriteStatus::Io;
        }
        let Some(mut h) = r::load(v, subject.record, v.write_record()) else {
            return WriteStatus::Io;
        };
        if h.sequence != subject.sequence
            || h.link_count != 1
            || !a::stream_valid(r::view(v.write_record()), h)
        {
            return WriteStatus::Io;
        }
        if !edit::resident(
            r::mutable(v.write_record()),
            &mut h,
            0x30,
            &[],
            &filename[..n],
        ) {
            return WriteStatus::Io;
        }
        if r::store(v, subject.record, v.write_record()) {
            WriteStatus::Ok
        } else {
            WriteStatus::Io
        }
    }
}

#[inline(never)]
unsafe fn detach(
    v: Volume,
    subject: LookupResult,
    old_parent: u64,
    old_name: View,
    new_parent: u64,
    new_name: View,
) -> WriteStatus {
    unsafe {
        let mut target = LookupResult::EMPTY;
        if lookup::lookup(v, new_parent, new_name, &mut target, true, 0) != Lookup::Found
            || target.record != subject.record
            || target.sequence != subject.sequence
        {
            return WriteStatus::Io;
        }
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = alias_value(v, new_parent, new_name, target, &mut filename) else {
            return WriteStatus::Io;
        };
        if canonical::exact(
            v,
            subject.record,
            subject.sequence,
            View::of(&filename[..n]),
        ) != Lookup::Found
        {
            return WriteStatus::Io;
        }
        let reference = (FileReference {
            record: subject.record,
            sequence: subject.sequence,
        })
        .pack();
        remove::remove(v, old_parent, old_name, Some(reference))
    }
}

#[inline(never)]
pub(super) unsafe fn rename(
    v: Volume,
    old_parent: u64,
    old_name: View,
    new_parent: u64,
    new_name: View,
) -> WriteStatus {
    unsafe {
        if old_name.len == 0
            || old_name.len > NAME_MAX
            || new_name.len == 0
            || new_name.len > NAME_MAX
        {
            return WriteStatus::Invalid;
        }
        let mut subject = LookupResult::EMPTY;
        let phase = inspect(v, old_parent, old_name, new_parent, new_name, &mut subject);
        match phase {
            Phase::Missing => return WriteStatus::NotFound,
            Phase::Conflict => return WriteStatus::Exists,
            Phase::Unsupported => return WriteStatus::Unsupported,
            Phase::Io => return WriteStatus::Io,
            Phase::Complete | Phase::AliasPublished | Phase::CanonicalRewritten => {
                let Some(dirty) = r::is_dirty(v) else {
                    return WriteStatus::Io;
                };
                if !dirty {
                    return if matches!(phase, Phase::Complete) {
                        WriteStatus::NotFound
                    } else {
                        WriteStatus::Io
                    };
                }
            }
            Phase::Initial => {}
        }
        if !r::set_dirty(v, true) {
            return WriteStatus::Io;
        }
        let result = locked(v, old_parent, old_name, new_parent, new_name, true);
        if result != WriteStatus::Ok {
            return result;
        }
        if r::set_dirty(v, false) {
            WriteStatus::Ok
        } else {
            WriteStatus::Io
        }
    }
}

#[inline(never)]
pub(super) unsafe fn locked(
    v: Volume,
    old_parent: u64,
    old_name: View,
    new_parent: u64,
    new_name: View,
    flush_steps: bool,
) -> WriteStatus {
    unsafe {
        for _ in 0..4 {
            let mut subject = LookupResult::EMPTY;
            let result = match inspect(v, old_parent, old_name, new_parent, new_name, &mut subject)
            {
                Phase::Initial => publish(v, subject, new_parent, new_name),
                Phase::AliasPublished => rewrite(v, subject, new_parent, new_name),
                Phase::CanonicalRewritten => {
                    detach(v, subject, old_parent, old_name, new_parent, new_name)
                }
                Phase::Complete => return WriteStatus::Ok,
                Phase::Missing => return WriteStatus::NotFound,
                Phase::Conflict => return WriteStatus::Exists,
                Phase::Unsupported => return WriteStatus::Unsupported,
                Phase::Io => return WriteStatus::Io,
            };
            if result != WriteStatus::Ok {
                return result;
            }
            if flush_steps && !cleanup::budgeted_flush(v) {
                return WriteStatus::Io;
            }
        }
        WriteStatus::Io
    }
}
