const config = @import("config");
const lapic = @import("../arch/x86_64/lapic.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const pit = @import("../arch/x86_64/pit.zig");
const monotonic_math = @import("../platform/monotonic_math.zig");

pub const Backend = enum {
    pit,
    hpet,
    lapic,
};

pub const PIT_IRQ = pit.IRQ;
pub const DEFAULT_HZ = pit.DEFAULT_HZ;

pub const EventClockInfo = struct {
    requested_hz: u32 = 0,
    effective_hz: u32 = 0,
    frequency_numerator: u64 = 0,
    frequency_denominator: u64 = 0,
    resolution_ns: u64 = 0,
};

var backend: Backend = .pit;
var initialized = false;
var tick_epoch: u64 = 0;
var tick_origin: u64 = 0;
var event_epoch_ns: u64 = 0;
var event_origin: u64 = 0;

pub fn initPit(requested_hz: u32) void {
    if (initialized) rebaseActiveClock();
    pit.init(requested_hz);
    backend = .pit;
    activateBackendOrigins();
    initialized = true;
}

pub fn trySwitchToLapic(requested_hz: u32) bool {
    if (!lapic.initTimerFromHpet(requested_hz)) return false;
    rebaseActiveClock();
    backend = .lapic;
    activateBackendOrigins();
    return true;
}

pub fn trySwitchToHpet(requested_hz: u32) bool {
    if (!hpet.startLegacyIrqTimer(requested_hz)) return false;
    rebaseActiveClock();
    backend = .hpet;
    activateBackendOrigins();
    return true;
}

pub fn fallbackToPit() void {
    rebaseActiveClock();
    if (backend == .lapic) lapic.stopTimer();
    if (backend == .hpet) hpet.stopLegacyIrqTimer();
    backend = .pit;
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
        .hpet => return hpet.onTimerIrq(),
        .lapic => return lapic.onTimerIrq(),
    }
    return tickCount();
}

pub fn tickCount() u64 {
    const local = rawTickCount();
    return tick_epoch +% (local -% tick_origin);
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
        .hpet => blk: {
            const status = hpet.status();
            break :blk monotonic_math.reducedRate(status.frequency_hz, status.timer0_period);
        },
        .lapic => blk: {
            const status = lapic.status();
            const numerator_wide = @as(u128, status.calibration_lapic_ticks) * hpet.status().frequency_hz;
            const denominator_wide = @as(u128, status.calibration_hpet_ticks) * status.timer_initial_count;
            if (numerator_wide == 0 or denominator_wide == 0 or
                numerator_wide > @as(u128, ~@as(u64, 0)) or
                denominator_wide > @as(u128, ~@as(u64, 0)))
            {
                break :blk monotonic_math.Rate{ .numerator = status.timer_frequency_hz, .denominator = 1 };
            }
            break :blk monotonic_math.reducedRate(@intCast(numerator_wide), @intCast(denominator_wide));
        },
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
        .hpet => hpet.timerTickCount(),
        .lapic => lapic.timerTickCount(),
    };
}

fn rebaseActiveClock() void {
    if (!initialized) return;
    const local = rawTickCount();
    tick_epoch +%= local -% tick_origin;
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
    return switch (backend) {
        .pit => pit.frequency(),
        .hpet => hpet.timerFrequency(),
        .lapic => lapic.timerFrequency(),
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
