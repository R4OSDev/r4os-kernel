//! Mutex priority inheritance, serialized by the scheduler Runtime owner.
//!
//! One exact waiter edge contributes to four per-lock rank counts. One lock
//! contributes its minimum to its held owner, including masked donations.
//! A rank change queues the blocked Task, never a naked mutex pointer. The
//! linked wait node retains both endpoints; detach removes pending work before
//! either can be recycled. No allocation, registry scan or recursive walk.
pub(crate) mod probe;
use super::{
    sync_types::Mutex,
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

#[derive(Clone, Copy)]
pub(crate) struct Edge {
    mutex: *mut Mutex,
    generation: u64,
    rank: u8,
}
impl Edge {
    pub(crate) const EMPTY: Self = Self {
        mutex: ptr::null_mut(),
        generation: 0,
        rank: NO_RANK,
    };
    pub(crate) fn is_empty(self) -> bool {
        self.mutex.is_null()
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
unsafe fn owner(m: *mut Mutex) -> *mut Task {
    unsafe {
        let t = (*m).owner_task;
        if t.is_null() {
            assert!((*m).owner == 0 && (*m).owner_generation == 0);
            return t;
        }
        let c = registry::core(t);
        assert!((*c).id == (*m).owner && (*c).generation == (*m).owner_generation);
        assert!((*c).held_lock_count != 0 && !matches!((*c).state, State::Unused | State::Dead));
        t
    }
}
unsafe fn refresh(m: *mut Mutex) {
    unsafe {
        let t = owner(m);
        if t.is_null() {
            assert!((*m).donation_rank == NO_RANK);
            return;
        }
        // Batch urgency cannot improve any role. Its count still tracks its
        // exact node so later promotion and detach use the same four buckets.
        let rank = (0..3)
            .find(|&i| (*m).waiter_ranks[i] != 0)
            .map_or(NO_RANK, |i| i as u8);
        let old = (*m).donation_rank;
        if rank == old {
            return;
        }
        let inversion = rank < registry::dispatch_rank(t);
        registry::replace_donation_owned(t, old, rank);
        (*m).donation_rank = rank;
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
pub(crate) unsafe fn accept_owner(m: *mut Mutex, t: *mut Task) {
    unsafe {
        assert!((*m).owner_task.is_null() && (*m).donation_rank == NO_RANK);
        (*m).owner_task = t;
        refresh(m);
    }
}
pub(crate) unsafe fn release_owner(m: *mut Mutex) {
    unsafe {
        let t = owner(m);
        assert!(!t.is_null());
        let old = (*m).donation_rank;
        if old != NO_RANK {
            registry::replace_donation_owned(t, old, NO_RANK);
            super::sync_stats::lock_change(|s| {
                s.role_donation_releases = s.role_donation_releases.wrapping_add(1)
            });
        }
        (*m).donation_rank = NO_RANK;
        (*m).owner_task = ptr::null_mut();
    }
}
/// A prospective edge is rejected if it would close a cycle, or if bounded
/// validation cannot prove it acyclic. This is an ordinary failed lock wait,
/// not a claimed cycle result. Following owners uses their held lifetime.
unsafe fn acyclic(waiter: *mut Task, m: *mut Mutex) -> bool {
    unsafe {
        let mut t = owner(m);
        for _ in 0..VALIDATION_STEPS {
            if t.is_null() {
                return true;
            }
            if t == waiter {
                return false;
            }
            let edge = (*registry::core(t)).mutex_edge;
            if edge.is_empty() {
                return true;
            }
            if !valid_edge(t, edge) {
                return false;
            }
            t = owner(edge.mutex);
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
            && (*c).wait_node.queue == ptr::addr_of_mut!((*edge.mutex).queue.core)
            && (*edge.mutex).queue.core.generation == edge.generation
            && !(*edge.mutex).queue.core.closing
    }
}
/// After exact node enrollment and Blocked publication, before the saved park.
pub(crate) unsafe fn bind(t: *mut Task, m: *mut Mutex) -> bool {
    unsafe {
        let c = registry::core(t);
        assert!((*c).mutex_edge.is_empty() && !(*c).pi_queued);
        let edge = Edge {
            mutex: m,
            generation: (*m).queue.core.generation,
            rank: registry::wake_rank(t),
        };
        if !valid_edge(t, edge) || !acyclic(t, m) {
            return false;
        }
        (*m).waiter_ranks[edge.rank as usize] += 1;
        (*c).mutex_edge = edge;
        refresh(m);
        true
    }
}
/// Mark even when dispatch rank is unchanged: a role change can alter fresh
/// wake priority while the currently exhausted dispatch rank stays the same.
pub(crate) unsafe fn changed(t: *mut Task) {
    unsafe {
        let c = registry::core(t);
        if (*c).mutex_edge.is_empty() || (*c).pi_queued {
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
        let edge = (*c).mutex_edge;
        unqueue(t);
        (*c).mutex_edge = Edge::EMPTY;
        if !edge.is_empty() {
            let m = edge.mutex;
            assert!((*c).wait_node.queue == ptr::addr_of_mut!((*m).queue.core));
            assert!(
                (*c).wait_node.owner == t.cast()
                    && (*c).wait_node.owner_generation == (*c).generation
            );
            assert!((*m).waiter_ranks[edge.rank as usize] != 0);
            (*m).waiter_ranks[edge.rank as usize] -= 1;
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
            let edge = (*c).mutex_edge;
            assert!(valid_edge(t, edge));
            let rank = registry::wake_rank(t);
            if rank == edge.rank {
                continue;
            }
            let m = edge.mutex;
            assert!((*m).waiter_ranks[edge.rank as usize] != 0);
            (*m).waiter_ranks[edge.rank as usize] -= 1;
            (*m).waiter_ranks[rank as usize] += 1;
            (*c).mutex_edge.rank = rank;
            refresh(m);
        }
        !(*PENDING.0.get()).head.is_null()
    }
}
