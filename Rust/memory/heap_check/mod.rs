//! Explicit diagnostic work. The allocator remains shared; this one resident
//! ledger belongs to exactly one admitted run, including across waits.
mod allocations;
mod cases;

use super::heap;
use crate::contract as c;
pub(crate) use crate::sched::task_context::Admission;
use allocations::State;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Cell(UnsafeCell<State>);
// Only RUNNING's successful CAS admits ledger access. Busy readers touch no
// ledger storage; a failed cleanup retains it without any automatic retry.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new(State::EMPTY));
static RUNNING: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Run;
impl Drop for Run {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

pub(super) fn stats() -> heap::Stats {
    let mut value = heap::Stats::default();
    unsafe {
        heap::r4os_rust_heap_stats(&mut value);
    }
    value
}

/// Admission outlives output publication at the R4DEV boundary. Boot callers
/// have no task and retain their original single BSP initialization context.
pub(crate) fn run(
    _admission: &Admission,
    first: u32,
    last: u32,
    cancelled: fn() -> bool,
) -> c::ProgramKernelHeapCheck {
    let mut report = c::ProgramKernelHeapCheck::default();
    if !heap::r4os_rust_heap_initialized() {
        return report;
    }
    report.flags = c::kernel_heap_check_flag_initialized;
    if RUNNING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        report.status = c::kernel_heap_check_status_busy;
        return report;
    }
    let _run = Run;
    // SAFETY: exclusive diagnostic admission spans the exact task's unwind
    // token. No other path observes STATE, even while allocation blocks.
    let state = unsafe { &mut *STATE.0.get() };
    if state.report.cleanup_errors != 0 {
        return state.report;
    }
    let Ok(previous) =
        SEQUENCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
    else {
        return report;
    };
    state.begin(previous + 1);
    let before = stats();
    state.report.before_used_bytes = before.used_bytes as u64;
    state.report.before_active_blocks = before.active_blocks as u64;
    state.report.status = c::kernel_heap_check_status_passed;
    for case in first..=last {
        if cancelled() {
            state.report.status = c::kernel_heap_check_status_cancelled;
            state.report.failed_case = case;
            break;
        }
        if !cases::run(case, state, cancelled) {
            state.report.status = if cancelled() {
                c::kernel_heap_check_status_cancelled
            } else {
                c::kernel_heap_check_status_failed
            };
            state.report.failed_case = case;
            break;
        }
        state.report.cases_completed += 1;
    }
    if state.report.status == c::kernel_heap_check_status_passed && cancelled() {
        state.report.status = c::kernel_heap_check_status_cancelled;
        state.report.failed_case = last;
    }
    state.cleanup();
    let budgets_match = {
        let _vm = super::owner::Guard::acquire(5);
        !super::vm_transactions::r4os_rust_vm_initialized() || super::vm_ranges::budget_invariant()
    };
    if state.report.owned_live_blocks == 0
        && state.report.vm_live_reservations == 0
        && state.report.cleanup_errors == 0
        && budgets_match
    {
        state.report.flags |= c::kernel_heap_check_flag_cleanup_complete;
    } else {
        state.report.status = c::kernel_heap_check_status_failed;
        state.report.failed_case = c::kernel_heap_check_case_cleanup;
    }
    let after = stats();
    state.report.after_used_bytes = after.used_bytes as u64;
    state.report.after_active_blocks = after.active_blocks as u64;
    state.report
}

pub(super) fn boot_range(first: u32, last: u32) -> c::ProgramKernelHeapCheck {
    let Some(admission) = Admission::acquire(false) else {
        return c::ProgramKernelHeapCheck::default();
    };
    run(&admission, first, last, || false)
}
