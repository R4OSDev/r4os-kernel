const acpi = @import("acpi.zig");
const bootlog = @import("../kernel/bootlog.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const pci_scan = @import("pci_scan.zig");

const MAX_DEVICES: usize = pci_scan.max_devices;
const MAX_LOGGED_DEVICES: usize = 24;

const CLASS_MASS_STORAGE: u8 = 0x01;
const SUBCLASS_AHCI: u8 = 0x06;
const SUBCLASS_NVME: u8 = 0x08;
const CLASS_NETWORK: u8 = 0x02;
const SUBCLASS_ETHERNET: u8 = 0x00;
const CLASS_MULTIMEDIA: u8 = 0x04;
const SUBCLASS_HDA: u8 = 0x03;
const CLASS_SERIAL_BUS: u8 = 0x0C;
const SUBCLASS_USB: u8 = 0x03;
const PROGIF_XHCI: u8 = 0x30;

pub const Device = pci_scan.Device;

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

var current: Status = .{};
var devices: [MAX_DEVICES]Device = .{Device{}} ** MAX_DEVICES;

pub fn configure(info: acpi.Info) void {
    const bus_count: u64 = if (info.mcfg_base != 0 and info.mcfg_start_bus <= info.mcfg_end_bus)
        @as(u64, info.mcfg_end_bus) - @as(u64, info.mcfg_start_bus) + 1
    else
        0;
    current = .{
        .available = info.mcfg_base != 0 and bus_count != 0,
        .mcfg_base = info.mcfg_base,
        .segment = info.mcfg_segment,
        .start_bus = info.mcfg_start_bus,
        .end_bus = info.mcfg_end_bus,
        .aperture_bytes = bus_count << 20,
        .reason = if (info.mcfg_base != 0 and bus_count != 0) "MCFG configured" else "MCFG ECAM missing",
    };
    @memset(devices[0..], Device{});
}

pub fn activateMappedAperture() bool {
    if (!current.available or current.aperture_bytes == 0) return false;
    if (current.mcfg_base > ~@as(u64, 0) - (current.aperture_bytes - 1)) {
        current.invalid_accesses +%= 1;
        current.reason = "MCFG ECAM range overflow";
        return false;
    }
    const last = current.mcfg_base + current.aperture_bytes - 1;
    current.mapping_checks +%= 2;
    const first_mapped = paging.isMapped(phys.physToVirt(current.mcfg_base));
    const last_mapped = paging.isMapped(phys.physToVirt(last));
    if (first_mapped) current.mapping_hits +%= 1 else current.mapping_misses +%= 1;
    if (last_mapped) current.mapping_hits +%= 1 else current.mapping_misses +%= 1;
    current.aperture_ready = first_mapped and last_mapped;
    current.reason = if (current.aperture_ready) "MCFG ECAM aperture ready" else "MCFG ECAM aperture mapping missing";
    return current.aperture_ready;
}

pub fn enumerate() Status {
    const info = acpi.info();
    configure(info);
    @memset(devices[0..], Device{});
    bootlog.puts("[PCIE] enumeration\r\n");
    if (info.mcfg_base == 0) {
        bootlog.puts("[PCIE][WARN] no MCFG ECAM base, skipping\r\n");
        return current;
    }

    var logged: usize = 0;
    var bus: u16 = info.mcfg_start_bus;
    while (bus <= info.mcfg_end_bus) : (bus += 1) {
        var device: u8 = 0;
        while (device < 32) : (device += 1) {
            var function: u8 = 0;
            while (function < 8) : (function += 1) {
                const vendor_device = readConfig32(info.mcfg_base, @intCast(bus), device, function, 0x00);
                const vendor: u16 = @truncate(vendor_device & 0xFFFF);
                if (vendor == 0xFFFF) {
                    if (function == 0) break;
                    continue;
                }
                const device_id: u16 = @truncate(vendor_device >> 16);
                const class_reg = readConfig32(info.mcfg_base, @intCast(bus), device, function, 0x08);
                const info_device = Device{
                    .bus = @intCast(bus),
                    .device = device,
                    .function = function,
                    .vendor_id = vendor,
                    .device_id = device_id,
                    .class_code = @truncate(class_reg >> 24),
                    .subclass = @truncate(class_reg >> 16),
                    .prog_if = @truncate(class_reg >> 8),
                };
                store(info_device);
                if (logged < MAX_LOGGED_DEVICES) {
                    logDevice("[PCIE] ", info_device);
                    logged += 1;
                }
                const header_type: u8 = @truncate(readConfig32(info.mcfg_base, @intCast(bus), device, function, 0x0C) >> 16);
                if (function == 0 and (header_type & 0x80) == 0) break;
            }
        }
    }

    bootlog.puts("[PCIE] devices found=");
    bootlog.putDec(current.device_count);
    bootlog.puts(" ahci=");
    bootlog.putDec(current.ahci_count);
    bootlog.puts(" nvme=");
    bootlog.putDec(current.nvme_count);
    bootlog.puts(" net=");
    bootlog.putDec(current.network_count);
    bootlog.puts(" hda=");
    bootlog.putDec(current.hda_count);
    bootlog.puts(" xhci=");
    bootlog.putDec(current.xhci_count);
    if (current.device_count > @as(u32, @intCast(logged))) {
        bootlog.puts(" logged=");
        bootlog.putDec(logged);
    }
    bootlog.puts("\r\n");
    current.enumerated = true;
    current.reason = "ECAM enumerated";
    return current;
}

pub fn status() Status {
    return current;
}

pub fn deviceAt(index: usize) ?Device {
    if (index >= current.stored_count or index >= devices.len) return null;
    return devices[index];
}

pub fn readConfig32(base: u64, bus: u8, device: u8, function: u8, offset: u16) u32 {
    current.config_reads +%= 1;
    const addr = configAddress(base, bus, device, function, offset) orelse {
        current.invalid_accesses +%= 1;
        return 0xFFFF_FFFF;
    };
    if (!ensureMapped(addr)) return 0xFFFF_FFFF;
    const ptr: *volatile u32 = @ptrFromInt(phys.physToVirt(addr));
    return ptr.*;
}

pub fn writeConfig32(base: u64, bus: u8, device: u8, function: u8, offset: u16, value: u32) void {
    current.config_writes +%= 1;
    const addr = configAddress(base, bus, device, function, offset) orelse {
        current.invalid_accesses +%= 1;
        return;
    };
    if (!ensureMapped(addr)) return;
    const ptr: *volatile u32 = @ptrFromInt(phys.physToVirt(addr));
    ptr.* = value;
}

pub fn readCurrentConfig32(bus: u8, device: u8, function: u8, offset: u16) u32 {
    return readConfig32(current.mcfg_base, bus, device, function, offset);
}

pub fn writeCurrentConfig32(bus: u8, device: u8, function: u8, offset: u16, value: u32) bool {
    if (!validAccess(current.mcfg_base, bus, device, function, offset)) {
        current.invalid_accesses +%= 1;
        return false;
    }
    writeConfig32(current.mcfg_base, bus, device, function, offset, value);
    return true;
}

pub fn readBar(device: Device, index: u8) u32 {
    if (index >= 6 or current.mcfg_base == 0) return 0;
    return readConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x10 + @as(u16, index) * 4);
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
    if (current.mcfg_base == 0) return 0;
    return @truncate(readConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x04) & 0xFFFF);
}

