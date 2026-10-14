// Private C view. Monotonic policy and mutable origins live in Rust.
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

const RawStamp = extern struct { raw: u64, generation: u32, kind: u8 };
const RawSnapshot = extern struct {
    flags: u32,
    source: u32,
    generation: u32,
    event_backend: u32,
    instant_ns: u64,
    resolution_ns: u64,
    source_frequency_hz: u64,
    event: timer.EventClockInfo,
};
extern fn r4os_rust_monotonic_control(operation: u8, value: u32) bool;
extern fn r4os_rust_monotonic_snapshot(output: *RawSnapshot) void;
extern fn r4os_rust_monotonic_now(output: *u64) bool;
extern fn r4os_rust_monotonic_capture(output: *RawStamp) void;
extern fn r4os_rust_monotonic_resolve(stamp: *const RawStamp, output: *u64) bool;
extern fn r4os_rust_monotonic_elapsed(start: *const RawStamp, end: *const RawStamp, output: *u64) bool;
fn rawStamp(stamp: Stamp) RawStamp {
    return .{ .raw = stamp.raw, .generation = stamp.generation, .kind = @intFromEnum(stamp.kind) };
}
pub fn earlyInit() void {
    _ = r4os_rust_monotonic_control(0, 0);
}
pub fn configureCpuClock() void {
    _ = r4os_rust_monotonic_control(1, 0);
}
pub fn attachPeriodicClock() void {
    _ = r4os_rust_monotonic_control(2, 0);
}
pub fn attachHpetClock() void {
    _ = r4os_rust_monotonic_control(3, 0);
}
pub fn registerCurrentCpu(index: u32) bool {
    return r4os_rust_monotonic_control(4, index);
}
pub fn finalizeCpuRegistration(index: u32) bool {
    return r4os_rust_monotonic_control(5, index);
}
pub fn periodicValidate() bool {
    return r4os_rust_monotonic_control(6, 0);
}
pub fn hardwareStatus() clocksource.Status {
    return clocksource.status();
}
pub fn hardwareFallbackReasonName(reason: clocksource.FallbackReason) []const u8 {
    return clocksource.fallbackReasonName(reason);
}
pub fn capture() Stamp {
    var raw: RawStamp = undefined;
    r4os_rust_monotonic_capture(&raw);
    return .{ .raw = raw.raw, .kind = @enumFromInt(raw.kind), .generation = raw.generation };
}
pub fn resolve(stamp: Stamp) ?u64 {
    const raw = rawStamp(stamp);
    var output: u64 = undefined;
    return if (r4os_rust_monotonic_resolve(&raw, &output)) output else null;
}
pub fn elapsedNanoseconds(start: Stamp, end: Stamp) ?u64 {
    const first = rawStamp(start);
    const last = rawStamp(end);
    var output: u64 = undefined;
    return if (r4os_rust_monotonic_elapsed(&first, &last, &output)) output else null;
}
pub fn elapsedSince(start: Stamp) ?u64 {
    return elapsedNanoseconds(start, capture());
}
pub fn nowNanoseconds() ?u64 {
    var output: u64 = undefined;
    return if (r4os_rust_monotonic_now(&output)) output else null;
}
pub fn snapshot() Snapshot {
    var raw: RawSnapshot = undefined;
    r4os_rust_monotonic_snapshot(&raw);
    return .{ .valid = raw.flags & flag_valid != 0, .instant_ns = raw.instant_ns, .flags = raw.flags, .source = @enumFromInt(raw.source), .generation = raw.generation, .resolution_ns = raw.resolution_ns, .source_frequency_hz = raw.source_frequency_hz, .event_backend = @enumFromInt(raw.event_backend), .event = raw.event };
}
comptime {
    if (@sizeOf(RawStamp) != 16 or @offsetOf(RawStamp, "kind") != 12 or @sizeOf(RawSnapshot) != 72 or @offsetOf(RawSnapshot, "event") != 40)
        @compileError("Rust monotonic C layout differs");
}
