extern fn r4os_rust_net_ipv4_payload_fits(u16, usize) bool;
extern fn r4os_rust_net_ipv4_same(*const [4]u8, *const [4]u8) bool;
const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_ipv4_reset(*NativeStats) void;
pub const HEADER_SIZE: usize = 20;

pub const PacketView = struct {
    protocol: u8,
    source_ip: [4]u8,
    dest_ip: [4]u8,
    payload: []const u8,
};

pub const Stats = struct {
    rx_packets: u64 = 0,
    tx_packets: u64 = 0,
    dropped_short: u64 = 0,
    dropped_version: u64 = 0,
    dropped_checksum: u64 = 0,
    dropped_fragment: u64 = 0,
    dropped_destination: u64 = 0,
    dropped_tx_too_large: u64 = 0,
    malformed: u64 = 0,
    last_protocol: u8 = 0,
    last_error: []const u8 = "none",
    last_source: [4]u8 = .{0} ** 4,
    last_dest: [4]u8 = .{0} ** 4,
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_ipv4_reset(&value);
    stats.* = fromNativeStats(value);
}

pub fn payloadFitsMtu(mtu: u16, payload_len: usize) bool {
    return r4os_rust_net_ipv4_payload_fits(mtu, payload_len);
}

pub fn sameIp(a: [4]u8, b: [4]u8) bool {
    return r4os_rust_net_ipv4_same(&a, &b);
}

const NativeStats = extern struct {
    rx_packets: u64,
    tx_packets: u64,
    dropped_short: u64,
    dropped_version: u64,
    dropped_checksum: u64,
    dropped_fragment: u64,
    dropped_destination: u64,
    dropped_tx_too_large: u64,
    malformed: u64,
    last_protocol: u8,
    last_error: Text,
    last_source: [4]u8,
    last_dest: [4]u8,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .dropped_short = value.dropped_short, .dropped_version = value.dropped_version, .dropped_checksum = value.dropped_checksum, .dropped_fragment = value.dropped_fragment, .dropped_destination = value.dropped_destination, .dropped_tx_too_large = value.dropped_tx_too_large, .malformed = value.malformed, .last_protocol = value.last_protocol, .last_error = value.last_error.slice(), .last_source = value.last_source, .last_dest = value.last_dest };
}

comptime {
    if (@sizeOf(NativeStats) != 104 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "rx_packets") != 0 or @offsetOf(NativeStats, "tx_packets") != 8 or @offsetOf(NativeStats, "dropped_short") != 16 or @offsetOf(NativeStats, "dropped_version") != 24 or @offsetOf(NativeStats, "dropped_checksum") != 32 or @offsetOf(NativeStats, "dropped_fragment") != 40 or @offsetOf(NativeStats, "dropped_destination") != 48 or @offsetOf(NativeStats, "dropped_tx_too_large") != 56 or @offsetOf(NativeStats, "malformed") != 64 or @offsetOf(NativeStats, "last_protocol") != 72 or @offsetOf(NativeStats, "last_error") != 80 or @offsetOf(NativeStats, "last_source") != 96 or @offsetOf(NativeStats, "last_dest") != 100) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .rx_packets = value.rx_packets, .tx_packets = value.tx_packets, .dropped_short = value.dropped_short, .dropped_version = value.dropped_version, .dropped_checksum = value.dropped_checksum, .dropped_fragment = value.dropped_fragment, .dropped_destination = value.dropped_destination, .dropped_tx_too_large = value.dropped_tx_too_large, .malformed = value.malformed, .last_protocol = value.last_protocol, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len }, .last_source = value.last_source, .last_dest = value.last_dest };
}
extern fn r4os_rust_net_ipv4_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").Ipv4Op) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").Ipv4Op) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_ipv4_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_ipv4_apply_tx_result(*NativeStats, *const @import("r4p_contract.zig").Ipv4Op) void;
pub fn applyTxResult(stats: *Stats, op: *const @import("r4p_contract.zig").Ipv4Op) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_ipv4_apply_tx_result(&value, op);
    stats.* = fromNativeStats(value);
}

extern fn r4os_rust_net_ipv4_owned_stats(*NativeStats) void;
extern fn r4os_rust_net_ipv4_owned_error(Text) void;
pub fn ownedStats() Stats {
    var value: NativeStats = undefined;
    r4os_rust_net_ipv4_owned_stats(&value);
    return fromNativeStats(value);
}
pub fn setOwnedError(reason: []const u8) void {
    r4os_rust_net_ipv4_owned_error(.{ .pointer = reason.ptr, .len = reason.len });
}
extern fn r4os_rust_net_ipv4_owned_rx(*const @import("r4p_contract.zig").Ipv4Op) void;
pub fn ownedApplyRx(op: *const @import("r4p_contract.zig").Ipv4Op) void {
    r4os_rust_net_ipv4_owned_rx(op);
}
extern fn r4os_rust_net_ipv4_owned_tx(*const @import("r4p_contract.zig").Ipv4Op) void;
pub fn ownedApplyTx(op: *const @import("r4p_contract.zig").Ipv4Op) void {
    r4os_rust_net_ipv4_owned_tx(op);
}
