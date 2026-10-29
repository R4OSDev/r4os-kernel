//! Exact driver epochs and stable per-device records under the shared BO owner.
//! Resident intrusive nodes retain self/state addresses through page walks;
//! metadata lookup never allocates, walks pages or invokes device callbacks.
use super::types::{Error, Owner};
use crate::driver::owner_table::Rows;
use crate::{
    contract as a,
    driver::treap::{Node, Tree},
};
use core::{mem::size_of, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Epoch {
    pub identity: Owner,
    pub closing: bool,
    pub mmio_busy: bool,
    pub mmio_retained: bool,
    pub mmio_pending: bool,
    pub devices: usize,
}
impl Default for Epoch {
    fn default() -> Self {
        Self {
            identity: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
            closing: false,
            mmio_busy: false,
            mmio_retained: false,
            mmio_pending: false,
            devices: 0,
        }
    }
}
#[repr(C)]
pub struct Device {
    pub index: Node,
    pub self_address: usize,
    pub state_address: usize,
    pub owner: Owner,
    pub descriptor: a::GfxDeviceLease,
    pub busy: bool,
}
impl Default for Device {
    fn default() -> Self {
        Self {
            index: Node {
                key: 0,
                priority: 0,
                parent: ptr::null_mut(),
                children: [ptr::null_mut(); 2],
            },
            self_address: 0,
            state_address: 0,
            owner: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
            descriptor: a::GfxDeviceLease::default(),
            busy: false,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub epochs: Rows<Epoch>,
    pub devices: *mut Tree,
    pub identity: usize,
}
pub fn mapping_range(offset: u64, bytes: u64, address: u64, object_bytes: u64) -> bool {
    address != 0
        && (offset | address) & 4095 == 0
        && bytes != 0
        && offset < object_bytes
        && bytes <= object_bytes - offset
        && address <= u64::MAX - bytes
        && (bytes & 4095 == 0 || bytes == object_bytes - offset)
}
fn descriptor_equal(a: a::GfxDeviceLease, b: a::GfxDeviceLease) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.lease.id == b.lease.id
        && a.lease.reserved0 == b.lease.reserved0
        && a.lease.generation == b.lease.generation
        && a.byte_offset == b.byte_offset
        && a.byte_length == b.byte_length
        && a.gpu_virtual_address == b.gpu_virtual_address
        && a.device_generation == b.device_generation
        && a.adapter_id == b.adapter_id
        && a.driver_owner == b.driver_owner
        && a.access == b.access
        && a.address_space == b.address_space
        && a.dma_mask == b.dma_mask
}
impl View {
    pub(crate) unsafe fn find(self, id: u64) -> *mut Epoch {
        if id == 0 {
            return ptr::null_mut();
        }
        for i in 0..unsafe { self.epochs.len() } {
            let e = unsafe { self.epochs.get(i) };
            if unsafe { ptr::addr_of!((*e).identity.id).read() } == id {
                return e;
            }
        }
        ptr::null_mut()
    }
    pub(crate) unsafe fn exact(self, identity: Owner) -> Result<*mut Epoch, Error> {
        let e = unsafe { self.find(identity.id) };
        if e.is_null() || unsafe { ptr::addr_of!((*e).identity).read() } != identity {
            return Err(Error::Stale);
        }
        Ok(e)
    }
    pub(crate) unsafe fn bind(self, id: u32, generation: u64) -> bool {
        unsafe {
            if id == 0 || generation == 0 || !self.find(u64::from(id)).is_null() {
                return false;
            }
            for i in 0..self.epochs.len() {
                let e = self.epochs.get(i);
                if (*e).identity.id == 0 {
                    e.write(Epoch {
                        identity: Owner {
                            kind: 2,
                            id: u64::from(id),
                            generation,
                        },
                        ..Epoch::default()
                    });
                    return true;
                }
            }
            false
        }
    }
    pub(crate) unsafe fn owner(self, id: u32, admission: bool) -> Result<Owner, Error> {
        let e = unsafe { self.find(u64::from(id)) };
        if e.is_null() {
            return Err(Error::Stale);
        }
        if admission && unsafe { (*e).closing } {
            return Err(Error::Closed);
        }
        Ok(unsafe { ptr::addr_of!((*e).identity).read() })
    }
    pub(crate) unsafe fn retained(self, identity: Owner, pending_only: bool) -> bool {
        let Ok(e) = (unsafe { self.exact(identity) }) else {
            return true;
        };
        unsafe {
            if pending_only {
                (*e).mmio_busy || (*e).mmio_pending
            } else {
                (*e).mmio_busy || (*e).mmio_retained || (*e).mmio_pending || (*e).devices != 0
            }
        }
    }
    pub(crate) unsafe fn retire(self, identity: Owner) -> bool {
        let Ok(e) = (unsafe { self.exact(identity) }) else {
            return false;
        };
        if unsafe { self.retained(identity, false) } {
            return false;
        }
        unsafe { e.write(Epoch::default()) }
        true
    }
    pub(crate) unsafe fn reserve(
        self,
        identity: Owner,
        descriptor: a::GfxDeviceLease,
        record: *mut Device,
    ) -> Result<(), Error> {
        unsafe {
            let e = self.exact(identity)?;
            if (*e).closing {
                return Err(Error::Closed);
            }
            if descriptor.lease.id == 0
                || descriptor.lease.generation == 0
                || (*record).self_address != 0
                || (*record).state_address != 0
            {
                return Err(Error::Stale);
            }
            if !(*self.devices)
                .find(u64::from(descriptor.lease.id))
                .is_null()
            {
                return Err(Error::Stale);
            }
            if (*e).devices == usize::MAX {
                return Err(Error::Exhausted);
            }
            record.write(Device {
                self_address: record.addr(),
                state_address: self.identity,
                owner: identity,
                descriptor,
                busy: true,
                ..Device::default()
            });
            (*self.devices).insert(
                u64::from(descriptor.lease.id),
                ptr::addr_of_mut!((*record).index),
            );
            (*e).devices += 1;
            Ok(())
        }
    }
    pub(crate) unsafe fn matching(
        self,
        identity: Owner,
        descriptor: a::GfxDeviceLease,
    ) -> Result<*mut Device, Error> {
        unsafe {
            let _ = self.exact(identity)?;
            let node = (*self.devices).find(u64::from(descriptor.lease.id));
            if node.is_null() {
                return Err(Error::Stale);
            }
            let record = node.cast::<Device>();
            if (*record).self_address != record.addr()
                || (*record).state_address != self.identity
                || (*record).owner != identity
                || !descriptor_equal((*record).descriptor, descriptor)
            {
                return Err(Error::Stale);
            }
            if (*record).busy {
                return Err(Error::Busy);
            }
            Ok(record)
        }
    }
    pub(crate) unsafe fn detach(self, record: *mut Device) -> Result<(), Error> {
        unsafe {
            if (*record).self_address != record.addr() || (*record).state_address != self.identity {
                return Err(Error::Stale);
            }
            let e = self.exact((*record).owner)?;
            let node = ptr::addr_of_mut!((*record).index);
            if (*self.devices).find(u64::from((*record).descriptor.lease.id)) != node
                || (*e).devices == 0
            {
                return Err(Error::Stale);
            }
            (*self.devices).remove(node);
            (*e).devices -= 1;
            record.write(Device::default());
            Ok(())
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_mapping_range(
    offset: u64,
    bytes: u64,
    address: u64,
    object_bytes: u64,
) -> bool {
    mapping_range(offset, bytes, address, object_bytes)
}
/// # Safety
/// View describes the caller's serialized initialized epoch array and exact
/// resident tree; its identity is the original enclosing state address.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_bind(
    v: *const View,
    id: u32,
    generation: u64,
) -> bool {
    unsafe { v.read().bind(id, generation) }
}
/// # Safety
/// Same exact metadata admission; caller owns the aligned identity output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_owner(
    v: *const View,
    id: u32,
    admit: bool,
    out: *mut Owner,
) -> i32 {
    match unsafe { v.read().owner(id, admit) } {
        Ok(value) => {
            unsafe { out.write(value) }
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Same serialized exact state view; no later epoch can coexist with this close.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_close(v: *const View, id: u32) {
    let e = unsafe { v.read().find(u64::from(id)) };
    if !e.is_null() {
        unsafe { ptr::addr_of_mut!((*e).closing).write(true) }
    }
}
/// # Safety
/// Same metadata admission; readable identity names the exact epoch to retire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_retire(
    v: *const View,
    identity: *const Owner,
) -> bool {
    unsafe { v.read().retire(identity.read()) }
}
/// # Safety
/// Same metadata admission; missing/mismatching epochs remain conservatively held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_retained(
    v: *const View,
    identity: *const Owner,
    pending_only: bool,
) -> bool {
    unsafe { v.read().retained(identity.read(), pending_only) }
}
/// # Safety
/// Record is resident exclusive unused storage disjoint from the indexed tree;
/// caller retains it through actual common-BO unmap and confirmed detach.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_reserve(
    v: *const View,
    identity: *const Owner,
    descriptor: *const a::GfxDeviceLease,
    record: *mut Device,
) -> i32 {
    match unsafe { v.read().reserve(identity.read(), descriptor.read(), record) } {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// View/identity/descriptor remain resident; output borrows the exact indexed
/// record only until the original shared metadata owner is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_matching(
    v: *const View,
    identity: *const Owner,
    descriptor: *const a::GfxDeviceLease,
    out: *mut *mut Device,
) -> i32 {
    match unsafe { v.read().matching(identity.read(), descriptor.read()) } {
        Ok(record) => {
            unsafe { out.write(record) }
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Caller already ended this record's common BO use. Storage remains resident
/// and owned until successful detach; actual heap release follows unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_detach(
    v: *const View,
    record: *mut Device,
) -> i32 {
    match unsafe { v.read().detach(record) } {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Exact initialized epoch and serialized metadata; begin/end retain the
/// existing single MMIO operation and completed/retained/pending distinction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_model_mmio(
    v: *const View,
    identity: *const Owner,
    begin: bool,
    retained: bool,
    pending: bool,
) {
    unsafe {
        let e = v
            .read()
            .exact(identity.read())
            .expect("Original exact MMIO epoch");
        if begin {
            assert!(!(*e).mmio_busy);
            (*e).mmio_busy = true;
        } else {
            assert!((*e).mmio_busy);
            (*e).mmio_busy = false;
            (*e).mmio_retained = retained;
            (*e).mmio_pending = pending;
        }
    }
}
const _: () = {
    assert!(size_of::<Epoch>() == 40);
    assert!(core::mem::align_of::<Epoch>() == 8);
    assert!(core::mem::offset_of!(Epoch, identity) == 0);
    assert!(core::mem::offset_of!(Epoch, closing) == 24);
    assert!(core::mem::offset_of!(Epoch, mmio_busy) == 25);
    assert!(core::mem::offset_of!(Epoch, mmio_retained) == 26);
    assert!(core::mem::offset_of!(Epoch, mmio_pending) == 27);
    assert!(core::mem::offset_of!(Epoch, devices) == 32);
    assert!(size_of::<Device>() == 168);
    assert!(core::mem::align_of::<Device>() == 8);
    assert!(core::mem::offset_of!(Device, index) == 0);
    assert!(core::mem::offset_of!(Device, self_address) == 40);
    assert!(core::mem::offset_of!(Device, state_address) == 48);
    assert!(core::mem::offset_of!(Device, owner) == 56);
    assert!(core::mem::offset_of!(Device, descriptor) == 80);
    assert!(core::mem::offset_of!(Device, busy) == 160);
    assert!(size_of::<View>() == 24);
    assert!(core::mem::offset_of!(View, epochs) == 0);
    assert!(core::mem::offset_of!(View, devices) == 8);
    assert!(core::mem::offset_of!(View, identity) == 16);
};
