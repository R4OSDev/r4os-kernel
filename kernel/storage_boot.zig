// Early storage foundation for kernel startup.
//
// This layer continues the storage foundation and later starts boot-critical
// controller probing in a fixed order.

const ahci = @import("../driver/storage/ahci.zig");
const block = @import("../storage/block.zig");
const boot_status = @import("boot_status.zig");
const driver_registry = @import("../driver/registry.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const page_cache = @import("../fs/page_cache.zig");
const fs_request = @import("../fs/request.zig");
const fatal = @import("fatal.zig");
const mbr = @import("../storage/mbr.zig");
const memory_boot = @import("memory_boot.zig");
const nvme = @import("../driver/storage/nvme.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const xhci = @import("../driver/usb/xhci.zig");
const k = @import("log.zig");

var foundation_initialized = false;
var controllers_initialized = false;
var mbr_scan_count: usize = 0;
var mbr_success_count: usize = 0;

pub fn initFoundation() bool {
    if (foundation_initialized) return true;

    if (!memory_boot.isCoreInitialized()) {
        return fail("Storage foundation before memory core");
    }

    drive.init();
    vfs.init();
    block.init();
    page_cache.init();
    fs_request.init();

    foundation_initialized = true;
    k.puts("  Storage foundation ");
    k.puts("[OK]\r\n");
    return true;
}

pub fn isFoundationInitialized() bool {
    return foundation_initialized;
}

pub fn initControllers(pcie_status: anytype) bool {
    if (controllers_initialized) return true;

    if (!foundation_initialized) {
        return fail("Storage controllers before foundation");
    }

    mbr_scan_count = 0;
    mbr_success_count = 0;

    probeXhci(pcie_status);
    probeUsbMsc();
    probeAtapioPreload();
    probeAhci(pcie_status);
    probeNvme(pcie_status);

    scanRegisteredBlockDevices();
    applyLegacyDataDriveLayoutPolicy();

    controllers_initialized = true;
    boot_status.statusLine("  Storage [OK]\r\n");
    return true;
}

pub fn isControllersInitialized() bool {
    return controllers_initialized;
}

pub fn mbrScans() usize {
    return mbr_scan_count;
}

pub fn mbrSuccesses() usize {
    return mbr_success_count;
}

fn probeXhci(pcie_status: anytype) void {
    if (pcie_status.xhci_count > 0) {
        const slot = if (driver_registry.findByName("XHCI")) |existing| blk: {
            const entry = driver_registry.get(existing) orelse break :blk driver_registry.beginLoad("XHCI", 255, 1);
            if (entry.source == .preload and entry.state == .active) {
                k.puts("[XHCI] preload R4D active; legacy rescue data path armed; owner=preload\r\n");
                break :blk null;
            }
            break :blk driver_registry.beginLoad("XHCI", 255, 1);
        } else driver_registry.beginLoad("XHCI", 255, 1);
        if (xhci.probe()) {
            markInitialized(slot);
            markActive(slot);
        } else {
            markFailed(slot);
        }
    } else {
        _ = xhci.probe();
    }
}

fn probeUsbMsc() void {
    const slot = if (driver_registry.findByName("USBMSC")) |existing| blk: {
        const entry = driver_registry.get(existing) orelse break :blk driver_registry.beginLoad("USBMSC", 2, 1);
        if (entry.source == .preload and entry.state == .active) {
            k.puts("[USBMSC] preload R4D active; legacy rescue data path armed; owner=preload\r\n");
            usb_msc.setPreloadOwner();
            break :blk existing;
        }
        usb_msc.resetBuiltInOwner();
        break :blk driver_registry.beginLoad("USBMSC", 2, 1);
    } else blk: {
        usb_msc.resetBuiltInOwner();
        break :blk driver_registry.beginLoad("USBMSC", 2, 1);
    };
    if (usb_msc.init()) {
        markInitialized(slot);
        if (usb_msc.blockDeviceCount() > 0) markActive(slot);
    } else {
        markFailed(slot);
    }
}

fn probeAtapioPreload() void {
    if (driver_registry.findByName("ATAPIO")) |existing| {
        const entry = driver_registry.get(existing);
        if (entry != null and entry.?.source == .preload and entry.?.state == .active) {
            if (hasPreloadStorageBackend(.ata)) {
                k.puts("[ATAPIO] preload R4D active; built-in ATA-PIO data path removed\r\n");
                return;
            }
            k.puts("[ATAPIO][WARN] preload R4D active without blockdevice; no built-in ATA-PIO fallback in standard kernel\r\n");
            return;
        }
    }
    k.puts("[ATAPIO][WARN] preload R4D missing; no built-in ATA-PIO fallback in standard kernel\r\n");
}

fn probeAhci(pcie_status: anytype) void {
    if (pcie_status.ahci_count > 0) {
        const slot = if (driver_registry.findByName("AHCI")) |existing| blk: {
            const entry = driver_registry.get(existing) orelse break :blk driver_registry.beginLoad("AHCI", 2, 1);
            if (entry.source == .preload and entry.state == .active) {
                k.puts("[AHCI] preload R4D active; legacy rescue data path armed; owner=preload\r\n");
                ahci.setPreloadOwner();
                break :blk existing;
            }
            ahci.resetBuiltInOwner();
            break :blk driver_registry.beginLoad("AHCI", 2, 1);
        } else blk: {
            ahci.resetBuiltInOwner();
            break :blk driver_registry.beginLoad("AHCI", 2, 1);
        };
        if (ahci.probe()) {
            markInitialized(slot);
            if (ahci.blockDeviceCount() > 0) markActive(slot);
        } else {
            markFailed(slot);
        }
    } else {
        ahci.resetBuiltInOwner();
        _ = ahci.probe();
    }
}

fn probeNvme(pcie_status: anytype) void {
    if (pcie_status.nvme_count > 0) {
        if (driver_registry.findByName("NVME")) |existing| {
            const entry = driver_registry.get(existing);
            if (entry != null and entry.?.source == .preload and entry.?.state == .active) {
                if (hasPreloadStorageBackend(.nvme)) {
                    k.puts("[NVME] preload R4D active; legacy rescue data path skipped; owner=preload\r\n");
                    return;
                }
                k.puts("[NVME][WARN] preload R4D active without blockdevice; legacy rescue data path armed; owner=preload\r\n");
            }
        }
        const slot = driver_registry.beginLoad("NVMe", 2, 1);
        if (nvme.probe()) {
            markInitialized(slot);
            if (nvme.blockDeviceCount() > 0) markActive(slot);
        } else {
            markFailed(slot);
        }
    }
}

fn scanRegisteredBlockDevices() void {
    var scanned: [16]bool = .{false} ** 16;

    if (usb_msc.deviceIndex()) |disk| {
        _ = usb_msc.reselectActiveDevice();
        scanBlockDevice(&scanned, disk, "USBMSC");
    }

    var ahci_slot: usize = 0;
    while (ahci_slot < 8) : (ahci_slot += 1) {
        if (ahci.deviceIndexAt(ahci_slot)) |disk| {
            scanBlockDevice(&scanned, disk, "AHCI");
        }
    }

    var nvme_slot: usize = 0;
    while (nvme_slot < nvme.blockDeviceCount()) : (nvme_slot += 1) {
        if (nvme.deviceIndexAt(nvme_slot)) |disk| {
            scanBlockDevice(&scanned, disk, "NVMe");
        }
    }

    scanExternalStorageBackends(&scanned);
}

fn scanExternalStorageBackends(scanned: *[16]bool) void {
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        const device = block.get(index) orelse continue;
        if (device.source == .builtin) continue;
        const name = switch (device.bus) {
            .nvme => "NVME.R4D",
            .ahci => "AHCI.R4D",
            .usb => "USBMSC.R4D",
            .ata => "ATAPIO.R4D",
            else => "R4D",
        };
        scanBlockDevice(scanned, index, name);
    }
}

fn hasPreloadStorageBackend(bus: block.Bus) bool {
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        const device = block.get(index) orelse continue;
        if (device.bus == bus and device.source == .preload) return true;
    }
    return false;
}

