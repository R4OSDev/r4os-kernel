const bootlog = @import("../kernel/bootlog.zig");
const monotonic = @import("monotonic.zig");
const pci = @import("pci.zig");
const pcie = @import("pcie.zig");
const scan = @import("pci_scan.zig");

pub const bus_kind_legacy = scan.bus_kind_legacy;
pub const bus_kind_ecam = scan.bus_kind_ecam;
pub const max_devices = scan.max_devices;
pub const Device = scan.Device;

pub const flag_enumerated: u32 = 1 << 0;
pub const flag_ecam: u32 = 1 << 1;
pub const flag_legacy: u32 = 1 << 2;
pub const flag_partial: u32 = 1 << 3;
pub const flag_truncated: u32 = 1 << 4;
pub const flag_ecam_aperture_ready: u32 = 1 << 5;
pub const flag_ecam_rejected_segment: u32 = 1 << 6;

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

pub const InterruptRoute = struct {
    line: u8 = 0xFF,
    pin: u8 = 0,
};

pub const Performance = extern struct {
    version: u32 = 1,
    size: u32 = @sizeOf(Performance),
    flags: u32 = 0,
    generation: u32 = 0,
    capacity: u32 = max_devices,
    mcfg_segment: u32 = 0,
    mcfg_start_bus: u32 = 0,
    mcfg_end_bus: u32 = 0,
    found: u64 = 0,
    stored: u64 = 0,
    dropped: u64 = 0,
    ecam_stored: u64 = 0,
    legacy_stored: u64 = 0,
    vendor_probes_ecam: u64 = 0,
    vendor_probes_legacy: u64 = 0,
    class_reads: u64 = 0,
    header_reads: u64 = 0,
    enumeration_config_reads: u64 = 0,
    function_pages: u64 = 0,
    early_stops: u64 = 0,
    ecam_config_reads: u64 = 0,
    ecam_config_writes: u64 = 0,
    legacy_config_reads: u64 = 0,
    legacy_config_writes: u64 = 0,
    mapping_checks: u64 = 0,
    mapping_hits: u64 = 0,
    mapping_misses: u64 = 0,
    mapping_fast_accesses: u64 = 0,
    invalid_accesses: u64 = 0,
    class_find_calls: u64 = 0,
    class_candidates: u64 = 0,
    detail_materializations: u64 = 0,
    interrupt_dword_reads: u64 = 0,
    command_reads: u64 = 0,
    bar_reads: u64 = 0,
    enumeration_total_ns: u64 = 0,
    ecam_enumeration_ns: u64 = 0,
    legacy_enumeration_ns: u64 = 0,
    timing_unavailable: u64 = 0,
};

var current: Status = .{};
var result: scan.Result = .{};
var generation: u32 = 0;
var class_find_calls: u64 = 0;
var class_candidates: u64 = 0;
var detail_materializations: u64 = 0;
var interrupt_dword_reads: u64 = 0;
var command_reads: u64 = 0;
var bar_reads: u64 = 0;

