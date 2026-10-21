//! Sole USB device catalogue. The caller holds the controller UnwindGuard;
//! passive readers receive fixed copied records under short runtime admission.
use super::types::{CoreSummary, Device};
use crate::arch::runtime::RuntimeGuard;
use core::{cell::UnsafeCell, ptr, slice};
const CAPACITY: usize = crate::platform::pci_scan::MAX_DEVICES * 8;
struct State {
    devices: [Device; CAPACITY],
    count: u32,
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
    unsafe {
        let s = state();
        for i in 0..(*s).count as usize {
            (*s).devices[i] = Device::EMPTY;
        }
        (*s).count = 0;
        (*s).truncated = false;
    }
}
/// Remove only this exact registration epoch; other controllers stay published.
pub(super) unsafe fn remove_controller(controller_id: u32) {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &mut *state() };
    let mut keep = 0usize;
    for i in 0..s.count as usize {
        if s.devices[i].controller_id != controller_id {
            s.devices[keep] = s.devices[i];
            keep += 1;
        }
    }
    for i in keep..s.count as usize {
        s.devices[i] = Device::EMPTY;
    }
    s.count = keep as u32;
}
pub(super) unsafe fn publish(mut device: Device) -> Option<usize> {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &mut *state() };
    device.active = true;
    if s.count as usize >= CAPACITY {
        s.truncated = true;
        return None;
    }
    let index = s.count as usize;
    s.devices[index] = device;
    s.count += 1;
    Some(index)
}
pub(super) unsafe fn publish_or_replace(mut device: Device) -> Option<usize> {
    let _guard = RuntimeGuard::acquire();
    device.active = true;
    for index in 0..unsafe { (*state()).count } as usize {
        let current = unsafe { ptr::addr_of!((*state()).devices[index]).read() };
        if current.port == device.port && current.controller_id == device.controller_id {
            unsafe { ptr::addr_of_mut!((*state()).devices[index]).write(device) };
            return Some(index);
        }
    }
    unsafe { publish(device) }
}
pub(super) unsafe fn remove(controller: crate::sched::sync_types::Text, port: u8) -> bool {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &mut *state() };
    for index in 0..s.count as usize {
        if s.devices[index].port != port
            || !unsafe { same_controller(s.devices[index].controller, controller) }
        {
            continue;
        }
        s.devices.copy_within(index + 1..s.count as usize, index);
        s.count = s.count.saturating_sub(1);
        s.devices[s.count as usize] = Device::EMPTY;
        return true;
    }
    false
}
pub(super) unsafe fn count_controller(id: u32) -> usize {
    let _guard = RuntimeGuard::acquire();
    unsafe {
        (&(*state()).devices)[..(*state()).count as usize]
            .iter()
            .filter(|d| d.controller_id == id)
            .count()
    }
}
pub(super) unsafe fn count() -> usize {
    let _guard = RuntimeGuard::acquire();
    unsafe { (*state()).count as usize }
}
pub(super) unsafe fn device_at(index: usize) -> Option<Device> {
    let _guard = RuntimeGuard::acquire();
    if index >= unsafe { (*state()).count as usize } {
        return None;
    }
    Some(unsafe { ptr::addr_of!((*state()).devices[index]).read() })
}
pub(super) unsafe fn summary() -> CoreSummary {
    let _guard = RuntimeGuard::acquire();
    let s = unsafe { &*state() };
    CoreSummary {
        count: s.count,
        configured: s.devices[..s.count as usize]
            .iter()
            .filter(|d| d.configured)
            .count() as u32,
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
