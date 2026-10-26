//! Native instrumentation: observational wait counters are CPU-sharded atomics.
//! They never publish queue/task state. Lock tracking retains the runtime owner.
use super::sync_types::{LockSummary, WaitSummary};
use super::task::task_types::WaitResult;
pub(super) use super::wait_counters::Metric as WaitMetric;
use crate::{
    arch::{percpu, runtime, smp_policy::MAX_CPUS},
    sched::scheduler,
};
use core::cell::UnsafeCell;
static WAIT: super::wait_counters::Counters<MAX_CPUS> = super::wait_counters::Counters::new();

// Relaxed is deliberate: these exact event counters do not protect resources
// or act as completion/publication flags. The functional runtime admission,
// enrollment, wake, ownership and lifetime barriers remain unchanged.
#[inline]
pub(super) fn wait_add(metric: WaitMetric, amount: u64) {
    WAIT.add(percpu::current_index() as usize, metric, amount);
}
pub(super) fn wait_latency(elapsed: u64, result: WaitResult) {
    let cpu = percpu::current_index() as usize;
    WAIT.latency(cpu, elapsed);
    if result == WaitResult::Timeout {
        WAIT.add(cpu, WaitMetric::Timeouts, 1);
    }
    if result == WaitResult::Cancelled {
        WAIT.add(cpu, WaitMetric::Cancellations, 1);
    }
}

struct Storage(UnsafeCell<LockSummary>);
// SAFETY: every lock-summary read/update holds the canonical runtime owner.
// Bounded update closures never call an owner or execute scheduler work.
unsafe impl Sync for Storage {}
static STATS: Storage = Storage(UnsafeCell::new(LockSummary {
    acquires: 0,
    releases: 0,
    recursive_acquires: 0,
    contention_waits: 0,
    contention_timeouts: 0,
    order_violations: 0,
    sleep_checks: 0,
    sleep_under_lock: 0,
    sleep_under_no_sleep_lock: 0,
    unlock_mismatches: 0,
    held_slots_used: 0,
    current_depth: 0,
    max_depth: 0,
    tracking_drops: 0,
    role_inversions: 0,
    role_donations: 0,
    role_donation_releases: 0,
}));
pub(super) fn lock_change(f: impl FnOnce(&mut LockSummary)) {
    // Updates already inside the canonical runtime section use that owner.
    // Outside it, acquire explicitly before borrowing the shared counters.
    let flags = if runtime::r4os_rust_runtime_control(8, 0) != 0 {
        None
    } else {
        Some(runtime::save_runtime())
    };
    unsafe { f(&mut *STATS.0.get()) };
    if let Some(flags) = flags {
        runtime::restore_runtime(flags);
    }
}
/// # Safety
/// Output points to exclusive aligned resident C summary storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_wait_summary(out: *mut WaitSummary) {
    let flags = runtime::save_runtime();
    let values = WAIT.snapshot();
    let wait_value = |metric: WaitMetric| values[metric as usize];
    let value = WaitSummary {
        queue_waits: wait_value(WaitMetric::QueueWaits),
        wake_one: wait_value(WaitMetric::WakeOne),
        wake_all: wait_value(WaitMetric::WakeAll),
        timeouts: wait_value(WaitMetric::Timeouts),
        cancellations: wait_value(WaitMetric::Cancellations),
        drops: wait_value(WaitMetric::Drops),
        total_wait_ticks: wait_value(WaitMetric::TotalWaitTicks),
        max_wait_ticks: wait_value(WaitMetric::MaxWaitTicks),
        last_wait_ticks: wait_value(WaitMetric::LastWaitTicks),
        directed_wakes: wait_value(WaitMetric::DirectedWakes),
        directed_scans: wait_value(WaitMetric::DirectedScans),
        directed_fifo_bypasses: wait_value(WaitMetric::DirectedFifoBypasses),
    };
    unsafe { out.write(value) };
    runtime::restore_runtime(flags);
}
/// # Safety
/// Output points to exclusive aligned resident C summary storage. Task reads
/// retain the canonical runtime owner until the projection is copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_lock_summary(out: *mut LockSummary) {
    let flags = runtime::save_runtime();
    let mut value = unsafe { *STATS.0.get() };
    let task = scheduler::current();
    if !task.is_null() {
        value.current_depth =
            unsafe { (*super::task::task_registry::core(task.cast())).held_lock_count };
    }
    unsafe { out.write(value) };
    runtime::restore_runtime(flags);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sync_note_sleep() {
    super::sync_locks::note_sleep();
}
