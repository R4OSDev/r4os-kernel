//! Original directory/drive/read metadata and caller-owned cursor APIs.
use super::{
    context::{self as ctx, PathBuffers},
    fs, io, path,
};
use crate::{
    contract as c,
    fs::{directory_changes as changes, drive},
};
use core::{mem::size_of, ptr};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
pub(super) fn metadata(entry: &fs::Entry, letter: u8, truncate: bool) -> c::FileInfo {
    let mut info = c::FileInfo {
        exists: 1,
        is_dir: u8::from(entry.is_dir()),
        drive: letter,
        attr: entry.attr,
        size: entry.size,
        first_cluster: if truncate {
            entry.node as u32
        } else {
            u32::try_from(entry.node).expect("File node projection")
        },
        created_time: entry.times[0],
        created_date: entry.times[1],
        access_date: entry.times[2],
        modified_time: entry.times[3],
        modified_date: entry.times[4],
        ..Default::default()
    };
    path::fixed_z(&mut info.name, &entry.name[..entry.name_len]);
    info
}
/// # Safety
/// Original bounded drive index and exclusive public output for its mounted
/// snapshot/request; no mutable drive-table borrow spans backend I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_drive_info(index: u32, out: *mut c::DriveInfo) -> i32 {
    unsafe { drive_info(index, out, true) }
}

/// # Safety
/// Same exclusive output and exact mounted request lifetime as drive_info.
/// Scalar geometry never invokes FREE; its free fields remain unknown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_drive_info_geometry(
    index: u32,
    out: *mut c::DriveInfo,
) -> i32 {
    unsafe { drive_info(index, out, false) }
}

