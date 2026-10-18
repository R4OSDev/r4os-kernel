//! Exact queue operations of the parent owner; no duplicated state.
use super::*;

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

pub(super) unsafe fn driver_gfx_queue_register_profile(
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

pub(super) unsafe fn driver_gfx_queue_publish_properties(
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

pub(super) unsafe fn driver_gfx_queue_update_operations(
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

pub(super) unsafe fn driver_gfx_queue_unregister(
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

pub(super) unsafe fn driver_gfx_queue_take(
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

pub(super) unsafe fn driver_gfx_queue_complete(
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

pub(super) unsafe fn driver_gfx_queue_read_render_list(
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

pub(super) unsafe fn driver_gfx_queue_read_render_grid_list(
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

pub(super) unsafe fn driver_gfx_queue_reset(
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

pub(super) unsafe fn driver_gfx_queue_read_render_color_list(
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

pub(super) unsafe fn driver_gfx_queue_segment(
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

pub(super) unsafe fn driver_gfx_queue_retain(
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

pub(super) unsafe fn driver_gfx_queue_retain_scanout(
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

pub(super) unsafe fn driver_gfx_queue_read_native_info(
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

pub(super) unsafe fn driver_gfx_queue_owner_info(
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

pub(super) unsafe fn driver_gfx_queue_read_native_data(
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

pub(super) unsafe fn driver_gfx_queue_read_native_binding(
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

pub(super) unsafe fn driver_gfx_queue_begin_scanout(owner: u32, input: *const a::GfxFence) -> i32 {
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

pub(super) unsafe fn driver_gfx_queue_scanout_retire_requested(
    owner: u32,
    input: *const a::GfxFence,
) -> i32 {
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

pub(super) unsafe fn driver_gfx_queue_register(
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
