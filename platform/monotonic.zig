const cpu = @import("cpu.zig");
const hpet = @import("../arch/x86_64/hpet.zig");
const timer = @import("../kernel/timer.zig");
const math = @import("monotonic_math.zig");

pub const clock_frequency_hz: u64 = math.nanoseconds_per_second;

pub const flag_valid: u32 = 1 << 0;
pub const flag_continuous: u32 = 1 << 1;
pub const flag_high_resolution: u32 = 1 << 2;
pub const flag_irq_independent: u32 = 1 << 3;
pub const flag_invariant: u32 = 1 << 4;
pub const flag_early_origin: u32 = 1 << 5;
pub const flag_calibrated: u32 = 1 << 6;
pub const flag_degraded: u32 = 1 << 7;

pub const Source = enum(u32) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
    periodic_event = 3,
};

const StampKind = enum(u8) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
    instant_ns = 3,
};

pub const Stamp = struct {
    raw: u64 = 0,
    kind: StampKind = .unavailable,
    generation: u32 = 0,
};

pub const Snapshot = struct {
    valid: bool = false,
    instant_ns: u64 = 0,
    flags: u32 = 0,
    source: Source = .unavailable,
    generation: u32 = 0,
    resolution_ns: u64 = 0,
    source_frequency_hz: u64 = 0,
    event_backend: timer.Backend = .pit,
    event: timer.EventClockInfo = .{},
};

const min_plausible_tsc_hz: u64 = 1_000_000;
const max_plausible_tsc_hz: u64 = 10_000_000_000;

var early_initialized = false;
var early_tsc_origin: u64 = 0;
var tsc_configured = false;
var tsc_frequency_hz: u64 = 0;
var hpet_configured = false;
var hpet_frequency_hz: u64 = 0;
var hpet_origin_raw: u64 = 0;
var hpet_epoch_ns: u64 = 0;
var active_source: Source = .unavailable;
var active_generation: u32 = 0;
var published_ns: u64 = 0;

pub fn earlyInit() void {
    if (early_initialized) return;
    early_tsc_origin = readTsc();
    early_initialized = true;
}

pub fn configureCpuClock() void {
    if (!early_initialized) earlyInit();
    const info = cpu.status();
    if (!info.features.tsc or !info.features.invariant_tsc) return;
    const frequency = exactTscFrequencyHz(info);
    if (frequency == 0) return;

    tsc_frequency_hz = frequency;
    tsc_configured = true;
    if (active_source == .unavailable) {
        active_source = .tsc;
        active_generation +%= 1;
    }
}

pub fn attachPeriodicClock() void {
    if (active_source != .unavailable) return;
    active_source = .periodic_event;
    active_generation +%= 1;
    _ = publish(timer.eventNanoseconds());
}

pub fn attachHpetClock() void {
    const status = hpet.status();
    if (!status.mapped or !status.enabled or status.frequency_hz == 0) return;

    const prior = currentCandidate();
    hpet_frequency_hz = status.frequency_hz;
    hpet_origin_raw = hpet.readExtendedMainCounter();
    hpet_epoch_ns = prior;
    hpet_configured = true;

    // A calibrated invariant TSC is cheaper than MMIO and already includes
    // the earliest kernel entry. HPET remains the preferred independent
    // source whenever such a TSC is unavailable.
    if (!tsc_configured) {
        active_source = .hpet;
        active_generation +%= 1;
        _ = publish(prior);
    }
}

pub fn capture() Stamp {
    return switch (active_source) {
        .tsc => .{ .raw = readTsc(), .kind = .tsc, .generation = active_generation },
        .hpet => .{ .raw = hpet.readExtendedMainCounter(), .kind = .hpet, .generation = active_generation },
        .periodic_event => .{ .raw = nowNanoseconds() orelse 0, .kind = .instant_ns, .generation = active_generation },
        .unavailable => if (early_initialized)
            .{ .raw = readTsc(), .kind = .tsc, .generation = active_generation }
        else
            .{},
    };
}

