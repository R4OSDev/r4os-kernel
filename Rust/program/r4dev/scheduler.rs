//! Explicit bounded native work. Output remains owned by the original task.
use crate::{
    contract as c,
    sched::{kernel_check, task_context::Admission},
};
use core::mem;
/// # Safety
/// Caller provides one resident exclusive v1 header and its stated capacity.
/// No pointer to this storage is passed to a worker or retained after return.
pub unsafe extern "C" fn kernel_check(
    selection: u32,
    out: *mut c::ProgramKernelSchedulerCheck,
) -> i32 {
    if selection == 0
        || selection
            & !(c::kernel_scheduler_check_case_all
                | c::kernel_scheduler_check_case_workers
                | c::kernel_scheduler_check_case_idle)
            != 0
        || (selection & c::kernel_scheduler_check_case_workers != 0
            && selection != c::kernel_scheduler_check_case_workers)
        || (selection & c::kernel_scheduler_check_case_idle != 0
            && selection != c::kernel_scheduler_check_case_idle)
        || out.is_null()
        || unsafe { (*out).version } != c::kernel_scheduler_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelSchedulerCheck>() as u32
    {
        return -1;
    }
    let Some(admission) = Admission::acquire(true) else {
        unsafe {
            out.write(c::ProgramKernelSchedulerCheck::default());
        }
        return 0;
    };
    let report = kernel_check::run(&admission, selection);
    unsafe {
        out.write(report);
    }
    1
}