pub fn writeCommand(device: Device, command: u16) void {
    if (current.mcfg_base == 0) return;
    const raw = readConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x04);
    const value = (raw & 0xFFFF_0000) | command;
    writeConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x04, value);
}

pub fn readInterruptLine(device: Device) u8 {
    if (current.mcfg_base == 0) return 0xFF;
    return @truncate(readConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x3C) & 0xFF);
}

pub fn readInterruptPin(device: Device) u8 {
    if (current.mcfg_base == 0) return 0;
    return @truncate((readConfig32(current.mcfg_base, device.bus, device.device, device.function, 0x3C) >> 8) & 0xFF);
}

fn store(device: Device) void {
    if (current.device_count == 0) current.reason = "ECAM contains PCIe functions";
    current.device_count += 1;
    if (current.stored_count < MAX_DEVICES) {
        devices[@intCast(current.stored_count)] = device;
        current.stored_count += 1;
    }
    if (device.class_code == CLASS_MASS_STORAGE and device.subclass == SUBCLASS_AHCI) {
        if (current.ahci_count == 0) current.first_ahci = device;
        current.ahci_count += 1;
    } else if (device.class_code == CLASS_MASS_STORAGE and device.subclass == SUBCLASS_NVME) {
        if (current.nvme_count == 0) current.first_nvme = device;
        current.nvme_count += 1;
    } else if (device.class_code == CLASS_NETWORK and device.subclass == SUBCLASS_ETHERNET) {
        if (current.network_count == 0) current.first_network = device;
        current.network_count += 1;
    } else if (device.class_code == CLASS_MULTIMEDIA and device.subclass == SUBCLASS_HDA) {
        if (current.hda_count == 0) current.first_hda = device;
        current.hda_count += 1;
    } else if (device.class_code == CLASS_SERIAL_BUS and device.subclass == SUBCLASS_USB and device.prog_if == PROGIF_XHCI) {
        if (current.xhci_count == 0) current.first_xhci = device;
        current.xhci_count += 1;
    }
}

fn ensureMapped(addr: u64) bool {
    if (current.aperture_ready) {
        current.fast_accesses +%= 1;
        return true;
    }
    const page = addr & ~(paging.PAGE_SIZE - 1);
    const virt = phys.physToVirt(page);
    current.mapping_checks +%= 1;
    if (!paging.isMapped(virt)) {
        current.mapping_misses +%= 1;
        if (!paging.mapPage(virt, page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) {
            current.malformed_reads += 1;
            return false;
        }
    } else {
        current.mapping_hits +%= 1;
    }
    return true;
}

fn configAddress(base: u64, bus: u8, device: u8, function: u8, offset: u16) ?u64 {
    if (!validAccess(base, bus, device, function, offset)) return null;
    const relative = pci_scan.ecamOffset(current.start_bus, bus, device, function, offset) orelse return null;
    if (base > ~@as(u64, 0) - relative) return null;
    return base + relative;
}

fn validAccess(base: u64, bus: u8, device: u8, function: u8, offset: u16) bool {
    return current.available and
        base == current.mcfg_base and
        bus >= current.start_bus and bus <= current.end_bus and
        device < 32 and function < 8 and offset <= 0x0FFF;
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

fn minStored(value: u32) usize {
    return if (value < MAX_LOGGED_DEVICES) @intCast(value) else MAX_LOGGED_DEVICES;
}
