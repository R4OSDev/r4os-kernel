const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_net_dns_reset(*NativeStats) void;
pub const PORT: u16 = 53;

pub const RESULT_OK: i32 = 0;
pub const RESULT_SHORT: i32 = -1;
pub const RESULT_HEADER: i32 = -2;
pub const RESULT_QNAME: i32 = -3;
pub const RESULT_QUESTION: i32 = -4;
pub const RESULT_ANAME: i32 = -5;
pub const RESULT_ANSWER: i32 = -6;
pub const RESULT_ATYPE: i32 = -7;
pub const RESULT_BUFFER_SMALL: i32 = -8;
pub const RESULT_NAME: i32 = -9;
pub const RESULT_NXDOMAIN: i32 = -10;
pub const RESULT_TIMEOUT: i32 = -11;
pub const RESULT_NO_SERVER: i32 = -12;
pub const RESULT_TX: i32 = -13;

pub const Stats = struct {
    queries_tx: u64 = 0,
    responses_rx: u64 = 0,
    a_records: u64 = 0,
    resolve_requests: u64 = 0,
    timeouts: u64 = 0,
    nxdomain: u64 = 0,
    tx_errors: u64 = 0,
    malformed: u64 = 0,
    self_tests: u64 = 0,
    last_id: u16 = 0,
    last_result: i32 = 0,
    last_server: [4]u8 = .{0} ** 4,
    last_answer: [4]u8 = .{0} ** 4,
    last_name: [64]u8 = .{0} ** 64,
    operation_pending: bool = false,
    pending_name: [64]u8 = .{0} ** 64,
    cache_valid: bool = false,
    cache_name: [64]u8 = .{0} ** 64,
    cache_server: [4]u8 = .{0} ** 4,
    cache_answer: [4]u8 = .{0} ** 4,
    cache_hits: u64 = 0,
    cache_stores: u64 = 0,
    cache_updated_tick: u64 = 0,
    cache_ttl_seconds: u32 = 60,
    last_error: []const u8 = "none",
};

pub fn reset(stats: *Stats) void {
    var value: NativeStats = undefined;
    r4os_rust_net_dns_reset(&value);
    stats.* = fromNativeStats(value);
}

const NativeStats = extern struct {
    queries_tx: u64,
    responses_rx: u64,
    a_records: u64,
    resolve_requests: u64,
    timeouts: u64,
    nxdomain: u64,
    tx_errors: u64,
    malformed: u64,
    self_tests: u64,
    last_id: u16,
    last_result: i32,
    last_server: [4]u8,
    last_answer: [4]u8,
    last_name: [64]u8,
    operation_pending: bool,
    pending_name: [64]u8,
    cache_valid: bool,
    cache_name: [64]u8,
    cache_server: [4]u8,
    cache_answer: [4]u8,
    cache_hits: u64,
    cache_stores: u64,
    cache_updated_tick: u64,
    cache_ttl_seconds: u32,
    last_error: Text,
};
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .queries_tx = value.queries_tx, .responses_rx = value.responses_rx, .a_records = value.a_records, .resolve_requests = value.resolve_requests, .timeouts = value.timeouts, .nxdomain = value.nxdomain, .tx_errors = value.tx_errors, .malformed = value.malformed, .self_tests = value.self_tests, .last_id = value.last_id, .last_result = value.last_result, .last_server = value.last_server, .last_answer = value.last_answer, .last_name = value.last_name, .operation_pending = value.operation_pending, .pending_name = value.pending_name, .cache_valid = value.cache_valid, .cache_name = value.cache_name, .cache_server = value.cache_server, .cache_answer = value.cache_answer, .cache_hits = value.cache_hits, .cache_stores = value.cache_stores, .cache_updated_tick = value.cache_updated_tick, .cache_ttl_seconds = value.cache_ttl_seconds, .last_error = value.last_error.slice() };
}

comptime {
    if (@sizeOf(NativeStats) != 344 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "queries_tx") != 0 or @offsetOf(NativeStats, "responses_rx") != 8 or @offsetOf(NativeStats, "a_records") != 16 or @offsetOf(NativeStats, "resolve_requests") != 24 or @offsetOf(NativeStats, "timeouts") != 32 or @offsetOf(NativeStats, "nxdomain") != 40 or @offsetOf(NativeStats, "tx_errors") != 48 or @offsetOf(NativeStats, "malformed") != 56 or @offsetOf(NativeStats, "self_tests") != 64 or @offsetOf(NativeStats, "last_id") != 72 or @offsetOf(NativeStats, "last_result") != 76 or @offsetOf(NativeStats, "last_server") != 80 or @offsetOf(NativeStats, "last_answer") != 84 or @offsetOf(NativeStats, "last_name") != 88 or @offsetOf(NativeStats, "operation_pending") != 152 or @offsetOf(NativeStats, "pending_name") != 153 or @offsetOf(NativeStats, "cache_valid") != 217 or @offsetOf(NativeStats, "cache_name") != 218 or @offsetOf(NativeStats, "cache_server") != 282 or @offsetOf(NativeStats, "cache_answer") != 286 or @offsetOf(NativeStats, "cache_hits") != 296 or @offsetOf(NativeStats, "cache_stores") != 304 or @offsetOf(NativeStats, "cache_updated_tick") != 312 or @offsetOf(NativeStats, "cache_ttl_seconds") != 320 or @offsetOf(NativeStats, "last_error") != 328) @compileError("Native network NativeStats layout");
}

