//! Bounded Ready-queue transfer under the existing scheduler runtime owner.
//! No running stack, allocation, wait, external callback or public affinity API.
use super::{
    task_registry as registry,
    task_types::{ExecutionOwnerKind, State, Task},
};
use crate::{arch::percpu, platform::numa};
use core::ptr;
const CANDIDATES: usize = 8;

/// # Safety
/// Caller owns Runtime admission; these copied revisions retain no task.
pub(crate) unsafe fn search_revisions() -> (u64, u64) {
    unsafe { ((*registry::raw()).epoch, (*registry::raw()).ready_revision) }
}

/// # Safety
/// Existing Ready/policy publication under Runtime, including a re-notify
/// after temporary affinity release without changing FIFO linkage.
pub(crate) unsafe fn note_ready_publication(task: *mut Task) {
    unsafe {
        if !task.is_null() && (*registry::core(task)).ready_linked {
            registry::publish_ready_change(registry::queue(u32::from(
                (*registry::core(task)).home_cpu,
            )));
        } else {
            (*registry::raw()).ready_revision = (*registry::raw()).ready_revision.wrapping_add(1);
        }
    }
}

/// # Safety
/// The following bounded pull uses this exact unchanged Runtime-owned queue.
pub(crate) unsafe fn complete_prefix(source: u32) -> bool {
    unsafe { (*registry::queue(source)).count <= CANDIDATES }
}

/// # Safety
/// Copied static exclusion from the completed traversal, under the same
/// Runtime admission. Saved stack/FPU/guard handoffs are transient and must
/// never become a negative hint merely because no cooldown clock was read.
pub(crate) unsafe fn stable_exclusion(source: u32, target: u32) -> bool {
    unsafe { (*registry::queue(source)).transfer_summary.excludes(target) }
}

/// # Safety
/// Runtime admission and the exact task lifetime remain with the caller.
/// Initial/wake rehome and queued transfer share every saved-state barrier.
pub(crate) unsafe fn placeable(candidate: *mut Task, initial: bool) -> bool {
    unsafe {
        !candidate.is_null()
            && (*registry::core(candidate)).smp_eligible
            && !(*registry::core(candidate)).cpu_affinity_bound
            && saved_continuation(candidate, initial)
    }
}

pub(crate) unsafe fn saved_continuation(candidate: *mut Task, initial: bool) -> bool {
    if candidate.is_null() {
        return false;
    }
    unsafe {
        let c = registry::core(candidate);
        (*c).state == State::Ready
            && (*c).running_cpu == 0xff
            && !(*c).cpu_idle
            && !(*c).placement_deferred
            && (initial || execution_owner_movable(candidate))
            && !(*c).retire_pending
            && !(*c).release_in_progress
            && !(*c).rollback_pending
            && (*c).preempt_disable_depth == 0
            && (*c).held_lock_count == 0
            && (!(*c).wait_handoff_guard_pending && (*c).guard_offer.is_empty())
            && (*c).wait_node.queue.is_null()
            && !(*c).timeout_linked
            && (*c).wake_tick == 0
            && (*c).rsp != 0
            && (!(*c).uses_fpu || ((*c).fpu_state_valid && (*c).fpu_state.is_some()))
    }
}
/// # Safety
/// Runtime retains the exact live Task. AsyncIO's authoritative atomic binding
/// must confirm that every foreign callback has returned; generic saved-stack,
/// FPU, guard and retirement checks remain at each caller.
pub(crate) unsafe fn execution_owner_movable(task: *mut Task) -> bool {
    unsafe {
        match (*registry::core(task)).execution_owner_kind {
            ExecutionOwnerKind::None | ExecutionOwnerKind::ProgramThread => true,
            ExecutionOwnerKind::AsyncIo => crate::program::async_io::idle_transfer(task),
            _ => false,
        }
    }
}
pub(crate) unsafe fn relocatable(candidate: *mut Task) -> bool {
    unsafe { placeable(candidate, false) }
}

