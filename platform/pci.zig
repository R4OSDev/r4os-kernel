const io = @import("../arch/x86_64/io.zig");
const bootlog = @import("../kernel/bootlog.zig");
const pci_scan = @import("pci_scan.zig");

const CONFIG_ADDRESS: u16 = 0x0CF8;
const CONFIG_DATA: u16 = 0x0CFC;
const MAX_DEVICES: usize = pci_scan.max_devices;
const MAX_LOGGED_DEVICES: usize = 24;

pub const Device = pci_scan.Device;

pub const AccessMetrics = struct {
    config_reads: u64 = 0,
    config_writes: u64 = 0,
};

var devices: [MAX_DEVICES]Device = undefined;
var device_count: usize = 0;
var access_metrics: AccessMetrics = .{};

pub fn resetAccessMetrics() void {
    access_metrics = .{};
}

pub fn accessMetrics() AccessMetrics {
    return access_metrics;
}

pub fn enumerateLegacy() void {
    device_count = 0;
    bootlog.puts("[PCI] legacy CF8/CFC enumeration\r\n");

    var logged: usize = 0;
    var bus: u16 = 0;
    while (bus < 256) : (bus += 1) {
        var dev: u8 = 0;
        while (dev < 32) : (dev += 1) {
            var func: u8 = 0;
            while (func < 8) : (func += 1) {
                const vendor_device = readConfig32(@intCast(bus), dev, func, 0x00);
                const vendor: u16 = @truncate(vendor_device & 0xFFFF);
                if (vendor == 0xFFFF) {
                    if (func == 0) break;
                    continue;
                }

                const device_id: u16 = @truncate(vendor_device >> 16);
                const class_reg = readConfig32(@intCast(bus), dev, func, 0x08);
                const info = Device{
                    .bus_kind = pci_scan.bus_kind_legacy,
                    .bus = @intCast(bus),
                    .device = dev,
                    .function = func,
                    .vendor_id = vendor,
                    .device_id = device_id,
                    .class_code = @truncate(class_reg >> 24),
                    .subclass = @truncate(class_reg >> 16),
                    .prog_if = @truncate(class_reg >> 8),
                };
                store(info);
                if (logged < MAX_LOGGED_DEVICES) {
                    logDevice("[PCI] ", info);
                    logged += 1;
                }

                const header_type: u8 = @truncate(readConfig32(@intCast(bus), dev, func, 0x0C) >> 16);
                if (func == 0 and (header_type & 0x80) == 0) break;
            }
        }
    }

    bootlog.puts("[PCI] devices found=");
    bootlog.putDec(device_count);
    if (device_count > logged) {
        bootlog.puts(" logged=");
        bootlog.putDec(logged);
    }
    bootlog.puts("\r\n");
}

pub fn findByClass(class_code: u8, subclass: u8) ?Device {
    var i: usize = 0;
    while (i < device_count) : (i += 1) {
        const d = devices[i];
        if (d.class_code == class_code and d.subclass == subclass) return d;
    }
    return null;
}

pub fn count() usize {
    return device_count;
}

pub fn deviceAt(index: usize) ?Device {
    if (index >= device_count) return null;
    return devices[index];
}

pub fn readBar(device: Device, index: u8) u32 {
    if (index >= 6) return 0;
    return readConfig32(device.bus, device.device, device.function, 0x10 + @as(u8, index) * 4);
}

pub fn readBar64(device: Device, index: u8) u64 {
    const low = readBar(device, index);
    if ((low & 1) != 0) return low;
    const bar_type = (low >> 1) & 0x3;
    if (bar_type != 0x2 or index >= 5) return low;
    const high = readBar(device, index + 1);
    return (@as(u64, high) << 32) | @as(u64, low);
}

pub fn readCommand(device: Device) u16 {
    return readConfig16(device.bus, device.device, device.function, 0x04);
}

pub fn readInterruptLine(device: Device) u8 {
    return @truncate(readConfig32(device.bus, device.device, device.function, 0x3C) & 0xFF);
}

pub fn readInterruptPin(device: Device) u8 {
    return @truncate((readConfig32(device.bus, device.device, device.function, 0x3C) >> 8) & 0xFF);
}

pub fn enableIoAndBusMaster(device: Device) void {
    var command = readConfig16(device.bus, device.device, device.function, 0x04);
    command |= 0x0005;
    command &= ~@as(u16, 0x0400);
    writeConfig16(device.bus, device.device, device.function, 0x04, command);
}

pub fn enableMemoryAndBusMaster(device: Device) void {
    var command = readConfig16(device.bus, device.device, device.function, 0x04);
    command |= 0x0006;
    command &= ~@as(u16, 0x0400);
    writeConfig16(device.bus, device.device, device.function, 0x04, command);
}

pub fn readConfig32(bus: u8, device: u8, function: u8, offset: u8) u32 {
    access_metrics.config_reads +%= 1;
    io.outl(CONFIG_ADDRESS, configAddress(bus, device, function, offset));
    return io.inl(CONFIG_DATA);
}

pub fn writeConfig32(bus: u8, device: u8, function: u8, offset: u8, value: u32) void {
    access_metrics.config_writes +%= 1;
    io.outl(CONFIG_ADDRESS, configAddress(bus, device, function, offset));
    io.outl(CONFIG_DATA, value);
}

pub fn writeConfig8(bus: u8, device: u8, function: u8, offset: u8, value: u8) void {
    access_metrics.config_writes +%= 1;
    io.outl(CONFIG_ADDRESS, configAddress(bus, device, function, offset));
    io.outb(CONFIG_DATA + @as(u16, offset & 3), value);
}

fn readConfig16(bus: u8, device: u8, function: u8, offset: u8) u16 {
    const value = readConfig32(bus, device, function, offset & 0xFC);
    const shift: u5 = @intCast((offset & 2) * 8);
    return @truncate(value >> shift);
}

fn writeConfig16(bus: u8, device: u8, function: u8, offset: u8, value: u16) void {
    const aligned = offset & 0xFC;
    const shift: u5 = @intCast((offset & 2) * 8);
    var current = readConfig32(bus, device, function, aligned);
    current &= ~(@as(u32, 0xFFFF) << shift);
    current |= @as(u32, value) << shift;
    writeConfig32(bus, device, function, aligned, current);
}

fn configAddress(bus: u8, device: u8, function: u8, offset: u8) u32 {
    return 0x8000_0000 |
        (@as(u32, bus) << 16) |
        (@as(u32, device) << 11) |
        (@as(u32, function) << 8) |
        @as(u32, offset & 0xFC);
}

fn store(device: Device) void {
    if (device_count >= devices.len) return;
    devices[device_count] = device;
    device_count += 1;
}

fn logDevice(prefix: []const u8, device: Device) void {
    bootlog.puts(prefix);
    bootlog.putDec(device.bus);
    bootlog.puts(":");
    bootlog.putDec(device.device);
    bootlog.puts(".");
    bootlog.putDec(device.function);
    bootlog.puts(" vid=0x");
    bootlog.putHex(device.vendor_id, 4);
    bootlog.puts(" did=0x");
    bootlog.putHex(device.device_id, 4);
    bootlog.puts(" class=0x");
    bootlog.putHex(device.class_code, 2);
    bootlog.puts(" subclass=0x");
    bootlog.putHex(device.subclass, 2);
    bootlog.puts(" if=0x");
    bootlog.putHex(device.prog_if, 2);
    bootlog.puts("\r\n");
}
