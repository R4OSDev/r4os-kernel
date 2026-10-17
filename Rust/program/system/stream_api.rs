//! Original public stream begin/write/finish/abort and storage lease admission.
//! Every slot lookup occurs after the exact volume gate; no stale pointer is
//! selected before a wait, and ambiguous handovers retain their exact token.
use super::{context as ctx, fs, path, stream, stream_publish as publish, stream_state as s};
use crate::{
    arch::runtime::RuntimeGuard,
    contract as c,
    sched::{scheduler, task_context},
};
use core::ptr;
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
const OPEN: u32 =
    c::file_stream_open_create | c::file_stream_open_truncate | c::file_stream_open_lease;

/// # Safety
/// Original NUL-terminated path and admitted caller lifetime through the
/// exact filesystem request. Only its exact slot may own a storage lease.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_begin(input: *const u8, flags: u32) -> i32 {
    unsafe {
        if flags & !OPEN != 0 {
            return c::file_stream_error_unsupported;
        }
        if flags & OPEN == 0 {
            return c::file_stream_error_invalid;
        }
        let lease = flags & c::file_stream_open_lease != 0;
        if lease && flags != (c::file_stream_open_create | c::file_stream_open_lease) {
            return c::file_stream_error_invalid;
        }
        let mut raw_buffer = path::Buffer::<1024>::new();
        let raw = some!(
            path::copy_z(input, &mut raw_buffer),
            c::file_stream_error_invalid
        );
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(
            ctx::resolve(raw, &mut resolved),
            c::file_stream_error_invalid
        );
        let volume = some!(ctx::volume(target), c::file_stream_error_io);
        let target_path = target.path.slice();
        if path::root(target_path) {
            return c::file_stream_error_io;
        }
        let letter = (*target.drive_ref).letter;
        let mut request = some!(
            fs::Request::claims(6, letter, volume),
            c::file_stream_error_io
        );
        stream::reap(letter);
        let mut parent = 0;
        if fs::parent(volume, path::parent(target_path), &mut parent) != 0 {
            return c::file_stream_error_io;
        }
        let mut existing = None;
        if fs::optional(volume, target_path, &mut existing) == 2 {
            return c::file_stream_error_io;
        }
        let name = path::base(target_path);
        let boot = ctx::boot_target(target);
        if stream::collision(letter, boot, parent, existing.as_ref(), name, ptr::null())
            || stream::find(letter, boot, parent, existing.as_ref(), name, false).is_some()
        {
            return c::file_stream_error_exists;
        }
        if let Some(entry) = existing.as_ref() {
            if entry.is_dir() {
                return c::file_stream_error_invalid;
            }
            if !lease && flags & c::file_stream_open_truncate == 0 {
                return c::file_stream_error_exists;
            }
        } else if flags & c::file_stream_open_create == 0 {
            return c::file_stream_error_not_found;
        }
        let owner = ctx::owner();
        let slot = some!(s::reserve(letter, owner), c::file_stream_error_io);
        s::remember(slot, target_path, letter, boot, parent, name, flags, owner);
        if volume.has_ref {
            match s::begin_use(volume.reference) {
                Ok(token) => (*slot).volume_use = Some(token),
                Err(_) => {
                    s::clear(slot);
                    return c::file_stream_error_io;
                }
            }
        } else if !scheduler::current().is_null() {
            s::clear(slot);
            return c::file_stream_error_io;
        }
        if !s::bind_ancestors(slot, volume, path::parent(target_path), parent) {
            s::clear(slot);
            return c::file_stream_error_io;
        }
        if lease && existing.is_some() {
            if !fs::delete(volume, parent, name) {
                s::clear(slot);
                return c::file_stream_error_io;
            }
            if fs::optional(volume, target_path, &mut existing) != 1 {
                s::clear(slot);
                return c::file_stream_error_io;
            }
        }
        if !fs::write(volume, parent, name, &[], false) {
            if existing.is_none() {
                let mut ambiguous = None;
                match fs::optional(volume, target_path, &mut ambiguous) {
                    0 => s::bind(slot, ambiguous.as_ref().expect("Found created stream")),
                    1 | 2 => s::clear(slot),
                    _ => unreachable!(),
                }
            } else {
                s::clear(slot);
            }
            return c::file_stream_error_io;
        }
        let mut created = fs::Entry::EMPTY;
        if fs::resolve(volume, target_path, &mut created) != 0 {
            s::clear(slot);
            return c::file_stream_error_io;
        }
        s::bind(slot, &created);
        (*slot).ready = true;
        request.ok = true;
        c::file_stream_result_ok
    }
}
struct ErrorLog {
    volume: fs::Volume,
    letter: u8,
    phase: u32,
    offset: u64,
    len: u32,
    ok: bool,
}
impl Drop for ErrorLog {
    fn drop(&mut self) {
        if !self.ok {
            unsafe {
                sys_stream_error(
                    false,
                    self.letter,
                    self.phase,
                    self.offset,
                    self.len,
                    &self.volume,
                )
            };
        }
    }
}
/// # Safety
/// Original immutable path/data span and caller lifetime. The admitted exact
/// lane retains the selected slot through real append and identity refresh.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_write(
    input: *const u8,
    offset: u64,
    data: *const u8,
    len: u32,
    flags: u32,
) -> i32 {
    unsafe {
        if flags != 0 {
            return c::file_stream_error_unsupported;
        }
        if !super::io::transfer_len_fits(u64::from(len))
            || offset > u32::MAX as u64
            || u64::from(len) > u32::MAX as u64 - offset
        {
            return c::file_stream_error_too_large;
        }
        let mut raw_buffer = path::Buffer::<1024>::new();
        let raw = some!(
            path::copy_z(input, &mut raw_buffer),
            c::file_stream_error_invalid
        );
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(
            ctx::resolve(raw, &mut resolved),
            c::file_stream_error_invalid
        );
        let volume = some!(ctx::volume(target), c::file_stream_error_io);
        let target_path = target.path.slice();
        let letter = (*target.drive_ref).letter;
        // Empty FAT streams initially lack a node identity; their admitted
        // write can refresh the C: claim. NTFS chunks retain their original
        // single-volume parallelism because their live identity is stable.
        let admission = if volume.kind == 0 {
            fs::Request::claims(7, letter, volume)
        } else {
            fs::Request::volume(7, letter, volume)
        };
        let mut request = match admission {
            Some(value) => value,
            None => {
                sys_stream_error(true, letter, 0, offset, len, &volume);
                return c::file_stream_error_io;
            }
        };
        let mut log = ErrorLog {
            volume,
            letter,
            phase: 1,
            offset,
            len,
            ok: false,
        };
        let mut parent = 0;
        if fs::parent(volume, path::parent(target_path), &mut parent) != 0 {
            return c::file_stream_error_io;
        }
        log.phase = 2;
        let mut entry = None;
        if fs::optional(volume, target_path, &mut entry) == 2 {
            return c::file_stream_error_io;
        }
        let entry = some!(entry, c::file_stream_error_not_found);
        log.phase = 3;
        if let Some(slot) = stream::find(
            letter,
            ctx::boot_target(target),
            parent,
            Some(&entry),
            path::base(target_path),
            true,
        ) {
            log.phase = 4;
            if !(*slot).ready || (*slot).finished {
                return c::file_stream_error_io;
            }
            s::bind(slot, &entry);
            if (*slot).size != offset {
                return c::file_stream_error_offset_mismatch;
            }
            if len == 0 {
                request.ok = true;
                log.ok = true;
                return 0;
            }
            log.phase = 5;
            let slot_volume = some!(s::volume(slot), c::file_stream_error_io);
            log.phase = 6;
            let result = fs::run(
                fs::op::APPEND_DEFERRED,
                fs::Args {
                    volume: slot_volume,
                    parent: (*slot).parent_node,
                    name: fs::bytes(s::name(slot)),
                    offset,
                    data: fs::Bytes {
                        data,
                        len: len as usize,
                    },
                    ..Default::default()
                },
                ptr::null_mut(),
            );
            match result.status {
                0 => {}
                1 => return c::file_stream_error_invalid,
                2 => return c::file_stream_error_not_found,
                3 => return c::file_stream_error_offset_mismatch,
                4 => return c::file_stream_error_too_large,
                5 => return c::file_stream_error_io,
                _ => unreachable!(),
            }
            let refresh = (*slot).file_node == 0 && (*slot).file_node_generation == 0;
            (*slot).size += u64::from(len);
            (*slot).identity_refresh_pending = refresh;
            log.phase = 70;
            let mut updated = fs::Entry::EMPTY;
            let lookup_status = fs::lookup(
                slot_volume,
                (*slot).parent_node,
                s::name(slot),
                false,
                &mut updated,
            );
            log.phase = 70 + u32::try_from(lookup_status).expect("VFS lookup status");
            match lookup_status {
                0 => {
                    log.phase = 8;
                    if updated.is_dir() || updated.size != (*slot).size {
                        return c::file_stream_error_io;
                    }
                    s::bind(slot, &updated);
                    if refresh {
                        publish::refresh(slot);
                    }
                }
                1 => return c::file_stream_error_not_found,
                2 => return c::file_stream_error_io,
                _ => unreachable!(),
            }
            request.ok = true;
            log.ok = true;
            return i32::try_from(len).expect("Stream write result length");
        }
        if entry.is_dir() {
            return c::file_stream_error_invalid;
        }
        if entry.size != offset {
            return c::file_stream_error_offset_mismatch;
        }
        c::file_stream_error_not_found
    }
}
/// # Safety
/// Original path/caller lifetime; the exact admitted slot persists until
/// backend flush acknowledgement or explicit retained-finished ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_finish(
    input: *const u8,
    size: u64,
    flags: u32,
) -> i32 {
    unsafe {
        if flags & !1 != 0 {
            return c::file_stream_error_unsupported;
        }
        let mut raw_buffer = path::Buffer::<1024>::new();
        let raw = some!(
            path::copy_z(input, &mut raw_buffer),
            c::file_stream_error_invalid
        );
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(
            ctx::resolve(raw, &mut resolved),
            c::file_stream_error_invalid
        );
        let volume = some!(ctx::volume(target), c::file_stream_error_io);
        let target_path = target.path.slice();
        let letter = (*target.drive_ref).letter;
        let mut request = some!(
            fs::Request::volume(8, letter, volume),
            c::file_stream_error_io
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(target_path), &mut parent) != 0 {
            return c::file_stream_error_io;
        }
        let mut entry = None;
        if fs::optional(volume, target_path, &mut entry) == 2 {
            return c::file_stream_error_io;
        }
        let entry = some!(entry, c::file_stream_error_not_found);
        if let Some(slot) = stream::find(
            letter,
            ctx::boot_target(target),
            parent,
            Some(&entry),
            path::base(target_path),
            true,
        ) {
            if !(*slot).ready || (*slot).finished {
                return c::file_stream_error_io;
            }
            s::bind(slot, &entry);
            if entry.is_dir() {
                return c::file_stream_error_invalid;
            }
            if (*slot).size != size || entry.size != size {
                return c::file_stream_error_size_mismatch;
            }
            let slot_volume = some!(s::volume(slot), c::file_stream_error_io);
            if !fs::flush(slot_volume) {
                return c::file_stream_error_io;
            }
            if flags & 1 != 0 {
                (*slot).finished = true;
            } else {
                s::clear(slot);
            }
            fs::notify(volume, parent);
            request.ok = true;
            return c::file_stream_result_ok;
        }
        if entry.is_dir() {
            return c::file_stream_error_invalid;
        }
        if entry.size != size {
            return c::file_stream_error_size_mismatch;
        }
        c::file_stream_error_not_found
    }
}
/// # Safety
/// Original path/caller lifetime and exact volume request. No current path
/// occupant is removed without the original slot's identity/terminal proof.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_abort(input: *const u8) -> i32 {
    unsafe {
        let mut raw_buffer = path::Buffer::<1024>::new();
        let raw = some!(
            path::copy_z(input, &mut raw_buffer),
            c::file_stream_error_invalid
        );
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(
            ctx::resolve(raw, &mut resolved),
            c::file_stream_error_invalid
        );
        let volume = some!(ctx::volume(target), c::file_stream_error_io);
        let target_path = target.path.slice();
        let letter = (*target.drive_ref).letter;
        if path::root(target_path) {
            return c::file_stream_error_io;
        }
        let mut request = some!(
            fs::Request::claims(9, letter, volume),
            c::file_stream_error_io
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(target_path), &mut parent) != 0 {
            return c::file_stream_error_io;
        }
        let mut entry = None;
        let status = fs::optional(volume, target_path, &mut entry);
        let slot = some!(
            stream::find(
                letter,
                ctx::boot_target(target),
                parent,
                if status == 0 { entry.as_ref() } else { None },
                path::base(target_path),
                true
            ),
            c::file_stream_error_not_found
        );
        if status == 0 {
            s::bind(slot, entry.as_ref().expect("Found stream abort entry"));
        }
        if (*slot).publish_started {
            if (*slot).publish_target_name_len == 0 || (*slot).publish_backup_name_len == 0 {
                return c::file_stream_error_io;
            }
            let result = publish::run(
                volume,
                (*slot).parent_node,
                publish::target(slot),
                s::name(slot),
                publish::backup(slot),
                slot,
            );
            if matches!(result, 0 | -2) {
                if (*slot).publish_claim_generation != 0
                    && !publish::retire((*slot).publish_claim_generation)
                {
                    return c::file_stream_error_io;
                }
                s::clear(slot);
                request.ok = true;
                return c::file_stream_result_ok;
            }
            return c::file_stream_error_io;
        }
        if status == 2 {
            return c::file_stream_error_io;
        }
        let slot_volume = some!(s::volume(slot), c::file_stream_error_io);
        if status == 1 {
            if !fs::flush(slot_volume) {
                return c::file_stream_error_io;
            }
            s::clear(slot);
            request.ok = true;
            return c::file_stream_result_ok;
        }
        fs::delete(slot_volume, (*slot).parent_node, s::name(slot));
        let mut after = None;
        let mut status = fs::optional(slot_volume, target_path, &mut after);
        if status == 0
            && s::owns_entry(
                slot,
                after.as_ref().expect("Found stream abort retry entry"),
            )
        {
            fs::delete(slot_volume, (*slot).parent_node, s::name(slot));
            status = fs::optional(slot_volume, target_path, &mut after);
        }
        if status == 2 || status == 0 || !fs::flush(slot_volume) {
            return c::file_stream_error_io;
        }
        if (*slot).publish_claim_generation != 0
            && !publish::retire((*slot).publish_claim_generation)
        {
            return c::file_stream_error_io;
        }
        s::clear(slot);
        request.ok = true;
        c::file_stream_result_ok
    }
}
struct Unwind(task_context::Unwind);
impl Drop for Unwind {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        unsafe { task_context::leave(self.0) };
    }
}
/// # Safety
/// Original readable path and exclusive ID output. Exact task unwind admission
/// precedes resolver waits, and its token is released after the lease result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_storage_use(input: *const u8, out: *mut u64) -> i32 {
    unsafe {
        out.write(0);
        let token = {
            let _runtime = RuntimeGuard::acquire();
            task_context::enter()
        };
        if !token.admitted() {
            return c::storage_error_capacity;
        }
        let _unwind = Unwind(token);
        let mut buffer = path::Buffer::<1024>::new();
        let raw = some!(path::copy_z(input, &mut buffer), c::storage_error_invalid);
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(ctx::resolve(raw, &mut resolved), c::storage_error_invalid);
        let volume = some!(ctx::volume(target), c::storage_error_not_found);
        if !volume.has_ref {
            return c::storage_error_stale;
        }
        match s::begin_use(volume.reference) {
            Ok(token) => {
                out.write(token.id);
                c::storage_result_ok
            }
            Err(error) => match error {
                3 => c::storage_error_busy,
                2 => c::storage_error_stale,
                5 => c::storage_error_capacity,
                _ => c::storage_error_invalid,
            },
        }
    }
}

