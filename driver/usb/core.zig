pub const Text = extern struct {
    pointer: [*]const u8,
    len: usize,
    pub fn init(value: []const u8) Text {
        return .{ .pointer = value.ptr, .len = value.len };
    }
    pub fn slice(self: Text) []const u8 {
        return self.pointer[0..self.len];
    }
};
pub const MAX_USB_DEVICES: usize = 8;
pub const MAX_USB_CONFIGS: usize = 1;
pub const MAX_USB_INTERFACES: usize = 8;
pub const MAX_USB_ENDPOINTS: usize = 16;
pub const STRING_LEN: usize = 32;

pub const Interface = extern struct {
    active: bool = false,
    number: u8 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    endpoint_count: u8 = 0,
    first_endpoint_address: u8 = 0,
    first_endpoint_attributes: u8 = 0,
    first_endpoint_max_packet: u16 = 0,
    first_endpoint_interval: u8 = 0,
    interrupt_in_endpoint_address: u8 = 0,
    interrupt_in_endpoint_max_packet: u16 = 0,
    interrupt_in_endpoint_interval: u8 = 0,
    bulk_in_endpoint_address: u8 = 0,
    bulk_in_endpoint_max_burst: u8 = 0,
    bulk_out_endpoint_address: u8 = 0,
    bulk_out_endpoint_max_burst: u8 = 0,
    hid_descriptor_len: u8 = 0,
    hid_report_descriptor_len: u16 = 0,
};

pub const Device = extern struct {
    active: bool = false,
    configured: bool = false,
    controller: Text = Text.init("unknown"),
    slot_id: u8 = 0,
    port: u8 = 0,
    speed: u8 = 0,
    usb_version_bcd: u16 = 0,
    vendor_id: u16 = 0,
    product_id: u16 = 0,
    device_version_bcd: u16 = 0,
    device_class: u8 = 0,
    device_subclass: u8 = 0,
    device_protocol: u8 = 0,
    config_value: u8 = 0,
    config_attributes: u8 = 0,
    config_max_power_ma: u16 = 0,
    interface_count: u8 = 0,
    interface_record_count: u8 = 0,
    interfaces: [MAX_USB_INTERFACES]Interface = .{Interface{}} ** MAX_USB_INTERFACES,
    endpoint_count: u8 = 0,
    first_interface_number: u8 = 0,
    first_interface_class: u8 = 0,
    first_interface_subclass: u8 = 0,
    first_interface_protocol: u8 = 0,
    first_endpoint_address: u8 = 0,
    first_endpoint_attributes: u8 = 0,
    first_endpoint_max_packet: u16 = 0,
    first_endpoint_interval: u8 = 0,
    hid_descriptor_len: u8 = 0,
    hid_report_descriptor_len: u16 = 0,
    bulk_in_endpoint_address: u8 = 0,
    bulk_in_endpoint_max_packet: u16 = 0,
    bulk_in_endpoint_max_burst: u8 = 0,
    bulk_out_endpoint_address: u8 = 0,
    bulk_out_endpoint_max_packet: u16 = 0,
    bulk_out_endpoint_max_burst: u8 = 0,
    string_language_id: u16 = 0,
    manufacturer_len: u8 = 0,
    product_len: u8 = 0,
    serial_len: u8 = 0,
    manufacturer_string: [STRING_LEN]u8 = .{0} ** STRING_LEN,
    product_string: [STRING_LEN]u8 = .{0} ** STRING_LEN,
    serial_string: [STRING_LEN]u8 = .{0} ** STRING_LEN,
};

pub const Summary = extern struct {
    count: u8 = 0,
    configured: u8 = 0,
    truncated: bool = false,
};

comptime {
    if (@sizeOf(Interface) != 22 or
        @alignOf(Interface) != 2 or
        @offsetOf(Interface, "active") != 0 or
        @offsetOf(Interface, "number") != 1 or
        @offsetOf(Interface, "class_code") != 2 or
        @offsetOf(Interface, "subclass") != 3 or
        @offsetOf(Interface, "protocol") != 4 or
        @offsetOf(Interface, "endpoint_count") != 5 or
        @offsetOf(Interface, "first_endpoint_address") != 6 or
        @offsetOf(Interface, "first_endpoint_attributes") != 7 or
        @offsetOf(Interface, "first_endpoint_max_packet") != 8 or
        @offsetOf(Interface, "first_endpoint_interval") != 10 or
        @offsetOf(Interface, "interrupt_in_endpoint_address") != 11 or
        @offsetOf(Interface, "interrupt_in_endpoint_max_packet") != 12 or
        @offsetOf(Interface, "interrupt_in_endpoint_interval") != 14 or
        @offsetOf(Interface, "bulk_in_endpoint_address") != 15 or
        @offsetOf(Interface, "bulk_in_endpoint_max_burst") != 16 or
        @offsetOf(Interface, "bulk_out_endpoint_address") != 17 or
        @offsetOf(Interface, "bulk_out_endpoint_max_burst") != 18 or
        @offsetOf(Interface, "hid_descriptor_len") != 19 or
        @offsetOf(Interface, "hid_report_descriptor_len") != 20) @compileError("USB Interface private C layout");
}

