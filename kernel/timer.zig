const config = @import("config");
const lapic = @import("../arch/x86_64/lapic.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const ioapic = @import("../arch/x86_64/ioapic.zig");
const pit = @import("../arch/x86_64/pit.zig");
const clocksource = @import("../platform/clocksource.zig");
const monotonic_math = @import("../platform/monotonic_math.zig");

pub const Backend = enum {
    pit,
    hpet,
    lapic,
};

pub const PIT_IRQ = pit.IRQ;
pub const DEFAULT_HZ = pit.DEFAULT_HZ;
pub const NO_DEADLINE: u64 = ~@as(u64, 0);
pub const MAX_FINITE_DEADLINE: u64 = NO_DEADLINE - 1;

pub const Mode = enum {
    periodic,
    one_shot_idle,
};

pub const EventClockInfo = struct {
    requested_hz: u32 = 0,
    effective_hz: u32 = 0,
    frequency_numerator: u64 = 0,
    frequency_denominator: u64 = 0,
    resolution_ns: u64 = 0,
};

pub const DeadlineStats = struct {
    enabled: bool = false,
    capable: bool = false,
    mode: Mode = .periodic,
    armed_deadline: u64 = NO_DEADLINE,
    timer_irqs: u64 = 0,
    idle_entries: u64 = 0,
    one_shot_arms: u64 = 0,
    earlier_reprograms: u64 = 0,
    cancels: u64 = 0,
    periodic_resumes: u64 = 0,
    runtime_fallbacks: u64 = 0,
    late_irqs: u64 = 0,
    max_lateness_ticks: u64 = 0,
    last_lateness_ticks: u64 = 0,
    last_irq_tick: u64 = 0,
};

var backend: Backend = .pit;
var initialized = false;
var tick_epoch: u64 = 0;
var tick_origin: u64 = 0;
var event_epoch_ns: u64 = 0;
var event_origin: u64 = 0;
var logical_hz: u32 = DEFAULT_HZ;
var modern_clock_origin_ns: u64 = 0;
var modern_counter_frequency_hz: u64 = 0;
var modern_tick_scale: monotonic_math.FixedScale = .{};
var deadline_enabled = false;
var deadline_mode: Mode = .periodic;
var armed_deadline: u64 = NO_DEADLINE;
var timer_irq_count: u64 = 0;
var idle_entry_count: u64 = 0;
var one_shot_arm_count: u64 = 0;
var earlier_reprogram_count: u64 = 0;
var deadline_cancel_count: u64 = 0;
var periodic_resume_count: u64 = 0;
var runtime_fallback_count: u64 = 0;
var late_irq_count: u64 = 0;
var max_lateness_ticks: u64 = 0;
var last_lateness_ticks: u64 = 0;
var last_irq_tick: u64 = 0;

pub fn initPit(requested_hz: u32) void {
    if (initialized) rebaseActiveClock();
    logical_hz = if (requested_hz == 0) DEFAULT_HZ else requested_hz;
    pit.init(logical_hz);
    backend = .pit;
    modern_clock_origin_ns = 0;
    modern_counter_frequency_hz = 0;
    modern_tick_scale = .{};
    deadline_enabled = false;
    deadline_mode = .periodic;
    armed_deadline = NO_DEADLINE;
    activateBackendOrigins();
    initialized = true;
}

pub fn trySwitchToLapic(requested_hz: u32) bool {
    if (!lapic.initTimerFromHpet(requested_hz)) return false;
    rebaseActiveClock();
    logical_hz = if (requested_hz == 0) DEFAULT_HZ else requested_hz;
    backend = .lapic;
    activateModernCounter();
    activateBackendOrigins();
    return true;
}

pub fn trySwitchToHpet(requested_hz: u32) bool {
    if (!hpet.startLegacyIrqTimer(requested_hz)) return false;
    rebaseActiveClock();
    logical_hz = if (requested_hz == 0) DEFAULT_HZ else requested_hz;
    backend = .hpet;
    activateModernCounter();
    activateBackendOrigins();
    return true;
}

pub fn fallbackToPit() void {
    rebaseActiveClock();
    if (backend == .lapic) lapic.stopTimer();
    if (backend == .hpet) hpet.stopLegacyIrqTimer();
    if (ioapic.isRoutingActive()) _ = ioapic.setLegacyIrqMasked(PIT_IRQ, false);
    pit.init(logical_hz);
    backend = .pit;
    modern_clock_origin_ns = 0;
    modern_counter_frequency_hz = 0;
    modern_tick_scale = .{};
    deadline_enabled = false;
    deadline_mode = .periodic;
    armed_deadline = NO_DEADLINE;
    activateBackendOrigins();
}

pub fn onIrq() u64 {
    // 0.56.15: COM1-TX-Ring pro Tick opportunistisch drainen, damit
    // gepufferte Logzeilen auch ohne weitere Ausgaben rausgehen.
    if (comptime config.enable_com1_debug) {
        const com = @import("../driver/com.zig");
        com.logDrain();
    }
    switch (backend) {
        .pit => pit.onTick(),
        .hpet => _ = hpet.onTimerIrq(),
        .lapic => _ = lapic.onTimerIrq(),
    }
    timer_irq_count +%= 1;
    if ((timer_irq_count & 0x3FF) == 0) _ = clocksource.periodicValidate();
    const now = tickCount();
    last_irq_tick = now;
    if (deadline_mode == .one_shot_idle) {
        last_lateness_ticks = 0;
        if (armed_deadline != NO_DEADLINE and now > armed_deadline) {
            const lateness = now - armed_deadline;
            last_lateness_ticks = lateness;
            late_irq_count +%= 1;
            if (lateness > max_lateness_ticks) max_lateness_ticks = lateness;
        }
        armed_deadline = NO_DEADLINE;
    }
    return now;
}

pub fn tickCount() u64 {
    const local = rawTickCount();
    return tick_epoch +| (local -% tick_origin);
}

pub fn deadlineAfter(now: u64, duration_ticks: u64) u64 {
    return monotonic_math.finiteDeadline(now, duration_ticks);
}

pub fn deadlineAfterNow(duration_ticks: u64) u64 {
    return deadlineAfter(tickCount(), duration_ticks);
}

pub fn remainingUntil(now: u64, deadline: u64) u64 {
    if (deadline == NO_DEADLINE) return NO_DEADLINE;
    return if (now >= deadline) 0 else deadline - now;
}

pub fn eventNanoseconds() u64 {
    if (!initialized) return 0;
    const local = rawTickCount();
    const elapsed = local -% event_origin;
    const rate = eventClockInfo();
    return event_epoch_ns +| monotonic_math.rateToNanoseconds(
        elapsed,
        rate.frequency_numerator,
        rate.frequency_denominator,
    );
}

pub fn eventClockInfo() EventClockInfo {
    const rate = switch (backend) {
        .pit => monotonic_math.reducedRate(pit.BASE_HZ, pit.divisor()),
        .hpet, .lapic => monotonic_math.Rate{ .numerator = logical_hz, .denominator = 1 },
    };
    const requested = frequency();
    return .{
        .requested_hz = requested,
        .effective_hz = monotonic_math.roundedRate(rate.numerator, rate.denominator),
        .frequency_numerator = rate.numerator,
        .frequency_denominator = rate.denominator,
        .resolution_ns = monotonic_math.resolutionNanoseconds(rate.numerator, rate.denominator),
    };
}

fn rawTickCount() u64 {
    return switch (backend) {
        .pit => pit.tickCount(),
        .hpet, .lapic => modernTickCount(),
    };
}

fn modernTickCount() u64 {
    if (modern_counter_frequency_hz == 0 or !modern_tick_scale.valid()) return 0;
    const now_ns = clocksource.nowNanoseconds() orelse return 0;
    return modern_tick_scale.apply(now_ns -% modern_clock_origin_ns);
}

fn activateModernCounter() void {
    const source = clocksource.status();
    modern_counter_frequency_hz = source.frequency_hz;
    modern_tick_scale = monotonic_math.FixedScale.initFloor(logical_hz, monotonic_math.nanoseconds_per_second);
    modern_clock_origin_ns = clocksource.nowNanoseconds() orelse 0;
}

fn rebaseActiveClock() void {
    if (!initialized) return;
    const local = rawTickCount();
    tick_epoch +|= local -% tick_origin;
    const rate = eventClockInfo();
    event_epoch_ns +|= monotonic_math.rateToNanoseconds(
        local -% event_origin,
        rate.frequency_numerator,
        rate.frequency_denominator,
    );
}

fn activateBackendOrigins() void {
    const local = rawTickCount();
    tick_origin = local;
    event_origin = local;
}

pub fn frequency() u32 {
    return logical_hz;
}

pub fn enableDeadlineScheduling() bool {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    deadline_enabled = backend != .pit and modern_counter_frequency_hz != 0;
    deadline_mode = .periodic;
    armed_deadline = NO_DEADLINE;
    return deadline_enabled;
}

pub fn enterIdleDeadline(requested_deadline: u64) bool {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    if (!deadline_enabled or backend == .pit) return false;

    const now = tickCount();
    if (deadline_mode == .periodic) {
        const prepared = switch (backend) {
            .hpet => hpet.startLegacyOneShotTimer(logical_hz),
            .lapic => lapic.startOneShotTimer(),
            .pit => false,
        };
        if (!prepared) {
            runtime_fallback_count +%= 1;
            fallbackToPit();
            return false;
        }
        deadline_mode = .one_shot_idle;
        idle_entry_count +%= 1;
    }

    if (requested_deadline == NO_DEADLINE) {
        disarmOneShotBackend();
        armed_deadline = NO_DEADLINE;
        return true;
    }

    if (armed_deadline != NO_DEADLINE and requested_deadline < armed_deadline) {
        earlier_reprogram_count +%= 1;
    }
    const delta = monotonic_math.boundedDeadlineDelta(now, requested_deadline, MAX_FINITE_DEADLINE -| now);
    const programmed_ticks = switch (backend) {
        .hpet => hpet.armLegacyOneShotTicks(delta, logical_hz),
        .lapic => lapic.armOneShotTicks(delta),
        .pit => 0,
    };
    if (programmed_ticks == 0) {
        runtime_fallback_count +%= 1;
        fallbackToPit();
        return false;
    }
    armed_deadline = deadlineAfter(now, programmed_ticks);
    one_shot_arm_count +%= 1;
    return true;
}

pub fn leaveIdleDeadline() bool {
    // Productive scheduler observations call this defensively.  The deadline
    // backend is owned by the BSP, so keep the overwhelmingly common periodic
    // path to one local branch instead of nesting the runtime owner on every
    // cooperative yield.
    if (deadline_mode != .one_shot_idle) return backend != .pit;
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    if (deadline_mode != .one_shot_idle) return backend != .pit;

    if (armed_deadline != NO_DEADLINE) deadline_cancel_count +%= 1;
    disarmOneShotBackend();
    armed_deadline = NO_DEADLINE;
    const resumed = switch (backend) {
        .hpet => hpet.resumeLegacyIrqTimer(logical_hz),
        .lapic => lapic.resumePeriodicTimer(),
        .pit => true,
    };
    if (!resumed) {
        runtime_fallback_count +%= 1;
        fallbackToPit();
        return false;
    }
    deadline_mode = .periodic;
    periodic_resume_count +%= 1;
    return true;
}

fn disarmOneShotBackend() void {
    switch (backend) {
        .hpet => hpet.disarmLegacyOneShotTimer(),
        .lapic => lapic.disarmOneShotTimer(),
        .pit => {},
    }
}

pub fn deadlineStats() DeadlineStats {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    return .{
        .enabled = deadline_enabled,
        .capable = backend != .pit and modern_counter_frequency_hz != 0,
        .mode = deadline_mode,
        .armed_deadline = armed_deadline,
        .timer_irqs = timer_irq_count,
        .idle_entries = idle_entry_count,
        .one_shot_arms = one_shot_arm_count,
        .earlier_reprograms = earlier_reprogram_count,
        .cancels = deadline_cancel_count,
        .periodic_resumes = periodic_resume_count,
        .runtime_fallbacks = runtime_fallback_count,
        .late_irqs = late_irq_count,
        .max_lateness_ticks = max_lateness_ticks,
        .last_lateness_ticks = last_lateness_ticks,
        .last_irq_tick = last_irq_tick,
    };
}

pub fn deadlineModeName() []const u8 {
    return switch (deadline_mode) {
        .periodic => if (deadline_enabled) "periodic-active" else "periodic-fallback",
        .one_shot_idle => "one-shot-idle",
    };
}

pub fn activeBackend() Backend {
    return backend;
}

pub fn backendName() []const u8 {
    return switch (backend) {
        .pit => "PIT",
        .hpet => "HPET",
        .lapic => "LAPIC timer",
    };
}
