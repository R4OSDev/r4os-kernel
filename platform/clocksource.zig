// Private C projection. Qualification, origins, generations and publication live in Rust.
const percpu = @import("../arch/x86_64/percpu.zig");
pub const Source = enum(u32) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
};

pub const FallbackReason = enum(u32) {
    none = 0,
    no_tsc = 1,
    non_invariant_without_reference = 2,
    frequency_unavailable = 3,
    calibration_unstable = 4,
    frequency_mismatch = 5,
    cpu_skew = 6,
    tsc_discontinuity = 7,
    hpet_unavailable = 8,
};

pub const StampKind = enum(u8) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
};

pub const Stamp = struct {
    raw: u64 = 0,
    kind: StampKind = .unavailable,
    generation: u32 = 0,
};

pub const Status = struct {
    source: Source = .unavailable,
    generation: u32 = 0,
    frequency_hz: u64 = 0,
    resolution_ns: u64 = 0,
    tsc_present: bool = false,
    tsc_invariant: bool = false,
    tsc_hpet_calibrated: bool = false,
    hpet_available: bool = false,
    registered_cpu_mask: u64 = 0,
    max_cpu_skew_ns: u64 = 0,
    calibration_error_ppm: u64 = 0,
    fallback_reason: FallbackReason = .none,
};

const RawStamp = extern struct { raw: u64, generation: u32, kind: u8 };
const RawStatus = extern struct {
    source: u32,
    generation: u32,
    frequency_hz: u64,
    resolution_ns: u64,
    flags: u32,
    fallback_reason: u32,
    registered_cpu_mask: u64,
    max_cpu_skew_ns: u64,
    calibration_error_ppm: u64,
};
extern fn r4os_rust_clocksource_control(operation: u8, value: u64) bool;
extern fn r4os_rust_clocksource_value(operation: u8, value: u64, output: *u64) bool;
extern fn r4os_rust_clocksource_status(output: *RawStatus) void;
extern fn r4os_rust_clocksource_capture(output: *RawStamp) void;
extern fn r4os_rust_clocksource_resolve(stamp: *const RawStamp, output: *u64) bool;
export fn r4os_zig_clock_cpu(operation: u8, index: u32) callconv(.c) u64 {
    return switch (operation) {
        0 => @intFromBool(percpu.state(index) == .parked),
        1 => percpu.clockQualifiedMask(),
        else => unreachable,
    };
}
fn value(operation: u8, input: u64) ?u64 {
    var result: u64 = undefined;
    return if (r4os_rust_clocksource_value(operation, input, &result)) result else null;
}
pub fn earlyInit() void {
    _ = r4os_rust_clocksource_control(0, 0);
}
pub fn configureCpuClock() void {
    _ = r4os_rust_clocksource_control(1, 0);
}
pub fn attachHpetClock(prior: u64) void {
    _ = r4os_rust_clocksource_control(2, prior);
}
pub fn registerCurrentCpu(index: u32) bool {
    return r4os_rust_clocksource_control(3, index);
}
pub fn finalizeCpuRegistration(index: u32) bool {
    return r4os_rust_clocksource_control(4, index);
}
pub fn periodicValidate() bool {
    return r4os_rust_clocksource_control(5, 0);
}
pub fn capture() Stamp {
    var raw: RawStamp = undefined;
    r4os_rust_clocksource_capture(&raw);
    return .{ .raw = raw.raw, .kind = @enumFromInt(raw.kind), .generation = raw.generation };
}
pub fn resolve(stamp: Stamp) ?u64 {
    const raw = RawStamp{ .raw = stamp.raw, .generation = stamp.generation, .kind = @intFromEnum(stamp.kind) };
    var result: u64 = undefined;
    return if (r4os_rust_clocksource_resolve(&raw, &result)) result else null;
}
pub fn nowNanoseconds() ?u64 {
    return value(0, 0);
}
pub fn publishExternal(candidate: u64) u64 {
    return value(2, candidate).?;
}
pub fn lastPublishedNanoseconds() u64 {
    return value(1, 0).?;
}
pub fn activeSource() Source {
    return @enumFromInt(@as(u32, @intCast(value(3, 0).?)));
}
pub fn generation() u32 {
    return @intCast(value(4, 0).?);
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_clocksource_status(&raw);
    return .{ .source = @enumFromInt(raw.source), .generation = raw.generation, .frequency_hz = raw.frequency_hz, .resolution_ns = raw.resolution_ns, .tsc_present = raw.flags & 1 != 0, .tsc_invariant = raw.flags & 2 != 0, .tsc_hpet_calibrated = raw.flags & 4 != 0, .hpet_available = raw.flags & 8 != 0, .registered_cpu_mask = raw.registered_cpu_mask, .max_cpu_skew_ns = raw.max_cpu_skew_ns, .calibration_error_ppm = raw.calibration_error_ppm, .fallback_reason = @enumFromInt(raw.fallback_reason) };
}
comptime {
    if (percpu.max_cpus != 32 or @sizeOf(RawStamp) != 16 or @offsetOf(RawStamp, "kind") != 12 or
        @sizeOf(RawStatus) != 56 or @offsetOf(RawStatus, "registered_cpu_mask") != 32)
        @compileError("Rust hardware clock C layout or CPU bound differs");
}
pub fn sourceName(source: Source) []const u8 {
    return switch (source) {
        .unavailable => "unavailable",
        .tsc => "TSC",
        .hpet => "HPET main counter",
    };
}

pub fn fallbackReasonName(reason: FallbackReason) []const u8 {
    return switch (reason) {
        .none => "none",
        .no_tsc => "no-tsc",
        .non_invariant_without_reference => "non-invariant-without-reference",
        .frequency_unavailable => "frequency-unavailable",
        .calibration_unstable => "calibration-unstable",
        .frequency_mismatch => "frequency-mismatch",
        .cpu_skew => "cpu-skew",
        .tsc_discontinuity => "tsc-discontinuity",
        .hpet_unavailable => "hpet-unavailable",
    };
}
