//! Direct services platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

pub(super) unsafe extern "C" fn sys_service_status(
    name_ptr: *const u8,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            0,
            name_ptr.cast(),
            out.cast(),
            0,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_open(
    name_ptr: *const u8,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            1,
            name_ptr.cast(),
            out.cast(),
            0,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_close(handle: u32) -> i32 {
    unsafe { crate::program::service_api::r4os_rust_program_service_endpoint(0, handle, 0) }
}
pub(super) unsafe extern "C" fn sys_service_call(
    handle: u32,
    op: u16,
    request_ptr: *const u8,
    request_len: u32,
    response_header: *mut c::ServiceMessageHeader,
    response_ptr: *mut u8,
    response_capacity: u32,
    timeout_ticks: u64,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_service(
            handle,
            op,
            request_ptr.cast(),
            request_len,
            response_header.cast(),
            response_ptr.cast(),
            response_capacity,
            timeout_ticks,
            0,
            true,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_endpoint_unregister(handle: u32) -> i32 {
    unsafe { crate::program::service_api::r4os_rust_program_service_endpoint(1, handle, 0) }
}
pub(super) unsafe extern "C" fn sys_service_endpoint_poll(handle: u32) -> i32 {
    unsafe { crate::program::service_api::r4os_rust_program_service_endpoint(2, handle, 0) }
}
pub(super) unsafe extern "C" fn sys_service_start(
    name_ptr: *const u8,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            2,
            name_ptr.cast(),
            out.cast(),
            0,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_stop(
    name_ptr: *const u8,
    out: *mut c::ServiceInfo,
    timeout_ticks: u64,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            3,
            name_ptr.cast(),
            out.cast(),
            0,
            timeout_ticks,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_restart(
    name_ptr: *const u8,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            4,
            name_ptr.cast(),
            out.cast(),
            0,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_remove(name_ptr: *const u8) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            7,
            name_ptr.cast(),
            core::ptr::null_mut(),
            0,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_set_start_mode(
    name_ptr: *const u8,
    start_mode_raw: u32,
    out: *mut c::ServiceInfo,
) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_named(
            6,
            name_ptr.cast(),
            out.cast(),
            start_mode_raw,
            0,
        )
    }
}
pub(super) unsafe extern "C" fn sys_service_endpoint_wait(handle: u32, timeout_ticks: u64) -> i32 {
    unsafe {
        crate::program::service_api::r4os_rust_program_service_endpoint(3, handle, timeout_ticks)
    }
}
