//! Original independent platform-input facts. No firmware or user policy runs
//! under the dedicated short metadata owner; publication precedes its wake.
use crate::{contract as a, memory::owner::Lock, platform::monotonic, service::desktop_events};
use core::{cell::UnsafeCell, mem, ptr};
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Error {
    Invalid = 1,
    Stale,
    Busy,
    Unsupported,
    Exhausted,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct State {
    pub value: a::PlatformInputSnapshot,
    pub owner: u32,
    pub epoch: u64,
    pub acpi_caps: u32,
    pub usb_present: bool,
}
impl State {
    pub const EMPTY: Self = Self {
        value: a::PlatformInputSnapshot {
            version: 1,
            size: mem::size_of::<a::PlatformInputSnapshot>() as u32,
            sequence: 0,
            brightness_up: 0,
            brightness_down: 0,
            lid_sequence: 0,
            lid_state: 0,
            capabilities: 0,
            sources: 0,
            reserved: 0,
            since_ns: 0,
        },
        owner: 0,
        epoch: 0,
        acpi_caps: 0,
        usb_present: false,
    };
    fn advance(&mut self, now: u64) -> Result<(), Error> {
        if now == 0 || now == u64::MAX || now < self.value.since_ns {
            return Err(Error::Invalid);
        }
        if self.value.sequence == u64::MAX {
            return Err(Error::Exhausted);
        }
        self.value.sequence += 1;
        self.value.since_ns = now;
        Ok(())
    }
    fn capabilities(&mut self) {
        self.value.capabilities = self.acpi_caps | u32::from(self.usb_present);
        self.value.sources = u32::from(self.owner != 0) | (u32::from(self.usb_present) << 1);
    }
    pub fn submit(
        &mut self,
        owner: u32,
        epoch: u64,
        kind: u32,
        value: u32,
        now: u64,
    ) -> Result<bool, Error> {
        if owner == 0 || epoch == 0 || kind == 0 || kind > 4 {
            return Err(Error::Invalid);
        }
        if kind == 4 {
            if value > 3 {
                return Err(Error::Invalid);
            }
            if self.owner != 0 && (owner != self.owner || epoch != self.epoch) {
                return Err(Error::Busy);
            }
            if value == 0 {
                return self.remove(owner, now);
            }
            if self.owner == owner && self.epoch == epoch && self.acpi_caps == value {
                return Ok(false);
            }
            self.advance(now)?;
            self.owner = owner;
            self.epoch = epoch;
            self.acpi_caps = value;
            if value & 2 == 0 && self.value.lid_state != 0 {
                self.value.lid_state = 0;
                self.value.lid_sequence = self.value.sequence;
            }
            self.capabilities();
            return Ok(true);
        }
        if self.owner != owner || self.epoch != epoch {
            return Err(Error::Stale);
        }
        if kind == 3 {
            if value > 2 {
                return Err(Error::Invalid);
            }
            if self.acpi_caps & 2 == 0 {
                return Err(Error::Unsupported);
            }
            if self.value.lid_state == value {
                return Ok(false);
            }
            self.advance(now)?;
            self.value.lid_state = value;
            self.value.lid_sequence = self.value.sequence;
            return Ok(true);
        }
        if self.acpi_caps & 1 == 0 {
            return Err(Error::Unsupported);
        }
        self.button(kind, value, now)
    }
    fn button(&mut self, kind: u32, value: u32, now: u64) -> Result<bool, Error> {
        if value != 1 || !matches!(kind, 1 | 2) {
            return Err(Error::Invalid);
        }
        let count = if kind == 1 {
            self.value.brightness_up
        } else {
            self.value.brightness_down
        };
        if count == u64::MAX {
            return Err(Error::Exhausted);
        }
        self.advance(now)?;
        if kind == 1 {
            self.value.brightness_up += 1
        } else {
            self.value.brightness_down += 1
        }
        Ok(true)
    }
    pub fn usb(&mut self, present: bool, now: u64) -> Result<bool, Error> {
        if self.usb_present == present {
            return Ok(false);
        }
        self.advance(now)?;
        self.usb_present = present;
        self.capabilities();
        Ok(true)
    }
    pub fn consumer(&mut self, usage: u32, now: u64) -> Result<bool, Error> {
        if !self.usb_present {
            return Err(Error::Unsupported);
        }
        let kind = match usage {
            0x6f => 1,
            0x70 => 2,
            _ => return Err(Error::Unsupported),
        };
        self.button(kind, 1, now)
    }
    pub fn remove(&mut self, owner: u32, now: u64) -> Result<bool, Error> {
        if self.owner == 0 || owner != self.owner {
            return Ok(false);
        }
        self.advance(now)?;
        self.owner = 0;
        self.epoch = 0;
        self.acpi_caps = 0;
        self.value.lid_state = 0;
        self.value.lid_sequence = self.value.sequence;
        self.capabilities();
        Ok(true)
    }
}
struct Storage(UnsafeCell<State>);
// SAFETY: every access holds the exact resident platform-input metadata lock.
unsafe impl Sync for Storage {}
static CURRENT: Storage = Storage(UnsafeCell::new(State::EMPTY));
static LOCK: Lock = Lock::new(3, 2);
fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let token = LOCK.acquire();
    let result = f(unsafe { &mut *CURRENT.0.get() });
    LOCK.release(token);
    result
}
fn stamp() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains an initialized aligned model or versioned output, exclusive for the call. Model mutation remains serialized by its owner.
pub unsafe extern "C" fn r4os_rust_input_platform_snapshot(
    out: *mut a::PlatformInputSnapshot,
) -> i32 {
    if out.is_null() || !out.is_aligned() {
        return -1;
    }
    if unsafe { ptr::addr_of!((*out).version).read() } != 1
        || unsafe { ptr::addr_of!((*out).size).read() }
            < mem::size_of::<a::PlatformInputSnapshot>() as u32
    {
        return -1;
    }
    let value = with_state(|s| s.value);
    unsafe { out.write(value) }
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_platform_submit(
    owner: u32,
    epoch: u64,
    kind: u32,
    value: u32,
) -> i32 {
    let now = stamp();
    let result = with_state(|s| s.submit(owner, epoch, kind, value, now));
    match result {
        Ok(changed) => {
            if changed {
                desktop_events::r4os_rust_desktop_signal()
            }
            a::driver_resource_ok
        }
        Err(Error::Busy) => a::driver_resource_error_busy,
        Err(Error::Stale) => a::driver_resource_error_stale,
        Err(_) => a::driver_resource_error_invalid,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_platform_remove(owner: u32) {
    let now = stamp();
    if with_state(|s| s.remove(owner, now)).unwrap_or(false) {
        desktop_events::r4os_rust_desktop_signal();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_platform_usb(present: bool) {
    let now = stamp();
    if with_state(|s| s.usb(present, now)).unwrap_or(false) {
        desktop_events::r4os_rust_desktop_signal();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_platform_consumer(usage: u32) {
    let now = stamp();
    if with_state(|s| s.consumer(usage, now)).unwrap_or(false) {
        desktop_events::r4os_rust_desktop_signal();
    }
}
// Pure caller-owned model entry for the original bounded Zig fixture.
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains an initialized aligned model or versioned output, exclusive for the call. Model mutation remains serialized by its owner.
pub unsafe extern "C" fn r4os_rust_input_platform_model(
    s: *mut State,
    operation: u8,
    owner: u32,
    epoch: u64,
    kind: u32,
    value: u32,
    now: u64,
) -> i32 {
    let state = unsafe { &mut *s };
    let result = match operation {
        0 => state.submit(owner, epoch, kind, value, now),
        1 => state.usb(value != 0, now),
        2 => state.consumer(value, now),
        3 => state.remove(owner, now),
        _ => panic!("Private platform input operation"),
    };
    match result {
        Ok(changed) => i32::from(changed),
        Err(e) => -(e as i32),
    }
}
const _: () = {
    assert!(mem::size_of::<State>() == 88);
    assert!(mem::align_of::<State>() == 8);
    assert!(mem::offset_of!(State, owner) == 64);
    assert!(mem::offset_of!(State, epoch) == 72);
    assert!(mem::offset_of!(State, acpi_caps) == 80);
    assert!(mem::offset_of!(State, usb_present) == 84);
};
