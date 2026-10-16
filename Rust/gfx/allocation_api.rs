//! Original driver and R4DRAW allocation transport. Wait deliberately holds no
//! unwind token: the exact task retirement path owns enrolled waiter cleanup.
use super::{allocations as runtime, buffer_api as api, buffer_owner::Args, buffers, types::Error};
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
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxNativeJob>()) {
                    return Err(Error::Invalid);
                }
                out.cast::<a::GfxNativeJob>().write(runtime::take(
                    args.owner,
                    api::handle(args.input.cast::<a::GfxBufferHandle>().read())?,
                )?);
            }
            3 => {
                if args.input.is_null() || args.other_input.is_null() || args.buffer.is_null() {
                    return Err(Error::Invalid);
                }
                let p = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                let h = api::handle(args.other_input.cast::<a::GfxBufferHandle>().read())?;
                runtime::complete(
                    args.owner,
                    p,
                    h,
                    args.access as i32,
                    args.buffer.cast::<a::GfxBufferHandle>().read(),
                )?;
            }
            _ => panic!("Invalid allocation driver API operation"),
        }
        Ok(())
    }
}
unsafe fn program(op: u32, args: Args, out: *mut c_void) -> Result<i32, Error> {
    unsafe {
        match op {
            0 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxNativeStatus>()) {
                    return Err(Error::Invalid);
                }
                let value = args.input.cast::<a::GfxNativeAllocation>().read();
                let _call = buffers::Call::acquire()?;
                out.cast::<a::GfxNativeStatus>()
                    .write(runtime::start(args.owner, value)?);
            }
            1 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxNativeStatus>()) {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                out.cast::<a::GfxNativeStatus>()
                    .write(runtime::query(args.owner, h)?);
            }
            2 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxBufferReference>()) {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                let _call = buffers::Call::acquire()?;
                return Ok(runtime::receive(args.owner, h, out.cast()));
            }
            3 => {
                if args.input.is_null() {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                let _call = buffers::Call::acquire()?;
                runtime::close(args.owner, h)?;
            }
            4 => {
                if args.input.is_null() || !api::valid_output(out.cast::<a::GfxNativeStatus>()) {
                    return Err(Error::Invalid);
                }
                let h = api::handle(args.input.cast::<a::GfxBufferHandle>().read())?;
                out.cast::<a::GfxNativeStatus>()
                    .write(runtime::wait(args.owner, h, args.value)?);
            }
            _ => panic!("Invalid allocation program API operation"),
        }
        Ok(1)
    }
}
/// # Safety
/// Args selects the exact resident public request/output records. Its admitted
/// driver/program epoch and executable callbacks retain their original lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_api(
    driver_call: bool,
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let result = unsafe {
        if driver_call {
            driver(op, args, out).map(|()| 1)
        } else {
            program(op, args, out)
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => api::status(e),
    }
}
