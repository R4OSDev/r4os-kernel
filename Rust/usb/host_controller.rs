//! Sole host-backend registry. Descriptor callbacks retain their original C
//! ABI. Raw slot identities, rather than Rust references, span callbacks.
use super::types::{
    CleanupResult, ControlRequest, Controller, HostDescriptor, HostDevice, HostEndpoint, HostStatus,
};
use core::{cell::UnsafeCell, ffi::c_void, mem, slice};

struct MetadataGuard(u64);
impl MetadataGuard {
    fn enter() -> Self {
        Self(crate::arch::runtime::save_runtime())
    }
}
impl Drop for MetadataGuard {
    fn drop(&mut self) {
        crate::arch::runtime::restore_runtime(self.0);
    }
}
const CAPACITY: usize = crate::platform::pci_scan::MAX_DEVICES;
static NEXT_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);
struct Storage(UnsafeCell<[Controller; CAPACITY]>);
// SAFETY: short metadata admission covers every slot read/mutation. The
// original driver/controller lifecycle still owns callback/descriptor lifetime.
// No metadata guard or Rust slot reference spans callbacks or logging.
unsafe impl Sync for Storage {}
static CONTROLLERS: Storage = Storage(UnsafeCell::new([Controller::EMPTY; CAPACITY]));
fn row(index: usize) -> *mut Controller {
    CONTROLLERS.0.get().cast::<Controller>().wrapping_add(index)
}
unsafe fn at(index: usize) -> Option<*mut Controller> {
    if index < CAPACITY && unsafe { (*row(index)).used } {
        Some(row(index))
    } else {
        None
    }
}
unsafe fn snapshot(index: usize) -> Option<Controller> {
    let _metadata = MetadataGuard::enter();
    unsafe { at(index) }.map(|pointer| unsafe { pointer.read() })
}
fn source_label(source: u8) -> &'static [u8] {
    match source {
        0 => b"built-in",
        1 => b"preload",
        2 => b"disk",
        _ => panic!("USB source"),
    }
}
unsafe fn find(name: &[u8]) -> Option<usize> {
    let _metadata = MetadataGuard::enter();
    for index in 0..CAPACITY {
        let c = unsafe { row(index).read() };
        if c.used
            && (c.name[..c.name_len].eq_ignore_ascii_case(name)
                || (name.eq_ignore_ascii_case(b"XHCI")
                    && c.name[..c.name_len].starts_with(b"XHCI@")))
        {
            return Some(index);
        }
    }
    None
}
unsafe fn account(index: usize, result: i32, transfer: bool) -> i32 {
    let _metadata = MetadataGuard::enter();
    let p = row(index);
    if transfer {
        unsafe { (*p).transfers = (*p).transfers.wrapping_add(1) }
    }
    if result < 0 {
        unsafe { (*p).failures = (*p).failures.wrapping_add(1) }
    }
    result
}
struct CallbackLease {
    descriptor: HostDescriptor,
    index: usize,
    id: u32,
}
impl core::ops::Deref for CallbackLease {
    type Target = HostDescriptor;
    fn deref(&self) -> &HostDescriptor {
        &self.descriptor
    }
}
impl Drop for CallbackLease {
    fn drop(&mut self) {
        let _metadata = MetadataGuard::enter();
        unsafe {
            let p = row(self.index);
            assert!(
                (*p).used && (*p).id == self.id && (*p).callbacks != 0,
                "USB callback epoch changed"
            );
            (*p).callbacks -= 1;
        }
    }
}
unsafe fn descriptor(index: usize) -> Result<CallbackLease, i32> {
    let _metadata = MetadataGuard::enter();
    let p = unsafe { at(index) }.ok_or(-1)?;
    unsafe {
        if (*p).closing {
            return Err(-4);
        }
        if (*p).descriptor.is_null() {
            return Err(account(index, -2, false));
        }
        (*p).callbacks = (*p).callbacks.checked_add(1).ok_or(-4)?;
        Ok(CallbackLease {
            descriptor: (*p).descriptor.read(),
            index,
            id: (*p).id,
        })
    }
}

unsafe fn callback(index: usize, address: usize) -> Result<usize, i32> {
    if address == 0 {
        Err(unsafe { account(index, -3, false) })
    } else {
        Ok(address)
    }
}
fn device_index(device: HostDevice) -> Option<usize> {
    let _metadata = MetadataGuard::enter();
    if device.controller_id == 0 {
        return None;
    }
    (0..CAPACITY).find(|i| unsafe { (*row(*i)).used && (*row(*i)).id == device.controller_id })
}

