// USB HID boot boundary for boot-adjacent keyboard and mouse bindings.

const Text = @import("../driver/usb/core.zig").Text;
const storage_boot = @import("storage_boot.zig");
const usb_hid = @import("../driver/usb/hid.zig");
const xhci = @import("../driver/usb/xhci.zig");

pub const Phase = enum(u8) {
    not_started,
    dependency_missing,
    no_hid,
    keyboard_bound,
    mouse_bound,
    keyboard_mouse_bound,
    warning,
    failed,
};

pub const RegistryState = enum(u8) {
    none,
    loaded,
    initialized,
    active,
    failed,
};

pub const Status = struct {
    phase: Phase = .not_started,
    attempted: bool = false,
    initialized: bool = false,
    init_returned_bound: bool = false,
    active: bool = false,
    failed: bool = false,
    fatal: bool = false,
    registry_slot_present: bool = false,
    registry_state: RegistryState = .none,
    keyboard_present: bool = false,
    keyboard_bound: bool = false,
    mouse_present: bool = false,
    mouse_bound: bool = false,
    unsupported_hid: u8 = 0,
    missing_endpoint_hid: u8 = 0,
    setup_warnings: u8 = 0,
    failures: u64 = 0,
    boot_dispatch_failures: u64 = 0,
    protocol_required_missing: u8 = 0,
    report_descriptor_malformed: bool = false,
    boot_source: []const u8 = "none",
    reason: []const u8 = "not started",
};

