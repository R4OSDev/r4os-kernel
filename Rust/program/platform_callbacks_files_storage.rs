//! Direct files_storage platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

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
