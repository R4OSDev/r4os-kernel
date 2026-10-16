const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_dhcp_reset(*NativeStats) void;
// RFC 2131: this wire value is not a finite countdown.
pub const INFINITE_SECONDS: u32 = 0xffff_ffff;

pub const CLIENT_PORT: u16 = 68;
pub const SERVER_PORT: u16 = 67;

pub const Lease = struct {
    bound: bool = false,
    xid: u32 = 0,
    offered_ip: [4]u8 = .{0} ** 4,
    server_ip: [4]u8 = .{0} ** 4,
    netmask: [4]u8 = .{ 255, 255, 255, 0 },
    gateway_ip: [4]u8 = .{0} ** 4,
    dns_ip: [4]u8 = .{0} ** 4,
    dns_configured: bool = false,
    lease_seconds: u32 = 0,
    renew_seconds: u32 = 0,
    rebind_seconds: u32 = 0,
};

pub const Stats = struct {
    discover_tx: u64 = 0,
    offer_rx: u64 = 0,
    request_tx: u64 = 0,
    ack_rx: u64 = 0,
    nak_rx: u64 = 0,
    release_tx: u64 = 0,
    retries: u64 = 0,
    timeouts: u64 = 0,
    release_errors: u64 = 0,
    malformed: u64 = 0,
    inactive_rx: u64 = 0,
    foreign_rx: u64 = 0,
    out_of_phase_rx: u64 = 0,
    self_tests: u64 = 0,
    lease: Lease = .{},
    lease_acquired_tick: u64 = 0,
    last_attempt: u8 = 0,
    last_type: u8 = 0,
    operation_pending: bool = false,
    pending_label: []const u8 = "idle",
    last_error: []const u8 = "none",
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_dhcp_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeLease = extern struct {
    bound: bool,
    xid: u32,
    offered_ip: [4]u8,
    server_ip: [4]u8,
    netmask: [4]u8,
    gateway_ip: [4]u8,
    dns_ip: [4]u8,
    dns_configured: bool,
    lease_seconds: u32,
    renew_seconds: u32,
    rebind_seconds: u32,
};
fn fromNativeLease(value: NativeLease) Lease {
    return .{ .bound = value.bound, .xid = value.xid, .offered_ip = value.offered_ip, .server_ip = value.server_ip, .netmask = value.netmask, .gateway_ip = value.gateway_ip, .dns_ip = value.dns_ip, .dns_configured = value.dns_configured, .lease_seconds = value.lease_seconds, .renew_seconds = value.renew_seconds, .rebind_seconds = value.rebind_seconds };
}

const NativeStats = extern struct {
    discover_tx: u64,
    offer_rx: u64,
    request_tx: u64,
    ack_rx: u64,
    nak_rx: u64,
    release_tx: u64,
    retries: u64,
    timeouts: u64,
    release_errors: u64,
    malformed: u64,
    inactive_rx: u64,
    foreign_rx: u64,
    out_of_phase_rx: u64,
    self_tests: u64,
    lease: NativeLease,
    lease_acquired_tick: u64,
    last_attempt: u8,
    last_type: u8,
    operation_pending: bool,
    pending_label: Text,
    last_error: Text,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .discover_tx = value.discover_tx, .offer_rx = value.offer_rx, .request_tx = value.request_tx, .ack_rx = value.ack_rx, .nak_rx = value.nak_rx, .release_tx = value.release_tx, .retries = value.retries, .timeouts = value.timeouts, .release_errors = value.release_errors, .malformed = value.malformed, .inactive_rx = value.inactive_rx, .foreign_rx = value.foreign_rx, .out_of_phase_rx = value.out_of_phase_rx, .self_tests = value.self_tests, .lease = fromNativeLease(value.lease), .lease_acquired_tick = value.lease_acquired_tick, .last_attempt = value.last_attempt, .last_type = value.last_type, .operation_pending = value.operation_pending, .pending_label = value.pending_label.slice(), .last_error = value.last_error.slice() };
}

comptime {
    if (@sizeOf(NativeLease) != 44 or @alignOf(NativeLease) != 4 or @offsetOf(NativeLease, "bound") != 0 or @offsetOf(NativeLease, "xid") != 4 or @offsetOf(NativeLease, "offered_ip") != 8 or @offsetOf(NativeLease, "server_ip") != 12 or @offsetOf(NativeLease, "netmask") != 16 or @offsetOf(NativeLease, "gateway_ip") != 20 or @offsetOf(NativeLease, "dns_ip") != 24 or @offsetOf(NativeLease, "dns_configured") != 28 or @offsetOf(NativeLease, "lease_seconds") != 32 or @offsetOf(NativeLease, "renew_seconds") != 36 or @offsetOf(NativeLease, "rebind_seconds") != 40) @compileError("Native network NativeLease layout");
}

comptime {
    if (@sizeOf(NativeStats) != 208 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "discover_tx") != 0 or @offsetOf(NativeStats, "offer_rx") != 8 or @offsetOf(NativeStats, "request_tx") != 16 or @offsetOf(NativeStats, "ack_rx") != 24 or @offsetOf(NativeStats, "nak_rx") != 32 or @offsetOf(NativeStats, "release_tx") != 40 or @offsetOf(NativeStats, "retries") != 48 or @offsetOf(NativeStats, "timeouts") != 56 or @offsetOf(NativeStats, "release_errors") != 64 or @offsetOf(NativeStats, "malformed") != 72 or @offsetOf(NativeStats, "inactive_rx") != 80 or @offsetOf(NativeStats, "foreign_rx") != 88 or @offsetOf(NativeStats, "out_of_phase_rx") != 96 or @offsetOf(NativeStats, "self_tests") != 104 or @offsetOf(NativeStats, "lease") != 112 or @offsetOf(NativeStats, "lease_acquired_tick") != 160 or @offsetOf(NativeStats, "last_attempt") != 168 or @offsetOf(NativeStats, "last_type") != 169 or @offsetOf(NativeStats, "operation_pending") != 170 or @offsetOf(NativeStats, "pending_label") != 176 or @offsetOf(NativeStats, "last_error") != 192) @compileError("Native network NativeStats layout");
}

