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
            && (initial
                || matches!(
                    (*c).execution_owner_kind,
                    ExecutionOwnerKind::None | ExecutionOwnerKind::ProgramThread
                ))
            && !(*c).retire_pending
            && !(*c).release_in_progress
            && !(*c).rollback_pending
            && (*c).preempt_disable_depth == 0
            && (*c).held_lock_count == 0
            && !(*c).wait_handoff_guard_pending
            && (*c).wait_node.queue.is_null()
            && !(*c).timeout_linked
            && (*c).wake_tick == 0
            && (*c).rsp != 0
            && (!(*c).uses_fpu || ((*c).fpu_state_valid && (*c).fpu_state.is_some()))
    }
}
pub(crate) unsafe fn relocatable(candidate: *mut Task) -> bool {
    unsafe { placeable(candidate, false) }
}

unsafe fn eligible(
    candidate: *mut Task,
    current: *mut Task,
    source: u32,
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
            && ((*c).last_migration_tick == 0
                || now().saturating_sub((*c).last_migration_tick) >= cooldown)
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
        (*from).count == 1 && !eligible((*from).head, current, source, now, cooldown)
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

/// # Safety
/// Caller holds runtime admission and supplies the source CPU's exact current
/// task. Even a logically Ready current task still owns its physical stack.
pub(crate) unsafe fn pull(
    source: u32,
    target: u32,
    current: *mut Task,
    target_current: *mut Task,
    mut now: impl FnMut() -> u64,
    cooldown: u64,
) -> *mut Task {
    if !unsafe { imbalanced(source, target, current, target_current) } {
        return ptr::null_mut();
    }
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
            if candidate != target_current
                && super::thread_placement::allows(candidate, target)
                && eligible(
                    candidate,
                    current,
                    source,
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
            (*c).last_migration_tick = stamp.max(1);
            (*c).last_migration_periodic = false;
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