fn scanBlockDevice(scanned: *[16]bool, device_index: usize, driver_name: []const u8) void {
    if (device_index < scanned.len and scanned[device_index]) return;
    if (device_index < scanned.len) scanned[device_index] = true;

    k.puts("  Storage scan ");
    k.puts(driver_name);
    k.puts(" block=");
    k.putDec(device_index);
    k.puts("\r\n");

    mbr_scan_count += 1;
    if (mbr.scan(device_index)) mbr_success_count += 1;
}

fn applyLegacyDataDriveLayoutPolicy() void {
    const volume = vfs.volumeForDrive('D') orelse return;
    const root = vfs.resolvePath(volume, "\\") orelse return;
    ensureDataDirectory(volume, root, "DOCS");
    ensureDataDirectory(volume, root, "TEMP");
    ensureDataDirectory(volume, root, "MEDIA");
}

fn ensureDataDirectory(volume: vfs.Volume, root: vfs.NodeRef, name: []const u8) void {
    var path: [16]u8 = .{0} ** 16;
    path[0] = '\\';
    const count = @min(name.len, path.len - 2);
    if (count > 0) @memcpy(path[1 .. 1 + count], name[0..count]);
    const full = path[0 .. 1 + count];
    if (vfs.resolvePath(volume, full) != null) return;
    if (vfs.makeDirectory(volume, root, name)) {
        k.puts("  D:\\");
        k.puts(name);
        k.puts(" [OK]\r\n");
    } else {
        k.puts("  D:\\");
        k.puts(name);
        k.puts(" [WARN]\r\n");
    }
}

fn markInitialized(slot: ?usize) void {
    if (slot) |driver_slot| driver_registry.setState(driver_slot, .initialized);
}

fn markActive(slot: ?usize) void {
    if (slot) |driver_slot| driver_registry.setState(driver_slot, .active);
}

fn markFailed(slot: ?usize) void {
    if (slot) |driver_slot| driver_registry.setState(driver_slot, .failed);
}

fn fail(message: []const u8) bool {
    return fatal.fail(.storage, message);
}
