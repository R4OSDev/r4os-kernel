//! Exact outputs operations of the parent owner; no duplicated state.
use super::*;

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

pub(super) unsafe fn driver_gfx_audio_publish(
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

pub(super) unsafe fn driver_gfx_audio_query(
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

pub(super) unsafe fn driver_gfx_brightness_read(
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

pub(super) unsafe fn driver_gfx_mode_enable(
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

pub(super) unsafe fn driver_gfx_mode_complete(
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

pub(super) unsafe fn driver_gfx_mode_restore(
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

pub(super) unsafe fn driver_gfx_mode_status(
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

pub(super) unsafe fn driver_gfx_mode_take(
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

pub(super) unsafe fn driver_gfx_mode_read_color(
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

pub(super) unsafe fn driver_gfx_output_color_publish(
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

pub(super) unsafe fn driver_gfx_output_refresh_publish(
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

pub(super) unsafe fn driver_gfx_output_power_publish(
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

pub(super) unsafe fn driver_gfx_output_brightness_publish(
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

pub(super) unsafe fn driver_gfx_output_pause(
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

pub(super) unsafe fn driver_gfx_power_read(
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

pub(super) unsafe fn driver_gfx_refresh_read(
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

pub(super) unsafe fn driver_gfx_output_withdraw(owner: u32, input: *const a::GfxOutputId) -> i32 {
    match crate::display::outputs::withdraw(owner, unsafe { input.read() }) {
        Ok(()) => a::gfx_output_ok,
        Err(e) => crate::program::gfx_output_api::code(e),
    }
}

pub(super) unsafe fn driver_gfx_output_publish(
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

pub(super) unsafe fn driver_gfx_receiver_close(
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

pub(super) unsafe fn driver_gfx_receiver_replace(
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

pub(super) unsafe fn driver_gfx_receiver_register(
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

pub(super) unsafe fn driver_gfx_output_query(
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
