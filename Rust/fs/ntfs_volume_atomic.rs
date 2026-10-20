//! Original create-only ownership transfer and two-leg atomic replacement.
use super::{
    ntfs_format_types::*, ntfs_volume_append as append, ntfs_volume_attributes as a,
    ntfs_volume_build_names as names, ntfs_volume_canonical as canonical,
    ntfs_volume_cleanup as cleanup, ntfs_volume_edit as edit, ntfs_volume_index_insert as insert,
    ntfs_volume_index_remove as remove, ntfs_volume_lookup as lookup, ntfs_volume_records as r,
    ntfs_volume_types::*,
};
use core::{ptr, sync::atomic::Ordering::Relaxed};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReplaceResult {
    Ok,
    Invalid,
    NotFound,
    Alias,
    Conflict,
    ReadOnly,
    Io,
    NotAtomic,
}

fn same(x: LookupResult, y: LookupResult) -> bool {
    x.record == y.record && x.sequence == y.sequence
}
fn read_only(x: LookupResult) -> bool {
    x.entry.is_dir() || x.entry.attr & 1 != 0
}

#[inline(never)]
pub(super) unsafe fn publish(
    v: Volume,
    parent: u64,
    target_name: View,
    staged_name: View,
) -> ReplaceResult {
    unsafe {
        if !names::valid(target_name.get()) || !names::valid(staged_name.get()) {
            return ReplaceResult::Invalid;
        }
        match canonical::equal(v, target_name, staged_name) {
            None => return ReplaceResult::Invalid,
            Some(true) => return ReplaceResult::Alias,
            Some(false) => {}
        }
        let Some(sequence) = lookup::sequence(v, parent) else {
            return ReplaceResult::Io;
        };
        let mut target = LookupResult::EMPTY;
        let target_status = lookup::lookup(v, parent, target_name, &mut target, true, 0);
        if target_status == Lookup::Io {
            return ReplaceResult::Io;
        }
        let mut staged = LookupResult::EMPTY;
        let staged_status = lookup::lookup(v, parent, staged_name, &mut staged, true, 0);
        if staged_status == Lookup::Io {
            return ReplaceResult::Io;
        }
        if target_status == Lookup::Found {
            if staged_status != Lookup::Found || !same(target, staged) {
                return ReplaceResult::Conflict;
            }
            if !lookup::live(v, target) || !lookup::live(v, staged) {
                return ReplaceResult::Io;
            }
            if read_only(target) || read_only(staged) {
                return ReplaceResult::ReadOnly;
            }
            let ct = canonical::status(
                v,
                target.record,
                target.sequence,
                parent,
                sequence,
                target_name,
            );
            if ct == Lookup::Io {
                return ReplaceResult::Io;
            }
            let cs = canonical::status(
                v,
                target.record,
                target.sequence,
                parent,
                sequence,
                staged_name,
            );
            if cs == Lookup::Io {
                return ReplaceResult::Io;
            }
            if ct != Lookup::Found && cs != Lookup::Found {
                return ReplaceResult::Conflict;
            }
            if !r::ensure_dirty(v) {
                return ReplaceResult::Io;
            }
            if ct != Lookup::Found
                && (canonical::rewrite(v, staged, parent, sequence, target_name) != WriteStatus::Ok
                    || !cleanup::budgeted_flush(v))
            {
                return ReplaceResult::Io;
            }
            if remove::remove(v, parent, staged_name, None) != WriteStatus::Ok
                || !cleanup::budgeted_flush(v)
                || !r::set_dirty(v, false)
            {
                return ReplaceResult::Io;
            }
            return ReplaceResult::Ok;
        }
        if staged_status != Lookup::Found {
            return ReplaceResult::NotFound;
        }
        if !lookup::live(v, staged) {
            return ReplaceResult::Io;
        }
        if read_only(staged) {
            return ReplaceResult::ReadOnly;
        }
        let cs = canonical::status(
            v,
            staged.record,
            staged.sequence,
            parent,
            sequence,
            staged_name,
        );
        if cs == Lookup::Io {
            return ReplaceResult::Io;
        }
        let ct = canonical::status(
            v,
            staged.record,
            staged.sequence,
            parent,
            sequence,
            target_name,
        );
        if ct == Lookup::Io {
            return ReplaceResult::Io;
        }
        if cs != Lookup::Found && ct != Lookup::Found {
            return ReplaceResult::Conflict;
        }
        let data = ptr::addr_of_mut!((*v.scratch()).attr_op);
        if r::collect(v, staged.record, 0x80, View::default(), data) != Lookup::Found {
            return ReplaceResult::Io;
        }
        let allocation = (*data).alloc_size;
        let size = (*data).data_size;
        let mut filename = [0u8; 0x42 + 510];
        let Some(n) = names::value(
            v,
            &mut filename,
            parent,
            sequence,
            target_name.get(),
            false,
            allocation,
            size,
            staged.entry.created_time_nt,
        ) else {
            return ReplaceResult::Invalid;
        };
        if !r::ensure_dirty(v) {
            return ReplaceResult::Io;
        }
        if ct != Lookup::Found {
            let Some(mut h) = r::load(v, staged.record, v.write_record()) else {
                return ReplaceResult::Io;
            };
            if h.sequence != staged.sequence || h.link_count != 1 {
                return ReplaceResult::Conflict;
            }
            if !a::stream_valid(r::view(v.write_record()), h)
                || !edit::resident(
                    r::mutable(v.write_record()),
                    &mut h,
                    0x30,
                    &[],
                    &filename[..n],
                )
                || !r::store(v, staged.record, v.write_record())
            {
                return ReplaceResult::Io;
            }
            if cleanup::CUT_AFTER_CANONICAL.load(Relaxed) {
                return ReplaceResult::Io;
            }
        }
        let mut key = [0u8; 16 + 0x42 + 510];
        let len = (16 + n + 7) & !7usize;
        r::put16(&mut key, 8, len as u16);
        r::put16(&mut key, 10, n as u16);
        key[16..16 + n].copy_from_slice(&filename[..n]);
        let reference = (FileReference {
            record: staged.record,
            sequence: staged.sequence,
        })
        .pack();
        if insert::insert(v, parent, reference, View::of(&key[..len])) != WriteStatus::Ok
            || !cleanup::budgeted_flush(v)
        {
            return ReplaceResult::Io;
        }
        let mut published = LookupResult::EMPTY;
        if lookup::lookup(v, parent, target_name, &mut published, true, 0) != Lookup::Found
            || !same(published, staged)
        {
            return ReplaceResult::Io;
        }
        let mut still_staged = LookupResult::EMPTY;
        if lookup::lookup(v, parent, staged_name, &mut still_staged, true, 0) != Lookup::Found
            || !same(still_staged, staged)
        {
            return ReplaceResult::Io;
        }
        if remove::remove(v, parent, staged_name, None) != WriteStatus::Ok
            || !cleanup::budgeted_flush(v)
            || !r::set_dirty(v, false)
        {
            return ReplaceResult::Io;
        }
        ReplaceResult::Ok
    }
}

