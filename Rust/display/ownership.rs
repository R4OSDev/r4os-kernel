//! Existing program-state owner, nonrecursive execution and exact call pins.
//! No guard borrow crosses the scheduler's blocking admission.
use crate::sched::{sync_types::UnwindGuard, task_context::Unwind};
use core::ptr;
/// Resident storage protected by the documented display metadata/execution
/// owner. Raw pointers keep Rust borrows out of blocking owner admission.
pub struct Resident<T>(pub core::cell::UnsafeCell<T>);
// SAFETY: each user supplies its exact existing display owner; this wrapper
// neither admits access nor creates a reference to the protected record.
unsafe impl<T> Sync for Resident<T> {}
pub const fn execution(name: &'static [u8]) -> UnwindGuard {
    UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: crate::sched::sync_types::Text::literal(name),
        queue: crate::sched::sync_types::WaitQueue::EMPTY,
    }
}
pub struct Execution(*mut UnwindGuard);
/// Existing reentrant unwind guard, used where a transition calls its own
/// callback or recursively removes its exact additional outputs.
pub struct RecursiveExecution(*mut UnwindGuard);
impl RecursiveExecution {
    /// # Safety
    /// Exact resident guard outlives all nested admissions and callbacks.
    pub unsafe fn acquire(guard: *mut UnwindGuard) -> Result<Self, super::error::Error> {
        if unsafe { raw_guard(guard, 2, 0) } {
            Ok(Self(guard))
        } else {
            Err(super::error::Error::Busy)
        }
    }
}
impl Drop for RecursiveExecution {
    fn drop(&mut self) {
        let _ = unsafe { raw_guard(self.0, 3, 0) };
    }
}
impl Execution {
    /// # Safety
    /// The exact resident guard remains alive until this execution is dropped.
    pub unsafe fn acquire(guard: *mut UnwindGuard) -> Result<Self, super::error::Error> {
        if unsafe { enter(guard, 0) } {
            Ok(Self(guard))
        } else {
            Err(super::error::Error::Busy)
        }
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        unsafe { leave(self.0) }
    }
}
pub struct Call(Unwind);
impl Call {
    pub fn acquire() -> Result<Self, super::error::Error> {
        let token = retain_call();
        if token.admitted() {
            Ok(Self(token))
        } else {
            Err(super::error::Error::Busy)
        }
    }
}
impl Drop for Call {
    fn drop(&mut self) {
        release_call(self.0)
    }
}
pub struct StateGuard {
    #[cfg(not(r4os_owner_fixture))]
    _owner: crate::memory::owner::Guard,
}
impl Drop for StateGuard {
    fn drop(&mut self) {
        #[cfg(r4os_owner_fixture)]
        unsafe {
            r4os_owner_fixture_display_state(false)
        };
    }
}
pub fn enter_state() -> StateGuard {
    #[cfg(not(r4os_owner_fixture))]
    {
        StateGuard {
            _owner: crate::memory::owner::Guard::acquire(0),
        }
    }
    #[cfg(r4os_owner_fixture)]
    {
        unsafe { r4os_owner_fixture_display_state(true) };
        StateGuard {}
    }
}
pub fn retain_call() -> Unwind {
    #[cfg(not(r4os_owner_fixture))]
    {
        unsafe { crate::sched::task_context::enter() }
    }
    #[cfg(r4os_owner_fixture)]
    {
        Unwind::EMPTY
    }
}
pub fn release_call(token: Unwind) {
    #[cfg(not(r4os_owner_fixture))]
    let _ = unsafe { crate::sched::task_context::leave(token) };
    #[cfg(r4os_owner_fixture)]
    let _ = token;
}
/// # Safety
/// The exact resident execution guard outlives admission and every callback.
pub unsafe fn enter(guard: *mut UnwindGuard, timeout: u64) -> bool {
    if !unsafe { raw_guard(guard, 2, timeout) } {
        return false;
    }
    if unsafe { ptr::addr_of!((*guard).depth).read() } != 1 {
        let _ = unsafe { raw_guard(guard, 3, 0) };
        return false;
    }
    true
}
/// # Safety
/// Consumes the exact admitted execution after all callback/resource work.
pub unsafe fn leave(guard: *mut UnwindGuard) {
    unsafe {
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }
    let _ = unsafe { raw_guard(guard, 3, 0) };
}
/// # Safety
/// Exact initialized resident guard, serialized by its own native admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_execution(
    op: u32,
    guard: *mut UnwindGuard,
    timeout: u64,
) -> bool {
    match op {
        0 => unsafe { enter(guard, timeout) },
        1 => {
            unsafe { leave(guard) };
            true
        }
        _ => panic!("display execution operation"),
    }
}

// Host transport exists only in the original-owner fixture archive. The
// product always executes its original native runtime/program owners above.
#[cfg(r4os_owner_fixture)]
unsafe extern "C" {
    fn r4os_owner_fixture_display_state(enter: bool);
    fn r4os_owner_fixture_display_guard(
        guard: *mut UnwindGuard,
        operation: u8,
        timeout: u64,
    ) -> bool;
}
/// # Safety
/// Exact resident guard outlives admission and all nested callbacks. Host
/// fixtures substitute only OS mutex/thread admission, never display results.
pub(super) unsafe fn raw_guard(guard: *mut UnwindGuard, operation: u8, timeout: u64) -> bool {
    #[cfg(not(r4os_owner_fixture))]
    {
        unsafe { crate::sched::sync_locks::r4os_rust_sync_guard(guard, operation, timeout) }
    }
    #[cfg(r4os_owner_fixture)]
    {
        unsafe { r4os_owner_fixture_display_guard(guard, operation, timeout) }
    }
}
