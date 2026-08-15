const acpi = @import("acpi.zig");
const usb_core = @import("../driver/usb/core.zig");
const usb_hid = @import("../driver/usb/hid.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const usb_host = @import("../driver/usb/host_controller.zig");
const block = @import("../storage/block.zig");
const cpu = @import("cpu.zig");
const display = @import("../display/display.zig");
const driver_registry = @import("../driver/registry.zig");
const drive = @import("../fs/drive.zig");
const irq_status = @import("irq.zig");
const net_config = @import("../net/config.zig");
const net = @import("../net/core.zig");
const pci = @import("pci.zig");
const pcie = @import("pcie.zig");
const protocol_registry = @import("../protocol/registry.zig");

const MAX_RECORDS: usize = 128;

pub const Bus = enum(u8) {
    platform,
    acpi,
    pci,
    pcie,
    storage,
    display,
    audio,
    input,
    driver,
    network,
    usb,
    protocol,
};

pub const Binding = enum(u8) {
    with_driver,
    without_driver,
    unknown,
};

pub const Record = struct {
    binding: Binding = .unknown,
    bus: Bus = .platform,
    name: []const u8 = "unclassified-device",
    driver: []const u8 = "",
    status: []const u8 = "",
    note: []const u8 = "",
    bus_no: u8 = 0,
    device_no: u8 = 0,
    function_no: u8 = 0,
    vendor_id: u16 = 0,
    device_id: u16 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    prog_if: u8 = 0,
    has_pci_address: bool = false,
};

pub const Snapshot = struct {
    records: [MAX_RECORDS]Record = .{Record{}} ** MAX_RECORDS,
    count: usize = 0,
    with_driver: u32 = 0,
    without_driver: u32 = 0,
    unknown: u32 = 0,
    truncated: bool = false,

    fn add(self: *Snapshot, rec: Record) void {
        if (self.count >= self.records.len) {
            self.truncated = true;
            return;
        }
        self.records[self.count] = rec;
        self.count += 1;
        switch (rec.binding) {
            .with_driver => self.with_driver += 1,
            .without_driver => self.without_driver += 1,
            .unknown => self.unknown += 1,
        }
    }
};

var current_snapshot: Snapshot = .{};
var network_notes: [net.MAX_ADAPTERS][96]u8 = .{.{0} ** 96} ** net.MAX_ADAPTERS;

pub fn snapshot() *const Snapshot {
    current_snapshot = .{};
    addPlatformRecords(&current_snapshot);
    addBusRecords(&current_snapshot);
    addUsbDeviceRecords(&current_snapshot);
    addRuntimeDriverRecords(&current_snapshot);
    addNetBackendRecords(&current_snapshot);
    addProtocolRecords(&current_snapshot);
    return &current_snapshot;
}

fn addPlatformRecords(s: *Snapshot) void {
    const ci = cpu.status();
    s.add(.{
        .binding = .with_driver,
        .bus = .platform,
        .name = "x86_64 CPU",
        .driver = "platform/cpu",
        .status = "diagnostic-active",
        .note = if (cpu.moduleSimdAllowed())
            "FPU/SSE task state active; module SIMD allowed"
        else if (ci.features.xsave and !ci.features.osxsave)
            "XSAVE present but OSXSAVE off; module SIMD blocked"
        else
            "feature gates active",
    });

    const ai = acpi.info();
    s.add(.{
        .binding = if (ai.rsdp_phys != 0) .with_driver else .without_driver,
        .bus = .acpi,
        .name = "ACPI tables",
        .driver = "platform/acpi",
        .status = if (ai.rsdp_phys != 0) "active" else "missing",
        .note = if (ai.mcfg_base != 0) "RSDP/MADT/MCFG/HPET visible" else "RSDP/MADT visible, MCFG missing or legacy profile",
    });

    const irq = irq_status.status();
    s.add(.{
        .binding = .with_driver,
        .bus = .platform,
        .name = "Interrupt controller",
        .driver = "platform/irq",
        .status = irq_status.controllerName(irq.controller),
        .note = irq.fallback_reason,
    });
    s.add(.{
        .binding = .with_driver,
        .bus = .platform,
        .name = "Timer",
        .driver = "platform/timer",
        .status = irq_status.timerName(irq.timer),
        .note = if (irq.hpet_available) "HPET visible" else "legacy timer fallback",
    });

    const ds = display.stats();
    s.add(.{
        .binding = .with_driver,
        .bus = .display,
        .name = "Active display backend",
        .driver = ds.name,
        .status = "active",
        .note = display.cachePolicyName(ds.mapping.cache_policy),
    });

    s.add(.{
        .binding = if (block.count() > 0) .with_driver else .without_driver,
        .bus = .storage,
        .name = "Block storage registry",
        .driver = "storage/block",
        .status = if (block.count() > 0) "active" else "empty",
        .note = if (drive.get('C') != null and drive.get('D') != null) "C: and D: mounted" else "mounts incomplete",
    });
    addBlockDeviceRecords(s);
}

fn addBlockDeviceRecords(s: *Snapshot) void {
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        const d = block.get(index) orelse continue;
        s.add(.{
            .binding = .with_driver,
            .bus = .storage,
            .name = d.name,
            .driver = d.driver,
            .status = blockDeviceMountStatus(index),
            .note = blockDeviceNote(d, index),
        });
    }
}

