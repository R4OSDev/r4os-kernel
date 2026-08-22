const interrupts = @import("../../arch/x86_64/interrupts.zig");
const hpet = @import("../../arch/x86_64/hpet.zig");
const timer = @import("../../kernel/timer.zig");
const platform_cpu = @import("../../platform/cpu.zig");
const monotonic = @import("../../platform/monotonic.zig");
const scheduler = @import("../../sched/scheduler.zig");
const timing = @import("usb_boot_timing.zig");
const std = @import("std");

const MIN_PLAUSIBLE_TSC_HZ: u64 = 1_000_000;
const MAX_PLAUSIBLE_TSC_HZ: u64 = 10_000_000_000;
// A deliberately high fallback can only make a deadline later on realistic
// x86_64 hardware; it must never turn a requested two seconds into 70 ms.
const FALLBACK_TSC_HZ: u64 = 10_000_000_000;
// Runtime xHCI completions are normally visible within a fraction of one
// scheduler tick. Give that fast path a short, real-time bounded poll grace;
// only a genuinely slower device parks the owner for a timer slice. This
// avoids turning a microsecond completion into scheduler-priority latency,
// while long waits still spend almost all of their time blocked.
const RUNTIME_POLL_GRACE_NS: u64 = std.time.ns_per_ms;
const RUNTIME_POLL_FALLBACK_ITERATIONS: u64 = 100_000;
// Last-resort bounded delay for a machine exposing neither an advancing IRQ
// clock nor HPET/TSC. It is deliberately only a liveness guard, not a claimed
// wall-clock calibration.
const NO_CLOCK_SPINS_PER_MS: u64 = 100_000;

pub const Deadline = struct {
    flags: u64,
    start_monotonic: monotonic.Stamp,
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

    pub fn begin(milliseconds_value: u32) Deadline {
        const flags = interrupts.saveAndDisable();
        const may_enable = interrupts.wereEnabled(flags) or scheduler.current() == null;
        const start_monotonic = monotonic.capture();
        const monotonic_clock = monotonic.snapshot();
        const monotonic_valid = monotonic.resolve(start_monotonic) != null and
            (may_enable or (monotonic_clock.flags & monotonic.flag_irq_independent) != 0);
        const hpet_info = hpet.status();
        const hpet_mask: u64 = if (hpet_info.counter_64bit)
            std.math.maxInt(u64)
        else
            std.math.maxInt(u32);
        const duration_hpet = if (hpet_info.mapped and
            hpet_info.enabled and
            hpet_info.frequency_hz != 0)
            clockCyclesForMilliseconds(
                hpet_info.frequency_hz,
                milliseconds_value,
                hpet_mask,
            )
        else
            0;
        // Early enumeration has no task yet and deliberately borrows IRQs so
        // PIT ticks can form a wall-clock deadline.  A runtime caller that
        // entered with IF=0 is inside a critical section: never silently
        // enable interrupts there. The free-running HPET remains usable with
        // IF=0; an invariant TSC is the second independent clock.
        const duration_ticks = if (may_enable)
            timing.ticksForMilliseconds(milliseconds_value, timer.frequency())
        else
            0;
        const cpu_info = platform_cpu.status();
        const trusted_tsc = cpu_info.features.tsc and cpu_info.features.invariant_tsc;
        const exact_tsc_hz = if (trusted_tsc) exactTscFrequencyHz(cpu_info) else 0;
        const has_wall_clock = duration_ticks != 0 or duration_hpet != 0;
        // Very old hardware can expose neither HPET nor an invariant TSC.
        // Keep an ordinary TSC only as a finite last-resort escape hatch when
        // no wall clock can advance. An invariant TSC may run in parallel,
        // but only CPUID.15 supplies an exact rate; otherwise the conservative
        // 10-GHz bound can delay, never prematurely shorten, the deadline.
        const use_tsc = cpu_info.features.tsc and (trusted_tsc or !has_wall_clock);
        const fallback_tsc = use_tsc and exact_tsc_hz == 0;
        const duration_tsc = if (use_tsc)
            clockCyclesForMilliseconds(
                if (exact_tsc_hz != 0) exact_tsc_hz else FALLBACK_TSC_HZ,
                milliseconds_value,
                std.math.maxInt(u64),
            )
        else
            0;
        const out = Deadline{
            .flags = flags,
            .start_monotonic = start_monotonic,
            .duration_ns = @as(u64, milliseconds_value) * std.time.ns_per_ms,
            .monotonic_valid = monotonic_valid,
            .start_tick = timer.tickCount(),
            .duration_ticks = duration_ticks,
            .start_hpet = if (duration_hpet != 0) hpet.readMainCounter() & hpet_mask else 0,
            .duration_hpet = duration_hpet,
            .hpet_mask = hpet_mask,
            .start_tsc = if (duration_tsc != 0) readTsc() else 0,
            .duration_tsc = duration_tsc,
            .fallback_tsc = fallback_tsc,
        };
        if (may_enable) interrupts.enable();
        return out;
    }

    pub fn expired(self: *const Deadline) bool {
        return self.duration_ticks != 0 and timer.tickCount() -% self.start_tick >= self.duration_ticks;
    }

    pub fn monotonicExpired(self: *const Deadline) bool {
        if (!self.monotonic_valid or self.duration_ns == 0) return false;
        const elapsed = monotonic.elapsedSince(self.start_monotonic) orelse return false;
        return elapsed >= self.duration_ns;
    }

    pub fn usesMonotonicDeadline(self: *const Deadline) bool {
        return self.monotonic_valid and self.duration_ns != 0;
    }

    pub fn usesTickDeadline(self: *const Deadline) bool {
        return self.duration_ticks != 0;
    }

    pub fn hpetExpired(self: *const Deadline) bool {
        return self.duration_hpet != 0 and self.elapsedHpet() >= self.duration_hpet;
    }

    pub fn usesHpetDeadline(self: *const Deadline) bool {
        return self.duration_hpet != 0;
    }

    pub fn tscExpired(self: *const Deadline) bool {
        return self.duration_tsc != 0 and self.elapsedTsc() >= self.duration_tsc;
    }

    pub fn usesTscDeadline(self: *const Deadline) bool {
        return self.duration_tsc != 0;
    }

    pub fn usesFallbackTsc(self: *const Deadline) bool {
        return self.fallback_tsc;
    }

    pub fn hasClock(self: *const Deadline) bool {
        return self.monotonic_valid or
            self.duration_ticks != 0 or
            self.duration_hpet != 0 or
            self.duration_tsc != 0;
    }

    pub fn expiredAny(self: *const Deadline) bool {
        return self.monotonicExpired() or self.expired() or self.hpetExpired() or self.tscExpired();
    }

    pub fn elapsedNanoseconds(self: *const Deadline) u64 {
        if (self.monotonic_valid) {
            if (monotonic.elapsedSince(self.start_monotonic)) |elapsed| return elapsed;
        }
        if (self.duration_ticks != 0 and timer.frequency() != 0) {
            const scaled = @as(u128, self.elapsedTicks()) * std.time.ns_per_s;
            return @intCast(@min(scaled / timer.frequency(), std.math.maxInt(u64)));
        }
        return 0;
    }

    pub fn canBlock(self: *const Deadline) bool {
        return scheduler.current() != null and interrupts.wereEnabled(self.flags);
    }

    pub fn waitStep(self: *const Deadline, reason: []const u8) bool {
        if (self.canBlock()) {
            scheduler.sleepTicksWithReason(1, reason);
            return true;
        }
        asm volatile ("pause");
        return false;
    }

    pub fn elapsedTicks(self: *const Deadline) u64 {
        return timer.tickCount() -% self.start_tick;
    }

    pub fn elapsedHpet(self: *const Deadline) u64 {
        if (self.duration_hpet == 0) return 0;
        const now = hpet.readMainCounter() & self.hpet_mask;
        return (now -% self.start_hpet) & self.hpet_mask;
    }

    pub fn elapsedTsc(self: *const Deadline) u64 {
        if (self.duration_tsc == 0) return 0;
        return readTsc() -% self.start_tsc;
    }

    pub fn finish(self: *const Deadline) void {
        interrupts.restore(self.flags);
    }
};

