//! Original exact-generation admission snapshots. Physical capacity and
//! placement remain R4D policy; no physical resource is acquired here.
use super::{
    buffer_api as api,
    buffer_owner::Args,
    buffers, driver_memory,
    layout::Binding,
    types::{Error, Owner},
};
use crate::contract as a;
unsafe fn request(
    input: *const a::GfxDeviceBudgetRequest,
    output: *mut a::GfxDeviceBudgetState,
) -> Result<a::GfxDeviceBudgetRequest, Error> {
    unsafe {
        if !api::valid_aligned(input) || !api::valid_aligned(output) {
            return Err(Error::Invalid);
        }
        let v = input.read();
        if v.adapter_id == 0
            || v.memory_generation == 0
            || v.operation > a::gfx_memory_budget_configure
            || (v.operation == a::gfx_memory_budget_query && v.limit_bytes != 0)
            || !v.limit_bytes.is_multiple_of(4096)
        {
            return Err(Error::Invalid);
        }
        Ok(v)
    }
}
unsafe fn snapshot_locked(
    value: a::GfxDeviceBudgetRequest,
    identity: Option<Owner>,
) -> Result<a::GfxDeviceBudgetState, Error> {
    unsafe {
        let r = buffers::read();
        let snapshot = if let Some(o) = identity {
            r.device_budget(
                o,
                Binding {
                    adapter: value.adapter_id,
                    driver_owner: u32::try_from(o.id).expect("Original driver ID"),
                    device_generation: value.memory_generation,
                },
            )?
        } else {
            r.query_device_budget(value.adapter_id, value.memory_generation)?
        };
        Ok(a::GfxDeviceBudgetState {
            adapter_id: value.adapter_id,
            memory_generation: value.memory_generation,
            flags: if snapshot.closing {
                a::gfx_memory_budget_closing
            } else {
                0
            },
            limit_bytes: snapshot.limit,
            charged_bytes: snapshot.charged,
            shared_limit_bytes: buffers::TOTAL_BUDGET,
            shared_charged_bytes: r.shared_charged_bytes(),
            shared_producer_limit_bytes: buffers::PRODUCER_BUDGET,
            ..a::GfxDeviceBudgetState::default()
        })
    }
}
pub(crate) unsafe fn query(
    input: *const a::GfxDeviceBudgetRequest,
    output: *mut a::GfxDeviceBudgetState,
) -> Result<(), Error> {
    unsafe {
        let value = request(input, output)?;
        if value.operation != a::gfx_memory_budget_query {
            return Err(Error::Invalid);
        }
        let snapshot = {
            let _lock = api::Locked::acquire();
            snapshot_locked(value, None)?
        };
        output.write(snapshot);
        Ok(())
    }
}
pub(crate) unsafe fn provider(
    identity: Owner,
    input: *const a::GfxDeviceBudgetRequest,
    output: *mut a::GfxDeviceBudgetState,
) -> Result<(), Error> {
    unsafe {
        let value = request(input, output)?;
        let _call = buffers::Call::acquire()?;
        let snapshot = {
            let _lock = api::Locked::acquire();
            if value.operation == a::gfx_memory_budget_configure {
                driver_memory::admit_locked(identity)?;
                buffers::table().set_device_budget(
                    identity,
                    Binding {
                        adapter: value.adapter_id,
                        driver_owner: u32::try_from(identity.id).expect("Original driver ID"),
                        device_generation: value.memory_generation,
                    },
                    value.limit_bytes,
                )?;
            }
            snapshot_locked(value, Some(identity))?
        };
        output.write(snapshot);
        Ok(())
    }
}
/// # Safety
/// Resident initialized contract request/output, with the exact admitted driver
/// owner retained for provider calls. Metadata borrows never span a wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_budget_api(
    args: *const Args,
    provider_call: bool,
    output: *mut a::GfxDeviceBudgetState,
) -> i32 {
    let a = unsafe { args.read() };
    let result = unsafe {
        if provider_call {
            provider(a.owner, a.input.cast(), output)
        } else {
            query(a.input.cast(), output)
        }
    };
    match result {
        Ok(()) => a::gfx_buffer_result_ok,
        Err(e) => api::status(e),
    }
}