fn addBusRecords(s: *Snapshot) void {
    const ps = pcie.status();
    if (ps.enumerated) {
        var index: usize = 0;
        while (pcie.deviceAt(index)) |d| : (index += 1) {
            s.add(recordFromPcie(d));
        }
    } else {
        var index: usize = 0;
        while (pci.deviceAt(index)) |d| : (index += 1) {
            s.add(recordFromPci(d));
        }
    }
}

fn addUsbDeviceRecords(s: *Snapshot) void {
    var index: usize = 0;
    while (usb_core.deviceAt(index)) |d| : (index += 1) {
        const class_code = if (d.first_interface_class != 0) d.first_interface_class else d.device_class;
        const subclass = if (d.first_interface_class != 0) d.first_interface_subclass else d.device_subclass;
        const protocol = if (d.first_interface_class != 0) d.first_interface_protocol else d.device_protocol;
        s.add(.{
            .binding = .with_driver,
            .bus = .usb,
            .name = usbDeviceName(class_code, subclass, protocol),
            .driver = usbDeviceDriver(class_code, subclass, protocol),
            .status = usbDeviceStatus(d, class_code, subclass, protocol),
            .note = usbDeviceNote(class_code, subclass, protocol),
            .vendor_id = d.vendor_id,
            .device_id = d.product_id,
            .class_code = class_code,
            .subclass = subclass,
            .prog_if = protocol,
        });
    }
}

fn addRuntimeDriverRecords(s: *Snapshot) void {
    var index: usize = 0;
    while (index < driver_registry.MAX_DRIVERS) : (index += 1) {
        const e = driver_registry.entryAt(index) orelse continue;
        if (e.driver_type == 2 or e.driver_type == 3 or e.driver_type == 5) continue;
        s.add(.{
            .binding = .with_driver,
            .bus = .driver,
            .name = e.name[0..e.name_len],
            .driver = driver_registry.sourceName(e.source),
            .status = driver_registry.stateName(e.state),
            .note = driver_registry.typeName(e.driver_type),
        });
    }
}

fn addNetBackendRecords(s: *Snapshot) void {
    var index: usize = 0;
    while (index < net.count()) : (index += 1) {
        const adapter = net.get(index) orelse continue;
        s.add(.{
            .binding = .with_driver,
            .bus = .network,
            .name = adapter.name,
            .driver = adapter.driver,
            .status = net.linkName(adapter.link),
            .note = networkNote(index, adapter),
            .bus_no = adapter.bus_no,
            .device_no = adapter.device_no,
            .function_no = adapter.function_no,
            .vendor_id = adapter.vendor_id,
            .device_id = adapter.device_id,
            .class_code = 0x02,
            .subclass = 0x00,
            .prog_if = 0x00,
            .has_pci_address = adapter.bus == .pci or adapter.bus == .pcie,
        });
    }
}

