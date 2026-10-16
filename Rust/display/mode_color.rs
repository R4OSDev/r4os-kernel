//! Exact encoded color image retention, separate from the console shadow.
//! Caller serializes its private owner across admission. Only short BO views
//! are borrowed; no owner record reference spans allocation or blocking entry.
use super::{error::Error, mode_color_state, outputs};
use crate::{
    contract as a,
    gfx::{
        buffer_api, buffers,
        types::{Handle, Owner as Identity},
    },
};
pub const DISPLAY_OWNER: Identity = Identity {
    kind: 0,
    id: 3,
    generation: 1,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Owner {
    pub reference: Handle,
    pub driver_reference: Handle,
    pub read: Handle,
    pub driver: Identity,
}
impl Default for Owner {
    fn default() -> Self {
        Self {
            reference: Handle::EMPTY,
            driver_reference: Handle::EMPTY,
            read: Handle::EMPTY,
            driver: Identity {
                kind: 2,
                id: 0,
                generation: 0,
            },
        }
    }
}
impl Owner {
    pub fn empty(&self) -> bool {
        self.reference.id == 0 && self.driver_reference.id == 0 && self.read.id == 0
    }
}
struct Lock;
impl Drop for Lock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
fn validate_locked(
    caller: Identity,
    request: &a::GfxModeColorRequest,
    source: a::GfxOutputColorState,
) -> Result<Handle, Error> {
    let handle = buffer_api::handle(request.image)?;
    let descriptor = unsafe { buffers::read() }.describe(handle, caller)?;
    mode_color_state::validate(request, source, descriptor)?;
    let shadow = buffer_api::handle(request.state.assignments[0].buffer)?;
    if unsafe { buffers::read() }.buffer_for(handle, caller)?
        == unsafe { buffers::read() }.buffer_for(shadow, caller)?
    {
        return Err(Error::Invalid);
    }
    Ok(handle)
}
pub fn validate(caller: Identity, request: &a::GfxModeColorRequest) -> Result<(), Error> {
    if request.state.count != 1 {
        return Err(Error::Invalid);
    }
    let source = outputs::color_at(request.state.assignments[0].output)?;
    buffers::lock();
    let _lock = Lock;
    let _ = validate_locked(caller, request, source)?;
    Ok(())
}
fn drop_reference(reference: Handle, owner: Identity) {
    unsafe { buffers::table() }
        .drop_reference(reference, owner)
        .expect("exact mode image reference");
}
fn end_read(read: Handle) {
    unsafe { buffers::table() }
        .end_use(read, DISPLAY_OWNER, true)
        .expect("exact mode image read lease");
}
/// # Safety
/// Caller retains its exact private image owner and immutable request across
/// unlocked output admission and prepared BO growth. No other operation may
/// replace that owner until this attempt returns.
pub unsafe fn prepare(
    owner: *mut Owner,
    caller: Identity,
    driver: Identity,
    request: &a::GfxModeColorRequest,
) -> Result<a::GfxDriverModeColor, Error> {
    if !unsafe { &*owner }.empty() {
        return Err(Error::Busy);
    }
    if request.state.count != 1 {
        return Err(Error::Invalid);
    }
    let source = outputs::color_at(request.state.assignments[0].output)?;
    buffers::lock_prepared(buffers::Need {
        objects: 0,
        references: 2,
        leases: 1,
    })?;
    let _lock = Lock;
    let handle = validate_locked(caller, request, source)?;
    let descriptor = unsafe { buffers::read() }.describe(handle, caller)?;
    let reference = unsafe { buffers::table() }.share(handle, DISPLAY_OWNER)?;
    let driver_reference = match unsafe { buffers::table() }.share(handle, driver) {
        Ok(value) => value,
        Err(error) => {
            drop_reference(reference, DISPLAY_OWNER);
            return Err(error.into());
        }
    };
    let read = match unsafe { buffers::table() }.use_resource(
        reference,
        DISPLAY_OWNER,
        2,
        0,
        descriptor.bytes,
    ) {
        Ok(value) => value,
        Err(error) => {
            drop_reference(driver_reference, driver);
            drop_reference(reference, DISPLAY_OWNER);
            return Err(error.into());
        }
    };
    let wire = match unsafe { buffer_api::reference_locked(driver_reference, driver) } {
        Ok(value) => value,
        Err(error) => {
            end_read(read.lease);
            drop_reference(driver_reference, driver);
            drop_reference(reference, DISPLAY_OWNER);
            return Err(error.into());
        }
    };
    unsafe {
        owner.write(Owner {
            reference,
            driver_reference,
            read: read.lease,
            driver,
        })
    };
    Ok(a::GfxDriverModeColor {
        signal: request.signal,
        reference: wire,
        ..a::GfxDriverModeColor::default()
    })
}
/// # Safety
/// Caller owns the exact image lifetime and already established the physical
/// result that permits this release. Partial successful prefixes remain closed.
pub unsafe fn release(owner: *mut Owner) -> Result<(), Error> {
    buffers::lock();
    let _lock = Lock;
    let owner = unsafe { &mut *owner };
    if owner.read.id != 0 {
        unsafe { buffers::table() }.end_use(owner.read, DISPLAY_OWNER, true)?;
        owner.read = Handle::EMPTY
    }
    if owner.driver_reference.id != 0 {
        unsafe { buffers::table() }.drop_reference(owner.driver_reference, owner.driver)?;
        owner.driver_reference = Handle::EMPTY
    }
    if owner.reference.id != 0 {
        unsafe { buffers::table() }.drop_reference(owner.reference, DISPLAY_OWNER)?;
        owner.reference = Handle::EMPTY
    }
    Ok(())
}
/// # Safety
/// Exact initialized caller/driver/request and exclusive output; prepare/release
/// retain the same private owner/lifetime guarantees as their typed functions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_color_owner(
    op: u32,
    owner: *mut Owner,
    caller: *const Identity,
    driver: *const Identity,
    request: *const a::GfxModeColorRequest,
    out: *mut a::GfxDriverModeColor,
) -> i32 {
    let result = match op {
        0 => unsafe { prepare(owner, caller.read(), driver.read(), &*request) }
            .map(|value| unsafe { out.write(value) }),
        1 => unsafe { release(owner) },
        2 => validate(unsafe { caller.read() }, unsafe { &*request }),
        _ => panic!("mode color owner operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
const _: () = {
    assert!(core::mem::size_of::<Owner>() == 72 && core::mem::offset_of!(Owner, driver) == 48);
};
