//! Exact display operations of the parent owner; no duplicated state.
use super::*;

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

pub(super) unsafe fn driver_gfx_cursor_configure(
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

pub(super) unsafe fn driver_gfx_cursor_complete(
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

pub(super) unsafe fn driver_gfx_cursor_take(
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

pub(super) unsafe fn driver_gfx_boot_hold(
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

pub(super) unsafe fn driver_gfx_boot_finish(
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

pub(super) unsafe fn driver_gfx_display_prepare_held(
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

pub(super) unsafe fn driver_gfx_display_prepare_reset(
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

pub(super) unsafe fn driver_gfx_display_boot_info(
    identity: *const Identity,
    output: *mut a::GfxNativeBootInfo,
) -> i32 {
    let _ = identity;
    unsafe { crate::display::native_driver::boot_info(output) }
}

pub(super) unsafe fn driver_gfx_display_prepare(
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

pub(super) unsafe fn driver_gfx_display_device_reset(
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

pub(super) unsafe fn driver_gfx_display_resume_headless(
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

pub(super) unsafe fn driver_gfx_display_terminal_release(
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

pub(super) unsafe fn driver_gfx_display_transition(
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

pub(super) unsafe fn driver_gfx_display_schedule(
    owner: u32,
    input: *const a::GfxBackendBinding,
) -> i32 {
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

pub(super) unsafe fn driver_gfx_display_output_register(
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

pub(super) unsafe fn driver_gfx_display_output_transition(
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

pub(super) unsafe fn driver_gfx_display_presentation_info(
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

pub(super) unsafe fn driver_gfx_display_presentation_stats(
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
