//! Original FAT mutation ordering and lost-completion reconciliation.
//! Ordinary mutations use exact cache batches; ownership/recovery retains
//! full-device barriers and the original no-retry/one-retry distinctions.
use super::{
    fat_chain as c, fat_directory as d, fat_files as f, fat_names as n, fat_state as s,
    fat_types::*,
};
use core::{mem::MaybeUninit, sync::atomic::Ordering::Relaxed};
use s::STATS;
#[inline(never)]
pub(super) unsafe fn write_file(original: Volume, parent: u32, name: Bytes, data: Bytes) -> bool {
    unsafe {
        if !n::valid(name.view()) {
            return false;
        }
        let mut op = s::Operation::begin(6);
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut loc = MaybeUninit::<Location>::uninit();
        match d::find_location(v, parent, name, loc.as_mut_ptr()) {
            Lookup::Found => {
                let loc = loc.assume_init();
                if loc.entry.is_dir() || loc.entry.read_only() || !d::delete_at(v, loc) {
                    return false;
                }
            }
            Lookup::NotFound => {}
            Lookup::Io => return false,
        }
        let first = if data.len == 0 {
            0
        } else {
            let Some(first) = c::allocate(v, c::clusters_for(v, data.len)) else {
                return false;
            };
            first
        };
        if first != 0 && !f::write_cluster_data(v, first, data) {
            c::free(v, first);
            return false;
        }
        let mut raw = [0; SLOT_MAX * 32];
        let Some(len) = d::build_entries(
            v,
            parent,
            name,
            0x20,
            first,
            u32::try_from(data.len).expect("FAT file size"),
            &mut raw,
        ) else {
            if first != 0 {
                c::free(v, first);
            }
            return false;
        };
        if !d::write_entries(v, parent, Bytes::of(&raw[..len])) {
            if first != 0 {
                c::free(v, first);
            }
            return false;
        }
        op.ok = s::flush_mutation(v);
        if op.ok {
            STATS.file_writes.fetch_add(1, Relaxed);
            STATS.file_write_bytes.fetch_add(data.len as u64, Relaxed);
        }
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn append(
    v: Volume,
    parent: u32,
    name: Bytes,
    expected: Option<u32>,
    create: bool,
    deferred: bool,
    data: Bytes,
) -> Append {
    unsafe {
        let mut op = s::Operation::begin(7);
        let status = append_internal(v, parent, name, expected, create, deferred, data);
        op.ok = status == Append::Ok;
        status
    }
}
#[allow(clippy::manual_is_multiple_of)] // Original modulo also rejects zero geometry by division trap.
unsafe fn append_internal(
    original: Volume,
    parent: u32,
    name: Bytes,
    expected: Option<u32>,
    create: bool,
    deferred: bool,
    data: Bytes,
) -> Append {
    unsafe {
        if !n::valid(name.view()) {
            return Append::Invalid;
        }
        let Some(v) = s::mutation(original) else {
            return Append::Io;
        };
        s::invalidate_extents();
        let loc = if let Some(loc) = f::append_location(v, parent, name, expected) {
            loc
        } else {
            let mut found = MaybeUninit::<Location>::uninit();
            match d::find_location(v, parent, name, found.as_mut_ptr()) {
                Lookup::Found => {}
                Lookup::Io => return Append::Io,
                Lookup::NotFound => {
                    if !create {
                        return Append::NotFound;
                    }
                    if !n::valid_short(name.view()) {
                        return Append::Invalid;
                    }
                    if !d::create_empty(v, parent, name) {
                        return Append::Io;
                    }
                    if d::find_location(v, parent, name, found.as_mut_ptr()) != Lookup::Found {
                        return Append::Io;
                    }
                }
            }
            found.assume_init()
        };
        if loc.entry.is_dir() || loc.entry.read_only() {
            return Append::Invalid;
        }
        if expected.is_some_and(|size| loc.entry.size != size) {
            return Append::OffsetMismatch;
        }
        if data.len == 0 {
            f::remember_append(
                v,
                parent,
                name,
                loc,
                loc.entry.first_cluster,
                loc.entry.size,
                f::append_last(v, parent, name, loc.entry.size).unwrap_or(0),
            );
            return Append::Ok;
        }
        let old = loc.entry.size as usize;
        if data.len > u32::MAX as usize - old {
            return Append::TooLarge;
        }
        let new = old + data.len;
        let bytes = v.cluster_bytes() as usize;
        let old_clusters = c::clusters_for(v, old);
        let needed = c::clusters_for(v, new);
        let mut first = loc.entry.first_cluster;
        let mut last = 0;
        if old_clusters > 0 {
            last = if let Some(last) = f::append_last(v, parent, name, loc.entry.size) {
                last
            } else {
                let Some(last) = c::nth(v, first, old_clusters - 1) else {
                    return Append::Io;
                };
                last
            };
        }
        let mut added = Allocation::default();
        if needed > old_clusters {
            let start = if (2..EOC - 1).contains(&last) {
                last + 1
            } else {
                c::next_start(v, 3)
            };
            let Some(a) = c::prefer(v, needed - old_clusters, start) else {
                return Append::Io;
            };
            added = a;
            if first < 2 {
                first = added.first;
            } else if !c::write_fat(v, last, added.first) {
                c::free(v, added.first);
                return Append::Io;
            }
        }
        if first < 2 {
            return Append::Io;
        }
        let start = if old == 0 {
            first
        } else if old % bytes == 0 {
            added.first
        } else {
            last
        };
        if start < 2 {
            return Append::Io;
        }
        if !f::write_from(v, start, old % bytes, data)
            || !d::update_location(
                v,
                loc.lba,
                loc.offset,
                first,
                u32::try_from(new).expect("FAT append size"),
            )
        {
            return Append::Io;
        }
        if !deferred && !s::flush_mutation(v) {
            return Append::Io;
        }
        let last = if added.last >= 2 { added.last } else { last };
        f::remember_append(v, parent, name, loc, first, new as u32, last);
        STATS.file_appends.fetch_add(1, Relaxed);
        STATS.file_append_bytes.fetch_add(data.len as u64, Relaxed);
        Append::Ok
    }
}
#[inline(never)]
pub(super) unsafe fn publish_copy(
    original: Volume,
    parent: u32,
    staged: Bytes,
    target: Bytes,
) -> bool {
    unsafe {
        if !n::valid_short(staged.view())
            || !n::valid(target.view())
            || n::equal_ascii(staged.view(), target.view())
        {
            return false;
        }
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut e = MaybeUninit::uninit();
        if d::find(v, parent, target, e.as_mut_ptr()) != Lookup::NotFound {
            return false;
        }
        let mut loc = MaybeUninit::<Location>::uninit();
        if d::find_location(v, parent, staged, loc.as_mut_ptr()) != Lookup::Found {
            return false;
        }
        let loc = loc.assume_init();
        if loc.entry.is_dir() || loc.entry.read_only() || loc.lfn_count != 0 {
            return false;
        }
        let mut raw = [0; SLOT_MAX * 32];
        let Some(len) = d::build_entries(
            v,
            parent,
            target,
            0x20,
            loc.entry.first_cluster,
            loc.entry.size,
            &mut raw,
        ) else {
            return false;
        };
        if !d::detach(v, loc) || !s::flush_mutation(v) {
            return false;
        }
        if !d::write_entries(v, parent, Bytes::of(&raw[..len])) {
            return false;
        }
        s::flush_mutation(v)
    }
}
#[inline(never)]
pub(super) unsafe fn copy(
    src: Volume,
    dst: Volume,
    e: Entry,
    parent: u32,
    name: Bytes,
    replace: bool,
) -> bool {
    unsafe {
        let mut op = s::Operation::begin(8);
        if e.is_dir() || !n::valid(name.view()) {
            return false;
        }
        let mut loc = MaybeUninit::<Location>::uninit();
        let status = d::find_location(dst, parent, name, loc.as_mut_ptr());
        let existing = if status == Lookup::Found {
            Some(loc.assume_init())
        } else {
            None
        };
        match status {
            Lookup::Found => {
                let loc = existing.unwrap();
                if !replace || loc.entry.is_dir() || loc.entry.read_only() {
                    return false;
                }
                if src.device_index == dst.device_index
                    && src.partition_lba == dst.partition_lba
                    && n::same_copy(e, loc.entry)
                {
                    return false;
                }
            }
            Lookup::NotFound => {}
            Lookup::Io => return false,
        }
        let Some(v) = s::mutation(dst) else {
            return false;
        };
        s::invalidate_append();
        if let Some(loc) = existing
            && !d::delete_at(v, loc)
        {
            return false;
        }
        let bytes = e.size as usize;
        let first = if bytes == 0 {
            0
        } else {
            let Some(first) = c::allocate(v, c::clusters_for(v, bytes)) else {
                return false;
            };
            first
        };
        if first != 0 && !f::copy_clusters(src, v, e, first) {
            c::free(v, first);
            return false;
        }
        let mut raw = [0; SLOT_MAX * 32];
        let Some(len) = d::build_entries(v, parent, name, 0x20, first, bytes as u32, &mut raw)
        else {
            if first != 0 {
                c::free(v, first);
            }
            return false;
        };
        if !d::write_entries(v, parent, Bytes::of(&raw[..len])) {
            if first != 0 {
                c::free(v, first);
            }
            return false;
        }
        op.ok = s::flush_mutation(v);
        if op.ok {
            STATS.file_write_bytes.fetch_add(bytes as u64, Relaxed);
        }
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn make_directory(original: Volume, parent: u32, name: Bytes) -> bool {
    unsafe {
        if !n::valid(name.view()) {
            return false;
        }
        let mut op = s::Operation::begin(9);
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut e = MaybeUninit::uninit();
        if d::find(v, parent, name, e.as_mut_ptr()) != Lookup::NotFound {
            return false;
        }
        let Some(cluster) = c::allocate(v, 1) else {
            return false;
        };
        if !d::init_cluster(v, cluster, parent) {
            c::free(v, cluster);
            return false;
        }
        let mut raw = [0; SLOT_MAX * 32];
        let Some(len) = d::build_entries(v, parent, name, 0x10, cluster, 0, &mut raw) else {
            c::free(v, cluster);
            return false;
        };
        if !d::write_entries(v, parent, Bytes::of(&raw[..len])) {
            c::free(v, cluster);
            return false;
        }
        op.ok = s::flush_mutation(v);
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn delete(original: Volume, parent: u32, name: Bytes) -> bool {
    unsafe {
        if !n::valid(name.view()) {
            return false;
        }
        let mut op = s::Operation::begin(10);
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut loc = MaybeUninit::<Location>::uninit();
        if d::find_location(v, parent, name, loc.as_mut_ptr()) != Lookup::Found {
            return false;
        }
        let loc = loc.assume_init();
        if loc.entry.is_dir() || loc.entry.read_only() || !d::delete_at(v, loc) {
            return false;
        }
        op.ok = s::flush_mutation(v);
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn delete_match(v: Volume, parent: u32, name: Bytes, expected: Entry) -> Delete {
    unsafe {
        if !n::valid(name.view()) {
            return Delete::Io;
        }
        let mut op = s::Operation::begin(10);
        s::invalidate_append();
        s::invalidate_extents();
        let mut loc = MaybeUninit::<Location>::uninit();
        match d::find_location(v, parent, name, loc.as_mut_ptr()) {
            Lookup::Found => {}
            Lookup::NotFound => {
                op.ok = s::flush_volume(v);
                return if op.ok { Delete::NotFound } else { Delete::Io };
            }
            Lookup::Io => return Delete::Io,
        }
        let loc = loc.assume_init();
        if loc.entry.is_dir() || loc.entry.read_only() || !n::same_identity(loc.entry, expected) {
            return Delete::Mismatch;
        }
        if !d::delete_at(v, loc) {
            let mut after = MaybeUninit::<Location>::uninit();
            return match d::find_location(v, parent, name, after.as_mut_ptr()) {
                Lookup::NotFound => {
                    op.ok = s::flush_volume(v);
                    if op.ok { Delete::Deleted } else { Delete::Io }
                }
                Lookup::Found => {
                    if n::same_identity(after.assume_init().entry, expected) {
                        Delete::Io
                    } else {
                        Delete::Mismatch
                    }
                }
                Lookup::Io => Delete::Io,
            };
        }
        op.ok = s::flush_volume(v);
        if op.ok { Delete::Deleted } else { Delete::Io }
    }
}
#[inline(never)]
pub(super) unsafe fn remove_directory(original: Volume, parent: u32, name: Bytes) -> bool {
    unsafe {
        if !n::valid(name.view()) {
            return false;
        }
        let mut op = s::Operation::begin(11);
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut loc = MaybeUninit::<Location>::uninit();
        if d::find_location(v, parent, name, loc.as_mut_ptr()) != Lookup::Found {
            return false;
        }
        let loc = loc.assume_init();
        if !loc.entry.is_dir() || !d::is_empty(v, loc.entry.first_cluster) || !d::delete_at(v, loc)
        {
            return false;
        }
        op.ok = s::flush_mutation(v);
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn rename(original: Volume, parent: u32, old: Bytes, new: Bytes) -> Rename {
    unsafe {
        if !n::valid(old.view()) || !n::valid(new.view()) {
            return Rename::Io;
        }
        let mut op = s::Operation::begin(12);
        let Some(v) = s::mutation(original) else {
            return Rename::Io;
        };
        s::invalidate_append();
        if !n::valid_short(old.view()) || !n::valid_short(new.view()) {
            return Rename::NotAtomic;
        }
        let mut e = MaybeUninit::uninit();
        match d::find(v, parent, new, e.as_mut_ptr()) {
            Lookup::Found => return Rename::Conflict,
            Lookup::NotFound => {}
            Lookup::Io => return Rename::Io,
        }
        let mut loc = MaybeUninit::<Location>::uninit();
        match d::find_location(v, parent, old, loc.as_mut_ptr()) {
            Lookup::Found => {}
            Lookup::NotFound => return Rename::NotFound,
            Lookup::Io => return Rename::Io,
        }
        let loc = loc.assume_init();
        if loc.entry.read_only() {
            return Rename::ReadOnly;
        }
        if loc.lfn_count != 0 {
            return Rename::NotAtomic;
        }
        let mut short = [0; 11];
        if !n::build_short(new.view(), &mut short) {
            return Rename::NotAtomic;
        }
        let mut sector = [0; SECTOR];
        if !s::read(
            v.device_index,
            u64::from(loc.lba),
            1,
            Buffer::of(&mut sector),
        ) {
            return Rename::Io;
        }
        sector[loc.offset..loc.offset + 11].copy_from_slice(&short);
        if !s::write(v, u64::from(loc.lba), 1, Bytes::of(&sector)) {
            return Rename::Io;
        }
        STATS.dir_entry_updates.fetch_add(1, Relaxed);
        if !s::flush_mutation(v) {
            return Rename::Io;
        }
        op.ok = true;
        Rename::Ok
    }
}
#[inline(never)]
pub(super) unsafe fn attributes(
    original: Volume,
    parent: u32,
    name: Bytes,
    set: u8,
    clear: u8,
) -> bool {
    unsafe {
        if !n::valid(name.view()) {
            return false;
        }
        let mut op = s::Operation::begin(13);
        let Some(v) = s::mutation(original) else {
            return false;
        };
        s::invalidate_append();
        let mut loc = MaybeUninit::<Location>::uninit();
        if d::find_location(v, parent, name, loc.as_mut_ptr()) != Lookup::Found {
            return false;
        }
        let loc = loc.assume_init();
        let mut sector = [0; SECTOR];
        if !s::read(
            v.device_index,
            u64::from(loc.lba),
            1,
            Buffer::of(&mut sector),
        ) {
            return false;
        }
        let mut attr = sector[loc.offset + 11];
        attr |= set & 0x27;
        attr &= !(clear & 0x27);
        if loc.entry.is_dir() {
            attr |= 0x10;
        }
        sector[loc.offset + 11] = attr;
        if !s::write(v, u64::from(loc.lba), 1, Bytes::of(&sector)) {
            return false;
        }
        STATS.dir_entry_updates.fetch_add(1, Relaxed);
        op.ok = s::flush_mutation(v);
        op.ok
    }
}
#[inline(never)]
pub(super) unsafe fn replace(
    v: Volume,
    parent: u32,
    target_name: Bytes,
    staged_name: Bytes,
    backup_name: Bytes,
    consume: bool,
) -> Replace {
    unsafe {
        if !n::valid(target_name.view())
            || !n::valid(staged_name.view())
            || !n::valid_short(backup_name.view())
        {
            return Replace::Invalid;
        }
        s::invalidate_append();
        s::invalidate_extents();
        if !consume {
            return Replace::NotAtomic;
        }
        if n::equal_ascii(target_name.view(), staged_name.view())
            || n::equal_ascii(target_name.view(), backup_name.view())
            || n::equal_ascii(staged_name.view(), backup_name.view())
        {
            return Replace::Alias;
        }
        let short = n::valid_short(target_name.view());
        let Ok(target) = d::optional_location(v, parent, target_name) else {
            return Replace::Io;
        };
        let Ok(staged) = d::optional_location(v, parent, staged_name) else {
            return Replace::Io;
        };
        let Ok(backup) = d::optional_location(v, parent, backup_name) else {
            return Replace::Io;
        };
        if target.is_none() && !short {
            return Replace::NotAtomic;
        }
        if target.is_some_and(|l| l.entry.is_dir() || l.entry.read_only()) {
            return Replace::ReadOnly;
        }
        if backup.is_some_and(|l| l.entry.is_dir() || l.entry.read_only()) {
            return Replace::Conflict;
        }
        let Some(staged) = staged else {
            return if target.is_some() {
                Replace::Ok
            } else {
                Replace::NotFound
            };
        };
        if staged.entry.is_dir() || staged.entry.read_only() {
            return Replace::ReadOnly;
        }
        if let Some(t) = target {
            if n::same_ownership(t.entry, staged.entry) {
                if !d::detach(v, staged) {
                    return Replace::Io;
                }
                return if s::flush_volume(v) {
                    Replace::Ok
                } else {
                    Replace::Io
                };
            }
            if let Some(b) = backup {
                if !n::same_ownership(b.entry, t.entry) {
                    return Replace::Conflict;
                }
            } else {
                if !d::alias(v, parent, backup_name, t.entry) || !s::flush_volume(v) {
                    return Replace::Io;
                }
                let Ok(Some(b)) = d::optional_location(v, parent, backup_name) else {
                    return Replace::Io;
                };
                if !n::same_ownership(b.entry, t.entry) {
                    return Replace::Io;
                }
            }
            if !d::update_location(
                v,
                t.lba,
                t.offset,
                staged.entry.first_cluster,
                staged.entry.size,
            ) || !s::flush_volume(v)
            {
                return Replace::Io;
            }
            let Ok(Some(after)) = d::optional_location(v, parent, target_name) else {
                return Replace::Io;
            };
            if !n::same_ownership(after.entry, staged.entry) {
                return Replace::Io;
            }
        } else {
            if backup.is_some() {
                return Replace::Conflict;
            }
            if !d::alias(v, parent, target_name, staged.entry) || !s::flush_volume(v) {
                return Replace::Io;
            }
        }
        let Ok(stage_after) = d::optional_location(v, parent, staged_name) else {
            return Replace::Io;
        };
        if let Some(after) = stage_after {
            if !n::same_ownership(after.entry, staged.entry) {
                return Replace::Conflict;
            }
            if !d::detach(v, after) || !s::flush_volume(v) {
                return Replace::Io;
            }
        }
        Replace::Ok
    }
}
unsafe fn ownership_flush(v: Volume) -> bool {
    unsafe { s::flush_volume(v) || s::flush_volume(v) }
}
#[inline(never)]
pub(super) unsafe fn replace_create(
    v: Volume,
    parent: u32,
    target_name: Bytes,
    staged_name: Bytes,
    backup_name: Bytes,
) -> Replace {
    unsafe {
        if !n::valid(target_name.view())
            || !n::valid_short(staged_name.view())
            || !n::valid_short(backup_name.view())
        {
            return Replace::Invalid;
        }
        s::invalidate_append();
        s::invalidate_extents();
        if !n::valid_short(target_name.view()) {
            return Replace::NotAtomic;
        }
        if n::equal_ascii(target_name.view(), staged_name.view())
            || n::equal_ascii(target_name.view(), backup_name.view())
            || n::equal_ascii(staged_name.view(), backup_name.view())
        {
            return Replace::Alias;
        }
        let Ok(target) = d::optional_location(v, parent, target_name) else {
            return Replace::Io;
        };
        let Ok(staged) = d::optional_location(v, parent, staged_name) else {
            return Replace::Io;
        };
        let Ok(backup) = d::optional_location(v, parent, backup_name) else {
            return Replace::Io;
        };
        if backup.is_some() {
            return Replace::Conflict;
        }
        let Some(staged) = staged else {
            return if target.is_none() {
                Replace::NotFound
            } else {
                Replace::Conflict
            };
        };
        if staged.entry.is_dir() || staged.entry.read_only() {
            return Replace::ReadOnly;
        }
        if let Some(t) = target {
            if !n::same_ownership(t.entry, staged.entry) {
                return Replace::Conflict;
            }
        } else {
            if !d::alias(v, parent, target_name, staged.entry) {
                let Ok(Some(t)) = d::optional_location(v, parent, target_name) else {
                    return Replace::Io;
                };
                if !n::same_ownership(t.entry, staged.entry) {
                    return Replace::Conflict;
                }
            }
            if !ownership_flush(v) {
                return Replace::Io;
            }
            let Ok(Some(t)) = d::optional_location(v, parent, target_name) else {
                return Replace::Io;
            };
            if !n::same_ownership(t.entry, staged.entry) {
                return Replace::Conflict;
            }
        }
        let Ok(stage_after) = d::optional_location(v, parent, staged_name) else {
            return Replace::Io;
        };
        if let Some(alias) = stage_after {
            if !n::same_ownership(alias.entry, staged.entry) {
                return Replace::Conflict;
            }
            if !d::detach(v, alias)
                && !matches!(d::optional_location(v, parent, staged_name), Ok(None))
            {
                return Replace::Io;
            }
        }
        if !ownership_flush(v) {
            return Replace::Io;
        }
        Replace::Ok
    }
}
