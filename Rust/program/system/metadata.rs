//! Namespace-qualified observations over the canonical VFS and scan owner.
use super::{context as ctx, directory, fs, name_limits, path};
use crate::{contract as c, fs::directory_changes as changes, memory::owner, sched::task_context};
use core::mem::size_of;

pub(super) fn checked_node(node: u64) -> Result<u32, i32> {
    u32::try_from(node).map_err(|_| c::file_metadata_error_unrepresentable)
}

pub(super) fn legacy(entry: &fs::Entry, letter: u8) -> Result<c::FileInfo, i32> {
    let mut info = c::FileInfo {
        exists: 1,
        is_dir: u8::from(entry.is_dir()),
        drive: letter,
        attr: entry.attr,
        size: entry.size,
        first_cluster: checked_node(entry.node)?,
        created_time: entry.times[0],
        created_date: entry.times[1],
        access_date: entry.times[2],
        modified_time: entry.times[3],
        modified_date: entry.times[4],
        ..Default::default()
    };
    path::fixed_z(&mut info.name, &entry.name[..entry.name_len]);
    Ok(info)
}

/// One copied entry; never a retain, stable infinite birth ID or content epoch.
pub(super) fn project(
    volume: fs::Volume,
    entry: &fs::Entry,
    letter: u8,
    namespace: u64,
    root: bool,
) -> Result<c::FileMetadata2, i32> {
    if !volume.has_ref || volume.reference.generation == 0 {
        return Err(c::file_metadata2_error_unsupported);
    }
    let provider = match volume.kind {
        0 => c::file_name_provider_fat32,
        1 => c::file_name_provider_ntfs,
        _ => return Err(c::file_metadata2_error_unsupported),
    };
    let (object_hi, object_lo, kind) = if root {
        (entry.node, 0, c::file_identity2_flag_root)
    } else if volume.kind == 0 {
        if !entry.has_lba || !entry.directory_offset.is_multiple_of(32) {
            return Err(c::file_metadata2_error_unsupported);
        }
        (
            entry.directory_lba,
            u64::from(entry.directory_offset),
            c::file_identity2_flag_fat_slot,
        )
    } else {
        (
            entry.node,
            u64::from(entry.generation),
            c::file_identity2_flag_ntfs_record,
        )
    };
    let mut result = c::FileMetadata2 {
        identity: c::FileIdentity2 {
            mount_generation: volume.reference.generation,
            namespace_generation: namespace,
            object_hi,
            object_lo,
            mount_slot: volume.reference.slot,
            provider,
            flags: c::file_identity2_flag_namespace | kind,
            ..Default::default()
        },
        backend_node: entry.node,
        flags: c::file_metadata2_flag_exists
            | c::file_metadata2_flag_identity_valid
            | c::file_metadata2_flag_backend_node_valid,
        attributes: u32::from(entry.attr),
        drive_letter: u32::from(letter),
        ..Default::default()
    };
    if entry.is_dir() {
        result.flags |= c::file_metadata2_flag_directory;
    } else {
        result.byte_size = entry.size;
        result.flags |= c::file_metadata2_flag_byte_size_valid;
    }
    if entry.reparse {
        result.flags |= c::file_metadata2_flag_reparse;
    }
    if !root {
        result.created_time = entry.times[0];
        result.created_date = entry.times[1];
        result.access_date = entry.times[2];
        result.modified_time = entry.times[3];
        result.modified_date = entry.times[4];
        result.flags |= c::file_metadata2_flag_dos_time_valid;
    }
    // No provider currently certifies a content revision. DOS timestamps and
    // backend nodes are intentionally never promoted into one.
    Ok(result)
}

fn unchanged(namespace: u64) -> Result<(), i32> {
    match changes::namespace_generation() {
        Some(current) if current == namespace => Ok(()),
        Some(_) => Err(c::file_metadata2_error_restart),
        None => Err(c::file_metadata2_error_namespace_exhausted),
    }
}

