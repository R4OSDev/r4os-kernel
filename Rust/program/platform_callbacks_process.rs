//! Direct process platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

pub(super) unsafe extern "C" fn sys_program_should_close() -> u32 {
    u32::from(super::process::should_close())
}
pub(super) unsafe extern "C" fn sys_program_class(path_ptr: *const u8, policy_raw: u32) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            3,
            path_ptr.cast(),
            core::ptr::null(),
            policy_raw,
            0,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_vm_decommit(region_id: u32, offset: u64, len: u64) -> i32 {
    unsafe { crate::program::vm::r4os_rust_program_vm_change(0, region_id, offset, len) }
}
pub(super) unsafe extern "C" fn sys_vm_release(region_id: u32) -> i32 {
    unsafe { crate::program::vm::r4os_rust_program_vm_change(1, region_id, 0, 0) }
}
pub(super) unsafe extern "C" fn sys_thread_create(
    entry: unsafe extern "C" fn(usize),
    arg: usize,
    stack_reserve_bytes: u64,
    flags: u32,
    out_thread_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::thread_runtime::r4os_rust_program_thread_create_legacy(
            (entry as usize) as _,
            arg,
            stack_reserve_bytes,
            flags,
            out_thread_id.cast(),
            thread_worker_entry,
        )
    }
}
pub(super) unsafe extern "C" fn sys_thread_exit(exit_code: i32) {
    unsafe { crate::program::thread_runtime::r4os_rust_program_thread_api_exit(exit_code) };
}
pub(super) unsafe extern "C" fn sys_thread_join(
    thread_id: u32,
    timeout_ticks: u64,
    out_exit_code: *mut i32,
) -> i32 {
    unsafe {
        crate::program::thread_join::r4os_rust_program_thread_join(
            thread_id,
            core::ptr::null(),
            false,
            timeout_ticks,
            out_exit_code.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_thread_current() -> u32 {
    unsafe { crate::program::threads::r4os_rust_program_thread_scalar(0) }
        .try_into()
        .expect("checked callback result")
}
pub(super) unsafe extern "C" fn sys_thread_status(
    thread_id: u32,
    out: *mut c::ProgramThreadInfo,
) -> i32 {
    unsafe {
        crate::program::thread_runtime::r4os_rust_program_thread_status(
            thread_id,
            core::ptr::null(),
            out.cast(),
            false,
        )
    }
}
pub(super) unsafe extern "C" fn sys_program_run(path_ptr: *const u8, args_ptr: *const u8) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            0,
            path_ptr.cast(),
            args_ptr.cast(),
            0,
            0,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_program_launch(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            0,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            0,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_program_spawn(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            1,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            0,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_program_kill(id: u32) -> i32 {
    unsafe { crate::program::process::r4os_rust_program_process_legacy(1, id) }
}
pub(super) unsafe extern "C" fn sys_program_request_close(id: u32) -> i32 {
    unsafe { crate::program::process::r4os_rust_program_process_legacy(0, id) }
}
pub(super) unsafe extern "C" fn sys_program_reap_instance(id: u32) -> i32 {
    unsafe { crate::program::process::r4os_rust_program_process_legacy(2, id) }
}
pub(super) unsafe extern "C" fn sys_program_spawn_handle(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
    out: *mut c::ProgramProcessHandle,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            2,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            0,
            out.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_thread_create_handle(
    entry: unsafe extern "C" fn(usize),
    arg: u64,
    stack_reserve_bytes: u64,
    flags: u32,
    out_handle: *mut c::ProgramJoinHandle,
) -> i32 {
    unsafe {
        crate::program::thread_runtime::r4os_rust_program_thread_create(
            (entry as usize) as _,
            arg,
            stack_reserve_bytes,
            flags,
            out_handle.cast(),
            thread_worker_entry,
        )
    }
}
pub(super) unsafe extern "C" fn sys_thread_handle_join(
    handle: *const c::ProgramJoinHandle,
    timeout_ticks: u64,
    out_exit_code: *mut i32,
) -> i32 {
    unsafe {
        crate::program::thread_join::r4os_rust_program_thread_join(
            0,
            handle.cast(),
            true,
            timeout_ticks,
            out_exit_code.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn sys_thread_handle_status(
    handle: *const c::ProgramJoinHandle,
    out: *mut c::ProgramThreadInfo,
) -> i32 {
    unsafe {
        crate::program::thread_runtime::r4os_rust_program_thread_status(
            0,
            handle.cast(),
            out.cast(),
            true,
        )
    }
}
pub(super) unsafe extern "C" fn sys_program_local_get(key: u64, output: *mut u64) -> i32 {
    unsafe {
        crate::program::process_data::r4os_rust_program_data_local(key, 0, output.cast(), false)
    }
}
pub(super) unsafe extern "C" fn sys_program_local_publish(
    key: u64,
    value: u64,
    output: *mut u64,
) -> i32 {
    unsafe {
        crate::program::process_data::r4os_rust_program_data_local(key, value, output.cast(), true)
    }
}
unsafe extern "C" fn thread_worker_entry() {
    unsafe { crate::program::entry::r4os_rust_program_entry_worker() }
}
