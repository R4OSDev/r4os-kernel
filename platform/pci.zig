// Native CF8/CFC owner; the existing caller retains boot/driver-init admission.
const scan = @import("pci_scan.zig");
pub const Device = scan.Device;
pub const AccessMetrics = extern struct { config_reads: u64 = 0, config_writes: u64 = 0 };
extern fn r4os_rust_pci_control(operation: u8, device: Device, argument: u8) u64;
extern fn r4os_rust_pci_read(bus: u8, device: u8, function: u8, offset: u8) u32;
extern fn r4os_rust_pci_write(bus: u8, device: u8, function: u8, offset: u8, value: u32, width: u8) void;
extern fn r4os_rust_pci_device(index: u64, output: *Device) bool;
extern fn r4os_rust_pci_find(class: u8, subclass: u8, output: *Device) bool;
extern fn r4os_rust_pci_metrics(output: *AccessMetrics) void;
pub fn resetAccessMetrics() void {
    _ = r4os_rust_pci_control(0, .{}, 0);
}
pub fn accessMetrics() AccessMetrics {
    var output: AccessMetrics = undefined;
    r4os_rust_pci_metrics(&output);
    return output;
}
pub fn enumerateLegacy() void {
    _ = r4os_rust_pci_control(1, .{}, 0);
}
pub fn count() usize {
    return @intCast(r4os_rust_pci_control(2, .{}, 0));
}
pub fn deviceAt(index: usize) ?Device {
    var output: Device = undefined;
    return if (r4os_rust_pci_device(index, &output)) output else null;
}
pub fn findByClass(class_code: u8, subclass: u8) ?Device {
    var output: Device = undefined;
    return if (r4os_rust_pci_find(class_code, subclass, &output)) output else null;
}
pub fn readBar(device: Device, index: u8) u32 {
    return @intCast(r4os_rust_pci_control(3, device, index));
}
pub fn readBar64(device: Device, index: u8) u64 {
    return r4os_rust_pci_control(4, device, index);
}
pub fn readCommand(device: Device) u16 {
    return @intCast(r4os_rust_pci_control(5, device, 0));
}
pub fn readInterruptLine(device: Device) u8 {
    return @intCast(r4os_rust_pci_control(6, device, 0));
}
pub fn readInterruptPin(device: Device) u8 {
    return @intCast(r4os_rust_pci_control(7, device, 0));
}
pub fn enableIoAndBusMaster(device: Device) void {
    _ = r4os_rust_pci_control(8, device, 0);
}
pub fn enableMemoryAndBusMaster(device: Device) void {
    _ = r4os_rust_pci_control(9, device, 0);
}
pub fn readConfig32(bus: u8, device: u8, function: u8, offset: u8) u32 {
    return r4os_rust_pci_read(bus, device, function, offset);
}
pub fn writeConfig32(bus: u8, device: u8, function: u8, offset: u8, value: u32) void {
    r4os_rust_pci_write(bus, device, function, offset, value, 32);
}
pub fn writeConfig8(bus: u8, device: u8, function: u8, offset: u8, value: u8) void {
    r4os_rust_pci_write(bus, device, function, offset, value, 8);
}
comptime {
    if (@sizeOf(AccessMetrics) != 16) @compileError("Rust PCI metrics layout differs");
}
