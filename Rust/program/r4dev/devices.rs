//! Passive cached device/hardware inventory. No new scans, logging or I/O.
use crate::{
    contract as c,
    platform::{acpi, cpu, device_inventory as inventory, irq, pci_inventory},
    storage::{block_diagnostics, block_registry},
    usb::{core as usb, host_controller as host},
};
/// # Safety
/// Exclusive canonical output; actual inventory owner retains its header and
/// availability checks and bounded snapshot admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_device_inventory_summary(
    out: *mut c::DeviceInventorySummary,
) -> i32 {
    unsafe { inventory::r4os_rust_device_inventory_summary(out) }
}
/// # Safety
/// Exclusive canonical output and original ordinal request for this snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_device_inventory_record(
    index: u32,
    out: *mut c::DeviceInventoryRecord,
) -> i32 {
    unsafe { inventory::r4os_rust_device_inventory_record(index, out) }
}
fn has_bus(bus: u8) -> bool {
    (0..block_registry::slot_count())
        .any(|i| block_diagnostics::device_snapshot(i).is_some_and(|d| d.bus == bus))
}
/// # Safety
/// Exclusive aligned canonical output for cached read-only hardware facts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_hardware_summary(out: *mut c::HardwareSummary) -> i32 {
    unsafe {
        let ai = super::snapshot(acpi::r4os_rust_acpi_info);
        let ps = pci_inventory::snapshot();
        let irq = irq::snapshot();
        let usb = super::snapshot(usb::r4os_rust_usb_core_summary);
        let inventory_counts = super::snapshot(inventory::r4os_rust_device_inventory_counts);
        let cs = super::snapshot(cpu::r4os_rust_cpu_status);
        let legacy_pci_devices = ps.legacy_stored_count;
        let block_devices = block_registry::count().min(u32::MAX as usize) as u32;
        let mut flags = 0;
        if ai.rsdp_phys != 0 {
            flags |= c::hardware_summary_flag_acpi
        }
        if ps.ecam_used {
            flags |= c::hardware_summary_flag_pcie
        }
        if ps.legacy_used && legacy_pci_devices != 0 {
            flags |= c::hardware_summary_flag_legacy_pci
        }
        if irq.ioapic_runtime_mapped || irq.ioapic_routing_active {
            flags |= c::hardware_summary_flag_ioapic
        }
        if irq.hpet_available {
            flags |= c::hardware_summary_flag_hpet
        }
        if ps.xhci_count != 0 || host::r4os_rust_usb_host_find(b"XHCI".as_ptr(), 4) != usize::MAX {
            flags |= c::hardware_summary_flag_xhci
        }
        if ps.ahci_count != 0 || has_bus(2) {
            flags |= c::hardware_summary_flag_ahci
        }
        if ps.nvme_count != 0 || has_bus(3) {
            flags |= c::hardware_summary_flag_nvme
        }
        if usb.configured != 0 {
            flags |= c::hardware_summary_flag_usb_configured
        }
        out.write(c::HardwareSummary {
            flags,
            acpi_tables: ai.table_count,
            acpi_invalid_tables: ai.invalid_table_count,
            pcie_devices: ps.ecam_stored_count,
            legacy_pci_devices,
            storage_controllers: ps.ahci_count + ps.nvme_count,
            network_controllers: ps.network_count,
            display_controllers: inventory_counts.displays,
            usb_controllers: ps.xhci_count,
            hda_controllers: ps.hda_count,
            block_devices,
            usb_devices: usb.count,
            usb_configured: usb.configured,
            driver_records: inventory_counts.drivers,
            protocol_records: inventory_counts.protocols,
            irq_controller: irq.controller,
            timer_backend: irq.timer,
            lapic_count: irq.lapic_count,
            ioapic_count: irq.ioapic_count,
            iso_count: irq.iso_count,
            hpet_frequency_hz: irq.hpet_frequency_hz,
            cpu_logical_processors: u32::from(cs.logical_processors.max(1)),
            cpu_physical_address_bits: cs.physical_address_bits,
            cpu_virtual_address_bits: cs.virtual_address_bits,
            ..Default::default()
        });
        1
    }
}
