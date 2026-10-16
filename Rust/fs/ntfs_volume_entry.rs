//! Private C value boundary; native callers use the volume functions directly.
use super::{
    ntfs_format_names as text, ntfs_format_types::*, ntfs_volume_append as append,
    ntfs_volume_atomic as atomic, ntfs_volume_cache::Cache, ntfs_volume_canonical as canonical,
    ntfs_volume_cleanup as cleanup, ntfs_volume_clusters as clusters,
    ntfs_volume_directories as directories, ntfs_volume_directory as directory,
    ntfs_volume_extension as extension, ntfs_volume_files as files, ntfs_volume_hardlinks as links,
    ntfs_volume_io as io, ntfs_volume_lookup as lookup, ntfs_volume_read as read,
    ntfs_volume_records as r, ntfs_volume_rename as rename, ntfs_volume_sparse as sparse,
    ntfs_volume_types::*,
};
use core::{ffi::c_void, ptr, sync::atomic::Ordering::Relaxed};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    pub volume: *const Volume,
    pub state: *mut c_void,
    pub out: *mut c_void,
    pub buffer: Buffer,
    pub name: View,
    pub other: View,
    pub third: View,
    pub number: u64,
    pub second: u64,
    pub third_number: u64,
    pub kind: u32,
    pub flag: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EnumSink {
    pub out: Buffer,
    pub has_out: bool,
    pub cursor: usize,
    pub listed: usize,
    pub max_entries: usize,
    pub wanted: usize,
    pub seen: usize,
    pub found: Entry,
    pub has_found: bool,
    pub failed: bool,
    pub count_ref: u64,
    pub has_count_ref: bool,
    pub matches: usize,
}
#[inline(never)]
unsafe fn enumerate(v: Volume, number: u64, state: *mut EnumSink) -> bool {
    unsafe {
        let a = state.read();
        let mut sink = directory::Sink {
            out: a.has_out.then_some(a.out),
            cursor: a.cursor,
            listed: a.listed,
            max_entries: a.max_entries,
            wanted: a.wanted,
            seen: a.seen,
            found: a.has_found.then_some(a.found),
            failed: a.failed,
            count_ref: a.has_count_ref.then_some(a.count_ref),
            matches: a.matches,
        };
        let result = directory::enumerate(v, number, &mut sink);
        (*state).cursor = sink.cursor;
        (*state).listed = sink.listed;
        (*state).seen = sink.seen;
        (*state).has_found = sink.found.is_some();
        if let Some(found) = sink.found {
            (*state).found = found;
        }
        (*state).failed = sink.failed;
        (*state).matches = sink.matches;
        result
    }
}
fn mutation(a: Args) -> Mutation {
    match a.kind {
        0 => Mutation::Payload,
        1 => Mutation::Record(a.number),
        2 => Mutation::Attribute {
            record: a.number,
            kind: a.second as u32,
        },
        3 => Mutation::Directory(a.number),
        4 => Mutation::System,
        5 => Mutation::Unknown,
        6 => Mutation::Recovery,
        _ => panic!("NTFS mutation tag"),
    }
}
unsafe fn optional<T: Copy>(out: *mut c_void, value: Option<T>) -> u64 {
    if let Some(value) = value {
        unsafe {
            out.cast::<T>().write(value);
        }
        1
    } else {
        0
    }
}