unsafe fn register(
    name: &[u8],
    descriptor: *const HostDescriptor,
    owner: u32,
    builtin: bool,
    log: Option<Logger>,
) -> Option<usize> {
    let (index, c) = {
        let _metadata = MetadataGuard::enter();

        if unsafe { find(name) }.is_some() {
            return None;
        }
        let index = (0..CAPACITY).find(|i| !unsafe { (*row(*i)).used })?;
        let id = NEXT_ID
            .try_update(
                core::sync::atomic::Ordering::Relaxed,
                core::sync::atomic::Ordering::Relaxed,
                |v| v.checked_add(1),
            )
            .ok()?;
        let mut c = Controller {
            id,
            used: true,
            source: if builtin {
                0
            } else {
                match unsafe { (*descriptor).source } {
                    1 => 1,
                    2 => 2,
                    _ => 0,
                }
            },
            owner_id: if builtin { 0 } else { owner },
            state: u8::from(builtin),
            descriptor,
            ..Controller::EMPTY
        };
        let n = if name.len() < c.name.len() {
            name.len()
        } else {
            c.name.len() - 1
        };
        c.name[..n].copy_from_slice(&name[..n]);
        c.name[n] = 0;
        c.name_len = n;
        unsafe { row(index).write(c) };

        (index, c)
    };
    if let Some(logger) = log {
        let write = |text: &[u8]| unsafe { logger(text.as_ptr(), text.len()) };
        write(if builtin {
            b"[USBHC] register built-in "
        } else {
            b"[USBHC] register backend "
        });
        write(&c.name[..c.name_len]);
        if !builtin {
            write(b" source=");
            write(source_label(c.source));
        }
        write(b"\r\n");
    }

    Some(index)
}
type Logger = unsafe extern "C" fn(*const u8, usize);
type Simple = unsafe extern "C" fn(*mut c_void) -> i32;
type Address = unsafe extern "C" fn(*mut c_void, u8, *mut HostDevice) -> i32;
type Configure = unsafe extern "C" fn(*mut c_void, *const HostDevice, u8) -> i32;
type Control = unsafe extern "C" fn(
    *mut c_void,
    *const HostDevice,
    *const ControlRequest,
    *mut u8,
    u32,
) -> i32;
type Bulk = unsafe extern "C" fn(*mut c_void, *const HostEndpoint, *mut u8, u32, u32) -> i32;
type Interrupt =
    unsafe extern "C" fn(*mut c_void, *const HostEndpoint, *mut u8, u32, *mut u32) -> i32;
type Port = unsafe extern "C" fn(*mut c_void, u8) -> i32;
type Endpoint = unsafe extern "C" fn(*mut c_void, *const HostEndpoint) -> i32;
type Status = unsafe extern "C" fn(*mut c_void, *mut HostStatus) -> i32;
unsafe fn unregister(index: usize, expected_id: u32, expected_owner: u32) -> bool {
    let c = {
        let _metadata = MetadataGuard::enter();
        let Some(p) = (unsafe { at(index) }) else {
            return false;
        };
        unsafe {
            if (*p).id != expected_id
                || (*p).owner_id != expected_owner
                || (*p).closing
                || (*p).callbacks != 0
            {
                return false;
            }
            (*p).closing = true;
            p.read()
        }
    };
    if !c.descriptor.is_null() {
        let d = unsafe { c.descriptor.read() };
        if d.shutdown != 0 && unsafe { mem::transmute::<usize, Simple>(d.shutdown)(d.context) } < 0
        {
            let _metadata = MetadataGuard::enter();
            unsafe {
                assert!((*row(index)).id == c.id);
                (*row(index)).closing = false;
                (*row(index)).state = 2;
                account(index, -1, false);
            }
            return false;
        }
    }
    let _metadata = MetadataGuard::enter();
    unsafe {
        assert!((*row(index)).id == c.id && (*row(index)).callbacks == 0 && (*row(index)).closing);
        row(index).write(Controller::EMPTY);
    }
    true
}

