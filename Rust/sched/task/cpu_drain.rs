//! Explicit bounded CPU withdrawal, under the existing runtime owner. The
//! normal physical-current/RSP/FPU and exact wait/ready owners remain intact.
use super::{
    balance, home, task_registry as registry,
    task_types::{ExecutionOwnerKind, State, Task},
    thread_placement,
};
use crate::{
    arch::{cpu_lifecycle::Reason, percpu},
    platform::{cpu_topology, numa},
    sched::{placement, scheduler},
};
const MAX_EXAMINED: usize = 4096;

/// A controller may sleep for real worker retirement. Admission pins its
/// lifetime, while its existing immutable BSP eligibility/fixed binding must
/// also keep execution on CPU0. A merely momentary CPU0 observation is unsafe.
pub(crate) fn controller_stays_bsp() -> bool {
    let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
    if percpu::current_index() != 0 {
        return false;
    }
    let task = unsafe { scheduler::current_on_cpu(0) }.cast::<Task>();
    if task.is_null() {
        return false;
    }
    unsafe {
        let c = registry::core(task);
        (*c).state == State::Running
            && (*c).unwind_guard_count != 0
            && (!(*c).smp_eligible || (*c).cpu_affinity_bound && (*c).home_cpu == 0)
            && !(*c).retire_pending
            && !(*c).release_in_progress
            && !(*c).rollback_pending
            && [(*c).placement_requested, (*c).placement_applied]
                .iter()
                .all(|p| p.cpus == 0 || p.cpus & 1 != 0)
    }
}

unsafe fn affected(task: *mut Task, cpu: u32) -> bool {
    unsafe {
        let c = registry::core(task);
        u32::from((*c).home_cpu) == cpu
            || (*c).running_cpu == cpu as u8
            || scheduler::current_on_cpu(cpu).cast::<Task>() == task
    }
}
unsafe fn safe_saved(task: *mut Task) -> bool {
    unsafe {
        let c = registry::core(task);
        balance::execution_owner_movable(task)
            && (*c).running_cpu == 0xff
            && !(*c).placement_deferred
            && !(*c).retire_pending
            && !(*c).release_in_progress
            && !(*c).rollback_pending
            && (*c).held_lock_count == 0
            && (*c).preempt_disable_depth == 0
            && (!(*c).wait_handoff_guard_pending && (*c).guard_offer.is_empty())
            && (*c).rsp != 0
            && (!(*c).uses_fpu || ((*c).fpu_state_valid && (*c).fpu_state.is_some()))
    }
}
/// # Safety
/// Runtime admission pins the complete registry and each exact physical current.
pub(crate) unsafe fn preflight(cpu: u32, remaining: u64) -> Result<(), Reason> {
    if cpu == 0 || remaining == 0 {
        return Err(Reason::Fixed);
    }
    unsafe {
        let mut task = (*registry::raw()).head;
        let mut examined = 0;
        while !task.is_null() {
            if examined == MAX_EXAMINED {
                return Err(Reason::TaskBudget);
            }
            examined += 1;
            let c = registry::core(task);
            if !(*c).cpu_idle
                && matches!((*c).state, State::Ready | State::Running | State::Blocked)
            {
                // A pending new hard set must not become impossible after its
                // old physical continuation finishes saving on another CPU.
                for p in [(*c).placement_applied, (*c).placement_requested] {
                    if p.cpus != 0 && p.cpus & remaining == 0 {
                        return Err(Reason::Fixed);
                    }
                }
                if affected(task, cpu) {
                    if (*c).cpu_affinity_bound || !(*c).smp_eligible {
                        return Err(Reason::Fixed);
                    }
                    if !balance::execution_owner_movable(task)
                        || (*c).execution_owner_kind == ExecutionOwnerKind::AsyncIo
                            && scheduler::current_on_cpu(cpu).cast::<Task>() == task
                    {
                        return Err(Reason::KernelContext);
                    }
                    if (*c).held_lock_count != 0
                        || (*c).preempt_disable_depth != 0
                        || ((*c).wait_handoff_guard_pending || !(*c).guard_offer.is_empty())
                        || (*c).retire_pending
                        || (*c).release_in_progress
                        || (*c).rollback_pending
                    {
                        return Err(Reason::Resources);
                    }
                    if scheduler::current_on_cpu(cpu).cast::<Task>() != task
                        && ((*c).state == State::Ready && !balance::saved_continuation(task, false)
                            || (*c).state == State::Blocked && !safe_saved(task))
                    {
                        return Err(Reason::Resources);
                    }
                }
            }
            task = (*c).registry_next;
        }
    }
    Ok(())
}
/// # Safety
/// Same runtime/registry lifetime as preflight, after scheduling admission has
/// closed. Physical currents remain on their original stack until normal save.
pub(crate) unsafe fn move_saved(cpu: u32) -> Result<u32, Reason> {
    unsafe {
        let mut task = (*registry::raw()).head;
        let mut examined = 0;
        let mut moved = 0;
        while !task.is_null() {
            if examined == MAX_EXAMINED {
                return Err(Reason::TaskBudget);
            }
            examined += 1;
            let c = registry::core(task);
            let next = (*c).registry_next;
            if !(*c).cpu_idle
                && u32::from((*c).home_cpu) == cpu
                && matches!((*c).state, State::Ready | State::Blocked)
                && scheduler::current_on_cpu(cpu).cast::<Task>() != task
            {
                let mask = thread_placement::allowed(task);
                if mask == 0 || !safe_saved(task) {
                    return Err(Reason::Resources);
                }
                if (*c).state == State::Ready {
                    if !balance::saved_continuation(task, false) {
                        return Err(Reason::Resources);
                    }
                    let linked = (*c).ready_linked;
                    if linked {
                        registry::unlink_ready(task);
                    }
                    home::select(task);
                    if (*c).home_cpu as u32 == cpu || mask & (1u64 << (*c).home_cpu) == 0 {
                        if linked {
                            registry::append_ready(task);
                        }
                        return Err(Reason::Resources);
                    }
                    if linked {
                        registry::append_ready(task);
                    }
                    scheduler::notify_ready(task.cast());
                } else {
                    let mut loads = [0usize; registry::MAX_CPUS];
                    for (index, load) in loads.iter_mut().enumerate() {
                        if mask & (1u64 << index) != 0 {
                            *load = balance::workload(
                                index as u32,
                                scheduler::current_on_cpu(index as u32).cast(),
                            );
                        }
                    }
                    let target = placement::choose_for_context(
                        mask,
                        &loads,
                        cpu_topology::domains(),
                        None,
                        (*c).placement_applied.preferred,
                        (*c).role == super::task_types::Role::Batch,
                        placement::Context {
                            memory: numa::preference(mask, (*c).memory_anchor),
                            ranks: crate::platform::cpu_power::hints::ranks(
                                mask,
                                (*c).role == super::task_types::Role::Batch,
                            ),
                        },
                    )
                    .ok_or(Reason::Resources)?;
                    // Wait node, timeout order, object, saved frame, FPU buffer
                    // and every generation/lease remain with this exact Task.
                    (*c).home_cpu = target as u8;
                    (*c).home_cpu_bound = true;
                }
                registry::bump_epoch();
                moved += 1;
            }
            task = next;
        }
        if (*registry::queue(cpu)).count != 0 {
            return Err(Reason::Resources);
        }
        Ok(moved)
    }
}

