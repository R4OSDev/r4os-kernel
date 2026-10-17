//! Explicit read-only hardware proof. Passive status never invokes this work.
use crate::{contract as c, sched::task_context::Admission, storage::nvme_check};
use core::mem;
/// # Safety
/// Exact caller retains resident exclusive v1 output/capacity. No worker or
/// driver receives this pointer; only resident private observation is shared.
pub unsafe extern "C" fn check(out: *mut c::ProgramKernelNvmeCheck) -> i32 {
    if out.is_null()
        || unsafe { (*out).version } != c::kernel_nvme_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelNvmeCheck>() as u32
    {
        return -1;
    }
    let Some(admission) = Admission::acquire(true) else {
        unsafe {
            out.write(c::ProgramKernelNvmeCheck {
                unavailable_reason: c::kernel_nvme_check_unavailable_context,
                ..Default::default()
            });
        }
        return 0;
    };
    let report = nvme_check::check(&admission);
    unsafe {
        out.write(report);
    }
    1
}
