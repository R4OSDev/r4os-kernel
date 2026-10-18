//! Single ECAM window/controller. Boot owns configuration/cache publication;
//! driver-init callers own device access. Atomic counters permit passive reads.
use super::{
    acpi, pci,
    pci_scan::{self, Device, MAX_DEVICES},
};
use crate::memory::{paging, phys};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub available: bool,
    pub enumerated: bool,
    pub mcfg_base: u64,
    pub segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
    pub aperture_ready: bool,
    pub aperture_bytes: u64,
    pub mapping_checks: u64,
    pub mapping_hits: u64,
    pub mapping_misses: u64,
    pub fast_accesses: u64,
    pub config_reads: u64,
    pub config_writes: u64,
    pub invalid_accesses: u64,
    pub device_count: u32,
    pub stored_count: u32,
    pub ahci_count: u32,
    pub nvme_count: u32,
    pub network_count: u32,
    pub hda_count: u32,
    pub xhci_count: u32,
    pub malformed_reads: u32,
    pub first_ahci: Device,
    pub first_nvme: Device,
    pub first_network: Device,
    pub first_hda: Device,
    pub first_xhci: Device,
    pub reason: u8,
}
impl Status {
    const EMPTY: Self = Self {
        available: false,
        enumerated: false,
        mcfg_base: 0,
        segment: 0,
        start_bus: 0,
        end_bus: 0,
        aperture_ready: false,
        aperture_bytes: 0,
        mapping_checks: 0,
        mapping_hits: 0,
        mapping_misses: 0,
        fast_accesses: 0,
        config_reads: 0,
        config_writes: 0,
        invalid_accesses: 0,
        device_count: 0,
        stored_count: 0,
        ahci_count: 0,
        nvme_count: 0,
        network_count: 0,
        hda_count: 0,
        xhci_count: 0,
        malformed_reads: 0,
        first_ahci: Device::EMPTY,
        first_nvme: Device::EMPTY,
        first_network: Device::EMPTY,
        first_hda: Device::EMPTY,
        first_xhci: Device::EMPTY,
        reason: 0,
    };
}

