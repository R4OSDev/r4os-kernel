// Platform and device mapping bridge for kernel startup.
//
// This layer creates the ACPI and canonical PCI state once and finalizes the
// late device mappings. Consumers read the inventory, never the raw buses.

const acpi = @import("../platform/acpi.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const memory_boot = @import("memory_boot.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const pcie = @import("../platform/pcie.zig");

var initialized = false;
var legacy_pci_enumerated = false;
var cached_acpi_info: acpi.Info = .{};
var cached_pci_status: pci_inventory.Status = .{};

pub fn initDeviceMappings() bool {
    if (initialized) return true;

    if (!memory_boot.isCoreInitialized()) {
        return fail("Platform boot before memory core");
    }

    cached_acpi_info = acpi.inspect();
    pcie.configure(cached_acpi_info);

    if (!memory_boot.finalizeDeviceMappings(cached_acpi_info)) {
        return false;
    }

    _ = pcie.activateMappedAperture();
    cached_pci_status = pci_inventory.enumerate();
    legacy_pci_enumerated = cached_pci_status.legacy_used;

    initialized = true;
    boot_status.statusLine("  Platform mappings [OK]\r\n");
    return true;
}

pub fn isInitialized() bool {
    return initialized;
}

pub fn isLegacyPciEnumerated() bool {
    return legacy_pci_enumerated;
}

pub fn acpiInfo() ?acpi.Info {
    if (!initialized) return null;
    return cached_acpi_info;
}

pub fn pcieStatus() ?pci_inventory.Status {
    if (!initialized) return null;
    return cached_pci_status;
}

pub fn pciInventoryStatus() ?pci_inventory.Status {
    if (!initialized) return null;
    return cached_pci_status;
}

fn fail(message: []const u8) bool {
    return fatal.fail(.platform, message);
}
