//! Direct async_io platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

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