fn addProtocolRecords(s: *Snapshot) void {
    var index: usize = 0;
    while (index < protocol_registry.MAX_PROTOCOLS) : (index += 1) {
        const e = protocol_registry.entryAt(index) orelse continue;
        s.add(.{
            .binding = .with_driver,
            .bus = .protocol,
            .name = e.role[0..e.role_len],
            .driver = protocol_registry.sourceName(e.source),
            .status = protocol_registry.stateName(e.state),
            .note = e.note[0..e.note_len],
            .class_code = @intCast(@min(e.category, 255)),
        });
    }
}

fn networkNote(index: usize, adapter: *const net.Adapter) []const u8 {
    if (index >= network_notes.len) return "registered NetBackend; trusted direct device access";
    var w = NoteWriter{ .out = network_notes[index][0..] };
    w.text("mac=");
    w.mac(adapter.mac);
    w.text(" ip=");
    w.ip(net_config.localIp());
    w.text(" gw=");
    w.ip(net_config.gatewayIp());
    w.text(" src=");
    w.text(net_config.sourceName());
    w.text(" rx=");
    w.num(adapter.stats.rx_packets);
    w.text(" tx=");
    w.num(adapter.stats.tx_packets);
    w.text(" err=");
    w.num(adapter.stats.errors);
    w.text(" last=");
    w.text(adapter.stats.last_error);
    return w.slice();
}

fn usbDeviceName(class_code: u8, subclass: u8, protocol: u8) []const u8 {
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01) return "USB HID Boot Keyboard";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02) return "USB HID Boot Mouse";
    if (class_code == 0x03) return "USB HID device";
    if (class_code == 0x08) return "USB Mass Storage device";
    return "USB device";
}

fn usbDeviceNote(class_code: u8, subclass: u8, protocol: u8) []const u8 {
    const hs = usb_hid.status();
    const ms = usb_msc.status();
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01 and hs.keyboard_bound) return usbKeyboardBoundNote(hs.boot_source);
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01 and hs.protocol_required_missing != 0) return "HID keyboard binding blocked; source=none; HIDREPORT.R4P and USBHID.R4P required";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01) return "descriptor parsed; HID keyboard binding pending";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02 and hs.mouse_bound) return usbMouseBoundNote(hs.boot_source);
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02 and hs.protocol_required_missing != 0) return "HID mouse binding blocked; source=none; HIDREPORT.R4P and USBHID.R4P required";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02) return "descriptor parsed; HID mouse binding follows after keyboard path";
    if (class_code == 0x03) return "HID descriptor parsed; non-boot HID handling planned later";
    if (class_code == 0x08 and ms.block_registered) return usbMscBoundNote(ms);
    if (class_code == 0x08 and ms.protocol_required_missing != 0) return "Mass Storage blocked; source=none; USBBOT.R4P and USBSCSI.R4P required";
    if (class_code == 0x08) return "Mass Storage descriptor parsed; USBMSC follows in 0.13.7";
    return "USB descriptors parsed; class driver pending";
}

fn usbMscBoundNote(status: usb_msc.Status) []const u8 {
    if (isR4pSource(status.bot_source) and isR4pSource(status.scsi_source)) {
        if (usb_msc.hasExternalOwner()) {
            if (status.write_protected) return "bound through USBMSC.R4D; preload/R4P-required active; rescue; read-only blockdevice registered";
            return "bound through USBMSC.R4D; preload/R4P-required active; rescue; writable blockdevice registered";
        }
        if (status.write_protected) return "Mass Storage bound through USBMSC; preload/R4P-required active; read-only blockdevice registered";
        return "Mass Storage bound through USBMSC; preload/R4P-required active; writable blockdevice registered";
    }
    return "Mass Storage bound through USBMSC; protocol role status unavailable";
}

fn usbKeyboardBoundNote(source: []const u8) []const u8 {
    if (isR4pSource(source)) return "HID boot keyboard bound through USBHID; preload/R4P-required active; PS/2 remains active";
    return "HID boot keyboard binding requires active USBHID.R4P; PS/2 remains active";
}

