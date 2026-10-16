const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_udp_reset(*NativeStats) void;
pub const IPV4_PROTOCOL: u8 = 17;
pub const HEADER_SIZE: usize = 8;

pub const DatagramView = struct {
    source_ip: [4]u8,
    dest_ip: [4]u8,
    source_port: u16,
    dest_port: u16,
    length: u16,
    payload: []const u8,
};

pub const Stats = struct {
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    dropped_short: u64 = 0,
    dropped_length: u64 = 0,
    checksum_errors: u64 = 0,
    malformed: u64 = 0,
    dhcp_rx: u64 = 0,
    dns_rx: u64 = 0,
    self_tests: u64 = 0,
    last_source_port: u16 = 0,
    last_dest_port: u16 = 0,
    last_error: []const u8 = "none",
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_udp_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeStats = extern struct {
    rx_packets: u64,
    tx_packets: u64,
    dropped_short: u64,
    dropped_length: u64,
    checksum_errors: u64,
    malformed: u64,
    dhcp_rx: u64,
    dns_rx: u64,
    self_tests: u64,
    last_source_port: u16,
    last_dest_port: u16,
    last_error: Text,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .dropped_short = value.dropped_short, .dropped_length = value.dropped_length, .checksum_errors = value.checksum_errors, .malformed = value.malformed, .dhcp_rx = value.dhcp_rx, .dns_rx = value.dns_rx, .self_tests = value.self_tests, .last_source_port = value.last_source_port, .last_dest_port = value.last_dest_port, .last_error = value.last_error.slice() };
}

comptime {
    if (@sizeOf(NativeStats) != 96 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "rx_packets") != 0 or @offsetOf(NativeStats, "tx_packets") != 8 or @offsetOf(NativeStats, "dropped_short") != 16 or @offsetOf(NativeStats, "dropped_length") != 24 or @offsetOf(NativeStats, "checksum_errors") != 32 or @offsetOf(NativeStats, "malformed") != 40 or @offsetOf(NativeStats, "dhcp_rx") != 48 or @offsetOf(NativeStats, "dns_rx") != 56 or @offsetOf(NativeStats, "self_tests") != 64 or @offsetOf(NativeStats, "last_source_port") != 72 or @offsetOf(NativeStats, "last_dest_port") != 74 or @offsetOf(NativeStats, "last_error") != 80) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .dropped_short = value.dropped_short, .dropped_length = value.dropped_length, .checksum_errors = value.checksum_errors, .malformed = value.malformed, .dhcp_rx = value.dhcp_rx, .dns_rx = value.dns_rx, .self_tests = value.self_tests, .last_source_port = value.last_source_port, .last_dest_port = value.last_dest_port, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
extern fn r4os_rust_net_udp_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").UdpOp) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").UdpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_udp_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_udp_apply_tx_result(*NativeStats, *const @import("r4p_contract.zig").UdpOp) void;
pub fn applyTxResult(stats: *Stats, op: *const @import("r4p_contract.zig").UdpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_udp_apply_tx_result(&value, op);
    stats.* = fromNativeStats(value);
}

extern fn r4os_rust_net_udp_owned_stats(*NativeStats) void;
extern fn r4os_rust_net_udp_owned_error(Text) void;
pub fn ownedStats() Stats {
    var value: NativeStats = undefined;
    r4os_rust_net_udp_owned_stats(&value);
    return fromNativeStats(value);
}
pub fn setOwnedError(reason: []const u8) void {
    r4os_rust_net_udp_owned_error(.{ .pointer = reason.ptr, .len = reason.len });
}
extern fn r4os_rust_net_udp_owned_rx(*const @import("r4p_contract.zig").UdpOp) void;
pub fn ownedApplyRx(op: *const @import("r4p_contract.zig").UdpOp) void {
    r4os_rust_net_udp_owned_rx(op);
}
extern fn r4os_rust_net_udp_owned_tx(*const @import("r4p_contract.zig").UdpOp) void;
pub fn ownedApplyTx(op: *const @import("r4p_contract.zig").UdpOp) void {
    r4os_rust_net_udp_owned_tx(op);
}
