//! Mutex/UnwindGuard priority inheritance, serialized by the scheduler Runtime owner.
//!
//! One exact waiter edge contributes to four per-lock rank counts. One lock
//! contributes its minimum to its held owner, including masked donations.
//! A rank change queues the blocked Task, never a naked mutex pointer. The
//! linked wait node retains both endpoints; detach removes pending work before
//! either can be recycled. No allocation, registry scan or recursive walk.
pub(crate) mod probe;
use super::{
    sync_types::{Mutex, UnwindGuard, WaitQueue},
    task::{
        task_registry as registry,
        task_types::{State, Task},
    },
    wait_node,
};
use core::{cell::UnsafeCell, ptr};

pub(crate) const NO_RANK: u8 = 255;
pub(crate) const STEPS: usize = 8;
const VALIDATION_STEPS: usize = 64;

// Both lock kinds share the same dependency graph and bounded continuation.
// A guard offer retains its exact task until cancellation/claim; actual owners
// retain their original held-lock or unwind admission count.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Lock {
    None,
    Mutex(*mut Mutex),
    Guard(*mut UnwindGuard),
}
macro_rules! field {
    ($name:ident, $ty:ty) => {
        unsafe fn $name(self) -> *mut $ty {
            unsafe {
                match self {
                    Self::Mutex(p) => ptr::addr_of_mut!((*p).$name),
                    Self::Guard(p) => ptr::addr_of_mut!((*p).$name),
                    Self::None => panic!("Missing PI lock"),
                }
            }
        }
    };
}
impl Lock {
    field!(queue, WaitQueue);
    field!(owner, u32);
    field!(owner_generation, u64);
    field!(owner_task, *mut Task);
    field!(donation_rank, u8);
    field!(waiter_ranks, [usize; 4]);
}

#[derive(Clone, Copy)]
pub(crate) struct Edge {
    lock: Lock,
    generation: u64,
    rank: u8,
}
impl Edge {
    pub(crate) const EMPTY: Self = Self {
        lock: Lock::None,
        generation: 0,
        rank: NO_RANK,
    };
    pub(crate) fn is_empty(self) -> bool {
        self.lock == Lock::None
    }
}
struct Pending {
    head: *mut Task,
    tail: *mut Task,
}
struct Cell(UnsafeCell<Pending>);
// All access, including scheduler IRQ continuation, holds Runtime. Membership
// is possible only while the Task's own wait node retains its exact mutex.
unsafe impl Sync for Cell {}
static PENDING: Cell = Cell(UnsafeCell::new(Pending {
    head: ptr::null_mut(),
    tail: ptr::null_mut(),
}));

