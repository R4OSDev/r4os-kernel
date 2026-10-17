//! Arbitrary R4X caller -> native BSP controller -> actual metadata retirement.
use super::*;
static DONE: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn check(_admission: &Admission) -> c::ProgramKernelNvmeCheck {
    let mut out = c::ProgramKernelNvmeCheck::default();
    if !any_device() {
        out.unavailable_reason = c::kernel_nvme_check_unavailable_device;
        return out;
    }
    let Some(mut gate) = gate() else {
        out.status = c::kernel_nvme_check_status_busy;
        return out;
    };
    let Ok(previous) =
        SEQUENCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
    else {
        out.unavailable_reason = c::kernel_nvme_check_unavailable_context;
        return out;
    };
    let run_id = previous + 1;
    unsafe {
        reset(run_id);
    }
    DONE.store(false, Ordering::Relaxed);
    let task = unsafe {
        (*session())
            .tasks
            .create(b"spectra-nvme-read", controller, Role::Interactive, None)
    };
    if task.is_null() {
        out.run_id = run_id;
        out.status = c::kernel_nvme_check_status_failed;
        out.fail_stage = c::kernel_nvme_check_stage_binding;
        return out;
    }
    unsafe {
        (*session()).controller = task;
    }
    if !diagnostic::start(task) {
        let released = unsafe { (*session()).tasks.finish_task(task, true) };
        out.run_id = run_id;
        out.status = c::kernel_nvme_check_status_failed;
        out.fail_stage = c::kernel_nvme_check_stage_binding;
        let (created, reaped, _) = unsafe { (*session()).tasks.summary() };
        out.tasks_created = created;
        out.tasks_reaped = reaped;
        out.retained_task_count = created - reaped;
        if !released {
            gate.0 = false;
            out.cleanup_errors = 1;
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
            out.status = c::kernel_nvme_check_status_failed;
            out.fail_stage = c::kernel_nvme_check_stage_cleanup;
            out.cleanup_errors = 1;
            out.flags = c::kernel_nvme_check_flag_partial_snapshot;
            out.tasks_created = u64::MAX;
            out.tasks_reaped = u64::MAX;
            out.retained_task_count = u64::MAX;
            out.retained_device_pin_count = u64::MAX;
            out.retained_buffer_count = u64::MAX;
            return out;
        }
        diagnostic::sleep(1, b"spectra-nvme-controller");
    }
    let released = unsafe { (*session()).tasks.finish_task(task, false) };
    out = unsafe { (*session()).report };
    let (created, reaped, retained) = unsafe { (*session()).tasks.summary() };
    out.tasks_created = created;
    out.tasks_reaped = reaped;
    out.retained_task_count = u64::from(retained);
    out.retained_device_pin_count = u64::from(unsafe { (*session()).pin_active });
    out.retained_buffer_count = u64::from(unsafe { (*session()).buffer_active });
    if released
        && retained == 0
        && out.retained_device_pin_count == 0
        && out.retained_buffer_count == 0
    {
        out.flags |= c::kernel_nvme_check_flag_cleanup_complete;
    } else {
        gate.0 = false;
        out.flags |= c::kernel_nvme_check_flag_partial_snapshot;
        out.cleanup_errors += 1;
        out.fail_stage = c::kernel_nvme_check_stage_cleanup;
        out.status = c::kernel_nvme_check_status_failed;
    }
    out
}
unsafe extern "C" fn controller() {
    let Some(_admission) = Admission::acquire(true) else {
        unsafe {
            (*session()).report.status = c::kernel_nvme_check_status_failed;
            (*session()).report.fail_stage = c::kernel_nvme_check_stage_binding;
        }
        DONE.store(true, Ordering::Release);
        return;
    };
    case::run();
    DONE.store(true, Ordering::Release);
}
