//! Explicit work only. A resident BSP controller reuses native boot cases;
//! AP callers see the immutable report only after the release/acquire handoff.
pub(crate) mod park;
pub(crate) mod tasks;
mod unwind;
use crate::{
    contract as c,
    platform::timer,
    sched::{
        scheduler, sync_selftest, task::task_types::Role, task_context::Admission, task_probe,
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static BUSY: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Cell(UnsafeCell<c::ProgramKernelSchedulerCheck>);
// Gate -> controller -> DONE acquire -> caller, never overlapping writers.
unsafe impl Sync for Cell {}
static REPORT: Cell = Cell(UnsafeCell::new(c::ProgramKernelSchedulerCheck {
    version: 1,
    size: 128,
    status: 0,
    selected: 0,
    passed: 0,
    failed: 0,
    unavailable: 0,
    isolated: 15,
    cleanup_errors: 0,
    live_tasks: 0,
    live_work: 0,
    reserved0: 0,
    run_id: 0,
    tasks_created: 0,
    tasks_reaped: 0,
    work_submitted: 0,
    work_released: 0,
    event_iterations: 0,
    if_checks: 0,
    deadline_tasks: 0,
    role_mask: 0,
    unwind_checks: 0,
}));
pub(crate) struct Gate(bool, Option<crate::arch::cpu_lifecycle::ProbeLease>);
impl Gate {
    pub(crate) fn retain(&mut self) {
        self.0 = false;
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        if self.0 {
            LIVE.store(false, Ordering::Release);
            BUSY.store(false, Ordering::Release);
        } else if let Some(lease) = self.1.take() {
            core::mem::forget(lease);
        }
    }
}
pub(crate) fn gate() -> Option<Gate> {
    let lease = crate::arch::cpu_lifecycle::probe_lease()?;
    BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .ok()
        .map(|_| Gate(true, Some(lease)))
}
pub(crate) fn live() -> bool {
    LIVE.load(Ordering::Acquire)
}
pub(crate) fn cancelled() -> bool {
    CANCEL.load(Ordering::Acquire)
}
pub(crate) fn update(f: impl FnOnce(&mut c::ProgramKernelSchedulerCheck)) {
    // Called only by the BSP controller, never its waiting AP caller.
    unsafe {
        f(&mut *REPORT.0.get());
    }
}
pub(crate) fn run(_admission: &Admission, selected: u32) -> c::ProgramKernelSchedulerCheck {
    let mut report = c::ProgramKernelSchedulerCheck {
        selected,
        ..Default::default()
    };
    let Some(mut gate) = gate() else {
        report.status = c::kernel_scheduler_check_status_busy;
        return report;
    };
    let Ok(previous) =
        SEQUENCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
    else {
        return report;
    };
    report.run_id = previous + 1;
    unsafe {
        tasks::reset();
        REPORT.0.get().write(report);
    }
    DONE.store(false, Ordering::Relaxed);
    CANCEL.store(false, Ordering::Relaxed);
    LIVE.store(true, Ordering::Release);
    let controller = tasks::create(b"spectra-kernel-check", controller, Role::Interactive);
    if controller.is_null() {
        report.status = c::kernel_scheduler_check_status_failed;
        report.failed = selected;
        return report;
    }
    if !tasks::start(controller) {
        report.status = c::kernel_scheduler_check_status_failed;
        report.failed = selected;
        if !tasks::finish_task(controller, true) {
            gate.retain();
            report.cleanup_errors = 1;
        }
        return report;
    }
    // WORKERS explicitly includes the real Desktop screenshot write/return.
    // All established owner cases, including default all/127, retain 20_000.
    let limit = if selected == c::kernel_scheduler_check_case_workers {
        45_000
    } else {
        20_000
    };
    let end = timer::tick_count().saturating_add(limit);
    while !DONE.load(Ordering::Acquire) {
        if crate::program::process::should_close() {
            CANCEL.store(true, Ordering::Release);
        }
        if timer::tick_count() >= end {
            CANCEL.store(true, Ordering::Release);
            gate.retain();
            report.status = c::kernel_scheduler_check_status_failed;
            report.failed = selected;
            report.cleanup_errors = 1;
            report.live_tasks = 1;
            return report;
        }
        tasks::sleep(1, b"spectra-kernel-controller");
    }
    // DONE publishes the whole immutable report and ledger, not task exit.
    report = unsafe { REPORT.0.get().read() };
    if !tasks::finish_task(controller, false) {
        report.cleanup_errors += 1;
    }
    (report.tasks_created, report.tasks_reaped, report.live_tasks) = tasks::summary();
    if report.cleanup_errors != 0 || report.live_tasks != 0 || report.live_work != 0 {
        report.status = c::kernel_scheduler_check_status_failed;
        gate.retain();
    }
    report
}
unsafe extern "C" fn controller() {
    let Some(_admission) = Admission::acquire(true) else {
        update(|r| {
            r.status = c::kernel_scheduler_check_status_failed;
            r.failed = r.selected;
        });
        DONE.store(true, Ordering::Release);
        return;
    };
    unsafe {
        task_probe::live::begin();
    }
    let selected = unsafe { (*REPORT.0.get()).selected };
    let mut failed = false;
    for bit in 0..8 {
        let mask = 1 << bit;
        if selected & mask == 0 {
            continue;
        }
        if cancelled() {
            update(|r| {
                r.failed |= mask;
                r.status = c::kernel_scheduler_check_status_cancelled;
            });
            break;
        }
        let result = match bit {
            0 => Some(sync_selftest::cases()),
            1 => Some(unwind::run()),
            7 => crate::sched::worker_observation::run(),
            _ => unsafe { task_probe::live::case(bit) },
        };
        match result {
            Some(true) => update(|r| r.passed |= mask),
            Some(false) => {
                failed = true;
                update(|r| r.failed |= mask);
                break;
            }
            None => update(|r| r.unavailable |= mask),
        }
    }
    sync_selftest::stop();
    if !tasks::cleanup_except(scheduler::current().cast()) {
        update(|r| r.cleanup_errors += 1);
    }
    update(|r| {
        if r.status != c::kernel_scheduler_check_status_cancelled {
            r.status = if failed || r.cleanup_errors != 0 {
                c::kernel_scheduler_check_status_failed
            } else if r.unavailable != 0 {
                c::kernel_scheduler_check_status_unavailable
            } else {
                c::kernel_scheduler_check_status_passed
            };
        }
    });
    DONE.store(true, Ordering::Release);
}
pub(crate) fn boot_sync() -> bool {
    let Some(mut gate) = gate() else {
        return false;
    };
    let Some(_admission) = Admission::acquire(false) else {
        return false;
    };
    unsafe {
        tasks::reset();
        REPORT
            .0
            .get()
            .write(c::ProgramKernelSchedulerCheck::default());
    }
    LIVE.store(true, Ordering::Release);
    CANCEL.store(false, Ordering::Relaxed);
    let ok = sync_selftest::cases();
    sync_selftest::stop();
    if !tasks::cleanup_except(ptr::null_mut()) || !ok {
        gate.retain();
        return false;
    }
    ok
}
