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
    stats.* = .{};
}
