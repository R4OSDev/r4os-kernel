//! Original real DriverWork IRQ/YMM callbacks and physical preallocated reserve
//! reuse under one-shot forced metadata admission failure; no simulated pool.
use super::{fpu, helpers::*, state::*, workers};
use crate::{
    arch::fpu as owner,
    sched::task::{
        task_resources as resources,
        task_types::{Bytes, CreateFailure, Role, State},
    },
};
use core::ptr;
unsafe extern "C" {
    fn r4os_zig_task_probe_work_submit(
        lane: usize,
        handler: unsafe extern "C" fn(usize) -> i32,
        deadline: u64,
        budget: u32,
        key: u64,
        deadline_lane: bool,
        out: *mut u32,
    ) -> i32;
    fn r4os_zig_task_probe_work(operation: u8, handle: u32, timeout: u64, out: *mut i32) -> i32;
    fn r4os_rust_task_identity(
        op: u8,
        id: u32,
        generation: u64,
        ctx: *mut core::ffi::c_void,
        value: u64,
    ) -> u64;
}
pub unsafe fn kill_id(id: u32) -> bool {
    unsafe { r4os_rust_task_identity(2, id, 0, ptr::null_mut(), 0) != 0 }
}
pub unsafe fn work_fpu() -> bool {
    unsafe {
        let mut status = core::mem::MaybeUninit::uninit();
        owner::r4os_rust_fpu_status(status.as_mut_ptr());
        if !status.assume_init().avx_enabled {
            return fail(b"fpu-work-avx-unavailable");
        }
        let mut passed = true;
        for lane in 0..2 {
            let mut handle = 0;
            let submitted = r4os_zig_task_probe_work_submit(
                lane,
                fpu::callback,
                now().saturating_add(1000),
                2,
                0x465055,
                lane != 0,
                &mut handle,
            );
            if submitted != 0 {
                return fail(b"fpu-work-submit");
            }
            let mut result = -1;
            if r4os_zig_task_probe_work(0, handle, 2000, &mut result) != 0 {
                r4os_zig_task_probe_work(1, handle, 0, ptr::null_mut());
                r4os_zig_task_probe_work(2, handle, 0, ptr::null_mut());
                return fail(b"fpu-work-wait");
            }
            if r4os_zig_task_probe_work(2, handle, 0, ptr::null_mut()) != 0 {
                return fail(b"fpu-work-release");
            }
            puts(b"TASKREG08201 fpu-work lane=");
            puts(if lane == 0 { b"normal" } else { b"deadline" });
            puts(b" ymm-preserved=");
            puts(if result == 0 { b"yes\r\n" } else { b"NO\r\n" });
            passed &= result == 0;
        }
        puts(if passed {
            b"TASKREG08201 fpu-work PASS lanes=2 irq-boundary=actual\r\n"
        } else {
            b"TASKREG08201 fpu-work FAIL lanes=2 irq-boundary=actual\r\n"
        });
        if passed && fpu::callback(2) != 0 {
            return fail(b"fpu-kernel-main");
        }
        passed
    }
}
fn hex(value: u64, digits: usize) {
    let mut text = [b'0'; 16];
    for (i, byte) in text[..digits].iter_mut().enumerate() {
        let n = ((value >> ((digits - 1 - i) * 4)) & 15) as u8;
        *byte = if n < 10 { b'0' + n } else { b'A' + n - 10 };
    }
    puts(&text[..digits]);
}
unsafe extern "C" fn critical_fpu() {
    unsafe {
        let (initial, mxcsr) = fpu::initial();
        if initial != [0; 4] || mxcsr != 0x1f80 {
            puts(b"TASKREG08201 fpu-reserve inherited-state ymm0=");
            for word in initial {
                hex(word, 16);
                puts(b" ");
            }
            puts(b"mxcsr=");
            hex(u64::from(mxcsr), 8);
            puts(b"\r\n");
            return;
        }
        if fpu::callback(3) == 0 {
            (*raw()).short_completed = (*raw()).short_completed.wrapping_add(1);
        }
    }
}
pub unsafe fn reserve_probe() -> bool {
    unsafe {
        let used_before = r4os_rust_task_control(4, 0, 0);
        if used_before >= 4 {
            return false;
        }
        if r4os_rust_task_control(30, 1, 0) == 0 {
            return false;
        }
        let mut failure = CreateFailure::None;
        let unexpected = super::owner_snapshots::create(
            b"taskreg-normal-admission",
            workers::short_main,
            State::Ready,
            Role::Interactive,
            &mut failure,
        );
        if !unexpected.is_null() {
            let v = view(unexpected);
            kill(v.id, v.generation);
            release(v.id, v.generation);
            return false;
        }
        if failure != CreateFailure::TaskMetadata {
            return false;
        }
        let mut first_fpu: *const u8 = ptr::null();
        for _ in 0..2 {
            (*raw()).short_completed = 0;
            let mut failure = CreateFailure::None;
            let t = resources::create_critical(
                Bytes {
                    pointer: b"taskreg-critical".as_ptr(),
                    len: 16,
                },
                State::Ready,
                critical_fpu,
                Role::Interactive,
                &mut failure,
            );
            if t.is_null() {
                return false;
            }
            if failure != CreateFailure::None || !pin(t) {
                return false;
            }
            let v = view(t);
            let stable = if let Some(memory) = v.fpu {
                first_fpu.is_null() || first_fpu == memory.pointer
            } else {
                false
            };
            if let Some(memory) = v.fpu {
                first_fpu = memory.pointer;
            }
            let mut spins = 0;
            while view(t).state != State::Dead && spins < 4096 {
                spins += 1;
                yield_task();
            }
            if view(t).state != State::Dead || (*raw()).short_completed != 1 || !stable {
                unpin(t);
                return false;
            }
            if !unpin(t) || !release(v.id, v.generation) {
                return false;
            }
        }
        if r4os_rust_task_control(4, 0, 0) != used_before {
            return false;
        }
        (*raw()).short_completed = 0;
        let mut failure = CreateFailure::None;
        let t = super::owner_snapshots::create(
            b"taskreg-recovery",
            workers::short_main,
            State::Ready,
            Role::Interactive,
            &mut failure,
        );
        if t.is_null() {
            return false;
        }
        if failure != CreateFailure::None || !pin(t) {
            return false;
        }
        let v = view(t);
        let mut spins = 0;
        while view(t).state != State::Dead && spins < 4096 {
            spins += 1;
            yield_task();
        }
        if view(t).state != State::Dead || (*raw()).short_completed != 1 {
            unpin(t);
            return false;
        }
        unpin(t) && release(v.id, v.generation)
    }
}
