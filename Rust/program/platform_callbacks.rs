//! Exact public C callbacks for the native platform tables. No state owner.
use crate::contract as c;

pub(super) unsafe extern "C" fn sys_write(data: *const u8, len: u32) -> i32 {
    unsafe { crate::program::program_io::r4os_rust_program_io_write(1, data.cast(), len, false) }
}

pub(super) unsafe extern "C" fn sys_putc(ch: u8) {
    crate::program::program_io::r4os_rust_program_io_byte(1, ch, false);
}

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

pub(super) unsafe extern "C" fn sys_io_file_read(
    path: *const u8,
    out: *mut u8,
    max_len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            1,
            path.cast(),
            0,
            0,
            0,
            (out as usize) as _,
            max_len,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_read_at(
    path: *const u8,
    offset: u64,
    out: *mut u8,
    max_len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            2,
            path.cast(),
            offset,
            0,
            0,
            (out as usize) as _,
            max_len,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_write(
    path: *const u8,
    data: *const u8,
    len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            3,
            path.cast(),
            0,
            (data as usize) as _,
            len,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_append(
    path: *const u8,
    data: *const u8,
    len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            4,
            path.cast(),
            0,
            (data as usize) as _,
            len,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_stream_begin(
    path: *const u8,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            5,
            path.cast(),
            0,
            0,
            0,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_stream_write(
    path: *const u8,
    offset: u64,
    data: *const u8,
    len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            6,
            path.cast(),
            offset,
            (data as usize) as _,
            len,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_stream_finish(
    path: *const u8,
    expected_size: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            7,
            path.cast(),
            expected_size,
            0,
            0,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_stream_abort(
    path: *const u8,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            8,
            path.cast(),
            0,
            0,
            0,
            0,
            0,
            0,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_service_call(
    handle: u32,
    op: u16,
    request_ptr: *const u8,
    request_len: u32,
    response_header: *mut c::ServiceMessageHeader,
    response_ptr: *mut u8,
    response_capacity: u32,
    timeout_ticks: u64,
    flags: u32,
    out_request_id: *mut u32,
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
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_status(request_id: u32, out: *mut c::ProgramIoInfo) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_request(0, request_id, 0, out.cast())
    }
}

pub(super) unsafe extern "C" fn sys_io_wait(
    request_id: u32,
    timeout_ticks: u64,
    out: *mut c::ProgramIoInfo,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_request(
            1,
            request_id,
            timeout_ticks,
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_close(request_id: u32) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_request(
            2,
            request_id,
            0,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_system_halt() -> () {
    unsafe { crate::program::system::control::r4os_rust_sys_halt() }
}

pub(super) unsafe extern "C" fn sys_system_poweroff() -> () {
    unsafe { crate::program::system::control::r4os_rust_sys_poweroff() }
}

pub(super) unsafe extern "C" fn sys_file_read(path: *const u8, out: *mut u8, max_len: u32) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            1,
            path.cast(),
            0,
            0,
            0,
            (out as usize) as _,
            u64::from(max_len),
            0,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_write(path: *const u8, data: *const u8, len: u32) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            3,
            path.cast(),
            0,
            (data as usize) as _,
            u64::from(len),
            0,
            0,
            0,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_read_at(
    path: *const u8,
    offset: u32,
    out: *mut u8,
    max_len: u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            2,
            path.cast(),
            u64::from(offset),
            0,
            0,
            (out as usize) as _,
            u64::from(max_len),
            0,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_append(path: *const u8, data: *const u8, len: u32) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            4,
            path.cast(),
            0,
            (data as usize) as _,
            u64::from(len),
            0,
            0,
            0,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_stream_begin(path: *const u8, flags: u32) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            5,
            path.cast(),
            0,
            0,
            0,
            0,
            0,
            flags,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_stream_write(
    path: *const u8,
    offset: u64,
    data: *const u8,
    len: u32,
    flags: u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            6,
            path.cast(),
            offset,
            (data as usize) as _,
            u64::from(len),
            0,
            0,
            flags,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_stream_finish(
    path: *const u8,
    expected_size: u64,
    flags: u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            7,
            path.cast(),
            expected_size,
            0,
            0,
            0,
            0,
            flags,
            true,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_file_stream_abort(path: *const u8) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            8,
            path.cast(),
            0,
            0,
            0,
            0,
            0,
            0,
            true,
            core::ptr::null_mut(),
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

pub(super) unsafe extern "C" fn sys_file_replace_atomic(
    target_ptr: *const u8,
    staged_ptr: *const u8,
    backup_ptr: *const u8,
    flags: u32,
) -> i32 {
    unsafe {
        crate::program::system::atomic::r4os_rust_sys_file_replace(
            target_ptr.cast(),
            staged_ptr.cast(),
            backup_ptr.cast(),
            flags,
            true,
        )
    }
}

pub(super) unsafe extern "C" fn sys_boot_ready() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(3)
}

pub(super) unsafe extern "C" fn sys_io_file_write_at(
    path: *const u8,
    offset: u64,
    data: *const u8,
    len: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            10,
            path.cast(),
            offset,
            (data as usize) as _,
            len,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_info(
    path: *const u8,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            11,
            path.cast(),
            0,
            0,
            0,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_io_file_lock(
    path: *const u8,
    offset: u64,
    length: u64,
    flags: u32,
    out_request_id: *mut u32,
) -> i32 {
    unsafe {
        crate::program::async_io::r4os_rust_program_async_file(
            12,
            path.cast(),
            offset,
            0,
            length,
            0,
            0,
            flags,
            false,
            out_request_id.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_inventory(out: *mut c::StorageInventory) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            1,
            &crate::storage::operations::Args {
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_device(
    generation: u64,
    slot: u32,
    out: *mut c::StorageDeviceInfo,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            2,
            &crate::storage::operations::Args {
                generation,
                slot,
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_partition(
    generation: u64,
    device: *const c::StorageDeviceRef,
    slot: u32,
    out: *mut c::StoragePartitionInfo,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            3,
            &crate::storage::operations::Args {
                generation,
                device: device.cast(),
                slot,
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_volume(
    generation: u64,
    slot: u32,
    out: *mut c::StorageVolumeInfo,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            4,
            &crate::storage::operations::Args {
                generation,
                slot,
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_claim_begin(
    target: *const c::StorageTarget,
    out: *mut u64,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            5,
            &crate::storage::operations::Args {
                target: target.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_claim_end(id: u64, flags: u32) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            6,
            &crate::storage::operations::Args {
                id,
                flags,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_read(
    target: *const c::StorageTarget,
    relative: u64,
    sectors: u32,
    out: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            7,
            &crate::storage::operations::Args {
                target: target.cast(),
                relative,
                sectors,
                data: out.cast(),
                len,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_claim_read(
    id: u64,
    relative: u64,
    sectors: u32,
    data: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            8,
            &crate::storage::operations::Args {
                id,
                relative,
                sectors,
                data: data.cast(),
                len,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_claim_write(
    id: u64,
    relative: u64,
    sectors: u32,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            9,
            &crate::storage::operations::Args {
                id,
                relative,
                sectors,
                data: data.cast_mut().cast(),
                len,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_claim_flush(id: u64) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            10,
            &crate::storage::operations::Args {
                id,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_rescan(device: *const c::StorageDeviceRef) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            11,
            &crate::storage::operations::Args {
                device: device.cast(),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_mount(
    target: *const c::StorageTarget,
    letter_arg: u32,
    out: *mut c::StorageVolumeRef,
) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            12,
            &crate::storage::operations::Args {
                target: target.cast(),
                slot: letter_arg,
                ..Default::default()
            },
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_unmount(volume: *const c::StorageVolumeRef) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            13,
            &crate::storage::operations::Args {
                ..Default::default()
            },
            volume.cast_mut().cast(),
        )
    }
}

pub(super) unsafe extern "C" fn sys_storage_use_end(id: u64) -> i32 {
    unsafe {
        crate::storage::operations::r4os_rust_storage_operations(
            14,
            &crate::storage::operations::Args {
                id,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
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

pub(super) unsafe extern "C" fn desk_read_key() -> u8 {
    { crate::program::program_io::r4os_rust_program_io_key(true) }
        .try_into()
        .expect("checked callback result")
}

pub(super) unsafe extern "C" fn desk_read_key_codepoint() -> u32 {
    crate::program::program_io::r4os_rust_program_io_key(false)
}

pub(super) unsafe extern "C" fn desk_mouse_state(out: *mut c::Mouse) {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            0,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    };
}

pub(super) unsafe extern "C" fn desk_mouse_motion(out: *mut c::MouseMotion) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            1,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_mouse_show() {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            2,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn desk_mouse_hide() {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            3,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn desk_keyboard_layout_current(
    out: *mut c::KeyboardLayoutInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            5,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_keyboard_layout_at(
    index: u32,
    out: *mut c::KeyboardLayoutInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            6,
            &crate::program::r4desk::Args {
                value: index,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_keyboard_layout_set(layout_name: *const u8) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            7,
            &crate::program::r4desk::Args {
                input: layout_name.cast(),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_program_set_window(id: u32, window_id: i32) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_window(
            id,
            core::ptr::null(),
            window_id,
            false,
        )
    }
}

pub(super) unsafe extern "C" fn desk_program_window_id() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(0)
}

pub(super) unsafe extern "C" fn desk_gui_set_text(text: *const u8) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(0, text.cast(), 0, 0) }
}

pub(super) unsafe extern "C" fn desk_gui_text(id: u32, out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        crate::program::gui_events::r4os_rust_program_gui_string(id, out.cast(), capacity, false)
    }
}

pub(super) unsafe extern "C" fn desk_gui_set_title(title: *const u8) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(1, title.cast(), 0, 0) }
}

pub(super) unsafe extern "C" fn desk_gui_title(id: u32, out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        crate::program::gui_events::r4os_rust_program_gui_string(id, out.cast(), capacity, true)
    }
}

pub(super) unsafe extern "C" fn desk_gui_set_min_size(w: i32, h: i32) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(2, core::ptr::null(), w, h) }
}

pub(super) unsafe extern "C" fn desk_console_push_key(id: u32, key: u8) -> i32 {
    unsafe {
        crate::program::console_input::r4os_rust_program_console_input_api_push(
            id,
            [key].as_ptr(),
            1,
            true,
        )
    }
}

pub(super) unsafe extern "C" fn desk_console_write(
    stream_raw: u32,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        crate::program::program_io::r4os_rust_program_io_write(stream_raw, data.cast(), len, true)
    }
}

pub(super) unsafe extern "C" fn desk_clipboard_write(data: *const u8, len: u32) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            8,
            &crate::program::r4desk::Args {
                input: data.cast(),
                value: len,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_clipboard_read(out: *mut u8, capacity: u32) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            9,
            &crate::program::r4desk::Args {
                value: capacity,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_clipboard_revision() -> u32 {
    unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            10,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn desk_clipboard_info(out: *mut c::ClipboardInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            11,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_clipboard_clear() -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            12,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_frame_info(out: *mut c::RemoteFrameInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            13,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_frame_read(
    offset_pixels: u32,
    out: *mut u32,
    pixel_count: u32,
    out_info: *mut c::RemoteFrameInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            14,
            &crate::program::r4desk::Args {
                a: u64::from(offset_pixels),
                value: pixel_count,
                second: out_info.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_frame_wait(
    last_revision: u32,
    timeout_ticks: u64,
    out: *mut c::RemoteFrameInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            15,
            &crate::program::r4desk::Args {
                value: last_revision,
                a: timeout_ticks,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_input_push(event: *const c::RemoteInputEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            26,
            &crate::program::r4desk::Args {
                input: event.cast(),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_input_poll(out: *mut c::RemoteInputEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            27,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_input_status(out: *mut c::RemoteInputStatus) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            28,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_desktop_activity_wait(
    last_seq: u64,
    timeout_ticks: u64,
    out_seq: *mut u64,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            16,
            &crate::program::r4desk::Args {
                a: last_seq,
                b: timeout_ticks,
                ..Default::default()
            },
            out_seq.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_frame_map(out: *mut c::RemoteFrameMapInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            29,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_program_current_console_host() -> u32 {
    { crate::program::program_api::r4os_rust_program_api_action(1) }
        .try_into()
        .expect("checked callback result")
}

pub(super) unsafe extern "C" fn desk_program_request_desktop() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(2)
}

pub(super) unsafe extern "C" fn desk_program_spawn_with_console_host(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
    host_raw: u32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            1,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            host_raw,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn desk_program_spawn_with_console_host_handle(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
    host_raw: u32,
    out: *mut c::ProgramProcessHandle,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            2,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            host_raw,
            out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn desk_program_set_window_handle(
    handle_ptr: *const c::ProgramProcessHandle,
    window_id: i32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_window(
            0,
            handle_ptr.cast(),
            window_id,
            true,
        )
    }
}

pub(super) unsafe extern "C" fn desk_console_push_input(
    id: u32,
    data: *const u8,
    length: u32,
) -> i32 {
    unsafe {
        crate::program::console_input::r4os_rust_program_console_input_api_push(
            id,
            data.cast(),
            length,
            false,
        )
    }
}

pub(super) unsafe extern "C" fn desk_remote_frame_consumers() -> u32 {
    unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            23,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn desk_physical_key_poll(out: *mut c::PhysicalKeyEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            4,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_remote_frame_capture_stats(
    out: *mut c::RemoteFrameCaptureStats,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            32,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn desk_desktop_activity_notify() -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            17,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_screen_width() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            3,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn draw_screen_height() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            4,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn draw_clear(rgb: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            5,
            &crate::program::r4draw::Args {
                color: rgb,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn draw_rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            6,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                color: rgb,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn draw_text(x: i32, y: i32, value: *const u8, fg: u32, bg: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            7,
            &crate::program::r4draw::Args {
                x,
                y,
                input: value.cast(),
                color: fg,
                bg,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn draw_display_revision() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            8,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn draw_display_begin_frame() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            9,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_begin_frame_rect(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            10,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_present() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            11,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_blit_xrgb32(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pixels: *const u32,
    pixel_count: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            12,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                input: pixels.cast(),
                b: u64::from(pixel_count),
                a: u64::from(w),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_gui_draw_text(
    x: i32,
    y: i32,
    text: *const u8,
    fg: u32,
    bg: u32,
) -> i32 {
    unsafe {
        crate::program::gui_draw::r4os_rust_program_gui_draw_text(
            x,
            y,
            text.cast(),
            fg,
            bg,
            0,
            0,
            true,
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_draw_text_ex(
    x: i32,
    y: i32,
    text: *const u8,
    fg: u32,
    bg: u32,
    font_id: u32,
    flags: u32,
) -> i32 {
    unsafe {
        crate::program::gui_draw::r4os_rust_program_gui_draw_text(
            x,
            y,
            text.cast(),
            fg,
            bg,
            font_id,
            flags,
            false,
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_present() -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(3, core::ptr::null(), 0, 0) }
}

pub(super) unsafe extern "C" fn draw_font_count() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            22,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn draw_font_info(font_id: u32, out: *mut c::GuiFontInfo) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            23,
            &crate::program::r4draw::Args {
                w: font_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_font_measure(
    font_id: u32,
    value: *const u8,
    out: *mut c::GuiTextMetrics,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            24,
            &crate::program::r4draw::Args {
                w: font_id,
                input: value.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_text_font(
    font_id: u32,
    x: i32,
    y: i32,
    value: *const u8,
    fg: u32,
    bg: u32,
) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            29,
            &crate::program::r4draw::Args {
                w: font_id,
                x,
                y,
                input: value.cast(),
                color: fg,
                bg,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}

pub(super) unsafe extern "C" fn draw_font_reload() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            27,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_font_glyph_row(font_id: u32, codepoint: u32, row: u32) -> u64 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            25,
            &crate::program::r4draw::Args {
                w: font_id,
                h: codepoint,
                a: u64::from(row),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_begin() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(0, core::ptr::null(), 0)
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_commit() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(3, core::ptr::null(), 0)
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_cancel() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(4, core::ptr::null(), 0)
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_info(
    handle_ptr: *const c::ProgramProcessHandle,
    caller_out: *mut c::GuiFrameInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            0,
            handle_ptr.cast(),
            0,
            caller_out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_display_blit_xrgb32_stride(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pixels: *const u32,
    pixel_count: u32,
    source_stride_pixels: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            12,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                input: pixels.cast(),
                b: u64::from(pixel_count),
                a: u64::from(source_stride_pixels),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_present_regions(
    request: *const c::DisplayPresentRequest,
    pixels: *const u32,
    pixel_count: u32,
    regions: *const c::DisplayDamageRect,
    region_count: u32,
    out: *mut c::DisplayPresentResult,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            13,
            &crate::program::r4draw::Args {
                input: request.cast(),
                second: pixels.cast(),
                third: regions.cast(),
                b: u64::from(pixel_count),
                a: u64::from(region_count),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_present_capabilities(
    out: *mut c::DisplayPresentCapabilities,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            14,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_present_completion(
    fence: u64,
    out: *mut c::DisplayPresentCompletion,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            15,
            &crate::program::r4draw::Args {
                a: fence,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_presentation_stats(
    head_id: u32,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            19,
            &crate::program::r4draw::Args {
                w: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_presentation_info(
    head_id: u32,
    out: *mut c::DisplayPresentationInfo,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            20,
            &crate::program::r4draw::Args {
                w: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_presentation_feedback(
    head_id: u32,
    source: *const c::GfxFence,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            21,
            &crate::program::r4draw::Args {
                w: head_id,
                input: source.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_cursor_info(output: *mut c::DisplayCursorInfo) -> i32 {
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            1,
            core::ptr::null(),
            core::ptr::null(),
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_display_cursor_status(
    output: *mut c::DisplayCursorStatus,
) -> i32 {
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            2,
            core::ptr::null(),
            core::ptr::null(),
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_begin_damage(
    regions_ptr: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(
            1,
            regions_ptr.cast(),
            region_count,
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_generation_info(
    handle_ptr: *const c::ProgramProcessHandle,
    generation: u64,
    caller_out: *mut c::GuiFrameGenerationInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            2,
            handle_ptr.cast(),
            generation,
            caller_out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_font_glyph_bitmap(
    font_id: u32,
    codepoint: u32,
    out: *mut c::GuiGlyphBitmap,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            26,
            &crate::program::r4draw::Args {
                w: font_id,
                h: codepoint,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_font_revision() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            28,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}

pub(super) unsafe extern "C" fn draw_gui_frame_begin_replace(
    regions_ptr: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(
            2,
            regions_ptr.cast(),
            region_count,
        )
    }
}

pub(super) unsafe extern "C" fn draw_gui_frame_stream_info(
    handle_ptr: *const c::ProgramProcessHandle,
    caller_out: *mut c::GuiFrameStreamInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            1,
            handle_ptr.cast(),
            0,
            caller_out.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_buffer_stats(output: *mut c::GfxBufferStats) -> i32 {
    unsafe {
        crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(
            7,
            &crate::gfx::buffer_owner::Args {
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_memory_budget(
    input: *const c::GfxDeviceBudgetRequest,
    output: *mut c::GfxDeviceBudgetState,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::budget_api::r4os_rust_gfx_budget_api(&args, false, output.cast()) }
}

pub(super) unsafe extern "C" fn draw_gfx_telemetry(
    input: *const c::GfxTelemetryRequest,
    output: *mut c::GfxTelemetryState,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::telemetry_api::r4os_rust_gfx_telemetry_api(0, &args, output.cast()) }
}

pub(super) unsafe extern "C" fn draw_display_output_target(
    adapter_id: u32,
    head_id: u32,
    out: *mut c::GfxOutputTarget,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            16,
            &crate::program::r4draw::Args {
                w: adapter_id,
                h: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_output_presentation_info(
    input: *const c::GfxOutputTarget,
    out: *mut c::DisplayPresentationInfo,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            17,
            &crate::program::r4draw::Args {
                input: input.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_display_output_presentation_feedback(
    input: *const c::GfxOutputTarget,
    source: *const c::GfxFence,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            18,
            &crate::program::r4draw::Args {
                input: input.cast(),
                second: source.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}

pub(super) unsafe extern "C" fn draw_gfx_fence_query(
    input: *const c::GfxFence,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            7,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_fence_wait(
    input: *const c::GfxFence,
    timeout_ticks: u64,
    wait_for: u32,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            8,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                value: timeout_ticks,
                which: wait_for,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_queue_backend(
    index: u32,
    output: *mut c::GfxBackendBinding,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            0,
            &crate::program::gfx_queue_api::Args {
                which: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_queue_backend_info(
    index: u32,
    output: *mut c::GfxBackendInfo,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            1,
            &crate::program::gfx_queue_api::Args {
                which: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_queue_backend_properties(
    input: *const c::GfxBackendBinding,
    output: *mut c::GfxBackendProperties,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            2,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_revision(
    output: *mut c::GfxDisplayRevision,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            0,
            &crate::program::gfx_output_api::Args {
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_info(
    index: u32,
    output: *mut c::GfxOutputInfo,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            1,
            &crate::program::gfx_output_api::Args {
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_mode(
    input: *const c::GfxOutputId,
    index: u32,
    output: *mut c::GfxOutputMode,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            2,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_edid(
    input: *const c::GfxOutputId,
    index: u32,
    output: *mut c::GfxEdidBlock,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            3,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_color(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputColorState,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            4,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_refresh(
    input: *const c::GfxOutputTarget,
    output: *mut c::GfxOutputRefresh,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            5,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_power(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputPower,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            7,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_output_brightness(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputBrightness,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            15,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}

pub(super) unsafe extern "C" fn draw_gfx_atomic_status(
    ticket: u64,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            13,
            &crate::program::gfx_output_api::Args {
                ticket,
                ..Default::default()
            },
            output.cast(),
        )
    }
}

unsafe extern "C" fn thread_worker_entry() {
    unsafe { crate::program::entry::r4os_rust_program_entry_worker() }
}
