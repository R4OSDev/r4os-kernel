//! Cached optional ordering in the canonical CPU catalogue. No MSR/callback,
//! worker, static Domain mutation or lifetime pin belongs to these read-only rows.
use super::{STATE, owner_identity};
use crate::{
    arch::{percpu, runtime::RuntimeGuard, smp_policy::MAX_CPUS},
    contract as c,
    platform::{cpu_capabilities, cpu_topology, monotonic},
    sched::task_context::Admission,
};
use core::{
    cell::UnsafeCell,
    mem,
    sync::atomic::{AtomicU64, Ordering},
};

struct Cache {
    rows: [Option<c::CpuPerformanceHintInfo>; MAX_CPUS],
    next_sequence: u64,
}
struct Storage(UnsafeCell<Cache>);
// Brief Runtime admission protects copies/publication only, never a callback.
unsafe impl Sync for Storage {}
static CACHE: Storage = Storage(UnsafeCell::new(Cache {
    rows: [None; MAX_CPUS],
    next_sequence: 1,
}));
static PRESENT: AtomicU64 = AtomicU64::new(0);

pub(super) fn clear(_runtime: &RuntimeGuard) {
    unsafe { (*CACHE.0.get()).rows = [None; MAX_CPUS] };
    PRESENT.store(0, Ordering::Release);
}
fn online() -> u64 {
    percpu::r4os_rust_percpu_control(14, 0, 0, 0)
}

/// Exact sealed APIC geometry, including all presently known package peers.
/// A partial/incoherent package never becomes a guessed comparable scale.
fn geometry(cpu: u32) -> Option<(u64, u64)> {
    let own = cpu_topology::snapshot(cpu)?;
    if !own.sealed || !own.qualified || !own.trusted {
        return None;
    }
    let g = own.data?.geometry.ok()?;
    let mut scope = 0u64;
    let mut core = 0u64;
    for index in 0..cpu_topology::count() {
        let peer = cpu_topology::snapshot(index)?;
        if !peer.sealed || !peer.trusted {
            return None;
        }
        let p = peer.data?.geometry.ok()?;
        if p.package_shift != g.package_shift {
            return None;
        }
        if peer.apic_id >> p.package_shift == own.apic_id >> g.package_shift {
            scope |= 1u64 << index;
            if p.smt_shift != g.smt_shift {
                return None;
            }
            if peer.apic_id >> p.smt_shift == own.apic_id >> g.smt_shift {
                core |= 1u64 << index;
            }
        }
    }
    (scope.count_ones() == g.logical_bound && core != 0).then_some((scope, core))
}

