// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_net_timing_status(*Status) void;
extern fn r4os_rust_net_deadline_start(u64, usize, *Deadline) void;
extern fn r4os_rust_net_deadline_expired(*const Deadline, u64) bool;
extern fn r4os_rust_net_deadline_loops(*const Deadline, usize) bool;
extern fn r4os_rust_net_now_ticks() u64;
extern fn r4os_rust_net_tick_hz() u64;
extern fn r4os_rust_net_elapsed_ticks(u64) u64;
extern fn r4os_rust_net_elapsed_seconds(u64) u32;
extern fn r4os_rust_net_seconds_remaining(u32, u32) u32;
extern fn r4os_rust_net_ticks_from_seconds(u32) u64;
extern fn r4os_rust_net_operation_name(u8, *Text) void;
extern fn r4os_rust_net_timing_check() bool;
const Text = @import("../driver/usb/core.zig").Text;

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

pub const Status = extern struct {
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

pub const Deadline = extern struct {
    start_tick: u64 = 0,
    timeout_ticks: u64 = 0,
    max_loops: usize = 0,

    pub fn start(timeout_ticks: u64, max_loops: usize) Deadline {
        var out: Deadline = undefined;
        r4os_rust_net_deadline_start(timeout_ticks, max_loops, &out);
        return out;
    }

    pub fn expired(self: Deadline, current_tick: u64) bool {
        return r4os_rust_net_deadline_expired(&self, current_tick);
    }

    pub fn expiredNow(self: Deadline) bool {
        return self.expired(nowTicks());
    }

    pub fn loopLimitReached(self: Deadline, loops: usize) bool {
        return r4os_rust_net_deadline_loops(&self, loops);
    }

    pub fn elapsedTicks(self: Deadline) u64 {
        return elapsedTicksSince(self.start_tick);
    }
};

pub fn status() Status {
    var out: Status = undefined;
    r4os_rust_net_timing_status(&out);
    return out;
}

pub fn nowTicks() u64 {
    return r4os_rust_net_now_ticks();
}

pub fn tickHz() u64 {
    return r4os_rust_net_tick_hz();
}

pub fn elapsedTicksSince(start_tick: u64) u64 {
    return r4os_rust_net_elapsed_ticks(start_tick);
}

pub fn elapsedSecondsSince(start_tick: u64) u32 {
    return r4os_rust_net_elapsed_seconds(start_tick);
}

pub fn secondsRemaining(total: u32, elapsed: u32) u32 {
    return r4os_rust_net_seconds_remaining(total, elapsed);
}

pub fn ticksFromSeconds(seconds: u32) u64 {
    return r4os_rust_net_ticks_from_seconds(seconds);
}

pub fn operationStatusName(state: OperationStatus) []const u8 {
    var out: Text = undefined;
    r4os_rust_net_operation_name(@intFromEnum(state), &out);
    return out.slice();
}

pub fn contractCheck() bool {
    return r4os_rust_net_timing_check();
}

comptime {
    if (@sizeOf(Status) != 88 or @alignOf(Status) != 8 or @offsetOf(Status, "ticks") != 0 or @offsetOf(Status, "hz") != 8 or @offsetOf(Status, "arp_cache_ttl_ticks") != 16 or @offsetOf(Status, "arp_resolve_timeout_ticks") != 24 or @offsetOf(Status, "dhcp_timeout_ticks") != 32 or @offsetOf(Status, "dns_timeout_ticks") != 40 or @offsetOf(Status, "tcp_listen_timeout_ticks") != 48 or @offsetOf(Status, "tcp_retransmit_timeout_ticks") != 56 or @offsetOf(Status, "tcp_time_wait_ticks") != 64 or @offsetOf(Status, "service_operation_timeout_ticks") != 72 or @offsetOf(Status, "operation_status_count") != 80) @compileError("Native network Status layout");
}

comptime {
    if (@sizeOf(Deadline) != 24 or @alignOf(Deadline) != 8 or @offsetOf(Deadline, "start_tick") != 0 or @offsetOf(Deadline, "timeout_ticks") != 8 or @offsetOf(Deadline, "max_loops") != 16) @compileError("Native network Deadline layout");
}