pub fn resolve(stamp: Stamp) ?u64 {
    return switch (stamp.kind) {
        .unavailable => null,
        .instant_ns => stamp.raw,
        .tsc => if (tsc_configured)
            math.cyclesToNanoseconds(stamp.raw -% early_tsc_origin, tsc_frequency_hz)
        else
            null,
        .hpet => if (hpet_configured)
            hpet_epoch_ns +| math.cyclesToNanoseconds(stamp.raw -% hpet_origin_raw, hpet_frequency_hz)
        else
            null,
    };
}

pub fn elapsedNanoseconds(start: Stamp, end: Stamp) ?u64 {
    const start_ns = resolve(start) orelse return null;
    const end_ns = resolve(end) orelse return null;
    if (end_ns < start_ns) return null;
    return end_ns - start_ns;
}

pub fn elapsedSince(start: Stamp) ?u64 {
    return elapsedNanoseconds(start, capture());
}

pub fn nowNanoseconds() ?u64 {
    if (active_source == .unavailable) return null;
    return publish(currentCandidate());
}

pub fn snapshot() Snapshot {
    const event = timer.eventClockInfo();
    const source_frequency = switch (active_source) {
        .tsc => tsc_frequency_hz,
        .hpet => hpet_frequency_hz,
        .periodic_event => event.effective_hz,
        .unavailable => 0,
    };
    const resolution = switch (active_source) {
        .tsc, .hpet => math.resolutionNanoseconds(source_frequency, 1),
        .periodic_event => event.resolution_ns,
        .unavailable => 0,
    };
    var flags: u32 = 0;
    if (active_source != .unavailable) flags |= flag_valid | flag_continuous;
    switch (active_source) {
        .tsc => flags |= flag_high_resolution | flag_irq_independent | flag_invariant | flag_early_origin | flag_calibrated,
        .hpet => flags |= flag_high_resolution | flag_irq_independent | flag_calibrated,
        .periodic_event => flags |= flag_degraded,
        .unavailable => {},
    }
    return .{
        .valid = active_source != .unavailable,
        .instant_ns = nowNanoseconds() orelse 0,
        .flags = flags,
        .source = active_source,
        .generation = active_generation,
        .resolution_ns = resolution,
        .source_frequency_hz = source_frequency,
        .event_backend = timer.activeBackend(),
        .event = event,
    };
}

fn currentCandidate() u64 {
    return switch (active_source) {
        .tsc => math.cyclesToNanoseconds(readTsc() -% early_tsc_origin, tsc_frequency_hz),
        .hpet => hpet_epoch_ns +| math.cyclesToNanoseconds(
            hpet.readExtendedMainCounter() -% hpet_origin_raw,
            hpet_frequency_hz,
        ),
        .periodic_event => timer.eventNanoseconds(),
        .unavailable => timer.eventNanoseconds(),
    };
}

fn publish(candidate: u64) u64 {
    var current = @atomicLoad(u64, &published_ns, .acquire);
    while (candidate > current) {
        if (@cmpxchgWeak(u64, &published_ns, current, candidate, .acq_rel, .acquire)) |actual| {
            current = actual;
        } else {
            return candidate;
        }
    }
    return current;
}

fn exactTscFrequencyHz(info: cpu.Status) u64 {
    if (info.tsc_denominator == 0 or info.tsc_numerator == 0 or info.crystal_hz == 0) return 0;
    const frequency = (@as(u128, info.crystal_hz) * info.tsc_numerator) / info.tsc_denominator;
    if (frequency < min_plausible_tsc_hz or frequency > max_plausible_tsc_hz) return 0;
    return @intCast(frequency);
}

fn readTsc() u64 {
    var low: u32 = undefined;
    var high: u32 = undefined;
    asm volatile ("lfence");
    asm volatile ("rdtsc"
        : [low] "={eax}" (low),
          [high] "={edx}" (high),
    );
    return (@as(u64, high) << 32) | low;
}
