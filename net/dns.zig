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
    stats.* = .{};
}
