//! Original VFS vocabulary over the one native per-mount NTFS owner.
use super::{
    ntfs_adapter_state as s,
    ntfs_adapter_types::*,
    ntfs_format_types::{Buffer, View},
    ntfs_volume_append as append, ntfs_volume_atomic as atomic, ntfs_volume_canonical as canonical,
    ntfs_volume_directories as dirs, ntfs_volume_directory as directory,
    ntfs_volume_entry as core_entry, ntfs_volume_files as files, ntfs_volume_hardlinks as links,
    ntfs_volume_lookup as lookup, ntfs_volume_names as diagnostic, ntfs_volume_records as records,
    ntfs_volume_rename as rename, ntfs_volume_sparse as sparse, ntfs_volume_types as t,
    vfs_types::{Location, Result as Output},
};
use core::{
    ptr,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};
static RECLAIM_CURSOR: AtomicUsize = AtomicUsize::new(0);
fn result(value: Option<usize>) -> Output {
    value.map_or(Output::default(), |x| Output {
        value: x as u64,
        status: 0,
        present: true,
    })
}
fn status(value: t::WriteStatus) -> Output {
    Output {
        value: 0,
        status: value as i32,
        present: value == t::WriteStatus::Ok,
    }
}
fn lookup_result(value: t::Lookup) -> Output {
    Output {
        value: 0,
        status: value as i32,
        present: value == t::Lookup::Found,
    }
}
unsafe fn entry_out(out: *mut Entry, value: t::Entry) {
    unsafe {
        out.write(s::project(value));
    }
}
unsafe fn append_bytes(out: Buffer, cursor: &mut usize, bytes: &[u8]) -> bool {
    if *cursor + bytes.len() >= out.len {
        return false;
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out.data.add(*cursor), bytes.len());
    }
    *cursor += bytes.len();
    true
}
#[inline(never)]
unsafe fn resolve(a: Args, out: *mut Entry) -> Output {
    unsafe {
        let mut found = t::ResolvedEntry {
            entry: t::Entry::EMPTY,
            parent_record: 0,
        };
        let st = lookup::resolve_entry(s::build(a.volume), a.name, &mut found);
        if st == t::Lookup::Found {
            entry_out(out, found.entry);
        }
        lookup_result(st)
    }
}
#[inline(never)]
unsafe fn child(a: Args, out: *mut Entry, transient: bool) -> Output {
    unsafe {
        let mut found = t::LookupResult::EMPTY;
        let st = lookup::lookup(
            s::build(a.volume),
            a.number,
            a.name,
            &mut found,
            transient,
            0,
        );
        if st == t::Lookup::Found {
            entry_out(out, found.entry);
        }
        lookup_result(st)
    }
}
#[inline(never)]
unsafe fn read(a: Args, whole: bool) -> Output {
    unsafe {
        let e = a.entry.read();
        if e.is_dir() || e.reparse || (whole && e.size > a.buffer.len as u64) {
            return Output::default();
        }
        let v = s::build(a.volume);
        if lookup::identity(v, e.record, e.sequence, false) != t::Lookup::Found {
            return Output::default();
        }
        let out = if whole {
            Buffer {
                data: a.buffer.data,
                len: e.size as usize,
            }
        } else {
            a.buffer
        };
        result(core_entry::read_file_guarded(
            v,
            e.record,
            None,
            if whole { 0 } else { a.number as usize },
            out,
            a.read_guard,
            a.read_reference,
        ))
    }
}
#[inline(never)]
unsafe fn read_directory(a: Args) -> Option<usize> {
    unsafe {
        let mut sink = directory::Sink {
            out: Some(a.buffer),
            max_entries: 64,
            ..directory::Sink::EMPTY
        };
        if !append_bytes(a.buffer, &mut sink.cursor, b".\r\n")
            || !append_bytes(a.buffer, &mut sink.cursor, b"..\r\n")
            || !directory::enumerate(s::build(a.volume), a.number, &mut sink)
        {
            return None;
        }
        if sink.cursor < a.buffer.len {
            a.buffer.data.add(sink.cursor).write(0);
        }
        Some(sink.cursor)
    }
}
#[inline(never)]
unsafe fn read_directory_entry(a: Args, out: *mut Entry) -> Output {
    unsafe {
        let index = a.second as usize;
        if index < 2 {
            if a.buffer.len < 3 {
                return lookup_result(t::Lookup::Io);
            }
            let mut entry = Entry {
                name_len: index + 1,
                attr: 16,
                record: a.number,
                ..Entry::EMPTY
            };
            entry.name[0] = b'.';
            a.buffer.data.write(b'.');
            if index == 1 {
                entry.name[1] = b'.';
                a.buffer.data.add(1).write(b'.');
            }
            a.buffer.data.add(entry.name_len).write(0);
            out.write(entry);
            return lookup_result(t::Lookup::Found);
        }
        let mut sink = directory::Sink {
            wanted: index - 2,
            ..directory::Sink::EMPTY
        };
        if !directory::enumerate(s::build(a.volume), a.number, &mut sink) {
            return lookup_result(t::Lookup::Io);
        }
        let Some(found) = sink.found else {
            return lookup_result(t::Lookup::NotFound);
        };
        if found.name_len + 1 > a.buffer.len {
            return lookup_result(t::Lookup::Io);
        }
        ptr::copy_nonoverlapping(found.name.as_ptr(), a.buffer.data, found.name_len);
        a.buffer.data.add(found.name_len).write(0);
        entry_out(out, found);
        lookup_result(t::Lookup::Found)
    }
}
#[inline(never)]
unsafe fn next(a: Args, out: *mut Entry) -> u64 {
    unsafe {
        let mut e = t::Entry::EMPTY;
        let st = directory::next(s::build(a.volume), a.number, a.state.cast(), &mut e);
        if st == t::DirectoryNext::Found {
            entry_out(out, e);
        }
        st as u64
    }
}
/// Typed bounded enumeration; only value projections cross the backend owner.
pub(crate) unsafe fn directory_batch(
    volume: Mounted,
    node: u64,
    cursor: *mut u64,
    capacity: usize,
    mut emit: impl FnMut(Entry),
) -> (usize, u8) {
    let (count, status) = unsafe {
        directory::batch(s::build(volume), node, cursor.cast(), capacity, |entry| {
            emit(s::project(entry))
        })
    };
    (count, status as u8)
}
#[inline(never)]
unsafe fn list_root(a: Args) -> bool {
    unsafe {
        super::fat_state::log(b"      NTFS root:\r\n");
        let mut bytes = [0u8; 1024];
        let mut a = a;
        a.number = 5;
        a.buffer = Buffer {
            data: bytes.as_mut_ptr(),
            len: 1024,
        };
        let Some(n) = read_directory(a) else {
            return false;
        };
        let mut start = 0;
        let mut shown = 0;
        for i in 0..n {
            if shown >= 12 {
                break;
            }
            if bytes[i] == b'\n' {
                super::fat_state::log(b"        ");
                super::fat_state::log(&bytes[start..i + 1]);
                start = i + 1;
                shown += 1;
            }
        }
        true
    }
}
#[inline(never)]
unsafe fn delete_identity(a: Args) -> i32 {
    unsafe {
        let v = s::build(a.volume);
        let expected = a.entry.read();
        let mut found = t::LookupResult::EMPTY;
        match lookup::lookup(v, a.number, a.name, &mut found, false, 0) {
            t::Lookup::NotFound => return if append::finish(v) { 1 } else { 3 },
            t::Lookup::Io => return 3,
            t::Lookup::Found => {}
        }
        if found.entry.is_dir()
            || found.record != expected.record
            || found.sequence != expected.sequence
            || found.entry.size != expected.size
        {
            return 2;
        }
        let st = files::delete(v, a.number, a.name);
        if st == t::WriteStatus::Ok {
            return 0;
        }
        if st != t::WriteStatus::Io && st != t::WriteStatus::NotFound {
            return 2;
        }
        match lookup::lookup(v, a.number, a.name, &mut found, false, 0) {
            t::Lookup::NotFound => {
                if append::finish(v) {
                    0
                } else {
                    3
                }
            }
            t::Lookup::Found => {
                if found.record == expected.record && found.sequence == expected.sequence {
                    3
                } else {
                    2
                }
            }
            t::Lookup::Io => 3,
        }
    }
}
#[inline(never)]
unsafe fn recovery_delete(a: Args) -> i32 {
    unsafe {
        let v = s::build(a.volume);
        let e = a.entry.read();
        let st = links::recover_delete(v, a.number, a.name, e.record, e.sequence);
        match st {
            links::RecoveryDelete::Ok => {
                if append::finish(v) {
                    0
                } else {
                    6
                }
            }
            links::RecoveryDelete::Unlinked => {
                if append::finish(v) {
                    1
                } else {
                    6
                }
            }
            links::RecoveryDelete::NotFound => {
                if append::finish(v) {
                    2
                } else {
                    6
                }
            }
            links::RecoveryDelete::Mismatch => 3,
            links::RecoveryDelete::Directory => 4,
            links::RecoveryDelete::Unsupported => 5,
            links::RecoveryDelete::Io => 6,
        }
    }
}
#[inline(never)]
unsafe fn child_size(a: Args) -> Output {
    unsafe {
        let v = s::build(a.volume);
        let mut found = t::LookupResult::EMPTY;
        let st = lookup::lookup(v, a.number, a.name, &mut found, false, 0);
        if st != t::Lookup::Found {
            return lookup_result(st);
        }
        if found.entry.is_dir() {
            return lookup_result(t::Lookup::Io);
        }
        let mut attr = t::AttrScratch::EMPTY;
        if records::collect(v, found.record, 0x80, View::default(), &mut attr) != t::Lookup::Found {
            return lookup_result(t::Lookup::Io);
        }
        Output {
            value: attr.data_size,
            status: 0,
            present: true,
        }
    }
}
fn whole_progress(st: t::WriteStatus, length: usize) -> t::WriteProgress {
    t::WriteProgress {
        status: st,
        completed: if st == t::WriteStatus::Ok { length } else { 0 },
        uncertain: matches!(st, t::WriteStatus::Io | t::WriteStatus::CleanupFailed),
    }
}
#[inline(never)]
unsafe fn write_range(a: Args) -> t::WriteProgress {
    unsafe {
        let e = a.entry.read();
        if e.is_dir() {
            return whole_progress(t::WriteStatus::Directory, 0);
        }
        let v = s::build(a.volume);
        if lookup::identity(v, e.record, e.sequence, false) != t::Lookup::Found {
            return whole_progress(t::WriteStatus::Io, 0);
        }
        let Some(end) = a.number.checked_add(a.name.len as u64) else {
            return whole_progress(t::WriteStatus::OffsetMismatch, 0);
        };
        if end <= e.size {
            return sparse::write_at(v, e.record, a.number, a.name);
        }
        let st = lookup::single_link(v, e.record, e.sequence);
        if st != t::WriteStatus::Ok {
            return whole_progress(st, 0);
        }
        let Some(location) = lookup::parent_name(v, e.record) else {
            return whole_progress(t::WriteStatus::Io, 0);
        };
        let name = View::of(&location.name[..location.name_len]);
        // In-place sparse writes above retain their existing metadata contract.
        // Growing/replacing the file needs a new timestamp before any mutation.
        let Some(v) = s::with_write_time(v) else {
            return t::WriteProgress {
                status: t::WriteStatus::Io,
                completed: 0,
                uncertain: false,
            };
        };
        if a.number == e.size {
            return whole_progress(
                append::append(v, location.parent, name, a.number, a.name, true),
                a.name.len,
            );
        }
        if a.number == 0 {
            return whole_progress(files::write(v, location.parent, name, a.name), a.name.len);
        }
        whole_progress(t::WriteStatus::OffsetMismatch, 0)
    }
}

