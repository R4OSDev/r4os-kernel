// Private metadata transport to the native inventory owner. Classification,
// bounded notes, record collection and Contract output are implemented in Rust.
const api = @import("../program/r4x_api.zig");
const block = @import("../storage/block.zig");
const display = @import("../display/display.zig");
const drive = @import("../fs/drive.zig");
const usb_core = @import("../driver/usb/core.zig");
const usb_hid = @import("../driver/usb/hid.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const usb_host = @import("../driver/usb/host_controller.zig");
const driver_registry = @import("../driver/registry.zig");
const protocol_registry = @import("../protocol/registry.zig");
const net = @import("../net/core.zig");
const net_config = @import("../net/config.zig");
const Text = extern struct { pointer: ?[*]const u8 = null, bytes: u64 = 0 };
fn text(value: []const u8) Text {
    return .{ .pointer = value.ptr, .bytes = value.len };
}
const DisplayInput = extern struct { name: Text, policy: Text };
const BlockInput = extern struct {
    name: Text,
    driver: Text,
    bus: u8,
    source: u8,
    port: u8,
    removable: bool,
    writable: bool,
};
const UsbInput = extern struct {
    configured: bool,
    vendor_id: u16,
    product_id: u16,
    class: u8,
    subclass: u8,
    protocol: u8,
    interface_class: u8,
    interface_subclass: u8,
    interface_protocol: u8,
};
const HidInput = extern struct { keyboard: bool, mouse: bool, missing: u32, source: Text };
const MscInput = extern struct { registered: bool, write_protected: bool, external_owner: bool, missing: u64, bot: Text, scsi: Text };
const DriverInput = extern struct { kind: u16, name: [32]u8, name_len: u64, source: Text, state: Text, type_name: Text };
comptime {
    if (@sizeOf(DriverInput) != 96 or @offsetOf(DriverInput, "name_len") != 40)
        @compileError("Native driver inventory copy layout drift");
}
const ProtocolInput = extern struct { category: u16, role: [64]u8, role_len: u64, source: Text, state: Text, note: [96]u8, note_len: u64 };
const NetInput = extern struct {
    name: Text,
    driver: Text,
    link: Text,
    last_error: Text,
    config_source: Text,
    rx: u64,
    tx: u64,
    errors: u64,
    mac: [6]u8,
    ip: [4]u8,
    gateway: [4]u8,
    bus: u8,
    bus_no: u8,
    device_no: u8,
    function_no: u8,
    vendor_id: u16,
    device_id: u16,
};
const XhciInput = extern struct {
    present: bool = false,
    exists: bool = false,
    status_ok: bool = false,
    source: u8 = 0,
    state: u8 = 0,
    source_label: Text = .{},
    flags: u32 = 0,
    queue_depth: u16 = 0,
    max_transfer_bytes: u32 = 0,
    active_transfers: u32 = 0,
    completions: u64 = 0,
    interrupts: u64 = 0,
    polls: u64 = 0,
    cancellations: u64 = 0,
};
pub const Counts = extern struct { drivers: u32 = 0, protocols: u32 = 0, displays: u32 = 0 };
extern fn r4os_rust_device_inventory_summary(out: *api.DeviceInventorySummary) i32;
extern fn r4os_rust_device_inventory_record(index: u32, out: *api.DeviceInventoryRecord) i32;
extern fn r4os_rust_device_inventory_counts(out: *Counts) void;
pub fn summary(out: *api.DeviceInventorySummary) i32 {
    return r4os_rust_device_inventory_summary(out);
}
pub fn record(index: u32, out: *api.DeviceInventoryRecord) i32 {
    return r4os_rust_device_inventory_record(index, out);
}
pub fn counts() Counts {
    var out: Counts = undefined;
    r4os_rust_device_inventory_counts(&out);
    return out;
}

export fn r4os_zig_inventory_scalar(operation: u8, argument: u64) callconv(.c) u64 {
    if (@import("builtin").is_test) @panic("Device metadata requested by host fixture");
    return switch (operation) {
        0 => block.count(),
        1 => block.slotCount(),
        2 => @intFromBool(drive.get(@intCast(argument)) != null),
        3 => blk: {
            const mounted = drive.get(@truncate(argument)) orelse break :blk 0;
            break :blk @intFromBool(mounted.block_device_index != null and mounted.block_device_index.? == (argument >> 8));
        },
        4 => @intFromBool(block.isBusVisible(.usb)),
        5 => driver_registry.MAX_DRIVERS,
        6 => protocol_registry.MAX_PROTOCOLS,
        7 => @intFromBool(net.hasAdapterForDevice(@truncate(argument >> 16), @truncate(argument))),
        8 => net.count(),
        else => 0,
    };
}
export fn r4os_zig_inventory_display(out: *DisplayInput) callconv(.c) void {
    if (@import("builtin").is_test) @panic("Display metadata requested by host fixture");
    const value = display.stats();
    out.* = .{ .name = text(value.name), .policy = text(display.cachePolicyName(value.mapping.cache_policy)) };
}
export fn r4os_zig_inventory_block(index: u64, out: *BlockInput) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("Block metadata requested by host fixture");
    const value = block.get(@intCast(index)) orelse return false;
    out.* = .{ .name = text(value.name), .driver = text(value.driver), .bus = @intFromEnum(value.bus), .source = @intFromEnum(value.source), .port = value.port, .removable = value.removable, .writable = value.writable };
    return true;
}
export fn r4os_zig_inventory_usb(index: u64, out: *UsbInput) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("USB metadata requested by host fixture");
    const value = usb_core.deviceAt(@intCast(index)) orelse return false;
    out.* = .{ .configured = value.configured, .vendor_id = value.vendor_id, .product_id = value.product_id, .class = value.device_class, .subclass = value.device_subclass, .protocol = value.device_protocol, .interface_class = value.first_interface_class, .interface_subclass = value.first_interface_subclass, .interface_protocol = value.first_interface_protocol };
    return true;
}
export fn r4os_zig_inventory_hid(out: *HidInput) callconv(.c) void {
    if (@import("builtin").is_test) @panic("HID metadata requested by host fixture");
    const value = usb_hid.status();
    out.* = .{ .keyboard = value.keyboard_bound, .mouse = value.mouse_bound, .missing = value.protocol_required_missing, .source = text(value.boot_source) };
}
export fn r4os_zig_inventory_msc(out: *MscInput) callconv(.c) void {
    if (@import("builtin").is_test) @panic("MSC metadata requested by host fixture");
    const value = usb_msc.status();
    out.* = .{ .registered = value.block_registered, .write_protected = value.write_protected, .external_owner = usb_msc.hasExternalOwner(), .missing = value.protocol_required_missing, .bot = text(value.bot_source), .scsi = text(value.scsi_source) };
}
export fn r4os_zig_inventory_driver(index: u64, out: *DriverInput) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("Driver metadata requested by host fixture");
    const value = driver_registry.entryAt(@intCast(index)) orelse return false;
    out.* = .{ .kind = value.driver_type, .name = value.name, .name_len = value.name_len, .source = text(driver_registry.sourceName(value.source)), .state = text(driver_registry.stateName(value.state)), .type_name = text(driver_registry.typeName(value.driver_type)) };
    return true;
}
export fn r4os_zig_inventory_protocol(index: u64, out: *ProtocolInput) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("Protocol metadata requested by host fixture");
    const value = protocol_registry.entryAt(@intCast(index)) orelse return false;
    out.* = .{ .category = value.category, .role = value.role, .role_len = value.role_len, .source = text(protocol_registry.sourceName(value.source)), .state = text(protocol_registry.stateName(value.state)), .note = value.note, .note_len = value.note_len };
    return true;
}
export fn r4os_zig_inventory_net(index: u64, out: *NetInput) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("Network metadata requested by host fixture");
    const value = net.get(@intCast(index)) orelse return false;
    out.* = .{ .name = text(value.name), .driver = text(value.driver), .link = text(net.linkName(value.link)), .last_error = text(value.stats.last_error), .config_source = text(net_config.sourceName()), .rx = value.stats.rx_packets, .tx = value.stats.tx_packets, .errors = value.stats.errors, .mac = value.mac, .ip = net_config.localIp(), .gateway = net_config.gatewayIp(), .bus = @intFromEnum(value.bus), .bus_no = value.bus_no, .device_no = value.device_no, .function_no = value.function_no, .vendor_id = value.vendor_id, .device_id = value.device_id };
    return true;
}
export fn r4os_zig_inventory_xhci(with_status: bool, out: *XhciInput) callconv(.c) void {
    if (@import("builtin").is_test) @panic("xHCI metadata requested by host fixture");
    out.* = .{};
    const index = usb_host.findByName("XHCI") orelse return;
    out.present = true;
    const value = usb_host.at(index) orelse return;
    out.exists = true;
    out.source = @intFromEnum(value.source);
    out.state = @intFromEnum(value.state);
    out.source_label = text(usb_host.sourceLabel(value.source));
    if (!with_status) return;
    var status: usb_host.Status = .{};
    out.status_ok = usb_host.statusAt(index, &status) == 0;
    out.flags = status.flags;
    out.queue_depth = status.queue_depth;
    out.max_transfer_bytes = status.max_transfer_bytes;
    out.active_transfers = status.active_transfers;
    out.completions = status.completions;
    out.interrupts = status.interrupts;
    out.polls = status.polls;
    out.cancellations = status.cancellations;
}
comptime {
    if (@sizeOf(Text) != 16 or @sizeOf(DisplayInput) != 32 or @sizeOf(BlockInput) != 40 or @sizeOf(UsbInput) != 12 or
        @sizeOf(HidInput) != 24 or @sizeOf(MscInput) != 48 or @sizeOf(DriverInput) != 96 or @sizeOf(ProtocolInput) != 216 or
        @sizeOf(NetInput) != 128 or @sizeOf(XhciInput) != 72 or @sizeOf(Counts) != 12)
        @compileError("Rust inventory transport layout differs");
    if (@sizeOf(api.DeviceInventorySummary) != 20 or @sizeOf(api.DeviceInventoryRecord) != 224)
        @compileError("Rust inventory public Contract layout differs");
    if (net.MAX_ADAPTERS != 8 or @intFromEnum(block.Bus.ata) != 1 or @intFromEnum(block.Bus.ahci) != 2 or
        @intFromEnum(block.Bus.nvme) != 3 or @intFromEnum(block.Bus.usb) != 4 or @intFromEnum(block.Bus.ram) != 5 or
        @intFromEnum(block.Source.preload) != 1 or @intFromEnum(block.Source.disk) != 2 or
        @intFromEnum(net.Bus.pci) != 1 or @intFromEnum(net.Bus.pcie) != 2 or
        @intFromEnum(usb_host.State.active) != 1 or @intFromEnum(usb_host.Source.preload) != 1 or
        (usb_host.FLAG_PORT_SCAN | usb_host.FLAG_CONTROL | usb_host.FLAG_BULK | usb_host.FLAG_INTERRUPT) != 15 or usb_host.FLAG_EVENT_IRQ != 16)
        @compileError("Rust inventory metadata values differ");
}