pub(crate) fn publish(input: c::CpuPerformanceHint) -> Result<u64, i32> {
    if input.cpu_index as usize >= MAX_CPUS
        || input.flags & !c::cpu_performance_hint_flag_valid != 0
    {
        return Err(c::cpu_capabilities_error_invalid);
    }
    let identity = owner_identity(false).ok_or(c::cpu_capabilities_error_context)?;
    // The public mechanism already retains the exact current-CPU Session.
    if cpu_topology::epoch(input.cpu_index) != input.epoch || input.epoch == 0 {
        return Err(c::cpu_capabilities_error_epoch);
    }
    let stamp = if input.flags == 0 {
        if input.captured_ns != 0
            || input.scope_mask != 0
            || input.core_mask != 0
            || input.rank != 0
            || input.scale != 0
            || input.clock_generation != 0
            || input.clock_source != 0
        {
            return Err(c::cpu_capabilities_error_invalid);
        }
        None
    } else {
        if input.rank == 0
            || input.rank > 255
            || input.scale == 0
            || geometry(input.cpu_index) != Some((input.scope_mask, input.core_mask))
            || online() & (1 << input.cpu_index) == 0
        {
            return Err(c::cpu_capabilities_error_invalid);
        }
        let clock = monotonic::snapshot();
        if input.captured_ns == 0
            || clock.flags & c::monotonic_clock_flag_valid == 0
            || input.clock_generation == 0
            || input.clock_generation == u32::MAX
            || input.clock_generation != clock.generation
            || input.clock_source != clock.source
            || clock.instant_ns < input.captured_ns
            || clock.instant_ns - input.captured_ns > c::cpu_performance_hint_max_age_ns
        {
            return Err(c::cpu_capabilities_error_busy);
        }
        Some(clock)
    };
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &*STATE.0.get() };
    let binding = state
        .binding
        .filter(|b| (b.owner, b.epoch) == identity && b.cppc.is_some() && !state.closing)
        .ok_or(c::cpu_capabilities_error_context)?;
    let cache = unsafe { &mut *CACHE.0.get() };
    let slot = input.cpu_index as usize;
    if stamp.is_none() {
        // Even revoke requires the live exact registration/CPU epoch above.
        cache.rows[slot] = None;
        PRESENT.fetch_and(!(1 << slot), Ordering::Release);
        return Ok(0);
    }
    if cache.rows[slot].is_some_and(|old| {
        old.provider_generation == binding.cookie
            && old.epoch == input.epoch
            && old.clock_generation == input.clock_generation
            && old.clock_source == input.clock_source
            && old.captured_ns >= input.captured_ns
    }) {
        return Err(c::cpu_capabilities_error_busy);
    }
    let sequence = cache.next_sequence;
    cache.next_sequence = sequence
        .checked_add(1)
        .ok_or(c::cpu_capabilities_error_busy)?;
    let expires_ns = input
        .captured_ns
        .checked_add(c::cpu_performance_hint_max_age_ns)
        .ok_or(c::cpu_capabilities_error_busy)?;
    cache.rows[slot] = Some(c::CpuPerformanceHintInfo {
        cpu_index: input.cpu_index,
        state: c::cpu_performance_hint_state_current,
        flags: c::cpu_performance_hint_flag_valid,
        source: c::cpu_performance_hint_source_cppc,
        clock_generation: input.clock_generation,
        clock_source: input.clock_source,
        epoch: input.epoch,
        provider_generation: binding.cookie,
        sequence,
        captured_ns: input.captured_ns,
        expires_ns,
        scope_mask: input.scope_mask,
        core_mask: input.core_mask,
        rank: input.rank,
        scale: input.scale,
        ..Default::default()
    });
    PRESENT.fetch_or(1 << slot, Ordering::Release);
    Ok(sequence)
}

