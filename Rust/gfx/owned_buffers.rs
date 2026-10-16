//! Original common owned-BO reservation/physical release bridge. Hardware
//! quiescence, allocation and placement remain with the exact R4D epoch.
use super::{
    buffer_api as api, buffer_owner as bo, buffers, driver_memory,
    layout::Binding,
    types::{Error, Owner},
};
use crate::contract as a;
use core::{ffi::c_void, mem::size_of};
pub fn lose(identity: Owner, adapter: u32, generation: u64, quiesced: u32) -> Result<(), Error> {
    if quiesced > 1 {
        return Err(Error::Invalid);
    }
    let _call = buffers::Call::acquire()?;
    let _lock = api::Locked::acquire();
    unsafe {
        buffers::table().lose_device(
            identity,
            Binding {
                adapter,
                driver_owner: u32::try_from(identity.id).expect("Original driver ID"),
                device_generation: generation,
            },
            quiesced == 1,
        )
    }
}
unsafe fn creation(input: *const a::GfxOwnedBufferReservation) -> Result<bo::OwnedCreate, Error> {
    if input.is_null() {
        return Err(Error::Invalid);
    }
    let v = unsafe { input.read() };
    if v.version != 1
        || v.size < size_of::<a::GfxOwnedBufferReservation>() as u32
        || v.reserved0 != 0
    {
        return Err(Error::Invalid);
    }
    Ok(bo::OwnedCreate {
        create: bo::Create {
            buffer: api::handle(v.buffer)?,
            reference: api::handle(v.reference)?,
            bytes: v.allocation_bytes,
        },
        cookie: v.cookie,
        driver: Owner {
            kind: 2,
            id: u64::from(v.driver_owner),
            generation: v.driver_generation,
        },
        binding: Binding {
            adapter: v.adapter_id,
            driver_owner: v.driver_owner,
            device_generation: v.device_generation,
        },
    })
}
pub(crate) unsafe fn reserve(
    identity: Owner,
    input: *const a::GfxBufferDescriptor,
    cookie: u64,
    output: *mut a::GfxOwnedBufferReservation,
) -> Result<(), Error> {
    unsafe {
        if input.is_null() || !api::valid_output(output) {
            return Err(Error::Invalid);
        }
        let mut d = api::descriptor(input.read())?;
        if d.binding.driver_owner != 0 && u64::from(d.binding.driver_owner) != identity.id {
            return Err(Error::Invalid);
        }
        d.binding.driver_owner = u32::try_from(identity.id).expect("Original driver ID");
        let _call = buffers::Call::acquire()?;
        buffers::lock_prepared(buffers::Need {
            objects: 1,
            references: 1,
            leases: 0,
        })?;
        let t = {
            let _lock = api::Locked;
            driver_memory::admit_locked(identity)?;
            buffers::table().begin_owned(identity, d, cookie)?
        };
        output.write(a::GfxOwnedBufferReservation {
            buffer: api::public_handle(t.create.buffer),
            reference: api::public_handle(t.create.reference),
            allocation_bytes: t.create.bytes,
            cookie: t.cookie,
            device_generation: t.binding.device_generation,
            driver_generation: identity.generation,
            adapter_id: t.binding.adapter,
            driver_owner: identity.id as u32,
            ..a::GfxOwnedBufferReservation::default()
        });
        Ok(())
    }
}
pub(crate) unsafe fn commit(
    identity: Owner,
    input: *const a::GfxOwnedBufferReservation,
    output: *mut a::GfxBufferReference,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_output(output) {
            return Err(Error::Invalid);
        }
        let ticket = creation(input)?;
        let _call = buffers::Call::acquire()?;
        let value = {
            let _lock = api::Locked::acquire();
            driver_memory::admit_locked(identity)?;
            buffers::table().commit_owned(ticket, identity)?;
            api::reference_locked(ticket.create.reference, identity)
                .expect("Original committed BO reference")
        };
        output.write(value);
        Ok(())
    }
}
pub(crate) unsafe fn abort(
    identity: Owner,
    input: *const a::GfxOwnedBufferReservation,
    quiesced: u32,
) -> Result<(), Error> {
    if quiesced > 1 {
        return Err(Error::Invalid);
    }
    let ticket = unsafe { creation(input) }?;
    let _call = buffers::Call::acquire()?;
    let _lock = api::Locked::acquire();
    unsafe { buffers::table().abort_owned(ticket, identity, quiesced == 1) }
}
pub(crate) unsafe fn take(
    identity: Owner,
    adapter: u32,
    generation: u64,
    output: *mut a::GfxOwnedBufferRelease,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_output(output) {
            return Err(Error::Invalid);
        }
        let _call = buffers::Call::acquire()?;
        let ticket = {
            let _lock = api::Locked::acquire();
            buffers::table()
                .take_owned_release(
                    identity,
                    Binding {
                        adapter,
                        driver_owner: u32::try_from(identity.id).expect("Original driver ID"),
                        device_generation: generation,
                    },
                )?
                .ok_or(Error::Busy)?
        };
        output.write(a::GfxOwnedBufferRelease {
            buffer: api::public_handle(ticket.release.buffer),
            cookie: ticket.release.backing.cookie,
            byte_length: ticket.release.backing.bytes,
            attempt: ticket.release.attempt,
            device_generation: generation,
            driver_generation: identity.generation,
            adapter_id: adapter,
            driver_owner: identity.id as u32,
            ..a::GfxOwnedBufferRelease::default()
        });
        Ok(())
    }
}
pub(crate) unsafe fn finish(
    identity: Owner,
    input: *const a::GfxOwnedBufferRelease,
    quiesced: u32,
) -> Result<(), Error> {
    unsafe {
        if input.is_null() || quiesced > 1 {
            return Err(Error::Invalid);
        }
        let v = input.read();
        if v.version != 1
            || v.size < size_of::<a::GfxOwnedBufferRelease>() as u32
            || v.reserved0 != 0
        {
            return Err(Error::Invalid);
        }
        let owner = Owner {
            kind: 2,
            id: u64::from(v.driver_owner),
            generation: v.driver_generation,
        };
        let ticket = bo::OwnedRelease {
            release: bo::Release {
                buffer: api::handle(v.buffer)?,
                backing: bo::Backing {
                    cookie: v.cookie,
                    bytes: v.byte_length,
                    has_driver: true,
                    driver: owner,
                    ..bo::Backing::default()
                },
                attempt: v.attempt,
            },
            driver: owner,
            binding: Binding {
                adapter: v.adapter_id,
                driver_owner: v.driver_owner,
                device_generation: v.device_generation,
            },
        };
        let _call = buffers::Call::acquire()?;
        let _lock = api::Locked::acquire();
        buffers::table().finish_owned_release(ticket, identity, quiesced == 1)
    }
}
/// # Safety
/// Operation-specific resident C records and exact admitted R4D epoch remain
/// live. Physical quiescence is an explicit caller receipt, never inferred.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_owned_api(
    op: u32,
    args: *const bo::Args,
    out: *mut c_void,
) -> i32 {
    let a = unsafe { args.read() };
    let result = unsafe {
        match op {
            0 => lose(a.owner, a.access, a.offset, a.value as u32),
            1 => reserve(a.owner, a.input.cast(), a.value, out.cast()),
            2 => commit(a.owner, a.input.cast(), out.cast()),
            3 => abort(a.owner, a.input.cast(), a.value as u32),
            4 => take(a.owner, a.access, a.offset, out.cast()),
            5 => finish(a.owner, a.input.cast(), a.value as u32),
            _ => panic!("Invalid owned BO operation"),
        }
    };
    match result {
        Ok(()) => a::gfx_buffer_result_ok,
        Err(e) => api::status(e),
    }
}
