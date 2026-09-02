// Early storage foundation for kernel startup.
//
// This layer continues the storage foundation and later starts boot-critical
// controller probing in a fixed order.

const ahci = @import("../driver/storage/ahci.zig");
const block = @import("../storage/block.zig");
const boot_status = @import("boot_status.zig");
const diag_screen = @import("diag_screen.zig");
const driver_api = @import("driver_api.zig");
const driver_registry = @import("../driver/registry.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const page_cache = @import("../fs/page_cache.zig");
const fs_request = @import("../fs/request.zig");
const fatal = @import("fatal.zig");
const mbr = @import("../storage/mbr.zig");
const memory_boot = @import("memory_boot.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const xhci = @import("../driver/usb/xhci.zig");
const usb_host = @import("../driver/usb/host_controller.zig");
const k = @import("log.zig");

const MAX_SCAN_EVIDENCE: usize = 8;

const ScanEvidence = struct {
    device_index: usize = 0,
    driver_name: []const u8 = "",
    report: mbr.ScanReport = .{},
};

var foundation_initialized = false;
var controllers_initialized = false;
var mbr_scan_count: usize = 0;
var mbr_success_count: usize = 0;
var scan_evidence: [MAX_SCAN_EVIDENCE]ScanEvidence = .{ScanEvidence{}} ** MAX_SCAN_EVIDENCE;
var scan_evidence_count: usize = 0;
var detected_ahci_count: u32 = 0;
var detected_nvme_count: u32 = 0;

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
    scan_evidence = .{ScanEvidence{}} ** MAX_SCAN_EVIDENCE;
    scan_evidence_count = 0;
    detected_ahci_count = pcie_status.ahci_count;
    detected_nvme_count = pcie_status.nvme_count;

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

/// Retains the partition-scan outcome until shell admission. If C: is still
/// absent, the fatal screen can distinguish controller, media, table and
/// filesystem failures without requiring a serial cable on real hardware.
pub fn renderMountDiagnostics() void {
    diag_screen.beginIncident();
    diag_screen.write("[STORAGE] C: missing blocks=");
    diag_screen.writeDec(block.count());
    diag_screen.write(" scans=");
    diag_screen.writeDec(mbr_scan_count);
    diag_screen.write(" pci-ahci=");
    diag_screen.writeDec(detected_ahci_count);
    diag_screen.write(" pci-nvme=");
    diag_screen.writeDec(detected_nvme_count);
    diag_screen.endLine();

    if (scan_evidence_count == 0) {
        diag_screen.line("[STORAGE] no registered block device was scanned");
        return;
    }

    var index: usize = 0;
    while (index < scan_evidence_count) : (index += 1) {
        const evidence = scan_evidence[index];
        const device = block.get(evidence.device_index) orelse {
            diag_screen.write("dev#");
            diag_screen.writeDec(evidence.device_index);
            diag_screen.line(" unavailable after scan");
            continue;
        };

        diag_screen.write("dev#");
        diag_screen.writeDec(evidence.device_index);
        diag_screen.write(" ");
        diag_screen.write(device.name);
        diag_screen.write(" scan=");
        diag_screen.write(evidence.driver_name);
        diag_screen.write(" bus=");
        diag_screen.write(@tagName(device.bus));
        diag_screen.write(" src=");
        diag_screen.write(@tagName(device.source));
        diag_screen.write(" state=");
        diag_screen.write(@tagName(device.state));
        diag_screen.endLine();

        diag_screen.write("  sector-size=");
        diag_screen.writeDec(device.sector_size);
        diag_screen.write(" sectors=");
        diag_screen.writeDec(device.sector_count);
        diag_screen.write(" table=");
        diag_screen.write(@tagName(evidence.report.table));
        diag_screen.endLine();

        diag_screen.write("  result=");
        diag_screen.write(@tagName(evidence.report.result));
        diag_screen.write(" parts=");
        diag_screen.writeDec(evidence.report.partition_count);
        diag_screen.write(" candidates=");
        diag_screen.writeDec(evidence.report.mountable_count);
        diag_screen.write(" fs=");
        diag_screen.writeDec(evidence.report.filesystem_count);
        diag_screen.write(" score=");
        diag_screen.writeDec(evidence.report.best_system_score);
        diag_screen.write("/6");
        diag_screen.write(" markers=");
        diag_screen.writeHex(evidence.report.marker_found_mask);
        diag_screen.write(" marker-io=");
        diag_screen.writeHex(evidence.report.marker_io_mask);
        diag_screen.endLine();

        if (evidence.report.detail.len != 0 or
            evidence.report.mbr_type != 0 or
            device.stats.read_failures != 0)
        {
            diag_screen.write("  detail=");
            diag_screen.write(if (evidence.report.detail.len != 0) evidence.report.detail else "none");
            if (evidence.report.mbr_type != 0) {
                diag_screen.write(" mbr-type=");
                diag_screen.writeHex(evidence.report.mbr_type);
            }
            if (device.stats.read_failures != 0) {
                diag_screen.write(" read-failures=");
                diag_screen.writeDec(device.stats.read_failures);
                diag_screen.write(" last-io=");
                diag_screen.write(@tagName(device.stats.last_error));
            }
            diag_screen.endLine();
        }

        diag_screen.write("  reads=");
        diag_screen.writeDec(device.stats.read_ops);
        diag_screen.write(" sectors=");
        diag_screen.writeDec(device.stats.read_sectors);
        diag_screen.write(" last-lba=");
        diag_screen.writeDec(device.stats.last_request.lba);
        diag_screen.write(" n=");
        diag_screen.writeDec(device.stats.last_request.sectors);
        diag_screen.write(" ticks=");
        diag_screen.writeDec(device.stats.completion_last_ticks);
        diag_screen.endLine();

        var backend_status = driver_api.StorageBackendStatus{
            .state = 0,
            .last_error = 0,
            .last_lba = 0,
            .last_sectors = 0,
            .recoveries = 0,
            .recovery_failures = 0,
        };
        if (driver_api.queryStorageBackendStatus(evidence.device_index, &backend_status)) {
            diag_screen.write("  backend-state=");
            diag_screen.writeDec(backend_status.state);
            diag_screen.write(" error=");
            diag_screen.writeHex(backend_status.last_error);
            diag_screen.write(" recoveries=");
            diag_screen.writeDec(backend_status.recoveries);
            diag_screen.write(" failed=");
            diag_screen.writeDec(backend_status.recovery_failures);
            diag_screen.endLine();
        }
    }
}

fn probeXhci(pcie_status: anytype) void {
    if (usb_host.findByName("XHCI")) |host_index| {
        const host = usb_host.at(host_index) orelse return;
        k.puts("[XHCI] canonical host already initialized source=");
        k.puts(usb_host.sourceLabel(host.source));
        k.puts("; no second controller owner\r\n");
        return;
    }

    const slot = if (pcie_status.xhci_count > 0)
        driver_registry.beginLoad("XHCI", 255, 2)
    else
        null;
    k.puts("[XHCI][WARN] activation R4D missing; starting sole built-in rescue owner\r\n");
    if (xhci.probe()) {
        markInitialized(slot);
        markActive(slot);
    } else {
        markFailed(slot);
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
    if (pcie_status.ahci_count == 0) {
        ahci.resetBuiltInOwner();
        _ = ahci.probe();
        return;
    }

    if (driver_registry.findByName("AHCI")) |existing| {
        const entry = driver_registry.get(existing) orelse {
            k.puts("[AHCI][WARN] canonical preload registry entry unavailable; no second controller owner\r\n");
            return;
        };
        if (entry.source == .preload and entry.state == .active and hasPreloadStorageBackend(.ahci)) {
            k.puts("[AHCI] canonical preload R4D active; owner=preload\r\n");
            return;
        }
        if (entry.source == .preload and entry.state == .active) {
            k.puts("[AHCI][WARN] canonical preload R4D active without blockdevice; no second controller owner\r\n");
            return;
        }
        k.puts("[AHCI][WARN] canonical preload R4D failed or inactive; built-in depth-one rescue requested\r\n");
    } else {
        k.puts("[AHCI][WARN] canonical preload R4D missing; built-in depth-one rescue requested\r\n");
    }

    ahci.resetBuiltInOwner();
    const slot = driver_registry.beginLoad("AHCI", 2, 1);
    if (ahci.probe()) {
        markInitialized(slot);
        if (ahci.blockDeviceCount() > 0) markActive(slot);
    } else {
        markFailed(slot);
    }
}

fn probeNvme(pcie_status: anytype) void {
    if (pcie_status.nvme_count == 0) return;
    if (driver_registry.findByName("NVME")) |existing| {
        const entry = driver_registry.get(existing) orelse {
            k.puts("[NVME][WARN] canonical preload registry entry unavailable\r\n");
            return;
        };
        if (entry.source == .preload and entry.state == .active and hasPreloadStorageBackend(.nvme)) {
            k.puts("[NVME] canonical preload R4D active; owner=preload\r\n");
            return;
        }
        if (entry.source == .preload and entry.state == .active) {
            k.puts("[NVME][WARN] canonical preload R4D active without blockdevice; no second controller owner\r\n");
            return;
        }
        k.puts("[NVME][WARN] canonical preload R4D failed or inactive; no second controller owner\r\n");
        return;
    }
    k.puts("[NVME][WARN] canonical preload R4D missing; no built-in controller fallback\r\n");
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
    const report = mbr.scan(device_index);
    if (report.table_valid) mbr_success_count += 1;
    if (scan_evidence_count < scan_evidence.len) {
        scan_evidence[scan_evidence_count] = .{
            .device_index = device_index,
            .driver_name = driver_name,
            .report = report,
        };
        scan_evidence_count += 1;
    }
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
