const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_arp_reset(*NativeStats) void;
pub const Stats = struct {
    requests_tx: u64 = 0,
    replies_tx: u64 = 0,
    replies_rx: u64 = 0,
    requests_rx: u64 = 0,
    malformed: u64 = 0,
    cache_updates: u64 = 0,
    cache_hits: u64 = 0,
    resolve_attempts: u64 = 0,
    resolve_retries: u64 = 0,
    resolve_timeouts: u64 = 0,
    resolve_misses: u64 = 0,
    pending_packets: u64 = 0,
    pending_timeouts: u64 = 0,
    pending_drops: u64 = 0,
    pending_queue_limit: u64 = 0,
    last_opcode: u16 = 0,
    last_error: []const u8 = "none",
    cache_valid: bool = false,
    cache_ip: [4]u8 = .{0} ** 4,
    cache_mac: [6]u8 = .{0} ** 6,
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_arp_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeStats = extern struct {
    requests_tx: u64,
    replies_tx: u64,
    replies_rx: u64,
    requests_rx: u64,
    malformed: u64,
    cache_updates: u64,
    cache_hits: u64,
    resolve_attempts: u64,
    resolve_retries: u64,
    resolve_timeouts: u64,
    resolve_misses: u64,
    pending_packets: u64,
    pending_timeouts: u64,
    pending_drops: u64,
    pending_queue_limit: u64,
    last_opcode: u16,
    last_error: Text,
    cache_valid: bool,
    cache_ip: [4]u8,
    cache_mac: [6]u8,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .requests_tx = value.requests_tx, .replies_tx = value.replies_tx, .replies_rx = value.replies_rx, .requests_rx = value.requests_rx, .malformed = value.malformed, .cache_updates = value.cache_updates, .cache_hits = value.cache_hits, .resolve_attempts = value.resolve_attempts, .resolve_retries = value.resolve_retries, .resolve_timeouts = value.resolve_timeouts, .resolve_misses = value.resolve_misses, .pending_packets = value.pending_packets, .pending_timeouts = value.pending_timeouts, .pending_drops = value.pending_drops, .pending_queue_limit = value.pending_queue_limit, .last_opcode = value.last_opcode, .last_error = value.last_error.slice(), .cache_valid = value.cache_valid, .cache_ip = value.cache_ip, .cache_mac = value.cache_mac };
}

comptime {
    if (@sizeOf(NativeStats) != 160 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "requests_tx") != 0 or @offsetOf(NativeStats, "replies_tx") != 8 or @offsetOf(NativeStats, "replies_rx") != 16 or @offsetOf(NativeStats, "requests_rx") != 24 or @offsetOf(NativeStats, "malformed") != 32 or @offsetOf(NativeStats, "cache_updates") != 40 or @offsetOf(NativeStats, "cache_hits") != 48 or @offsetOf(NativeStats, "resolve_attempts") != 56 or @offsetOf(NativeStats, "resolve_retries") != 64 or @offsetOf(NativeStats, "resolve_timeouts") != 72 or @offsetOf(NativeStats, "resolve_misses") != 80 or @offsetOf(NativeStats, "pending_packets") != 88 or @offsetOf(NativeStats, "pending_timeouts") != 96 or @offsetOf(NativeStats, "pending_drops") != 104 or @offsetOf(NativeStats, "pending_queue_limit") != 112 or @offsetOf(NativeStats, "last_opcode") != 120 or @offsetOf(NativeStats, "last_error") != 128 or @offsetOf(NativeStats, "cache_valid") != 144 or @offsetOf(NativeStats, "cache_ip") != 145 or @offsetOf(NativeStats, "cache_mac") != 149) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .requests_tx = value.requests_tx, .replies_tx = value.replies_tx, .replies_rx = value.replies_rx, .requests_rx = value.requests_rx, .malformed = value.malformed, .cache_updates = value.cache_updates, .cache_hits = value.cache_hits, .resolve_attempts = value.resolve_attempts, .resolve_retries = value.resolve_retries, .resolve_timeouts = value.resolve_timeouts, .resolve_misses = value.resolve_misses, .pending_packets = value.pending_packets, .pending_timeouts = value.pending_timeouts, .pending_drops = value.pending_drops, .pending_queue_limit = value.pending_queue_limit, .last_opcode = value.last_opcode, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len }, .cache_valid = value.cache_valid, .cache_ip = value.cache_ip, .cache_mac = value.cache_mac };
}
extern fn r4os_rust_net_arp_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").ArpOp) bool;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").ArpOp) bool {
    var value = toNativeStats(stats.*);
    const action = r4os_rust_net_arp_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
    return action;
}
extern fn r4os_rust_net_arp_apply_tx_result(*NativeStats, *const @import("r4p_contract.zig").ArpOp) void;
pub fn applyTxResult(stats: *Stats, op: *const @import("r4p_contract.zig").ArpOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_arp_apply_tx_result(&value, op);
    stats.* = fromNativeStats(value);
}

