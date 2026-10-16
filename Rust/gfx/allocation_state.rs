//! Original bounded cross-owner allocation metadata. No placement, callback or wait.
use super::{
    layout::{self, Descriptor},
    types::{Error, Handle, Owner},
};
use crate::contract as a;
use core::{ffi::c_void, mem::size_of, ptr};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub handle: Handle,
    pub owner: Owner,
    pub provider: Handle,
    pub allocation: a::GfxNativeAllocation,
    pub phase: u32,
    pub result: i32,
    pub completed_ns: u64,
    pub open: bool,
    pub claimed: bool,
    pub reference: Handle,
    pub waiters: u32,
    pub notification_pending: bool,
    pub publishing: bool,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            handle: Handle::EMPTY,
            owner: Owner {
                kind: 1,
                id: 0,
                generation: 0,
            },
            provider: Handle::EMPTY,
            allocation: a::GfxNativeAllocation::default(),
            phase: 0,
            result: 0,
            completed_ns: 0,
            open: false,
            claimed: false,
            reference: Handle::EMPTY,
            waiters: 0,
            notification_pending: false,
            publishing: false,
        }
    }
}
impl Entry {
    pub fn status(&self) -> a::GfxNativeStatus {
        a::GfxNativeStatus {
            request: a::GfxBufferHandle {
                id: self.handle.id,
                generation: self.handle.generation,
                reserved0: 0,
            },
            phase: self.phase,
            result: self.result,
            flags: u32::from(self.claimed),
            deadline_ns: self.allocation.deadline_ns,
            completed_ns: self.completed_ns,
            ..a::GfxNativeStatus::default()
        }
    }
    pub fn terminal(&mut self, result: i32, instant: u64) {
        if self.phase == 2 {
            return;
        }
        self.phase = 2;
        self.result = result;
        self.completed_ns = instant;
        self.notification_pending = true;
    }
}
pub fn validate(input: a::GfxNativeAllocation, instant: u64) -> Result<(), Error> {
    if input.version != 1
        || input.size < size_of::<a::GfxNativeAllocation>() as u32
        || input.adapter_id == 0
        || input.memory_generation == 0
        || input.reserved0 != 0
        || instant == 0
        || input.deadline_ns <= instant
        || input.deadline_ns == u64::MAX
        || input.usage == 0
        || input.usage & !60 != 0
    {
        return Err(Error::Invalid);
    }
    if input.kind > 1 || input.layout > 1 {
        return Err(Error::Unsupported);
    }
    if input.kind == 0 {
        if input.byte_length == 0
            || input.width != 0
            || input.height != 0
            || input.format != 0
            || input.layout != 0
        {
            return Err(Error::Invalid);
        }
    } else {
        if input.byte_length != 0 || input.width == 0 || input.height == 0 || input.format == 0 {
            return Err(Error::Invalid);
        }
        if !matches!(
            input.format,
            layout::BYTES
                | layout::XRGB8888
                | layout::ARGB8888
                | layout::XRGB2101010
                | layout::ARGB2101010
                | layout::ABGR16161616F
                | layout::R8
                | layout::NV12
                | layout::P010
        ) {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}
pub fn matches(input: a::GfxNativeAllocation, d: Descriptor, driver: Owner) -> bool {
    d.location == 1
        && d.binding.adapter == input.adapter_id
        && d.binding.device_generation == input.memory_generation
        && u64::from(d.binding.driver_owner) == driver.id
        && d.usage & input.usage == input.usage
        && d.usage & 3 == 0
        && (if input.layout == 0 {
            d.modifier == 0
        } else {
            d.modifier != 0
        })
        && (if input.kind == 0 {
            d.format == layout::BYTES && d.bytes >= input.byte_length
        } else {
            d.width == input.width && d.height == input.height && d.format == input.format
        })
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub entries: *mut Entry,
    pub capacity: usize,
    pub serial: *mut u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    pub owner: Owner,
    pub provider: Handle,
    pub handle: Handle,
    pub input: *const a::GfxNativeAllocation,
    pub instant: u64,
    pub result: i32,
}
impl View {
    pub(crate) unsafe fn find(self, handle: Handle) -> Result<*mut Entry, Error> {
        if handle.id == 0 || handle.id as usize > self.capacity || handle.generation == 0 {
            return Err(Error::Stale);
        }
        let entry = unsafe { self.entries.add(handle.id as usize - 1) };
        if unsafe { (*entry).handle } != handle {
            return Err(Error::Stale);
        }
        Ok(entry)
    }
    pub(crate) unsafe fn owned(self, handle: Handle, owner: Owner) -> Result<*mut Entry, Error> {
        let entry = unsafe { self.find(handle)? };
        if unsafe { (*entry).owner } != owner {
            return Err(Error::WrongOwner);
        }
        if !unsafe { (*entry).open } {
            return Err(Error::Closed);
        }
        Ok(entry)
    }
    pub(crate) unsafe fn start(
        self,
        owner: Owner,
        provider: Handle,
        input: a::GfxNativeAllocation,
        instant: u64,
    ) -> Result<*mut Entry, Error> {
        validate(input, instant)?;
        if !owner.valid() || provider.id == 0 || provider.generation == 0 {
            return Err(Error::Invalid);
        }
        if unsafe { self.serial.read() } == u64::MAX {
            return Err(Error::Exhausted);
        }
        let mut count = 0;
        for i in 0..self.capacity {
            let e = unsafe { &*self.entries.add(i) };
            if e.handle.id != 0 && e.owner == owner {
                count += 1;
            }
        }
        if count >= 16 {
            return Err(Error::Budget);
        }
        for i in 0..self.capacity {
            let e = unsafe { self.entries.add(i) };
            if unsafe { (*e).handle.id } == 0 {
                unsafe {
                    *self.serial += 1;
                    e.write(Entry {
                        handle: Handle {
                            id: u32::try_from(i + 1).expect("allocation handle capacity"),
                            generation: *self.serial,
                        },
                        owner,
                        provider,
                        allocation: a::GfxNativeAllocation {
                            size: size_of::<a::GfxNativeAllocation>() as u32,
                            ..input
                        },
                        open: true,
                        ..Entry::default()
                    });
                }
                return Ok(e);
            }
        }
        Err(Error::Capacity)
    }
    pub(crate) unsafe fn expire(self, instant: u64) {
        for i in 0..self.capacity {
            let e = unsafe { &mut *self.entries.add(i) };
            if e.handle.id != 0 && e.phase != 2 && instant >= e.allocation.deadline_ns {
                e.terminal(a::gfx_queue_error_wait_timeout, instant);
            }
        }
    }
    pub(crate) unsafe fn take(self, provider: Handle, instant: u64) -> *mut Entry {
        unsafe {
            self.expire(instant);
        }
        let mut selected: *mut Entry = ptr::null_mut();
        for i in 0..self.capacity {
            let e = unsafe { self.entries.add(i) };
            unsafe {
                if (*e).handle.id != 0
                    && (*e).phase == 0
                    && (*e).provider == provider
                    && (selected.is_null()
                        || (*e).handle.generation < (*selected).handle.generation)
                {
                    selected = e;
                }
            }
        }
        if !selected.is_null() {
            unsafe {
                (*selected).phase = 1;
                (*selected).claimed = true;
            }
        }
        selected
    }
    pub(crate) unsafe fn claim(
        self,
        handle: Handle,
        provider: Handle,
    ) -> Result<*mut Entry, Error> {
        let e = unsafe { self.find(handle)? };
        if unsafe { (*e).provider } != provider {
            return Err(Error::WrongOwner);
        }
        if !unsafe { (*e).claimed } {
            return Err(Error::Closed);
        }
        Ok(e)
    }
    pub(crate) unsafe fn finish(
        self,
        handle: Handle,
        provider: Handle,
        result: i32,
        instant: u64,
    ) -> Result<(), Error> {
        if result == 0 || result > 1 {
            return Err(Error::Invalid);
        }
        let e = unsafe { &mut *self.claim(handle, provider)? };
        e.claimed = false;
        e.terminal(result, instant);
        Ok(())
    }
    pub(crate) unsafe fn close(
        self,
        handle: Handle,
        owner: Owner,
        instant: u64,
    ) -> Result<Handle, Error> {
        let e = unsafe { &mut *self.owned(handle, owner)? };
        e.open = false;
        e.terminal(a::gfx_queue_error_wait_cancelled, instant);
        let reference = e.reference;
        e.reference = Handle::EMPTY;
        Ok(reference)
    }
    pub(crate) unsafe fn close_provider(self, provider: Handle, instant: u64) {
        for i in 0..self.capacity {
            let e = unsafe { &mut *self.entries.add(i) };
            if e.handle.id != 0 && e.provider == provider {
                e.terminal(a::gfx_queue_error_device_lost, instant);
            }
        }
    }
    pub(crate) unsafe fn retains_provider(self, provider: Handle) -> bool {
        for i in 0..self.capacity {
            let e = unsafe { &*self.entries.add(i) };
            if e.handle.id != 0 && e.provider == provider && e.claimed {
                return true;
            }
        }
        false
    }
    pub(crate) unsafe fn reap(self) {
        for i in 0..self.capacity {
            let e = unsafe { &mut *self.entries.add(i) };
            if e.handle.id != 0
                && !e.open
                && !e.claimed
                && e.reference.id == 0
                && e.waiters == 0
                && !e.notification_pending
                && !e.publishing
            {
                *e = Entry::default();
            }
        }
    }
}
/// # Safety
/// The caller holds the original no-sleep owner over initialized disjoint
/// resident entries/scalar. Each operation's exact output record is writable,
/// disjoint and resident. Returned entry pointers cannot outlive that owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_state_run(
    view: *const View,
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let v = unsafe { view.read() };
    let args = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out.cast::<*mut Entry>().write(v.find(args.handle)?),
                1 => out
                    .cast::<*mut Entry>()
                    .write(v.owned(args.handle, args.owner)?),
                2 => out.cast::<*mut Entry>().write(v.start(
                    args.owner,
                    args.provider,
                    args.input.read(),
                    args.instant,
                )?),
                3 => out
                    .cast::<*mut Entry>()
                    .write(v.take(args.provider, args.instant)),
                4 => out
                    .cast::<*mut Entry>()
                    .write(v.claim(args.handle, args.provider)?),
                5 => v.finish(args.handle, args.provider, args.result, args.instant)?,
                6 => out
                    .cast::<Handle>()
                    .write(v.close(args.handle, args.owner, args.instant)?),
                7 => v.expire(args.instant),
                8 => v.close_provider(args.provider, args.instant),
                9 => out.cast::<bool>().write(v.retains_provider(args.provider)),
                10 => v.reap(),
                _ => panic!("Invalid private allocation operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// The exact resident entry is protected by its original metadata owner. A status
/// output is disjoint; mutation has exclusive access, without a callback or wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_entry(
    entry: *mut Entry,
    op: u32,
    result: i32,
    instant: u64,
    out: *mut a::GfxNativeStatus,
) {
    match op {
        0 => unsafe {
            out.write((&*entry).status());
        },
        1 => unsafe {
            (&mut *entry).terminal(result, instant);
        },
        _ => panic!("Invalid allocation entry operation"),
    }
}
/// # Safety
/// Input points to its complete initialized resident contract record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_validate(
    input: *const a::GfxNativeAllocation,
    instant: u64,
) -> i32 {
    match validate(unsafe { input.read() }, instant) {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Both inputs point to complete initialized resident records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_matches(
    input: *const a::GfxNativeAllocation,
    descriptor: *const Descriptor,
    driver: *const Owner,
) -> bool {
    matches(
        unsafe { input.read() },
        unsafe { descriptor.read() },
        unsafe { driver.read() },
    )
}

const _: () = {
    assert!(core::mem::size_of::<Entry>() == 168 && core::mem::align_of::<Entry>() == 8);
    assert!(core::mem::offset_of!(Entry, handle) == 0);
    assert!(core::mem::offset_of!(Entry, owner) == 16);
    assert!(core::mem::offset_of!(Entry, provider) == 40);
    assert!(core::mem::offset_of!(Entry, allocation) == 56);
    assert!(core::mem::offset_of!(Entry, phase) == 120);
    assert!(core::mem::offset_of!(Entry, result) == 124);
    assert!(core::mem::offset_of!(Entry, completed_ns) == 128);
    assert!(core::mem::offset_of!(Entry, open) == 136);
    assert!(core::mem::offset_of!(Entry, claimed) == 137);
    assert!(core::mem::offset_of!(Entry, reference) == 144);
    assert!(core::mem::offset_of!(Entry, waiters) == 160);
    assert!(core::mem::offset_of!(Entry, notification_pending) == 164);
    assert!(core::mem::offset_of!(Entry, publishing) == 165);
    assert!(core::mem::size_of::<View>() == 24 && core::mem::align_of::<View>() == 8);
    assert!(core::mem::offset_of!(View, entries) == 0);
    assert!(core::mem::offset_of!(View, capacity) == 8);
    assert!(core::mem::offset_of!(View, serial) == 16);
    assert!(core::mem::size_of::<Args>() == 80 && core::mem::align_of::<Args>() == 8);
    assert!(core::mem::offset_of!(Args, owner) == 0);
    assert!(core::mem::offset_of!(Args, provider) == 24);
    assert!(core::mem::offset_of!(Args, handle) == 40);
    assert!(core::mem::offset_of!(Args, input) == 56);
    assert!(core::mem::offset_of!(Args, instant) == 64);
    assert!(core::mem::offset_of!(Args, result) == 72);
};