pub(crate) unsafe fn status_exact(id: u32, out: *mut HostStatus) -> i32 {
    let index = unsafe { r4os_rust_usb_host_index(id) };
    unsafe { status_expected(index, Some(id), out) }
}
unsafe fn status_at(index: usize, out: *mut HostStatus) -> i32 {
    unsafe { status_expected(index, None, out) }
}
unsafe fn status_expected(index: usize, expected: Option<u32>, out: *mut HostStatus) -> i32 {
    let Some(c) = (unsafe { snapshot(index) }) else {
        return -1;
    };
    if expected.is_some_and(|id| c.id != id) {
        return -1;
    }
    unsafe {
        out.write(HostStatus {
            state: u32::from(c.state),
            source: u32::from(c.source),
            transfers: c.transfers,
            failures: c.failures,
            polls: c.polls,
            ..HostStatus::EMPTY
        })
    };
    if c.descriptor.is_null() {
        return 0;
    }
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != c.id {
        return -1;
    }
    if d.status != 0 {
        let result = unsafe { mem::transmute::<usize, Status>(d.status)(d.context, out) };
        if result < 0 {
            return unsafe { account(index, result, false) };
        }
    }
    let c = unsafe { snapshot(index) }.unwrap_or(Controller::EMPTY);
    unsafe {
        (*out).state = u32::from(c.state);
        (*out).source = u32::from(c.source);
        (*out).flags |= d.flags;
        (*out).transfers = (*out).transfers.max(c.transfers);
        (*out).failures = (*out).failures.max(c.failures);
        (*out).polls = (*out).polls.max(c.polls);
    }
    0
}
unsafe fn simple(index: usize, poll: bool) -> i32 {
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    let address = match unsafe { callback(index, if poll { d.poll } else { d.port_scan }) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    if poll {
        let _metadata = MetadataGuard::enter();
        unsafe { (*row(index)).polls = (*row(index)).polls.wrapping_add(1) }
    }
    let result = unsafe { mem::transmute::<usize, Simple>(address)(d.context) };
    unsafe { account(index, result, false) }
}

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_reset() {
    let _metadata = MetadataGuard::enter();
    unsafe { CONTROLLERS.0.get().write([Controller::EMPTY; CAPACITY]) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_register(
    name: *const u8,
    len: usize,
    descriptor: *const HostDescriptor,
    owner: u32,
    builtin: bool,
    log: Option<Logger>,
) -> usize {
    unsafe {
        register(
            if len == 0 {
                &[]
            } else {
                slice::from_raw_parts(name, len)
            },
            descriptor,
            owner,
            builtin,
            log,
        )
    }
    .unwrap_or(usize::MAX)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_set_state(index: usize, state: u8) -> bool {
    let _metadata = MetadataGuard::enter();
    assert!(state <= 2);
    if let Some(p) = unsafe { at(index) } {
        unsafe { (*p).state = state };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_id(index: usize) -> u32 {
    unsafe { snapshot(index) }.map_or(0, |c| c.id)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_index(id: u32) -> usize {
    device_index(HostDevice {
        controller_id: id,
        ..HostDevice::EMPTY
    })
    .unwrap_or(usize::MAX)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_unregister(index: usize) -> bool {
    let Some(c) = (unsafe { snapshot(index) }) else {
        return false;
    };
    unsafe { unregister(index, c.id, c.owner_id) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_unregister_name(
    name: *const u8,
    len: usize,
    owner: u32,
) -> bool {
    let name = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(name, len) }
    };
    if name.eq_ignore_ascii_case(b"XHCI") {
        let mut found = false;
        let mut all = true;
        for index in 0..CAPACITY {
            let Some(c) = (unsafe { snapshot(index) }) else {
                continue;
            };
            if c.owner_id != owner
                || !(c.name[..c.name_len] == *b"XHCI" || c.name[..c.name_len].starts_with(b"XHCI@"))
            {
                continue;
            }
            found = true;
            all &= unsafe { unregister(index, c.id, owner) };
        }
        return found && all;
    }
    let Some(index) = (unsafe { find(name) }) else {
        return false;
    };
    let Some(c) = (unsafe { snapshot(index) }) else {
        return false;
    };
    unsafe { unregister(index, c.id, owner) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_cleanup(owner: u32, out: *mut CleanupResult) {
    let mut result = CleanupResult::EMPTY;
    if owner != 0 {
        for index in 0..CAPACITY {
            let Some(c) = (unsafe { snapshot(index) }) else {
                continue;
            };
            if c.owner_id != owner {
                continue;
            }
            if unsafe { unregister(index, c.id, owner) } {
                result.removed += 1;
            } else {
                result.failed = true;
            }
        }
    }
    unsafe { out.write(result) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_count() -> usize {
    let _metadata = MetadataGuard::enter();
    (0..CAPACITY).filter(|i| unsafe { (*row(*i)).used }).count()
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_at(index: usize, out: *mut Controller) -> bool {
    if let Some(value) = unsafe { snapshot(index) } {
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
pub unsafe extern "C" fn r4os_rust_usb_host_find(name: *const u8, len: usize) -> usize {
    unsafe {
        find(if len == 0 {
            &[]
        } else {
            slice::from_raw_parts(name, len)
        })
    }
    .unwrap_or(usize::MAX)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_status(index: usize, out: *mut HostStatus) -> i32 {
    unsafe { status_at(index, out) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_scan(index: usize) -> i32 {
    unsafe { simple(index, false) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_poll(index: usize) -> i32 {
    unsafe { simple(index, true) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_address(
    index: usize,
    port: u8,
    out: *mut HostDevice,
) -> i32 {
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    let f = match unsafe { callback(index, d.address_device) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe { mem::transmute::<usize, Address>(f)(d.context, port, out) };
    if result >= 0 {
        unsafe { (*out).controller_id = snapshot(index).map_or(0, |c| c.id) }
    }
    unsafe { account(index, result, false) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_configure(
    device: *const HostDevice,
    configuration: u8,
) -> i32 {
    let Some(index) = device_index(unsafe { *device }) else {
        return -1;
    };
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != unsafe { (*device).controller_id } {
        return -1;
    }
    let f = match unsafe { callback(index, d.configure_device) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe { mem::transmute::<usize, Configure>(f)(d.context, device, configuration) };
    unsafe { account(index, result, false) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_control(
    device: *const HostDevice,
    request: *const ControlRequest,
    buffer: *mut u8,
    len: usize,
) -> i32 {
    let Some(index) = device_index(unsafe { *device }) else {
        return -1;
    };
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != unsafe { (*device).controller_id } {
        return -1;
    }
    let f = match unsafe { callback(index, d.control_transfer) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe {
        mem::transmute::<usize, Control>(f)(
            d.context,
            device,
            request,
            buffer,
            u32::try_from(len).expect("USB control buffer"),
        )
    };
    unsafe { account(index, result, true) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_bulk(
    endpoint: *const HostEndpoint,
    buffer: *mut u8,
    len: usize,
    direction: u32,
) -> i32 {
    let Some(index) = device_index(unsafe { (*endpoint).device }) else {
        return -1;
    };
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != unsafe { (*endpoint).device.controller_id } {
        return -1;
    }
    let f = match unsafe { callback(index, d.bulk_transfer) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe {
        mem::transmute::<usize, Bulk>(f)(
            d.context,
            endpoint,
            buffer,
            u32::try_from(len).expect("USB bulk buffer"),
            direction,
        )
    };
    unsafe { account(index, result, true) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_interrupt(
    endpoint: *const HostEndpoint,
    buffer: *mut u8,
    len: usize,
    actual: *mut u32,
) -> i32 {
    let Some(index) = device_index(unsafe { (*endpoint).device }) else {
        return -1;
    };
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != unsafe { (*endpoint).device.controller_id } {
        return -1;
    }
    let f = match unsafe { callback(index, d.interrupt_transfer) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe {
        mem::transmute::<usize, Interrupt>(f)(
            d.context,
            endpoint,
            buffer,
            u32::try_from(len).expect("USB interrupt buffer"),
            actual,
        )
    };
    unsafe { account(index, result, true) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_reset_port(index: usize, port: u8) -> i32 {
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    let f = match unsafe { callback(index, d.reset_port) } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe { mem::transmute::<usize, Port>(f)(d.context, port) };
    unsafe { account(index, result, false) }
}
unsafe fn endpoint_action(endpoint: *const HostEndpoint, reset: bool) -> i32 {
    let Some(index) = device_index(unsafe { (*endpoint).device }) else {
        return -1;
    };
    let d = match unsafe { descriptor(index) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    if d.id != unsafe { (*endpoint).device.controller_id } {
        return -1;
    }
    let f = match unsafe {
        callback(
            index,
            if reset {
                d.reset_endpoint
            } else {
                d.clear_halt
            },
        )
    } {
        Ok(f) => f,
        Err(e) => return e,
    };
    let result = unsafe { mem::transmute::<usize, Endpoint>(f)(d.context, endpoint) };
    unsafe { account(index, result, false) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_clear_halt(endpoint: *const HostEndpoint) -> i32 {
    unsafe { endpoint_action(endpoint, false) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_host_reset_endpoint(endpoint: *const HostEndpoint) -> i32 {
    unsafe { endpoint_action(endpoint, true) }
}
