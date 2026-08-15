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
    stats.* = .{};
}
