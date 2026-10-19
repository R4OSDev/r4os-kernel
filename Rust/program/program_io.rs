//! Program output/capture and byte/codepoint facade counters. The retired
//! memory/allocator slots keep their existing zero/null/error behavior.
use super::{
    console, console_input, context, payload, registry_types::ProgramInstance, threads, util,
};
use crate::{arch::percpu, contract, fs::vfs_types::Bytes, sched::scheduler};
use core::{cell::UnsafeCell, ffi::c_void, mem, ptr};
struct Capture {
    owner: *mut c_void,
    data: *mut u8,
    capacity: usize,
    len: usize,
    present: bool,
    truncated: bool,
}
struct Cell(UnsafeCell<Capture>);
// SAFETY: original BSP capture transaction is bound to the exact current task.
// Other CPUs never read its contents; no second buffer/reference owner exists.
unsafe impl Sync for Cell {}
static CAPTURE: Cell = Cell(UnsafeCell::new(Capture {
    owner: ptr::null_mut(),
    data: ptr::null_mut(),
    capacity: 0,
    len: 0,
    present: false,
    truncated: false,
}));
#[repr(C)]
pub struct CaptureResult {
    pub len: usize,
    pub truncated: bool,
}
unsafe fn capture_byte(ch: u8) {
    unsafe {
        let capture = CAPTURE.0.get();
        if (*capture).present {
            if (*capture).len < (*capture).capacity {
                (*capture).data.add((*capture).len).write(ch);
                (*capture).len += 1;
            } else {
                (*capture).truncated = true;
            }
        }
    }
}
unsafe fn smoke(instance: *const ProgramInstance, stream: u32, ch: u8) {
    unsafe {
        if stream == 0 {
            return;
        }
        let host = if (*instance).console_payload.is_null() {
            0
        } else {
            (*payload::console(instance)).host
        };
        if (*instance).role != 1 || host != 0 {
            return;
        }
        let args = ptr::addr_of!((*payload::process(instance)).args).cast::<u8>();
        if !util::r4os_rust_program_util_compare(
            3,
            Bytes {
                data: args,
                len: 128,
            },
            util::span(b"SMOKE"),
        ) {
            return;
        }
        console::sink(0, u32::from(ch), 0);
    }
}
pub(super) unsafe fn output(stream: u32, data: Bytes) {
    unsafe {
        if data.len == 0 || threads::compute_worker() {
            return;
        }
        let data_bytes = util::bytes(data);
        if percpu::current_index() == 0 {
            let capture = CAPTURE.0.get();
            if (*capture).present && scheduler::current() == (*capture).owner {
                for &ch in data_bytes {
                    if ch == b'\n' {
                        capture_byte(b'\r');
                        capture_byte(b'\n');
                    } else if ch != b'\r' {
                        capture_byte(ch);
                    }
                }
                return;
            }
        }
        if console::r4os_rust_program_console_route(stream, data, true) {
            return;
        }
        let instance = context::current();
        if !instance.is_null() {
            for &ch in data_bytes {
                if ch == b'\n' {
                    smoke(instance, stream, b'\r');
                    smoke(instance, stream, b'\n');
                } else if ch != b'\r' {
                    smoke(instance, stream, ch);
                }
            }
        }
    }
}
/// # Safety
/// Begin receives the original exclusive capture buffer for its exact BSP
/// task transaction. End consumes its result and relinquishes that buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_capture(
    begin: bool,
    data: *mut u8,
    len: usize,
    out: *mut CaptureResult,
) {
    unsafe {
        let capture = CAPTURE.0.get();
        if begin {
            capture.write(Capture {
                owner: scheduler::current(),
                data,
                capacity: len,
                len: 0,
                present: true,
                truncated: false,
            });
        } else {
            out.write(CaptureResult {
                len: (*capture).len,
                truncated: (*capture).truncated,
            });
            (*capture).data = ptr::null_mut();
            (*capture).capacity = 0;
            (*capture).present = false;
            (*capture).owner = ptr::null_mut();
            (*capture).len = 0;
            (*capture).truncated = false;
        }
    }
}
/// # Safety
/// Original bounded input span remains resident through output routing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_output(stream: u32, data: Bytes) {
    unsafe { output(stream, data) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_io_byte(stream: u32, ch: u8, capture: bool) {
    unsafe {
        if capture {
            capture_byte(ch);
        } else {
            output(
                stream,
                Bytes {
                    data: ptr::addr_of!(ch),
                    len: 1,
                },
            );
        }
    }
}
/// # Safety
/// Retained exact instance and original current console output admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_smoke(
    instance: *const ProgramInstance,
    stream: u32,
    ch: u8,
) {
    unsafe { smoke(instance, stream, ch) }
}
/// # Safety
/// Same resident bounded NUL input as the original Print slot; at most4096
/// bytes are inspected and output normalization remains with this owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_print(text: *const u8) {
    unsafe {
        let mut len = 0;
        while len < 4096 && text.add(len).read() != 0 {
            len += 1;
        }
        output(1, Bytes { data: text, len });
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_io_key(byte: bool) -> u32 {
    if threads::compute_worker() {
        return 0;
    }
    unsafe {
        let state = console_input::r4os_rust_program_console_input_state();
        (*state).read_calls = (*state).read_calls.wrapping_add(1);
        let mut value = 0;
        if !console_input::r4os_rust_program_console_input_read(&mut value, byte) {
            (*state).read_empty = (*state).read_empty.wrapping_add(1);
            return 0;
        }
        (*state).read_bytes = (*state).read_bytes.wrapping_add(1);
        value
    }
}
/// # Safety
/// Original caller byte span retains its lifetime. Legacy mode limits output
/// to4096; the R4SYS slot preserves its full input and checked signed return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_write(
    stream: u32,
    data: *const u8,
    len: u32,
    legacy: bool,
) -> i32 {
    unsafe {
        if threads::compute_worker() {
            return -2;
        }
        if legacy {
            if stream > 2 {
                return -1;
            }
            if stream == 0 {
                return -2;
            }
            let count = (len as usize).min(4096);
            output(stream, Bytes { data, len: count });
            count as i32
        } else {
            output(
                1,
                Bytes {
                    data,
                    len: len as usize,
                },
            );
            i32::try_from(len).expect("R4SYS write signed result")
        }
    }
}
/// # Safety
/// Retired alloc/realloc outputs are exclusive u64; the retired statistics
/// output retains the exact original canonical104-byte all-zero record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_io_retired(op: u8, out: *mut c_void) -> i32 {
    unsafe {
        match op {
            0 => out.cast::<u64>().write(0),
            1 => {}
            2 => out
                .cast::<contract::ProgramMemoryStats>()
                .write(contract::ProgramMemoryStats::default()),
            _ => unreachable!(),
        }
        -8
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_io_allocator(_op: u8) -> usize {
    0
}
const _: () = {
    assert!(mem::size_of::<CaptureResult>() == 16);
    assert!(mem::align_of::<CaptureResult>() == 8);
    assert!(mem::offset_of!(CaptureResult, len) == 0);
    assert!(mem::offset_of!(CaptureResult, truncated) == 8);
    assert!(mem::size_of::<contract::ProgramMemoryStats>() == 104);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, base) == 0);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, reserved_bytes) == 8);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, committed_bytes) == 16);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, used_bytes) == 24);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, free_bytes) == 32);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, largest_free) == 40);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, active_blocks) == 48);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, free_blocks) == 52);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, reserved0) == 56);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, error_flags) == 60);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, allocation_errors) == 64);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, invalid_free_errors) == 72);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, size_mismatch_errors) == 80);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, oom_errors) == 88);
    assert!(mem::offset_of!(contract::ProgramMemoryStats, owner_mismatch_errors) == 96);
};