fn toNativeStats(value: Stats) NativeStats {
    return .{ .queries_tx = value.queries_tx, .responses_rx = value.responses_rx, .a_records = value.a_records, .resolve_requests = value.resolve_requests, .timeouts = value.timeouts, .nxdomain = value.nxdomain, .tx_errors = value.tx_errors, .malformed = value.malformed, .self_tests = value.self_tests, .last_id = value.last_id, .last_result = value.last_result, .last_server = value.last_server, .last_answer = value.last_answer, .last_name = value.last_name, .operation_pending = value.operation_pending, .pending_name = value.pending_name, .cache_valid = value.cache_valid, .cache_name = value.cache_name, .cache_server = value.cache_server, .cache_answer = value.cache_answer, .cache_hits = value.cache_hits, .cache_stores = value.cache_stores, .cache_updated_tick = value.cache_updated_tick, .cache_ttl_seconds = value.cache_ttl_seconds, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
extern fn r4os_rust_net_dns_apply_rx_result(*NativeStats, *const @import("r4p_contract.zig").DnsOp) void;
pub fn applyRxResult(stats: *Stats, op: *const @import("r4p_contract.zig").DnsOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_dns_apply_rx_result(&value, op);
    stats.* = fromNativeStats(value);
}
extern fn r4os_rust_net_dns_apply_build_result(*NativeStats, *const @import("r4p_contract.zig").DnsOp) void;
pub fn applyBuildResult(stats: *Stats, op: *const @import("r4p_contract.zig").DnsOp) void {
    var value = toNativeStats(stats.*);
    r4os_rust_net_dns_apply_build_result(&value, op);
    stats.* = fromNativeStats(value);
}

pub const CacheStatus = extern struct {
    valid: bool = false,
    negative: bool = false,
    age_seconds: u32 = 0,
    ttl_seconds: u32 = 0,
    remaining_seconds: u32 = 0,
};
extern fn r4os_rust_dns_cache_lookup(*NativeStats, [*]const u8, usize, *const [4]u8, *[4]u8, *i32) bool;
extern fn r4os_rust_dns_cache_store(*NativeStats, [*]const u8, usize, *const [4]u8, *const [4]u8, i32) void;
extern fn r4os_rust_dns_cache_expire(*NativeStats) void;
extern fn r4os_rust_dns_cache_sync(*NativeStats) void;
extern fn r4os_rust_dns_cache_status(*NativeStats, *CacheStatus) void;
extern fn r4os_rust_dns_cache_count(*NativeStats, bool) u32;
pub fn cacheLookup(stats: *Stats, name: []const u8, server: [4]u8, out: *[4]u8) ?i32 {
    var value = toNativeStats(stats.*);
    var result: i32 = 0;
    const found = r4os_rust_dns_cache_lookup(&value, name.ptr, name.len, &server, out, &result);
    stats.* = fromNativeStats(value);
    return if (found) result else null;
}
pub fn cacheStore(stats: *Stats, name: []const u8, server: [4]u8, answer: [4]u8, result: i32) void {
    var value = toNativeStats(stats.*);
    r4os_rust_dns_cache_store(&value, name.ptr, name.len, &server, &answer, result);
    stats.* = fromNativeStats(value);
}
pub fn cacheExpire(stats: *Stats) void {
    var value = toNativeStats(stats.*);
    r4os_rust_dns_cache_expire(&value);
    stats.* = fromNativeStats(value);
}
pub fn cacheSync(stats: *Stats) void {
    var value = toNativeStats(stats.*);
    r4os_rust_dns_cache_sync(&value);
    stats.* = fromNativeStats(value);
}
pub fn cacheStatus(stats: *Stats) CacheStatus {
    var value = toNativeStats(stats.*);
    var out: CacheStatus = undefined;
    r4os_rust_dns_cache_status(&value, &out);
    stats.* = fromNativeStats(value);
    return out;
}
pub fn cacheCount(stats: *Stats, negative: bool) u32 {
    var value = toNativeStats(stats.*);
    const count = r4os_rust_dns_cache_count(&value, negative);
    stats.* = fromNativeStats(value);
    return count;
}

comptime {
    if (@sizeOf(CacheStatus) != 16 or @alignOf(CacheStatus) != 4 or @offsetOf(CacheStatus, "valid") != 0 or @offsetOf(CacheStatus, "negative") != 1 or @offsetOf(CacheStatus, "age_seconds") != 4 or @offsetOf(CacheStatus, "ttl_seconds") != 8 or @offsetOf(CacheStatus, "remaining_seconds") != 12) @compileError("Native network CacheStatus layout");
}

extern fn r4os_rust_dns_client_stats(*NativeStats, bool) void;
extern fn r4os_rust_dns_client_set_pending([*]const u8, usize) void;
extern fn r4os_rust_dns_client_cache_status(*CacheStatus) void;
pub fn ownedStats(expire: bool) Stats {
    var value: NativeStats = undefined;
    r4os_rust_dns_client_stats(&value, expire);
    return fromNativeStats(value);
}
pub fn setOwnedPending(name: []const u8) void {
    r4os_rust_dns_client_set_pending(name.ptr, name.len);
}
pub fn ownedCacheStatus() CacheStatus {
    var out: CacheStatus = undefined;
    r4os_rust_dns_client_cache_status(&out);
    return out;
}
