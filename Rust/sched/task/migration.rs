//! Shared cooldown exception for saved wake placement and Ready transfer.
//! Runtime owns every fact. Frame/lock/wait/lifetime gates stay at the callers.
use super::{
    task_registry as registry,
    task_types::{Role, Task},
    thread_placement,
};
use crate::{
    arch::percpu,
    platform::cpu_topology,
    sched::{
        migration_policy::{self, Budget},
        scheduler,
    },
};

pub(super) unsafe fn cooling(task: *mut Task, now: u64, cooldown: u64) -> bool {
    migration_policy::cooling(
        unsafe { (*registry::core(task)).last_migration_tick },
        now,
        cooldown,
    )
}
pub(super) unsafe fn available(task: *mut Task, now: u64, cooldown: u64) -> bool {
    let c = unsafe { &*registry::core(task) };
    Budget {
        last_move: c.last_migration_tick,
        last_exception: c.last_migration_exception_tick,
        latency_role: matches!(c.role, Role::Interactive | Role::ShortCompletion),
        boost_budget: c.role == Role::ShortCompletion,
        ticks: c.role_budget_ticks_remaining,
        dispatches: c.role_budget_dispatches_remaining,
    }
    .available(now, cooldown)
}
unsafe fn competing(task: *mut Task, source: u32) -> bool {
    let c = unsafe { &*registry::core(task) };
    let queued_self = c.ready_linked && u32::from(c.home_cpu) == source;
    let current = unsafe { scheduler::current_on_cpu(source) }.cast::<Task>();
    unsafe {
        (*registry::queue(source)).count > usize::from(queued_self)
            || !current.is_null() && current != task && !(*registry::core(current)).cpu_idle
    }
}
pub(super) unsafe fn targets(task: *mut Task, source: u32, now: u64, cooldown: u64) -> u64 {
    if !unsafe { available(task, now, cooldown) && competing(task, source) } {
        return 0;
    }
    let allowed = unsafe { thread_placement::allowed(task) } & !(1 << source);
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let mut possible = allowed;
    let mut result = 0;
    while possible != 0 {
        let target = possible.trailing_zeros();
        possible &= possible - 1;
        if free(target, allowed, online) {
            result |= 1 << target;
        }
    }
    result
}
fn free(target: u32, allowed: u64, online: u64) -> bool {
    migration_policy::free_core(
        target,
        allowed,
        online,
        cpu_topology::domains()[target as usize].core,
        |cpu| {
            let current = unsafe { scheduler::current_on_cpu(cpu) }.cast::<Task>();
            // A physically departing Blocked/Ready stack is not idle.
            !current.is_null()
                && unsafe {
                    (*registry::core(current)).cpu_idle && (*registry::queue(cpu)).count == 0
                }
        },
    )
}
pub(super) unsafe fn can_move(
    task: *mut Task,
    source: u32,
    target: u32,
    now: u64,
    cooldown: u64,
) -> bool {
    if source == target || !unsafe { available(task, now, cooldown) && competing(task, source) } {
        return false;
    }
    free(
        target,
        unsafe { thread_placement::allowed(task) },
        percpu::r4os_rust_percpu_control(14, 0, 0, 0),
    )
}
pub(super) unsafe fn record(task: *mut Task, now: u64, exception: bool) {
    let c = unsafe { &mut *registry::core(task) };
    c.last_migration_tick = now.max(1);
    c.last_migration_periodic = false;
    if exception {
        c.last_migration_exception_tick = now.max(1);
    }
}
