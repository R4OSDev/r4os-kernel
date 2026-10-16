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
            let free = fs::run(
                fs::op::FREE,
                fs::Args {
                    volume,
                    ..Default::default()
                },
                ptr::null_mut(),
            );
            if free.present {
                (*out).free_clusters = u32::try_from(free.value).expect("Free clusters");
                (*out).free_bytes = free.value * cluster;
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
