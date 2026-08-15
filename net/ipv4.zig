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
    stats.* = .{};
}

pub fn payloadFitsMtu(mtu: u16, payload_len: usize) bool {
    return HEADER_SIZE + payload_len <= @as(usize, mtu);
}

pub fn sameIp(a: [4]u8, b: [4]u8) bool {
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}
