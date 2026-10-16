// USB deadline/wait policy and metrics have one native Rust owner.
const Text = @import("core.zig").Text;
pub const Phase = enum(u8) { early_boot = 0, runtime = 1 };
const Stamp = extern struct { raw: u64, generation: u32, kind: u8 };
pub const Deadline = extern struct {
    flags: u64,
    start_monotonic: Stamp,
    duration_ns: u64,
    monotonic_valid: bool,
    start_tick: u64,
    duration_ticks: u64,
    start_hpet: u64,
    duration_hpet: u64,
    hpet_mask: u64,
    start_tsc: u64,
    duration_tsc: u64,
    fallback_tsc: bool,
    pub fn begin(ms: u32) Deadline {
        var out: Deadline = undefined;
        r4os_rust_usb_deadline_begin(ms, &out);
        return out;
    }
    pub fn expired(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_expired(self);
    }
    pub fn monotonicExpired(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_monotonic_expired(self);
    }
    pub fn usesMonotonicDeadline(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_uses_monotonic_deadline(self);
    }
    pub fn usesTickDeadline(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_uses_tick_deadline(self);
    }
    pub fn hpetExpired(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_hpet_expired(self);
    }
    pub fn usesHpetDeadline(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_uses_hpet_deadline(self);
    }
    pub fn tscExpired(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_tsc_expired(self);
    }
    pub fn usesTscDeadline(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_uses_tsc_deadline(self);
    }
    pub fn usesFallbackTsc(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_uses_fallback_tsc(self);
    }
    pub fn hasClock(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_has_clock(self);
    }
    pub fn expiredAny(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_expired_any(self);
    }
    pub fn elapsedNanoseconds(self: *const Deadline) u64 {
        return r4os_rust_usb_deadline_elapsed_nanoseconds(self);
    }
    pub fn canBlock(self: *const Deadline) bool {
        return r4os_rust_usb_deadline_can_block(self);
    }
    pub fn elapsedTicks(self: *const Deadline) u64 {
        return r4os_rust_usb_deadline_elapsed_ticks(self);
    }
    pub fn elapsedHpet(self: *const Deadline) u64 {
        return r4os_rust_usb_deadline_elapsed_hpet(self);
    }
    pub fn elapsedTsc(self: *const Deadline) u64 {
        return r4os_rust_usb_deadline_elapsed_tsc(self);
    }
    pub fn waitStep(self: *const Deadline, reason: []const u8) bool {
        return r4os_rust_usb_deadline_step(self, reason.ptr, reason.len);
    }
    pub fn finish(self: *const Deadline) void {
        r4os_rust_usb_deadline_finish(self);
    }
};
comptime {
    if (@sizeOf(Deadline) != 104 or
        @alignOf(Deadline) != 8 or
        @offsetOf(Deadline, "flags") != 0 or
        @offsetOf(Deadline, "start_monotonic") != 8 or
        @offsetOf(Deadline, "duration_ns") != 24 or
        @offsetOf(Deadline, "monotonic_valid") != 32 or
        @offsetOf(Deadline, "start_tick") != 40 or
        @offsetOf(Deadline, "duration_ticks") != 48 or
        @offsetOf(Deadline, "start_hpet") != 56 or
        @offsetOf(Deadline, "duration_hpet") != 64 or
        @offsetOf(Deadline, "hpet_mask") != 72 or
        @offsetOf(Deadline, "start_tsc") != 80 or
        @offsetOf(Deadline, "duration_tsc") != 88 or
        @offsetOf(Deadline, "fallback_tsc") != 96) @compileError("USB Deadline private C layout");
}
pub const WaitResult = extern struct {
    reason: Text,
    phase: Phase,
    requested_ms: u32,
    elapsed_ns: u64,
    iterations: u64,
    blocked_ticks: u64,
    retry: u8,
    success: bool,
};
comptime {
    if (@sizeOf(WaitResult) != 56 or
        @alignOf(WaitResult) != 8 or
        @offsetOf(WaitResult, "reason") != 0 or
        @offsetOf(WaitResult, "phase") != 16 or
        @offsetOf(WaitResult, "requested_ms") != 20 or
        @offsetOf(WaitResult, "elapsed_ns") != 24 or
        @offsetOf(WaitResult, "iterations") != 32 or
        @offsetOf(WaitResult, "blocked_ticks") != 40 or
        @offsetOf(WaitResult, "retry") != 48 or
        @offsetOf(WaitResult, "success") != 49) @compileError("USB WaitResult private C layout");
}
pub const Metrics = extern struct {
    calls: u64 = 0,
    successes: u64 = 0,
    timeouts: u64 = 0,
    early_calls: u64 = 0,
    runtime_calls: u64 = 0,
    requested_ms: u64 = 0,
    elapsed_ns: u64 = 0,
    max_elapsed_ns: u64 = 0,
    iterations: u64 = 0,
    blocked_ticks: u64 = 0,
    last_reason: Text = Text.init("none"),
    last_phase: Phase = .early_boot,
    last_requested_ms: u32 = 0,
    last_elapsed_ns: u64 = 0,
    last_iterations: u64 = 0,
    last_blocked_ticks: u64 = 0,
    last_retry: u8 = 0,
    last_success: bool = false,
};
comptime {
    if (@sizeOf(Metrics) != 136 or
        @alignOf(Metrics) != 8 or
        @offsetOf(Metrics, "calls") != 0 or
        @offsetOf(Metrics, "successes") != 8 or
        @offsetOf(Metrics, "timeouts") != 16 or
        @offsetOf(Metrics, "early_calls") != 24 or
        @offsetOf(Metrics, "runtime_calls") != 32 or
        @offsetOf(Metrics, "requested_ms") != 40 or
        @offsetOf(Metrics, "elapsed_ns") != 48 or
        @offsetOf(Metrics, "max_elapsed_ns") != 56 or
        @offsetOf(Metrics, "iterations") != 64 or
        @offsetOf(Metrics, "blocked_ticks") != 72 or
        @offsetOf(Metrics, "last_reason") != 80 or
        @offsetOf(Metrics, "last_phase") != 96 or
        @offsetOf(Metrics, "last_requested_ms") != 100 or
        @offsetOf(Metrics, "last_elapsed_ns") != 104 or
        @offsetOf(Metrics, "last_iterations") != 112 or
        @offsetOf(Metrics, "last_blocked_ticks") != 120 or
        @offsetOf(Metrics, "last_retry") != 128 or
        @offsetOf(Metrics, "last_success") != 129) @compileError("USB Metrics private C layout");
}
pub const Wait = extern struct {
    deadline: Deadline,
    reason: Text,
    phase: Phase,
    requested_ms: u32,
    iterations: u64 = 0,
    blocked_ticks: u64 = 0,
    retry: u8 = 0,
    runtime_poll_grace: bool = true,
    pub fn begin(ms: u32, reason: []const u8, retry: u8) Wait {
        var out: Wait = undefined;
        r4os_rust_usb_wait_begin(ms, reason.ptr, reason.len, retry, &out);
        return out;
    }
    pub fn expired(self: *const Wait) bool {
        return r4os_rust_usb_wait_expired(self);
    }
    pub fn idle(self: *Wait) void {
        r4os_rust_usb_wait_idle(self);
    }
    pub fn mayBlockOnIdle(self: *const Wait) bool {
        return r4os_rust_usb_wait_may_block(self);
    }
    pub fn pollOnce(self: *Wait) void {
        r4os_rust_usb_wait_poll_once(self);
    }
    pub fn finish(self: *Wait, success: bool) WaitResult {
        var out: WaitResult = undefined;
        r4os_rust_usb_wait_finish(self, success, &out);
        return out;
    }
};
comptime {
    if (@sizeOf(Wait) != 152 or
        @alignOf(Wait) != 8 or
        @offsetOf(Wait, "deadline") != 0 or
        @offsetOf(Wait, "reason") != 104 or
        @offsetOf(Wait, "phase") != 120 or
        @offsetOf(Wait, "requested_ms") != 124 or
        @offsetOf(Wait, "iterations") != 128 or
        @offsetOf(Wait, "blocked_ticks") != 136 or
        @offsetOf(Wait, "retry") != 144 or
        @offsetOf(Wait, "runtime_poll_grace") != 145) @compileError("USB Wait private C layout");
}
extern fn r4os_rust_usb_deadline_begin(u32, *Deadline) void;
extern fn r4os_rust_usb_deadline_expired(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_monotonic_expired(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_uses_monotonic_deadline(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_uses_tick_deadline(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_hpet_expired(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_uses_hpet_deadline(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_tsc_expired(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_uses_tsc_deadline(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_uses_fallback_tsc(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_has_clock(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_expired_any(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_elapsed_nanoseconds(*const Deadline) u64;
extern fn r4os_rust_usb_deadline_can_block(*const Deadline) bool;
extern fn r4os_rust_usb_deadline_elapsed_ticks(*const Deadline) u64;
extern fn r4os_rust_usb_deadline_elapsed_hpet(*const Deadline) u64;
extern fn r4os_rust_usb_deadline_elapsed_tsc(*const Deadline) u64;
extern fn r4os_rust_usb_deadline_step(*const Deadline, [*]const u8, usize) bool;
extern fn r4os_rust_usb_deadline_finish(*const Deadline) void;
extern fn r4os_rust_usb_wait_reset_metrics() void;
extern fn r4os_rust_usb_wait_metrics(*Metrics) void;
extern fn r4os_rust_usb_wait_begin(u32, [*]const u8, usize, u8, *Wait) void;
extern fn r4os_rust_usb_wait_expired(*const Wait) bool;
extern fn r4os_rust_usb_wait_idle(*Wait) void;
extern fn r4os_rust_usb_wait_may_block(*const Wait) bool;
extern fn r4os_rust_usb_wait_poll_once(*Wait) void;
extern fn r4os_rust_usb_wait_finish(*Wait, bool, *WaitResult) void;
extern fn r4os_rust_usb_wait_milliseconds(u32, [*]const u8, usize, u8, *WaitResult) void;
pub fn resetMetrics() void {
    r4os_rust_usb_wait_reset_metrics();
}
pub fn metrics() Metrics {
    var out: Metrics = undefined;
    r4os_rust_usb_wait_metrics(&out);
    return out;
}
pub fn milliseconds(value: u32) void {
    _ = millisecondsWithReason(value, "usb-delay", 0);
}
pub fn millisecondsWithReason(value: u32, reason: []const u8, retry: u8) WaitResult {
    var out: WaitResult = undefined;
    r4os_rust_usb_wait_milliseconds(value, reason.ptr, reason.len, retry, &out);
    return out;
}