fn usbMouseBoundNote(source: []const u8) []const u8 {
    if (isR4pSource(source)) return "HID boot mouse bound through USBHID; preload/R4P-required active; PS/2 remains active";
    return "HID boot mouse binding requires active USBHID.R4P; PS/2 remains active";
}

fn isR4pSource(source: []const u8) bool {
    return (source.len == 3 and source[0] == 'r' and source[1] == '4' and source[2] == 'p') or
        (source.len == 7 and source[0] == 'p' and source[1] == 'r' and source[2] == 'e' and source[3] == 'l' and source[4] == 'o' and source[5] == 'a' and source[6] == 'd');
}

fn usbDeviceDriver(class_code: u8, subclass: u8, protocol: u8) []const u8 {
    const hs = usb_hid.status();
    const ms = usb_msc.status();
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01 and hs.keyboard_bound) return "USBHID";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02 and hs.mouse_bound) return "USBHID";
    if (class_code == 0x08 and ms.block_registered) return "USBMSC";
    return "usb/core";
}

fn usbDeviceStatus(d: *const usb_core.Device, class_code: u8, subclass: u8, protocol: u8) []const u8 {
    const hs = usb_hid.status();
    const ms = usb_msc.status();
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01 and hs.keyboard_bound) return "bound-input";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x01 and hs.protocol_required_missing != 0) return "blocked";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02 and hs.mouse_bound) return "bound-input";
    if (class_code == 0x03 and subclass == 0x01 and protocol == 0x02 and hs.protocol_required_missing != 0) return "blocked";
    if (class_code == 0x08 and ms.block_registered) return "bound-storage";
    if (class_code == 0x08 and ms.protocol_required_missing != 0) return "blocked";
    return if (d.configured) "configured" else "addressed";
}

fn recordFromPci(d: pci.Device) Record {
    return recordFromFields(.pci, d.bus, d.device, d.function, d.vendor_id, d.device_id, d.class_code, d.subclass, d.prog_if);
}

fn recordFromPcie(d: pcie.Device) Record {
    return recordFromFields(.pcie, d.bus, d.device, d.function, d.vendor_id, d.device_id, d.class_code, d.subclass, d.prog_if);
}

fn recordFromFields(bus_kind: Bus, bus_no: u8, device_no: u8, function_no: u8, vendor_id: u16, device_id: u16, class_code: u8, subclass: u8, prog_if: u8) Record {
    const known = knownName(class_code, subclass, prog_if, vendor_id, device_id);
    const binding = bindingFor(class_code, subclass, prog_if, vendor_id, device_id);
    return .{
        .binding = binding,
        .bus = bus_kind,
        .name = if (known.len > 0) known else "unclassified-device",
        .driver = driverFor(binding, class_code, subclass, vendor_id, device_id),
        .status = statusFor(binding, class_code, subclass, vendor_id, device_id),
        .note = noteFor(class_code, subclass, prog_if, vendor_id, device_id),
        .bus_no = bus_no,
        .device_no = device_no,
        .function_no = function_no,
        .vendor_id = vendor_id,
        .device_id = device_id,
        .class_code = class_code,
        .subclass = subclass,
        .prog_if = prog_if,
        .has_pci_address = true,
    };
}

