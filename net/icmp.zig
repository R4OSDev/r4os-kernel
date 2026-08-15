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
    stats.* = .{};
}
