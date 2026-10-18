//! Exact memory operations of the parent owner; no duplicated state.
use super::*;

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_create(
    input: *const a::GfxBufferDescriptor,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_buffer_create(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_describe(
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferDescriptor,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_buffer_describe(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_import(
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_buffer_import(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_release(
    input: *const a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_buffer_release(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_map(
    input: *const a::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut a::GfxBufferMap,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_buffer_map(
            ptr::addr_of!(identity),
            input,
            access,
            offset,
            bytes,
            output,
        )
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_buffer_unmap(
    input: *const a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_buffer_unmap(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_device_acquire(
    reference: *const a::GfxBufferHandle,
    request: *const a::GfxDeviceRequest,
    output: *mut a::GfxDeviceLease,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_device_acquire(ptr::addr_of!(identity), reference, request, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_device_segment(
    lease: *const a::GfxDeviceLease,
    offset: u64,
    output: *mut a::GfxDmaSegment,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_device_segment(ptr::addr_of!(identity), lease, offset, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_device_release(
    lease: *const a::GfxDeviceLease,
    quiesced: u32,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_device_release(ptr::addr_of!(identity), lease, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mmio_map(
    request: *const a::GfxMmioRequest,
    output: *mut a::GfxMmioWindow,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mmio_map(ptr::addr_of!(identity), request, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mmio_unmap(
    window: *const a::GfxBufferHandle,
    quiesced: u32,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mmio_unmap(ptr::addr_of!(identity), window, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_collect() -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_collect(ptr::addr_of!(identity), held()) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_owned_reserve(
    input: *const a::GfxBufferDescriptor,
    cookie: u64,
    output: *mut a::GfxOwnedBufferReservation,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_owned_reserve(ptr::addr_of!(identity), input, cookie, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_owned_commit(
    input: *const a::GfxOwnedBufferReservation,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_owned_commit(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_owned_abort(
    input: *const a::GfxOwnedBufferReservation,
    quiesced: u32,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_owned_abort(ptr::addr_of!(identity), input, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_owned_take_release(
    adapter: u32,
    generation: u64,
    output: *mut a::GfxOwnedBufferRelease,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_owned_take_release(ptr::addr_of!(identity), adapter, generation, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_owned_finish_release(
    input: *const a::GfxOwnedBufferRelease,
    quiesced: u32,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_owned_finish_release(ptr::addr_of!(identity), input, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_virtual_register(
    input: *const a::GfxNativeProvider,
    output: *mut a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_virtual_register(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_virtual_unregister(
    input: *const a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_virtual_unregister(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_virtual_take(
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxVirtualJob,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_virtual_take(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_virtual_complete(
    input: *const a::GfxBufferHandle,
    completion: *const a::GfxVirtualCompletion,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_virtual_complete(ptr::addr_of!(identity), input, completion) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_native_register(
    input: *const a::GfxNativeProvider,
    output: *mut a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_native_register(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_native_unregister(
    input: *const a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_native_unregister(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_native_take(
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxNativeJob,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_native_take(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_native_complete(
    provider: *const a::GfxBufferHandle,
    request: *const a::GfxBufferHandle,
    result: i32,
    reference: *const a::GfxBufferHandle,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_native_complete(
            ptr::addr_of!(identity),
            provider,
            request,
            result,
            reference,
        )
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_memory_stats(output: *mut a::GfxBufferStats) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_memory_stats(ptr::addr_of!(identity), output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_memory_budget(
    input: *const a::GfxDeviceBudgetRequest,
    output: *mut a::GfxDeviceBudgetState,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_memory_budget(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_telemetry_exchange(
    input: *const a::GfxTelemetryState,
    output: *mut a::GfxTelemetryDemand,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_telemetry_exchange(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_device_lost(
    adapter: u32,
    generation: u64,
    quiesced: u32,
) -> i32 {
    let identity = match identity(0, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_device_lost(ptr::addr_of!(identity), adapter, generation, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_reserved_span(base: u64, bytes: u64) -> i32 {
    if let Err(code) = identity(0, true) {
        return code;
    }
    if base == 0
        || bytes == 0
        || (base | bytes) & 4095 != 0
        || base >= (1u64 << 52)
        || bytes > (1u64 << 52) - base
    {
        return a::gfx_buffer_error_invalid;
    }
    if unsafe { driver_gfx_span(base, bytes, false) } {
        a::gfx_buffer_result_ok
    } else {
        a::gfx_buffer_error_unsupported
    }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_unmanaged_span(base: u64, bytes: u64) -> i32 {
    if let Err(code) = identity(0, true) {
        return code;
    }
    if base == 0
        || bytes == 0
        || (base | bytes) & 4095 != 0
        || base >= (1u64 << 52)
        || bytes > (1u64 << 52) - base
    {
        return a::gfx_buffer_error_invalid;
    }
    if unsafe { driver_gfx_span(base, bytes, true) } {
        a::gfx_buffer_result_ok
    } else {
        a::gfx_buffer_error_unsupported
    }
}

pub(super) unsafe fn driver_gfx_buffer_create(
    identity: *const Identity,
    input: *const a::GfxBufferDescriptor,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(0, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_buffer_describe(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferDescriptor,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(1, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_buffer_import(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(2, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_buffer_release(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(3, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn driver_gfx_buffer_map(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut a::GfxBufferMap,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        access,
        offset,
        bytes,
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(4, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_buffer_unmap(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(6, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn driver_gfx_native_register(
    identity: *const Identity,
    input: *const a::GfxNativeProvider,
    output: *mut a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(true, 0, &args, output.cast())
    }
}

pub(super) unsafe fn driver_gfx_native_unregister(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(
            true,
            1,
            &args,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe fn driver_gfx_native_take(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxNativeJob,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(true, 2, &args, output.cast())
    }
}

pub(super) unsafe fn driver_gfx_native_complete(
    identity: *const Identity,
    provider: *const a::GfxBufferHandle,
    request: *const a::GfxBufferHandle,
    result: i32,
    reference: *const a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: provider.cast(),
        other_input: request.cast(),
        access: result as u32,
        buffer: reference.cast_mut().cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::allocation_api::r4os_rust_gfx_allocation_api(
            true,
            3,
            &args,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe fn driver_gfx_virtual_register(
    identity: *const Identity,
    input: *const a::GfxNativeProvider,
    output: *mut a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(true, 0, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_virtual_unregister(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(true, 1, &args, core::ptr::null_mut())
    }
}

pub(super) unsafe fn driver_gfx_virtual_take(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxVirtualJob,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(true, 2, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_virtual_complete(
    identity: *const Identity,
    input: *const a::GfxBufferHandle,
    completion: *const a::GfxVirtualCompletion,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        other_input: completion.cast(),
        ..Default::default()
    };
    unsafe {
        crate::gfx::virtual_api::r4os_rust_gfx_virtual_api(true, 3, &args, core::ptr::null_mut())
    }
}

pub(super) unsafe fn driver_gfx_memory_stats(
    identity: *const Identity,
    output: *mut a::GfxBufferStats,
) -> i32 {
    let _ = identity;
    unsafe {
        crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(7, &Default::default(), output.cast())
    }
}

pub(super) unsafe fn driver_gfx_device_acquire(
    identity: *const Identity,
    reference: *const a::GfxBufferHandle,
    request: *const a::GfxDeviceRequest,
    output: *mut a::GfxDeviceLease,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: reference.cast(),
        other_input: request.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(7, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_device_segment(
    identity: *const Identity,
    lease: *const a::GfxDeviceLease,
    offset: u64,
    output: *mut a::GfxDmaSegment,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: lease.cast(),
        offset,
        ..Default::default()
    };
    unsafe { crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(8, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_device_release(
    identity: *const Identity,
    lease: *const a::GfxDeviceLease,
    quiesced: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: lease.cast(),
        value: u64::from(quiesced),
        ..Default::default()
    };
    unsafe {
        crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(9, &args, core::ptr::null_mut())
    }
}

pub(super) unsafe fn driver_gfx_mmio_map(
    identity: *const Identity,
    request: *const a::GfxMmioRequest,
    output: *mut a::GfxMmioWindow,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: request.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(10, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_mmio_unmap(
    identity: *const Identity,
    window: *const a::GfxBufferHandle,
    quiesced: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: window.cast(),
        value: u64::from(quiesced),
        ..Default::default()
    };
    unsafe {
        crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(11, &args, core::ptr::null_mut())
    }
}

pub(super) unsafe fn driver_gfx_collect(identity: *const Identity, full: bool) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        ..Default::default()
    };
    unsafe {
        crate::gfx::driver_memory::r4os_rust_gfx_driver_memory_run(
            if full { 12 } else { 13 },
            &args,
            core::ptr::null_mut(),
        )
    }
}

pub(super) unsafe fn driver_gfx_device_lost(
    identity: *const Identity,
    adapter: u32,
    generation: u64,
    quiesced: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        access: adapter,
        offset: generation,
        value: u64::from(quiesced),
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(0, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn driver_gfx_owned_reserve(
    identity: *const Identity,
    input: *const a::GfxBufferDescriptor,
    cookie: u64,
    output: *mut a::GfxOwnedBufferReservation,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        value: cookie,
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(1, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_owned_commit(
    identity: *const Identity,
    input: *const a::GfxOwnedBufferReservation,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(2, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_owned_abort(
    identity: *const Identity,
    input: *const a::GfxOwnedBufferReservation,
    quiesced: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        value: u64::from(quiesced),
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(3, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn driver_gfx_owned_take_release(
    identity: *const Identity,
    adapter: u32,
    generation: u64,
    output: *mut a::GfxOwnedBufferRelease,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        access: adapter,
        offset: generation,
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(4, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_owned_finish_release(
    identity: *const Identity,
    input: *const a::GfxOwnedBufferRelease,
    quiesced: u32,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        value: u64::from(quiesced),
        ..Default::default()
    };
    unsafe { crate::gfx::owned_buffers::r4os_rust_gfx_owned_api(5, &args, core::ptr::null_mut()) }
}

pub(super) unsafe fn driver_gfx_telemetry_exchange(
    identity: *const Identity,
    input: *const a::GfxTelemetryState,
    output: *mut a::GfxTelemetryDemand,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::telemetry_api::r4os_rust_gfx_telemetry_api(1, &args, output.cast()) }
}

pub(super) unsafe fn driver_gfx_memory_budget(
    identity: *const Identity,
    input: *const a::GfxDeviceBudgetRequest,
    output: *mut a::GfxDeviceBudgetState,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::budget_api::r4os_rust_gfx_budget_api(&args, true, output) }
}

pub(super) unsafe fn driver_gfx_span(base: u64, bytes: u64, unmanaged: bool) -> bool {
    let info = unsafe { &*crate::boot::info::r4os_rust_boot_info() };
    info.flags & 1 != 0
        && info.flags & 2 == 0
        && info.invalid_entries == 0
        && unsafe {
            crate::memory::map::r4os_rust_device_extent(
                base,
                bytes,
                info.memory,
                info.memory_count,
                unmanaged,
            )
        }
}