fn knownName(class_code: u8, subclass: u8, prog_if: u8, vendor_id: u16, device_id: u16) []const u8 {
    if (vendor_id == 0x8086 and device_id == 0x29C0) return "Intel Q35 host bridge";
    if (vendor_id == 0x8086 and device_id == 0x2918) return "Intel ICH9 LPC/ISA bridge";
    if (vendor_id == 0x8086 and device_id == 0x2930) return "Intel ICH9 SMBus";
    if (vendor_id == 0x8086 and device_id == 0x2922) return "Intel ICH9 AHCI";
    if (vendor_id == 0x8086 and device_id == 0x293E) return "Intel ICH9 HDA";
    if (vendor_id == 0x8086 and device_id == 0x1237) return "Intel i440FX host bridge";
    if (vendor_id == 0x8086 and device_id == 0x7000) return "Intel PIIX3 ISA bridge";
    if (vendor_id == 0x8086 and device_id == 0x7010) return "Intel PIIX3 IDE";
    if (vendor_id == 0x8086 and device_id == 0x7113) return "Intel PIIX4 ACPI";
    if (vendor_id == 0x8086 and device_id == 0x100E) return "Intel e1000 network";
    if (vendor_id == 0x8086 and device_id == 0x10D3) return "Intel e1000e network";
    if (vendor_id == 0x10EC and device_id == 0x8139) return "Realtek RTL8139 network";
    if (vendor_id == 0x8086 and device_id == 0x2668) return "Intel HDA";
    if (vendor_id == 0x1234 and device_id == 0x1111) return "QEMU Standard VGA";
    if (vendor_id == 0x1B36 and device_id == 0x0010) return "QEMU NVMe";
    if (class_code == 0x01 and subclass == 0x01) return "IDE controller";
    if (class_code == 0x01 and subclass == 0x06) return "AHCI SATA controller";
    if (class_code == 0x01 and subclass == 0x08) return "NVMe controller";
    if (class_code == 0x02) return "Network controller";
    if (class_code == 0x03) return "Display controller";
    if (class_code == 0x04 and subclass == 0x03) return "High Definition Audio";
    if (class_code == 0x06 and subclass == 0x00) return "Host bridge";
    if (class_code == 0x06 and subclass == 0x01) return "ISA bridge";
    if (class_code == 0x0C and subclass == 0x03 and prog_if == 0x30) return "xHCI USB controller";
    if (class_code == 0x0C and subclass == 0x05) return "SMBus controller";
    return "";
}

fn bindingFor(class_code: u8, subclass: u8, prog_if: u8, vendor_id: u16, device_id: u16) Binding {
    if (class_code == 0x01 and subclass == 0x01 and preloadBlockCount(.ata) > 0) return .with_driver;
    if (class_code == 0x01 and subclass == 0x06 and blockCount(.ahci) > 0) return .with_driver;
    if (class_code == 0x01 and subclass == 0x08 and preloadBlockCount(.nvme) > 0) return .with_driver;
    if (class_code == 0x01 and subclass == 0x08 and blockCount(.nvme) > 0) return .with_driver;
    if (class_code == 0x0C and subclass == 0x03 and prog_if == 0x30 and xhciHostPresent()) return .with_driver;
    // Realtek-NICs, fuer die ein NetBackend gebunden ist, sind der PCI-Sicht
    // nach mit Treiber versorgt (sonst erscheint dieselbe Karte doppelt: als
    // gebundenes NetBackend UND als vermeintlich treiberloser Controller).
    // Gilt fuer die 8139 und die 8168-Familie gleichermassen.
    if (vendor_id == 0x10EC and (device_id == 0x8139 or device_id == 0x8168) and
        net.hasAdapterForDevice(vendor_id, device_id)) return .with_driver;
    if (class_code == 0x04 and subclass == 0x03) return .with_driver;
    if (class_code == 0x06 and (subclass == 0x00 or subclass == 0x01)) return .with_driver;
    if (vendor_id == 0x8086 and device_id == 0x7113) return .with_driver;
    if (class_code == 0x01 and (subclass == 0x06 or subclass == 0x08)) return .without_driver;
    if (class_code == 0x0C and subclass == 0x03 and prog_if == 0x30) return .without_driver;
    if (class_code == 0x02 or class_code == 0x03) return .without_driver;
    if (class_code == 0x0C and subclass == 0x05) return .without_driver;
    if (knownName(class_code, subclass, prog_if, vendor_id, device_id).len > 0) return .without_driver;
    return .unknown;
}

fn driverFor(binding: Binding, class_code: u8, subclass: u8, vendor_id: u16, device_id: u16) []const u8 {
    if (binding != .with_driver) return "";
    if (vendor_id == 0x10EC and device_id == 0x8139) return "RTL8139";
    if (vendor_id == 0x10EC and device_id == 0x8168) return "RTL8168";
    if (class_code == 0x01 and subclass == 0x01) return "ATAPIO";
    if (class_code == 0x01 and subclass == 0x06) return "AHCI";
    if (class_code == 0x01 and subclass == 0x08) return "NVMe";
    if (class_code == 0x0C and subclass == 0x03) return "XHCI";
    if (class_code == 0x04 and subclass == 0x03) return "HDA";
    if (class_code == 0x06) return "platform/chipset";
    return "platform";
}