comptime {
    if (@sizeOf(Device) != 352 or
        @alignOf(Device) != 8 or
        @offsetOf(Device, "active") != 0 or
        @offsetOf(Device, "configured") != 1 or
        @offsetOf(Device, "controller") != 8 or
        @offsetOf(Device, "slot_id") != 24 or
        @offsetOf(Device, "port") != 25 or
        @offsetOf(Device, "speed") != 26 or
        @offsetOf(Device, "usb_version_bcd") != 28 or
        @offsetOf(Device, "vendor_id") != 30 or
        @offsetOf(Device, "product_id") != 32 or
        @offsetOf(Device, "device_version_bcd") != 34 or
        @offsetOf(Device, "device_class") != 36 or
        @offsetOf(Device, "device_subclass") != 37 or
        @offsetOf(Device, "device_protocol") != 38 or
        @offsetOf(Device, "config_value") != 39 or
        @offsetOf(Device, "config_attributes") != 40 or
        @offsetOf(Device, "config_max_power_ma") != 42 or
        @offsetOf(Device, "interface_count") != 44 or
        @offsetOf(Device, "interface_record_count") != 45 or
        @offsetOf(Device, "interfaces") != 46 or
        @offsetOf(Device, "endpoint_count") != 222 or
        @offsetOf(Device, "first_interface_number") != 223 or
        @offsetOf(Device, "first_interface_class") != 224 or
        @offsetOf(Device, "first_interface_subclass") != 225 or
        @offsetOf(Device, "first_interface_protocol") != 226 or
        @offsetOf(Device, "first_endpoint_address") != 227 or
        @offsetOf(Device, "first_endpoint_attributes") != 228 or
        @offsetOf(Device, "first_endpoint_max_packet") != 230 or
        @offsetOf(Device, "first_endpoint_interval") != 232 or
        @offsetOf(Device, "hid_descriptor_len") != 233 or
        @offsetOf(Device, "hid_report_descriptor_len") != 234 or
        @offsetOf(Device, "bulk_in_endpoint_address") != 236 or
        @offsetOf(Device, "bulk_in_endpoint_max_packet") != 238 or
        @offsetOf(Device, "bulk_in_endpoint_max_burst") != 240 or
        @offsetOf(Device, "bulk_out_endpoint_address") != 241 or
        @offsetOf(Device, "bulk_out_endpoint_max_packet") != 242 or
        @offsetOf(Device, "bulk_out_endpoint_max_burst") != 244 or
        @offsetOf(Device, "string_language_id") != 246 or
        @offsetOf(Device, "manufacturer_len") != 248 or
        @offsetOf(Device, "product_len") != 249 or
        @offsetOf(Device, "serial_len") != 250 or
        @offsetOf(Device, "manufacturer_string") != 251 or
        @offsetOf(Device, "product_string") != 283 or
        @offsetOf(Device, "serial_string") != 315) @compileError("USB Device private C layout");
}

comptime {
    if (@sizeOf(Summary) != 3 or
        @alignOf(Summary) != 1 or
        @offsetOf(Summary, "count") != 0 or
        @offsetOf(Summary, "configured") != 1 or
        @offsetOf(Summary, "truncated") != 2) @compileError("USB Summary private C layout");
}

extern fn r4os_rust_usb_core_reset() void;
extern fn r4os_rust_usb_core_first(*const Device) void;
extern fn r4os_rust_usb_core_publish(*const Device) usize;
extern fn r4os_rust_usb_core_replace(*const Device) usize;
extern fn r4os_rust_usb_core_remove([*]const u8, usize, u8) bool;
extern fn r4os_rust_usb_core_count() usize;
extern fn r4os_rust_usb_core_at(usize, *Device) bool;
extern fn r4os_rust_usb_core_summary(*Summary) void;
pub fn reset() void {
    r4os_rust_usb_core_reset();
}
pub fn publishFirstDevice(device: Device) void {
    r4os_rust_usb_core_first(&device);
}
pub fn publishDevice(device: Device) ?usize {
    const index = r4os_rust_usb_core_publish(&device);
    return if (index == @import("std").math.maxInt(usize)) null else index;
}
pub fn publishOrReplaceByPort(device: Device) ?usize {
    const index = r4os_rust_usb_core_replace(&device);
    return if (index == @import("std").math.maxInt(usize)) null else index;
}
pub fn removeByPort(controller: []const u8, port: u8) bool {
    return r4os_rust_usb_core_remove(controller.ptr, controller.len, port);
}
pub fn count() usize {
    return r4os_rust_usb_core_count();
}
pub fn deviceAt(index: usize) ?Device {
    var out: Device = undefined;
    return if (r4os_rust_usb_core_at(index, &out)) out else null;
}
pub fn summary() Summary {
    var out: Summary = undefined;
    r4os_rust_usb_core_summary(&out);
    return out;
}