fn toNativeLease(value: Lease) NativeLease {
    return .{ .bound = value.bound, .xid = value.xid, .offered_ip = value.offered_ip, .server_ip = value.server_ip, .netmask = value.netmask, .gateway_ip = value.gateway_ip, .dns_ip = value.dns_ip, .dns_configured = value.dns_configured, .lease_seconds = value.lease_seconds, .renew_seconds = value.renew_seconds, .rebind_seconds = value.rebind_seconds };
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .discover_tx = value.discover_tx, .offer_rx = value.offer_rx, .request_tx = value.request_tx, .ack_rx = value.ack_rx, .nak_rx = value.nak_rx, .release_tx = value.release_tx, .retries = value.retries, .timeouts = value.timeouts, .release_errors = value.release_errors, .malformed = value.malformed, .inactive_rx = value.inactive_rx, .foreign_rx = value.foreign_rx, .out_of_phase_rx = value.out_of_phase_rx, .self_tests = value.self_tests, .lease = toNativeLease(value.lease), .lease_acquired_tick = value.lease_acquired_tick, .last_attempt = value.last_attempt, .last_type = value.last_type, .operation_pending = value.operation_pending, .pending_label = .{ .pointer = value.pending_label.ptr, .len = value.pending_label.len }, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
extern fn r4os_rust_net_dhcp_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").DhcpOp) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").DhcpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_dhcp_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_dhcp_apply_build_result(*NativeStats, *const @import("r4p_contract.zig").DhcpOp) void;
pub fn applyBuildResult(stats: *Stats, op: *const @import("r4p_contract.zig").DhcpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_dhcp_apply_build_result(&value, op);
    stats.* = fromNativeStats(value);
}

pub const Expectation = extern struct {
    expected: u8 = 0,
    xid: u32 = 0,
    mac: [6]u8 = .{0} ** 6,
};
const MessageReport = extern struct {
    dispatch_failures: u64,
    reason: Text,
    action: u8,
    remaining: u8,
};
pub const MessageResult = struct { accepted: bool, dispatch_failures: u64 };
extern fn r4os_rust_dhcp_messages_reset() void;
extern fn r4os_rust_dhcp_test_injection(Text, Text) void;
extern fn r4os_rust_dhcp_expectation(*Expectation) void;
extern fn r4os_rust_dhcp_expectation_set(*const Expectation) void;
extern fn r4os_rust_dhcp_message(bool, [*]const u8, usize, *@import("r4p_contract.zig").DhcpOp, *MessageReport) void;
extern fn r4os_rust_dhcp_message_apply(*NativeStats, *const @import("r4p_contract.zig").DhcpOp, *const MessageReport) bool;
extern fn r4os_rust_dhcp_message_log(*const MessageReport) void;
pub fn resetMessages() void {
    r4os_rust_dhcp_messages_reset();
}
pub fn expectation() Expectation {
    var out: Expectation = undefined;
    r4os_rust_dhcp_expectation(&out);
    return out;
}
pub fn setExpectation(value: Expectation) void {
    r4os_rust_dhcp_expectation_set(&value);
}
pub fn configureTestInjection(offers: ?[]const u8, acks: ?[]const u8) void {
    const offer_text = offers orelse "";
    const ack_text = acks orelse "";
    r4os_rust_dhcp_test_injection(.{ .pointer = offer_text.ptr, .len = offer_text.len }, .{ .pointer = ack_text.ptr, .len = ack_text.len });
}
pub fn handleMessage(stats: *Stats, data: []const u8) MessageResult {
    var op: @import("r4p_contract.zig").DhcpOp = undefined;
    var report: MessageReport = undefined;
    r4os_rust_dhcp_message(stats.operation_pending, data.ptr, data.len, &op, &report);
    var value = toNativeStats(stats.*);
    const accepted = r4os_rust_dhcp_message_apply(&value, &op, &report);
    stats.* = fromNativeStats(value);
    r4os_rust_dhcp_message_log(&report);
    return .{ .accepted = accepted, .dispatch_failures = report.dispatch_failures };
}

comptime {
    if (@sizeOf(Expectation) != 16 or @alignOf(Expectation) != 4 or @offsetOf(Expectation, "expected") != 0 or @offsetOf(Expectation, "xid") != 4 or @offsetOf(Expectation, "mac") != 8) @compileError("Native network Expectation layout");
}

comptime {
    if (@sizeOf(MessageReport) != 32 or @alignOf(MessageReport) != 8 or @offsetOf(MessageReport, "dispatch_failures") != 0 or @offsetOf(MessageReport, "reason") != 8 or @offsetOf(MessageReport, "action") != 24 or @offsetOf(MessageReport, "remaining") != 25) @compileError("Native network MessageReport layout");
}

extern fn r4os_rust_dhcp_client_stats(*NativeStats) void;
extern fn r4os_rust_dhcp_client_set_pending(bool, ?*const Text) void;
pub fn ownedStats() Stats {
    var value: NativeStats = undefined;
    r4os_rust_dhcp_client_stats(&value);
    return fromNativeStats(value);
}
pub fn setOwnedPending(pending: bool, label: ?[]const u8) void {
    var value: Text = undefined;
    if (label) |v| value = .{ .pointer = v.ptr, .len = v.len };
    r4os_rust_dhcp_client_set_pending(pending, if (label != null) &value else null);
}
