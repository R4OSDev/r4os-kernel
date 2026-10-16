//! Exact retained cursor image reference/read lease and driver borrow. Caller
//! holds the common BO metadata owner; no pixel access or callback occurs here.
use super::error::Error;
use crate::{
    contract as a,
    gfx::{
        buffer_owner::{RawView, Table},
        buffers, layout,
        types::{Handle, Owner},
    },
};
pub const OWNER: Owner = Owner {
    kind: 0,
    id: 4,
    generation: 1,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub reference: Handle,
    pub borrowed: Handle,
    pub lease: Handle,
    pub driver: Owner,
}
impl Default for Source {
    fn default() -> Self {
        Self {
            reference: Handle::EMPTY,
            borrowed: Handle::EMPTY,
            lease: Handle::EMPTY,
            driver: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
        }
    }
}
impl Source {
    pub fn open(
        &mut self,
        store: &mut Table<'_>,
        caller: Owner,
        driver: Owner,
        request: a::DisplayCursorRequest,
    ) -> Result<(), Error> {
        if self.reference.id != 0 || self.borrowed.id != 0 || self.lease.id != 0 {
            return Err(Error::Busy);
        }
        let input = Handle {
            id: request.reference.id,
            generation: request.reference.generation,
        };
        let d = store.read_view().describe(input, caller)?;
        if d.location != 0
            || !d.binding.portable()
            || d.modifier != 0
            || d.format != layout::ARGB8888
            || d.plane_count != 1
            || d.planes[0].offset != 0
            || d.width != request.width
            || d.height != request.height
            || d.planes[0].pitch != request.pitch
            || d.bytes != request.byte_length
            || d.usage & 1 == 0
        {
            return Err(Error::Invalid);
        }
        self.driver = driver;
        let result = (|| {
            self.reference = store.share(input, OWNER)?;
            let held = store.use_resource(self.reference, OWNER, 0, 0, d.bytes)?;
            self.lease = held.lease;
            if held.backing.cache != 1 {
                return Err(Error::Unsupported);
            }
            self.borrowed = store.share(self.reference, driver)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.close(store);
        }
        result
    }
    pub fn close(&mut self, store: &mut Table<'_>) -> Result<(), Error> {
        if self.borrowed.id != 0 {
            store.drop_reference(self.borrowed, self.driver)?;
            self.borrowed = Handle::EMPTY
        }
        if self.lease.id != 0 {
            store.end_use(self.lease, OWNER, false)?;
            self.lease = Handle::EMPTY
        }
        if self.reference.id != 0 {
            store.drop_reference(self.reference, OWNER)?;
            self.reference = Handle::EMPTY
        }
        Ok(())
    }
    pub fn wire(&self) -> a::GfxBufferHandle {
        a::GfxBufferHandle {
            id: self.borrowed.id,
            generation: self.borrowed.generation,
            ..a::GfxBufferHandle::default()
        }
    }
}
/// # Safety
/// Exact initialized source and disjoint mutable BO metadata under their owner.
/// Null view selects the single resident global BO table, already admitted by
/// the caller. Explicit views lend fixed fixture/model storage only this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_cursor_source(
    op: u32,
    source: *mut Source,
    view: *const RawView,
    caller: *const Owner,
    driver: *const Owner,
    request: *const a::DisplayCursorRequest,
) -> i32 {
    let mut store = if view.is_null() {
        unsafe { buffers::table() }
    } else {
        unsafe { view.read().borrow() }
    };
    let source = unsafe { &mut *source };
    let result = match op {
        0 => source.open(
            &mut store,
            unsafe { caller.read() },
            unsafe { driver.read() },
            unsafe { request.read() },
        ),
        1 => source.close(&mut store),
        _ => panic!("cursor source operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
const _: () = {
    assert!(core::mem::size_of::<Source>() == 72 && core::mem::offset_of!(Source, driver) == 48);
};