pub fn enumerate() Status {
    const total_start = monotonic.capture();
    result = .{};
    current = .{};
    class_find_calls = 0;
    class_candidates = 0;
    detail_materializations = 0;
    interrupt_dword_reads = 0;
    command_reads = 0;
    bar_reads = 0;
    pci.resetAccessMetrics();

    const ecam = pcie.status();
    const plan = scan.planCoverage(ecam.aperture_ready, ecam.segment, ecam.start_bus, ecam.end_bus);
    current.ecam_used = plan.ecam_used;
    current.legacy_used = plan.legacy_used;
    current.partial = plan.partial;

    var range_index: usize = 0;
    while (range_index < plan.count) : (range_index += 1) {
        const range = plan.ranges[range_index];
        const before = result.metrics;
        const started = monotonic.capture();
        const complete = scan.scanRange(
            &result,
            range,
            if (range.bus_kind == bus_kind_ecam)
                .{ .read_config32 = readEcam }
            else
                .{ .read_config32 = readLegacy },
        );
        recordRange(range.bus_kind, before, result.metrics, started);
        if (!complete) break;
    }

    generation +%= 1;
    if (generation == 0) generation = 1;
    current.enumerated = true;
    current.generation = generation;
    current.device_count = result.metrics.found;
    current.stored_count = result.metrics.stored;
    current.dropped_count = result.metrics.dropped;
    current.truncated = result.metrics.truncated;
    current.class_reads = result.metrics.class_reads;
    current.header_reads = result.metrics.header_reads;
    current.enumeration_config_reads = result.metrics.config_reads;
    current.function_pages = result.metrics.function_pages;
    current.early_stops = result.metrics.early_stops;
    current.flags = flag_enumerated |
        (if (current.ecam_used) flag_ecam else 0) |
        (if (current.legacy_used) flag_legacy else 0) |
        (if (current.partial) flag_partial else 0) |
        (if (current.truncated) flag_truncated else 0) |
        (if (ecam.aperture_ready) flag_ecam_aperture_ready else 0) |
        (if (ecam.available and ecam.segment != 0) flag_ecam_rejected_segment else 0);
    classifyStoredDevices();
    if (monotonic.elapsedNanoseconds(total_start, monotonic.capture())) |elapsed| {
        if (elapsed != 0) {
            current.enumeration_total_ns = elapsed;
        } else {
            current.timing_unavailable +%= 1;
        }
    } else {
        current.timing_unavailable +%= 1;
    }
    current.reason = if (current.truncated)
        "canonical PCI inventory truncated"
    else if (current.partial)
        "ECAM plus uncovered legacy buses enumerated"
    else if (current.ecam_used)
        "ECAM canonical inventory enumerated"
    else
        "legacy PCI fallback enumerated";
    logSummary();
    return current;
}

pub fn status() Status {
    return current;
}

pub fn count() usize {
    return @intCast(current.stored_count);
}

pub fn deviceAt(index: usize) ?Device {
    if (index >= current.stored_count or index >= result.devices.len) return null;
    return result.devices[index];
}

pub fn findByClass(class_code: u8, subclass: u8, start_index: usize) ?usize {
    class_find_calls +%= 1;
    const found = scan.findByClass(&result, class_code, subclass, start_index);
    const stored: usize = @intCast(current.stored_count);
    if (start_index < stored) {
        class_candidates +%= if (found) |index| index - start_index + 1 else stored - start_index;
    }
    return found;
}

pub fn noteDetailMaterialization() void {
    detail_materializations +%= 1;
}

pub fn readConfig32(device: Device, offset: u16) u32 {
    return readConfig32At(device.bus_kind, device.bus, device.device, device.function, offset);
}

pub fn readConfig32At(bus_kind: u8, bus: u8, device: u8, function: u8, offset: u16) u32 {
    if (bus_kind == bus_kind_ecam) return pcie.readCurrentConfig32(bus, device, function, offset);
    if (bus_kind == bus_kind_legacy and offset <= 0xFF) return pci.readConfig32(bus, device, function, @truncate(offset));
    return 0xFFFF_FFFF;
}

pub fn writeConfig32At(bus_kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) bool {
    if (bus_kind == bus_kind_ecam) return pcie.writeCurrentConfig32(bus, device, function, offset, value);
    if (bus_kind == bus_kind_legacy and offset <= 0xFF) {
        pci.writeConfig32(bus, device, function, @truncate(offset), value);
        return true;
    }
    return false;
}

pub fn readBar(device: Device, index: u8) u32 {
    if (index >= 6) return 0;
    bar_reads +%= 1;
    return readConfig32(device, 0x10 + @as(u16, index) * 4);
}

pub fn readBar64(device: Device, index: u8) u64 {
    const low = readBar(device, index);
    if ((low & 1) != 0) return low;
    const bar_type = (low >> 1) & 0x3;
    if (bar_type != 0x2 or index >= 5) return low;
    const high = readBar(device, index + 1);
    return (@as(u64, high) << 32) | low;
}

pub fn readCommand(device: Device) u16 {
    command_reads +%= 1;
    return @truncate(readConfig32(device, 0x04));
}

pub fn writeCommand(device: Device, command: u16) bool {
    return writeConfig32At(device.bus_kind, device.bus, device.device, device.function, 0x04, command);
}

pub fn readInterruptRoute(device: Device) InterruptRoute {
    interrupt_dword_reads +%= 1;
    const raw = readConfig32(device, 0x3C);
    if (raw == 0xFFFF_FFFF) return .{};
    return .{
        .line = @truncate(raw),
        .pin = @truncate(raw >> 8),
    };
}

