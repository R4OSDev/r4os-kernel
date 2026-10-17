//! Native BSP controller; the exact R4X caller owns output and waits on DONE.
use super::*;
use crate::contract as c;
static DONE: AtomicBool = AtomicBool::new(false);
static WORK_OK: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn check(_admission: &Admission) -> c::ProgramKernelBlockQueueCheck {
    let mut out = c::ProgramKernelBlockQueueCheck::default();
    if !workers::r4os_rust_block_workers_ready() {
        out.unavailable_cases = 255;
        return out;
    }
    let Some(mut gate) = session::gate() else {
        out.status = c::kernel_block_queue_check_status_busy;
        return out;
    };
    let Ok(previous) =
        SEQUENCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
    else {
        out.unavailable_cases = 255;
        return out;
    };
    let run_id = previous + 1;
    unsafe {
        session::reset();
    }
    DONE.store(false, Ordering::Relaxed);
    WORK_OK.store(false, Ordering::Relaxed);
    let task = unsafe {
        (*session::session()).tasks.create(
            b"spectra-block-check",
            controller,
            Role::Interactive,
            None,
        )
    };
    if task.is_null() {
        out.status = c::kernel_block_queue_check_status_failed;
        out.fail_stage = 1;
        out.run_id = run_id;
        return out;
    }
    unsafe {
        (*session::session()).controller = task;
    }
    if !diagnostic::start(task) {
        let released = unsafe { (*session::session()).tasks.finish_task(task, true) };
        out = session::collect(run_id);
        out.status = c::kernel_block_queue_check_status_failed;
        out.fail_stage = 1;
        if !released {
            gate.0 = false;
            out.cleanup_errors += 1;
        } else {
            out.flags = c::kernel_block_queue_check_flag_cleanup_complete;
        }
        return out;
    }
    let deadline = timer::tick_count().saturating_add(60 * crate::arch::pit::DEFAULT_HZ as u64);
    while !DONE.load(Ordering::Acquire) {
        if crate::program::process::should_close() {
            CANCEL.store(true, Ordering::Release);
        }
        if timer::tick_count() >= deadline {
            CANCEL.store(true, Ordering::Release);
            gate.0 = false;
            out.run_id = run_id;
            out.status = c::kernel_block_queue_check_status_failed;
            out.fail_stage = 9;
            out.cleanup_errors = 1;
            out.flags = c::kernel_block_queue_check_flag_partial_snapshot;
            out.tasks_created = u64::MAX;
            out.tasks_reaped = u64::MAX;
            out.retained_task_count = u64::MAX;
            out.retained_controller_count = u64::MAX;
            out.retained_request_count = u64::MAX;
            out.retained_buffer_count = u64::MAX;
            return out;
        }
        diagnostic::sleep(1, b"spectra-block-controller");
    }
    let released = unsafe { (*session::session()).tasks.finish_task(task, false) };
    out = session::collect(run_id);
    if !released {
        out.cleanup_errors += 1;
    }
    let clean = out.cleanup_errors == 0
        && out.retained_task_count == 0
        && out.retained_controller_count == 0
        && out.retained_request_count == 0
        && out.retained_buffer_count == 0
        && !unsafe { (*session::session()).retained };
    if clean {
        out.flags |= c::kernel_block_queue_check_flag_cleanup_complete;
    } else {
        gate.0 = false;
        out.cleanup_errors = out.cleanup_errors.max(1);
        out.fail_stage = 9;
    }
    if WORK_OK.load(Ordering::Acquire) {
        out.flags |= c::kernel_block_queue_check_flag_work_complete;
    }
    out.status = if CANCEL.load(Ordering::Acquire) {
        c::kernel_block_queue_check_status_cancelled
    } else if WORK_OK.load(Ordering::Acquire) && clean {
        c::kernel_block_queue_check_status_passed
    } else {
        c::kernel_block_queue_check_status_failed
    };
    out
}
unsafe extern "C" fn controller() {
    let Some(_admission) = Admission::acquire(true) else {
        unsafe {
            (*session::session()).retained = true;
            (*session::session()).cleanup_errors += 1;
            (*session::session()).stage = 1;
        }
        DONE.store(true, Ordering::Release);
        return;
    };
    let ok = cases::run();
    WORK_OK.store(ok, Ordering::Release);
    let _ = session::cleanup();
    DONE.store(true, Ordering::Release);
}
