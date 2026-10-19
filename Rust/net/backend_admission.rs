//! Sole callback/transition admission. A callback reference pins the original
//! adapter enumeration through protocol work and waits; it is not a held
//! runtime/network token. Terminal quarantine is never reopened.
use super::timing;
use crate::{boot::foundation, sched::scheduler};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
static TRANSITION: AtomicBool = AtomicBool::new(false);
static CLOSED: AtomicBool = AtomicBool::new(false);
static MUTATION: AtomicBool = AtomicBool::new(false);
static UNSAFE: AtomicBool = AtomicBool::new(false);
static CALLBACKS: AtomicU32 = AtomicU32::new(0);
const DRAIN_TICKS: u64 = timing::ms_to_ticks(1000);
pub(super) fn reset() {
    TRANSITION.store(false, Ordering::Release);
    CLOSED.store(false, Ordering::Release);
    MUTATION.store(false, Ordering::Release);
    UNSAFE.store(false, Ordering::Release);
    CALLBACKS.store(0, Ordering::Release);
}
fn wait(reason: &[u8]) {
    if !scheduler::current().is_null() {
        unsafe { scheduler::sleep(1, reason.as_ptr(), reason.len()) };
    } else {
        core::hint::spin_loop();
    }
}
fn drain_mutation() -> bool {
    let start = timing::now_ticks();
    while MUTATION.load(Ordering::Acquire) {
        if timing::now_ticks().wrapping_sub(start) >= DRAIN_TICKS {
            foundation::puts(b"[NET] system-transition backend mutation timeout\r\n");
            return false;
        }
        wait(b"net-transition-mutation");
    }
    true
}
fn drain_callbacks(reason: &[u8]) -> bool {
    let start = timing::now_ticks();
    while CALLBACKS.load(Ordering::Acquire) != 0 {
        if timing::now_ticks().wrapping_sub(start) >= DRAIN_TICKS {
            foundation::puts(b"[NET] ");
            foundation::puts(reason);
            foundation::puts(b" callback drain timeout active=");
            foundation::dec(u64::from(CALLBACKS.load(Ordering::Acquire)));
            foundation::puts(b"\r\n");
            return false;
        }
        wait(b"net-transition-drain");
    }
    true
}
pub(super) fn begin_transition() -> bool {
    if TRANSITION.swap(true, Ordering::AcqRel) {
        return false;
    }
    CLOSED.swap(true, Ordering::AcqRel);
    if !drain_mutation() {
        return false;
    }
    if UNSAFE.load(Ordering::Acquire) {
        foundation::puts(b"[NET] quarantined backend blocks warm reset\r\n");
        return false;
    }
    // A temporary mutation may have reopened just before its final release.
    CLOSED.swap(true, Ordering::AcqRel);
    drain_callbacks(b"system-transition")
}
pub(super) fn begin_mutation() -> bool {
    if TRANSITION.load(Ordering::Acquire) || MUTATION.swap(true, Ordering::AcqRel) {
        return false;
    }
    if TRANSITION.load(Ordering::Acquire) {
        MUTATION.store(false, Ordering::Release);
        return false;
    }
    if CLOSED.swap(true, Ordering::AcqRel) {
        MUTATION.store(false, Ordering::Release);
        return false;
    }
    if TRANSITION.load(Ordering::Acquire) {
        end_mutation();
        return false;
    }
    if drain_callbacks(b"backend-mutation") {
        true
    } else {
        end_mutation();
        false
    }
}
pub(super) fn end_mutation() {
    if !TRANSITION.load(Ordering::Acquire) && !UNSAFE.load(Ordering::Acquire) {
        CLOSED.store(false, Ordering::Release);
    }
    MUTATION.store(false, Ordering::Release);
}
pub(super) fn quarantine() {
    UNSAFE.swap(true, Ordering::AcqRel);
    CLOSED.swap(true, Ordering::AcqRel);
    MUTATION.store(false, Ordering::Release);
}
pub(super) fn enter() -> bool {
    if CLOSED.load(Ordering::Acquire) {
        return false;
    }
    CALLBACKS.fetch_add(1, Ordering::AcqRel);
    if !CLOSED.load(Ordering::Acquire) {
        return true;
    }
    CALLBACKS.fetch_sub(1, Ordering::Release);
    false
}
pub(crate) fn callbacks_open() -> bool {
    !CLOSED.load(Ordering::Acquire)
}
pub(super) fn leave() {
    CALLBACKS.fetch_sub(1, Ordering::Release);
}
pub(crate) struct CallbackGuard {
    unwind: crate::sched::task_context::Unwind,
}
impl CallbackGuard {
    pub(crate) fn acquire() -> Option<Self> {
        let unwind = {
            let _guard = crate::arch::runtime::RuntimeGuard::acquire();
            unsafe { crate::sched::task_context::enter() }
        };
        if !unwind.admitted() {
            return None;
        }
        if enter() {
            Some(Self { unwind })
        } else {
            let _guard = crate::arch::runtime::RuntimeGuard::acquire();
            assert!(unsafe { crate::sched::task_context::leave(unwind) });
            None
        }
    }
}
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        leave();
        let _guard = crate::arch::runtime::RuntimeGuard::acquire();
        assert!(
            unsafe { crate::sched::task_context::leave(self.unwind) },
            "Exact network callback unwind release"
        );
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_admission_reset() {
    reset();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_transition_begin() -> bool {
    begin_transition()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_mutation_begin() -> bool {
    begin_mutation()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_mutation_end() {
    end_mutation();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_mutation_quarantine() {
    quarantine();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_callback_enter() -> bool {
    enter()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_callback_leave() {
    leave();
}
