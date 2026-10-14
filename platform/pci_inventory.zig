// Native canonical inventory; public performance comes from the Contract.
const std = @import("std");
const api = @import("../program/r4x_api.zig");
const scan = @import("pci_scan.zig");
pub const Device = scan.Device;
pub const bus_kind_legacy = scan.bus_kind_legacy;
pub const bus_kind_ecam = scan.bus_kind_ecam;
pub const max_devices = scan.max_devices;
pub const flag_enumerated = api.pci_inventory_flag_enumerated;
pub const flag_ecam = api.pci_inventory_flag_ecam;
pub const flag_legacy = api.pci_inventory_flag_legacy;
pub const flag_partial = api.pci_inventory_flag_partial;
pub const flag_truncated = api.pci_inventory_flag_truncated;
pub const flag_ecam_aperture_ready = api.pci_inventory_flag_ecam_aperture_ready;
pub const flag_ecam_rejected_segment = api.pci_inventory_flag_ecam_rejected_segment;
pub const Status = struct {
    enumerated: bool = false,
    ecam_used: bool = false,
    legacy_used: bool = false,
    partial: bool = false,
    truncated: bool = false,
    flags: u32 = 0,
    generation: u32 = 0,
    device_count: u32 = 0,
    stored_count: u32 = 0,
    dropped_count: u32 = 0,
    ecam_stored_count: u32 = 0,
    legacy_stored_count: u32 = 0,
    ahci_count: u32 = 0,
    nvme_count: u32 = 0,
    network_count: u32 = 0,
    hda_count: u32 = 0,
    xhci_count: u32 = 0,
    first_ahci: Device = .{},
    first_nvme: Device = .{},
    first_network: Device = .{},
    first_hda: Device = .{},
    first_xhci: Device = .{},
    ecam_vendor_probes: u64 = 0,
    legacy_vendor_probes: u64 = 0,
    class_reads: u64 = 0,
    header_reads: u64 = 0,
    enumeration_config_reads: u64 = 0,
    function_pages: u64 = 0,
    early_stops: u64 = 0,
    enumeration_total_ns: u64 = 0,
    ecam_enumeration_ns: u64 = 0,
    legacy_enumeration_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    reason: []const u8 = "not initialized",
};

pub const RawStatus = extern struct {
    enumerated: bool = false,
    ecam_used: bool = false,
    legacy_used: bool = false,
    partial: bool = false,
    truncated: bool = false,
    flags: u32 = 0,
    generation: u32 = 0,
    device_count: u32 = 0,
    stored_count: u32 = 0,
    dropped_count: u32 = 0,
    ecam_stored_count: u32 = 0,
    legacy_stored_count: u32 = 0,
    ahci_count: u32 = 0,
    nvme_count: u32 = 0,
    network_count: u32 = 0,
    hda_count: u32 = 0,
    xhci_count: u32 = 0,
    first_ahci: Device = .{},
    first_nvme: Device = .{},
    first_network: Device = .{},
    first_hda: Device = .{},
    first_xhci: Device = .{},
    ecam_vendor_probes: u64 = 0,
    legacy_vendor_probes: u64 = 0,
    class_reads: u64 = 0,
    header_reads: u64 = 0,
    enumeration_config_reads: u64 = 0,
    function_pages: u64 = 0,
    early_stops: u64 = 0,
    enumeration_total_ns: u64 = 0,
    ecam_enumeration_ns: u64 = 0,
    legacy_enumeration_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    reason: u8 = 0,
};

pub const InterruptRoute = extern struct { line: u8 = 0xff, pin: u8 = 0 };
pub const Performance = api.ProgramPciInventoryPerformanceInfo;
extern fn r4os_rust_pci_inventory_control(operation: u8, device: Device, argument: u16) u64;
extern fn r4os_rust_pci_inventory_config(operation: u8, kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) u64;
extern fn r4os_rust_pci_inventory_status(output: *RawStatus) void;
extern fn r4os_rust_pci_inventory_device(index: u64, output: *Device) bool;
extern fn r4os_rust_pci_inventory_find(class: u8, subclass: u8, start: u64, output: *u64) bool;
extern fn r4os_rust_pci_inventory_route(device: Device, output: *InterruptRoute) void;
extern fn r4os_rust_pci_inventory_performance(output: *Performance) i32;
pub fn project(raw: RawStatus) Status {
    var value: Status = .{};
    inline for (std.meta.fields(Status)) |field| {
        if (comptime std.mem.eql(u8, field.name, "reason")) {
            value.reason = switch (raw.reason) {
                0 => "not initialized",
                1 => "canonical PCI inventory truncated",
                2 => "ECAM plus uncovered legacy buses enumerated",
                3 => "ECAM canonical inventory enumerated",
                4 => "legacy PCI fallback enumerated",
                else => unreachable,
            };
        } else @field(value, field.name) = @field(raw, field.name);
    }
    return value;
}
pub fn enumerate() Status {
    _ = r4os_rust_pci_inventory_control(0, .{}, 0);
    return status();
}
pub fn status() Status {
    var output: RawStatus = undefined;
    r4os_rust_pci_inventory_status(&output);
    return project(output);
}
pub fn count() usize {
    return @intCast(r4os_rust_pci_inventory_control(1, .{}, 0));
}
pub fn deviceAt(index: usize) ?Device {
    var output: Device = undefined;
    return if (r4os_rust_pci_inventory_device(index, &output)) output else null;
}
pub fn findByClass(class_code: u8, subclass: u8, start_index: usize) ?usize {
    var output: u64 = undefined;
    return if (r4os_rust_pci_inventory_find(class_code, subclass, start_index, &output)) @intCast(output) else null;
}
pub fn noteDetailMaterialization() void {
    _ = r4os_rust_pci_inventory_control(2, .{}, 0);
}
pub fn readConfig32(device: Device, offset: u16) u32 {
    return readConfig32At(device.bus_kind, device.bus, device.device, device.function, offset);
}
pub fn readConfig32At(kind: u8, bus: u8, device: u8, function: u8, offset: u16) u32 {
    return @intCast(r4os_rust_pci_inventory_config(0, kind, bus, device, function, offset, 0));
}
pub fn writeConfig32At(kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) bool {
    return r4os_rust_pci_inventory_config(1, kind, bus, device, function, offset, value) != 0;
}
pub fn readBar(device: Device, index: u8) u32 {
    return @intCast(r4os_rust_pci_inventory_control(3, device, index));
}
pub fn readBar64(device: Device, index: u8) u64 {
    return r4os_rust_pci_inventory_control(4, device, index);
}
pub fn readCommand(device: Device) u16 {
    return @intCast(r4os_rust_pci_inventory_control(5, device, 0));
}
pub fn writeCommand(device: Device, command: u16) bool {
    return r4os_rust_pci_inventory_control(6, device, command) != 0;
}
pub fn readInterruptRoute(device: Device) InterruptRoute {
    var output: InterruptRoute = undefined;
    r4os_rust_pci_inventory_route(device, &output);
    return output;
}
pub fn readPerformance(output: *Performance) i32 {
    return r4os_rust_pci_inventory_performance(output);
}
pub fn performance() Performance {
    var output: Performance = undefined;
    _ = readPerformance(&output);
    return output;
}
comptime {
    if (@sizeOf(RawStatus) != 216 or @offsetOf(RawStatus, "first_ahci") != 56 or @offsetOf(RawStatus, "reason") != 208 or
        @sizeOf(InterruptRoute) != 2 or @sizeOf(Performance) != 280)
        @compileError("Rust PCI inventory layout differs");
}
