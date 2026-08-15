// Platform and device mapping bridge for kernel startup.
//
// This layer creates the ACPI, PCIe, and PCI state once and finalizes the late
// device mappings. Later legacy layers may read this state, but must not
// inspect or enumerate it again.

const acpi = @import("../platform/acpi.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const memory_boot = @import("memory_boot.zig");
const pci = @import("../platform/pci.zig");
const pcie = @import("../platform/pcie.zig");

var initialized = false;
var legacy_pci_enumerated = false;
var cached_acpi_info: acpi.Info = .{};
var cached_pcie_status: pcie.Status = .{};

pub fn initDeviceMappings() bool {
    if (initialized) return true;

    if (!memory_boot.isCoreInitialized()) {
        return fail("Platform boot before memory core");
    }

    cached_acpi_info = acpi.inspect();
    cached_pcie_status = pcie.enumerate();
    pci.enumerateLegacy();
    legacy_pci_enumerated = true;

    if (!memory_boot.finalizeDeviceMappings(cached_acpi_info)) {
        return false;
    }

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

pub fn pcieStatus() ?pcie.Status {
    if (!initialized) return null;
    return cached_pcie_status;
}

fn fail(message: []const u8) bool {
    return fatal.fail(.platform, message);
}
