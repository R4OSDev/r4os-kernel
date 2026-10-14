// Native ECAM owner; fixed C projections preserve existing callers.
const std = @import("std");
const acpi = @import("acpi.zig");
pub const Device = @import("pci_scan.zig").Device;
pub const Status = struct {
    available: bool = false,
    enumerated: bool = false,
    mcfg_base: u64 = 0,
    segment: u16 = 0,
    start_bus: u8 = 0,
    end_bus: u8 = 0,
    aperture_ready: bool = false,
    aperture_bytes: u64 = 0,
    mapping_checks: u64 = 0,
    mapping_hits: u64 = 0,
    mapping_misses: u64 = 0,
    fast_accesses: u64 = 0,
    config_reads: u64 = 0,
    config_writes: u64 = 0,
    invalid_accesses: u64 = 0,
    device_count: u32 = 0,
    stored_count: u32 = 0,
    ahci_count: u32 = 0,
    nvme_count: u32 = 0,
    network_count: u32 = 0,
    hda_count: u32 = 0,
    xhci_count: u32 = 0,
    malformed_reads: u32 = 0,
    first_ahci: Device = .{},
    first_nvme: Device = .{},
    first_network: Device = .{},
    first_hda: Device = .{},
    first_xhci: Device = .{},
    reason: []const u8 = "not initialized",
};

const RawStatus = extern struct {
    available: bool = false,
    enumerated: bool = false,
    mcfg_base: u64 = 0,
    segment: u16 = 0,
    start_bus: u8 = 0,
    end_bus: u8 = 0,
    aperture_ready: bool = false,
    aperture_bytes: u64 = 0,
    mapping_checks: u64 = 0,
    mapping_hits: u64 = 0,
    mapping_misses: u64 = 0,
    fast_accesses: u64 = 0,
    config_reads: u64 = 0,
    config_writes: u64 = 0,
    invalid_accesses: u64 = 0,
    device_count: u32 = 0,
    stored_count: u32 = 0,
    ahci_count: u32 = 0,
    nvme_count: u32 = 0,
    network_count: u32 = 0,
    hda_count: u32 = 0,
    xhci_count: u32 = 0,
    malformed_reads: u32 = 0,
    first_ahci: Device = .{},
    first_nvme: Device = .{},
    first_network: Device = .{},
    first_hda: Device = .{},
    first_xhci: Device = .{},
    reason: u8 = 0,
};

extern fn r4os_rust_pcie_configure(base: u64, segment: u16, start: u8, end: u8) void;
extern fn r4os_rust_pcie_control(operation: u8, device: Device, argument: u16) u64;
extern fn r4os_rust_pcie_config(operation: u8, base: u64, bus: u8, device: u8, function: u8, offset: u16, value: u32) u64;
extern fn r4os_rust_pcie_status(output: *RawStatus) void;
extern fn r4os_rust_pcie_device(index: u64, output: *Device) bool;
fn project(raw: RawStatus) Status {
    var value: Status = .{};
    inline for (std.meta.fields(Status)) |field| {
        if (comptime std.mem.eql(u8, field.name, "reason")) {
            value.reason = switch (raw.reason) {
                0 => "not initialized",
                1 => "MCFG configured",
                2 => "MCFG ECAM missing or invalid",
                3 => "MCFG ECAM range overflow",
                4 => "MCFG ECAM aperture ready",
                5 => "MCFG ECAM aperture mapping missing",
                6 => "ECAM contains PCIe functions",
                7 => "ECAM enumerated",
                else => unreachable,
            };
        } else @field(value, field.name) = @field(raw, field.name);
    }
    return value;
}
pub fn configure(info: acpi.Info) void {
    r4os_rust_pcie_configure(info.mcfg_base, info.mcfg_segment, info.mcfg_start_bus, info.mcfg_end_bus);
}
pub fn activateMappedAperture() bool {
    return r4os_rust_pcie_control(1, .{}, 0) != 0;
}
pub fn enumerate() Status {
    _ = r4os_rust_pcie_control(0, .{}, 0);
    return status();
}
pub fn status() Status {
    var output: RawStatus = undefined;
    r4os_rust_pcie_status(&output);
    return project(output);
}
pub fn deviceAt(index: usize) ?Device {
    var output: Device = undefined;
    return if (r4os_rust_pcie_device(index, &output)) output else null;
}
pub fn readConfig32(base: u64, bus: u8, device: u8, function: u8, offset: u16) u32 {
    return @intCast(r4os_rust_pcie_config(0, base, bus, device, function, offset, 0));
}
pub fn writeConfig32(base: u64, bus: u8, device: u8, function: u8, offset: u16, value: u32) void {
    _ = r4os_rust_pcie_config(1, base, bus, device, function, offset, value);
}
pub fn readCurrentConfig32(bus: u8, device: u8, function: u8, offset: u16) u32 {
    return @intCast(r4os_rust_pcie_config(2, 0, bus, device, function, offset, 0));
}
pub fn writeCurrentConfig32(bus: u8, device: u8, function: u8, offset: u16, value: u32) bool {
    return r4os_rust_pcie_config(3, 0, bus, device, function, offset, value) != 0;
}
pub fn readBar(device: Device, index: u8) u32 {
    return @intCast(r4os_rust_pcie_control(2, device, index));
}
pub fn readBar64(device: Device, index: u8) u64 {
    return r4os_rust_pcie_control(3, device, index);
}
pub fn readCommand(device: Device) u16 {
    return @intCast(r4os_rust_pcie_control(4, device, 0));
}
pub fn readInterruptLine(device: Device) u8 {
    return @intCast(r4os_rust_pcie_control(5, device, 0));
}
pub fn readInterruptPin(device: Device) u8 {
    return @intCast(r4os_rust_pcie_control(6, device, 0));
}
pub fn writeCommand(device: Device, command: u16) void {
    _ = r4os_rust_pcie_control(7, device, command);
}
comptime {
    if (@sizeOf(RawStatus) != 184 or @offsetOf(RawStatus, "first_ahci") != 120 or @offsetOf(RawStatus, "reason") != 180) @compileError("Rust PCIe status layout differs");
}