/// # Safety
/// Private initialized C values, writable output for the selected operation;
/// existing storage and per-volume request admission spans every callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ntfs_adapter(
    op: u8,
    input: *const Args,
    out: *mut core::ffi::c_void,
) -> Output {
    unsafe {
        let a = input.read();
        match op {
            0 | 1 => {
                if let Some(v) = s::inspect(
                    a.device,
                    a.number as u32,
                    if op == 1 { Some(a.count) } else { None },
                ) {
                    out.cast::<Mounted>().write(v);
                    Output {
                        present: true,
                        ..Output::default()
                    }
                } else {
                    Output::default()
                }
            }
            2 => {
                let slot = usize::from(a.volume.state_slot);
                if slot >= s::VOLUMES {
                    return Output::default();
                }
                let st = s::state(slot);
                if !(*st).in_use {
                    return Output::default();
                }
                out.cast::<Location>().write(Location {
                    device: (*st).device,
                    first: u64::from((*st).partition),
                    count: (*st).sectors,
                });
                Output {
                    present: true,
                    ..Output::default()
                }
            }
            3 => {
                for i in 0..s::VOLUMES {
                    let cache = {
                        let _owner = crate::memory::owner::Guard::acquire(1);
                        let st = s::state(i);
                        if (*st).in_use
                            && (*st).device == a.device
                            && u64::from((*st).partition) >= a.number
                            && u64::from((*st).partition) - a.number < a.count
                        {
                            Some(s::cache(st))
                        } else {
                            None
                        }
                    };
                    if let Some(cache) = cache {
                        super::ntfs_decoded_cache::forget(cache);
                    }
                }
                let _owner = crate::memory::owner::Guard::acquire(1);
                for i in 0..s::VOLUMES {
                    let st = s::state(i);
                    if !(*st).in_use
                        || (*st).device != a.device
                        || u64::from((*st).partition) < a.number
                        || u64::from((*st).partition) - a.number >= a.count
                    {
                        continue;
                    }
                    s::release(st);
                }
                Output::default()
            }
            4 => {
                let _owner = crate::memory::owner::Guard::acquire(1);
                for i in 0..s::VOLUMES {
                    let st = s::state(i);
                    if (*st).in_use && (*st).device == a.device {
                        super::ntfs_volume_cache::with(s::cache(st), |cache| {
                            cache.invalidate_external()
                        });
                    }
                }
                Output::default()
            }
            5 => {
                out.cast::<t::MetadataReclaimResult>()
                    .write(reclaim(a.count as u32));
                Output::default()
            }
            6 => {
                out.cast::<t::MetadataCacheSummary>().write(summary());
                Output::default()
            }
            7 => {
                let mut n = 0;
                let st = lookup::resolve_path(s::build(a.volume), a.name, &mut n);
                Output {
                    value: n,
                    status: st as i32,
                    present: st == t::Lookup::Found,
                }
            }
            8 => resolve(a, out.cast()),
            9 | 10 => child(a, out.cast(), op == 10),
            11 | 12 => read(a, op == 11),
            13 => result(read_directory(a)),
            14 => read_directory_entry(a, out.cast()),
            15 => Output {
                value: next(a, out.cast()),
                ..Output::default()
            },
            16 => Output {
                present: list_root(a),
                ..Output::default()
            },
            17 => status(s::build_write(a.volume).map_or(t::WriteStatus::Io, |v| {
                files::write(v, a.number, a.name, a.other)
            })),
            18 => status(s::build_write(a.volume).map_or(t::WriteStatus::Io, |v| {
                files::create(v, a.number, a.name, a.other)
            })),
            19 | 20 => status(s::build_write(a.volume).map_or(t::WriteStatus::Io, |v| {
                append::append(v, a.number, a.name, a.second, a.other, op == 19)
            })),
            21 => Output {
                value: u64::from(diagnostic::append_stage()),
                ..Output::default()
            },
            22 => Output {
                value: u64::from(diagnostic::lookup_stage()),
                ..Output::default()
            },
            23 => Output {
                present: append::finish(s::build(a.volume)),
                ..Output::default()
            },
            24 => status(files::delete(s::build(a.volume), a.number, a.name)),
            25 => Output {
                status: delete_identity(a),
                ..Output::default()
            },
            26 => Output {
                status: recovery_delete(a),
                ..Output::default()
            },
            27 => canonical::equal(s::build(a.volume), a.name, a.other)
                .filter(|_| {
                    a.name.len != 0 && a.name.len <= 768 && a.other.len != 0 && a.other.len <= 768
                })
                .map_or(Output::default(), |x| Output {
                    value: u64::from(x),
                    present: true,
                    status: 0,
                }),
            28 => status(
                s::build_write(a.volume)
                    .map_or(t::WriteStatus::Io, |v| dirs::create(v, a.number, a.name)),
            ),
            29 => status(dirs::delete(s::build(a.volume), a.number, a.name)),
            30 => {
                let st = s::build_write(a.volume).map_or(t::WriteStatus::Io, |v| {
                    rename::rename(v, a.number, a.name, a.number, a.other)
                });
                Output {
                    status: match st {
                        t::WriteStatus::Ok => 0,
                        t::WriteStatus::NotFound => 1,
                        t::WriteStatus::Exists | t::WriteStatus::Invalid => 3,
                        _ => 4,
                    },
                    ..Output::default()
                }
            }
            31 => status(s::build_write(a.volume).map_or(t::WriteStatus::Io, |v| {
                rename::rename(v, a.number, a.name, a.second, a.other)
            })),
            32 => super::ntfs_metadata_read::free_count(
                s::build(a.volume),
                a.read_guard,
                a.read_reference,
            )
            .map_or(Output::default(), |x| Output {
                value: x.min(u64::from(u32::MAX)),
                present: true,
                status: 0,
            }),
            33 => Output {
                status: s::build_write(a.volume).map_or(atomic::ReplaceResult::Io, |v| {
                    atomic::replace(v, a.number, a.name, a.other, a.third, a.flag)
                }) as i32,
                ..Output::default()
            },
            34 => Output {
                status: s::build_write(a.volume).map_or(atomic::ReplaceResult::Io, |v| {
                    atomic::publish(v, a.number, a.name, a.other)
                }) as i32,
                ..Output::default()
            },
            35 => publish_absent_backup(a),
            36 => child_size(a),
            37 => {
                out.cast::<t::WriteProgress>().write(write_range(a));
                Output::default()
            }
            38 => Output {
                value: 5,
                ..Output::default()
            },
            _ => panic!("NTFS adapter operation"),
        }
    }
}
#[inline(never)]
unsafe fn publish_absent_backup(a: Args) -> Output {
    unsafe {
        let v = s::build(a.volume);
        let mut backup = t::LookupResult::EMPTY;
        let st = match lookup::lookup(v, a.number, a.third, &mut backup, false, 0) {
            t::Lookup::Found => atomic::ReplaceResult::Conflict,
            t::Lookup::Io => atomic::ReplaceResult::Io,
            t::Lookup::NotFound => s::with_write_time(v).map_or(atomic::ReplaceResult::Io, |v| {
                atomic::publish(v, a.number, a.name, a.other)
            }),
        };
        Output {
            status: st as i32,
            ..Output::default()
        }
    }
}
pub(crate) unsafe fn reclaim(requested: u32) -> t::MetadataReclaimResult {
    let _owner = crate::memory::owner::Guard::acquire(1);
    unsafe {
        let requested = requested.min(8);
        let mut result = t::MetadataReclaimResult {
            requested_entries: requested,
            inspected_entries: 0,
            reclaimed_entries: 0,
        };
        if requested == 0 {
            return result;
        }
        for _ in 0..s::VOLUMES {
            if result.reclaimed_entries >= requested {
                break;
            }
            let i = RECLAIM_CURSOR.load(Relaxed);
            RECLAIM_CURSOR.store((i + 1) % s::VOLUMES, Relaxed);
            let st = s::state(i);
            if !(*st).in_use {
                continue;
            }
            let one = super::ntfs_volume_cache::with(s::cache(st), |cache| {
                cache.reclaim(requested - result.reclaimed_entries)
            });
            result.inspected_entries = result
                .inspected_entries
                .saturating_add(one.inspected_entries);
            result.reclaimed_entries = result
                .reclaimed_entries
                .saturating_add(one.reclaimed_entries);
        }
        result
    }
}

