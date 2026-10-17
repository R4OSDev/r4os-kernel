//! Explicit live caller protocol. The BSP controller never retains user
//! output; COMPLETE hands only resident owner state back to the exact caller.
use super::*;
use crate::{
    contract as c,
    sched::{task::diagnostic, task_context::Admission},
};
static COMPLETE: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static PASSED: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(super) fn clear_cancel() {
    CANCEL.store(false, Ordering::Release);
}
pub(super) fn cancelled() -> bool {
    CANCEL.load(Ordering::Acquire)
}

pub(crate) fn check(_admission: &Admission) -> c::ProgramKernelSmpCheck {
    let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let mut report = c::ProgramKernelSmpCheck {
        online_mask: online,
        online_count: online.count_ones(),
        ..Default::default()
    };
    if online.count_ones() < 2 {
        return report;
    }
    let Some(mut gate) = probe_gate() else {
        report.status = c::kernel_smp_check_status_busy;
        return report;
    };
    if retained_allocations() != 0 {
        gate.0 = false;
        report.status = c::kernel_smp_check_status_failed;
        report.cleanup_errors = 1;
        return report;
    }
    let Ok(previous) =
        SEQUENCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
    else {
        return report;
    };
    report.run_id = previous + 1;
    unsafe {
        session().write(Session {
            mapping: TlbProbe::EMPTY,
            tasks: diagnostic::Ledger::EMPTY,
            controller: ptr::null_mut(),
            retained: false,
            online_mask: online,
            placement_mask: 0,
            expected: 0,
            actual: 0,
            stage: 0,
            checked_clock: None,
        });
    }
    clear_cancel();
    reset(0);
    COMPLETE.store(false, Ordering::Relaxed);
    PASSED.store(false, Ordering::Relaxed);
    let controller = unsafe {
        (*session()).tasks.create(
            b"spectra-smp-check",
            controller,
            crate::sched::task::task_types::Role::Interactive,
            None,
        )
    };
    if controller.is_null() {
        report.status = c::kernel_smp_check_status_failed;
        report.fail_stage = c::kernel_smp_check_stage_workers;
        return report;
    }
    unsafe {
        (*session()).controller = controller;
    }
    if !diagnostic::start(controller) {
        let retired = unsafe { (*session()).tasks.finish_task(controller, true) };
        report.status = c::kernel_smp_check_status_failed;
        report.fail_stage = c::kernel_smp_check_stage_workers;
        (report.tasks_created, report.tasks_reaped, _) =
            unsafe { (&*ptr::addr_of!((*session()).tasks)).summary() };
        report.retained_task_count = report.tasks_created - report.tasks_reaped;
        if !retired {
            gate.0 = false;
            report.cleanup_errors = 1;
        }
        return report;
    }
    let deadline = timer::tick_count().saturating_add(60_000);
    while !COMPLETE.load(Ordering::Acquire) {
        if crate::program::process::should_close() {
            CANCEL.store(true, Ordering::Release);
        }
        if timer::tick_count() >= deadline {
            CANCEL.store(true, Ordering::Release);
            gate.0 = false;
            report.status = c::kernel_smp_check_status_failed;
            report.fail_stage = c::kernel_smp_check_stage_cleanup;
            report.flags = c::kernel_smp_check_flag_partial_snapshot;
            report.cleanup_errors = 1;
            // The controller still owns these facts. Never read its mutable
            // ledger or turn unavailable counts into measured zeroes.
            report.tasks_created = u64::MAX;
            report.tasks_reaped = u64::MAX;
            report.retained_task_count = u64::MAX;
            report.retained_frame_count = u64::MAX;
            report.retained_range_count = u64::MAX;
            report.retained_allocation_count = u64::MAX;
            return report;
        }
        diagnostic::sleep(1, b"spectra-smp-controller");
    }
    let retired = unsafe { (*session()).tasks.finish_task(controller, false) };
    report = collect(report.run_id);
    if !retired {
        report.cleanup_errors += 1;
    }
    let clean = report.retained_task_count == 0
        && report.retained_frame_count == 0
        && report.retained_range_count == 0
        && report.retained_allocation_count == 0
        && report.cleanup_errors == 0
        && !unsafe { (*session()).retained };
    if clean {
        report.flags |= c::kernel_smp_check_flag_cleanup_complete;
    } else {
        gate.0 = false;
        report.cleanup_errors = report.cleanup_errors.max(1);
    }
    if cancelled() {
        report.status = c::kernel_smp_check_status_cancelled;
    } else if report.online_count < 2 {
        report.status = c::kernel_smp_check_status_unavailable;
    } else if PASSED.load(Ordering::Acquire) && clean {
        report.status = c::kernel_smp_check_status_passed;
    } else {
        report.status = c::kernel_smp_check_status_failed;
    }
    if !clean {
        report.fail_stage = c::kernel_smp_check_stage_cleanup;
    }
    report
}
unsafe extern "C" fn controller() {
    let Some(_admission) = Admission::acquire(true) else {
        unsafe {
            (*session()).retained = true;
            (*session()).stage = c::kernel_smp_check_stage_workers;
        }
        COMPLETE.store(true, Ordering::Release);
        return;
    };
    let passed = cases::run(0, true);
    PASSED.store(passed, Ordering::Release);
    COMPLETE.store(true, Ordering::Release);
}
fn collect(run_id: u64) -> c::ProgramKernelSmpCheck {
    unsafe {
        let s = session();
        let clock = (*s)
            .checked_clock
            .unwrap_or_else(crate::platform::clocksource::snapshot);
        let receipt = (*s).mapping.last_unmap;
        let (created, reaped, retained) = (&*ptr::addr_of!((*s).tasks)).summary();
        let mut report = c::ProgramKernelSmpCheck {
            run_id,
            online_count: (*s).online_mask.count_ones(),
            online_mask: (*s).online_mask,
            placement_mask: (*s).placement_mask,
            observed_mask: CPUS.load(Ordering::Acquire),
            tlb_ready_mask: TLB_READY.load(Ordering::Acquire),
            tlb_updated_mask: TLB_UPDATED.load(Ordering::Acquire),
            owner_done_mask: OWNER_DONE.load(Ordering::Acquire),
            worker_failures: FAILURES.load(Ordering::Acquire),
            clock_regressions: REGRESSIONS.load(Ordering::Acquire),
            tlb_failures: TLB_FAILURES.load(Ordering::Acquire),
            owner_failures: OWNER_FAILURES.load(Ordering::Acquire),
            expected_checksum: (*s).expected,
            actual_checksum: (*s).actual,
            tlb_generation: receipt.generation,
            tlb_target_mask: receipt.target_mask,
            tlb_acknowledgement_mask: receipt.acknowledgement_mask,
            tlb_offline_mask: receipt.offline_mask,
            clock_source: clock.source,
            clock_generation: clock.generation,
            clock_registered_mask: clock.registered_cpu_mask,
            tasks_created: created,
            tasks_reaped: reaped,
            retained_task_count: u64::from(retained),
            retained_frame_count: (*s).mapping.live_frames(),
            retained_range_count: u64::from((*s).mapping.range_id != 0),
            retained_allocation_count: retained_allocations(),
            mutex_operations: MUTEX_COUNT.load(Ordering::Acquire),
            fail_stage: (*s).stage,
            ..Default::default()
        };
        if PASSED.load(Ordering::Acquire) {
            report.flags |= c::kernel_smp_check_flag_work_complete;
        }
        if retained != 0 {
            report.flags |= c::kernel_smp_check_flag_partial_snapshot;
        }
        if (*s).retained {
            report.cleanup_errors = 1;
        }
        report
    }
}