fn statusFor(binding: Binding, class_code: u8, subclass: u8, vendor_id: u16, device_id: u16) []const u8 {
    if (vendor_id == 0x10EC and (device_id == 0x8139 or device_id == 0x8168) and
        net.hasAdapterForDevice(vendor_id, device_id)) return "active-netbackend";
    if (class_code == 0x01 and subclass == 0x01 and preloadBlockCount(.ata) > 1) return "active-block-multi-preload";
    if (class_code == 0x01 and subclass == 0x01 and preloadBlockCount(.ata) > 0) return "active-block-preload";
    if (class_code == 0x01 and subclass == 0x06) {
        const preload_blocks = preloadBlockCount(.ahci);
        const total_blocks = blockCount(.ahci);
        if (preload_blocks > 1) return "active-block-multi-preload";
        if (preload_blocks > 0) return "active-block-preload";
        if (total_blocks > 1) return "active-block-multi-rescue";
        if (total_blocks > 0) return "active-block-rescue";
    }
    if (class_code == 0x01 and subclass == 0x08) {
        const preload_blocks = preloadBlockCount(.nvme);
        const total_blocks = blockCount(.nvme);
        if (preload_blocks > 1) return "active-block-multi-preload";
        if (preload_blocks > 0) return "active-block-preload";
        if (total_blocks > 1) return "active-block-multi";
        if (total_blocks > 0) return "active-block";
    }
    if (class_code == 0x0C and subclass == 0x03 and xhciHostIsPreload()) return "host-preload-active";
    if (class_code == 0x0C and subclass == 0x03 and xhciHostPresent()) return "host-rescue-active";
    return switch (binding) {
        .with_driver => "bound-or-platform-active",
        .without_driver => "detected-no-driver",
        .unknown => "unknown",
    };
}