struct State {
    status: Status,
    devices: [Device; MAX_DEVICES],
}
struct Storage(UnsafeCell<State>);
// Metadata/cache mutation belongs to the existing serialized BSP context,
// before concurrent observers; live reads/writes below only mutate atomics.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    status: Status::EMPTY,
    devices: [Device::EMPTY; MAX_DEVICES],
}));
static CHECKS: AtomicU64 = AtomicU64::new(0);
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static FAST: AtomicU64 = AtomicU64::new(0);
static READS: AtomicU64 = AtomicU64::new(0);
static WRITES: AtomicU64 = AtomicU64::new(0);
static INVALID: AtomicU64 = AtomicU64::new(0);
static MALFORMED: AtomicU32 = AtomicU32::new(0);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    unsafe { f(&mut *STATE.0.get()) }
}
fn metadata() -> Status {
    unsafe { core::ptr::addr_of!((*STATE.0.get()).status).read() }
}
pub fn snapshot() -> Status {
    let mut status = metadata();
    status.mapping_checks = CHECKS.load(Ordering::Relaxed);
    status.mapping_hits = HITS.load(Ordering::Relaxed);
    status.mapping_misses = MISSES.load(Ordering::Relaxed);
    status.fast_accesses = FAST.load(Ordering::Relaxed);
    status.config_reads = READS.load(Ordering::Relaxed);
    status.config_writes = WRITES.load(Ordering::Relaxed);
    status.invalid_accesses = INVALID.load(Ordering::Relaxed);
    status.malformed_reads = MALFORMED.load(Ordering::Relaxed);
    status
}
pub fn configure(base: u64, segment: u16, start: u8, end: u8) {
    let window = pci_scan::ecam_window(base, start, end);
    let status = Status {
        available: window.is_some(),
        mcfg_base: base,
        segment,
        start_bus: start,
        end_bus: end,
        aperture_bytes: window.map_or(0, |w| w.bytes),
        reason: if window.is_some() { 1 } else { 2 },
        ..Status::EMPTY
    };
    state(|state| {
        state.status = status;
        state.devices.fill(Device::EMPTY);
    });
    for counter in [&CHECKS, &HITS, &MISSES, &FAST, &READS, &WRITES, &INVALID] {
        counter.store(0, Ordering::Relaxed);
    }
    MALFORMED.store(0, Ordering::Relaxed);
}
pub fn activate_mapped_aperture() -> bool {
    state(|state| state.status.aperture_ready = false);
    let value = metadata();
    if !value.available || value.aperture_bytes == 0 {
        return false;
    }
    let Some(window) = pci_scan::ecam_window(value.mcfg_base, value.start_bus, value.end_bus)
    else {
        INVALID.fetch_add(1, Ordering::Relaxed);
        state(|state| state.status.reason = 3);
        return false;
    };
    let last = window.base + window.bytes - 1;
    CHECKS.fetch_add(2, Ordering::Relaxed);
    let first_mapped = paging::ensure_mmio(phys::r4os_rust_phys_to_virt(window.base), window.base);
    let last_mapped = paging::ensure_mmio(phys::r4os_rust_phys_to_virt(last & !4095), last & !4095);
    if first_mapped {
        HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        MISSES.fetch_add(1, Ordering::Relaxed);
    }
    if last_mapped {
        HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        MISSES.fetch_add(1, Ordering::Relaxed);
    }
    let mut ready = first_mapped && last_mapped;
    if ready {
        for page in (window.base..window.base + window.bytes).step_by(4096) {
            if !paging::ensure_mmio(phys::r4os_rust_phys_to_virt(page), page) {
                ready = false;
                break;
            }
        }
    }
    state(|state| {
        state.status.aperture_ready = ready;
        state.status.reason = if ready { 4 } else { 5 };
    });
    ready
}
fn mapped(address: u64) -> bool {
    let mut value = 0;
    unsafe { paging::r4os_rust_paging_read(address, 3, false, &mut value) }
}
fn valid_access(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> bool {
    let current = metadata();
    current.available
        && base == current.mcfg_base
        && bus >= current.start_bus
        && bus <= current.end_bus
        && device < 32
        && function < 8
        && offset <= 0x0fff
}
fn config_address(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> Option<u64> {
    if !valid_access(base, bus, device, function, offset) {
        return None;
    }
    base.checked_add(pci_scan::ecam_offset(
        metadata().start_bus,
        bus,
        device,
        function,
        offset,
    )?)
}
fn ensure_mapped(address: u64) -> bool {
    if metadata().aperture_ready {
        FAST.fetch_add(1, Ordering::Relaxed);
        return true;
    }
    let page = address & !4095;
    let virtual_address = phys::r4os_rust_phys_to_virt(page);
    CHECKS.fetch_add(1, Ordering::Relaxed);
    if !mapped(virtual_address) {
        MISSES.fetch_add(1, Ordering::Relaxed);
    } else {
        HITS.fetch_add(1, Ordering::Relaxed);
    }
    if !paging::ensure_mmio(virtual_address, page) {
        MALFORMED
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("PCIe malformed-read counter overflow");
        return false;
    }
    true
}
pub fn read_config32(base: u64, bus: u8, device: u8, function: u8, offset: u16) -> u32 {
    READS.fetch_add(1, Ordering::Relaxed);
    let Some(address) = config_address(base, bus, device, function, offset) else {
        INVALID.fetch_add(1, Ordering::Relaxed);
        return u32::MAX;
    };
    if !ensure_mapped(address) {
        return u32::MAX;
    }
    let virtual_address = phys::r4os_rust_phys_to_virt(address);
    assert_eq!(
        virtual_address & 3,
        0,
        "unaligned PCIe configuration register"
    );
    unsafe { core::ptr::read_volatile(virtual_address as *const u32) }
}
pub fn write_config32(base: u64, bus: u8, device: u8, function: u8, offset: u16, value: u32) {
    WRITES.fetch_add(1, Ordering::Relaxed);
    let Some(address) = config_address(base, bus, device, function, offset) else {
        INVALID.fetch_add(1, Ordering::Relaxed);
        return;
    };
    if !ensure_mapped(address) {
        return;
    }
    let virtual_address = phys::r4os_rust_phys_to_virt(address);
    assert_eq!(
        virtual_address & 3,
        0,
        "unaligned PCIe configuration register"
    );
    unsafe { core::ptr::write_volatile(virtual_address as *mut u32, value) };
}
pub fn read_current_config32(bus: u8, device: u8, function: u8, offset: u16) -> u32 {
    read_config32(metadata().mcfg_base, bus, device, function, offset)
}
pub fn write_current_config32(bus: u8, device: u8, function: u8, offset: u16, value: u32) -> bool {
    let base = metadata().mcfg_base;
    if !valid_access(base, bus, device, function, offset) {
        INVALID.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    write_config32(base, bus, device, function, offset, value);
    true
}
pub fn device_at(index: usize) -> Option<Device> {
    if index >= metadata().stored_count as usize || index >= MAX_DEVICES {
        return None;
    }
    Some(unsafe {
        core::ptr::addr_of!((*STATE.0.get()).devices)
            .cast::<Device>()
            .add(index)
            .read()
    })
}
fn store(device: Device) {
    state(|state| {
        let current = &mut state.status;
        if current.device_count == 0 {
            current.reason = 6;
        }
        current.device_count += 1;
        if (current.stored_count as usize) < MAX_DEVICES {
            state.devices[current.stored_count as usize] = device;
            current.stored_count += 1;
        }
        if device.class_code == 1 && device.subclass == 6 {
            if current.ahci_count == 0 {
                current.first_ahci = device;
            }
            current.ahci_count += 1;
        } else if device.class_code == 1 && device.subclass == 8 {
            if current.nvme_count == 0 {
                current.first_nvme = device;
            }
            current.nvme_count += 1;
        } else if device.class_code == 2 && device.subclass == 0 {
            if current.network_count == 0 {
                current.first_network = device;
            }
            current.network_count += 1;
        } else if device.class_code == 4 && device.subclass == 3 {
            if current.hda_count == 0 {
                current.first_hda = device;
            }
            current.hda_count += 1;
        } else if device.class_code == 12 && device.subclass == 3 && device.prog_if == 0x30 {
            if current.xhci_count == 0 {
                current.first_xhci = device;
            }
            current.xhci_count += 1;
        }
    });
}
pub fn enumerate() -> Status {
    let info = acpi::snapshot();
    configure(
        info.mcfg_base,
        info.mcfg_segment,
        info.mcfg_start_bus,
        info.mcfg_end_bus,
    );
    state(|state| state.devices.fill(Device::EMPTY));
    pci::puts(b"[PCIE] enumeration\r\n");
    if info.mcfg_base == 0 {
        pci::puts(b"[PCIE][WARN] no MCFG ECAM base, skipping\r\n");
        return snapshot();
    }
    let mut logged = 0;
    for bus in u16::from(info.mcfg_start_bus)..=u16::from(info.mcfg_end_bus) {
        for device in 0..32 {
            for function in 0..8 {
                let raw = read_config32(info.mcfg_base, bus as u8, device, function, 0);
                let vendor = raw as u16;
                if vendor == u16::MAX {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let class = read_config32(info.mcfg_base, bus as u8, device, function, 8);
                // This historical helper leaves bus_kind at its zero default.
                let found = Device {
                    bus: bus as u8,
                    device,
                    function,
                    vendor_id: vendor,
                    device_id: (raw >> 16) as u16,
                    class_code: (class >> 24) as u8,
                    subclass: (class >> 16) as u8,
                    prog_if: (class >> 8) as u8,
                    ..Device::EMPTY
                };
                store(found);
                if logged < 24 {
                    pci::log_device(b"[PCIE] ", found);
                    logged += 1;
                }
                let header =
                    (read_config32(info.mcfg_base, bus as u8, device, function, 12) >> 16) as u8;
                if function == 0 && header & 0x80 == 0 {
                    break;
                }
            }
        }
    }
    let value = metadata();
    pci::puts(b"[PCIE] devices found=");
    pci::dec(u64::from(value.device_count));
    pci::puts(b" ahci=");
    pci::dec(u64::from(value.ahci_count));
    pci::puts(b" nvme=");
    pci::dec(u64::from(value.nvme_count));
    pci::puts(b" net=");
    pci::dec(u64::from(value.network_count));
    pci::puts(b" hda=");
    pci::dec(u64::from(value.hda_count));
    pci::puts(b" xhci=");
    pci::dec(u64::from(value.xhci_count));
    if value.device_count > logged {
        pci::puts(b" logged=");
        pci::dec(u64::from(logged));
    }
    pci::puts(b"\r\n");
    state(|state| {
        state.status.enumerated = true;
        state.status.reason = 7;
    });
    snapshot()
}
pub fn read_bar(device: Device, index: u8) -> u32 {
    let base = metadata().mcfg_base;
    if index >= 6 || base == 0 {
        return 0;
    }
    read_config32(
        base,
        device.bus,
        device.device,
        device.function,
        0x10 + u16::from(index) * 4,
    )
}
pub fn read_bar64(device: Device, index: u8) -> u64 {
    let low = read_bar(device, index);
    if low & 1 != 0 || (low >> 1) & 3 != 2 || index >= 5 {
        return u64::from(low);
    }
    (u64::from(read_bar(device, index + 1)) << 32) | u64::from(low)
}
pub fn read_command(device: Device) -> u16 {
    let base = metadata().mcfg_base;
    if base == 0 {
        return 0;
    }
    read_config32(base, device.bus, device.device, device.function, 4) as u16
}
pub fn write_command(device: Device, command: u16) {
    let base = metadata().mcfg_base;
    if base == 0 {
        return;
    }
    let raw = read_config32(base, device.bus, device.device, device.function, 4);
    write_config32(
        base,
        device.bus,
        device.device,
        device.function,
        4,
        (raw & 0xffff_0000) | u32::from(command),
    );
}
pub fn read_interrupt_line(device: Device) -> u8 {
    let base = metadata().mcfg_base;
    if base == 0 {
        return 0xff;
    }
    read_config32(base, device.bus, device.device, device.function, 0x3c) as u8
}
pub fn read_interrupt_pin(device: Device) -> u8 {
    let base = metadata().mcfg_base;
    if base == 0 {
        return 0;
    }
    (read_config32(base, device.bus, device.device, device.function, 0x3c) >> 8) as u8
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pcie_configure(base: u64, segment: u16, start: u8, end: u8) {
    configure(base, segment, start, end);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pcie_control(operation: u8, device: Device, argument: u16) -> u64 {
    match operation {
        0 => {
            enumerate();
            0
        }
        1 => u64::from(activate_mapped_aperture()),
        2 => u64::from(read_bar(device, argument as u8)),
        3 => read_bar64(device, argument as u8),
        4 => u64::from(read_command(device)),
        5 => u64::from(read_interrupt_line(device)),
        6 => u64::from(read_interrupt_pin(device)),
        7 => {
            write_command(device, argument);
            0
        }
        _ => 0,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pcie_config(
    operation: u8,
    base: u64,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u32,
) -> u64 {
    match operation {
        0 => u64::from(read_config32(base, bus, device, function, offset)),
        1 => {
            write_config32(base, bus, device, function, offset, value);
            0
        }
        2 => u64::from(read_current_config32(bus, device, function, offset)),
        3 => u64::from(write_current_config32(bus, device, function, offset, value)),
        _ => 0,
    }
}
/// # Safety
/// Output is aligned exclusive C storage; cache/configuration publication has
/// completed before consumers. Live diagnostics use independent atomic counts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pcie_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
/// # Safety
/// Output is aligned exclusive C storage and the boot-owned cache is published.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pcie_device(index: u64, output: *mut Device) -> bool {
    let Some(device) = device_at(index as usize) else {
        return false;
    };
    unsafe { output.write(device) };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 184);
    assert!(core::mem::offset_of!(Status, first_ahci) == 120);
    assert!(core::mem::offset_of!(Status, reason) == 180);
};