unsafe fn drive_info(index: u32, out: *mut c::DriveInfo, include_free: bool) -> i32 {
    unsafe {
        if index >= 26 {
            return -1;
        }
        let letter = b'A' + index as u8;
        out.write(c::DriveInfo {
            letter,
            ..Default::default()
        });
        let drive = drive::r4os_rust_fs_drive_lookup(1, usize::from(letter));
        if drive.is_null() {
            return 0;
        }
        (*out).mounted = 1;
        (*out).kind = (*drive).kind;
        (*out).role = (*drive).role;
        (*out).bytes = (*drive).bytes as u64;
        path::fixed_z(
            &mut (*out).name,
            core::slice::from_raw_parts((*drive).name.data, (*drive).name.len),
        );
        if let Some(volume) = fs::volume((*drive).letter) {
            let mut request = some!(fs::Request::volume(1, (*drive).letter, volume), -2);
            let cluster = fs::scalar(volume, 0);
            let total = fs::scalar(volume, 1);
            (*out).cluster_bytes = u32::try_from(cluster).expect("Cluster bytes");
            (*out).total_clusters = u32::try_from(total).expect("Cluster count");
            (*out).bytes = total * cluster;
            if include_free {
                let free = fs::run(
                    fs::op::FREE,
                    fs::Args {
                        volume,
                        read_guard: ptr::addr_of_mut!(request.guard),
                        ..Default::default()
                    },
                    ptr::null_mut(),
                );
                if free.present {
                    (*out).free_clusters = u32::try_from(free.value).expect("Free clusters");
                    (*out).free_bytes = free.value * cluster;
                    (*out).reserved |= c::drive_info_flag_free_valid;
                }
            }
            request.ok = true;
        }
        1
    }
}
/// # Safety
/// Original path and exclusive output buffer cover the exact directory request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_dir_list(input: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe {
        if len == 0 {
            return -1;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -2);
        let volume = some!(ctx::volume(target), -3);
        let mut request = some!(
            fs::Request::volume(10, (*target.drive_ref).letter, volume),
            -6
        );
        let mut node = 0;
        if fs::parent(volume, target.path.slice(), &mut node) != 0 {
            return -4;
        }
        let result = fs::run(
            fs::op::READ_DIR,
            fs::Args {
                volume,
                parent: node,
                buffer: out,
                buffer_len: len as usize,
                ..Default::default()
            },
            ptr::null_mut(),
        );
        if !result.present {
            return -5;
        }
        request.ok = true;
        i32::try_from(result.value).expect("Directory list result")
    }
}
/// # Safety
/// Original readable path and initialized/exclusive public change cursor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_directory_change_begin(
    input: *const u8,
    cursor: *mut c::DirectoryChangeCursor,
) -> i32 {
    unsafe {
        if (*cursor).version != 1
            || (*cursor).size != size_of::<c::DirectoryChangeCursor>() as u32
            || (*cursor).reserved != 0
        {
            return -1;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -3);
        let mut request = some!(
            fs::Request::volume(11, (*target.drive_ref).letter, volume),
            -9
        );
        let mut node = 0;
        match fs::parent(volume, target.path.slice(), &mut node) {
            0 => {}
            1 => return -3,
            2 => return -9,
            _ => unreachable!(),
        }
        if !volume.has_ref {
            return -3;
        }
        changes::r4os_rust_fs_directory_changes(1, &volume.reference, node, cursor);
        request.ok = true;
        0
    }
}
/// # Safety
/// Original exclusive initialized public change cursor storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_directory_change_poll(
    cursor: *mut c::DirectoryChangeCursor,
) -> i32 {
    unsafe { changes::r4os_rust_fs_directory_changes(2, ptr::null(), 0, cursor) }
}
/// # Safety
/// Original path and exclusive bounded output remain live through its lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_dir_entry(
    input: *const u8,
    index: u32,
    out: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        if len == 0 {
            return -1;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -2);
        let path = target.path.slice();
        let letter = (*target.drive_ref).letter;
        let output = core::slice::from_raw_parts_mut(out, len as usize);
        if index == 0 {
            return if path::drive_z(letter, path, output) {
                1
            } else {
                -7
            };
        }
        if index == 1 {
            let mut buffer = [0u8; 1024];
            let parent = some!(path::parent_buffer(path, &mut buffer), -7);
            return if path::drive_z(letter, parent, output) {
                1
            } else {
                -7
            };
        }
        let volume = some!(ctx::volume(target), -3);
        let mut request = some!(fs::Request::volume(11, letter, volume), -8);
        let mut node = 0;
        match fs::parent(volume, path, &mut node) {
            0 => {}
            1 => return -4,
            2 => return -9,
            _ => unreachable!(),
        }
        let mut name = [0u8; 769];
        let mut entry = fs::Entry::EMPTY;
        match fs::run(
            fs::op::DIR_ENTRY,
            fs::Args {
                volume,
                parent: node,
                index: index as usize,
                buffer: name.as_mut_ptr(),
                buffer_len: name.len(),
                ..Default::default()
            },
            &mut entry,
        )
        .status
        {
            0 => {}
            1 => return -5,
            2 => return -9,
            _ => unreachable!(),
        }
        let mut joined = [0u8; 1024];
        let path = some!(path::join(path, &name[..entry.name_len], &mut joined), -7);
        if !path::drive_z(letter, path, output) {
            return -7;
        }
        request.ok = true;
        i32::from(entry.is_dir())
    }
}
/// # Safety
/// Original path and disjoint caller cursor/path/info outputs. Exact logical
/// owner, mount generation and backend state remain bound to the same scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_directory_next(
    input: *const u8,
    cursor: *mut c::DirectoryScanCursor,
    out: *mut u8,
    len: u32,
    info: *mut c::FileInfo,
) -> i32 {
    unsafe {
        if (*cursor).version != 1
            || (*cursor).size != size_of::<c::DirectoryScanCursor>() as u32
            || len == 0
        {
            return -1;
        }
        let owner = some!(ctx::owner(), -1);
        let mut next = cursor.read();
        let initial = next.change.mount_generation == 0;
        if !initial
            && (next.owner_id != owner.id
                || next.owner_generation != owner.generation
                || next.owner_kind != u32::from(owner.kind))
        {
            return -10;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), if initial { -3 } else { -10 });
        let letter = (*target.drive_ref).letter;
        let mut request = some!(fs::Request::volume(11, letter, volume), -9);
        if !volume.has_ref {
            return -3;
        }
        let mut node = 0;
        match fs::parent(volume, target.path.slice(), &mut node) {
            0 => {}
            1 => return if initial { -3 } else { -10 },
            2 => return -9,
            _ => unreachable!(),
        }
        if initial {
            next = c::DirectoryScanCursor {
                owner_id: owner.id,
                owner_kind: u32::from(owner.kind),
                owner_generation: owner.generation,
                ..Default::default()
            };
            changes::r4os_rust_fs_directory_changes(1, &volume.reference, node, &mut next.change);
        } else {
            if next.change.node != node
                || next.change.mount_slot != volume.reference.slot
                || next.change.mount_generation != volume.reference.generation
            {
                return -10;
            }
            if changes::r4os_rust_fs_directory_changes(2, ptr::null(), 0, &mut next.change) != 0 {
                return -10;
            }
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::run(
            fs::op::DIR_NEXT,
            fs::Args {
                volume,
                parent: node,
                cursor: next.backend.as_mut_ptr(),
                ..Default::default()
            },
            &mut entry,
        )
        .status
        {
            0 => {}
            3 => return -9,
            2 => {
                cursor.write(next);
                request.ok = true;
                return 2;
            }
            1 => {
                cursor.write(next);
                request.ok = true;
                return -5;
            }
            _ => unreachable!(),
        }
        let mut joined = [0u8; 1024];
        let path = some!(
            path::join(
                target.path.slice(),
                &entry.name[..entry.name_len],
                &mut joined
            ),
            -7
        );
        let mut full = [0u8; 1024];
        if !path::drive_z(letter, path, &mut full) {
            return -7;
        }
        let count = some!(full.iter().position(|&b| b == 0), -7);
        if count + 1 > len as usize {
            return -7;
        }
        let metadata = metadata(&entry, letter, true);
        ptr::copy_nonoverlapping(full.as_ptr(), out, count + 1);
        info.write(metadata);
        cursor.write(next);
        request.ok = true;
        i32::from(entry.is_dir())
    }
}
/// # Safety
/// Original path and exclusive public output live through metadata lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_info(input: *const u8, out: *mut c::FileInfo) -> i32 {
    unsafe {
        out.write(Default::default());
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let path = target.path.slice();
        let letter = (*target.drive_ref).letter;
        (*out).drive = letter;
        path::fixed_z(&mut (*out).name, path::base(path));
        let volume = some!(ctx::volume(target), -2);
        let mut request = some!(fs::Request::volume(12, letter, volume), -3);
        if path::root(path) {
            (*out).exists = 1;
            (*out).is_dir = 1;
            (*out).attr = 0x10;
            (*out).first_cluster =
                u32::try_from(fs::scalar(volume, 2)).expect("Root node projection");
            path::fixed_z(&mut (*out).name, b"\\");
            request.ok = true;
            return 1;
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, path, &mut entry) {
            0 => {}
            1 => {
                io::record_failure(12, 0, io::lookup_stage(volume));
                return 0;
            }
            2 => {
                io::record_failure(12, -4, 0);
                return -4;
            }
            _ => unreachable!(),
        }
        out.write(metadata(&entry, letter, false));
        request.ok = true;
        1
    }
}

