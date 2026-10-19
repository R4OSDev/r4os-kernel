//! Explicit lifecycle control; passive query never claims or changes a CPU.
use crate::{arch::cpu_lifecycle as life, contract as c, sched::task_context::Admission};
use core::{mem, ptr};
/// # Safety
/// The caller retains initialized v1 header and exclusive resident capacity.
/// No output borrow or worker pointer survives this call. Errors preserve it.
pub(crate) unsafe extern "C" fn control(
    operation: u32,
    index: u32,
    expected: u64,
    out: *mut c::CpuLifecycleInfo,
) -> i32 {
    if out.is_null()
        || !out.is_aligned()
        || unsafe { ptr::addr_of!((*out).version).read() } != c::cpu_lifecycle_version
        || unsafe { ptr::addr_of!((*out).size).read() }
            < mem::size_of::<c::CpuLifecycleInfo>() as u32
        || operation > c::cpu_lifecycle_op_online
        || operation == c::cpu_lifecycle_op_query && expected != 0
    {
        return c::cpu_lifecycle_error_invalid;
    }
    if operation == c::cpu_lifecycle_op_query {
        let Some(value) = life::snapshot(index) else {
            return c::cpu_lifecycle_error_index;
        };
        unsafe {
            out.write(value);
        }
        return c::cpu_lifecycle_ok;
    }
    if !life::caller_context() {
        return c::cpu_lifecycle_error_context;
    }
    let Some(_admission) = Admission::acquire(true) else {
        return c::cpu_lifecycle_error_context;
    };
    let claim = match life::claim() {
        Ok(claim) => claim,
        Err(reason) => return life::result(reason),
    };
    let action = if operation == c::cpu_lifecycle_op_offline {
        life::offline(&claim, index, expected)
    } else {
        life::online(&claim, index, expected)
    };
    if let Err(reason) = action {
        return life::result(reason);
    }
    let Some(value) = life::snapshot(index) else {
        return c::cpu_lifecycle_error_index;
    };
    unsafe {
        out.write(value);
    }
    c::cpu_lifecycle_ok
}
/// # Safety
/// The original task pins this exclusive resident header/output through every
/// actual retirement. No worker receives the output or a caller stack borrow.
pub(crate) unsafe extern "C" fn check(out: *mut c::ProgramKernelCpuLifecycleCheck) -> i32 {
    if out.is_null()
        || !out.is_aligned()
        || unsafe { ptr::addr_of!((*out).version).read() } != c::cpu_lifecycle_version
        || unsafe { ptr::addr_of!((*out).size).read() }
            < mem::size_of::<c::ProgramKernelCpuLifecycleCheck>() as u32
    {
        return -1;
    }
    if !life::caller_context() {
        return 0;
    }
    let Some(_admission) = Admission::acquire(true) else {
        return 0;
    };
    let report = life::check();
    unsafe {
        out.write(report);
    }
    1
}
