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
    if report.status == c::kernel_block_queue_check_status_passed {
        // Passive observations only during this explicit native diagnosis.
        // Content, cancellation and retirement retain their separate oracles.
        let block = crate::storage::block_workers::execution_cpus();
        crate::boot::log_route::puts(b"WORKERCPU block=");
        for (label, mask) in [
            (b"".as_slice(), block.0),
            (b" r4d_block=", block.1),
            (b" cache_policy=", crate::fs::page_cache::worker_cpus()),
            (b" rx_frames=", crate::net::rx_runtime::execution_cpus()),
            (b" ipc_handlers=", crate::service::ipc::execution_cpus()),
        ] {
            crate::boot::log_route::puts(label);
            crate::boot::log_route::dec(mask);
        }
        crate::boot::log_route::puts(b"\r\n");
    }
    unsafe {
        out.write(report);
    }
    1
}