pub(crate) unsafe fn summary() -> t::MetadataCacheSummary {
    let _owner = crate::memory::owner::Guard::acquire(1);
    unsafe {
        let mut out = t::MetadataCacheSummary::EMPTY;
        out.active_volumes = 0;
        out.mount_generation = 0;
        out.content_generation = 0;
        out.negative_ttl_ticks = 0;
        for i in 0..s::VOLUMES {
            let st = s::state(i);
            if !(*st).in_use {
                continue;
            }
            let one = super::ntfs_volume_cache::with(s::cache(st), |cache| cache.summary());
            out.bytes_per_volume = out.bytes_per_volume.max(one.bytes_per_volume);
            out.mount_generation = out.mount_generation.max(one.mount_generation);
            out.content_generation = out.content_generation.max(one.content_generation);
            out.negative_ttl_ticks = out.negative_ttl_ticks.max(one.negative_ttl_ticks);
            out.active_volumes = out.active_volumes.saturating_add(1);
            out.record_entries = out.record_entries.saturating_add(one.record_entries);
            out.attribute_entries = out.attribute_entries.saturating_add(one.attribute_entries);
            out.index_entries = out.index_entries.saturating_add(one.index_entries);
            out.path_entries = out.path_entries.saturating_add(one.path_entries);
            out.record_hits = out.record_hits.wrapping_add(one.record_hits);
            out.record_misses = out.record_misses.wrapping_add(one.record_misses);
            out.record_stores = out.record_stores.wrapping_add(one.record_stores);
            out.record_evictions = out.record_evictions.wrapping_add(one.record_evictions);
            out.attribute_hits = out.attribute_hits.wrapping_add(one.attribute_hits);
            out.attribute_misses = out.attribute_misses.wrapping_add(one.attribute_misses);
            out.attribute_stores = out.attribute_stores.wrapping_add(one.attribute_stores);
            out.attribute_evictions = out
                .attribute_evictions
                .wrapping_add(one.attribute_evictions);
            out.index_hits = out.index_hits.wrapping_add(one.index_hits);
            out.index_misses = out.index_misses.wrapping_add(one.index_misses);
            out.index_stores = out.index_stores.wrapping_add(one.index_stores);
            out.index_evictions = out.index_evictions.wrapping_add(one.index_evictions);
            out.path_queries = out.path_queries.wrapping_add(one.path_queries);
            out.path_positive_hits = out.path_positive_hits.wrapping_add(one.path_positive_hits);
            out.path_negative_hits = out.path_negative_hits.wrapping_add(one.path_negative_hits);
            out.path_misses = out.path_misses.wrapping_add(one.path_misses);
            out.path_positive_stores = out
                .path_positive_stores
                .wrapping_add(one.path_positive_stores);
            out.path_negative_stores = out
                .path_negative_stores
                .wrapping_add(one.path_negative_stores);
            out.path_expirations = out.path_expirations.wrapping_add(one.path_expirations);
            out.lookup_tree_walks = out.lookup_tree_walks.wrapping_add(one.lookup_tree_walks);
            out.recovery_cache_bypasses = out
                .recovery_cache_bypasses
                .wrapping_add(one.recovery_cache_bypasses);
            out.mount_invalidations = out
                .mount_invalidations
                .wrapping_add(one.mount_invalidations);
            out.mutation_invalidations = out
                .mutation_invalidations
                .wrapping_add(one.mutation_invalidations);
            out.external_invalidations = out
                .external_invalidations
                .wrapping_add(one.external_invalidations);
            out.invalidated_entries = out
                .invalidated_entries
                .wrapping_add(one.invalidated_entries);
            out.payload_write_retentions = out
                .payload_write_retentions
                .wrapping_add(one.payload_write_retentions);
            out.system_write_retentions = out
                .system_write_retentions
                .wrapping_add(one.system_write_retentions);
            out.targeted_invalidations = out
                .targeted_invalidations
                .wrapping_add(one.targeted_invalidations);
            out.targeted_record_invalidations = out
                .targeted_record_invalidations
                .wrapping_add(one.targeted_record_invalidations);
            out.targeted_attribute_invalidations = out
                .targeted_attribute_invalidations
                .wrapping_add(one.targeted_attribute_invalidations);
            out.targeted_directory_invalidations = out
                .targeted_directory_invalidations
                .wrapping_add(one.targeted_directory_invalidations);
            out.global_mutation_invalidations = out
                .global_mutation_invalidations
                .wrapping_add(one.global_mutation_invalidations);
            out.recovery_invalidations = out
                .recovery_invalidations
                .wrapping_add(one.recovery_invalidations);
            out.mutation_invalidated_record_entries = out
                .mutation_invalidated_record_entries
                .wrapping_add(one.mutation_invalidated_record_entries);
            out.mutation_invalidated_attribute_entries = out
                .mutation_invalidated_attribute_entries
                .wrapping_add(one.mutation_invalidated_attribute_entries);
            out.mutation_invalidated_index_entries = out
                .mutation_invalidated_index_entries
                .wrapping_add(one.mutation_invalidated_index_entries);
            out.mutation_invalidated_path_entries = out
                .mutation_invalidated_path_entries
                .wrapping_add(one.mutation_invalidated_path_entries);
            out.reclaim_requests = out.reclaim_requests.wrapping_add(one.reclaim_requests);
            out.reclaim_scans = out.reclaim_scans.wrapping_add(one.reclaim_scans);
            out.reclaimed_entries = out.reclaimed_entries.wrapping_add(one.reclaimed_entries);
        }
        out
    }
}
