const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_icmp_reset(*NativeStats) void;
pub const IPV4_PROTOCOL: u8 = 1;

pub const Stats = struct {
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    echo_requests_rx: u64 = 0,
    echo_replies_rx: u64 = 0,
    echo_requests_tx: u64 = 0,
    echo_replies_tx: u64 = 0,
    destination_unreachable_rx: u64 = 0,
    port_unreachable_rx: u64 = 0,
    time_exceeded_rx: u64 = 0,
    malformed: u64 = 0,
    checksum_errors: u64 = 0,
    last_type: u8 = 0,
    last_code: u8 = 0,
    last_id: u16 = 0,
    last_seq: u16 = 0,
    last_error: []const u8 = "none",
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_icmp_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeStats = extern struct {
    rx_packets: u64,
    tx_packets: u64,
    echo_requests_rx: u64,
    echo_replies_rx: u64,
    echo_requests_tx: u64,
    echo_replies_tx: u64,
    destination_unreachable_rx: u64,
    port_unreachable_rx: u64,
    time_exceeded_rx: u64,
    malformed: u64,
    checksum_errors: u64,
    last_type: u8,
    last_code: u8,
    last_id: u16,
    last_seq: u16,
    last_error: Text,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .echo_requests_rx = value.echo_requests_rx, .echo_replies_rx = value.echo_replies_rx, .echo_requests_tx = value.echo_requests_tx, .echo_replies_tx = value.echo_replies_tx, .destination_unreachable_rx = value.destination_unreachable_rx, .port_unreachable_rx = value.port_unreachable_rx, .time_exceeded_rx = value.time_exceeded_rx, .malformed = value.malformed, .checksum_errors = value.checksum_errors, .last_type = value.last_type, .last_code = value.last_code, .last_id = value.last_id, .last_seq = value.last_seq, .last_error = value.last_error.slice() };
}

comptime {
    if (@sizeOf(NativeStats) != 112 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "rx_packets") != 0 or @offsetOf(NativeStats, "tx_packets") != 8 or @offsetOf(NativeStats, "echo_requests_rx") != 16 or @offsetOf(NativeStats, "echo_replies_rx") != 24 or @offsetOf(NativeStats, "echo_requests_tx") != 32 or @offsetOf(NativeStats, "echo_replies_tx") != 40 or @offsetOf(NativeStats, "destination_unreachable_rx") != 48 or @offsetOf(NativeStats, "port_unreachable_rx") != 56 or @offsetOf(NativeStats, "time_exceeded_rx") != 64 or @offsetOf(NativeStats, "malformed") != 72 or @offsetOf(NativeStats, "checksum_errors") != 80 or @offsetOf(NativeStats, "last_type") != 88 or @offsetOf(NativeStats, "last_code") != 89 or @offsetOf(NativeStats, "last_id") != 90 or @offsetOf(NativeStats, "last_seq") != 92 or @offsetOf(NativeStats, "last_error") != 96) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .echo_requests_rx = value.echo_requests_rx, .echo_replies_rx = value.echo_replies_rx, .echo_requests_tx = value.echo_requests_tx, .echo_replies_tx = value.echo_replies_tx, .destination_unreachable_rx = value.destination_unreachable_rx, .port_unreachable_rx = value.port_unreachable_rx, .time_exceeded_rx = value.time_exceeded_rx, .malformed = value.malformed, .checksum_errors = value.checksum_errors, .last_type = value.last_type, .last_code = value.last_code, .last_id = value.last_id, .last_seq = value.last_seq, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
extern fn r4os_rust_net_icmp_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").IcmpOp) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").IcmpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_icmp_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_icmp_apply_tx_result(*NativeStats, *const @import("r4p_contract.zig").IcmpOp) void;
pub fn applyTxResult(stats: *Stats, op: *const @import("r4p_contract.zig").IcmpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_icmp_apply_tx_result(&value, op);
    stats.* = fromNativeStats(value);
}

extern fn r4os_rust_net_icmp_owned_stats(*NativeStats) void;
extern fn r4os_rust_net_icmp_owned_error(Text) void;
pub fn ownedStats() Stats {
    var value: NativeStats = undefined;
    r4os_rust_net_icmp_owned_stats(&value);
    return fromNativeStats(value);
}
pub fn setOwnedError(reason: []const u8) void {
    r4os_rust_net_icmp_owned_error(.{ .pointer = reason.ptr, .len = reason.len });
}
extern fn r4os_rust_net_icmp_owned_rx(*const @import("r4p_contract.zig").IcmpOp) void;
pub fn ownedApplyRx(op: *const @import("r4p_contract.zig").IcmpOp) void {
    r4os_rust_net_icmp_owned_rx(op);
}
extern fn r4os_rust_net_icmp_owned_tx(*const @import("r4p_contract.zig").IcmpOp) void;
pub fn ownedApplyTx(op: *const @import("r4p_contract.zig").IcmpOp) void {
    r4os_rust_net_icmp_owned_tx(op);
}
