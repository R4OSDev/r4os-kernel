//! One atomic firmware-writer gate and original exact task unwind lease.
//! A display takeover revokes only between fully completed writers.
use super::ownership;
use crate::sched::task_context::Unwind;
use core::sync::atomic::{AtomicU32, Ordering};
const REVOKED: u32 = 1 << 31;
#[repr(C)]
pub struct Gate {
    state: AtomicU32,
}
impl Gate {
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
        }
    }
    pub fn try_acquire(&self) -> bool {
        let observed = self.state.load(Ordering::Acquire);
        if observed >= REVOKED - 1 {
            return false;
        }
        self.state
            .compare_exchange(observed, observed + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn release(&self) {
        unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)) }
        let old = self.state.fetch_sub(1, Ordering::Release);
        assert!(old > 0 && old < REVOKED);
    }
    pub fn try_revoke(&self) -> bool {
        self.state
            .compare_exchange(0, REVOKED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn restore(&self) {
        assert!(self.state.load(Ordering::Acquire) == REVOKED);
        self.state.store(0, Ordering::Release)
    }
    pub fn is_revoked(&self) -> bool {
        self.state.load(Ordering::Acquire) & REVOKED != 0
    }
}
impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}
#[unsafe(no_mangle)]
pub static r4os_rust_display_firmware_gate: Gate = Gate::new();
pub fn acquire_token() -> Option<Unwind> {
    let call = ownership::retain_call();
    if !call.admitted() {
        return None;
    }
    if !r4os_rust_display_firmware_gate.try_acquire() {
        ownership::release_call(call);
        return None;
    }
    Some(call)
}
pub fn release_token(call: Unwind) {
    r4os_rust_display_firmware_gate.release();
    ownership::release_call(call)
}
pub struct Lease(Unwind);
impl Lease {
    pub fn acquire() -> Option<Self> {
        acquire_token().map(Self)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        release_token(self.0)
    }
}
/// # Safety
/// Exact initialized gate remains resident; operation follows its admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_firmware_gate_op(op: u32, gate: *const Gate) -> bool {
    let gate = unsafe { &*gate };
    match op {
        0 => gate.try_acquire(),
        1 => {
            gate.release();
            true
        }
        2 => gate.try_revoke(),
        3 => {
            gate.restore();
            true
        }
        4 => gate.is_revoked(),
        _ => panic!("firmware gate operation"),
    }
}
/// # Safety
/// Exclusive output holds the exact admitted task token until release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_firmware_acquire(out: *mut Unwind) -> bool {
    let Some(call) = acquire_token() else {
        return false;
    };
    unsafe { out.write(call) };
    true
}
/// # Safety
/// Consumes the exact live firmware/call token once, after the writer completes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_firmware_release(token: *const Unwind) {
    release_token(unsafe { token.read() })
}
const _: () = {
    assert!(core::mem::size_of::<Gate>() == 4 && core::mem::align_of::<Gate>() == 4);
};