unsafe fn sys_stream_error(
    admission: bool,
    letter: u8,
    phase: u32,
    offset: u64,
    len: u32,
    volume: *const fs::Volume,
) {
    use crate::boot::log_route as log;
    if admission {
        log::puts(b"[FSSTREAM] write admission failed\r\n");
        return;
    }
    let volume = unsafe { volume.read() };
    log::puts(b"[FSSTREAM] write failed drive=");
    log::putc(letter);
    log::puts(b" phase=");
    log::dec(u64::from(phase));
    log::puts(b" offset=");
    log::dec(offset);
    log::puts(b" bytes=");
    log::dec(u64::from(len));
    log::puts(b" lookup=");
    let a = crate::fs::vfs_types::Args {
        volume,
        ..Default::default()
    };
    let v = unsafe { crate::fs::vfs::r4os_rust_vfs_run(4, &a, core::ptr::null_mut()) };
    log::dec(v.value);
    if volume.kind == 1 {
        log::puts(b" append=");
        log::dec(u64::from(crate::fs::ntfs_volume_names::append_stage()));
    }
    log::puts(b"\r\n");
    let mut cache = core::mem::MaybeUninit::uninit();
    unsafe {
        crate::fs::page_cache::r4os_rust_pagecache_summary(cache.as_mut_ptr());
    }
    let cache = unsafe { cache.assume_init() };
    log::puts(b"[FSSTREAM] cache read/write/writeback/alloc/lock failures=");
    let values = [
        cache.read_errors,
        cache.write_errors,
        cache.writeback_errors,
        cache.payload_allocation_failures,
        cache.lock_timeouts,
    ];
    for (i, v) in values.into_iter().enumerate() {
        if i != 0 {
            log::puts(b"/");
        }
        log::dec(v);
    }
    log::puts(b"\r\n");
}
