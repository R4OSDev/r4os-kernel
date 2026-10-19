//! Native thread policy. The caller pins the exact Thread; registry admission
//! resolves its immutable task identity. No allocation, caller buffer, foreign
//! callback or registry lock spans these scheduler mutations.
use super::{
    balance, home, task_registry as registry,
    task_types::{ExecutionOwnerKind, Role, State, Task},
};
use crate::{
    arch::{percpu, runtime},
    contract as c,
    sched::scheduler,
};
use core::ffi::c_void;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Policy {
    pub cpus: u64, // Private scheduler representation, not the public ABI.
    pub preferred: Option<u32>,
    pub class: u32,
}
#[derive(Clone, Copy)]
pub(crate) struct Identity {
    pub id: u32,
    pub generation: u64,
    pub thread: *mut c_void,
}

const _: () = assert!(registry::MAX_CPUS <= 64);

pub(crate) fn word_count() -> u32 {
    // Configured dense slots, including failed qualification, define numbering.
    let configured = crate::platform::cpu_topology::count();
    configured.max(1).div_ceil(64)
}
unsafe fn admitted(task: *mut Task) -> u64 {
    unsafe {
        let core = registry::core(task);
        let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        let module = if (*core).smp_eligible {
            online
        } else {
            online & 1
        };
        if (*core).cpu_affinity_bound {
            module & (1 << (*core).home_cpu)
        } else {
            module
        }
    }
}
pub(crate) unsafe fn allowed(task: *mut Task) -> u64 {
    unsafe {
        let p = (*registry::core(task)).placement_applied;
        admitted(task) & if p.cpus == 0 { u64::MAX } else { p.cpus }
    }
}
pub(crate) unsafe fn allows(task: *mut Task, cpu: u32) -> bool {
    cpu < registry::MAX_CPUS as u32 && unsafe { allowed(task) & (1 << cpu) != 0 }
}
pub(crate) unsafe fn pending(task: *mut Task) -> bool {
    !task.is_null()
        && unsafe {
            let core = registry::core(task);
            (*core).placement_revision != (*core).placement_applied_revision
        }
}
unsafe fn physical_cpu(task: *mut Task) -> Option<u32> {
    (0..registry::MAX_CPUS as u32)
        .find(|&cpu| unsafe { scheduler::current_on_cpu(cpu).cast::<Task>() == task })
}
unsafe fn resolve(identity: Identity) -> Result<*mut Task, i32> {
    unsafe {
        let task = registry::find_identity(identity.id, identity.generation);
        if task.is_null() {
            return Err(c::thread_error_not_found);
        }
        let core = registry::core(task);
        if (*core).execution_owner_kind != ExecutionOwnerKind::ProgramThread
            || (*core).execution_owner_context != identity.thread
            || !matches!(
                (*core).state,
                State::Ready | State::Running | State::Blocked
            )
            || (*core).retire_pending
            || (*core).release_in_progress
            || (*core).rollback_pending
        {
            return Err(c::thread_error_not_found);
        }
        Ok(task)
    }
}
pub(crate) fn get(identity: Identity) -> Result<(c::ThreadPlacementInfo, u64), i32> {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe {
        let task = resolve(identity)?;
        let core = registry::core(task);
        let p = (*core).placement_requested;
        let flags = if p.cpus == 0 {
            c::thread_placement_flag_default_cpus
        } else {
            0
        } | if pending(task) {
            c::thread_placement_flag_pending
        } else {
            0
        } | if (*core).smp_eligible {
            c::thread_placement_flag_parallel
        } else {
            0
        } | if (*core).cpu_affinity_bound {
            c::thread_placement_flag_internal_fixed
        } else {
            0
        };
        Ok((
            c::ThreadPlacementInfo {
                flags,
                work_class: p.class,
                preferred_cpu: p.preferred.unwrap_or(c::thread_placement_cpu_none),
                home_cpu: u32::from((*core).home_cpu),
                running_cpu: physical_cpu(task).unwrap_or(c::thread_placement_cpu_none),
                cpu_word_count: word_count(),
                request_revision: (*core).placement_revision,
                applied_revision: (*core).placement_applied_revision,
                ..Default::default()
            },
            if p.cpus == 0 { admitted(task) } else { p.cpus },
        ))
    }
}
pub(crate) fn set(identity: Identity, policy: Policy) -> i32 {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe {
        let task = match resolve(identity) {
            Ok(task) => task,
            Err(error) => return error,
        };
        let core = registry::core(task);
        let admission = admitted(task);
        if admission == 0 {
            return c::thread_error_busy;
        }
        if policy.class > c::thread_placement_class_preferred
            || policy.cpus & !admission != 0
            || policy.preferred.is_some_and(|cpu| {
                cpu >= registry::MAX_CPUS as u32
                    || (if policy.cpus == 0 {
                        admission
                    } else {
                        policy.cpus
                    }) & (1 << cpu)
                        == 0
            })
        {
            return c::thread_error_invalid;
        }
        if policy == (*core).placement_requested {
            return c::thread_ok;
        }
        let Some(revision) = (*core).placement_revision.checked_add(1) else {
            return c::thread_error_busy;
        };
        let physical = physical_cpu(task);
        // A saved Ready continuation can move only through the same full
        // frame/FPU/resource predicates as normal balancing. Blocked waits keep
        // their exact queue/timeout leases and acquire a home when they wake.
        if physical.is_none()
            && (((*core).state == State::Ready && !balance::saved_continuation(task, false))
                || (*core).held_lock_count != 0
                || (*core).preempt_disable_depth != 0
                || (*core).wait_handoff_guard_pending
                || (*core).rsp == 0
                || ((*core).uses_fpu && (!(*core).fpu_state_valid || (*core).fpu_state.is_none())))
        {
            return c::thread_error_busy;
        }
        (*core).placement_requested = policy;
        (*core).placement_revision = revision;
        if let Some(cpu) = physical {
            scheduler::request_placement_switch(cpu);
        } else {
            apply(task);
            if (*core).state == State::Ready {
                scheduler::notify_ready(task.cast());
            }
        }
        registry::bump_epoch();
        c::thread_ok
    }
}

