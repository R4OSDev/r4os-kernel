//! Task-bound TCP admission. Public buffers may fault into the pager, so the
//! table owner must allow a task wait without holding a CPU/runtime/no-sleep
//! token. The native unwind guard pins its exact task generation until release.
use crate::sched::{
    sync_locks,
    sync_types::{Text, UnwindGuard, WaitQueue},
};
use core::{cell::UnsafeCell, marker::PhantomData};
struct Storage(UnsafeCell<UnwindGuard>);
// SAFETY: only native guard operations mutate this resident owner. Every TCP
// table access holds the same exact task/generation-bound acquisition.
unsafe impl Sync for Storage {}
static OWNER: Storage = Storage(UnsafeCell::new(UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"net-tcp"),
    queue: WaitQueue::EMPTY,
}));
pub(super) struct Guard {
    _task_bound: PhantomData<*mut ()>,
}
impl Guard {
    pub(super) fn acquire() -> Option<Self> {
        crate::memory::owner::prepare_stack();
        if unsafe { sync_locks::guard_enter(OWNER.0.get(), u64::MAX) } {
            Some(Self {
                _task_bound: PhantomData,
            })
        } else {
            None
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        // Publish the bounded non-owning scheduler projection before releasing
        // the exact TCP task owner. Scheduler predicates cannot wait for us.
        unsafe { super::tcp_wait::publish_owned() };
        assert!(
            unsafe { sync_locks::guard_leave(OWNER.0.get()) },
            "Exact TCP owner release"
        );
    }
}