unsafe fn eligible(
    candidate: *mut Task,
    current: *mut Task,
    source: u32,
    target: Option<u32>,
    now: impl FnOnce() -> u64,
    cooldown: u64,
) -> bool {
    if candidate.is_null() {
        return false;
    }
    unsafe {
        let c = registry::core(candidate);
        candidate != current
            && relocatable(candidate)
            && (*c).ready_linked
            && u32::from((*c).home_cpu) == source
            && ((*c).last_migration_tick == 0 || {
                let stamp = now();
                !super::migration::cooling(candidate, stamp, cooldown)
                    || match target {
                        Some(target) => {
                            super::migration::can_move(candidate, source, target, stamp, cooldown)
                        }
                        // A singleton shortcut may reject a structurally bound
                        // task or exhausted budget, but cannot hide a possible
                        // exception before the bounded target search runs.
                        None => super::migration::available(candidate, stamp, cooldown),
                    }
            })
    }
}
/// # Safety
/// Caller retains runtime admission and the exact source current. A singleton
/// denied by the same transfer predicate cannot use any target CPU. Reject it
/// before the wake hot path searches targets; leave larger queues/cursors alone.
/// The bounded pure clock supplier is called only after structural admission,
/// and only when a previous migration actually requires a cooldown check.
pub(crate) unsafe fn singleton_blocked(
    source: u32,
    current: *mut Task,
    now: impl FnOnce() -> u64,
    cooldown: u64,
) -> bool {
    unsafe {
        let from = registry::queue(source);
        (*from).count == 1 && !eligible((*from).head, current, source, None, now, cooldown)
    }
}

/// # Safety
/// Runtime admission protects the queue and the CPU's exact physical current
/// task. A Ready current continuation can already be linked in that queue;
/// count its work once. Blocked/dead and pure idle continuations add no work.
pub(crate) unsafe fn workload(cpu: u32, current: *mut Task) -> usize {
    unsafe {
        let ready = (*registry::queue(cpu)).count;
        if current.is_null() {
            return ready;
        }
        let c = registry::core(current);
        ready.saturating_add(usize::from(
            !(*c).cpu_idle
                && matches!((*c).state, State::Running | State::Ready)
                && !(*c).ready_linked,
        ))
    }
}
/// # Safety
/// Both exact current instances and queues remain under runtime admission.
pub(crate) unsafe fn imbalanced(
    source: u32,
    target: u32,
    current: *mut Task,
    target_current: *mut Task,
) -> bool {
    if source == target
        || source as usize >= registry::MAX_CPUS
        || target as usize >= registry::MAX_CPUS
    {
        return false;
    }
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    if online & (1 << source) == 0 || online & (1 << target) == 0 {
        return false;
    }
    unsafe {
        (*registry::queue(source)).count != 0
            && workload(source, current) > workload(target, target_current).saturating_add(1)
    }
}

/// A single-use result, valid only within the caller's uninterrupted runtime
/// admission and while queues, physical currents and membership stay unchanged.
pub(crate) struct Transfer {
    source: u32,
    target: u32,
    current: *mut Task,
    target_current: *mut Task,
}

impl Transfer {
    #[inline]
    pub(crate) fn target(&self) -> u32 {
        self.target
    }
}

/// # Safety
/// Caller retains runtime admission and both exact physical current lifetimes.
#[inline]
pub(crate) unsafe fn prepare(
    source: u32,
    target: u32,
    current: *mut Task,
    target_current: *mut Task,
) -> Option<Transfer> {
    if !unsafe { imbalanced(source, target, current, target_current) } {
        return None;
    }
    unsafe {
        let q = registry::queue(source);
        let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        (*q).transfer_summary.begin((*q).transfer_revision, online);
        if (*q).transfer_summary.excludes(target) {
            return None;
        }
    }
    Some(Transfer {
        source,
        target,
        current,
        target_current,
    })
}

