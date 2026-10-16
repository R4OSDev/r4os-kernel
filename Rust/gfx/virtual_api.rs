//! Original R4D/R4DRAW virtual-resource transport. The synchronous caller bit
//! is only sampled under the BO owner; no process tombstone/pointer is stored.
use super::{
    buffer_api as api, buffer_owner::Args, buffers, types::Error, virtual_runtime as runtime,
};
use crate::contract as a;
use core::ffi::c_void;
unsafe fn driver(op: u32, args: Args, out: *mut c_void) -> Result<(), Error> {
    unsafe {
        match op {
            0 => {
                if args.input.is_null() || out.is_null() {
                    return Err(Error::Invalid);
                }
                out.cast::<a::GfxBufferHandle>()
                    .write(api::public_handle(runtime::register(
                        args.owner,
                        args.input.cast::<a::GfxNativeProvider>().read(),
                    )?));
            }
            1 => {
                if args.input.is_null() {
                    return Err(Error::Invalid);
                }
                runtime::unregister(
                    args.owner,
                    api::handle(args.input.cast::<a::GfxBufferHandle>().read())?,
                )?;
            }
            2 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxVirtualJob>()) {
                    return Err(Error::Invalid);
                }
                out.cast::<a::GfxVirtualJob>().write(runtime::take(
                    args.owner,
                    api::handle(args.input.cast::<a::GfxBufferHandle>().read())?,
                )?);
            }
            3 => {
                if args.input.is_null() || args.other_input.is_null() {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                runtime::complete(
                    args.owner,
                    h,
                    args.other_input.cast::<a::GfxVirtualCompletion>().read(),
                )?;
            }
            _ => panic!("Invalid virtual driver API operation"),
        }
        Ok(())
    }
}
unsafe fn program(op: u32, args: Args, out: *mut c_void) -> Result<(), Error> {
    unsafe {
        match op {
            0 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxVirtualStatus>()) {
                    return Err(Error::Invalid);
                }
                let v = args.input.cast::<a::GfxVirtualRequest>().read();
                let _call = buffers::Call::acquire()?;
                out.cast::<a::GfxVirtualStatus>().write(runtime::start(
                    args.owner,
                    args.other_input.cast(),
                    v,
                )?);
            }
            1 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxVirtualStatus>()) {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                out.cast::<a::GfxVirtualStatus>()
                    .write(runtime::query(args.owner, h)?);
            }
            2 => {
                if args.input.is_null() {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                let _call = buffers::Call::acquire()?;
                runtime::close(args.owner, h, args.access)?;
            }
            3 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxVirtualStatus>()) {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                out.cast::<a::GfxVirtualStatus>().write(runtime::wait(
                    args.owner,
                    h,
                    args.access,
                    args.value,
                )?);
            }
            _ => panic!("Invalid virtual program API operation"),
        }
        Ok(())
    }
}
/// # Safety
/// Args selects exact resident contract inputs/outputs with the original
/// driver/program lifetime. Wait holds no unwind token across scheduling.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_api(
    driver_call: bool,
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let result = unsafe {
        if driver_call {
            driver(op, args, out)
        } else {
            program(op, args, out)
        }
    };
    match result {
        Ok(()) => a::gfx_buffer_result_ok,
        Err(e) => api::status(e),
    }
}
