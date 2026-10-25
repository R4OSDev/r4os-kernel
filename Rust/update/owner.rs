//! Exclusive admission for one resident update scratch owner. The existing
//! wait-spanning UnwindGuard protects task lifetime while real I/O can park;
//! it never retains the scheduler RuntimeOwner or opens a filesystem request.
use crate::sched::{
    sync_locks,
    sync_types::{Text, UnwindGuard, WaitQueue},
};
use core::cell::UnsafeCell;
pub(super) struct Exclusive(UnsafeCell<UnwindGuard>);
// SAFETY: native sync serializes owner/generation/wait-queue transitions under
// its existing runtime owner. Before runtime only the linear BSP may enter.
unsafe impl Sync for Exclusive {}
impl Exclusive {
    pub const fn new(name: &'static [u8]) -> Self {
        Self(UnsafeCell::new(UnwindGuard {
            owner: 0,
            owner_generation: 0,
            boot_owner: false,
            depth: 0,
            name: Text::literal(name),
            queue: WaitQueue::EMPTY,
            ..UnwindGuard::EMPTY
        }))
    }
    pub fn acquire(&self) -> Option<Permit<'_>> {
        // Reentering the same scratch from a backend callback would overwrite
        // live bytes. Internal raw leaf helpers never reopen this admission.
        if unsafe { sync_locks::guard_owned(self.0.get()) } {
            return None;
        }
        if unsafe { sync_locks::guard_enter(self.0.get(), u64::MAX) } {
            Some(Permit(self))
        } else {
            None
        }
    }
}
pub(super) struct Permit<'a>(&'a Exclusive);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        assert!(
            unsafe { sync_locks::guard_leave(self.0.0.get()) },
            "Exact update scratch owner release"
        );
    }
}
