//! Original basic public file operations and coherent failure diagnostics.
//! The already native filesystem request owner retains each real I/O lifetime.
use super::super::registry_format_types::Bytes;
use super::{
    context::{self as ctx, PathBuffers},
    fs, path, registry_state, stream,
};
use crate::{
    arch::runtime::RuntimeGuard,
    sched::{scheduler, task},
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Failure {
    pub sequence: u32,
    pub task_id: u32,
    pub kind: u32,
    pub result: i32,
    pub lookup_stage: u32,
}
struct Diagnostic(UnsafeCell<Failure>);
// SAFETY: Every read/write is a short Runtime-owned resident snapshot.
unsafe impl Sync for Diagnostic {}
static FAILURE: Diagnostic = Diagnostic(UnsafeCell::new(Failure {
    sequence: 0,
    task_id: 0,
    kind: 12,
    result: 0,
    lookup_stage: 0,
}));
pub(super) unsafe fn record_failure(kind: u32, result: i32, lookup_stage: u32) {
    unsafe {
        let _runtime = RuntimeGuard::acquire();
        let mut sequence = (*FAILURE.0.get()).sequence.wrapping_add(1);
        if sequence == 0 {
            sequence = 1;
        }
        let current = scheduler::current();
        let task_id = if current.is_null() {
            0
        } else {
            task::immutable_identity(current).0
        };
        FAILURE.0.get().write(Failure {
            sequence,
            task_id,
            kind,
            result,
            lookup_stage,
        });
    }
}
/// # Safety
/// Exclusive fixed-layout snapshot output. The caller copy follows release of
/// Runtime so a pageable output never faults while this diagnostic owns it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_failure(out: *mut Failure) {
    let snapshot = {
        let _runtime = RuntimeGuard::acquire();
        unsafe { FAILURE.0.get().read() }
    };
    unsafe { out.write(snapshot) };
}
pub(super) unsafe fn lookup_stage(volume: fs::Volume) -> u32 {
    unsafe {
        fs::run(
            fs::op::DIAGNOSTIC,
            fs::Args {
                volume,
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .value as u32
    }
}
/// # Safety
/// Original path and exclusive caller read buffer remain live through request
/// admission and backend completion; output lengths retain checked i32 casts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_read(input: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let mut request = some!(
            fs::Request::volume(2, (*target.drive_ref).letter, volume),
            -7
        );
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, target.path.slice(), &mut entry) {
            0 => {}
            1 => return -3,
            2 => return -6,
            _ => unreachable!(),
        }
        if entry.is_dir() {
            return -4;
        }
        if entry.size > u64::from(len) {
            return -5;
        }
        let count = some!(fs::read(volume, &entry, 0, out, len as usize, false), -6);
        request.ok = true;
        i32::try_from(count).expect("File read result")
    }
}
/// # Safety
/// Original path and immutable caller bytes remain valid through exact request
/// completion; namespace invalidation precedes the single backend write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_write(
    input: *const u8,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let value = target.path.slice();
        let mut request = some!(
            fs::Request::volume(4, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(value), &mut parent) != 0 {
            return -3;
        }
        let name = path::base(value);
        let mut existing = None;
        if fs::optional(volume, value, &mut existing) == 2 {
            return -4;
        }
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            existing.as_ref(),
            name,
        ) {
            return -4;
        }
        if !fs::write(
            volume,
            parent,
            name,
            Bytes {
                data,
                len: len as usize,
            }
            .slice(),
            false,
        ) {
            return -4;
        }
        registry_state::r4os_rust_sys_registry_invalidate(Bytes::from_slice(buffers.raw()));
        request.ok = true;
        i32::try_from(len).expect("File write result")
    }
}
/// # Safety
/// Original path and caller buffer span cover the admitted range read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_read_at(
    input: *const u8,
    offset: u64,
    out: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        if offset > usize::MAX as u64 {
            return -8;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let mut request = some!(
            fs::Request::volume(3, (*target.drive_ref).letter, volume),
            -7
        );
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, target.path.slice(), &mut entry) {
            0 => {}
            1 => {
                record_failure(3, -3, lookup_stage(volume));
                return -3;
            }
            2 => {
                record_failure(3, -6, 0);
                return -6;
            }
            _ => unreachable!(),
        }
        if entry.is_dir() {
            return -4;
        }
        let count = match fs::read(volume, &entry, offset, out, len as usize, true) {
            Some(count) => count,
            None => {
                record_failure(3, -6, 0);
                return -6;
            }
        };
        request.ok = true;
        i32::try_from(count).expect("File range read result")
    }
}
/// # Safety
/// Original path/immutable data lifetime includes exact request/backend write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_write_at(
    input: *const u8,
    offset: u64,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        if offset > usize::MAX as u64 {
            return -8;
        }
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let value = target.path.slice();
        let mut request = some!(
            fs::Request::volume(22, (*target.drive_ref).letter, volume),
            -7
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(value), &mut parent) != 0 {
            return -3;
        }
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, value, &mut entry) {
            0 => {}
            1 => return -3,
            2 => return -6,
            _ => unreachable!(),
        }
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            Some(&entry),
            path::base(value),
        ) {
            return -6;
        }
        if entry.is_dir() {
            return -4;
        }
        let result = fs::run(
            fs::op::WRITE_RANGE,
            fs::Args {
                volume,
                entry: &entry,
                offset,
                data: fs::Bytes {
                    data,
                    len: len as usize,
                },
                ..Default::default()
            },
            ptr::null_mut(),
        );
        fs::notify(volume, parent);
        if !result.present {
            return -6;
        }
        request.ok = true;
        i32::try_from(result.value).expect("File range write result")
    }
}
/// # Safety
/// Original immutable path/data storage and exact filesystem request lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_append(
    input: *const u8,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let volume = some!(ctx::volume(target), -2);
        let value = target.path.slice();
        let mut request = some!(
            fs::Request::volume(5, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        if fs::parent(volume, path::parent(value), &mut parent) != 0 {
            return -3;
        }
        let name = path::base(value);
        let mut existing = None;
        if fs::optional(volume, value, &mut existing) == 2
            || !stream::invalidate(
                (*target.drive_ref).letter,
                ctx::boot_target(target),
                parent,
                existing.as_ref(),
                name,
            )
        {
            return -4;
        }
        if !fs::write(
            volume,
            parent,
            name,
            Bytes {
                data,
                len: len as usize,
            }
            .slice(),
            true,
        ) {
            return -4;
        }
        request.ok = true;
        i32::try_from(len).expect("File append result")
    }
}
unsafe extern "C" {
    fn r4os_zig_sys_rewrite(
        address: usize,
        input: Bytes,
        output: *mut u8,
        capacity: usize,
        context: *const c_void,
        result: *mut usize,
    ) -> bool;
}
/// # Safety
/// Caller retains disjoint input/output and original rewrite/context handles.
/// Rewrite must neither enter R4SYS nor block on another filesystem request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_rewrite(
    raw: Bytes,
    input: *mut u8,
    input_capacity: usize,
    output: *mut u8,
    output_capacity: usize,
    context: *const c_void,
    rewrite: usize,
) -> i32 {
    unsafe {
        let mut resolved = path::Buffer::<1024>::new();
        let target = some!(ctx::resolve(raw.slice(), &mut resolved), -1);
        let volume = some!(ctx::volume(target), -2);
        let path = target.path.slice();
        if path::root(path) {
            return -3;
        }
        let mut request = some!(
            fs::Request::volume(21, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        match fs::parent(volume, path::parent(path), &mut parent) {
            0 => {}
            1 => return -3,
            2 => return -6,
            _ => unreachable!(),
        }
        let mut existing = None;
        if fs::optional(volume, path, &mut existing) == 2 {
            return -6;
        }
        let mut input_len = 0;
        if let Some(entry) = existing.as_ref() {
            if entry.is_dir() {
                return -4;
            }
            if entry.size > input_capacity as u64 {
                return -5;
            }
            input_len = some!(fs::read(volume, entry, 0, input, input_capacity, false), -6);
        }
        let mut output_len = 0;
        if !r4os_zig_sys_rewrite(
            rewrite,
            Bytes {
                data: input,
                len: input_len,
            },
            output,
            output_capacity,
            context,
            &mut output_len,
        ) {
            return -5;
        }
        if output_len > output_capacity || output_len > i32::MAX as usize {
            return -5;
        }
        let name = path::base(path);
        if !stream::invalidate(
            (*target.drive_ref).letter,
            ctx::boot_target(target),
            parent,
            existing.as_ref(),
            name,
        ) {
            return -6;
        }
        if !fs::write(
            volume,
            parent,
            name,
            Bytes {
                data: output,
                len: output_len,
            }
            .slice(),
            false,
        ) {
            return -6;
        }
        request.ok = true;
        output_len as i32
    }
}
/// # Safety
/// Original immutable NUL-terminated module path lasts through provider query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_module_running(input: *const u8) -> i32 {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = some!(buffers.resolve(input), -1);
        let path = target.path.slice();
        if path::root(path) {
            return -1;
        }
        ctx::module_running((*target.drive_ref).letter, path)
    }
}