/// # Safety
/// Caller holds runtime admission and supplies the source CPU's exact current
/// task. Even a logically Ready current task still owns its physical stack.
pub(crate) unsafe fn pull(
    source: u32,
    target: u32,
    current: *mut Task,
    target_current: *mut Task,
    now: impl FnMut() -> u64,
    cooldown: u64,
) -> *mut Task {
    let Some(transfer) = (unsafe { prepare(source, target, current, target_current) }) else {
        return ptr::null_mut();
    };
    unsafe { pull_prepared(transfer, now, cooldown) }
}

/// # Safety
/// The caller consumes its exact prepared result without any intervening queue,
/// physical-current or membership mutation, under the same runtime admission.
pub(crate) unsafe fn pull_prepared(
    transfer: Transfer,
    mut now: impl FnMut() -> u64,
    cooldown: u64,
) -> *mut Task {
    let Transfer {
        source,
        target,
        current,
        target_current,
    } = transfer;
    unsafe {
        // One functional snapshot per bounded attempt, only once a structural
        // candidate needs cooldown/admission time. Bound-only prefixes never
        // publish the global clock merely to discover they cannot move.
        let mut time = None;
        let from = registry::queue(source);
        let mut candidate = if (*from).balance_cursor.is_null() {
            (*from).head
        } else {
            (*from).balance_cursor
        };
        let mut selected: *mut Task = ptr::null_mut();
        let mut proximity = None;
        for _ in 0..(*from).count.min(CANDIDATES) {
            if candidate.is_null() {
                break;
            }
            let c = registry::core(candidate);
            let next = if (*c).ready_next.is_null() {
                (*from).head
            } else {
                (*c).ready_next
            };
            (*from).balance_cursor = next;
            let possible = if (*c).smp_eligible && !(*c).cpu_affinity_bound && !(*c).cpu_idle {
                super::thread_placement::allowed(candidate)
            } else {
                0
            };
            (*from).transfer_summary.visit(possible, (*from).count);
            if candidate != current
                && candidate != target_current
                && possible & (1u64 << target) != 0
                && eligible(
                    candidate,
                    current,
                    source,
                    Some(target),
                    || *time.get_or_insert_with(&mut now),
                    cooldown,
                )
            {
                let key = numa::preference(1 << target, (*c).memory_anchor).map(|p| p.key(target));
                if selected.is_null() {
                    selected = candidate;
                    proximity = key;
                } else if let (Some(key), Some(old_key)) = (key, proximity)
                    && key < old_key
                    && registry::dispatch_rank(candidate) <= registry::dispatch_rank(selected)
                    && (*c).ready_order <= (*registry::core(selected)).ready_order
                {
                    // Nearness cannot replace older/stronger work with a
                    // younger or weaker task. Keep every original age/rank.
                    selected = candidate;
                    proximity = Some(key);
                }
                // Unknown/one-node behavior is the original first-eligible
                // fast path. A confirmed local anchor cannot improve further.
                if proximity.is_none() || proximity == Some(0) {
                    break;
                }
            }
            candidate = next;
        }
        if !selected.is_null() {
            let c = registry::core(selected);
            let stamp = *time.get_or_insert_with(&mut now);
            registry::unlink_ready(selected);
            (*c).home_cpu = target as u8;
            (*c).home_cpu_bound = true;
            let exception = super::migration::cooling(selected, stamp, cooldown);
            super::migration::record(selected, stamp, exception);
            // Saved frames, exact owners, ready_order/time and budgets stay.
            registry::append_ready(selected);
            registry::bump_epoch();
            return selected;
        }
    }
    ptr::null_mut()
}
/// # Safety
/// The exact successful transfer remains resident under the same runtime
/// admission. This receipt records origin only; it never admits migration.
pub(crate) unsafe fn mark_periodic(task: *mut Task) {
    unsafe {
        (*registry::core(task)).last_migration_periodic = true;
    }
}
