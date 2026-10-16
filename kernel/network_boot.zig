// Sole native network boot state/sequencing; peers retain their own owners.
const boot_status = @import("boot_status.zig");
const boot_config = @import("boot_config.zig");
const ipc = @import("ipc.zig");
const core = @import("../net/core.zig");
const services = @import("../net/ipc_services.zig");
const Text = @import("../driver/usb/core.zig").Text;
pub const Phase = enum(u8) {
    not_initialized,
    contract_ready,
    core_ready,
    boot_config_applied,
    serial_link_ready,
    ipc_ready,
    net_ipc_services_ready,
    failed,
};

pub const Status = struct {
    phase: Phase = .not_initialized,
    contract_ready: bool = false,
    core_ready: bool = false,
    boot_config_applied: bool = false,
    serial_link_ready: bool = false,
    ipc_ready: bool = false,
    net_ipc_services_ready: bool = false,
    adapter_count: usize = 0,
    serial_link_present: bool = false,
    config_invalid_options: u64 = 0,
    config_source: []const u8 = "none",
    failed: bool = false,
    reason: []const u8 = "not initialized",
};

const NativeStatus = extern struct {
    phase: u8,
    contract_ready: bool,
    core_ready: bool,
    boot_config_applied: bool,
    serial_link_ready: bool,
    ipc_ready: bool,
    net_ipc_services_ready: bool,
    adapter_count: usize,
    serial_link_present: bool,
    config_invalid_options: u64,
    config_source: Text,
    failed: bool,
    reason: Text,
};
const Peers = extern struct {
    core_init: *const fn () callconv(.c) void,
    core_count: *const fn () callconv(.c) usize,
    core_configure: *const fn (*const boot_config.Config) callconv(.c) void,
    ipc_init: *const fn () callconv(.c) void,
    services_init: *const fn () callconv(.c) void,
    status_line: *const fn ([*]const u8, usize) callconv(.c) void,
};
const peers = Peers{ .core_init = coreInit, .core_count = coreCount, .core_configure = coreConfigure, .ipc_init = ipcInit, .services_init = servicesInit, .status_line = statusLine };
fn coreInit() callconv(.c) void {
    core.init();
}
fn coreCount() callconv(.c) usize {
    return core.count();
}
fn coreConfigure(config: *const boot_config.Config) callconv(.c) void {
    core.configureDhcpTestInjection(config);
}
fn ipcInit() callconv(.c) void {
    ipc.init();
}
fn servicesInit() callconv(.c) void {
    services.init();
}
fn statusLine(text: [*]const u8, len: usize) callconv(.c) void {
    boot_status.statusLine(text[0..len]);
}
extern fn r4os_rust_network_boot_prepare(*const Peers) bool;
extern fn r4os_rust_network_boot_init(*const Peers) bool;
extern fn r4os_rust_network_boot_ready() bool;
extern fn r4os_rust_network_boot_contract() bool;
extern fn r4os_rust_network_boot_dump() void;
extern fn r4os_rust_network_boot_status(*NativeStatus) void;
extern fn r4os_rust_network_boot_phase(u8, *Text) void;
pub fn prepareContract() bool {
    return r4os_rust_network_boot_prepare(&peers);
}
pub fn init() bool {
    return r4os_rust_network_boot_init(&peers);
}
pub fn isInitialized() bool {
    return r4os_rust_network_boot_ready();
}
pub fn isContractReady() bool {
    return r4os_rust_network_boot_contract();
}
pub fn dumpStatus() void {
    r4os_rust_network_boot_dump();
}
pub fn phaseName(phase: Phase) []const u8 {
    var out: Text = undefined;
    r4os_rust_network_boot_phase(@intFromEnum(phase), &out);
    return out.slice();
}
pub fn status() Status {
    var value: NativeStatus = undefined;
    r4os_rust_network_boot_status(&value);
    return .{
        .phase = @enumFromInt(value.phase),
        .contract_ready = value.contract_ready,
        .core_ready = value.core_ready,
        .boot_config_applied = value.boot_config_applied,
        .serial_link_ready = value.serial_link_ready,
        .ipc_ready = value.ipc_ready,
        .net_ipc_services_ready = value.net_ipc_services_ready,
        .adapter_count = value.adapter_count,
        .serial_link_present = value.serial_link_present,
        .config_invalid_options = value.config_invalid_options,
        .config_source = value.config_source.slice(),
        .failed = value.failed,
        .reason = value.reason.slice(),
    };
}

comptime {
    if (@sizeOf(NativeStatus) != 72 or @alignOf(NativeStatus) != 8 or @offsetOf(NativeStatus, "phase") != 0 or @offsetOf(NativeStatus, "contract_ready") != 1 or @offsetOf(NativeStatus, "core_ready") != 2 or @offsetOf(NativeStatus, "boot_config_applied") != 3 or @offsetOf(NativeStatus, "serial_link_ready") != 4 or @offsetOf(NativeStatus, "ipc_ready") != 5 or @offsetOf(NativeStatus, "net_ipc_services_ready") != 6 or @offsetOf(NativeStatus, "adapter_count") != 8 or @offsetOf(NativeStatus, "serial_link_present") != 16 or @offsetOf(NativeStatus, "config_invalid_options") != 24 or @offsetOf(NativeStatus, "config_source") != 32 or @offsetOf(NativeStatus, "failed") != 48 or @offsetOf(NativeStatus, "reason") != 56) @compileError("Native network NativeStatus layout");
}
