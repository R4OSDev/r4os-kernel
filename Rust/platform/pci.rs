//! Legacy CF8/CFC cycles retain their boot/driver-init caller admission.
//! Device discovery publishes its bounded cache before concurrent observers;
//! live access counters are atomic and no state borrow crosses port/log calls.
use super::pci_scan::{Device, MAX_DEVICES};
use crate::arch::io;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AccessMetrics {
    pub config_reads: u64,
    pub config_writes: u64,
}
struct State {
    devices: [Device; MAX_DEVICES],
    count: usize,
}
struct Storage(UnsafeCell<State>);
// The existing serialized boot/driver-init context owns cache construction.
// It completes before consumers; later live metrics use independent atomics.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    devices: [Device::EMPTY; MAX_DEVICES],
    count: 0,
}));
static READS: AtomicU64 = AtomicU64::new(0);
static WRITES: AtomicU64 = AtomicU64::new(0);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    unsafe { f(&mut *STATE.0.get()) }
}
pub fn reset_access_metrics() {
    READS.store(0, Ordering::Relaxed);
    WRITES.store(0, Ordering::Relaxed);
}
pub fn access_metrics() -> AccessMetrics {
    AccessMetrics {
        config_reads: READS.load(Ordering::Relaxed),
        config_writes: WRITES.load(Ordering::Relaxed),
    }
}
fn address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc)
}
pub fn read_config32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    READS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        io::outl(0xcf8, address(bus, device, function, offset));
        io::inl(0xcfc)
    }
}
pub fn write_config32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    WRITES.fetch_add(1, Ordering::Relaxed);
    unsafe {
        io::outl(0xcf8, address(bus, device, function, offset));
        io::outl(0xcfc, value)
    };
}
pub fn write_config8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    WRITES.fetch_add(1, Ordering::Relaxed);
    unsafe {
        io::outl(0xcf8, address(bus, device, function, offset));
        io::outb(0xcfc + u16::from(offset & 3), value)
    };
}
fn read_config16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    (read_config32(bus, device, function, offset & 0xfc) >> ((offset & 2) * 8)) as u16
}
fn write_config16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let aligned = offset & 0xfc;
    let shift = (offset & 2) * 8;
    let mut current = read_config32(bus, device, function, aligned);
    current &= !(0xffffu32 << shift);
    current |= u32::from(value) << shift;
    write_config32(bus, device, function, aligned, current);
}
pub fn count() -> usize {
    unsafe { (*STATE.0.get()).count }
}
pub fn device_at(index: usize) -> Option<Device> {
    if index >= count() {
        return None;
    }
    Some(unsafe {
        core::ptr::addr_of!((*STATE.0.get()).devices)
            .cast::<Device>()
            .add(index)
            .read()
    })
}
pub fn find_by_class(class: u8, subclass: u8) -> Option<Device> {
    for index in 0..count() {
        let device = device_at(index).unwrap();
        if device.class_code == class && device.subclass == subclass {
            return Some(device);
        }
    }
    None
}
fn store(device: Device) {
    state(|state| {
        if state.count < MAX_DEVICES {
            state.devices[state.count] = device;
            state.count += 1;
        }
    });
}
pub fn enumerate_legacy() {
    state(|state| state.count = 0);
    puts(b"[PCI] legacy CF8/CFC enumeration\r\n");
    let mut logged = 0;
    for bus in 0..256u16 {
        for device in 0..32 {
            for function in 0..8 {
                let raw = read_config32(bus as u8, device, function, 0);
                let vendor = raw as u16;
                if vendor == u16::MAX {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let class = read_config32(bus as u8, device, function, 8);
                let found = Device {
                    bus_kind: super::pci_scan::LEGACY,
                    bus: bus as u8,
                    device,
                    function,
                    vendor_id: vendor,
                    device_id: (raw >> 16) as u16,
                    class_code: (class >> 24) as u8,
                    subclass: (class >> 16) as u8,
                    prog_if: (class >> 8) as u8,
                };
                store(found);
                if logged < 24 {
                    log_device(b"[PCI] ", found);
                    logged += 1;
                }
                // Keep the original legacy helper's header read per function.
                let header = (read_config32(bus as u8, device, function, 12) >> 16) as u8;
                if function == 0 && header & 0x80 == 0 {
                    break;
                }
            }
        }
    }
    puts(b"[PCI] devices found=");
    dec(count() as u64);
    if count() > logged {
        puts(b" logged=");
        dec(logged as u64);
    }
    puts(b"\r\n");
}
pub fn read_bar(device: Device, index: u8) -> u32 {
    if index >= 6 {
        return 0;
    }
    read_config32(device.bus, device.device, device.function, 0x10 + index * 4)
}
pub fn read_bar64(device: Device, index: u8) -> u64 {
    let low = read_bar(device, index);
    if low & 1 != 0 || (low >> 1) & 3 != 2 || index >= 5 {
        return u64::from(low);
    }
    (u64::from(read_bar(device, index + 1)) << 32) | u64::from(low)
}
pub fn read_command(device: Device) -> u16 {
    read_config16(device.bus, device.device, device.function, 4)
}
pub fn read_interrupt_line(device: Device) -> u8 {
    read_config32(device.bus, device.device, device.function, 0x3c) as u8
}
pub fn read_interrupt_pin(device: Device) -> u8 {
    (read_config32(device.bus, device.device, device.function, 0x3c) >> 8) as u8
}
pub fn enable_bus_master(device: Device, flags: u16) {
    let command = (read_config16(device.bus, device.device, device.function, 4) | flags) & !0x0400;
    write_config16(device.bus, device.device, device.function, 4, command);
}
pub(super) fn puts(text: &[u8]) {
    unsafe { crate::boot::log::r4os_rust_bootlog_write(text.as_ptr(), text.len()) };
}
pub(super) fn dec(value: u64) {
    crate::boot::log::r4os_rust_bootlog_dec(value);
}
pub(super) fn hex(value: u64, width: usize) {
    crate::boot::log::r4os_rust_bootlog_hex(value, width);
}
pub(super) fn log_device(prefix: &[u8], device: Device) {
    puts(prefix);
    dec(u64::from(device.bus));
    puts(b":");
    dec(u64::from(device.device));
    puts(b".");
    dec(u64::from(device.function));
    puts(b" vid=0x");
    hex(u64::from(device.vendor_id), 4);
    puts(b" did=0x");
    hex(u64::from(device.device_id), 4);
    puts(b" class=0x");
    hex(u64::from(device.class_code), 2);
    puts(b" subclass=0x");
    hex(u64::from(device.subclass), 2);
    puts(b" if=0x");
    hex(u64::from(device.prog_if), 2);
    puts(b"\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pci_read(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    read_config32(bus, device, function, offset)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pci_write(
    bus: u8,
    device: u8,
    function: u8,
    offset: u8,
    value: u32,
    width: u8,
) {
    if width == 8 {
        write_config8(bus, device, function, offset, value as u8);
    } else {
        write_config32(bus, device, function, offset, value);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pci_control(operation: u8, device: Device, argument: u8) -> u64 {
    match operation {
        0 => {
            reset_access_metrics();
            0
        }
        1 => {
            enumerate_legacy();
            0
        }
        2 => count() as u64,
        3 => u64::from(read_bar(device, argument)),
        4 => read_bar64(device, argument),
        5 => u64::from(read_command(device)),
        6 => u64::from(read_interrupt_line(device)),
        7 => u64::from(read_interrupt_pin(device)),
        8 => {
            enable_bus_master(device, 5);
            0
        }
        9 => {
            enable_bus_master(device, 6);
            0
        }
        _ => 0,
    }
}
/// # Safety
/// Output is aligned exclusive C storage for one Device. Cache construction
/// is complete before observers, under the existing boot/driver-init caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_device(index: u64, output: *mut Device) -> bool {
    let Some(device) = device_at(index as usize) else {
        return false;
    };
    unsafe { output.write(device) };
    true
}
/// # Safety
/// Output is aligned exclusive C storage for one Device after cache publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_find(class: u8, subclass: u8, output: *mut Device) -> bool {
    let Some(device) = find_by_class(class, subclass) else {
        return false;
    };
    unsafe { output.write(device) };
    true
}
/// # Safety
/// Output is aligned exclusive C storage for one AccessMetrics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_metrics(output: *mut AccessMetrics) {
    unsafe { output.write(access_metrics()) };
}
