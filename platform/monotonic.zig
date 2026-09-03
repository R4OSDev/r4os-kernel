const clocksource = @import("clocksource.zig");
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

var periodic_attached = false;
var periodic_was_active = false;

pub fn earlyInit() void {
    clocksource.earlyInit();
}

pub fn configureCpuClock() void {
    clocksource.configureCpuClock();
}

pub fn attachPeriodicClock() void {
    periodic_attached = true;
    if (clocksource.activeSource() == .unavailable) {
        periodic_was_active = true;
        _ = clocksource.publishExternal(timer.eventNanoseconds());
    }
}

pub fn attachHpetClock() void {
    const prior = nowNanoseconds() orelse timer.eventNanoseconds();
    clocksource.attachHpetClock(prior);
}

pub fn registerCurrentCpu(index: u32) bool {
    return clocksource.registerCurrentCpu(index);
}

pub fn finalizeCpuRegistration(index: u32) bool {
    return clocksource.finalizeCpuRegistration(index);
}

pub fn periodicValidate() bool {
    return clocksource.periodicValidate();
}

pub fn hardwareStatus() clocksource.Status {
    return clocksource.status();
}

pub fn hardwareFallbackReasonName(reason: clocksource.FallbackReason) []const u8 {
    return clocksource.fallbackReasonName(reason);
}

pub fn capture() Stamp {
    if (activeSource() == .periodic_event) {
        return .{ .raw = nowNanoseconds() orelse 0, .kind = .instant_ns, .generation = activeGeneration() };
    }
    const hardware = clocksource.capture();
    return .{
        .raw = hardware.raw,
        .kind = switch (hardware.kind) {
            .unavailable => .unavailable,
            .tsc => .tsc,
            .hpet => .hpet,
        },
        .generation = activeGeneration(),
    };
}

pub fn resolve(stamp: Stamp) ?u64 {
    return switch (stamp.kind) {
        .unavailable => null,
        .instant_ns => stamp.raw,
        .tsc => clocksource.resolve(.{ .raw = stamp.raw, .kind = .tsc, .generation = stamp.generation }),
        .hpet => clocksource.resolve(.{ .raw = stamp.raw, .kind = .hpet, .generation = stamp.generation }),
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
    if (clocksource.nowNanoseconds()) |instant| return instant;
    if (!periodic_attached) return null;
    return clocksource.publishExternal(timer.eventNanoseconds());
}

pub fn snapshot() Snapshot {
    const event = timer.eventClockInfo();
    const hardware = clocksource.status();
    const source = activeSource();
    const source_frequency = switch (source) {
        .tsc, .hpet => hardware.frequency_hz,
        .periodic_event => event.effective_hz,
        .unavailable => 0,
    };
    const resolution = switch (source) {
        .tsc, .hpet => hardware.resolution_ns,
        .periodic_event => event.resolution_ns,
        .unavailable => 0,
    };
    var flags: u32 = 0;
    if (source != .unavailable) flags |= flag_valid | flag_continuous;
    switch (source) {
        .tsc => {
            flags |= flag_high_resolution | flag_irq_independent | flag_early_origin | flag_calibrated;
            if (hardware.tsc_invariant) flags |= flag_invariant;
        },
        .hpet => flags |= flag_high_resolution | flag_irq_independent | flag_calibrated,
        .periodic_event => flags |= flag_degraded,
        .unavailable => {},
    }
    return .{
        .valid = source != .unavailable,
        .instant_ns = nowNanoseconds() orelse 0,
        .flags = flags,
        .source = source,
        .generation = activeGeneration(),
        .resolution_ns = resolution,
        .source_frequency_hz = source_frequency,
        .event_backend = timer.activeBackend(),
        .event = event,
    };
}

fn activeSource() Source {
    return switch (clocksource.activeSource()) {
        .tsc => .tsc,
        .hpet => .hpet,
        .unavailable => if (periodic_attached) .periodic_event else .unavailable,
    };
}

fn activeGeneration() u32 {
    const hardware_generation = clocksource.generation();
    if (clocksource.activeSource() != .unavailable) {
        return hardware_generation +% @as(u32, @intFromBool(periodic_was_active));
    }
    return if (periodic_was_active) @max(@as(u32, 1), hardware_generation) else hardware_generation;
}