#[inline(never)]
pub(super) unsafe fn read_file(
    v: Volume,
    number: u64,
    name: Option<View>,
    offset: usize,
    out: Buffer,
) -> Option<usize> {
    unsafe {
        let mut utf16 = [0u8; 510];
        let stream = if let Some(name) = name {
            let n = text::to_utf16(name.get(), &mut utf16)?;
            View::of(&utf16[..n])
        } else {
            View::default()
        };
        let mut attr = AttrScratch::EMPTY;
        if r::collect(v, number, 0x80, stream, &mut attr) != Lookup::Found {
            return None;
        }
        read::attribute(v.io, &attr, offset, out)
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Inputs and outputs must have the private C layouts and stay resident for
/// this operation; the caller holds the existing volume gate across callbacks.
pub unsafe extern "C" fn r4_ntfs_volume(op: u32, input: *const Args) -> u64 {
    unsafe {
        let a = input.read();
        // Initializers, mount and pure diagnostics do not require a Volume.
        match op {
            0 => {
                Scratch::initialize(a.state.cast());
                return 1;
            }
            1 => {
                Cache::initialize(a.state.cast());
                return 1;
            }
            2 => {
                let v = a.volume.read();
                return optional(
                    a.out,
                    r::mount(
                        v.io.device,
                        v.geometry().partition_lba,
                        v.scratch(),
                        v.mft_runs,
                        v.mft_capacity,
                        v.io.work_counters,
                    ),
                );
            }
            40 => return u64::from(super::ntfs_volume_names::lookup_stage()),
            41 => return u64::from(super::ntfs_volume_names::append_stage()),
            42 => {
                cleanup::set_budget(a.flag.then_some(a.number as u32));
                cleanup::CUT_AFTER_CANONICAL.store(a.kind != 0, Relaxed);
                return 1;
            }
            43 => return cleanup::budget().map_or(0, |x| (1u64 << 32) | u64::from(x)),
            _ => {}
        }
        let v = a.volume.read();
        match op {
            3 => optional(a.out, r::load(v, a.number, a.buffer)),
            4 => r::collect(v, a.number, a.kind, a.name, a.out.cast()) as u64,
            5 => {
                a.out.cast::<LookupResult>().write(LookupResult::EMPTY);
                lookup::lookup(v, a.number, a.name, a.out.cast(), a.flag, 0) as u64
            }
            6 => lookup::resolve_path(v, a.name, a.out.cast()) as u64,
            7 => lookup::resolve_entry(v, a.name, a.out.cast()) as u64,
            8 => optional(a.out, lookup::parent_name(v, a.number)),
            9 => directory::next(v, a.number, a.state.cast(), a.out.cast()) as u64,
            10 => u64::from(enumerate(v, a.number, a.state.cast())),
            11 | 12 => optional(
                a.out,
                read_file(
                    v,
                    a.number,
                    (op == 12).then_some(a.name),
                    a.second as usize,
                    a.buffer,
                ),
            ),
            13 => optional(
                a.out,
                read::attribute(v.io, a.state.cast(), a.second as usize, a.buffer),
            ),
            14 => files::create(v, a.number, a.name, a.other) as u64,
            15 => files::write(v, a.number, a.name, a.other) as u64,
            16 => files::delete(v, a.number, a.name) as u64,
            17 => append::append(v, a.number, a.name, a.second, a.other, a.flag) as u64,
            18 => u64::from(append::finish(v)),
            19 => directories::create(v, a.number, a.name) as u64,
            20 => directories::delete(v, a.number, a.name) as u64,
            21 => rename::rename(v, a.number, a.name, a.second, a.other) as u64,
            22 => atomic::replace(v, a.number, a.name, a.other, a.third, a.flag) as u64,
            23 => atomic::publish(v, a.number, a.name, a.other) as u64,
            24 => {
                if a.name.len == 0
                    || a.name.len > NAME_MAX
                    || a.other.len == 0
                    || a.other.len > NAME_MAX
                {
                    return 0;
                }
                canonical::equal(v, a.name, a.other).map_or(0, |x| 1 + u64::from(x))
            }
            25 => {
                a.out
                    .cast::<WriteProgress>()
                    .write(sparse::write_at(v, a.number, a.second, a.name));
                1
            }
            26 => optional(a.out, clusters::free_count(v)),
            27 => u64::from(r::set_dirty(v, a.flag)),
            28 => r::is_dirty(v).map_or(0, |x| 1 + u64::from(x)),
            29 => u64::from(extension::resides(v, a.number, a.name)),
            30 => {
                let result = clusters::allocate(
                    v,
                    a.number,
                    clusters::RunBuffer {
                        data: a.buffer.data.cast(),
                        len: a.buffer.len,
                    },
                );
                a.out.cast::<usize>().write(result.produced);
                result.status as u64
            }
            31 => u64::from(clusters::free(
                v,
                io::Runs {
                    data: a.buffer.data.cast(),
                    len: a.buffer.len,
                },
            )),
            32 => cleanup::abort(v, status(a.kind)) as u64,
            33 => cleanup::freeing(
                v,
                io::Runs {
                    data: a.buffer.data.cast(),
                    len: a.buffer.len,
                },
                status(a.kind),
            ) as u64,
            34 => u64::from(r::store(v, a.number, v.record())),
            35 => optional(a.out, r::load(v, a.number, v.record())),
            36 | 37 => u64::from(io::lcn(
                v.io,
                a.number,
                a.second,
                a.buffer,
                if op == 37 {
                    Some(mutation(Args {
                        number: a.third_number,
                        ..a
                    }))
                } else {
                    None
                },
                ptr::null_mut(),
            )),
            38 => u64::from(clusters::set_range(
                v,
                a.state.cast(),
                a.number,
                a.second,
                a.flag,
            )),
            39 => {
                links::recover_delete(v, a.number, a.name, a.second, a.third_number as u16) as u64
            }
            44 => lookup::single_link(v, a.number, a.second as u16) as u64,
            45 => lookup::identity(v, a.number, a.second as u16, a.flag) as u64,
            46 => {
                if let Some((sequence, links)) = lookup::record_link(v, a.number) {
                    a.out
                        .cast::<u32>()
                        .write(u32::from(sequence) | (u32::from(links) << 16));
                    Lookup::Found as u64
                } else {
                    Lookup::Io as u64
                }
            }
            _ => panic!("NTFS volume operation"),
        }
    }
}
fn status(tag: u32) -> WriteStatus {
    match tag {
        0 => WriteStatus::Ok,
        1 => WriteStatus::Invalid,
        2 => WriteStatus::NotFound,
        3 => WriteStatus::Exists,
        4 => WriteStatus::Directory,
        5 => WriteStatus::NotDirectory,
        6 => WriteStatus::NotEmpty,
        7 => WriteStatus::ReadOnlyTarget,
        8 => WriteStatus::NoSpace,
        9 => WriteStatus::DirFull,
        10 => WriteStatus::RecordFull,
        11 => WriteStatus::Unsupported,
        12 => WriteStatus::OffsetMismatch,
        13 => WriteStatus::Io,
        14 => WriteStatus::CleanupFailed,
        _ => panic!("NTFS status"),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// The caller exclusively owns a properly initialized resident native Cache;
/// value arguments and output addresses follow the private C layouts.
pub unsafe extern "C" fn r4_ntfs_metadata(op: u32, input: *const Args) -> u64 {
    let _owner = crate::memory::owner::Guard::acquire(1);
    unsafe {
        let a = input.read();
        let c = a.state.cast::<Cache>();
        match op {
            0 => {
                Cache::initialize(c);
                1
            }
            1 => {
                (*c).begin_mount(a.number);
                1
            }
            2 => {
                (*c).invalidate_external();
                1
            }
            3 => {
                (*c).invalidate(mutation(a));
                1
            }
            4 => u64::from((*c).lookup_record(a.number, r::mutable(a.buffer))),
            5 => {
                (*c).store_record(a.number, a.name.get());
                1
            }
            6 => u64::from((*c).lookup_attribute(
                a.number,
                a.kind,
                a.name.get(),
                &mut *a.out.cast::<AttrScratch>(),
            )),
            7 => {
                (*c).store_attribute(
                    a.number,
                    a.kind,
                    a.name.get(),
                    &*a.out.cast::<AttrScratch>(),
                );
                1
            }
            8 => u64::from((*c).lookup_index(a.number, a.second, r::mutable(a.buffer))),
            9 => {
                (*c).store_index(a.number, a.second, a.name.get());
                1
            }
            10 => {
                a.out.cast::<LookupResult>().write(LookupResult::EMPTY);
                (*c).lookup_path(
                    a.other.get(),
                    a.number,
                    a.name.get(),
                    a.second,
                    &mut *a.out.cast::<LookupResult>(),
                ) as u64
            }
            11 => {
                (*c).store_path(
                    a.number,
                    a.name.get(),
                    if a.flag {
                        Lookup::Found
                    } else {
                        Lookup::NotFound
                    },
                    &*a.out.cast::<LookupResult>(),
                    a.second,
                );
                1
            }
            12 => {
                (*c).note_tree_walk();
                1
            }
            13 => {
                (*c).note_recovery_bypass();
                1
            }
            14 => {
                a.out
                    .cast::<MetadataReclaimResult>()
                    .write((*c).reclaim(a.kind));
                1
            }
            15 => {
                a.out.cast::<MetadataCacheSummary>().write((*c).summary());
                1
            }
            16 => optional(a.out, (*c).free_clusters(a.number)),
            17 => {
                (*c).store_free_clusters(a.number, a.second);
                1
            }
            _ => panic!("NTFS metadata operation"),
        }
    }
}
