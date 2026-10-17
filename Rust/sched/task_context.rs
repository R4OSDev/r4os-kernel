//! Exact task-owned unwind anchor, CPU-local binding, and overflow admission.
//! Raw tokens retain their original counter through waits and task migration;
//! leave must never select whichever task happens to be current later.
use crate::arch::{percpu, smp_policy::MAX_CPUS};
use core::cell::UnsafeCell;
/// Keeps the original task resident through blocking native work and output.
/// Runtime admission covers only the counter transitions, never the work.
pub(crate) struct Admission(Unwind);
impl Admission {
    pub(crate) fn acquire(require_task: bool) -> Option<Self> {
        let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
        let token = unsafe { enter() };
        if !token.admitted() || require_task && !token.context_present {
            return None;
        }
        Some(Self(token))
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
        assert!(unsafe { leave(self.0) }, "native task unwind identity");
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Unwind {
    pub counter: *mut u32,
    pub context_present: bool,
    pub active: bool,
}
impl Unwind {
    pub const EMPTY: Self = Self {
        counter: core::ptr::null_mut(),
        context_present: false,
        active: false,
    };
    pub fn admitted(self) -> bool {
        !self.context_present || self.active
    }
}
struct Bindings(UnsafeCell<[*mut u32; MAX_CPUS]>);
// SAFETY: each slot is updated by its exact CPU under scheduler admission;
// current tasks own counters. Raw tokens are protected by the original unwind
// count and never become Rust references across a yield or context switch.
unsafe impl Sync for Bindings {}
static CURRENT: Bindings = Bindings(UnsafeCell::new([core::ptr::null_mut(); MAX_CPUS]));
/// # Safety
/// Scheduler binds the resident exact task counter with local IF disabled.
/// Null clears the binding. Counter storage outlives every admitted token.
pub unsafe fn bind(counter: *mut u32) {
    unsafe {
        CURRENT
            .0
            .get()
            .cast::<*mut u32>()
            .add(percpu::current_index() as usize)
            .write(counter)
    };
}
/// # Safety
/// Caller retains the scheduler's current-task admission. A successful token
/// becomes the exact unwind anchor and must be returned once to leave().
pub unsafe fn enter() -> Unwind {
    let counter = unsafe {
        CURRENT
            .0
            .get()
            .cast::<*mut u32>()
            .add(percpu::current_index() as usize)
            .read()
    };
    if counter.is_null() {
        return Unwind::EMPTY;
    }
    let depth = unsafe { counter.read() };
    if depth == u32::MAX {
        return Unwind {
            counter,
            context_present: true,
            active: false,
        };
    }
    unsafe { counter.write(depth + 1) };
    Unwind {
        counter,
        context_present: true,
        active: true,
    }
}
/// # Safety
/// Token retains its original resident exact counter and is consumed once.
/// Admission and lifetime remain the existing task/runtime owner's contract.
pub unsafe fn leave(token: Unwind) -> bool {
    if !token.active {
        return !token.context_present;
    }
    if token.counter.is_null() {
        return false;
    }
    let depth = unsafe { token.counter.read() };
    if depth == 0 {
        return false;
    }
    unsafe { token.counter.write(depth - 1) };
    true
}
/// # Safety
/// Same exact CPU/task admission as bind(); C transports only a raw pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_context_bind(counter: *mut u32) {
    unsafe { bind(counter) }
}
/// # Safety
/// Output is an exclusive aligned C Unwind; caller retains enter() admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_context_enter(output: *mut Unwind) {
    unsafe { output.write(enter()) }
}
/// # Safety
/// Input is a copied exact live C token under leave()'s lifetime contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_context_leave(token: *const Unwind) -> bool {
    unsafe { leave(token.read()) }
}
const _: () = {
    assert!(core::mem::size_of::<Unwind>() == 16);
    assert!(core::mem::offset_of!(Unwind, context_present) == 8);
    assert!(core::mem::offset_of!(Unwind, active) == 9);
};
