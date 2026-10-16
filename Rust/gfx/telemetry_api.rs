//! Original finite telemetry demand transport. Sensor collection/policy stays
//! with existing R4D Work. The native cache has one BO-serialized owner.
use super::{
    buffer_api as api,
    buffer_owner::Args,
    buffers, driver_memory, telemetry as model,
    types::{Error, Owner},
};
use crate::{contract as a, platform::monotonic};
use core::{cell::UnsafeCell, ffi::c_void};
const EMPTY: model::Entry = model::Entry {
    has_owner: false,
    owner: Owner::EMPTY,
    data: a::GfxTelemetryState {
        version: 1,
        size: 544,
        adapter_id: 0,
        reserved0: 0,
        memory_generation: 0,
        sampled_ns: 0,
        valid_until_ns: 0,
        source: 0,
        state: 0,
        policy: 0,
        boost: 0,
        control_status: 0,
        reserved1: 0,
        metrics: [a::GfxTelemetryMetric {
            status: 0,
            flags: 0,
            source_stamp: 0,
            values: [0; 4],
        }; 10],
    },
    demand: a::GfxTelemetryDemand {
        version: 1,
        size: 40,
        adapter_id: 0,
        reserved0: 0,
        memory_generation: 0,
        until_ns: 0,
        metric_mask: 0,
    },
};
struct Cell(UnsafeCell<[model::Entry; 16]>);
// SAFETY: all cache/demand reads and mutation use the one shared BO owner.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new([EMPTY; 16]));
pub(crate) unsafe fn query(
    input: *const a::GfxTelemetryRequest,
    output: *mut a::GfxTelemetryState,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_aligned(input) || !api::valid_aligned(output) {
            return Err(Error::Invalid);
        }
        let instant = monotonic::now_nanoseconds().unwrap_or(0);
        let result = {
            let _lock = api::Locked::acquire();
            model::query(&mut *STATE.0.get(), input.read(), instant)?
        };
        output.write(result);
        Ok(())
    }
}
pub(crate) unsafe fn publish(
    owner: Owner,
    input: *const a::GfxTelemetryState,
    output: *mut a::GfxTelemetryDemand,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_aligned(input) || !api::valid_aligned(output) {
            return Err(Error::Invalid);
        }
        let _call = buffers::Call::acquire()?;
        let instant = monotonic::now_nanoseconds().unwrap_or(0);
        let result = {
            let _lock = api::Locked::acquire();
            driver_memory::admit_locked(owner)?;
            model::publish(&mut *STATE.0.get(), owner, input.read(), instant)?
        };
        output.write(result);
        Ok(())
    }
}
pub fn close_driver(id: u32) {
    let _lock = api::Locked::acquire();
    unsafe {
        model::close(&mut *STATE.0.get(), id);
    }
}
/// # Safety
/// Each operation retains exact resident C input/output storage and its original
/// admitted driver identity. No metadata borrow spans external work or waiting.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_telemetry_api(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let a = unsafe { args.read() };
    let result = unsafe {
        match op {
            0 => query(a.input.cast(), out.cast()),
            1 => publish(a.owner, a.input.cast(), out.cast()),
            2 => {
                close_driver(a.value as u32);
                Ok(())
            }
            _ => panic!("Invalid telemetry API operation"),
        }
    };
    match result {
        Ok(()) => a::gfx_buffer_result_ok,
        Err(e) => api::status(e),
    }
}
