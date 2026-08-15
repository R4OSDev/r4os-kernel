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
    stats.* = .{};
}
