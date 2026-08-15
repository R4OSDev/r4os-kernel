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
    stats.* = .{};
}
