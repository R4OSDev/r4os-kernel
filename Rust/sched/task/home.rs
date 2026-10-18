//! Initial/wake home selection only for an unlinked saved continuation.
//! Exact physical-current and resource barriers precede topology preferences.
use super::{balance, task_registry as registry, task_types::Task};
use crate::{
    arch::percpu,
    platform::cpu_topology,
    sched::{placement, scheduler},
};

pub(super) unsafe fn select(task: *mut Task) {
    unsafe {
        let c = registry::core(task);
        if !(*c).smp_eligible {
            (*c).home_cpu = 0;
            (*c).home_cpu_bound = true;
            return;
        }
        if (*c).cpu_affinity_bound || (*c).ready_linked {
            return;
        }
        let initial =
            !(*c).home_cpu_bound && (*c).switches_in == 0 && (*c).last_scheduled_tick == 0;
        if !balance::placeable(task, initial) {
            return;
        }
        let old = u32::from((*c).home_cpu);
        // mark_ready/switched_out may already have cleared running_cpu while
        // the outgoing RSP has not been saved. The exact current still owns it.
        let old_current = scheduler::current_on_cpu(old).cast::<Task>();
        if old_current == task {
            return;
        }
        let mask = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        let previous = ((*c).home_cpu_bound && old < registry::MAX_CPUS as u32).then_some(old);
        let now = (*c).ready_since_tick;
        if previous.is_some()
            && mask & (1 << old) != 0
            && (*c).last_migration_tick != 0
            && now.saturating_sub((*c).last_migration_tick) < scheduler::PLACEMENT_COOLDOWN
        {
            return;
        }
        if previous.is_some() && mask & (1 << old) != 0 {
            let old_load = balance::workload(old, old_current);
            // All admitted exact currents have work: every possible load is
            // at least one and every core occupied, including unknown domains.
            // A prior CPU with one unit is already optimal in all chooser keys.
            if old_load == 1 && scheduler::all_current_have_work(mask) {
                return;
            }
            // Zero load on the prior CPU with no busy sibling is already
            // minimal load, best core class and best possible locality. No
            // other CPU can improve that exact chooser key. Check only its
            // confirmed siblings, retaining flat behavior for unknown domains.
            if old_load == 0 {
                let mut siblings = cpu_topology::domains()[old as usize].core & mask & !(1 << old);
                let mut core_busy = false;
                while siblings != 0 {
                    let cpu = siblings.trailing_zeros();
                    siblings &= siblings - 1;
                    if balance::workload(cpu, scheduler::current_on_cpu(cpu).cast()) != 0 {
                        core_busy = true;
                        break;
                    }
                }
                if !core_busy {
                    return;
                }
            }
        }
        let mut loads = [0usize; registry::MAX_CPUS];
        for (cpu, load) in loads.iter_mut().enumerate() {
            if mask & (1 << cpu) == 0 {
                continue;
            }
            let current = scheduler::current_on_cpu(cpu as u32).cast::<Task>();
            if current == task {
                return;
            }
            *load = balance::workload(cpu as u32, current);
        }
        if let Some(target) = placement::choose(mask, &loads, cpu_topology::domains(), previous) {
            if previous.is_some() && target != old {
                (*c).last_migration_tick = now.max(1);
            }
            (*c).home_cpu = target as u8;
            (*c).home_cpu_bound = true;
        }
    }
}