fn scan_poll(
    change: &mut c::DirectoryChangeCursor,
    parent: &mut c::DirectoryChangeCursor,
) -> Result<bool, i32> {
    let before = change.sequence;
    if changes::poll_scan(change, parent) != 0 {
        return Err(-10);
    }
    Ok(change.sequence != before)
}
unsafe fn scan_node(volume: fs::Volume, relative: &[u8]) -> Result<(u64, u32, u64), i32> {
    unsafe {
        if path::root(relative) {
            let node = fs::scalar(volume, 2);
            return Ok((node, 0, node));
        }
        let mut parent = 0;
        match fs::parent(volume, path::parent(relative), &mut parent) {
            0 => {}
            1 => return Err(-3),
            _ => return Err(-9),
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::lookup(volume, parent, path::base(relative), false, &mut entry) {
            // Preserve resolve_path refusal of NTFS reparse traversal.
            0 if entry.is_dir() && entry.reparse => Err(-9),
            0 if entry.is_dir() => Ok((entry.node, u32::from(entry.generation), parent)),
            0 | 1 => Err(-3),
            _ => Err(-9),
        }
    }
}
/// # Safety
/// Exclusive aligned cursor and disjoint initialized output records for the
/// supplied capacity. The initial path is NUL terminated. Continuation uses
/// only captured inline values. No mounted request, backend pointer, heap
/// allocation or kernel scan reference survives a return or caller unwind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_directory_batch(
    input: *const u8,
    cursor: *mut c::DirectoryBatchCursor,
    out: *mut c::DirectoryBatchEntry,
    capacity: u32,
) -> i32 {
    unsafe {
        if cursor.is_null()
            || capacity > c::directory_batch_max_entries
            || capacity != 0 && out.is_null()
            || (*cursor).version != 1
            || (*cursor).size != size_of::<c::DirectoryBatchCursor>() as u32
        {
            return -1;
        }
        let owner = some!(ctx::owner(), -1);
        // The hot buffered-entry check copies only the two40-byte watches,
        // not the2160-byte path/backend cursor. A changed global sequence
        // falls through with the original caller state for path validation.
        if capacity == 0 && (*cursor).change.mount_generation != 0 {
            if (*cursor).owner_id != owner.id
                || (*cursor).owner_generation != owner.generation
                || (*cursor).owner_kind != u32::from(owner.kind)
            {
                return -10;
            }
            let mut change = (*cursor).change;
            let mut parent = (*cursor).parent_change;
            match scan_poll(&mut change, &mut parent) {
                Err(code) => return code,
                Ok(false) => {
                    (*cursor).change = change;
                    (*cursor).parent_change = parent;
                    return 0;
                }
                Ok(true) => {}
            }
        }
        let mut next = cursor.read();
        let initial = next.change.mount_generation == 0;
        let mut namespace_changed = false;
        if initial {
            if capacity == 0 || input.is_null() {
                return -1;
            }
        } else {
            if next.owner_id != owner.id
                || next.owner_generation != owner.generation
                || next.owner_kind != u32::from(owner.kind)
            {
                return -10;
            }
            let length = next.path_len as usize;
            if !(3..next.path.len()).contains(&length)
                || next.path[length] != 0
                || !next.path[0].is_ascii_alphabetic()
                || next.path[1] != b':'
                || !path::separator(next.path[2])
            {
                return -1;
            }
            namespace_changed = match scan_poll(&mut next.change, &mut next.parent_change) {
                Ok(changed) => changed,
                Err(code) => return code,
            };
            // Buffered SDK records are revalidated before each publication.
            // This common unchanged-namespace case is entirely RAM-only.
            if capacity == 0 && !namespace_changed {
                cursor.write(next);
                return 0;
            }
        }
        let mut buffers = PathBuffers::new();
        let volume = if initial {
            let target = some!(buffers.resolve(input), -1);
            let volume = some!(ctx::volume(target), -3);
            if !volume.has_ref {
                return -3;
            }
            next = c::DirectoryBatchCursor {
                owner_id: owner.id,
                owner_kind: u32::from(owner.kind),
                owner_generation: owner.generation,
                ..Default::default()
            };
            if !path::drive_z(
                (*target.drive_ref).letter,
                target.path.slice(),
                &mut next.path,
            ) {
                return -7;
            }
            next.path_len = some!(next.path.iter().position(|&b| b == 0), -7) as u32;
            volume
        } else {
            some!(
                crate::fs::vfs::mounted_volume_by_reference(
                    crate::storage::access_state::Reference {
                        slot: next.change.mount_slot,
                        generation: next.change.mount_generation
                    }
                ),
                -10
            )
        };
        let letter = next.path[0];
        let mut request = some!(fs::Request::volume(11, letter, volume), -9);
        let relative = &next.path[2..next.path_len as usize];
        if initial {
            let (node, generation, parent) = match scan_node(volume, relative) {
                Ok(bound) => bound,
                Err(code) => return code,
            };
            next.node_generation = generation;
            (next.change, next.parent_change) = changes::begin_scan(volume.reference, node, parent);
        } else {
            namespace_changed |= match scan_poll(&mut next.change, &mut next.parent_change) {
                Ok(changed) => changed,
                Err(code) => return code,
            };
            if namespace_changed {
                // Only an intervening namespace publication needs path
                // resolution again; ancestor moves are included. This is not
                // performed for every entry of an unchanged scan.
                let (node, generation, parent) = match scan_node(volume, relative) {
                    Ok(bound) => bound,
                    Err(-3) => return -10,
                    Err(code) => return code,
                };
                if node != next.change.node
                    || generation != next.node_generation
                    || parent != next.parent_change.node
                {
                    return -10;
                }
            }
        }
        if capacity == 0 {
            if scan_poll(&mut next.change, &mut next.parent_change).is_err() {
                return -10;
            }
            cursor.write(next);
            request.ok = true;
            return 0;
        }
        let mut entries = [fs::Entry::EMPTY; c::directory_batch_max_entries as usize];
        let (count, status) = crate::fs::vfs_backends::directory_batch(
            volume,
            next.change.node,
            next.backend.as_mut_ptr(),
            &mut entries[..capacity as usize],
        );
        if status == 3 {
            return -9;
        }
        if scan_poll(&mut next.change, &mut next.parent_change).is_err() {
            return -10;
        }
        // Validate every full path before committing the opaque continuation.
        // A failed output has no authority and cannot silently consume data.
        for (index, entry) in entries[..count].iter().enumerate() {
            let mut joined = [0u8; 1024];
            let path = some!(
                path::join(relative, &entry.name[..entry.name_len], &mut joined),
                -7
            );
            let mut record = c::DirectoryBatchEntry {
                info: metadata(entry, letter, true),
                ..Default::default()
            };
            if !path::drive_z(letter, path, &mut record.path) {
                return -7;
            }
            out.add(index).write(record);
        }
        cursor.write(next);
        request.ok = true;
        if count != 0 {
            count as i32
        } else if status == 1 {
            -5
        } else {
            0
        }
    }
}
