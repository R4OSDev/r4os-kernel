//! Copied public output payloads. Pageable outputs are touched only after the
//! native catalog owner returns; explicit work retains the original call pin.
use crate::{
    contract as a,
    display::{error::Error, mode_color, mode_work as modes, outputs, ownership, wire},
    gfx::{buffer_api, types::Owner},
};
use core::{ffi::c_void, mem::size_of};
pub fn code(error: Error) -> i32 {
    match error {
        Error::Bandwidth => a::gfx_output_error_bandwidth,
        Error::Routing => a::gfx_output_error_routing,
        Error::Dependency => a::gfx_output_error_dependency,
        other => super::gfx_queue_api::error_code(other),
    }
}
unsafe fn input<T>(value: *const T, aligned: bool) -> Result<T, i32>
where
    T: Copy,
{
    if value.is_null() || (aligned && !value.is_aligned()) {
        Err(a::gfx_output_error_invalid)
    } else {
        Ok(unsafe { value.read() })
    }
}
unsafe fn output<T>(value: *const T) -> Result<(), i32> {
    if unsafe { buffer_api::valid_output(value) } {
        Ok(())
    } else {
        Err(a::gfx_output_error_invalid)
    }
}
fn call() -> Result<ownership::Call, i32> {
    ownership::Call::acquire().map_err(|_| a::gfx_output_error_busy)
}
fn finish(value: Result<(), i32>) -> i32 {
    value.map_or_else(|error| error, |()| a::gfx_output_ok)
}
/// # Safety
/// Initialized public header and writable complete output remain resident.
pub unsafe extern "C" fn revision(out: *mut a::GfxDisplayRevision) -> i32 {
    finish((|| {
        unsafe {
            output(out)?;
            let snapshot = outputs::revision();
            out.write(snapshot)
        };
        Ok(())
    })())
}
/// # Safety
/// Writable initialized output remains resident after the catalog query.
pub unsafe extern "C" fn info(index: u32, out: *mut a::GfxOutputInfo) -> i32 {
    finish((|| {
        unsafe {
            output(out)?;
            if let Some(snapshot) = outputs::info_at(index) {
                out.write(snapshot)
            } else {
                return Err(0);
            }
        };
        Ok(())
    })())
}
/// # Safety
/// Exact readable identity and initialized output follow their public layouts.
pub unsafe extern "C" fn mode(
    id: *const a::GfxOutputId,
    index: u32,
    out: *mut a::GfxOutputMode,
) -> i32 {
    finish((|| {
        unsafe {
            if id.is_null() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let snapshot = outputs::mode_at(id.read(), index).map_err(code)?;
            if let Some(snapshot) = snapshot {
                out.write(snapshot)
            } else {
                return Err(0);
            }
        };
        Ok(())
    })())
}
/// # Safety
/// Exact readable identity and complete initialized EDID output are retained.
pub unsafe extern "C" fn edid(
    id: *const a::GfxOutputId,
    index: u32,
    out: *mut a::GfxEdidBlock,
) -> i32 {
    finish((|| {
        unsafe {
            if id.is_null() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let snapshot = outputs::edid_at(id.read(), index).map_err(code)?;
            if let Some(snapshot) = snapshot {
                out.write(snapshot)
            } else {
                return Err(0);
            }
        };
        Ok(())
    })())
}
/// # Safety
/// Identity is complete; output retains its declared released color prefix.
pub unsafe extern "C" fn color(id: *const a::GfxOutputId, out: *mut a::GfxOutputColorState) -> i32 {
    finish((|| {
        unsafe {
            if id.is_null() || !id.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            let count = wire::color_capacity(out).ok_or(a::gfx_output_error_invalid)?;
            let snapshot = outputs::color_at(id.read()).map_err(code)?;
            wire::write(out, count, snapshot)
        };
        Ok(())
    })())
}
/// # Safety
/// Retained full target and initialized refresh output have exact layouts.
pub unsafe extern "C" fn refresh(
    target: *const a::GfxOutputTarget,
    out: *mut a::GfxOutputRefresh,
) -> i32 {
    finish((|| {
        unsafe {
            if target.is_null() || !target.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let value = outputs::refresh_at(target.read()).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
/// # Safety
/// Copied aligned request and initialized output survive the owner handoff.
pub unsafe fn request_refresh(
    owner: Owner,
    request: *const a::GfxRefreshRequest,
    out: *mut a::GfxRefreshRequest,
) -> i32 {
    finish((|| {
        unsafe {
            if request.is_null() || !request.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let request = input(request, true)?;
            let value = outputs::request_refresh(owner, request).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact aligned output identity and writable initialized power record.
pub unsafe extern "C" fn power(id: *const a::GfxOutputId, out: *mut a::GfxOutputPower) -> i32 {
    finish((|| {
        unsafe {
            if id.is_null() || !id.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let value = outputs::power_at(id.read()).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
/// # Safety
/// Complete aligned copied request/output remain resident through admission.
pub unsafe fn request_power(
    owner: Owner,
    request: *const a::GfxPowerRequest,
    out: *mut a::GfxPowerRequest,
) -> i32 {
    finish((|| {
        unsafe {
            if request.is_null() || !request.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let request = request.read();
            let value = outputs::request_power(owner, request).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact owner and resident request/output are retained through explicit work.
pub unsafe fn atomic(
    owner: Owner,
    state: *const a::GfxAtomicState,
    out: *mut a::GfxAtomicResult,
    commit: bool,
) -> i32 {
    finish((|| {
        unsafe {
            if state.is_null() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let state = state.read();
            let _call = call()?;
            let result = outputs::atomic(owner, &state, commit).map_err(code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Complete request/output and exact caller remain valid through submission.
pub unsafe fn submit(
    owner: Owner,
    state: *const a::GfxAtomicState,
    confirmation_ms: u32,
    out: *mut a::GfxModeStatus,
) -> i32 {
    finish((|| {
        unsafe {
            if state.is_null() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let state = state.read();
            let _call = call()?;
            let result = modes::submit(owner, &state, confirmation_ms).map_err(modes::code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact aligned color request and initialized output remain fully resident.
pub unsafe fn test_color(
    owner: Owner,
    request: *const a::GfxModeColorRequest,
    out: *mut a::GfxAtomicResult,
) -> i32 {
    finish((|| {
        unsafe {
            if request.is_null() || !request.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let request = request.read();
            let _call = call()?;
            mode_color::validate(owner, &request).map_err(code)?;
            let result = outputs::atomic(owner, &request.state, false).map_err(code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact aligned request/output and source payloads survive this submission.
pub unsafe fn submit_color(
    owner: Owner,
    request: *const a::GfxModeColorRequest,
    confirmation_ms: u32,
    out: *mut a::GfxModeStatus,
) -> i32 {
    finish((|| {
        unsafe {
            if request.is_null() || !request.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let request = request.read();
            let _call = call()?;
            let result =
                modes::submit_color(owner, &request, confirmation_ms).map_err(modes::code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Complete initialized status output remains writable after the query.
pub unsafe extern "C" fn mode_status(ticket: u64, out: *mut a::GfxModeStatus) -> i32 {
    finish((|| {
        unsafe {
            output(out)?;
            let result = modes::status(ticket).map_err(modes::code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact admitted caller and initialized output remain valid through resolve.
pub unsafe fn resolve(owner: Owner, ticket: u64, action: u32, out: *mut a::GfxModeStatus) -> i32 {
    finish((|| {
        unsafe {
            output(out)?;
            let result = modes::resolve(owner, ticket, action).map_err(modes::code)?;
            out.write(result)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact aligned identity and complete initialized brightness output.
pub unsafe extern "C" fn brightness(
    id: *const a::GfxOutputId,
    out: *mut a::GfxOutputBrightness,
) -> i32 {
    finish((|| {
        unsafe {
            if id.is_null() || !id.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let value = outputs::brightness_at(id.read()).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
/// # Safety
/// Exact aligned copied request and initialized output remain fully live.
pub unsafe fn request_brightness(
    owner: Owner,
    request: *const a::GfxBrightnessRequest,
    out: *mut a::GfxBrightnessRequest,
) -> i32 {
    finish((|| {
        unsafe {
            if request.is_null() || !request.is_aligned() {
                return Err(a::gfx_output_error_invalid);
            }
            output(out)?;
            let request = request.read();
            let value = outputs::request_brightness(owner, request).map_err(code)?;
            out.write(value)
        };
        Ok(())
    })())
}
#[repr(C)]
#[derive(Default)]
pub struct Args {
    pub owner: Owner,
    pub ticket: u64,
    pub value: u32,
    pub other: u32,
    pub input: *const c_void,
}
/// # Safety
/// Kernel facade retains the exact admitted public input/output for each slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_output_api(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    unsafe {
        match op {
            0 => revision(out.cast()),
            1 => info(args.value, out.cast()),
            2 => mode(args.input.cast(), args.value, out.cast()),
            3 => edid(args.input.cast(), args.value, out.cast()),
            4 => color(args.input.cast(), out.cast()),
            5 => refresh(args.input.cast(), out.cast()),
            6 => request_refresh(args.owner, args.input.cast(), out.cast()),
            7 => power(args.input.cast(), out.cast()),
            8 => request_power(args.owner, args.input.cast(), out.cast()),
            9 => atomic(args.owner, args.input.cast(), out.cast(), args.other != 0),
            10 => submit(args.owner, args.input.cast(), args.value, out.cast()),
            11 => test_color(args.owner, args.input.cast(), out.cast()),
            12 => submit_color(args.owner, args.input.cast(), args.value, out.cast()),
            13 => mode_status(args.ticket, out.cast()),
            14 => resolve(args.owner, args.ticket, args.value, out.cast()),
            15 => brightness(args.input.cast(), out.cast()),
            16 => request_brightness(args.owner, args.input.cast(), out.cast()),
            17 => match crate::display::error::status(args.value as i32) {
                Err(e) => code(e),
                Ok(()) => panic!("output error projection"),
            },
            _ => panic!("public output API operation"),
        }
    }
}
const _: () = {
    assert!(size_of::<Args>() == 48);
    assert!(core::mem::offset_of!(Args, input) == 40);
};
