//! Explicit owned synthetic blockqueue work; no passive query invokes it.
use crate::{contract as c, sched::task_context::Admission, storage::block_probes};
use core::mem;
/// # Safety
/// Exclusive resident v1 header/capacity through return. Worker tasks receive
/// no output pointer or caller-owned buffer; the exact caller retains admission.
pub unsafe extern "C" fn check(out: *mut c::ProgramKernelBlockQueueCheck) -> i32 {
    if out.is_null()
        || unsafe { (*out).version } != c::kernel_block_queue_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelBlockQueueCheck>() as u32
    {
        return -1;
    }
    let Some(admission) = Admission::acquire(true) else {
        unsafe {
            out.write(c::ProgramKernelBlockQueueCheck::default());
        }
        return 0;
    };
    let report = block_probes::check(&admission);
    unsafe {
        out.write(report);
    }
    1
}
