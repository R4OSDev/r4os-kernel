//! Native medium registry and exact per-device BOT ownership. Published media
//! remain resident; a transaction pins the original xHCI runtime across waits.
use super::{
    core as usb_core, msc_bot, msc_retry,
    msc_scsi::{self, RetryKind},
    msc_types::Status,
    xhci,
    xhci_api::Guard,
    xhci_owner,
    xhci_types::{DeviceHandle, DeviceTransaction, EndpointHandle},
};
use crate::{
    boot::foundation::puts,
    fs::vfs_types::Bytes,
    loader::r4p,
    sched::{
        sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue},
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};
const MAX_MEDIA: usize = 16;
const MAX_TRANSFER_BYTES: u32 = 64 * 1024;
const OWNERSHIP_TIMEOUT_TICKS: u64 = 15 * crate::arch::pit::DEFAULT_HZ as u64;
const fn guard(name: &'static [u8]) -> UnwindGuard {
    UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(name),
        queue: WaitQueue::EMPTY,
    }
}
#[repr(C)]
pub(super) struct Medium {
    pub state: Status,
    pub index: usize,
    pub ownership: UnwindGuard,
    pub data_buffer: [u8; 4096],
}
impl Medium {
    const EMPTY: Self = Self {
        state: Status::EMPTY,
        index: 0,
        ownership: guard(b"usbmsc-device-bot"),
        data_buffer: [0; 4096],
    };
}
struct Storage(UnsafeCell<[Medium; MAX_MEDIA]>);
// SAFETY: every medium has its original reentrant BOT guard. Slots, guards
// and buffers remain resident; no global Rust borrow spans callbacks/waits.
unsafe impl Sync for Storage {}
static MEDIA: Storage = Storage(UnsafeCell::new([const { Medium::EMPTY }; MAX_MEDIA]));
struct RegistryGuard(UnsafeCell<UnwindGuard>);
// SAFETY: only the synchronization owner mutates the guard's queue/identity.
unsafe impl Sync for RegistryGuard {}
static REGISTRY: RegistryGuard = RegistryGuard(UnsafeCell::new(guard(b"usbmsc-registry")));
static MEDIA_COUNT: AtomicUsize = AtomicUsize::new(0);
static LAST_MEDIUM: AtomicUsize = AtomicUsize::new(0);
static SOURCE: AtomicU8 = AtomicU8::new(0);
#[derive(Clone, Copy)]
struct DeviceBinding {
    descriptor: super::types::Device,
    device: DeviceHandle,
    input: EndpointHandle,
    output: EndpointHandle,
}
unsafe extern "C" {
    fn r4os_zig_usb_msc_register(current: *mut Status, index: usize, source: u8) -> usize;
}
fn item(index: usize) -> *mut Medium {
    assert!(index < MAX_MEDIA);
    unsafe { MEDIA.0.get().cast::<Medium>().add(index) }
}
pub(super) unsafe fn medium(current: *mut Status) -> *mut Medium {
    unsafe {
        current
            .cast::<u8>()
            .sub(core::mem::offset_of!(Medium, state))
            .cast()
    }
}
struct Admission(*mut UnwindGuard);
impl Admission {
    fn enter(g: *mut UnwindGuard) -> Option<Self> {
        if unsafe { sync_locks::guard_enter(g, OWNERSHIP_TIMEOUT_TICKS) } {
            Some(Self(g))
        } else {
            None
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        unsafe {
            sync_locks::guard_leave(self.0);
        }
    }
}
struct Operation {
    transaction: DeviceTransaction,
    _admission: Admission,
}
impl Drop for Operation {
    fn drop(&mut self) {
        unsafe {
            xhci_owner::end_transaction(self.transaction);
        }
    }
}
unsafe fn begin_operation(current: *mut Status) -> Option<Operation> {
    unsafe {
        let admission = Admission::enter(ptr::addr_of_mut!((*medium(current)).ownership))?;
        let transaction = xhci_owner::begin_transaction(ptr::addr_of_mut!((*current).device))?;
        LAST_MEDIUM.store((*medium(current)).index, Ordering::Release);
        Some(Operation {
            transaction,
            _admission: admission,
        })
    }
}
fn role(name: &'static [u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}
fn active(name: &'static [u8]) -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role(name)) }
}
fn source_name(name: &'static [u8]) -> Text {
    let s = unsafe { r4p::r4os_rust_r4p_source(role(name), true) };
    Text {
        pointer: s.data,
        len: s.len,
    }
}
fn missing_reason() -> Text {
    Text::literal(match (active(b"usb.msc_bot"), active(b"usb.scsi_block")) {
        (false, false) => b"USBBOT.R4P and USBSCSI.R4P required",
        (false, true) => b"USBBOT.R4P required",
        _ => b"USBSCSI.R4P required",
    })
}
unsafe fn require_roles(current: *mut Status) -> bool {
    if active(b"usb.msc_bot") && active(b"usb.scsi_block") {
        return true;
    }
    unsafe {
        (*current).protocol_required_missing = (*current).protocol_required_missing.wrapping_add(1);
        (*current).reason = missing_reason();
    }
    false
}
pub fn set_owner(source: u8) {
    assert!(source <= 2);
    SOURCE.store(source, Ordering::Release);
}
pub fn has_external_owner() -> bool {
    SOURCE.load(Ordering::Acquire) != 0
}
pub fn init() -> bool {
    let Some(_registry) = Admission::enter(REGISTRY.0.get()) else {
        return false;
    };
    if MEDIA_COUNT.load(Ordering::Acquire) != 0 {
        return block_device_count() != 0;
    }
    let mut index = 0;
    loop {
        let binding = unsafe {
            let Some(_controller) = Guard::enter() else {
                return false;
            };
            usb_core::device_at(index).map(|dev| DeviceBinding {
                descriptor: dev,
                device: xhci::device_handle_from_core(dev),
                input: xhci::bulk_in_handle_from_core(dev),
                output: xhci::bulk_out_handle_from_core(dev),
            })
        };
        index += 1;
        let Some(selected) = binding else {
            break;
        };
        let dev = selected.descriptor;
        if dev.first_interface_class != 8
            || dev.first_interface_subclass != 6
            || dev.first_interface_protocol != 0x50
        {
            continue;
        }
        let n = MEDIA_COUNT.load(Ordering::Acquire);
        if n == MAX_MEDIA {
            break;
        }
        unsafe {
            let entry = item(n);
            let Some(_medium) = Admission::enter(ptr::addr_of_mut!((*entry).ownership)) else {
                return false;
            };
            (*entry).index = n;
            let current = ptr::addr_of_mut!((*entry).state);
            current.write(Status {
                initialized: true,
                ..Status::EMPTY
            });
            initialize_medium(current, selected);
            // Registration identity is immutable after this publication. The
            // counters/error state remain exclusively under the medium guard.
            MEDIA_COUNT.store(n + 1, Ordering::Release);
        }
    }
    block_device_count() != 0
}
unsafe fn initialize_medium(current: *mut Status, binding: DeviceBinding) -> bool {
    unsafe {
        let dev = binding.descriptor;
        (*current).present = true;
        (*current).port = dev.port;
        (*current).device = binding.device;
        (*current).bulk_in_handle = binding.input;
        (*current).bulk_out_handle = binding.output;
        (*current).interface_number = dev.first_interface_number;
        (*current).bulk_in = dev.bulk_in_endpoint_address;
        (*current).bulk_out = dev.bulk_out_endpoint_address;
        (*current).bulk_in_max_packet = dev.bulk_in_endpoint_max_packet;
        (*current).bulk_out_max_packet = dev.bulk_out_endpoint_max_packet;
        if (*current).bulk_in == 0 || (*current).bulk_out == 0 {
            (*current).failures += 1;
            (*current).reason = Text::literal(b"bulk endpoints missing");
            return false;
        }
        let Some(_operation) = begin_operation(current) else {
            return false;
        };
        if !require_roles(current) || !ensure_selected(current) {
            return false;
        }
        (*current).bound = true;
        (*current).inquiry_ok = msc_scsi::with_retry(current, RetryKind::Inquiry);
        if !(*current).inquiry_ok {
            return false;
        }
        (*current).test_unit_ready_ok = msc_scsi::with_retry(current, RetryKind::Ready);
        if !(*current).test_unit_ready_ok {
            (*current).reason = Text::literal(b"SCSI device not ready after retry window");
            return false;
        }
        (*current).read_capacity_ok = msc_scsi::with_retry(current, RetryKind::Capacity);
        if !(*current).read_capacity_ok {
            (*current).reason = Text::literal(b"SCSI READ CAPACITY failed");
            return false;
        }
        (*current).mode_sense_ok = msc_scsi::mode_sense6(current);
        if !valid_logical_block_size((*current).sector_size) {
            (*current).reason = Text::literal(b"unsupported logical block size");
            return false;
        }
        (*current).max_sectors_per_request =
            (MAX_TRANSFER_BYTES / (*current).sector_size).min(65535) as u16;
        let registered = r4os_zig_usb_msc_register(
            current,
            (*medium(current)).index,
            SOURCE.load(Ordering::Acquire),
        );
        if registered != usize::MAX {
            (*current).block_index = registered;
            (*current).block_registered = true;
        }
        (*current).reason = Text::literal(if (*current).block_registered {
            b"USB mass storage block device active"
        } else {
            b"block register failed"
        });
        (*current).block_registered
    }
}
pub fn block_device_count() -> usize {
    // These two registration fields are written before Release publication,
    // then retained unchanged through media-change/transport-fault handling.
    let mut count = 0;
    for i in 0..MEDIA_COUNT.load(Ordering::Acquire) {
        if unsafe { (*item(i)).state.block_registered } {
            count += 1;
        }
    }
    count
}
pub fn device_index() -> Option<usize> {
    for i in 0..MEDIA_COUNT.load(Ordering::Acquire) {
        unsafe {
            if (*item(i)).state.block_registered {
                return Some((*item(i)).state.block_index);
            }
        }
    }
    None
}
pub fn reselect_active_device() -> bool {
    for i in 0..MEDIA_COUNT.load(Ordering::Acquire) {
        unsafe {
            if (*item(i)).state.block_registered {
                let current = ptr::addr_of_mut!((*item(i)).state);
                let Some(_op) = begin_operation(current) else {
                    return false;
                };
                return ensure_selected(current);
            }
        }
    }
    false
}
fn valid_logical_block_size(bytes: u32) -> bool {
    (512..=MAX_TRANSFER_BYTES).contains(&bytes) && bytes & (bytes - 1) == 0
}
unsafe fn ensure_selected(current: *mut Status) -> bool {
    let Some(_g) = Guard::enter() else {
        return false;
    };
    unsafe {
        if (*current).media_changed
            || ((*current).block_registered && !xhci::device_handle_current((*current).device))
        {
            (*current).media_changed = true;
            (*current).failures += 1;
            (*current).reason =
                Text::literal(b"USBMSC medium identity changed; old registration retired from I/O");
            return false;
        }
        if (*current).transport_faulted {
            (*current).failures += 1;
            (*current).reason = Text::literal(b"USBMSC transport recovery incomplete");
            return false;
        }
        let xs = xhci::status();
        let exact = (*current).device.slot_id != 0
            && xs.addressed_slot_id == (*current).device.slot_id
            && xs.addressed_port == (*current).device.port;
        if exact && (xs.bulk_endpoints_faulted || xs.control_endpoint_faulted) {
            (*current).failures += 1;
            (*current).reason = Text::literal(b"USBMSC xHCI endpoint faulted");
            return false;
        }
        if !exact || !xs.bulk_endpoints_configured {
            if !xhci::select_device_handle(ptr::addr_of_mut!((*current).device)) {
                (*current).failures += 1;
                (*current).reason = Text::literal(b"USBMSC device select failed");
                return false;
            }
            let selected = xhci::status();
            if selected.bulk_endpoints_faulted || selected.control_endpoint_faulted {
                (*current).failures += 1;
                (*current).reason = Text::literal(b"USBMSC xHCI endpoint faulted");
                return false;
            }
            if selected.addressed_slot_id == (*current).device.slot_id
                && selected.addressed_port == (*current).device.port
                && selected.bulk_endpoints_configured
            {
                return true;
            }
            if !xhci::set_configuration_for_handle(ptr::addr_of_mut!((*current).device)) {
                (*current).failures += 1;
                (*current).reason = Text::literal(b"SET_CONFIGURATION failed");
                return false;
            }
            if !xhci::configure_bulk_endpoint_handles(
                ptr::addr_of_mut!((*current).bulk_in_handle),
                ptr::addr_of_mut!((*current).bulk_out_handle),
            ) {
                (*current).failures += 1;
                (*current).reason = Text::literal(b"bulk endpoint configure failed");
                return false;
            }
        }
        true
    }
}
pub(super) unsafe fn read_block(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        let Some(_operation) = begin_operation(current) else {
            return false;
        };
        if sectors == 0
            || sectors > (*current).max_sectors_per_request
            || len < usize::from(sectors) * (*current).sector_size as usize
        {
            (*current).read_failures += 1;
            return false;
        }
        if !ensure_selected(current) {
            (*current).read_failures += 1;
            return false;
        }
        let count = usize::from(sectors) * (*current).sector_size as usize;
        if !msc_scsi::read(current, lba, sectors, out, count) {
            if msc_retry::retry_transport(
                (*current).last_failure_transport,
                (*current).last_recovery_ok,
                0,
            ) {
                (*current).read_transport_retries += 1;
                msc_scsi::retry_delay(
                    current,
                    b"usbmsc-read-retry",
                    if msc_scsi::uses_read_write16(lba, sectors) {
                        0x88
                    } else {
                        0x28
                    },
                    1,
                );
                if ensure_selected(current) && msc_scsi::read(current, lba, sectors, out, count) {
                    (*current).read_transport_retry_successes += 1;
                    (*current).reads += 1;
                    puts(b"[USBMSC] READ transport retry recovered\n");
                    return true;
                }
            }
            if !(*current).last_failure_transport {
                msc_scsi::request_sense(current);
            }
            (*current).read_failures += 1;
            return false;
        }
        (*current).reads += 1;
        true
    }
}
pub(super) unsafe fn write_block(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    bytes: *const u8,
    len: usize,
) -> bool {
    unsafe {
        let Some(_operation) = begin_operation(current) else {
            return false;
        };
        if sectors == 0
            || sectors > (*current).max_sectors_per_request
            || len < usize::from(sectors) * (*current).sector_size as usize
        {
            (*current).write_failures += 1;
            return false;
        }
        if !ensure_selected(current) {
            (*current).write_failures += 1;
            return false;
        }
        let count = usize::from(sectors) * (*current).sector_size as usize;
        if !msc_scsi::write(current, lba, sectors, bytes, count) {
            if msc_retry::retry_transport(
                (*current).last_failure_transport,
                (*current).last_recovery_ok,
                0,
            ) {
                (*current).write_transport_retries += 1;
                msc_scsi::retry_delay(
                    current,
                    b"usbmsc-write-retry",
                    if msc_scsi::uses_read_write16(lba, sectors) {
                        0x8a
                    } else {
                        0x2a
                    },
                    1,
                );
                if ensure_selected(current) && msc_scsi::write(current, lba, sectors, bytes, count)
                {
                    (*current).write_transport_retry_successes += 1;
                    (*current).writes += 1;
                    puts(b"[USBMSC] WRITE transport retry recovered\n");
                    return true;
                }
            }
            if !(*current).last_failure_transport {
                msc_scsi::request_sense(current);
            }
            (*current).write_failures += 1;
            return false;
        }
        (*current).writes += 1;
        true
    }
}
pub(super) unsafe fn flush_block(current: *mut Status) -> bool {
    unsafe {
        let Some(_operation) = begin_operation(current) else {
            return false;
        };
        if !ensure_selected(current) {
            (*current).flush_failures += 1;
            return false;
        }
        if !msc_scsi::synchronize_cache10(current) {
            if msc_retry::retry_transport(
                (*current).last_failure_transport,
                (*current).last_recovery_ok,
                0,
            ) {
                (*current).flush_transport_retries += 1;
                msc_scsi::retry_delay(current, b"usbmsc-flush-retry", 0x35, 1);
                if ensure_selected(current) && msc_scsi::synchronize_cache10(current) {
                    (*current).flush_transport_retry_successes += 1;
                    (*current).flushes += 1;
                    puts(b"[USBMSC] SYNC-CACHE transport retry recovered\n");
                    return true;
                }
            }
            if !(*current).last_failure_transport
                && msc_scsi::request_sense(current)
                && (*current).sense_valid
                && (*current).sense_for_opcode == 0x35
                && (*current).sense_key == 5
            {
                (*current).flush_unsupported += 1;
                (*current).flushes += 1;
                return true;
            }
            (*current).flush_failures += 1;
            return false;
        }
        (*current).flushes += 1;
        true
    }
}
/// Diagnostic value owns every byte of the reason, even after being moved.
/// Public Text fields describe only immutable module-source labels.
pub struct Snapshot {
    pub status: Status,
    pub reason: [u8; 128],
    pub reason_len: usize,
}
pub fn snapshot() -> Snapshot {
    let mut result = Snapshot {
        status: Status::EMPTY,
        reason: [0; 128],
        reason_len: 0,
    };
    let current = unsafe { ptr::addr_of_mut!((*item(LAST_MEDIUM.load(Ordering::Acquire))).state) };
    let Some(_g) = Admission::enter(unsafe { ptr::addr_of_mut!((*medium(current)).ownership) })
    else {
        let reason = b"USBMSC device transaction busy";
        result.reason[..reason.len()].copy_from_slice(reason);
        result.reason_len = reason.len();
        return result;
    };
    unsafe {
        (*current).bot_source = source_name(b"usb.msc_bot");
        (*current).bot_r4p_build = msc_bot::BUILDS.load(Ordering::Relaxed);
        (*current).bot_r4p_csw = msc_bot::CSWS.load(Ordering::Relaxed);
        (*current).bot_dispatch_failures = msc_bot::DISPATCH_FAILURES.load(Ordering::Relaxed);
        (*current).bot_last_result = msc_bot::LAST_RESULT.load(Ordering::Relaxed);
        (*current).scsi_source = source_name(b"usb.scsi_block");
        (*current).scsi_r4p_build = msc_scsi::BUILDS.load(Ordering::Relaxed);
        (*current).scsi_r4p_parse = msc_scsi::PARSES.load(Ordering::Relaxed);
        (*current).scsi_dispatch_failures = msc_scsi::DISPATCH_FAILURES.load(Ordering::Relaxed);
        (*current).scsi_last_result = msc_scsi::LAST_RESULT.load(Ordering::Relaxed);
        result.status = current.read();
        let bytes = msc_bot::text((*current).reason);
        result.reason_len = bytes.len().min(128);
        result.reason[..result.reason_len].copy_from_slice(&bytes[..result.reason_len]);
        result.status.reason = Text::literal(b"");
        result.status.reason_storage = [0; 128];
    }
    result
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_init() -> bool {
    init()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_owner(source: u8) {
    set_owner(source);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_external_owner() -> bool {
    has_external_owner()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_block_device_count() -> usize {
    block_device_count()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_device_index() -> usize {
    device_index().unwrap_or(usize::MAX)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_msc_reselect() -> bool {
    reselect_active_device()
}
/// # Safety
/// Exclusive paired output, 128-byte reason buffer and length remain resident.
/// The output owns its reason bytes and contains no mutable-medium pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_msc_status(
    out: *mut Status,
    reason: *mut u8,
    len: *mut usize,
) {
    let value = snapshot();
    unsafe {
        out.write(value.status);
        ptr::copy_nonoverlapping(value.reason.as_ptr(), reason, 128);
        len.write(value.reason_len);
    }
}
/// # Safety
/// Context is the exact registered resident medium; output retains its caller's
/// lifetime until the synchronous block callback returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_msc_read(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    if current.is_null() {
        return false;
    }
    unsafe { read_block(current, lba, sectors, out, len) }
}
/// # Safety
/// Exact registered medium and caller's readable buffer stay pinned until the
/// synchronous callback/recovery/retry sequence has completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_msc_write(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    bytes: *const u8,
    len: usize,
) -> bool {
    if current.is_null() {
        return false;
    }
    unsafe { write_block(current, lba, sectors, bytes, len) }
}
/// # Safety
/// Context is the exact registered resident medium throughout the flush.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_msc_flush(current: *mut Status) -> bool {
    if current.is_null() {
        return false;
    }
    unsafe { flush_block(current) }
}
