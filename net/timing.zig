const time_core = @import("../platform/time.zig");

// 0.56.29: Zeitbudgets in ms definiert und comptime gegen die
// Kernel-Tickrate gerechnet (vorher harte Tick-Werte fuer 100 Hz).
const timer_hz = @import("../kernel/timer.zig").DEFAULT_HZ;

pub fn msToTicks(comptime ms: u64) u64 {
    const t = (ms * timer_hz) / 1000;
    return if (t == 0) 1 else t;
}

pub const DEFAULT_ARP_CACHE_TTL_TICKS: u64 = msToTicks(30_000);
pub const DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS: u64 = msToTicks(500);
pub const DEFAULT_DHCP_TIMEOUT_TICKS: u64 = msToTicks(2_000);
pub const DEFAULT_DNS_TIMEOUT_TICKS: u64 = msToTicks(4_000);
pub const DEFAULT_TCP_LISTEN_TIMEOUT_TICKS: u64 = msToTicks(10_000);
pub const DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS: u64 = msToTicks(1_500);
pub const DEFAULT_TCP_TIME_WAIT_TICKS: u64 = msToTicks(3_000);
pub const DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS: u64 = msToTicks(4_000);

pub const OperationStatus = enum(u8) {
    idle,
    pending,
    ok,
    timeout,
    failed,
    cancelled,
    would_block,
};

pub const Status = struct {
    ticks: u64 = 0,
    hz: u64 = 0,
    arp_cache_ttl_ticks: u64 = DEFAULT_ARP_CACHE_TTL_TICKS,
    arp_resolve_timeout_ticks: u64 = DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS,
    dhcp_timeout_ticks: u64 = DEFAULT_DHCP_TIMEOUT_TICKS,
    dns_timeout_ticks: u64 = DEFAULT_DNS_TIMEOUT_TICKS,
    tcp_listen_timeout_ticks: u64 = DEFAULT_TCP_LISTEN_TIMEOUT_TICKS,
    tcp_retransmit_timeout_ticks: u64 = DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS,
    tcp_time_wait_ticks: u64 = DEFAULT_TCP_TIME_WAIT_TICKS,
    service_operation_timeout_ticks: u64 = DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS,
    operation_status_count: u32 = 7,
};

pub const Deadline = struct {
    start_tick: u64 = 0,
    timeout_ticks: u64 = 0,
    max_loops: usize = 0,

    pub fn start(timeout_ticks: u64, max_loops: usize) Deadline {
        return .{
            .start_tick = nowTicks(),
            .timeout_ticks = timeout_ticks,
            .max_loops = max_loops,
        };
    }

    pub fn expired(self: Deadline, current_tick: u64) bool {
        if (self.timeout_ticks == 0) return false;
        if (current_tick < self.start_tick) return false;
        return current_tick - self.start_tick >= self.timeout_ticks;
    }

    pub fn expiredNow(self: Deadline) bool {
        return self.expired(nowTicks());
    }

    pub fn loopLimitReached(self: Deadline, loops: usize) bool {
        return self.max_loops != 0 and loops >= self.max_loops;
    }

    pub fn elapsedTicks(self: Deadline) u64 {
        return elapsedTicksSince(self.start_tick);
    }
};

pub fn status() Status {
    return .{
        .ticks = nowTicks(),
        .hz = tickHz(),
    };
}

pub fn nowTicks() u64 {
    return time_core.monotonicTicks();
}

pub fn tickHz() u64 {
    return time_core.monotonicFrequency();
}

pub fn elapsedTicksSince(start_tick: u64) u64 {
    if (start_tick == 0) return 0;
    const now = nowTicks();
    if (now <= start_tick) return 0;
    return now - start_tick;
}

pub fn elapsedSecondsSince(start_tick: u64) u32 {
    const hz = tickHz();
    if (hz == 0) return 0;
    const seconds = elapsedTicksSince(start_tick) / hz;
    return if (seconds > 0xFFFF_FFFF) 0xFFFF_FFFF else @intCast(seconds);
}

pub fn secondsRemaining(total: u32, elapsed: u32) u32 {
    if (total == 0 or elapsed >= total) return 0;
    return total - elapsed;
}

pub fn ticksFromSeconds(seconds: u32) u64 {
    const hz = tickHz();
    if (hz == 0 or seconds == 0) return 0;
    return @as(u64, seconds) * hz;
}

pub fn operationStatusName(state: OperationStatus) []const u8 {
    return switch (state) {
        .idle => "idle",
        .pending => "pending",
        .ok => "ok",
        .timeout => "timeout",
        .failed => "failed",
        .cancelled => "cancelled",
        .would_block => "would-block",
    };
}

pub fn contractCheck() bool {
    const deadline = Deadline{ .start_tick = 10, .timeout_ticks = 5, .max_loops = 3 };
    if (deadline.expired(14)) return false;
    if (!deadline.expired(15)) return false;
    if (deadline.loopLimitReached(2)) return false;
    if (!deadline.loopLimitReached(3)) return false;
    if (secondsRemaining(10, 3) != 7) return false;
    if (secondsRemaining(10, 10) != 0) return false;
    if (!textEquals(operationStatusName(.would_block), "would-block")) return false;
    if (!textEquals(operationStatusName(.cancelled), "cancelled")) return false;
    const s = status();
    return s.tcp_time_wait_ticks == DEFAULT_TCP_TIME_WAIT_TICKS and
        s.service_operation_timeout_ticks == DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS and
        s.operation_status_count == 7;
}

fn textEquals(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (a[index] != b[index]) return false;
    }
    return true;
}
