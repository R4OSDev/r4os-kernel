//! Original staged copy callbacks and public copy/move. Pair admission keeps
//! mutation/unwind ownership while reader admission releases only backend gates.
use super::super::registry_format_types::Bytes;
use super::{
    context::{self as ctx, PathBuffers},
    fs, path, registry_state, stream,
};
use crate::{
    contract as c,
    fs::{
        copy_transfer::{self, Io, Progress},
        request,
    },
    memory::heap,
    sched::scheduler,
};
use core::{
    ffi::c_void,
    mem::size_of,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
static SEQUENCE: AtomicU32 = AtomicU32::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
pub(crate) fn install_program_cancellation() {
    CANCEL.store(true, Ordering::Release)
}
struct CopyIo {
    lease: *mut request::Guard,
    source: fs::Volume,
    target: fs::Volume,
    source_letter: u8,
    target_letter: u8,
    entry: fs::Entry,
    parent: u64,
    target_name: Bytes,
    stage: [u8; 13],
    stage_len: usize,
    stage_owned: bool,
    readers_allowed: bool,
}
unsafe fn stage<'a>(io: *const CopyIo) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(ptr::addr_of!((*io).stage).cast(), (*io).stage_len) }
}
unsafe extern "C" fn cancelled(_: *mut c_void) -> bool {
    CANCEL.load(Ordering::Acquire)
        && unsafe { super::super::process::r4os_rust_program_process_legacy(3, 0) } != 0
}
unsafe extern "C" fn prepare(context: *mut c_void) -> bool {
    unsafe {
        let io = context.cast::<CopyIo>();
        for _ in 0..128 {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) & 0xffffff;
            let mut name = *b"~C000000.TMP\0";
            for index in 0..6 {
                name[2 + index] =
                    b"0123456789ABCDEF"[((sequence >> (4 * (5 - index))) & 15) as usize];
            }
            (*io).stage = name;
            let name = &name[..12];
            if name.eq_ignore_ascii_case((*io).target_name.slice()) {
                continue;
            }
            let mut entry = fs::Entry::EMPTY;
            match fs::lookup((*io).target, (*io).parent, name, false, &mut entry) {
                0 => continue,
                2 => return false,
                1 => {}
                _ => unreachable!(),
            }
            (*io).stage_len = name.len();
            (*io).stage_owned = true;
            return fs::write((*io).target, (*io).parent, stage(io), &[], false);
        }
        false
    }
}
unsafe extern "C" fn allow_readers(context: *mut c_void) {
    unsafe {
        let io = context.cast::<CopyIo>();
        (*io).readers_allowed = request::r4os_rust_fs_request_readers((*io).lease);
    }
}
unsafe extern "C" fn read(
    context: *mut c_void,
    offset: u32,
    out: *mut u8,
    len: usize,
    count: *mut usize,
) -> bool {
    unsafe {
        let io = context.cast::<CopyIo>();
        let Some(mut request) = fs::Request::volume(3, (*io).source_letter, (*io).source) else {
            return false;
        };
        let Some(got) = fs::read(
            (*io).source,
            &(*io).entry,
            u64::from(offset),
            out,
            len,
            true,
        ) else {
            return false;
        };
        request.ok = got == len;
        count.write(got);
        true
    }
}
unsafe extern "C" fn append(
    context: *mut c_void,
    offset: u64,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        let io = context.cast::<CopyIo>();
        let Some(mut request) = fs::Request::volume(5, (*io).target_letter, (*io).target) else {
            return false;
        };
        request.ok = fs::run(
            fs::op::APPEND_DEFERRED,
            fs::Args {
                volume: (*io).target,
                parent: (*io).parent,
                name: fs::bytes(stage(io)),
                offset,
                data: fs::Bytes { data, len },
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .status
            == 0;
        request.ok
    }
}
unsafe extern "C" fn progress(context: *mut c_void) {
    unsafe { request::r4os_rust_fs_request_copy_progress((*context.cast::<CopyIo>()).lease) }
}
unsafe extern "C" fn pause(context: *mut c_void) {
    if unsafe { (*context.cast::<CopyIo>()).readers_allowed } {
        scheduler::yield_task();
    }
}
unsafe extern "C" fn flush(context: *mut c_void) -> bool {
    unsafe {
        let io = context.cast::<CopyIo>();
        let Some(mut request) = fs::Request::volume(8, (*io).target_letter, (*io).target) else {
            return false;
        };
        request.ok = fs::flush((*io).target);
        request.ok
    }
}
unsafe extern "C" fn publish(context: *mut c_void) -> bool {
    unsafe {
        let io = context.cast::<CopyIo>();
        let Some(mut request) = fs::Request::volume(17, (*io).target_letter, (*io).target) else {
            return false;
        };
        let mut old = fs::Entry::EMPTY;
        match fs::lookup(
            (*io).target,
            (*io).parent,
            (*io).target_name.slice(),
            false,
            &mut old,
        ) {
            0 => {
                if old.is_dir()
                    || old.read_only()
                    || !fs::delete((*io).target, (*io).parent, (*io).target_name.slice())
                {
                    return false;
                }
            }
            1 => {}
            2 => return false,
            _ => unreachable!(),
        }
        request.ok = fs::run(
            fs::op::PUBLISH_COPY,
            fs::Args {
                volume: (*io).target,
                parent: (*io).parent,
                name: fs::bytes(stage(io)),
                second: fs::bytes((*io).target_name.slice()),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present;
        if request.ok {
            (*io).stage_owned = false;
        }
        request.ok
    }
}
unsafe extern "C" fn abort(context: *mut c_void) {
    unsafe {
        let io = context.cast::<CopyIo>();
        if !(*io).stage_owned {
            return;
        }
        let Some(mut request) = fs::Request::volume(13, (*io).target_letter, (*io).target) else {
            return;
        };
        request.ok = fs::delete((*io).target, (*io).parent, stage(io));
        if request.ok {
            (*io).stage_owned = false;
        }
    }
}
struct Allocation(*mut u8);
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { heap::r4os_rust_heap_free(self.0, 32768) };
        }
    }
}
unsafe fn transfer(
    io: *mut CopyIo,
    supplied: Option<(*mut u8, usize)>,
    output: *mut Progress,
) -> bool {
    unsafe {
        let owned = Allocation(if supplied.is_none() {
            heap::r4os_rust_heap_alloc(32768, 1)
        } else {
            ptr::null_mut()
        });
        if supplied.is_none() && owned.0.is_null() {
            return false;
        }
        let (buffer, len) = supplied.unwrap_or((owned.0, 32768));
        let mut local = Progress {
            bytes: 0,
            source_size: 0,
            chunks: 0,
            max_chunk: 0,
        };
        let output = if output.is_null() { &mut local } else { output };
        let callbacks = Io {
            context: io.cast(),
            cancelled,
            prepare,
            allow_readers,
            read,
            append,
            progress,
            pause,
            flush,
            publish,
            abort,
        };
        copy_transfer::r4os_rust_fs_copy_transfer(&callbacks, buffer, len, (*io).entry.size, output)
    }
}
unsafe fn same_volume(first: fs::Volume, second: fs::Volume) -> bool {
    unsafe {
        crate::fs::vfs::r4os_rust_vfs_identity(
            0,
            &fs::Args {
                volume: first,
                other: second,
                ..Default::default()
            },
        )
    }
}
unsafe fn perform(
    source: *const u8,
    target: *const u8,
    supplied: Option<(*mut u8, usize)>,
    progress: *mut Progress,
    moving: bool,
) -> i32 {
    unsafe {
        let mut source_buffers = PathBuffers::new();
        let mut target_buffers = PathBuffers::new();
        if !source_buffers.copy(source) || !target_buffers.copy(target) {
            return -1;
        }
        let source = some!(source_buffers.resolve_copied(), -1);
        let target = some!(target_buffers.resolve_copied(), -1);
        let sv = some!(ctx::volume(source), -2);
        let tv = some!(ctx::volume(target), -2);
        let sp = source.path.slice();
        let tp = target.path.slice();
        if path::root(sp) || path::root(tp) {
            return -3;
        }
        let sl = (*source.drive_ref).letter;
        let tl = (*target.drive_ref).letter;
        if sl == tl && sp == tp {
            return -8;
        }
        let mut request = some!(
            fs::Request::begin(if moving { 18 } else { 17 }, sl, tl, 1, false, &[sv, tv]),
            if moving { -12 } else { -10 }
        );
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(sv, sp, &mut entry) {
            0 => {}
            1 => return 0,
            2 => return -9,
            _ => unreachable!(),
        }
        if !progress.is_null() {
            (*progress).source_size = entry.size;
        }
        if entry.is_dir() {
            return -4;
        }
        if !moving && entry.size > u32::MAX as u64 {
            return -13;
        }
        if moving && entry.read_only() {
            return -11;
        }
        let mut target_parent = 0;
        let mut source_parent = 0;
        if fs::parent(tv, path::parent(tp), &mut target_parent) != 0 {
            return -5;
        }
        if moving && fs::parent(sv, path::parent(sp), &mut source_parent) != 0 {
            return -6;
        }
        let mut existing = None;
        if fs::optional(tv, tp, &mut existing) == 2 {
            return -9;
        }
        if let Some(destination) = existing.as_ref()
            && same_volume(sv, tv)
            && fs::identity(sv, &entry, destination, 2)
        {
            return -8;
        }
        if moving
            && !stream::invalidate(
                sl,
                ctx::boot_target(source),
                source_parent,
                Some(&entry),
                path::base(sp),
            )
        {
            return -9;
        }
        if !stream::invalidate(
            tl,
            ctx::boot_target(target),
            target_parent,
            existing.as_ref(),
            path::base(tp),
        ) {
            return -9;
        }
        let mut renamed = false;
        if moving && existing.is_none() && same_volume(sv, tv) && source_parent == target_parent {
            match fs::run(
                fs::op::RENAME,
                fs::Args {
                    volume: sv,
                    parent: source_parent,
                    name: fs::bytes(path::base(sp)),
                    second: fs::bytes(path::base(tp)),
                    ..Default::default()
                },
                ptr::null_mut(),
            )
            .status
            {
                0 => renamed = true,
                1 | 3 => return 0,
                2 => {}
                4 => return -9,
                _ => unreachable!(),
            }
        }
        if !renamed {
            if existing
                .as_ref()
                .is_some_and(|entry| entry.is_dir() || entry.read_only())
            {
                return -9;
            }
            let mut io = CopyIo {
                lease: &mut request.guard,
                source: sv,
                target: tv,
                source_letter: sl,
                target_letter: tl,
                entry,
                parent: target_parent,
                target_name: Bytes::from_slice(path::base(tp)),
                stage: [0; 13],
                stage_len: 0,
                stage_owned: false,
                readers_allowed: false,
            };
            if !transfer(&mut io, supplied, progress) {
                return -9;
            }
            if moving {
                let mut remove = some!(fs::Request::volume(13, sl, sv), -10);
                remove.ok = fs::delete(sv, source_parent, path::base(sp));
                if !remove.ok {
                    return -10;
                }
            }
        }
        if moving {
            registry_state::r4os_rust_sys_registry_invalidate(Bytes::from_slice(
                source_buffers.raw(),
            ));
        }
        registry_state::r4os_rust_sys_registry_invalidate(Bytes::from_slice(target_buffers.raw()));
        request.ok = true;
        1
    }
}
/// # Safety
/// Original two immutable paths and exact pair-request lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_copy(source: *const u8, target: *const u8) -> i32 {
    unsafe { perform(source, target, None, ptr::null_mut(), false) }
}
/// # Safety
/// Original paths plus exclusive caller scratch/progress last through copy,
/// its exact staged ownership and all synchronous waiting callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_copy_buffered(
    source: *const u8,
    target: *const u8,
    buffer: *mut u8,
    len: u32,
    out: *mut c::FileCopyProgress,
) -> i32 {
    unsafe {
        if (*out).version != 1 || (*out).size < size_of::<c::FileCopyProgress>() as u32 {
            return c::file_stream_error_invalid;
        }
        out.write(Default::default());
        if len == 0 {
            return c::file_stream_error_invalid;
        }
        let mut progress = Progress {
            bytes: 0,
            source_size: 0,
            chunks: 0,
            max_chunk: 0,
        };
        let result = perform(
            source,
            target,
            Some((buffer, len as usize)),
            &mut progress,
            false,
        );
        (*out).bytes = progress.bytes;
        (*out).source_size = progress.source_size;
        (*out).chunks = progress.chunks;
        (*out).max_chunk = progress.max_chunk;
        match result {
            1 => c::file_stream_result_ok,
            0 | -5 => c::file_stream_error_not_found,
            -1 | -3 | -4 | -8 => c::file_stream_error_invalid,
            -13 => c::file_stream_error_too_large,
            _ => c::file_stream_error_io,
        }
    }
}
/// # Safety
/// Original paths and exact pair-request lifetime; ambiguous rename never
/// crosses into the copy/delete fallback, and source deletion has its own ack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_move(source: *const u8, target: *const u8) -> i32 {
    unsafe { perform(source, target, None, ptr::null_mut(), true) }
}