pub const Phase = enum(u8) {
    early_boot = 0,
    runtime = 1,
};

pub const WaitResult = struct {
    reason: []const u8,
    phase: Phase,
    requested_ms: u32,
    elapsed_ns: u64,
    iterations: u64,
    blocked_ticks: u64,
    retry: u8,
    success: bool,
};

pub const Metrics = struct {
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
    last_reason: []const u8 = "none",
    last_phase: Phase = .early_boot,
    last_requested_ms: u32 = 0,
    last_elapsed_ns: u64 = 0,
    last_iterations: u64 = 0,
    last_blocked_ticks: u64 = 0,
    last_retry: u8 = 0,
    last_success: bool = false,
};

var current_metrics: Metrics = .{};

pub fn resetMetrics() void {
    current_metrics = .{};
}

pub fn metrics() Metrics {
    return current_metrics;
}

pub const Wait = struct {
    deadline: Deadline,
    reason: []const u8,
    phase: Phase,
    requested_ms: u32,
    iterations: u64 = 0,
    blocked_ticks: u64 = 0,
    retry: u8 = 0,
    runtime_poll_grace: bool = true,

    pub fn begin(milliseconds_value: u32, reason: []const u8, retry: u8) Wait {
        const phase: Phase = if (scheduler.current() == null) .early_boot else .runtime;
        return .{
            .deadline = Deadline.begin(milliseconds_value),
            .reason = reason,
            .phase = phase,
            .requested_ms = milliseconds_value,
            .retry = retry,
        };
    }

    pub fn expired(self: *const Wait) bool {
        if (self.deadline.hasClock()) return self.deadline.expiredAny();
        return self.iterations >= fallbackIterations(self.requested_ms);
    }

    pub fn idle(self: *Wait) void {
        self.iterations +%= 1;
        if (self.phase == .runtime and
            self.runtime_poll_grace and
            self.deadline.canBlock() and
            !self.runtimePollGraceExpired())
        {
            asm volatile ("pause");
            return;
        }
        if (self.deadline.waitStep(self.reason)) self.blocked_ticks +%= 1;
    }

    fn runtimePollGraceExpired(self: *const Wait) bool {
        const elapsed_ns = self.deadline.elapsedNanoseconds();
        if (elapsed_ns != 0) return elapsed_ns >= RUNTIME_POLL_GRACE_NS;
        return self.iterations >= RUNTIME_POLL_FALLBACK_ITERATIONS;
    }

    pub fn finish(self: *Wait, success: bool) WaitResult {
        const result = WaitResult{
            .reason = self.reason,
            .phase = self.phase,
            .requested_ms = self.requested_ms,
            .elapsed_ns = self.deadline.elapsedNanoseconds(),
            .iterations = self.iterations,
            .blocked_ticks = self.blocked_ticks,
            .retry = self.retry,
            .success = success,
        };
        self.deadline.finish();
        record(result);
        return result;
    }
};

