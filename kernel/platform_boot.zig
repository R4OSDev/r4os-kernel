// Rust owns platform boot order and cached completion views.
const acpi = @import("../platform/acpi.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const memory_boot = @import("memory_boot.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
extern fn r4os_rust_platform_boot_control(operation: u8) bool;
extern fn r4os_rust_platform_boot_acpi(output: *acpi.RawInfo) bool;
extern fn r4os_rust_platform_boot_pci(output: *pci_inventory.RawStatus) bool;
export fn r4os_zig_platform_boot_step(operation: u8, input: ?*const acpi.RawInfo) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("platform boot work requested by host fixture");
    return switch (operation) {
        0 => memory_boot.isCoreInitialized(),
        1 => memory_boot.finalizeDeviceMappings(acpi.project(input.?.*)),
        2 => blk: {
            boot_status.statusLine("  Platform mappings [OK]\r\n");
            break :blk true;
        },
        3 => fatal.fail(.platform, "Platform boot before memory core"),
        else => false,
    };
}
pub fn initDeviceMappings() bool {
    return r4os_rust_platform_boot_control(0);
}
pub fn isInitialized() bool {
    return r4os_rust_platform_boot_control(1);
}
pub fn isLegacyPciEnumerated() bool {
    return r4os_rust_platform_boot_control(2);
}
pub fn acpiInfo() ?acpi.Info {
    var output: acpi.RawInfo = undefined;
    return if (r4os_rust_platform_boot_acpi(&output)) acpi.project(output) else null;
}
pub fn pcieStatus() ?pci_inventory.Status {
    var output: pci_inventory.RawStatus = undefined;
    return if (r4os_rust_platform_boot_pci(&output)) pci_inventory.project(output) else null;
}
pub fn pciInventoryStatus() ?pci_inventory.Status {
    return pcieStatus();
}