fn noteFor(class_code: u8, subclass: u8, prog_if: u8, vendor_id: u16, device_id: u16) []const u8 {
    if (class_code == 0x01 and subclass == 0x01 and preloadBlockCount(.ata) > 1) return "Legacy IDE/ATA source=preload; ATAPIO.R4D blockdevices registered";
    if (class_code == 0x01 and subclass == 0x01 and preloadBlockCount(.ata) > 0) return "Legacy IDE/ATA source=preload; ATAPIO.R4D blockdevice registered";
    if (class_code == 0x01 and subclass == 0x06 and preloadBlockCount(.ahci) > 1) return "AHCI source=preload; AHCI.R4D storage boundary active; legacy rescue data path registered block devices";
    if (class_code == 0x01 and subclass == 0x06 and preloadBlockCount(.ahci) > 0) return "AHCI source=preload; AHCI.R4D storage boundary active; legacy rescue data path registered block device";
    if (class_code == 0x01 and subclass == 0x06 and blockCount(.ahci) > 0) return "AHCI source=legacy-rescue; block device registered outside standard owner";
    if (class_code == 0x01 and subclass == 0x06) return "AHCI candidate; standard owner is AHCI.R4D preload";
    if (class_code == 0x01 and subclass == 0x08 and preloadBlockCount(.nvme) > 1) return "NVMe source=preload; NVME.R4D active namespaces registered as read/write block devices";
    if (class_code == 0x01 and subclass == 0x08 and preloadBlockCount(.nvme) > 0) return "NVMe source=preload; NVME.R4D namespace registered as read/write block device";
    if (class_code == 0x01 and subclass == 0x08 and blockCount(.nvme) > 1) return "NVMe source=legacy-rescue; active namespaces registered as read/write block devices";
    if (class_code == 0x01 and subclass == 0x08 and blockCount(.nvme) > 0) return "NVMe source=legacy-rescue; namespace registered as read/write block device";
    if (class_code == 0x01 and subclass == 0x08) return "NVMe candidate; standard owner is NVME.R4D preload";
    if (class_code == 0x0C and subclass == 0x03 and prog_if == 0x30) {
        if (usb_host.findByName("XHCI")) |host_slot| {
            const controller = usb_host.at(host_slot);
            const is_preload = if (controller) |hc| hc.source == .preload else false;
            const usb = usb_core.summary();
            if (usb.configured != 0 and is_preload) return "xHCI source=preload; XHCI.R4D host backend registered; legacy rescue data path parsed USB configuration";
            if (is_preload) return "xHCI source=preload; XHCI.R4D host backend registered; legacy rescue data path armed";
            if (usb.configured != 0) return "xHCI source=legacy-rescue; USB configuration parsed outside standard owner";
            return "xHCI source=legacy-rescue; host backend registered outside standard owner";
        }
    }
    if (class_code == 0x0C and subclass == 0x03 and prog_if == 0x30) return "xHCI USB candidate; standard owner is XHCI.R4D preload";
    if (vendor_id == 0x10EC and device_id == 0x8139 and net.hasAdapterForDevice(vendor_id, device_id)) return "RTL8139 netbackend registered; IRQ-first RX/TX active with poll fallback";
    if (vendor_id == 0x10EC and device_id == 0x8139) return "RTL8139 candidate, planned first R4OS network backend";
    if (vendor_id == 0x10EC and device_id == 0x8168 and net.hasAdapterForDevice(vendor_id, device_id)) return "RTL8168 netbackend registered; MSI RX/TX active, warm-reboot bridge repair at bind";
    if (vendor_id == 0x10EC and device_id == 0x8168) return "RTL8168 candidate; standard owner is RTL8168.R4D";
    if (class_code == 0x02) return "network candidate, no net driver or PCI IRQ/MSI route yet";
    if (class_code == 0x03) return "bootfb active; native display driver missing";
    if (class_code == 0x0C and subclass == 0x05) return "chipset-visible, no SMBus driver";
    if (vendor_id == 0x8086 and device_id == 0x7113) return "PIIX ACPI bridge visible via PCI";
    return "";
}

const NoteWriter = struct {
    out: []u8,
    pos: usize = 0,

    fn put(self: *NoteWriter, ch: u8) void {
        if (self.pos + 1 >= self.out.len) return;
        self.out[self.pos] = ch;
        self.pos += 1;
        self.out[self.pos] = 0;
    }

    fn text(self: *NoteWriter, value: []const u8) void {
        for (value) |ch| {
            if (ch == 0) return;
            self.put(ch);
        }
    }

    fn num(self: *NoteWriter, value: u64) void {
        var buf: [20]u8 = undefined;
        var pos: usize = buf.len;
        var n = value;
        if (n == 0) {
            self.put('0');
            return;
        }
        while (n > 0) {
            pos -= 1;
            buf[pos] = '0' + @as(u8, @intCast(n % 10));
            n /= 10;
        }
        self.text(buf[pos..]);
    }

    fn ip(self: *NoteWriter, value: [4]u8) void {
        self.num(value[0]);
        self.put('.');
        self.num(value[1]);
        self.put('.');
        self.num(value[2]);
        self.put('.');
        self.num(value[3]);
    }

    fn mac(self: *NoteWriter, value: [6]u8) void {
        var index: usize = 0;
        while (index < value.len) : (index += 1) {
            if (index != 0) self.put(':');
            self.hex8(value[index]);
        }
    }

    fn hex8(self: *NoteWriter, value: u8) void {
        self.hexNibble(value >> 4);
        self.hexNibble(value & 0xF);
    }

    fn hexNibble(self: *NoteWriter, value: u8) void {
        self.put(if (value < 10) '0' + value else 'A' + value - 10);
    }

    fn slice(self: *const NoteWriter) []const u8 {
        return self.out[0..self.pos];
    }
};

fn blockDeviceMountStatus(block_index: usize) []const u8 {
    if (isBlockMountedAs(block_index, 'C')) return "mounted-C";
    if (isBlockMountedAs(block_index, 'D')) return "mounted-D";
    if (isBlockMountedAs(block_index, 'E')) return "mounted-E";
    if (hasMountedDrive(block_index)) return "mounted";
    return "active-unmounted";
}