fn record(result: WaitResult) void {
    current_metrics.calls +%= 1;
    if (result.success) {
        current_metrics.successes +%= 1;
    } else {
        current_metrics.timeouts +%= 1;
    }
    switch (result.phase) {
        .early_boot => current_metrics.early_calls +%= 1,
        .runtime => current_metrics.runtime_calls +%= 1,
    }
    current_metrics.requested_ms +%= result.requested_ms;
    current_metrics.elapsed_ns +%= result.elapsed_ns;
    current_metrics.max_elapsed_ns = @max(current_metrics.max_elapsed_ns, result.elapsed_ns);
    current_metrics.iterations +%= result.iterations;
    current_metrics.blocked_ticks +%= result.blocked_ticks;
    current_metrics.last_reason = result.reason;
    current_metrics.last_phase = result.phase;
    current_metrics.last_requested_ms = result.requested_ms;
    current_metrics.last_elapsed_ns = result.elapsed_ns;
    current_metrics.last_iterations = result.iterations;
    current_metrics.last_blocked_ticks = result.blocked_ticks;
    current_metrics.last_retry = result.retry;
    current_metrics.last_success = result.success;
}

// Storage discovery runs before the scheduler. Temporarily enable IRQs so the
// early PIT can provide a real time base, then restore the caller's IF state.
pub fn milliseconds(value: u32) void {
    _ = millisecondsWithReason(value, "usb-delay", 0);
}

pub fn millisecondsWithReason(value: u32, reason: []const u8, retry: u8) WaitResult {
    if (value == 0) {
        const result = WaitResult{
            .reason = reason,
            .phase = if (scheduler.current() == null) .early_boot else .runtime,
            .requested_ms = 0,
            .elapsed_ns = 0,
            .iterations = 0,
            .blocked_ticks = 0,
            .retry = retry,
            .success = true,
        };
        record(result);
        return result;
    }
    var wait = Wait.begin(value, reason, retry);
    // A named delay has no hardware condition to discover early. At runtime
    // it blocks immediately instead of spending the event fast-path grace
    // as active CPU time.
    wait.runtime_poll_grace = false;
    while (!wait.expired()) wait.idle();
    return wait.finish(true);
}

fn fallbackIterations(value: u32) u64 {
    return @as(u64, value) * NO_CLOCK_SPINS_PER_MS;
}

fn clockCyclesForMilliseconds(
    frequency: u64,
    milliseconds_value: u32,
    maximum: u64,
) u64 {
    if (frequency == 0 or milliseconds_value == 0) return 0;
    const product = @as(u128, frequency) * milliseconds_value;
    const cycles = (product + 999) / 1000;
    const maximum_wide = @as(u128, maximum);
    // Modular elapsed-time comparison needs the requested interval to fit in
    // one hardware-counter revolution. Disable this source otherwise.
    if (cycles == 0 or cycles > maximum_wide) return 0;
    return @intCast(cycles);
}

fn exactTscFrequencyHz(info: platform_cpu.Status) u64 {
    if (info.tsc_denominator != 0 and info.tsc_numerator != 0 and info.crystal_hz != 0) {
        const frequency = (@as(u128, info.crystal_hz) * info.tsc_numerator) / info.tsc_denominator;
        if (frequency >= MIN_PLAUSIBLE_TSC_HZ and frequency <= MAX_PLAUSIBLE_TSC_HZ)
            return @intCast(frequency);
    }
    // CPUID.16 base_mhz describes the processor base frequency, not a
    // guaranteed TSC frequency. Treating it as exact can make a parallel TSC
    // source beat a valid PIT/HPET deadline far too early.
    return 0;
}

fn readTsc() u64 {
    var lo: u32 = undefined;
    var hi: u32 = undefined;
    // Order preceding MMIO/event-ring observations before the timestamp so a
    // speculative RDTSC cannot shorten the visible timeout window.
    asm volatile ("lfence");
    asm volatile ("rdtsc"
        : [lo] "={eax}" (lo),
          [hi] "={edx}" (hi),
    );
    return (@as(u64, hi) << 32) | lo;
}
