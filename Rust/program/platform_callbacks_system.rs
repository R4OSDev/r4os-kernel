//! Direct system platform callbacks; original typed owners and call boundaries.
pub(super) unsafe extern "C" fn sys_waitset_source_prepare(
    request: *const crate::contract::WaitSourceRequest,
    output: *mut crate::contract::WaitSource,
) -> i32 {
    unsafe { super::wait_sources::prepare(request, output) }
}
pub(super) unsafe extern "C" fn sys_waitset_create(capacity: u32, output: *mut u64) -> i32 {
    unsafe { super::waitsets::create(capacity, output) }
}
pub(super) unsafe extern "C" fn sys_waitset_enroll(
    handle: u64,
    source: *const crate::contract::WaitSource,
    output: *mut u64,
) -> i32 {
    unsafe { super::waitsets::enroll(handle, source, output) }
}
pub(super) unsafe extern "C" fn sys_waitset_rearm(
    handle: u64,
    registration: u64,
    source: *const crate::contract::WaitSource,
) -> i32 {
    unsafe { super::waitsets::rearm(handle, registration, source) }
}
pub(super) unsafe extern "C" fn sys_waitset_remove(handle: u64, registration: u64) -> i32 {
    unsafe { super::waitsets::remove(handle, registration) }
}
pub(super) unsafe extern "C" fn sys_waitset_wait(
    handle: u64,
    deadline: *const crate::contract::WaitSetDeadline,
    events: *mut crate::contract::WaitSetEvent,
    capacity: u32,
    batch: *mut crate::contract::WaitSetBatch,
) -> i32 {
    unsafe { super::waitsets::wait(handle, deadline, events, capacity, batch) }
}
pub(super) unsafe extern "C" fn sys_waitset_close(handle: u64) -> i32 {
    unsafe { super::waitsets::close(handle) }
}
pub(super) unsafe extern "C" fn sys_write(data: *const u8, len: u32) -> i32 {
    unsafe { crate::program::program_io::r4os_rust_program_io_write(1, data.cast(), len, false) }
}
pub(super) unsafe extern "C" fn sys_putc(ch: u8) {
    crate::program::program_io::r4os_rust_program_io_byte(1, ch, false);
}
pub(super) unsafe extern "C" fn sys_system_halt() {
    unsafe { crate::program::system::control::r4os_rust_sys_halt() }
}
pub(super) unsafe extern "C" fn sys_system_poweroff() {
    unsafe { crate::program::system::control::r4os_rust_sys_poweroff() }
}
pub(super) unsafe extern "C" fn sys_boot_ready() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(3)
}
pub(super) unsafe extern "C" fn sys_notification_create(output: *mut u64) -> i32 {
    unsafe {
        crate::program::peer_lifetime::r4os_rust_program_peer_notification(
            0,
            0,
            0,
            0,
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_notification_query(handle: u64, output: *mut u64) -> i32 {
    unsafe {
        crate::program::peer_lifetime::r4os_rust_program_peer_notification(
            1,
            handle,
            0,
            0,
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_notification_notify(handle: u64, count: u32) -> i32 {
    unsafe {
        crate::program::peer_lifetime::r4os_rust_program_peer_notification(
            2,
            handle,
            u64::from(count),
            0,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_notification_wait(
    handle: u64,
    observed: u64,
    timeout_ticks: u64,
) -> i32 {
    unsafe {
        crate::program::peer_lifetime::r4os_rust_program_peer_notification(
            3,
            handle,
            observed,
            timeout_ticks,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_notification_close(handle: u64) -> i32 {
    unsafe {
        crate::program::peer_lifetime::r4os_rust_program_peer_notification(
            4,
            handle,
            0,
            0,
            core::ptr::null_mut(),
        )
    }
}
