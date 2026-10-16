//! Original graphics entrypoint ownership. Cached tables carry no identity;
//! admission resolves the exact driver epoch again for each public callback.
use crate::{
    contract as a,
    driver::{api_admission as admission, heap, irq_router, threads},
    sched::scheduler,
};
use core::{mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Identity {
    pub id: u64,
    pub generation: u64,
}

pub fn identity(mode: u8, admit: bool) -> Result<Identity, i32> {
    if mode == 1
        && (admission::r4os_rust_driver_api_owner_scalar(0) == 0
            || admission::r4os_rust_driver_api_owner_scalar(1) == 0)
    {
        return Err(a::gfx_buffer_error_invalid);
    }
    if irq_router::r4os_rust_irq_router_local(false) != 0
        || (mode != 3 && admission::r4os_rust_driver_api_storage_owner() != 0)
        || threads::r4os_rust_driver_thread_current(0, 0) != 0
        || (mode < 2 && scheduler::current().is_null())
    {
        return Err(a::gfx_buffer_error_invalid);
    }
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    if owner == 0 {
        return Err(a::gfx_buffer_error_invalid);
    }
    if mode < 2 {
        let mut out = MaybeUninit::uninit();
        let code = unsafe { driver_buffer_identity(owner, admit, out.as_mut_ptr()) };
        return if code == 0 {
            Ok(unsafe { out.assume_init() })
        } else {
            Err(code)
        };
    }
    let mut snapshot = a::DriverHeapStats::default();
    if unsafe { heap::r4os_rust_driver_heap_stats(owner, &mut snapshot) } != a::driver_heap_ok
        || snapshot.owner_epoch == 0
    {
        return Err(a::gfx_buffer_error_stale);
    }
    if admit && snapshot.closing != 0 {
        return Err(a::gfx_buffer_error_closed);
    }
    Ok(Identity {
        id: u64::from(owner),
        generation: snapshot.owner_epoch,
    })
}
/// # Safety
/// Output is an aligned resident private identity record, exclusive for this
/// call. The backend retains its exact driver epoch until confirmed cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_identity(
    mode: u8,
    admit: bool,
    out: *mut Identity,
) -> i32 {
    match identity(mode, admit) {
        Ok(value) => {
            unsafe { ptr::write(out, value) };
            0
        }
        Err(code) => code,
    }
}
fn held() -> bool {
    admission::r4os_rust_driver_api_owner_scalar(0) != 0
        && admission::r4os_rust_driver_api_owner_scalar(1) != 0
}
fn invalid(irq: bool) -> bool {
    irq && irq_router::r4os_rust_irq_router_local(false) != 0
}
/// # Safety
/// Pointer is resident and readable for the schema header after nonzero and
/// alignment admission; size checks never form references beyond its prefix.
pub unsafe fn header<T>(p: *const T, aligned: bool) -> bool {
    if p.is_null() || aligned && !(p as usize).is_multiple_of(core::mem::align_of::<T>()) {
        return false;
    }
    let h = p.cast::<u32>();
    unsafe { *h == 1 && *h.add(1) >= core::mem::size_of::<T>() as u32 }
}
const _: () = {
    assert!(core::mem::size_of::<Identity>() == 16);
};

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_boot_hold(
    input: *const a::GfxBootHoldRequest,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_boot_hold(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_cursor_configure(
    input: *const a::DisplayCursorInfo,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_cursor_configure(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_cursor_take(
    backend: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverCursorJob,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_cursor_take(ptr::addr_of!(identity), backend, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_cursor_complete(
    input: *const a::GfxDriverCursorCompletion,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_cursor_complete(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_boot_finish(
    generation: u64,
    operation: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_boot_finish(ptr::addr_of!(identity), generation, operation, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_boot_info(
    output: *mut a::GfxNativeBootInfo,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_display_boot_info(ptr::addr_of!(identity), output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_prepare(
    input: *const a::GfxNativeRegistration,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_display_prepare(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_prepare_held(
    input: *const a::GfxNativeRegistration,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_display_prepare_held(ptr::addr_of!(identity), input, generation, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_transition(
    generation: u64,
    operation: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_display_transition(ptr::addr_of!(identity), generation, operation, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_device_reset(
    input: *const a::GfxBackendBinding,
    generation: u64,
    quiesced: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_display_device_reset(
            ptr::addr_of!(identity),
            input,
            generation,
            quiesced,
            output,
        )
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_resume_headless(
    input: *const a::GfxBackendBinding,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_display_resume_headless(ptr::addr_of!(identity), input, generation, output)
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_terminal_release(
    input: *const a::GfxBackendBinding,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_display_terminal_release(ptr::addr_of!(identity), input, generation, output)
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_prepare_reset(
    input: *const a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe {
        driver_gfx_display_prepare_reset(
            ptr::addr_of!(identity),
            input,
            held_generation,
            reset_generation,
            output,
        )
    }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_query(
    output: *mut a::GfxDriverOutputApi,
) -> i32 {
    let identity = match identity(2, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_query(ptr::addr_of!(identity), output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_register(
    input: *const a::GfxBackendRegistration,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_queue_register(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_register_profile(
    input: *const a::GfxBackendRegistration,
    profile: *const a::GfxBackendProfile,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_queue_register_profile(ptr::addr_of!(identity), input, profile, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_publish_properties(
    input: *const a::GfxBackendBinding,
    properties: *const a::GfxBackendProperties,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_queue_publish_properties(ptr::addr_of!(identity), input, properties) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_native_info(
    input: *const a::GfxFence,
    output: *mut a::GfxNativeJobInfo,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_native_info(owner, input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_native_data(
    input: *const a::GfxFence,
    offset: u32,
    output: *mut u8,
    count: u32,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_native_data(owner, input, offset, output, count) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_native_binding(
    input: *const a::GfxFence,
    index: u32,
    output: *mut a::GfxNativeBinding,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_native_binding(owner, input, index, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_owner_info(
    input: *const a::GfxBackendBinding,
    timeline: u64,
    output: *mut a::GfxQueueOwnerInfo,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_owner_info(owner, input, timeline, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_unregister(
    input: *const a::GfxBackendBinding,
    quiesced: u32,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_unregister(owner, input, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_take(
    input: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverJob,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_take(owner, input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_render_list(
    input: *const a::GfxFence,
    output: *mut a::GfxRenderList,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_render_list(owner, input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_render_grid_list(
    input: *const a::GfxFence,
    output: *mut a::GfxRenderGridList,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_render_grid_list(owner, input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_update_operations(
    input: *const a::GfxBackendBinding,
    operations: u64,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_update_operations(owner, input, operations) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_read_render_color_list(
    input: *const a::GfxFence,
    output: *mut a::GfxRenderColorList,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_read_render_color_list(owner, input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_complete(
    input: *const a::GfxFence,
    result: u32,
    quiesced: u32,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_complete(owner, input, result, quiesced) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_reset(
    input: *const a::GfxBackendBinding,
    quiesced: u32,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_reset(owner, input, quiesced, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_segment(
    input: *const a::GfxFence,
    which: u32,
    offset: u64,
    mask: u64,
    output: *mut a::GfxDmaSegment,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_segment(owner, input, which, offset, mask, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_retain(
    input: *const a::GfxFence,
    which: u32,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_queue_retain(ptr::addr_of!(identity), input, which, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_retain_scanout(
    input: *const a::GfxFence,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let identity = match identity(0, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_queue_retain_scanout(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_begin_scanout(
    input: *const a::GfxFence,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_begin_scanout(owner, input) }
}

/// # Safety
/// Resident Contract buffers retain their declared prefixes and exact driver
/// lifetime through the original synchronous backend operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_scanout_retire_requested(
    input: *const a::GfxFence,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_queue_scanout_retire_requested(owner, input) }
}

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
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_power_publish(
    input: *const a::GfxOutputPower,
) -> i32 {
    if input.is_null()
        || invalid(true)
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputPower>())
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_power_publish(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_refresh_publish(
    input: *const a::GfxOutputRefresh,
) -> i32 {
    if input.is_null()
        || invalid(true)
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputRefresh>())
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_refresh_publish(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_brightness_publish(
    input: *const a::GfxOutputBrightness,
) -> i32 {
    if input.is_null()
        || invalid(true)
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputBrightness>())
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_brightness_publish(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_enable(
    input: *const a::GfxBackendBinding,
) -> i32 {
    if input.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mode_enable(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_complete(
    input: *const a::GfxDriverModeCompletion,
) -> i32 {
    if input.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mode_complete(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_receiver_close(
    input: *const a::GfxReceiverSource,
) -> i32 {
    if input.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(2, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_receiver_close(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_power_read(
    input: *const a::GfxOutputId,
    output: *mut a::GfxPowerRequest,
) -> i32 {
    if input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputId>())
        || !unsafe { header(output, false) }
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_power_read(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_refresh_read(
    input: *const a::GfxOutputTarget,
    output: *mut a::GfxRefreshRequest,
) -> i32 {
    if input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputTarget>())
        || !unsafe { header(output, false) }
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_refresh_read(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_brightness_read(
    input: *const a::GfxOutputId,
    output: *mut a::GfxBrightnessRequest,
) -> i32 {
    if input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputId>())
        || !unsafe { header(output, false) }
    {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_brightness_read(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_output_register(
    input: *const a::GfxAdditionalOutput,
    output: *mut a::GfxOutputTarget,
) -> i32 {
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    if input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxAdditionalOutput>())
        || !unsafe { header(output, false) }
        || unsafe { (*output).size } != core::mem::size_of::<a::GfxOutputTarget>() as u32
    {
        return a::gfx_output_error_invalid;
    }
    unsafe { driver_gfx_display_output_register(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_output_transition(
    input: *const a::GfxOutputTarget,
    operation: u32,
    quiesced: u32,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    if input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputTarget>())
    {
        return a::gfx_output_error_invalid;
    }
    unsafe {
        driver_gfx_display_output_transition(ptr::addr_of!(identity), input, operation, quiesced)
    }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_read_color(
    ticket: u64,
    sequence: u64,
    output: *mut a::GfxDriverModeColor,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    if !unsafe { header(output, false) } {
        return a::gfx_output_error_invalid;
    }
    unsafe { driver_gfx_mode_read_color(ptr::addr_of!(identity), ticket, sequence, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_pause(
    input: *const a::GfxOutputId,
    paused: u32,
) -> i32 {
    if input.is_null() || paused > 1 || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_pause(ptr::addr_of!(identity), input, paused) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_restore(
    input: *const a::GfxAtomicState,
    output: *mut a::GfxModeStatus,
) -> i32 {
    if input.is_null() || !unsafe { header(output, false) } || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mode_restore(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_status(
    ticket: u64,
    output: *mut a::GfxModeStatus,
) -> i32 {
    if !unsafe { header(output, false) } || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mode_status(ptr::addr_of!(identity), ticket, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_audio_publish(input: *const a::GfxAudioRoute) -> i32 {
    if input.is_null() || invalid(true) || !unsafe { header(input, false) } {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(2, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_audio_publish(ptr::addr_of!(identity), input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_audio_query(
    location: u32,
    device: u32,
    index: u32,
    output: *mut a::GfxAudioRoute,
) -> i32 {
    if !unsafe { header(output, false) } || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(3, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_audio_query(ptr::addr_of!(identity), location, device, index, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_mode_take(
    input: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverModeJob,
) -> i32 {
    if input.is_null() || !unsafe { header(output, false) } || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_mode_take(ptr::addr_of!(identity), input, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_receiver_replace(
    input: *const a::GfxReceiverUpdate,
) -> i32 {
    if input.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let value = unsafe { ptr::read(input) };
    let identity = match identity(2, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_receiver_replace(ptr::addr_of!(identity), ptr::addr_of!(value)) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_publish(
    input: *const a::GfxOutputPublication,
    output: *mut a::GfxOutputId,
) -> i32 {
    if input.is_null() || invalid(true) || output.is_null() {
        return a::gfx_output_error_invalid;
    }
    let value = unsafe { ptr::read(input) };
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_output_publish(owner, ptr::addr_of!(value), output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_withdraw(input: *const a::GfxOutputId) -> i32 {
    if input.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_output_withdraw(owner, input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_schedule(
    input: *const a::GfxBackendBinding,
) -> i32 {
    if !unsafe { header(input, false) } {
        return a::gfx_queue_error_invalid;
    }
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    unsafe { driver_gfx_display_schedule(owner, input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_receiver_register(
    adapter: u32,
    output: *mut a::GfxReceiverSource,
) -> i32 {
    if output.is_null() || invalid(true) {
        return a::gfx_output_error_invalid;
    }
    let epoch = { crate::display::outputs::receiver_epoch() };
    let identity = match identity(2, true) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_receiver_register(ptr::addr_of!(identity), epoch, adapter, output) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_presentation_stats(
    input: *const a::DisplayPresentationStats,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    if !unsafe { header(input, false) } {
        return a::gfx_output_error_invalid;
    }
    let result =
        unsafe { driver_gfx_display_presentation_stats(ptr::addr_of!(identity), 0, input) };
    if result != 0 {
        return result;
    }
    unsafe { driver_gfx_display_presentation_stats(ptr::addr_of!(identity), 1, input) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_presentation_info(
    input: *const a::DisplayPresentationInfo,
) -> i32 {
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    if !unsafe { header(input, false) } {
        return a::gfx_output_error_invalid;
    }
    let result = unsafe { driver_gfx_display_presentation_info(ptr::addr_of!(identity), 0, input) };
    if result != 0 {
        return result;
    }
    unsafe { driver_gfx_display_presentation_info(ptr::addr_of!(identity), 1, input) }
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

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_output_color_publish(
    input: *const a::GfxOutputColorState,
) -> i32 {
    if invalid(true)
        || input.is_null()
        || !(input as usize).is_multiple_of(core::mem::align_of::<a::GfxOutputColorState>())
    {
        return a::gfx_output_error_invalid;
    }
    let h = input.cast::<u32>();
    if unsafe { *h } != 1 || unsafe { *h.add(1) } < 128 {
        return a::gfx_output_error_invalid;
    }
    let count = if unsafe { *h.add(1) } >= 192 {
        192
    } else {
        128
    };
    let mut value = a::GfxOutputColorState::default();
    unsafe {
        ptr::copy_nonoverlapping(
            input.cast::<u8>(),
            ptr::addr_of_mut!(value).cast::<u8>(),
            count,
        )
    };
    value.size = 192;
    let identity = match identity(1, false) {
        Ok(value) => value,
        Err(code) => return code,
    };
    unsafe { driver_gfx_output_color_publish(ptr::addr_of!(identity), ptr::addr_of!(value)) }
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_display_query(
    output: *mut a::GfxDriverDisplayApi,
) -> i32 {
    if let Err(code) = identity(1, true) {
        return code;
    }
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 40
    {
        return a::gfx_output_error_invalid;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverDisplayApi>());
    let value = a::GfxDriverDisplayApi {
        size: bytes as u32,
        boot_info: r4os_rust_driver_gfx_display_boot_info as *const () as u64,
        prepare: r4os_rust_driver_gfx_display_prepare as *const () as u64,
        transition: r4os_rust_driver_gfx_display_transition as *const () as u64,
        schedule: r4os_rust_driver_gfx_display_schedule as *const () as u64,
        boot_hold: r4os_rust_driver_gfx_boot_hold as *const () as u64,
        boot_finish: r4os_rust_driver_gfx_boot_finish as *const () as u64,
        prepare_held: r4os_rust_driver_gfx_display_prepare_held as *const () as u64,
        presentation_stats: r4os_rust_driver_gfx_display_presentation_stats as *const () as u64,
        cursor_configure: r4os_rust_driver_gfx_cursor_configure as *const () as u64,
        cursor_take: r4os_rust_driver_gfx_cursor_take as *const () as u64,
        cursor_complete: r4os_rust_driver_gfx_cursor_complete as *const () as u64,
        presentation_info: r4os_rust_driver_gfx_display_presentation_info as *const () as u64,
        output_register: r4os_rust_driver_gfx_display_output_register as *const () as u64,
        output_transition: r4os_rust_driver_gfx_display_output_transition as *const () as u64,
        device_reset: r4os_rust_driver_gfx_display_device_reset as *const () as u64,
        prepare_reset: r4os_rust_driver_gfx_display_prepare_reset as *const () as u64,
        terminal_release: r4os_rust_driver_gfx_display_terminal_release as *const () as u64,
        resume_headless: r4os_rust_driver_gfx_display_resume_headless as *const () as u64,
        ..a::GfxDriverDisplayApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_output_ok
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_queue_query(
    output: *mut a::GfxDriverQueueApi,
) -> i32 {
    if let Err(code) = identity(0, true) {
        return code;
    }
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 56
    {
        return a::gfx_queue_error_invalid;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverQueueApi>());
    let value = a::GfxDriverQueueApi {
        size: bytes as u32,
        register_backend: r4os_rust_driver_gfx_queue_register as *const () as u64,
        unregister_backend: r4os_rust_driver_gfx_queue_unregister as *const () as u64,
        take: r4os_rust_driver_gfx_queue_take as *const () as u64,
        complete: r4os_rust_driver_gfx_queue_complete as *const () as u64,
        reset: r4os_rust_driver_gfx_queue_reset as *const () as u64,
        segment: r4os_rust_driver_gfx_queue_segment as *const () as u64,
        retain_resource: r4os_rust_driver_gfx_queue_retain as *const () as u64,
        register_profile: r4os_rust_driver_gfx_queue_register_profile as *const () as u64,
        update_operations: r4os_rust_driver_gfx_queue_update_operations as *const () as u64,
        read_render_list: r4os_rust_driver_gfx_queue_read_render_list as *const () as u64,
        read_render_grid_list: r4os_rust_driver_gfx_queue_read_render_grid_list as *const () as u64,
        read_render_color_list: r4os_rust_driver_gfx_queue_read_render_color_list as *const ()
            as u64,
        publish_properties: r4os_rust_driver_gfx_queue_publish_properties as *const () as u64,
        read_native_info: r4os_rust_driver_gfx_queue_read_native_info as *const () as u64,
        read_native_data: r4os_rust_driver_gfx_queue_read_native_data as *const () as u64,
        read_native_binding: r4os_rust_driver_gfx_queue_read_native_binding as *const () as u64,
        queue_owner_info: r4os_rust_driver_gfx_queue_owner_info as *const () as u64,
        retain_scanout: r4os_rust_driver_gfx_queue_retain_scanout as *const () as u64,
        begin_scanout: r4os_rust_driver_gfx_queue_begin_scanout as *const () as u64,
        scanout_retire_requested: r4os_rust_driver_gfx_queue_scanout_retire_requested as *const ()
            as u64,
        ..a::GfxDriverQueueApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_queue_ok
}

/// # Safety
/// Resident Contract buffers retain their declared C prefixes through the
/// original operation. Admission and output publication preserve their order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_gfx_memory_query(
    output: *mut a::GfxDriverMemoryApi,
) -> i32 {
    if output.is_null()
        || unsafe { *output.cast::<u32>() } != 1
        || unsafe { *output.cast::<u32>().add(1) } < 112
    {
        return a::gfx_buffer_error_invalid;
    }
    if let Err(code) = identity(0, true) {
        return code;
    }
    let bytes = (unsafe { *output.cast::<u32>().add(1) } as usize & !7)
        .min(core::mem::size_of::<a::GfxDriverMemoryApi>());
    let value = a::GfxDriverMemoryApi {
        size: bytes as u32,
        buffer_create: r4os_rust_driver_gfx_buffer_create as *const () as u64,
        buffer_describe: r4os_rust_driver_gfx_buffer_describe as *const () as u64,
        buffer_import: r4os_rust_driver_gfx_buffer_import as *const () as u64,
        buffer_release: r4os_rust_driver_gfx_buffer_release as *const () as u64,
        buffer_map: r4os_rust_driver_gfx_buffer_map as *const () as u64,
        buffer_unmap: r4os_rust_driver_gfx_buffer_unmap as *const () as u64,
        device_acquire: r4os_rust_driver_gfx_device_acquire as *const () as u64,
        device_segment: r4os_rust_driver_gfx_device_segment as *const () as u64,
        device_release: r4os_rust_driver_gfx_device_release as *const () as u64,
        mmio_map: if held() {
            r4os_rust_driver_gfx_mmio_map as *const () as u64
        } else {
            0
        },
        mmio_unmap: if held() {
            r4os_rust_driver_gfx_mmio_unmap as *const () as u64
        } else {
            0
        },
        collect: r4os_rust_driver_gfx_collect as *const () as u64,
        buffer_stats: r4os_rust_driver_gfx_memory_stats as *const () as u64,
        buffer_reserve: r4os_rust_driver_gfx_owned_reserve as *const () as u64,
        buffer_commit: r4os_rust_driver_gfx_owned_commit as *const () as u64,
        buffer_abort: r4os_rust_driver_gfx_owned_abort as *const () as u64,
        buffer_take_release: r4os_rust_driver_gfx_owned_take_release as *const () as u64,
        buffer_finish_release: r4os_rust_driver_gfx_owned_finish_release as *const () as u64,
        native_register: r4os_rust_driver_gfx_native_register as *const () as u64,
        native_unregister: r4os_rust_driver_gfx_native_unregister as *const () as u64,
        native_take: r4os_rust_driver_gfx_native_take as *const () as u64,
        native_complete: r4os_rust_driver_gfx_native_complete as *const () as u64,
        virtual_register: r4os_rust_driver_gfx_virtual_register as *const () as u64,
        virtual_unregister: r4os_rust_driver_gfx_virtual_unregister as *const () as u64,
        virtual_take: r4os_rust_driver_gfx_virtual_take as *const () as u64,
        virtual_complete: r4os_rust_driver_gfx_virtual_complete as *const () as u64,
        memory_budget: r4os_rust_driver_gfx_memory_budget as *const () as u64,
        telemetry_exchange: r4os_rust_driver_gfx_telemetry_exchange as *const () as u64,
        device_lost: r4os_rust_driver_gfx_device_lost as *const () as u64,
        reserved_span: r4os_rust_driver_gfx_reserved_span as *const () as u64,
        unmanaged_span: r4os_rust_driver_gfx_unmanaged_span as *const () as u64,
        ..a::GfxDriverMemoryApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::gfx_buffer_result_ok
}

unsafe fn driver_gfx_buffer_create(
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

unsafe fn driver_gfx_buffer_describe(
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

unsafe fn driver_gfx_buffer_import(
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

unsafe fn driver_gfx_buffer_release(
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

unsafe fn driver_gfx_buffer_map(
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

unsafe fn driver_gfx_buffer_unmap(
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

unsafe fn driver_gfx_native_register(
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

unsafe fn driver_gfx_native_unregister(
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

unsafe fn driver_gfx_native_take(
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

unsafe fn driver_gfx_native_complete(
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

unsafe fn driver_gfx_virtual_register(
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

unsafe fn driver_gfx_virtual_unregister(
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

unsafe fn driver_gfx_virtual_take(
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

unsafe fn driver_gfx_virtual_complete(
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

unsafe fn driver_gfx_cursor_configure(
    identity: *const Identity,
    input: *const a::DisplayCursorInfo,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            0,
            &owner,
            input.cast(),
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_cursor_complete(
    identity: *const Identity,
    input: *const a::GfxDriverCursorCompletion,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            5,
            &owner,
            input.cast(),
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_cursor_take(
    identity: *const Identity,
    backend: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverCursorJob,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            4,
            &owner,
            backend.cast(),
            output.cast(),
        )
    }
}

unsafe fn driver_gfx_boot_hold(
    identity: *const Identity,
    input: *const a::GfxBootHoldRequest,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe { crate::display::boot_driver::r4os_rust_boot_driver_hold(&owner, input, output) }
}

unsafe fn driver_gfx_boot_finish(
    identity: *const Identity,
    generation: u64,
    operation: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::boot_driver::r4os_rust_boot_driver_finish(
            &owner, generation, operation, output,
        )
    }
}

unsafe fn driver_gfx_display_prepare_held(
    identity: *const Identity,
    input: *const a::GfxNativeRegistration,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::boot_driver::r4os_rust_boot_driver_prepare(
            &owner, input, generation, 0, false, output,
        )
    }
}

unsafe fn driver_gfx_display_prepare_reset(
    identity: *const Identity,
    input: *const a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::boot_driver::r4os_rust_boot_driver_prepare(
            &owner,
            input,
            held_generation,
            reset_generation,
            true,
            output,
        )
    }
}

unsafe fn driver_buffer_identity(owner: u32, admit: bool, out: *mut Identity) -> i32 {
    match crate::gfx::driver_memory::owner(owner, admit) {
        Ok(id) => {
            unsafe {
                out.write(Identity {
                    id: id.id,
                    generation: id.generation,
                })
            };
            0
        }
        Err(e) => crate::gfx::buffer_api::status(e),
    }
}

unsafe fn driver_gfx_audio_publish(
    identity: *const Identity,
    input: *const a::GfxAudioRoute,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::publish_audio(owner, unsafe { &*input }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_audio_query(
    identity: *const Identity,
    location: u32,
    device: u32,
    index: u32,
    output: *mut a::GfxAudioRoute,
) -> i32 {
    let _ = identity;
    match crate::display::outputs::query_audio(location, device, index) {
        Some(v) => {
            unsafe { output.write(v) };
            1
        }
        None => 0,
    }
}

unsafe fn driver_gfx_brightness_read(
    identity: *const Identity,
    input: *const a::GfxOutputId,
    output: *mut a::GfxBrightnessRequest,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::read_brightness(
        u32::try_from(owner.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::program::gfx_queue_api::error_code(e),
    }
}

unsafe fn driver_gfx_memory_stats(
    identity: *const Identity,
    output: *mut a::GfxBufferStats,
) -> i32 {
    let _ = identity;
    unsafe {
        crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(7, &Default::default(), output.cast())
    }
}

unsafe fn driver_gfx_device_acquire(
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

unsafe fn driver_gfx_device_segment(
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

unsafe fn driver_gfx_device_release(
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

unsafe fn driver_gfx_mmio_map(
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

unsafe fn driver_gfx_mmio_unmap(
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

unsafe fn driver_gfx_collect(identity: *const Identity, full: bool) -> i32 {
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

unsafe fn driver_gfx_device_lost(
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

unsafe fn driver_gfx_owned_reserve(
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

unsafe fn driver_gfx_owned_commit(
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

unsafe fn driver_gfx_owned_abort(
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

unsafe fn driver_gfx_owned_take_release(
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

unsafe fn driver_gfx_owned_finish_release(
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

unsafe fn driver_gfx_telemetry_exchange(
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

unsafe fn driver_gfx_memory_budget(
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

unsafe fn driver_gfx_display_boot_info(
    identity: *const Identity,
    output: *mut a::GfxNativeBootInfo,
) -> i32 {
    let _ = identity;
    unsafe { crate::display::native_driver::boot_info(output) }
}

unsafe fn driver_gfx_display_prepare(
    identity: *const Identity,
    input: *const a::GfxNativeRegistration,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe { crate::display::native_driver::prepare_request(owner, input, 0, 0, output) }
}

unsafe fn driver_gfx_display_device_reset(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
    generation: u64,
    quiesced: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::native_driver::device_reset(owner, input, generation, quiesced, output)
    }
}

unsafe fn driver_gfx_display_resume_headless(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe { crate::display::native_driver::resume_headless(owner, input, generation, output) }
}

unsafe fn driver_gfx_display_terminal_release(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
    generation: u64,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe { crate::display::native_driver::release_terminal(owner, input, generation, output) }
}

unsafe fn driver_gfx_display_transition(
    identity: *const Identity,
    generation: u64,
    operation: u32,
    output: *mut a::GfxNativeState,
) -> i32 {
    let owner = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    unsafe {
        crate::display::native_driver::transition(
            u32::try_from(owner.id).expect("driver id"),
            generation,
            operation,
            output,
        )
    }
}

unsafe fn driver_gfx_display_schedule(owner: u32, input: *const a::GfxBackendBinding) -> i32 {
    match crate::display::queue::wake_native(
        owner,
        crate::display::queue_state::Binding {
            adapter: unsafe { (*input).adapter_id },
            device_generation: unsafe { (*input).device_generation },
            reset_generation: unsafe { (*input).reset_generation },
        },
    ) {
        Ok(()) => a::gfx_queue_ok,
        Err(e) => crate::program::gfx_queue_api::error_code(e),
    }
}

unsafe fn driver_gfx_mode_enable(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::enable(driver, unsafe { input.read() }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_mode_complete(
    identity: *const Identity,
    input: *const a::GfxDriverModeCompletion,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::complete(driver, unsafe { input.read() }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_mode_restore(
    identity: *const Identity,
    input: *const a::GfxAtomicState,
    output: *mut a::GfxModeStatus,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::restore(driver, unsafe { &*input }) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_mode_status(
    identity: *const Identity,
    ticket: u64,
    output: *mut a::GfxModeStatus,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::driver_status(driver, ticket) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_mode_take(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverModeJob,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::take(driver, unsafe { input.read() }) {
        Ok(v) => match v {
            Some(v) => {
                unsafe { output.write(v) };
                a::gfx_output_ok
            }
            None => 0,
        },
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_mode_read_color(
    identity: *const Identity,
    ticket: u64,
    sequence: u64,
    output: *mut a::GfxDriverModeColor,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::mode_work::read_color(driver, ticket, sequence) {
        Ok(v) => match v {
            Some(v) => {
                unsafe { output.write(v) };
                a::gfx_output_ok
            }
            None => 0,
        },
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_color_publish(
    identity: *const Identity,
    input: *const a::GfxOutputColorState,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::publish_color(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_refresh_publish(
    identity: *const Identity,
    input: *const a::GfxOutputRefresh,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::publish_refresh(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_power_publish(
    identity: *const Identity,
    input: *const a::GfxOutputPower,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::publish_power(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_brightness_publish(
    identity: *const Identity,
    input: *const a::GfxOutputBrightness,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::publish_brightness(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_pause(
    identity: *const Identity,
    input: *const a::GfxOutputId,
    paused: u32,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::pause(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
        paused != 0,
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_power_read(
    identity: *const Identity,
    input: *const a::GfxOutputId,
    output: *mut a::GfxPowerRequest,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::read_power(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_refresh_read(
    identity: *const Identity,
    input: *const a::GfxOutputTarget,
    output: *mut a::GfxRefreshRequest,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::read_refresh(
        u32::try_from(driver.id).expect("driver id"),
        unsafe { input.read() },
    ) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::display::mode_work::code(e),
    }
}

unsafe fn driver_gfx_output_withdraw(owner: u32, input: *const a::GfxOutputId) -> i32 {
    match crate::display::outputs::withdraw(owner, unsafe { input.read() }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_output_publish(
    owner: u32,
    input: *const a::GfxOutputPublication,
    output: *mut a::GfxOutputId,
) -> i32 {
    match unsafe { crate::display::outputs::publish(owner, &*input) } {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_display_output_register(
    identity: *const Identity,
    input: *const a::GfxAdditionalOutput,
    output: *mut a::GfxOutputTarget,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::output_runtime::register(driver, unsafe { input.read() }) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_display_output_transition(
    identity: *const Identity,
    input: *const a::GfxOutputTarget,
    operation: u32,
    quiesced: u32,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::output_runtime::transition(
        driver,
        unsafe { input.read() },
        operation,
        quiesced,
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_receiver_close(
    identity: *const Identity,
    input: *const a::GfxReceiverSource,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::close_receiver_source(driver, unsafe { input.read() }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_receiver_replace(
    identity: *const Identity,
    input: *const a::GfxReceiverUpdate,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match unsafe { crate::display::outputs::replace_receivers(driver, input.read()) } {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_receiver_register(
    identity: *const Identity,
    epoch: u64,
    adapter: u32,
    output: *mut a::GfxReceiverSource,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    match crate::display::outputs::register_receiver_source(driver, adapter, epoch) {
        Ok(v) => {
            unsafe { output.write(v) };
            a::gfx_output_ok
        }
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_display_presentation_info(
    identity: *const Identity,
    stage: u8,
    input: *const a::DisplayPresentationInfo,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    if stage == 0 {
        return match crate::display::output_runtime::publish_info(driver, unsafe { input.read() }) {
            Ok(v) => i32::from(v),
            Err(e) => crate::program::gfx_output_api::code(e),
        };
    }
    match crate::display::manager::publish_presentation_info(
        driver.id as usize,
        driver.generation,
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_display_presentation_stats(
    identity: *const Identity,
    stage: u8,
    input: *const a::DisplayPresentationStats,
) -> i32 {
    let driver = unsafe {
        let id = identity.read();
        crate::gfx::types::Owner {
            kind: 2,
            id: id.id,
            generation: id.generation,
        }
    };
    if stage == 0 {
        return match crate::display::output_runtime::publish_stats(driver, unsafe { input.read() })
        {
            Ok(v) => i32::from(v),
            Err(e) => crate::program::gfx_output_api::code(e),
        };
    }
    match crate::display::manager::publish_presentation_stats(
        driver.id as usize,
        driver.generation,
        unsafe { input.read() },
    ) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

unsafe fn driver_gfx_queue_register_profile(
    identity: *const Identity,
    input: *const a::GfxBackendRegistration,
    profile: *const a::GfxBackendProfile,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        second: profile.cast(),
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(0, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_publish_properties(
    identity: *const Identity,
    input: *const a::GfxBackendBinding,
    properties: *const a::GfxBackendProperties,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        second: properties.cast(),
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            2,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_update_operations(
    owner: u32,
    input: *const a::GfxBackendBinding,
    operations: u64,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        value: operations,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            3,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_unregister(
    owner: u32,
    input: *const a::GfxBackendBinding,
    quiesced: u32,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        other: quiesced,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            1,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_take(
    owner: u32,
    input: *const a::GfxBackendBinding,
    output: *mut a::GfxDriverJob,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(4, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_complete(
    owner: u32,
    input: *const a::GfxFence,
    result: u32,
    quiesced: u32,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        which: result,
        other: quiesced,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            5,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_read_render_list(
    owner: u32,
    input: *const a::GfxFence,
    output: *mut a::GfxRenderList,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(6, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_read_render_grid_list(
    owner: u32,
    input: *const a::GfxFence,
    output: *mut a::GfxRenderGridList,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(7, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_reset(
    owner: u32,
    input: *const a::GfxBackendBinding,
    quiesced: u32,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        other: quiesced,
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(8, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_read_render_color_list(
    owner: u32,
    input: *const a::GfxFence,
    output: *mut a::GfxRenderColorList,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(9, &args, output.cast()) }
}

unsafe fn driver_gfx_queue_segment(
    owner: u32,
    input: *const a::GfxFence,
    which: u32,
    offset: u64,
    mask: u64,
    output: *mut a::GfxDmaSegment,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        which,
        value: offset,
        extra_value: mask,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(10, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_retain(
    identity: *const Identity,
    input: *const a::GfxFence,
    which: u32,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        which,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(11, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_retain_scanout(
    identity: *const Identity,
    input: *const a::GfxFence,
    output: *mut a::GfxBufferReference,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
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
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(12, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_read_native_info(
    owner: u32,
    input: *const a::GfxFence,
    output: *mut a::GfxNativeJobInfo,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(13, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_owner_info(
    owner: u32,
    input: *const a::GfxBackendBinding,
    timeline: u64,
    output: *mut a::GfxQueueOwnerInfo,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        value: timeline,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(14, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_read_native_data(
    owner: u32,
    input: *const a::GfxFence,
    offset: u32,
    output: *mut u8,
    count: u32,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        which: offset,
        other: count,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(15, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_read_native_binding(
    owner: u32,
    input: *const a::GfxFence,
    index: u32,
    output: *mut a::GfxNativeBinding,
) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        which: index,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(16, &args, output.cast())
    }
}

unsafe fn driver_gfx_queue_begin_scanout(owner: u32, input: *const a::GfxFence) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            17,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_scanout_retire_requested(owner: u32, input: *const a::GfxFence) -> i32 {
    let args = crate::display::driver_queue_api::Args {
        input: input.cast(),
        id: owner,
        ..Default::default()
    };
    unsafe {
        crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(
            18,
            &args,
            core::ptr::null_mut(),
        )
    }
}

unsafe fn driver_gfx_queue_register(
    identity: *const Identity,
    input: *const a::GfxBackendRegistration,
    output: *mut a::GfxBackendBinding,
) -> i32 {
    let profile = a::GfxBackendProfile::default();
    let args = crate::display::driver_queue_api::Args {
        owner: unsafe {
            let id = identity.read();
            crate::gfx::types::Owner {
                kind: 2,
                id: id.id,
                generation: id.generation,
            }
        },
        input: input.cast(),
        second: core::ptr::addr_of!(profile).cast(),
        ..Default::default()
    };
    unsafe { crate::display::driver_queue_api::r4os_rust_gfx_driver_queue(0, &args, output.cast()) }
}

unsafe fn driver_gfx_span(base: u64, bytes: u64, unmanaged: bool) -> bool {
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

unsafe fn driver_gfx_output_query(
    identity: *const Identity,
    output: *mut a::GfxDriverOutputApi,
) -> i32 {
    let _ = identity;
    let functions = a::GfxDriverOutputApi {
        publish: r4os_rust_driver_gfx_output_publish as *const () as u64,
        withdraw: r4os_rust_driver_gfx_output_withdraw as *const () as u64,
        register_source: r4os_rust_driver_gfx_receiver_register as *const () as u64,
        replace_receivers: r4os_rust_driver_gfx_receiver_replace as *const () as u64,
        close_source: r4os_rust_driver_gfx_receiver_close as *const () as u64,
        mode_enable: r4os_rust_driver_gfx_mode_enable as *const () as u64,
        mode_take: r4os_rust_driver_gfx_mode_take as *const () as u64,
        mode_complete: r4os_rust_driver_gfx_mode_complete as *const () as u64,
        audio_publish: r4os_rust_driver_gfx_audio_publish as *const () as u64,
        audio_query: r4os_rust_driver_gfx_audio_query as *const () as u64,
        output_pause: r4os_rust_driver_gfx_output_pause as *const () as u64,
        mode_restore: r4os_rust_driver_gfx_mode_restore as *const () as u64,
        mode_status: r4os_rust_driver_gfx_mode_status as *const () as u64,
        color_publish: r4os_rust_driver_gfx_output_color_publish as *const () as u64,
        mode_read_color: r4os_rust_driver_gfx_mode_read_color as *const () as u64,
        refresh_publish: r4os_rust_driver_gfx_output_refresh_publish as *const () as u64,
        refresh_read: r4os_rust_driver_gfx_refresh_read as *const () as u64,
        power_publish: r4os_rust_driver_gfx_output_power_publish as *const () as u64,
        power_read: r4os_rust_driver_gfx_power_read as *const () as u64,
        brightness_publish: r4os_rust_driver_gfx_output_brightness_publish as *const () as u64,
        brightness_read: r4os_rust_driver_gfx_brightness_read as *const () as u64,
        ..Default::default()
    };
    unsafe {
        crate::display::output_validation::r4os_rust_display_output_driver_table(output, &functions)
    }
}
