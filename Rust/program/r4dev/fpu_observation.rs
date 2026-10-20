//! Typed optional observation API. Caller data is copied outside runtime
//! admission. Only the exact task's retained image interval is admitted.
use crate::{arch::runtime, contract as c, sched::fpu_observation};
use core::mem;
/// # Safety
/// Caller retains aligned initialized request/output records through this
/// synchronous call. No pointer is kept during the observed userland work.
pub(crate) unsafe extern "C" fn call(
    request: *const c::FpuObservationRequest,
    out: *mut c::FpuObservationInfo,
) -> i32 {
    if request.is_null() || !request.is_aligned() || out.is_null() || !out.is_aligned() {
        return c::fpu_observation_error_invalid;
    }
    let request = unsafe { request.read() };
    let header = out.cast::<u32>();
    if unsafe { header.read() } != 1
        || unsafe { header.add(1).read() } < mem::size_of::<c::FpuObservationInfo>() as u32
    {
        return c::fpu_observation_error_invalid;
    }
    let result = {
        let _runtime = runtime::RuntimeGuard::acquire();
        if request.operation == c::fpu_observation_begin {
            let instance = unsafe { crate::program::context::execution() };
            if instance.is_null() {
                return c::fpu_observation_error_unavailable;
            }
            if request.rip_end <= request.rip_begin
                || request.rip_end - request.rip_begin > c::fpu_observation_max_window_bytes
                || !unsafe {
                    crate::program::peer_lifetime::contains(instance, request.rip_begin)
                        && crate::program::peer_lifetime::contains(instance, request.rip_end - 1)
                }
            {
                return c::fpu_observation_error_invalid;
            }
        }
        fpu_observation::control(request)
    };
    match result {
        Ok(info) => {
            unsafe {
                out.write(info);
            }
            0
        }
        Err(reason) => reason,
    }
}
const _: () = {
    assert!(mem::size_of::<c::FpuObservationRequest>() == 48);
    assert!(mem::size_of::<c::FpuObservationInfo>() == 128);
};
