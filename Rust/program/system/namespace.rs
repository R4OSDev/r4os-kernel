//! Original idempotent namespace mutations, backend-exact collation and
//! fingerprint/identity-bound deletion. Live publish tuples remain barriers.
use super::super::registry_format_types::Bytes;
use super::{
    context::{self as ctx, PathBuffers},
    fs, io, path, registry_state, stream,
};
use crate::sched::{
    sync_locks,
    sync_types::{Mutex, Text},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
unsafe fn invalidate_cache(raw: &[u8]) {
    unsafe { registry_state::r4os_rust_sys_registry_invalidate(Bytes::from_slice(raw)) }
}
/// # Safety
/// Original path and exact request lifetime; absence reconciles a lost backend
/// completion, while I/O ambiguity never authorizes a path-only retry/delete.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_delete(input: *const u8) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let path = target.path.slice();
        if path::root(path) {
            return -3;
        }
        let mut request = some!(
            fs::Request::volume(13, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        match fs::parent(volume, path::parent(path), &mut parent) {
            0 => {}
            1 => {
                io::record_failure(13, -4, io::lookup_stage(volume));
                return -4;
            }
            2 => {
                io::record_failure(13, -4, 0);
                return -4;
            }
            _ => unreachable!(),
        }
        let mut existing = None;
        match fs::optional(volume, path, &mut existing) {
            0 => {}
            1 => {
                invalidate_cache(buffers.raw());
                request.ok = true;
                return 0;
            }
            2 => return -6,
            _ => unreachable!(),
        }
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            existing.as_ref(),
            path::base(path),
        ) {
            return -6;
        }
        if !fs::delete(volume, parent, path::base(path)) {
            let mut after = fs::Entry::EMPTY;
            match fs::resolve(volume, path, &mut after) {
                1 => {
                    invalidate_cache(buffers.raw());
                    request.ok = true;
                    return 1;
                }
                0 | 2 => return -6,
                _ => unreachable!(),
            }
        }
        invalidate_cache(buffers.raw());
        request.ok = true;
        1
    }
}
/// # Safety
/// Original immutable path and caller/request lifetime through directory create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_dir_create(input: *const u8) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let path = target.path.slice();
        if path::root(path) {
            return -3;
        }
        let mut request = some!(
            fs::Request::volume(14, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(path), &mut parent) != 0 {
            return -4;
        }
        let mut existing = None;
        match fs::optional(volume, path, &mut existing) {
            0 => {
                if !existing
                    .as_ref()
                    .expect("Found directory create entry")
                    .is_dir()
                {
                    return -6;
                }
                request.ok = true;
                return 0;
            }
            1 => {}
            2 => return -6,
            _ => unreachable!(),
        }
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            existing.as_ref(),
            path::base(path),
        ) {
            return -6;
        }
        let created = fs::run(
            fs::op::MKDIR,
            fs::Args {
                volume,
                parent,
                name: fs::bytes(path::base(path)),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present;
        if !created {
            let mut after = fs::Entry::EMPTY;
            if fs::resolve(volume, path, &mut after) != 0 || !after.is_dir() {
                return -6;
            }
        }
        request.ok = true;
        1
    }
}
/// # Safety
/// Original path/request lifetime. Captured physical stream ancestors protect
/// directories even when a caller reaches them through another spelling/alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_dir_delete(input: *const u8) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let path = target.path.slice();
        if path::root(path) {
            return -3;
        }
        let letter = (*target.drive_ref).letter;
        let mut request = some!(fs::Request::volume(15, letter, volume), -5);
        let mut parent = 0;
        if fs::parent(volume, path::parent(path), &mut parent) != 0 {
            return -4;
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, path, &mut entry) {
            0 => {
                if !entry.is_dir() {
                    return -4;
                }
            }
            1 => {
                request.ok = true;
                return 0;
            }
            2 => return -6,
            _ => unreachable!(),
        }
        if stream::subtree(letter, ctx::boot_target(target), path, &entry) {
            return -5;
        }
        let removed = fs::run(
            fs::op::RMDIR,
            fs::Args {
                volume,
                parent,
                name: fs::bytes(path::base(path)),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present;
        if !removed {
            let mut after = fs::Entry::EMPTY;
            if fs::resolve(volume, path, &mut after) != 1 {
                return -6;
            }
        }
        request.ok = true;
        1
    }
}
/// # Safety
/// Original two path lifetimes and exact namespace request. Copy/delete is
/// entered only after an explicit backend not-atomic result, never on I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_rename(old: *const u8, new: *const u8) -> i32 {
    unsafe {
        let mut old_buffers = PathBuffers::new();
        let mut new_buffers = PathBuffers::new();
        if !old_buffers.copy(old) || !new_buffers.copy(new) {
            return -1;
        }
        let old = some!(old_buffers.resolve_copied(), -1);
        let new = some!(new_buffers.resolve_copied(), -1);
        let volume = some!(ctx::volume(old), -2);
        if ctx::volume(new).is_none() {
            return -2;
        }
        if (*old.drive_ref).letter != (*new.drive_ref).letter {
            return -3;
        }
        let old_path = old.path.slice();
        let new_path = new.path.slice();
        if path::root(old_path) || path::root(new_path) {
            return -4;
        }
        let parent_path = path::parent(old_path);
        if parent_path != path::parent(new_path) {
            return -5;
        }
        let letter = (*old.drive_ref).letter;
        let mut request = some!(fs::Request::volume(16, letter, volume), -7);
        let mut parent = 0;
        match fs::parent(volume, parent_path, &mut parent) {
            0 => {}
            1 => return -6,
            2 => return -8,
            _ => unreachable!(),
        }
        let mut old_entry = None;
        let mut new_entry = None;
        let old_status = fs::optional(volume, old_path, &mut old_entry);
        let new_status = fs::optional(volume, new_path, &mut new_entry);
        if old_status == 2 || new_status == 2 {
            return -8;
        }
        if old_status == 1 {
            request.ok = true;
            return 0;
        }
        if let Some(entry) = old_entry.as_ref()
            && entry.is_dir()
            && stream::subtree(letter, ctx::boot_target(old), old_path, entry)
        {
            return -7;
        }
        if let Some(entry) = new_entry.as_ref()
            && entry.is_dir()
            && stream::subtree(
                (*new.drive_ref).letter,
                ctx::boot_target(new),
                new_path,
                entry,
            )
        {
            return -7;
        }
        if !stream::invalidate(
            letter,
            ctx::boot_target(old),
            parent,
            old_entry.as_ref(),
            path::base(old_path),
        ) || !stream::invalidate(
            (*new.drive_ref).letter,
            ctx::boot_target(new),
            parent,
            new_entry.as_ref(),
            path::base(new_path),
        ) {
            return -8;
        }
        let renamed = fs::run(
            fs::op::RENAME,
            fs::Args {
                volume,
                parent,
                name: fs::bytes(path::base(old_path)),
                second: fs::bytes(path::base(new_path)),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .status;
        match renamed {
            0 => {
                invalidate_cache(old_buffers.raw());
                invalidate_cache(new_buffers.raw());
                request.ok = true;
                return 1;
            }
            1 | 3 => {
                request.ok = true;
                return 0;
            }
            4 => return -8,
            2 => {}
            _ => unreachable!(),
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, old_path, &mut entry) {
            0 => {}
            1 => {
                request.ok = true;
                return 0;
            }
            2 => return -8,
            _ => unreachable!(),
        }
        if entry.is_dir() {
            request.ok = true;
            return 0;
        }
        if !fs::run(
            fs::op::COPY_NO_REPLACE,
            fs::Args {
                volume,
                other: volume,
                entry: &entry,
                parent,
                name: fs::bytes(path::base(new_path)),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present
        {
            request.ok = true;
            return 0;
        }
        if !fs::delete(volume, parent, path::base(old_path)) {
            return -8;
        }
        invalidate_cache(old_buffers.raw());
        invalidate_cache(new_buffers.raw());
        request.ok = true;
        1
    }
}
/// # Safety
/// Original two readable NUL paths. Collation belongs to exactly one volume;
/// no guessed Unicode folding or cross-volume identity is synthesized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_names_equal(left: *const u8, right: *const u8) -> i32 {
    unsafe {
        let mut left_buffers = PathBuffers::new();
        let mut right_buffers = PathBuffers::new();
        if !left_buffers.copy(left) || !right_buffers.copy(right) {
            return -1;
        }
        let left = some!(left_buffers.resolve_copied(), -3);
        let right = some!(right_buffers.resolve_copied(), -3);
        let l = left.path.slice();
        let r = right.path.slice();
        if path::root(l) || path::root(r) {
            return -3;
        }
        let lv = some!(ctx::volume(left), -2);
        let rv = some!(ctx::volume(right), -2);
        if !crate::fs::vfs::r4os_rust_vfs_identity(
            0,
            &fs::Args {
                volume: lv,
                other: rv,
                ..Default::default()
            },
        ) {
            return -4;
        }
        if path::parent(l) != path::parent(r) {
            return 0;
        }
        let result = fs::run(
            fs::op::NAMES,
            fs::Args {
                volume: lv,
                name: fs::bytes(path::base(l)),
                second: fs::bytes(path::base(r)),
                ..Default::default()
            },
            ptr::null_mut(),
        );
        if !result.present {
            return -1;
        }
        i32::from(result.value != 0)
    }
}
struct Checksum(UnsafeCell<[MaybeUninit<u8>; 32768]>);
// SAFETY: The dedicated sleepable scratch mutex serializes this one resident
// buffer over backend waits, following request admission and before release.
unsafe impl Sync for Checksum {}
static CHECKSUM: Checksum = Checksum(UnsafeCell::new([MaybeUninit::uninit(); 32768]));
struct ScratchLock(UnsafeCell<Mutex>);
// SAFETY: Native sync owns lock fields under Runtime; resident stable address.
unsafe impl Sync for ScratchLock {}
static CHECKSUM_LOCK: ScratchLock = ScratchLock(UnsafeCell::new(Mutex {
    rank: 116,
    name: Text::literal(b"r4sys-delete-checksum"),
    ..Mutex::EMPTY
}));
struct Scratch;
impl Scratch {
    unsafe fn acquire() -> Option<Self> {
        if unsafe { sync_locks::r4os_rust_sync_mutex(CHECKSUM_LOCK.0.get(), 1, u64::MAX) } {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe { sync_locks::r4os_rust_sync_mutex(CHECKSUM_LOCK.0.get(), 2, 0) };
    }
}
/// # Safety
/// Original readable path and exact request lifetime. The complete checksum
/// and backend identity delete share one admitted namespace interval.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_delete_match(
    input: *const u8,
    expected: u64,
    checksum: u32,
) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let path = target.path.slice();
        if path::root(path) {
            return -1;
        }
        let mut request = some!(
            fs::Request::volume(24, (*target.drive_ref).letter, volume),
            -4
        );
        let mut parent = 0;
        match fs::parent(volume, path::parent(path), &mut parent) {
            0 => {}
            1 => {
                request.ok = fs::flush(volume);
                return if request.ok { 0 } else { -4 };
            }
            2 => return -4,
            _ => unreachable!(),
        }
        let name = path::base(path);
        let mut entry = fs::Entry::EMPTY;
        match fs::lookup(volume, parent, name, false, &mut entry) {
            0 => {}
            1 => {
                request.ok = fs::flush(volume);
                return if request.ok { 0 } else { -4 };
            }
            2 => return -4,
            _ => unreachable!(),
        }
        if entry.is_dir() || entry.size != expected {
            return -3;
        }
        if entry.size > usize::MAX as u64 {
            return -4;
        }
        let _scratch = some!(Scratch::acquire(), -4);
        let data = CHECKSUM.0.get().cast::<u8>();
        let mut hash = 2166136261u32;
        let mut offset = 0;
        while offset < entry.size {
            let want = (entry.size - offset).min(32768);
            let got = some!(
                fs::read(volume, &entry, offset, data, want as usize, true),
                -4
            );
            if got != want as usize {
                return -4;
            }
            for &byte in core::slice::from_raw_parts(data, got) {
                hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
            }
            offset += want;
        }
        if hash != checksum {
            return -3;
        }
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            Some(&entry),
            name,
        ) {
            return -4;
        }
        let result = fs::run(
            fs::op::DELETE_MATCH,
            fs::Args {
                volume,
                parent,
                name: fs::bytes(name),
                entry: &entry,
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .status;
        request.ok = result != 3;
        match result {
            0 => 1,
            1 => 0,
            2 => -3,
            3 => -4,
            _ => unreachable!(),
        }
    }
}