/// Called under runtime admission after the outgoing RSP and FPU save. The
/// exact old descriptor is still resident; no current CPU may own its stack.
unsafe fn apply(task: *mut Task) {
    unsafe {
        let core = registry::core(task);
        let linked = (*core).ready_linked;
        if linked {
            registry::unlink_ready(task);
        }
        let p = (*core).placement_requested;
        let old_role = (*core).role;
        if old_role == Role::ShortCompletion {
            (*core).placement_preferred_ticks = (*core).role_budget_ticks_remaining;
            (*core).placement_preferred_dispatches = (*core).role_budget_dispatches_remaining;
        }
        let role = match p.class {
            1 => Role::Batch,
            2 => Role::ShortCompletion,
            _ => Role::Interactive,
        };
        if role != old_role {
            registry::configure_role(task, role);
            if role == Role::ShortCompletion {
                (*core).role_budget_ticks_remaining = (*core)
                    .role_budget_ticks_remaining
                    .min((*core).placement_preferred_ticks);
                (*core).role_budget_dispatches_remaining = (*core)
                    .role_budget_dispatches_remaining
                    .min((*core).placement_preferred_dispatches);
                // Consumption records exhaustion at the first depleted limit.
                // Re-entering this class is not a new activation/receipt.
                (*core).role_budget_exhausted_recorded = (*core).role_budget_ticks_remaining == 0
                    || (*core).role_budget_dispatches_remaining == 0;
            }
        }
        (*core).placement_applied = p;
        (*core).placement_applied_revision = (*core).placement_revision;
        (*core).placement_deferred = false;
        if (*core).state == State::Blocked && !allows(task, u32::from((*core).home_cpu)) {
            // The saved wait/timeout nodes belong to the exact Task, not to a
            // CPU. Retain every lease; normal wake chooses within this set.
            (*core).home_cpu = allowed(task).trailing_zeros() as u8;
            (*core).home_cpu_bound = true;
            (*core).last_migration_tick = 0;
        }
        if (*core).state == State::Ready {
            // A changed hard set overrides the migration cooldown. Home select
            // retains physical-current and saved-state guards after publication.
            home::select(task);
            if !allows(task, u32::from((*core).home_cpu)) {
                // Explicit policy application has already passed the saved
                // continuation predicate; a cold allowed slot is always valid.
                (*core).home_cpu = allowed(task).trailing_zeros() as u8;
                (*core).home_cpu_bound = true;
            }
            if linked || (*core).ready_order != 0 {
                registry::append_ready(task);
            } else {
                registry::link_ready(task);
            }
        }
    }
}
/// Remove a pending physical current from selection, preserving its exact FIFO
/// age. It remains private under the held runtime owner until finish_switch.
pub(crate) unsafe fn stage(task: *mut Task, depth: u32) -> bool {
    unsafe { stage_impl(task, depth, false) }
}
/// The same physical saved-context handoff can drain a sole task to its idle
/// continuation without inventing a public policy revision or a new budget.
pub(crate) unsafe fn stage_cpu_drain(task: *mut Task, depth: u32) -> bool {
    if task.is_null() {
        return false;
    }
    unsafe {
        let core = registry::core(task);
        if !(*core).smp_eligible
            || (*core).cpu_affinity_bound
            || (*core).cpu_idle
            || !matches!(
                (*core).execution_owner_kind,
                ExecutionOwnerKind::None | ExecutionOwnerKind::ProgramThread
            )
            || allowed(task) == 0
        {
            return false;
        }
        stage_impl(task, depth, true)
    }
}
unsafe fn stage_impl(task: *mut Task, depth: u32, drain: bool) -> bool {
    unsafe {
        if !drain && !pending(task) {
            return false;
        }
        let core = registry::core(task);
        if matches!((*core).state, State::Unused | State::Dead)
            || (*core).held_lock_count != 0
            || (*core).preempt_disable_depth > depth
            || (*core).wait_handoff_guard_pending
        {
            return false;
        }
        if (*core).state == State::Ready && (*core).ready_linked {
            registry::unlink_ready(task);
        } else {
            (*core).ready_order = 0;
        }
        (*core).placement_deferred = true;
        true
    }
}
pub(crate) unsafe fn finish_switch(task: *mut Task) {
    unsafe {
        if task.is_null() {
            return;
        }
        let core = registry::core(task);
        assert!((*core).placement_deferred && physical_cpu(task).is_none());
        if matches!((*core).state, State::Unused | State::Dead) {
            (*core).placement_deferred = false;
            return;
        }
        assert!((*core).rsp != 0 && (!(*core).uses_fpu || (*core).fpu_state_valid));
        let old_home = u32::from((*core).home_cpu);
        if pending(task) {
            apply(task);
        } else {
            // CPU withdrawal is a physical save/rehome, not a new class
            // request. In particular kernel roles need not equal Policy's
            // default class, and their consumed budgets must survive.
            (*core).placement_deferred = false;
            if (*core).state == State::Ready {
                home::select(task);
                if !allows(task, u32::from((*core).home_cpu)) {
                    (*core).home_cpu = allowed(task).trailing_zeros() as u8;
                    (*core).home_cpu_bound = true;
                }
                if (*core).ready_order != 0 {
                    registry::append_ready(task);
                } else {
                    registry::link_ready(task);
                }
            }
        }
        if (*core).state == State::Ready {
            if u32::from((*core).home_cpu) != old_home {
                crate::arch::cpu_lifecycle::note_migration(old_home);
            }
            scheduler::notify_ready(task.cast());
        }
    }
}