/// # Safety
/// Runtime admission, running on the exact AP's permanent idle continuation.
pub(crate) unsafe fn idle_receipt(cpu: u32) -> Option<u64> {
    if cpu != percpu::current_index() {
        return None;
    }
    let current = unsafe { scheduler::current_on_cpu(cpu) }.cast::<Task>();
    if current.is_null() {
        return None;
    }
    unsafe {
        let c = registry::core(current);
        if !(*c).cpu_idle
            || (*c).state != State::Running
            || (*c).uses_fpu
            || (*c).held_lock_count != 0
            || (*c).preempt_disable_depth != 0
            || (*c).unwind_guard_count != 0
            || ((*c).wait_handoff_guard_pending || !(*c).guard_offer.is_empty())
            || (*c).placement_deferred
        {
            return None;
        }
        Some((*c).switches_in)
    }
}
/// # Safety
/// Only the owned native lifecycle worker calls this from its own pinned,
/// FPU-ready running Task. It relaxes its diagnostic binding so the real
/// nonpreemptible-kernel Busy path can be tested without forcing a migration.
pub(crate) unsafe fn diagnostic_release_current_binding() -> bool {
    let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
    let task = unsafe { scheduler::current_on_cpu(percpu::current_index()) }.cast::<Task>();
    if task.is_null() {
        return false;
    }
    unsafe {
        let c = registry::core(task);
        if !(*c).cpu_affinity_bound
            || !(*c).smp_eligible
            || (*c).execution_owner_kind != ExecutionOwnerKind::None
            || (*c).state != State::Running
            || (*c).pin_count == 0
        {
            return false;
        }
        (*c).cpu_affinity_bound = false;
    }
    true
}
