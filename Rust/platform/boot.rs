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
unsafe extern "C" {
    fn r4os_zig_platform_boot_step(operation: u8, info: *const acpi::Info) -> bool;
}
fn step(operation: u8, info: *const acpi::Info) -> bool {
    unsafe { r4os_zig_platform_boot_step(operation, info) }
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