#[inline(never)]
pub(super) unsafe fn replace(
    v: Volume,
    parent: u64,
    target_name: View,
    staged_name: View,
    backup_name: View,
    consume_stage: bool,
) -> ReplaceResult {
    unsafe {
        if !consume_stage {
            return ReplaceResult::NotAtomic;
        }
        if !names::valid(target_name.get())
            || !names::safe83(staged_name.get())
            || !names::safe83(backup_name.get())
        {
            return ReplaceResult::Invalid;
        }
        let Some(ts) = canonical::equal(v, target_name, staged_name) else {
            return ReplaceResult::Invalid;
        };
        let Some(tb) = canonical::equal(v, target_name, backup_name) else {
            return ReplaceResult::Invalid;
        };
        let Some(sb) = canonical::equal(v, staged_name, backup_name) else {
            return ReplaceResult::Invalid;
        };
        if ts || tb || sb {
            return ReplaceResult::Alias;
        }
        let mut target = LookupResult::EMPTY;
        let target_status = lookup::lookup(v, parent, target_name, &mut target, true, 0);
        if target_status == Lookup::Io {
            return ReplaceResult::Io;
        }
        if target_status == Lookup::Found {
            if !lookup::live(v, target) {
                return ReplaceResult::Io;
            }
            if read_only(target) {
                return ReplaceResult::ReadOnly;
            }
        }
        let mut backup = LookupResult::EMPTY;
        let backup_status = lookup::lookup(v, parent, backup_name, &mut backup, true, 0);
        if backup_status == Lookup::Io {
            return ReplaceResult::Io;
        }
        if backup_status == Lookup::Found {
            if !lookup::live(v, backup) {
                return ReplaceResult::Io;
            }
            if read_only(backup) {
                return ReplaceResult::Conflict;
            }
        }
        let mut staged = LookupResult::EMPTY;
        let staged_status = lookup::lookup(v, parent, staged_name, &mut staged, true, 0);
        if staged_status == Lookup::Io {
            return ReplaceResult::Io;
        }
        if staged_status != Lookup::Found {
            if target_status != Lookup::Found {
                return ReplaceResult::NotFound;
            }
            return if append::finish(v) {
                ReplaceResult::Ok
            } else {
                ReplaceResult::Io
            };
        }
        if !lookup::live(v, staged) {
            return ReplaceResult::Io;
        }
        if read_only(staged) {
            return ReplaceResult::ReadOnly;
        }
        if target_status == Lookup::Found {
            if same(target, staged) {
                return publish(v, parent, target_name, staged_name);
            }
            if backup_status == Lookup::Found && !same(target, backup) {
                return ReplaceResult::Conflict;
            }
            let first = publish(v, parent, backup_name, target_name);
            if first != ReplaceResult::Ok {
                return first;
            }
            return publish(v, parent, target_name, staged_name);
        }
        publish(v, parent, target_name, staged_name)
    }
}
