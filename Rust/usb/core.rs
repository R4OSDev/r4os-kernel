//! Sole USB device catalogue. The caller holds the controller UnwindGuard;
//! passive readers receive fixed copied records under short runtime admission.
use super::types::{CoreSummary, Device};
use crate::arch::runtime::RuntimeGuard;
use core::{cell::UnsafeCell, ptr, slice};
const CAPACITY: usize = 8;
struct State {
    devices: [Device; CAPACITY],
    count: u8,
    truncated: bool,
}
impl State {
    const EMPTY: Self = Self {
        devices: [Device::EMPTY; CAPACITY],
        count: 0,
        truncated: false,
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: every read/mutation holds runtime admission; writer sequencing also
// retains the existing controller owner. No registry pointer escapes.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn state() -> *mut State {
    STATE.0.get()
}
unsafe fn same_controller(
    a: crate::sched::sync_types::Text,
    b: crate::sched::sync_types::Text,
) -> bool {
    if a.len != b.len {
        return false;
    }
    if a.len == 0 {
        return true;
    }
    unsafe { slice::from_raw_parts(a.pointer, a.len) == slice::from_raw_parts(b.pointer, b.len) }
}
pub(super) unsafe fn reset() {
    let _guard = RuntimeGuard::acquire();
    unsafe { state().write(State::EMPTY) };
}
pub(super) unsafe fn publish(mut device: Device) -> Option<usize> {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &mut *state() };
    device.active = true;
    if usize::from(s.count) >= CAPACITY {
        s.truncated = true;
        return None;
    }
    let index = usize::from(s.count);
    s.devices[index] = device;
    s.count += 1;
    Some(index)
}
pub(super) unsafe fn publish_or_replace(mut device: Device) -> Option<usize> {
    let _guard = RuntimeGuard::acquire();
    device.active = true;
    for index in 0..unsafe { (*state()).count } as usize {
        let current = unsafe { ptr::addr_of!((*state()).devices[index]).read() };
        if current.port == device.port
            && unsafe { same_controller(current.controller, device.controller) }
        {
            unsafe { ptr::addr_of_mut!((*state()).devices[index]).write(device) };
            return Some(index);
        }
    }
    unsafe { publish(device) }
}
pub(super) unsafe fn remove(controller: crate::sched::sync_types::Text, port: u8) -> bool {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &mut *state() };
    for index in 0..usize::from(s.count) {
        if s.devices[index].port != port
            || !unsafe { same_controller(s.devices[index].controller, controller) }
        {
            continue;
        }
        s.devices
            .copy_within(index + 1..usize::from(s.count), index);
        s.count = s.count.saturating_sub(1);
        s.devices[usize::from(s.count)] = Device::EMPTY;
        return true;
    }
    false
}
pub(super) unsafe fn count() -> usize {
    let _guard = RuntimeGuard::acquire();
    unsafe { usize::from((*state()).count) }
}
pub(super) unsafe fn device_at(index: usize) -> Option<Device> {
    let _guard = RuntimeGuard::acquire();
    if index >= unsafe { usize::from((*state()).count) } {
        return None;
    }
    Some(unsafe { ptr::addr_of!((*state()).devices[index]).read() })
}
pub(super) unsafe fn summary() -> CoreSummary {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &*state() };
    CoreSummary {
        count: s.count,
        configured: s.devices[..usize::from(s.count)]
            .iter()
            .filter(|d| d.configured)
            .count() as u8,
        truncated: s.truncated,
    }
}

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_reset() {
    unsafe { reset() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_first(device: *const Device) {
    unsafe {
        reset();
        publish(*device)
    };
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_publish(device: *const Device) -> usize {
    unsafe { publish(*device) }.unwrap_or(usize::MAX)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_replace(device: *const Device) -> usize {
    unsafe { publish_or_replace(*device) }.unwrap_or(usize::MAX)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_remove(label: *const u8, len: usize, port: u8) -> bool {
    unsafe {
        remove(
            crate::sched::sync_types::Text {
                pointer: label,
                len,
            },
            port,
        )
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_count() -> usize {
    unsafe { count() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_at(index: usize, out: *mut Device) -> bool {
    if let Some(value) = unsafe { device_at(index) } {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_core_summary(out: *mut CoreSummary) {
    unsafe { out.write(summary()) }
}