use crate::sched::placement::Ranks;
fn copies() -> [Option<c::CpuPerformanceHintInfo>; MAX_CPUS] {
    let _runtime = RuntimeGuard::acquire();
    unsafe { (*CACHE.0.get()).rows }
}
fn fresh(row: c::CpuPerformanceHintInfo, clock: monotonic::Snapshot, available: u64) -> bool {
    clock.flags & c::monotonic_clock_flag_valid != 0
        && row.clock_generation != 0
        && row.clock_generation != u32::MAX
        && row.clock_generation == clock.generation
        && row.clock_source == clock.source
        && row.captured_ns != 0
        && row.captured_ns <= clock.instant_ns
        && clock.instant_ns <= row.expires_ns
        && row.epoch == cpu_topology::epoch(row.cpu_index)
        && available & (1 << row.cpu_index) != 0
}
fn comparable(
    mask: u64,
    rows: &[Option<c::CpuPerformanceHintInfo>; MAX_CPUS],
    available: u64,
) -> Option<c::CpuPerformanceHintInfo> {
    let first = rows[mask.trailing_zeros() as usize]?;
    if mask & !first.scope_mask != 0 {
        return None;
    }
    let mut peers = first.scope_mask & available;
    while peers != 0 {
        let cpu = peers.trailing_zeros() as usize;
        peers &= peers - 1;
        let row = rows[cpu]?;
        if row.scope_mask != first.scope_mask
            || row.provider_generation != first.provider_generation
            || row.source != first.source
            || row.scale != first.scale
            || row.clock_generation != first.clock_generation
            || row.clock_source != first.clock_source
            || row.epoch != cpu_topology::epoch(cpu as u32)
        {
            return None;
        }
        let mut siblings = row.core_mask & available;
        while siblings != 0 {
            let peer = rows[siblings.trailing_zeros() as usize]?;
            siblings &= siblings - 1;
            if peer.core_mask != row.core_mask || peer.rank != row.rank {
                return None;
            }
        }
    }
    Some(first)
}
fn all_fresh(
    first: c::CpuPerformanceHintInfo,
    rows: &[Option<c::CpuPerformanceHintInfo>; MAX_CPUS],
    available: u64,
    clock: monotonic::Snapshot,
) -> bool {
    let mut peers = first.scope_mask & available;
    while peers != 0 {
        let cpu = peers.trailing_zeros() as usize;
        peers &= peers - 1;
        if !rows[cpu].is_some_and(|row| fresh(row, clock, available)) {
            return false;
        }
    }
    true
}
/// No-present, incomplete, uniform and efficient paths perform no clock probe.
/// One functional clock snapshot bounds a genuinely usable copied rank group.
pub(crate) fn ranks(mask: u64, efficient: bool) -> Option<Ranks> {
    if efficient || mask == 0 || mask & !PRESENT.load(Ordering::Acquire) != 0 {
        return None;
    }
    let available = online();
    if mask & !available != 0 {
        return None;
    }
    let rows = copies();
    let first = comparable(mask, &rows, available)?;
    let mut values = [0u8; MAX_CPUS];
    let mut different = false;
    let mut candidates = mask;
    while candidates != 0 {
        let cpu = candidates.trailing_zeros() as usize;
        candidates &= candidates - 1;
        let rank = rows[cpu]?.rank as u8;
        values[cpu] = rank;
        different |= rank != first.rank as u8;
    }
    if !different || !all_fresh(first, &rows, available, monotonic::snapshot()) {
        return None;
    }
    Some(Ranks { values })
}
/// # Safety
/// Exclusive initialized fixed output/tail. Passive cache only, no provider call.
pub(crate) unsafe extern "C" fn info(
    cpu: u32,
    epoch: u64,
    out: *mut c::CpuPerformanceHintInfo,
) -> i32 {
    if out.is_null()
        || !out.is_aligned()
        || unsafe {
            (*out).version != 1 || (*out).size < mem::size_of::<c::CpuPerformanceHintInfo>() as u32
        }
    {
        return c::cpu_capabilities_error_invalid;
    }
    let Some(_admission) = Admission::acquire(true) else {
        return c::cpu_capabilities_error_context;
    };
    let cap = match cpu_capabilities::info(cpu) {
        Ok(cap) => cap,
        Err(reason) => return reason,
    };
    if epoch == 0 || epoch != cap.epoch {
        return c::cpu_capabilities_error_epoch;
    }
    let mut value = c::CpuPerformanceHintInfo {
        cpu_index: cpu,
        epoch,
        ..Default::default()
    };
    if PRESENT.load(Ordering::Acquire) & (1 << cpu) != 0 {
        let rows = copies();
        if let Some(row) = rows[cpu as usize].filter(|row| row.epoch == epoch) {
            value = row;
            let clock = monotonic::snapshot();
            let available = online();
            if !fresh(row, clock, available) {
                value.state = c::cpu_performance_hint_state_stale;
                value.flags = 0;
            } else if comparable(1 << cpu, &rows, available)
                .is_some_and(|first| all_fresh(first, &rows, available, clock))
            {
                value.flags |= c::cpu_performance_hint_flag_group_ready;
            }
        }
    }
    unsafe { out.write(value) };
    0
}
