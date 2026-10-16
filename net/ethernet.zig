const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_ethernet_reset(*NativeStats) void;
pub const TYPE_IPV4: u16 = 0x0800;
pub const TYPE_ARP: u16 = 0x0806;
pub const TYPE_R4OS_DIAG: u16 = 0x88B5;

pub const MIN_FRAME_SIZE: usize = 60;
pub const HEADER_SIZE: usize = 14;

pub const Stats = struct {
    rx: u64 = 0,
    tx: u64 = 0,
    broadcast: u64 = 0,
    own_unicast: u64 = 0,
    dropped_short: u64 = 0,
    dropped_filter: u64 = 0,
    unknown_ethertype: u64 = 0,
    test_frames: u64 = 0,
    last_ethertype: u16 = 0,
    last_error: []const u8 = "none",
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_ethernet_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeStats = extern struct {
    rx: u64,
    tx: u64,
    broadcast: u64,
    own_unicast: u64,
    dropped_short: u64,
    dropped_filter: u64,
    unknown_ethertype: u64,
    test_frames: u64,
    last_ethertype: u16,
    last_error: Text,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .rx = value.rx, .tx = value.tx, .broadcast = value.broadcast, .own_unicast = value.own_unicast, .dropped_short = value.dropped_short, .dropped_filter = value.dropped_filter, .unknown_ethertype = value.unknown_ethertype, .test_frames = value.test_frames, .last_ethertype = value.last_ethertype, .last_error = value.last_error.slice() };
}

comptime {
    if (@sizeOf(NativeStats) != 88 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "rx") != 0 or @offsetOf(NativeStats, "tx") != 8 or @offsetOf(NativeStats, "broadcast") != 16 or @offsetOf(NativeStats, "own_unicast") != 24 or @offsetOf(NativeStats, "dropped_short") != 32 or @offsetOf(NativeStats, "dropped_filter") != 40 or @offsetOf(NativeStats, "unknown_ethertype") != 48 or @offsetOf(NativeStats, "test_frames") != 56 or @offsetOf(NativeStats, "last_ethertype") != 64 or @offsetOf(NativeStats, "last_error") != 72) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .rx = value.rx, .tx = value.tx, .broadcast = value.broadcast, .own_unicast = value.own_unicast, .dropped_short = value.dropped_short, .dropped_filter = value.dropped_filter, .unknown_ethertype = value.unknown_ethertype, .test_frames = value.test_frames, .last_ethertype = value.last_ethertype, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
extern fn r4os_rust_net_ethernet_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").EthernetFrameOp) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").EthernetFrameOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_ethernet_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_ethernet_apply_tx_result(*NativeStats, *const @import("r4p_contract.zig").EthernetFrameOp) void;
pub fn applyTxResult(stats: *Stats, op: *const @import("r4p_contract.zig").EthernetFrameOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_ethernet_apply_tx_result(&value, op);
    stats.* = fromNativeStats(value);
}

extern fn r4os_rust_net_ethernet_owned_stats(*NativeStats) void;
extern fn r4os_rust_net_ethernet_owned_error(Text) void;
pub fn ownedStats() Stats {
    var value: NativeStats = undefined;
    r4os_rust_net_ethernet_owned_stats(&value);
    return fromNativeStats(value);
}
pub fn setOwnedError(reason: []const u8) void {
    r4os_rust_net_ethernet_owned_error(.{ .pointer = reason.ptr, .len = reason.len });
}
extern fn r4os_rust_net_ethernet_owned_rx(*const @import("r4p_contract.zig").EthernetFrameOp) void;
pub fn ownedApplyRx(op: *const @import("r4p_contract.zig").EthernetFrameOp) void {
    r4os_rust_net_ethernet_owned_rx(op);
}
extern fn r4os_rust_net_ethernet_owned_tx(*const @import("r4p_contract.zig").EthernetFrameOp) void;
pub fn ownedApplyTx(op: *const @import("r4p_contract.zig").EthernetFrameOp) void {
    r4os_rust_net_ethernet_owned_tx(op);
}
