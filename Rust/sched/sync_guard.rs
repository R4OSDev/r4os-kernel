//! Wait-spanning guard ownership and abortable FIFO successor reservations.
//! Runtime serializes every transition. A caller retains guard storage for
//! its whole admission, including the offered interval after node detach.
//! Task death cancels that offer before descriptor/stack retirement. A ready
//! offer keeps its original absolute timeout in the existing deadline index.
pub(crate) mod probe;
use super::{
    mutex_pi::{self as pi, Lock},
    scheduler,
    sync_types::{UnwindGuard, WaitRequest},
    sync_wait as wait,
    task::{
        task_registry as registry,
        task_types::{State, Task, WaitResult},
    },
};
use crate::{arch::runtime::RuntimeGuard, platform::timer};
use core::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
pub(crate) struct Offer {
    guard: *mut UnwindGuard,
    queue_generation: u64,
    epoch: u64,
}
impl Offer {
    pub(crate) const EMPTY: Self = Self {
        guard: ptr::null_mut(),
        queue_generation: 0,
        epoch: 0,
    };
    pub(crate) fn is_empty(self) -> bool {
        self.guard.is_null()
    }
}
pub(crate) struct GuardMetadata {
    pub owner: u32,
    pub generation: u64,
    pub depth: u32,
    pub boot_owner: bool,
}
pub(crate) unsafe fn guard_metadata(g: *const UnwindGuard) -> GuardMetadata {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        GuardMetadata {
            owner: (*g).owner,
            generation: (*g).owner_generation,
            depth: (*g).depth,
            boot_owner: (*g).boot_owner,
        }
    }
}
unsafe fn free(g: *const UnwindGuard) -> bool {
    unsafe { (*g).owner == 0 && (*g).owner_generation == 0 && !(*g).boot_owner && (*g).depth == 0 }
}
/// Exact task/guard/offer identity; queue close may advance the queue epoch
/// before cancellation, so validity for claim is checked separately.
pub(super) unsafe fn offered(t: *mut Task, g: *mut UnwindGuard) -> bool {
    unsafe {
        let c = registry::core(t);
        let o = (*c).guard_offer;
        o.guard == g
            && !g.is_null()
            && o.epoch != 0
            && o.epoch == (*g).handoff_epoch
            && (*g).owner == (*c).id
            && (*g).owner_generation == (*c).generation
            && (*g).owner_task == t
            && (*g).depth == 0
            && !(*g).boot_owner
    }
}
pub(crate) unsafe fn guard_owned(g: *const UnwindGuard) -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let t = scheduler::current().cast::<Task>();
        if t.is_null() {
            return (*g).boot_owner && (*g).depth != 0;
        }
        let c = registry::core(t);
        (*g).owner == (*c).id && (*g).owner_generation == (*c).generation && (*g).depth != 0
    }
}
pub(crate) unsafe fn guard_try(g: *mut UnwindGuard) -> bool {
    let flags = wait::enter();
    let result = unsafe { try_owned(g) };
    wait::leave(flags);
    result
}
unsafe fn try_owned(g: *mut UnwindGuard) -> bool {
    unsafe {
        if (*g).queue.core.closing {
            return false;
        }
        let t = scheduler::current().cast::<Task>();
        if t.is_null() {
            if free(g) {
                (*g).boot_owner = true;
                (*g).depth = 1;
                return true;
            }
            if (*g).boot_owner && (*g).depth != u32::MAX {
                (*g).depth += 1;
                return true;
            }
            return false;
        }
        let c = registry::core(t);
        if free(g) {
            if (*c).unwind_guard_count == u32::MAX {
                return false;
            }
            assert!((*c).guard_offer.is_empty());
            (*g).owner = (*c).id;
            (*g).owner_generation = (*c).generation;
            (*g).depth = 1;
            (*c).unwind_guard_count += 1;
            pi::accept_owner(Lock::Guard(g), t);
            true
        } else if (*g).owner == (*c).id
            && (*g).owner_generation == (*c).generation
            && (*g).depth != 0
            && (*g).depth != u32::MAX
        {
            (*g).depth += 1;
            true
        } else {
            false
        }
    }
}
unsafe extern "C" fn needed(raw: *mut c_void) -> bool {
    unsafe { !free(raw.cast()) }
}
pub(crate) unsafe fn guard_enter(g: *mut UnwindGuard, timeout: u64) -> bool {
    unsafe {
        if guard_try(g) {
            return true;
        }
        if timeout == 0 {
            return false;
        }
        let request = WaitRequest::predicate(timeout, (*g).name, needed, g.cast());
        // wait_guard either acquires the free guard under its enrollment
        // admission or claims the exact offer after park, before returning.
        wait::wait_guard(g, &request) == WaitResult::Signaled
    }
}
/// One FIFO node, no scan or drain. A selected expired waiter still receives
/// only an offer; the regular bounded timeout owner or claim rejects it and
/// hands on to one successor. Remaining waiters donate to the offered task.
unsafe fn select(g: *mut UnwindGuard) {
    unsafe {
        assert!(free(g) && (*g).owner_task.is_null());
        if (*g).queue.core.closing {
            return;
        }
        let node = (*g).queue.core.head;
        if node.is_null() {
            return;
        }
        let t = (*node).owner.cast::<Task>();
        let c = registry::core(t);
        assert!((*c).state == State::Blocked && (*c).generation == (*node).owner_generation);
        assert!(node == ptr::addr_of_mut!((*c).wait_node) && (*c).guard_offer.is_empty());
        assert!(pi::bound(t, Lock::Guard(g)));
        pi::detach(t);
        (*g).handoff_epoch = (*g).handoff_epoch.wrapping_add(1).max(1);
        (*g).owner = (*c).id;
        (*g).owner_generation = (*c).generation;
        (*c).guard_offer = Offer {
            guard: g,
            queue_generation: (*g).queue.core.generation,
            epoch: (*g).handoff_epoch,
        };
        pi::accept_owner(Lock::Guard(g), t);
        assert!(
            scheduler::r4os_rust_scheduler_task(
                2,
                t.cast(),
                WaitResult::Signaled as u64,
                0,
                ptr::null(),
                0
            ) != 0
        );
        wait::note(super::sync_stats::WaitMetric::DirectedWakes);
        wait::note(super::sync_stats::WaitMetric::WakeOne);
    }
}
/// Runtime held. Cancel before Task death, timeout, explicit cancellation or
/// failed admission. No admission counter was acquired by the reservation.
/// Source storage remains retained by the exact caller until this completes.
pub(crate) unsafe fn cancel(t: *mut Task, result: WaitResult) -> bool {
    unsafe {
        let c = registry::core(t);
        let g = (*c).guard_offer.guard;
        if g.is_null() {
            return false;
        }
        assert!(offered(t, g));
        pi::release_owner(Lock::Guard(g));
        (*c).guard_offer = Offer::EMPTY;
        registry::clear_guard_deadline(t);
        (*c).wait_result = result;
        (*g).owner = 0;
        (*g).owner_generation = 0;
        select(g);
        true
    }
}
/// Resume under Runtime, before the guard admission can return to its caller.
/// Do not touch a canceled offer's guard pointer: cancellation already gave
/// back its lifetime. A surviving offer is the explicit resident lease.
pub(super) unsafe fn claim(t: *mut Task) -> WaitResult {
    unsafe {
        let c = registry::core(t);
        let o = (*c).guard_offer;
        if o.is_empty() {
            return if (*c).wait_result == WaitResult::Signaled {
                WaitResult::Failed
            } else {
                (*c).wait_result
            };
        }
        let g = o.guard;
        assert!(offered(t, g));
        let failure = if (*g).queue.core.closing || (*g).queue.core.generation != o.queue_generation
        {
            Some(WaitResult::Cancelled)
        } else if (*c).wake_tick != 0 && timer::tick_count() >= (*c).wake_tick {
            Some(WaitResult::Timeout)
        } else if (*c).unwind_guard_count == u32::MAX {
            Some(WaitResult::Failed)
        } else {
            None
        };
        if let Some(result) = failure {
            cancel(t, result);
            return result;
        }
        (*c).unwind_guard_count += 1;
        (*g).depth = 1;
        (*c).guard_offer = Offer::EMPTY;
        registry::clear_guard_deadline(t);
        WaitResult::Signaled
    }
}
pub(crate) unsafe fn guard_leave(g: *mut UnwindGuard) -> bool {
    let flags = wait::enter();
    let result = unsafe {
        let t = scheduler::current().cast::<Task>();
        if t.is_null() {
            if !(*g).boot_owner || (*g).depth == 0 {
                false
            } else {
                (*g).depth -= 1;
                if (*g).depth == 0 {
                    (*g).boot_owner = false;
                    select(g);
                }
                true
            }
        } else {
            let c = registry::core(t);
            if (*g).owner != (*c).id || (*g).owner_generation != (*c).generation || (*g).depth == 0
            {
                false
            } else {
                if (*g).depth == 1 {
                    pi::release_owner(Lock::Guard(g));
                    assert!((*c).unwind_guard_count != 0);
                    (*c).unwind_guard_count -= 1;
                    (*g).depth = 0;
                    (*g).owner = 0;
                    (*g).owner_generation = 0;
                    select(g);
                } else {
                    (*g).depth -= 1;
                }
                true
            }
        }
    };
    wait::leave(flags);
    result
}