/// Caller holds Runtime and the exact held mutex. held_lock_count excludes
/// retirement of owner_task until release_owner clears that pointer.
unsafe fn owner(m: Lock) -> *mut Task {
    unsafe {
        let t = *m.owner_task();
        if t.is_null() {
            assert!((*m.owner()) == 0 && (*m.owner_generation()) == 0);
            return t;
        }
        let c = registry::core(t);
        assert!((*c).id == (*m.owner()) && (*c).generation == (*m.owner_generation()));
        assert!(!matches!((*c).state, State::Unused | State::Dead));
        match m {
            Lock::Mutex(_) => assert!((*c).held_lock_count != 0),
            Lock::Guard(g) => assert!(
                (*g).depth != 0 && (*c).unwind_guard_count != 0
                    || (*g).depth == 0 && super::sync_guard::offered(t, g)
            ),
            Lock::None => unreachable!(),
        }
        t
    }
}
unsafe fn refresh(m: Lock) {
    unsafe {
        let t = owner(m);
        if t.is_null() {
            assert!((*m.donation_rank()) == NO_RANK);
            return;
        }
        // Batch urgency cannot improve any role. Its count still tracks its
        // exact node so later promotion and detach use the same four buckets.
        let rank = (0..3)
            .find(|&i| (*m.waiter_ranks())[i] != 0)
            .map_or(NO_RANK, |i| i as u8);
        let old = *m.donation_rank();
        if rank == old {
            return;
        }
        let inversion = rank < registry::dispatch_rank(t);
        registry::replace_donation_owned(t, old, rank);
        (*m.donation_rank()) = rank;
        super::sync_stats::lock_change(|s| {
            if old != NO_RANK {
                s.role_donation_releases = s.role_donation_releases.wrapping_add(1);
            }
            if rank != NO_RANK {
                s.role_donations = s.role_donations.wrapping_add(1);
                if inversion {
                    s.role_inversions = s.role_inversions.wrapping_add(1);
                }
            }
        });
    }
}
pub(super) unsafe fn accept_owner(m: Lock, t: *mut Task) {
    unsafe {
        assert!((*m.owner_task()).is_null() && (*m.donation_rank()) == NO_RANK);
        (*m.owner_task()) = t;
        refresh(m);
    }
}
pub(super) unsafe fn release_owner(m: Lock) {
    unsafe {
        let t = owner(m);
        assert!(!t.is_null());
        let old = *m.donation_rank();
        if old != NO_RANK {
            registry::replace_donation_owned(t, old, NO_RANK);
            super::sync_stats::lock_change(|s| {
                s.role_donation_releases = s.role_donation_releases.wrapping_add(1)
            });
        }
        (*m.donation_rank()) = NO_RANK;
        (*m.owner_task()) = ptr::null_mut();
    }
}
/// A prospective edge is rejected if it would close a cycle, or if bounded
/// validation cannot prove it acyclic. This is an ordinary failed lock wait,
/// not a claimed cycle result. Following owners uses their held lifetime.
unsafe fn acyclic(waiter: *mut Task, m: Lock) -> bool {
    unsafe {
        let mut t = owner(m);
        for _ in 0..VALIDATION_STEPS {
            if t.is_null() {
                return true;
            }
            if t == waiter {
                return false;
            }
            let edge = (*registry::core(t)).lock_edge;
            if edge.is_empty() {
                return true;
            }
            if !valid_edge(t, edge) {
                return false;
            }
            t = owner(edge.lock);
        }
        false
    }
}
unsafe fn valid_edge(t: *mut Task, edge: Edge) -> bool {
    unsafe {
        let c = registry::core(t);
        !edge.is_empty()
            && (*c).state == State::Blocked
            && (*c).wait_node.owner == t.cast()
            && (*c).wait_node.owner_generation == (*c).generation
            && (*c).wait_node.queue == ptr::addr_of_mut!((*edge.lock.queue()).core)
            && (*edge.lock.queue()).core.generation == edge.generation
            && !(*edge.lock.queue()).core.closing
    }
}
pub(super) unsafe fn bound(t: *mut Task, lock: Lock) -> bool {
    unsafe {
        let edge = (*registry::core(t)).lock_edge;
        edge.lock == lock && valid_edge(t, edge)
    }
}
/// After exact node enrollment and Blocked publication, before the saved park.
pub(super) unsafe fn bind(t: *mut Task, m: Lock) -> bool {
    unsafe {
        let c = registry::core(t);
        assert!((*c).lock_edge.is_empty() && !(*c).pi_queued);
        let edge = Edge {
            lock: m,
            generation: (*m.queue()).core.generation,
            rank: registry::wake_rank(t),
        };
        if !valid_edge(t, edge) || !acyclic(t, m) {
            return false;
        }
        (*m.waiter_ranks())[edge.rank as usize] += 1;
        (*c).lock_edge = edge;
        refresh(m);
        true
    }
}
/// Mark even when dispatch rank is unchanged: a role change can alter fresh
/// wake priority while the currently exhausted dispatch rank stays the same.
pub(crate) unsafe fn changed(t: *mut Task) {
    unsafe {
        let c = registry::core(t);
        // Plain waits and changes behind an already queued PI propagation
        // still publish their current wake rank in this same admission.
        if !(*c).wait_node.queue.is_null()
            && (*c).wait_node.owner == t.cast()
            && (*c).wait_node.owner_generation == (*c).generation
        {
            wait_node::set_rank(ptr::addr_of_mut!((*c).wait_node), registry::wake_rank(t));
        }
        if (*c).lock_edge.is_empty() || (*c).pi_queued {
            return;
        }
        let p = PENDING.0.get();
        (*c).pi_prev = (*p).tail;
        (*c).pi_next = ptr::null_mut();
        (*c).pi_queued = true;
        if (*p).tail.is_null() {
            (*p).head = t;
        } else {
            (*registry::core((*p).tail)).pi_next = t;
        }
        (*p).tail = t;
    }
}
unsafe fn unqueue(t: *mut Task) {
    unsafe {
        let c = registry::core(t);
        if !(*c).pi_queued {
            return;
        }
        let p = PENDING.0.get();
        if (*c).pi_prev.is_null() {
            (*p).head = (*c).pi_next;
        } else {
            (*registry::core((*c).pi_prev)).pi_next = (*c).pi_next;
        }
        if (*c).pi_next.is_null() {
            (*p).tail = (*c).pi_prev;
        } else {
            (*registry::core((*c).pi_next)).pi_prev = (*c).pi_prev;
        }
        (*c).pi_prev = ptr::null_mut();
        (*c).pi_next = ptr::null_mut();
        (*c).pi_queued = false;
    }
}
/// Remove only this exact Task edge before the generic node loses its storage
/// lifetime. A close advances queue generation *before* draining; it must still
/// withdraw the enrolled contribution, without treating it as a fresh edge.
pub(crate) unsafe fn detach(t: *mut Task) -> bool {
    unsafe {
        let c = registry::core(t);
        let edge = (*c).lock_edge;
        unqueue(t);
        (*c).lock_edge = Edge::EMPTY;
        if !edge.is_empty() {
            let m = edge.lock;
            assert!((*c).wait_node.queue == ptr::addr_of_mut!((*m.queue()).core));
            assert!(
                (*c).wait_node.owner == t.cast()
                    && (*c).wait_node.owner_generation == (*c).generation
            );
            assert!((*m.waiter_ranks())[edge.rank as usize] != 0);
            (*m.waiter_ranks())[edge.rank as usize] -= 1;
            refresh(m);
        }
        wait_node::detach(ptr::addr_of_mut!((*c).wait_node))
    }
}
/// At most eight changed edges per call, with four waiter buckets each and
/// the existing Task rank publication. Queue membership holds
/// storage; no pin release/heap teardown can occur inside this IRQ-safe step.
/// Scheduler tick, dispatch and idle admission continue any remaining work.
/// All callers hold Runtime; true forbids an idle halt with unfinished work.
pub(crate) unsafe fn pump() -> bool {
    unsafe {
        for _ in 0..STEPS {
            let t = (*PENDING.0.get()).head;
            if t.is_null() {
                return false;
            }
            unqueue(t);
            let c = registry::core(t);
            let edge = (*c).lock_edge;
            assert!(valid_edge(t, edge));
            let rank = registry::wake_rank(t);
            if rank == edge.rank {
                continue;
            }
            let m = edge.lock;
            assert!((*m.waiter_ranks())[edge.rank as usize] != 0);
            (*m.waiter_ranks())[edge.rank as usize] -= 1;
            (*m.waiter_ranks())[rank as usize] += 1;
            (*c).lock_edge.rank = rank;
            refresh(m);
        }
        !(*PENDING.0.get()).head.is_null()
    }
}