const RawStatus = extern struct {
    phase: u8 = 0,
    attempted: bool = false,
    initialized: bool = false,
    init_returned_bound: bool = false,
    active: bool = false,
    failed: bool = false,
    fatal: bool = false,
    registry_slot_present: bool = false,
    registry_state: u8 = 0,
    keyboard_present: bool = false,
    keyboard_bound: bool = false,
    mouse_present: bool = false,
    mouse_bound: bool = false,
    unsupported_hid: u8 = 0,
    missing_endpoint_hid: u8 = 0,
    setup_warnings: u8 = 0,
    failures: u64 = 0,
    boot_dispatch_failures: u64 = 0,
    protocol_required_missing: u8 = 0,
    report_descriptor_malformed: bool = false,
    boot_source: Text = Text.init("none"),
    reason: Text = Text.init("not started"),
};
const XhciStatus = extern struct { interrupt_recoveries: u64, interrupt_pending_timeouts: u64, interrupt_halted_checks: u64 };
const Peers = extern struct {
    storage_ready: *const fn () callconv(.c) bool,
    hid_init: *const fn () callconv(.c) bool,
    hid_status: *const fn (*RawStatus) callconv(.c) void,
    xhci_status: *const fn (*XhciStatus) callconv(.c) void,
    port_start: *const fn () callconv(.c) bool,
    hid_start: *const fn () callconv(.c) bool,
};
const peers: Peers = .{ .storage_ready = storageReady, .hid_init = hidInit, .hid_status = hidStatus, .xhci_status = xhciStatus, .port_start = portStart, .hid_start = hidStart };
extern fn r4os_rust_usb_hid_boot_init(*const Peers) bool;
extern fn r4os_rust_usb_hid_boot_initialized() bool;
extern fn r4os_rust_usb_hid_boot_active() bool;
extern fn r4os_rust_usb_hid_boot_status(*RawStatus) void;
extern fn r4os_rust_usb_hid_boot_phase(u8) Text;
extern fn r4os_rust_usb_hid_boot_registry(u8) Text;
extern fn r4os_rust_usb_hid_boot_dump() void;
extern fn r4os_rust_usb_hid_boot_start(*const Peers) bool;
pub fn init() bool {
    return r4os_rust_usb_hid_boot_init(&peers);
}
pub fn isInitialized() bool {
    return r4os_rust_usb_hid_boot_initialized();
}
pub fn isActive() bool {
    return r4os_rust_usb_hid_boot_active();
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_usb_hid_boot_status(&raw);
    var out: Status = undefined;
    inline for (@import("std").meta.fields(Status)) |field| {
        if (comptime field.type == []const u8) @field(out, field.name) = @field(raw, field.name).slice() else if (comptime field.type == Phase or field.type == RegistryState) @field(out, field.name) = @enumFromInt(@field(raw, field.name)) else @field(out, field.name) = @field(raw, field.name);
    }
    return out;
}
pub fn phaseName(phase: Phase) []const u8 {
    return r4os_rust_usb_hid_boot_phase(@intFromEnum(phase)).slice();
}
pub fn registryStateName(state: RegistryState) []const u8 {
    return r4os_rust_usb_hid_boot_registry(@intFromEnum(state)).slice();
}
pub fn dumpStatus() void {
    r4os_rust_usb_hid_boot_dump();
}
pub fn startPollTask() bool {
    return r4os_rust_usb_hid_boot_start(&peers);
}
fn storageReady() callconv(.c) bool {
    return storage_boot.isControllersInitialized();
}
fn hidInit() callconv(.c) bool {
    return usb_hid.init();
}
fn portStart() callconv(.c) bool {
    return xhci.startPortTask();
}
fn hidStart() callconv(.c) bool {
    return usb_hid.startPollTask();
}
fn xhciStatus(out: *XhciStatus) callconv(.c) void {
    const value = xhci.status();
    out.* = .{ .interrupt_recoveries = value.interrupt_recoveries, .interrupt_pending_timeouts = value.interrupt_pending_timeouts, .interrupt_halted_checks = value.interrupt_halted_checks };
}
fn hidStatus(out: *RawStatus) callconv(.c) void {
    const value = usb_hid.status();
    out.* = .{};
    out.initialized = value.initialized;
    out.keyboard_present = value.keyboard_present;
    out.keyboard_bound = value.keyboard_bound;
    out.mouse_present = value.mouse_present;
    out.mouse_bound = value.mouse_bound;
    out.unsupported_hid = value.unsupported_hid;
    out.missing_endpoint_hid = value.missing_endpoint_hid;
    out.setup_warnings = value.setup_warnings;
    out.failures = value.failures;
    out.boot_dispatch_failures = value.boot_dispatch_failures;
    out.protocol_required_missing = value.protocol_required_missing;
    out.report_descriptor_malformed = value.report_descriptor_malformed;
    out.boot_source = Text.init(value.boot_source);
    out.reason = Text.init(value.reason);
}
comptime {
    if (@sizeOf(RawStatus) != 72 or
        @offsetOf(RawStatus, "phase") != 0 or
        @offsetOf(RawStatus, "attempted") != 1 or
        @offsetOf(RawStatus, "initialized") != 2 or
        @offsetOf(RawStatus, "init_returned_bound") != 3 or
        @offsetOf(RawStatus, "active") != 4 or
        @offsetOf(RawStatus, "failed") != 5 or
        @offsetOf(RawStatus, "fatal") != 6 or
        @offsetOf(RawStatus, "registry_slot_present") != 7 or
        @offsetOf(RawStatus, "registry_state") != 8 or
        @offsetOf(RawStatus, "keyboard_present") != 9 or
        @offsetOf(RawStatus, "keyboard_bound") != 10 or
        @offsetOf(RawStatus, "mouse_present") != 11 or
        @offsetOf(RawStatus, "mouse_bound") != 12 or
        @offsetOf(RawStatus, "unsupported_hid") != 13 or
        @offsetOf(RawStatus, "missing_endpoint_hid") != 14 or
        @offsetOf(RawStatus, "setup_warnings") != 15 or
        @offsetOf(RawStatus, "failures") != 16 or
        @offsetOf(RawStatus, "boot_dispatch_failures") != 24 or
        @offsetOf(RawStatus, "protocol_required_missing") != 32 or
        @offsetOf(RawStatus, "report_descriptor_malformed") != 33 or
        @offsetOf(RawStatus, "boot_source") != 40 or
        @offsetOf(RawStatus, "reason") != 56) @compileError("USB HID boot private C layout");
    if (@sizeOf(Peers) != 48 or @sizeOf(XhciStatus) != 24) @compileError("USB HID boot peers");
}
