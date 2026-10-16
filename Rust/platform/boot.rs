//! Existing platform boot order and its immutable cached ACPI/PCI views.
//! The BSP publishes completion only after final mappings and enumeration;
//! no orchestration-state borrow spans a memory/diagnostic callback.
use super::{acpi, pci_inventory, pcie};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static LEGACY: AtomicBool = AtomicBool::new(false);
struct Storage(UnsafeCell<(acpi::Info, pci_inventory::Status)>);
// Written once in singleton BSP startup, before the completion publication.
unsafe impl Sync for Storage {}
static CACHED: Storage = Storage(UnsafeCell::new((
    acpi::Info::EMPTY,
    pci_inventory::Status::EMPTY,
)));

fn step(operation: u8, info: *const acpi::Info) -> bool {
    unsafe { platform_boot_step(operation, info) }
}
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
pub fn init() -> bool {
    if initialized() {
        return true;
    }
    if !step(0, core::ptr::null()) {
        return step(3, core::ptr::null());
    }
    let info = acpi::inspect();
    pcie::configure(
        info.mcfg_base,
        info.mcfg_segment,
        info.mcfg_start_bus,
        info.mcfg_end_bus,
    );
    unsafe { core::ptr::addr_of_mut!((*CACHED.0.get()).0).write(info) };
    if !step(1, &info) {
        return false;
    }
    pcie::activate_mapped_aperture();
    let pci = pci_inventory::enumerate();
    unsafe { core::ptr::addr_of_mut!((*CACHED.0.get()).1).write(pci) };
    LEGACY.store(pci.legacy_used, Ordering::Relaxed);
    INITIALIZED.store(true, Ordering::Release);
    step(2, core::ptr::null());
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_platform_boot_control(operation: u8) -> bool {
    match operation {
        0 => init(),
        1 => initialized(),
        2 => LEGACY.load(Ordering::Acquire),
        _ => false,
    }
}
/// # Safety
/// Output is exclusive aligned C storage. The cached view is immutable after
/// the acquire observation of successful BSP completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_platform_boot_acpi(output: *mut acpi::Info) -> bool {
    if !initialized() {
        return false;
    }
    unsafe { output.write(core::ptr::addr_of!((*CACHED.0.get()).0).read()) };
    true
}
/// # Safety
/// Output is exclusive aligned C storage after successful BSP publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_platform_boot_pci(output: *mut pci_inventory::Status) -> bool {
    if !initialized() {
        return false;
    }
    unsafe { output.write(core::ptr::addr_of!((*CACHED.0.get()).1).read()) };
    true
}

unsafe fn platform_boot_step(operation: u8, info: *const acpi::Info) -> bool {
    match operation {
        0 => crate::memory::boot::core_ready(),
        1 => {
            let i = unsafe { info.read() };
            let raw = crate::memory::root::AcpiInput {
                rsdp: i.rsdp_phys,
                xsdt: i.xsdt_phys,
                rsdt: u64::from(i.rsdt_phys),
                mcfg: i.mcfg_phys,
                madt: i.madt_phys,
                fadt: i.fadt_phys,
                hpet: i.hpet_phys,
                dsdt: i.fadt_dsdt_phys,
                x_dsdt: i.fadt_x_dsdt_phys,
                lapic: i.madt_lapic_address,
                first_ioapic: u64::from(i.madt_first_ioapic_address),
                hpet_base: i.hpet_base,
                mcfg_base: i.mcfg_base,
                reset_address: i.fadt_reset_address,
                ioapics: i.madt_ioapics.map(|a| u64::from(a.address)),
                ioapic_count: i.madt_ioapic_count,
                start_bus: i.mcfg_start_bus,
                end_bus: i.mcfg_end_bus,
                reset_address_space: i.fadt_reset_address_space,
                found: i.found,
                reset_supported: i.fadt_reset_supported,
                reset_valid: i.fadt_reset_gas_valid,
            };
            unsafe { crate::memory::boot::r4os_rust_memory_finalize(&raw) }
        }
        2 => {
            crate::boot::log_route::puts(b"  Platform mappings [OK]\r\n");
            true
        }
        3 => {
            let message = b"Platform boot before memory core";
            unsafe {
                crate::boot::fatal::r4os_rust_fatal_record(9, message.as_ptr(), message.len())
            };
            false
        }
        _ => false,
    }
}
