//! Existing pure owner cases: bounded caller-private stack data, no live USB.
use crate::{
    contract as c,
    usb::{event_router, ring_cycle},
};
use core::mem;
/// # Safety
/// Caller retains exclusive resident fixed v1 output capacity for this call.
/// Nothing is borrowed by another task, controller, mailbox or callback.
pub unsafe extern "C" fn check(out: *mut c::ProgramKernelUsbCheck) -> i32 {
    if out.is_null()
        || unsafe { (*out).version } != c::kernel_usb_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelUsbCheck>() as u32
    {
        return -1;
    }
    let mut passed = 0;
    if ring_cycle::self_test() {
        passed |= c::kernel_usb_check_case_ring;
    }
    if event_router::self_test() {
        passed |= c::kernel_usb_check_case_router;
    }
    let failed = c::kernel_usb_check_cases_all & !passed;
    unsafe {
        out.write(c::ProgramKernelUsbCheck {
            status: if failed == 0 {
                c::kernel_usb_check_status_passed
            } else {
                c::kernel_usb_check_status_failed
            },
            selected_cases: c::kernel_usb_check_cases_all,
            attempted_cases: c::kernel_usb_check_cases_all,
            passed_cases: passed,
            failed_cases: failed,
            ..Default::default()
        });
    }
    1
}