/// # Safety
/// Readable bounded NUL path and aligned exclusive output cover the actual
/// synchronous request. No caller pointer or mount lease survives return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_metadata2(
    input: *const u8,
    out: *mut c::FileMetadata2,
) -> i32 {
    if input.is_null()
        || out.is_null()
        || !out.is_aligned()
        || crate::program::threads::compute_worker()
    {
        return c::file_metadata2_error_invalid;
    }
    let Some(_caller) = task_context::Admission::acquire(true) else {
        return c::file_metadata2_error_io;
    };
    owner::prepare_stack();
    unsafe {
        let mut raw_buffer = path::Buffer::<1024>::new();
        let Some(raw) = path::copy_z(input, &mut raw_buffer) else {
            return c::file_metadata2_error_path_bounds;
        };
        let mut normalized = path::Buffer::<1024>::new();
        let target = match name_limits::resolve(raw, &mut normalized) {
            Ok(value) => value,
            Err(c::file_stream_error_name_too_long) => {
                return c::file_metadata2_error_path_bounds;
            }
            Err(_) => return c::file_metadata2_error_invalid,
        };
        let Some(volume) = ctx::volume(target) else {
            return c::file_metadata2_error_no_mount;
        };
        let letter = (*target.drive_ref).letter;
        let Some(mut request) = fs::Request::volume(12, letter, volume) else {
            return c::file_metadata2_error_io;
        };
        let Some(namespace) = changes::namespace_generation() else {
            return c::file_metadata2_error_namespace_exhausted;
        };
        let relative = target.path.slice();
        let root = path::root(relative);
        let mut entry = fs::Entry::EMPTY;
        if root {
            entry.node = fs::scalar(volume, 2);
            entry.attr = 0x10;
        } else {
            match fs::resolve(volume, relative, &mut entry) {
                0 => {}
                1 => {
                    if let Err(code) = unchanged(namespace) {
                        return code;
                    }
                    request.ok = true;
                    return c::file_metadata2_result_missing;
                }
                _ => return c::file_metadata2_error_io,
            }
        }
        let info = match project(volume, &entry, letter, namespace, root) {
            Ok(value) => value,
            Err(code) => return code,
        };
        if let Err(code) = unchanged(namespace) {
            return code;
        }
        out.write(info);
        request.ok = true;
        c::file_metadata2_result_found
    }
}

/// # Safety
/// Exclusive initialized opaque cursor and disjoint mapped whole output rows
/// remain borrowed until real return. Error rows have no adoption authority.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_directory_metadata_batch2(
    input: *const u8,
    cursor: *mut c::DirectoryMetadataCursor2,
    out: *mut c::DirectoryMetadataEntry2,
    capacity: u32,
) -> i32 {
    if cursor.is_null()
        || !cursor.is_aligned()
        || capacity > c::directory_batch_max_entries
        || capacity != 0 && (out.is_null() || !out.is_aligned())
        || crate::program::threads::compute_worker()
    {
        return c::file_metadata2_error_invalid;
    }
    let Some(_caller) = task_context::Admission::acquire(true) else {
        return c::file_metadata2_error_io;
    };
    owner::prepare_stack();
    unsafe {
        let mut next = cursor.read();
        if next.version != c::file_metadata2_version
            || next.size != size_of::<c::DirectoryMetadataCursor2>() as u32
            || next.flags & !c::directory_metadata2_flag_bound != 0
            || next.reserved0 != 0
        {
            return c::file_metadata2_error_invalid;
        }
        let bound = next.flags & c::directory_metadata2_flag_bound != 0;
        if bound != (next.scan.change.mount_generation != 0)
            || !bound && next.namespace_generation != 0
        {
            return c::file_metadata2_error_invalid;
        }
        let Some(namespace) = changes::namespace_generation() else {
            return c::file_metadata2_error_namespace_exhausted;
        };
        if bound && next.namespace_generation != namespace {
            return c::file_metadata2_error_restart;
        }
        let result = directory::directory_batch_core(
            input,
            &mut next.scan,
            out.cast(),
            capacity,
            Some(namespace),
        );
        if result < 0 && result != c::file_metadata2_result_end {
            return result;
        }
        if let Err(code) = unchanged(namespace) {
            return code;
        }
        next.namespace_generation = namespace;
        next.flags = c::directory_metadata2_flag_bound;
        cursor.write(next);
        result
    }
}