extern fn r4os_rust_arp_cache_remember(*NativeStats, *const [4]u8, *const [6]u8) void;
extern fn r4os_rust_arp_cache_remember_stats(*const NativeStats) void;
extern fn r4os_rust_arp_cache_lookup(*NativeStats, *const [4]u8, *[6]u8) bool;
extern fn r4os_rust_arp_cache_count(*NativeStats) u32;
extern fn r4os_rust_arp_cache_expire(*NativeStats) void;
extern fn r4os_rust_arp_cache_flush(*NativeStats) void;
extern fn r4os_rust_arp_cache_age(bool) u64;
pub fn rememberPeer(stats: *Stats, ip: [4]u8, mac: [6]u8) void {
    var value = toNativeStats(stats.*);
    r4os_rust_arp_cache_remember(&value, &ip, &mac);
    stats.* = fromNativeStats(value);
}
pub fn rememberPeerFromStats(stats: *const Stats) void {
    const value = toNativeStats(stats.*);
    r4os_rust_arp_cache_remember_stats(&value);
}
pub fn cacheLookup(stats: *Stats, ip: [4]u8, out: *[6]u8) bool {
    var value = toNativeStats(stats.*);
    const found = r4os_rust_arp_cache_lookup(&value, &ip, out);
    stats.* = fromNativeStats(value);
    return found;
}
pub fn cacheCount(stats: *Stats) u32 {
    var value = toNativeStats(stats.*);
    const count = r4os_rust_arp_cache_count(&value);
    stats.* = fromNativeStats(value);
    return count;
}
pub fn cacheExpire(stats: *Stats) void {
    var value = toNativeStats(stats.*);
    r4os_rust_arp_cache_expire(&value);
    stats.* = fromNativeStats(value);
}
pub fn cacheFlush(stats: *Stats) void {
    var value = toNativeStats(stats.*);
    r4os_rust_arp_cache_flush(&value);
    stats.* = fromNativeStats(value);
}
pub fn cacheAge(valid: bool) u64 {
    return r4os_rust_arp_cache_age(valid);
}

extern fn r4os_rust_net_arp_owned_stats(*NativeStats, bool) void;
extern fn r4os_rust_net_arp_owned_error(Text) void;
pub fn ownedStats(expire: bool) Stats {
    var value: NativeStats = undefined;
    r4os_rust_net_arp_owned_stats(&value, expire);
    return fromNativeStats(value);
}
pub fn setOwnedError(reason: []const u8) void {
    r4os_rust_net_arp_owned_error(.{ .pointer = reason.ptr, .len = reason.len });
}
extern fn r4os_rust_net_arp_owned_rx(*const @import("r4p_contract.zig").ArpOp) bool;
pub fn ownedApplyRx(op: *const @import("r4p_contract.zig").ArpOp) bool {
    return r4os_rust_net_arp_owned_rx(op);
}
extern fn r4os_rust_net_arp_owned_tx(*const @import("r4p_contract.zig").ArpOp) void;
pub fn ownedApplyTx(op: *const @import("r4p_contract.zig").ArpOp) void {
    r4os_rust_net_arp_owned_tx(op);
}
