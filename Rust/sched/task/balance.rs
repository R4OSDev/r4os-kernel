//! Bounded Ready-queue transfer under the existing scheduler runtime owner.
//! No running stack, allocation, wait, callback or public affinity API.
use super::{
    task_registry as registry,
    task_types::{ExecutionOwnerKind, State, Task},
};
use crate::arch::percpu;
use core::ptr;
const CANDIDATES: usize = 8;

/// # Safety
/// Caller holds runtime admission and supplies the source CPU's exact current
/// task. Even a logically Ready current task still owns its physical stack.
pub(crate) unsafe fn pull(
    source: u32,
    target: u32,
    current: *mut Task,
    now: u64,
    cooldown: u64,
) -> *mut Task {
    if source == target
        || source as usize >= registry::MAX_CPUS
        || target as usize >= registry::MAX_CPUS
    {
        return ptr::null_mut();
    }
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    if online & (1 << source) == 0 || online & (1 << target) == 0 {
        return ptr::null_mut();
    }
    unsafe {
        let from = registry::queue(source);
        if (*registry::queue(target)).count != 0 || (*from).count < 2 {
            return ptr::null_mut();
        }
        let mut candidate = if (*from).balance_cursor.is_null() {
            (*from).head
        } else {
            (*from).balance_cursor
        };
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
            let eligible = candidate != current
                && (*c).state == State::Ready
                && (*c).ready_linked
                && u32::from((*c).home_cpu) == source
                && (*c).running_cpu == 0xff
                && (*c).smp_eligible
                && !(*c).cpu_idle
                && !(*c).cpu_affinity_bound
                && matches!(
                    (*c).execution_owner_kind,
                    ExecutionOwnerKind::None | ExecutionOwnerKind::ProgramThread
                )
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
                && ((*c).last_migration_tick == 0
                    || now.saturating_sub((*c).last_migration_tick) >= cooldown);
            if eligible {
                registry::unlink_ready(candidate);
                (*c).home_cpu = target as u8;
                (*c).home_cpu_bound = true;
                (*c).last_migration_tick = now.max(1);
                // Existing ready_order/time, role budgets, task generation,
                // saved RSP/FPU and exact unwind counter are not re-created.
                registry::append_ready(candidate);
                registry::bump_epoch();
                return candidate;
            }
            candidate = next;
        }
    }
    ptr::null_mut()
}