fn blockDeviceNote(d: *const block.Device, block_index: usize) []const u8 {
    _ = block_index;
    if (d.bus == .nvme) {
        if (d.source == .preload and d.port == 1) return "NVME.R4D source=preload; namespace NSID 1; read/write blockdevice";
        if (d.source == .preload and d.port == 2) return "NVME.R4D source=preload; namespace NSID 2; read/write blockdevice";
        if (d.source == .preload and d.port == 3) return "NVME.R4D source=preload; namespace NSID 3; read/write blockdevice";
        if (d.source == .preload and d.port == 4) return "NVME.R4D source=preload; namespace NSID 4; read/write blockdevice";
        if (d.source == .preload) return "NVME.R4D source=preload; namespace read/write blockdevice";
        if (d.port == 1) return "NVMe namespace NSID 1; read/write blockdevice";
        if (d.port == 2) return "NVMe namespace NSID 2; read/write blockdevice";
        if (d.port == 3) return "NVMe namespace NSID 3; read/write blockdevice";
        if (d.port == 4) return "NVMe namespace NSID 4; read/write blockdevice";
        return "NVMe namespace; read/write blockdevice";
    }
    if (d.bus == .ata and d.source == .preload) return "ATAPIO.R4D source=preload; legacy IDE/ATA read/write blockdevice";
    if (d.driver.len == 3 and d.driver[0] == 'R' and d.driver[1] == '4' and d.driver[2] == 'D') {
        if (d.source == .preload) return "R4D StorageBackend source=preload; structured blockdevice";
        if (d.source == .disk) return "R4D StorageBackend source=disk; structured blockdevice";
        return "R4D StorageBackend source=built-in; structured blockdevice";
    }
    if (d.bus == .usb) {
        if (d.source == .preload and d.removable and d.writable) return "USBMSC source=preload; USB removable read/write blockdevice";
        if (d.source == .preload and d.removable) return "USBMSC source=preload; USB removable read-only blockdevice";
        if (d.source == .preload) return "USBMSC source=preload; USB blockdevice";
        if (d.removable and d.writable) return "USB source=built-in; removable read/write blockdevice";
        if (d.removable) return "USB source=built-in; removable read-only blockdevice";
        return "USB source=built-in; blockdevice";
    }
    if (d.bus == .ahci and d.source == .preload) return "AHCI source=preload; AHCI.R4D legacy rescue read/write blockdevice";
    if (d.bus == .ahci) return "AHCI source=built-in; port registered as read/write blockdevice";
    if (d.bus == .ata) return "Legacy IDE/ATA source=built-in; device registered as blockdevice";
    if (d.bus == .ram) return "RAM drive blockdevice";
    return "registered blockdevice";
}

fn preloadBlockCount(bus: block.Bus) usize {
    var found: usize = 0;
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        const d = block.get(index) orelse continue;
        if (d.bus == bus and d.source == .preload) found += 1;
    }
    return found;
}

fn blockCount(bus: block.Bus) usize {
    var found: usize = 0;
    var index: usize = 0;
    while (index < block.slotCount()) : (index += 1) {
        const d = block.get(index) orelse continue;
        if (d.bus == bus) found += 1;
    }
    return found;
}

fn xhciHostPresent() bool {
    return usb_host.findByName("XHCI") != null;
}

fn xhciHostIsPreload() bool {
    const index = usb_host.findByName("XHCI") orelse return false;
    const controller = usb_host.at(index) orelse return false;
    return controller.source == .preload;
}

fn hasMountedDrive(block_index: usize) bool {
    var letter: u8 = 'A';
    while (letter <= 'Z') : (letter += 1) {
        if (isBlockMountedAs(block_index, letter)) return true;
    }
    return false;
}

fn isBlockMountedAs(block_index: usize, letter: u8) bool {
    const d = drive.get(letter) orelse return false;
    return d.block_device_index != null and d.block_device_index.? == block_index;
}
