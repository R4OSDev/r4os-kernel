//! Shared bounded CPU preference. Admission/frames/queues stay with the task
//! owner. Optional copied rank hints follow static class/NUMA, before locality.
use crate::{
    arch::smp_policy::MAX_CPUS,
    platform::{cpu_topology::Domain, numa::Preference},
};

#[derive(Clone, Copy)]
pub(crate) struct Ranks {
    pub values: [u8; MAX_CPUS],
}
impl Ranks {
    pub(crate) fn key(self, cpu: u32) -> u8 {
        u8::MAX - self.values[cpu as usize]
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Context {
    pub memory: Option<Preference>,
    pub ranks: Option<Ranks>,
}
pub(crate) fn better_targets_for_context(
    mask: u64,
    domains: &[Domain; MAX_CPUS],
    old: u32,
    efficient: bool,
    context: Context,
) -> u64 {
    let Some(ranks) = context.ranks.filter(|_| !efficient) else {
        return better_targets(mask, domains, old, efficient, context.memory);
    };
    let mask = mask & crate::arch::smp_policy::CPU_MASK;
    if old as usize >= MAX_CPUS || mask & (1 << old) == 0 {
        return 0;
    }
    let active = capacity_available(mask, domains);
    let key = |cpu: u32| {
        (
            capacity_key(domains[cpu as usize], active, efficient),
            context.memory.map_or(0, |p| p.key(cpu)),
            ranks.key(cpu),
        )
    };
    let old_key = key(old);
    let mut remaining = mask;
    let mut better = 0;
    while remaining != 0 {
        let cpu = remaining.trailing_zeros();
        remaining &= remaining - 1;
        if key(cpu) < old_key {
            better |= 1 << cpu;
        }
    }
    better
}

pub(crate) fn choose(
    mask: u64,
    loads: &[usize; MAX_CPUS],
    domains: &[Domain; MAX_CPUS],
    previous: Option<u32>,
) -> Option<u32> {
    choose_with_preference(mask, loads, domains, previous, None, false)
}

// A static ordering is comparable only inside its proven package/type scope.
// Unknown or mixed scopes retain the old flat preference, not a low capacity.
fn capacity_available(mask: u64, domains: &[Domain; MAX_CPUS]) -> bool {
    if mask == 0 {
        return false;
    }
    let scope = domains[mask.trailing_zeros() as usize].capacity_scope;
    scope != 0 && mask & !scope == 0
}
fn capacity_key(domain: Domain, active: bool, efficient: bool) -> u16 {
    if !active {
        0
    } else if efficient {
        domain.capacity
    } else {
        u16::MAX - domain.capacity
    }
}
pub(crate) fn better_targets(
    mask: u64,
    domains: &[Domain; MAX_CPUS],
    old: u32,
    efficient: bool,
    memory: Option<Preference>,
) -> u64 {
    let mask = mask & crate::arch::smp_policy::CPU_MASK;
    if old as usize >= MAX_CPUS || mask & (1 << old) == 0 {
        return 0;
    }
    let active = capacity_available(mask, domains);
    if memory.is_none() {
        if !active {
            return 0;
        }
        let d = domains[old as usize];
        return mask & if efficient { d.slower } else { d.faster };
    }
    let old_key = (
        capacity_key(domains[old as usize], active, efficient),
        memory.map_or(0, |p| p.key(old)),
    );
    let mut remaining = mask;
    let mut better = 0;
    while remaining != 0 {
        let cpu = remaining.trailing_zeros();
        remaining &= remaining - 1;
        let key = (
            capacity_key(domains[cpu as usize], active, efficient),
            memory.map_or(0, |p| p.key(cpu)),
        );
        if key < old_key {
            better |= 1 << cpu;
        }
    }
    better
}

pub(crate) fn choose_with_preference(
    mask: u64,
    loads: &[usize; MAX_CPUS],
    domains: &[Domain; MAX_CPUS],
    previous: Option<u32>,
    preferred: Option<u32>,
    efficient: bool,
) -> Option<u32> {
    choose_for_memory(mask, loads, domains, previous, preferred, efficient, None)
}
pub(crate) fn choose_for_memory(
    mask: u64,
    loads: &[usize; MAX_CPUS],
    domains: &[Domain; MAX_CPUS],
    previous: Option<u32>,
    preferred: Option<u32>,
    efficient: bool,
    memory: Option<Preference>,
) -> Option<u32> {
    choose_for_context(
        mask,
        loads,
        domains,
        previous,
        preferred,
        efficient,
        Context {
            memory,
            ranks: None,
        },
    )
}
pub(crate) fn choose_for_context(
    mask: u64,
    loads: &[usize; MAX_CPUS],
    domains: &[Domain; MAX_CPUS],
    previous: Option<u32>,
    preferred: Option<u32>,
    efficient: bool,
    context: Context,
) -> Option<u32> {
    let memory = context.memory;
    let ranks = context.ranks.filter(|_| !efficient);
    let mut candidates = mask & crate::arch::smp_policy::CPU_MASK;
    if candidates == 0 {
        return None;
    }
    let capacity_active = capacity_available(candidates, domains);
    let busy = loads
        .iter()
        .enumerate()
        .fold(0u64, |mask, (i, &load)| mask | (u64::from(load != 0) << i));
    let previous = previous.filter(|&cpu| cpu < MAX_CPUS as u32);
    let mut best = None;
    let mut best_key = (
        usize::MAX,
        true,
        true,
        u16::MAX,
        u16::MAX,
        u8::MAX,
        3u8,
        u32::MAX,
    );
    while candidates != 0 {
        let cpu = candidates.trailing_zeros();
        candidates &= candidates - 1;
        let d = domains[cpu as usize];
        // Unknown domains retain flat behavior; they never fabricate siblings.
        let occupied_core = loads[cpu as usize] != 0 || d.core & busy != 0;
        let locality = previous.map_or(2, |old| {
            if cpu == old {
                0
            } else if domains[old as usize].cache & (1 << cpu) != 0 {
                1
            } else {
                2
            }
        });
        let key = (
            loads[cpu as usize],
            occupied_core,
            preferred.is_some_and(|hint| hint != cpu),
            capacity_key(d, capacity_active, efficient),
            memory.map_or(0, |p| p.key(cpu)),
            ranks.map_or(0, |p| p.key(cpu)),
            locality,
            cpu,
        );
        if key < best_key {
            best_key = key;
            best = Some(cpu);
        }
    }
    // One queued task of prior-CPU slack is permitted only when every
    // admissible CPU already has work, never at the cost of a truly idle CPU
    // or a better physical-core/static work-class choice. No task can be hidden
    // by a preference.
    if best_key.0 != 0
        && preferred.is_none()
        && let Some(old) = previous
        && mask & (1 << old) != 0
    {
        let load = loads[old as usize];
        let occupied = load != 0 || domains[old as usize].core & busy != 0;
        if load <= best_key.0.saturating_add(1)
            && occupied <= best_key.1
            && (
                capacity_key(domains[old as usize], capacity_active, efficient),
                memory.map_or(0, |p| p.key(old)),
                ranks.map_or(0, |p| p.key(old)),
            ) <= (best_key.3, best_key.4, best_key.5)
        {
            return Some(old);
        }
    }
    best
}