pub fn performance() Performance {
    const ecam = pcie.status();
    const legacy = pci.accessMetrics();
    return .{
        .flags = current.flags,
        .generation = current.generation,
        .mcfg_segment = ecam.segment,
        .mcfg_start_bus = ecam.start_bus,
        .mcfg_end_bus = ecam.end_bus,
        .found = current.device_count,
        .stored = current.stored_count,
        .dropped = current.dropped_count,
        .ecam_stored = current.ecam_stored_count,
        .legacy_stored = current.legacy_stored_count,
        .vendor_probes_ecam = current.ecam_vendor_probes,
        .vendor_probes_legacy = current.legacy_vendor_probes,
        .class_reads = current.class_reads,
        .header_reads = current.header_reads,
        .enumeration_config_reads = current.enumeration_config_reads,
        .function_pages = current.function_pages,
        .early_stops = current.early_stops,
        .ecam_config_reads = ecam.config_reads,
        .ecam_config_writes = ecam.config_writes,
        .legacy_config_reads = legacy.config_reads,
        .legacy_config_writes = legacy.config_writes,
        .mapping_checks = ecam.mapping_checks,
        .mapping_hits = ecam.mapping_hits,
        .mapping_misses = ecam.mapping_misses,
        .mapping_fast_accesses = ecam.fast_accesses,
        .invalid_accesses = ecam.invalid_accesses,
        .class_find_calls = class_find_calls,
        .class_candidates = class_candidates,
        .detail_materializations = detail_materializations,
        .interrupt_dword_reads = interrupt_dword_reads,
        .command_reads = command_reads,
        .bar_reads = bar_reads,
        .enumeration_total_ns = current.enumeration_total_ns,
        .ecam_enumeration_ns = current.ecam_enumeration_ns,
        .legacy_enumeration_ns = current.legacy_enumeration_ns,
        .timing_unavailable = current.timing_unavailable,
    };
}

fn readEcam(_: ?*anyopaque, bus: u8, device: u8, function: u8, offset: u16) u32 {
    return pcie.readCurrentConfig32(bus, device, function, offset);
}

fn readLegacy(_: ?*anyopaque, bus: u8, device: u8, function: u8, offset: u16) u32 {
    if (offset > 0xFF) return 0xFFFF_FFFF;
    return pci.readConfig32(bus, device, function, @truncate(offset));
}

fn recordRange(bus_kind: u8, before: scan.Metrics, after: scan.Metrics, started: monotonic.Stamp) void {
    const vendor_delta = after.vendor_probes -% before.vendor_probes;
    const stored_delta = after.stored -% before.stored;
    if (bus_kind == bus_kind_ecam) {
        current.ecam_vendor_probes +%= vendor_delta;
        current.ecam_stored_count +%= stored_delta;
    } else {
        current.legacy_vendor_probes +%= vendor_delta;
        current.legacy_stored_count +%= stored_delta;
    }
    if (monotonic.elapsedNanoseconds(started, monotonic.capture())) |elapsed| {
        if (elapsed != 0) {
            if (bus_kind == bus_kind_ecam) current.ecam_enumeration_ns +%= elapsed else current.legacy_enumeration_ns +%= elapsed;
        } else {
            current.timing_unavailable +%= 1;
        }
    } else {
        current.timing_unavailable +%= 1;
    }
}

fn classifyStoredDevices() void {
    var index: usize = 0;
    while (index < current.stored_count and index < result.devices.len) : (index += 1) {
        const device = result.devices[index];
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
}

fn logSummary() void {
    bootlog.puts("[PCI] canonical source=");
    if (current.ecam_used and current.legacy_used) {
        bootlog.puts("ecam+legacy");
    } else if (current.ecam_used) {
        bootlog.puts("ecam");
    } else {
        bootlog.puts("legacy");
    }
    bootlog.puts(" found=");
    bootlog.putDec(current.device_count);
    bootlog.puts(" stored=");
    bootlog.putDec(current.stored_count);
    bootlog.puts(" dropped=");
    bootlog.putDec(current.dropped_count);
    bootlog.puts(" vendor-probes=");
    bootlog.putDec(current.ecam_vendor_probes + current.legacy_vendor_probes);
    bootlog.puts(" config-reads=");
    bootlog.putDec(current.enumeration_config_reads);
    bootlog.puts(" truncated=");
    bootlog.puts(if (current.truncated) "yes" else "no");
    bootlog.puts("\r\n");
}
