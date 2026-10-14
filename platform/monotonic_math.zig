const std = @import("std");
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}

extern fn r4os_rust_clock_math(operation: u8, a: u64, b: u64, c: u64) u64;
extern fn r4os_rust_clock_correction(expected: u64, observed: u64, output: *i64) bool;
extern fn r4os_rust_clock_fixed(operation: u8, value: *FixedScale, a: u64, b: u64) u64;
extern fn r4os_rust_clock_rate(a: u64, b: u64, output: *Rate) void;
extern fn r4os_rust_clock_durations(operation: u8, value: *RawDurations, a: u64, b: u64, flags: u8) bool;

pub const nanoseconds_per_second: u64 = 1_000_000_000;
pub const Rate = extern struct { numerator: u64, denominator: u64 };
const RawDurations = extern struct {
    first: u64,
    last: u64,
    total: u64,
    unavailable: u32,
    has_first: bool,
    has_last: bool,
};

/// Native option projection only; Rust owns all interval admission/accounting.
pub const OrderedDurations = struct {
    first: ?u64 = null,
    last: ?u64 = null,
    total: u64 = 0,
    unavailable: u32 = 0,
    fn raw(self: OrderedDurations) RawDurations {
        return .{ .first = self.first orelse 0, .last = self.last orelse 0, .total = self.total, .unavailable = self.unavailable, .has_first = self.first != null, .has_last = self.last != null };
    }
    pub fn add(self: *OrderedDurations, start: ?u64, end: ?u64) void {
        var state = self.raw();
        const flags: u8 = @as(u8, @intFromBool(start != null)) | (@as(u8, @intFromBool(end != null)) << 1);
        _ = r4os_rust_clock_durations(0, &state, start orelse 0, end orelse 0, flags);
        self.first = if (state.has_first) state.first else null;
        self.last = if (state.has_last) state.last else null;
        self.total = state.total;
        self.unavailable = state.unavailable;
    }
    pub fn valid(self: OrderedDurations) bool {
        var state = self.raw();
        return r4os_rust_clock_durations(1, &state, 0, 0, 0);
    }
};

pub const FixedScale = extern struct {
    multiplier: u64 = 0,
    shift: u8 = 0,
    pub fn initFloor(numerator: u64, denominator: u64) FixedScale {
        var result: FixedScale = undefined;
        _ = r4os_rust_clock_fixed(0, &result, numerator, denominator);
        return result;
    }
    pub fn valid(self: FixedScale) bool {
        var value = self;
        return r4os_rust_clock_fixed(1, &value, 0, 0) != 0;
    }
    pub fn apply(self: FixedScale, value: u64) u64 {
        var scale = self;
        return r4os_rust_clock_fixed(2, &scale, value, 0);
    }
};

pub const HardwareClockChoice = enum { unavailable, tsc, hpet };
pub fn chooseHardwareClock(tsc_present: bool, tsc_frequency_valid: bool, invariant_tsc: bool, hpet_valid: bool, hpet_calibration_valid: bool, cpu_offsets_valid: bool) HardwareClockChoice {
    const flags: u8 = @as(u8, @intFromBool(tsc_present)) |
        (@as(u8, @intFromBool(tsc_frequency_valid)) << 1) |
        (@as(u8, @intFromBool(invariant_tsc)) << 2) |
        (@as(u8, @intFromBool(hpet_valid)) << 3) |
        (@as(u8, @intFromBool(hpet_calibration_valid)) << 4) |
        (@as(u8, @intFromBool(cpu_offsets_valid)) << 5);
    return @enumFromInt(r4os_rust_clock_math(13, flags, 0, 0));
}
pub fn frequencyErrorPpm(measured_hz: u64, expected_hz: u64) u64 {
    return r4os_rust_clock_math(2, measured_hz, expected_hz, 0);
}
pub fn signedCounterCorrection(expected: u64, observed: u64) ?i64 {
    var result: i64 = 0;
    return if (r4os_rust_clock_correction(expected, observed, &result)) result else null;
}
pub fn applySignedCorrection(raw: u64, correction: i64) u64 {
    return r4os_rust_clock_math(3, raw, @bitCast(correction), 0);
}
pub fn absoluteCorrection(correction: i64) u64 {
    return r4os_rust_clock_math(4, @bitCast(correction), 0, 0);
}
pub fn counterAdvanced(previous: u64, current: u64) bool {
    return r4os_rust_clock_math(5, previous, current, 0) != 0;
}
pub fn scaleFloor(value: u64, numerator: u64, denominator: u64) u64 {
    return r4os_rust_clock_math(0, value, numerator, denominator);
}
pub fn scaleCeil(value: u64, numerator: u64, denominator: u64) u64 {
    return r4os_rust_clock_math(1, value, numerator, denominator);
}
pub fn counterToTicks(counter_delta: u64, counter_frequency_hz: u64, tick_frequency_hz: u32) u64 {
    return scaleFloor(counter_delta, tick_frequency_hz, counter_frequency_hz);
}
pub fn ticksToCounterCeil(ticks: u64, counter_frequency_hz: u64, tick_frequency_hz: u32) u64 {
    return scaleCeil(ticks, counter_frequency_hz, tick_frequency_hz);
}
pub fn finiteDeadline(now: u64, duration: u64) u64 {
    return r4os_rust_clock_math(6, now, duration, 0);
}
pub fn boundedDeadlineDelta(now: u64, deadline: u64, maximum: u64) u64 {
    return r4os_rust_clock_math(7, now, deadline, maximum);
}
pub fn cyclesToNanoseconds(cycles: u64, frequency_hz: u64) u64 {
    return scaleFloor(cycles, nanoseconds_per_second, frequency_hz);
}
pub fn rateToNanoseconds(events: u64, numerator_hz: u64, denominator_hz: u64) u64 {
    return r4os_rust_clock_math(8, events, numerator_hz, denominator_hz);
}
pub fn resolutionNanoseconds(numerator_hz: u64, denominator_hz: u64) u64 {
    return r4os_rust_clock_math(9, numerator_hz, denominator_hz, 0);
}
pub fn roundedRate(numerator_hz: u64, denominator_hz: u64) u32 {
    return @intCast(r4os_rust_clock_math(10, numerator_hz, denominator_hz, 0));
}
pub fn gcd(left: u64, right: u64) u64 {
    return r4os_rust_clock_math(11, left, right, 0);
}
pub fn reducedRate(numerator_hz: u64, denominator_hz: u64) Rate {
    var result: Rate = undefined;
    r4os_rust_clock_rate(numerator_hz, denominator_hz, &result);
    return result;
}
pub fn extendCounter32(published: u64, low: u32) u64 {
    return r4os_rust_clock_math(12, published, low, 0);
}
comptime {
    if (@sizeOf(Rate) != 16 or @sizeOf(FixedScale) != 16 or @offsetOf(FixedScale, "shift") != 8 or
        @sizeOf(RawDurations) != 32 or @offsetOf(RawDurations, "has_first") != 28)
        @compileError("Rust clock-math C layouts differ");
}

test "cycle conversion keeps sub-millisecond spans" {
    try std.testing.expectEqual(@as(u64, 250), cyclesToNanoseconds(750, 3_000_000_000));
    try std.testing.expectEqual(@as(u64, 1_000_000), cyclesToNanoseconds(3_000_000, 3_000_000_000));
}

test "fixed scale tracks exact floor without runtime division" {
    const scale = FixedScale.initFloor(nanoseconds_per_second, 3_000_000_000);
    try std.testing.expect(scale.valid());
    const samples = [_]u64{ 1, 750, 3_000_000, 3_000_000_000, 9_000_000_000_000 };
    for (samples) |sample| {
        const exact = cyclesToNanoseconds(sample, 3_000_000_000);
        const fixed = scale.apply(sample);
        try std.testing.expect(fixed <= exact);
        try std.testing.expect(exact - fixed <= 1);
    }
}

test "clock qualification falls back instead of trusting an unsafe TSC" {
    try std.testing.expectEqual(HardwareClockChoice.tsc, chooseHardwareClock(true, true, true, false, false, true));
    try std.testing.expectEqual(HardwareClockChoice.tsc, chooseHardwareClock(true, true, false, true, true, true));
    try std.testing.expectEqual(HardwareClockChoice.hpet, chooseHardwareClock(true, true, false, true, false, true));
    try std.testing.expectEqual(HardwareClockChoice.hpet, chooseHardwareClock(true, true, true, true, true, false));
    try std.testing.expectEqual(HardwareClockChoice.unavailable, chooseHardwareClock(false, false, false, false, false, false));
}

test "frequency error and signed CPU correction are bounded" {
    try std.testing.expectEqual(@as(u64, 500), frequencyErrorPpm(2_398_800_000, 2_400_000_000));
    const correction = signedCounterCorrection(1_000_000, 1_000_125) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, -125), correction);
    try std.testing.expectEqual(@as(u64, 1_000_000), applySignedCorrection(1_000_125, correction));
    try std.testing.expectEqual(@as(u64, 125), absoluteCorrection(correction));
}

test "counter progress distinguishes wrap from a backwards jump" {
    try std.testing.expect(counterAdvanced(0xFFFF_FFFF_FFFF_FFF0, 0x20));
    try std.testing.expect(counterAdvanced(10, 11));
    try std.testing.expect(!counterAdvanced(1000, 900));
}

test "event rate preserves effective rational frequency" {
    try std.testing.expectEqual(@as(u64, 999_847), rateToNanoseconds(1, 1_193_182, 1193));
    try std.testing.expectEqual(@as(u32, 1000), roundedRate(1_193_182, 1193));
    try std.testing.expectEqual(@as(u64, 999_848), resolutionNanoseconds(1_193_182, 1193));
}

test "conversion saturates instead of wrapping" {
    try std.testing.expectEqual(std.math.maxInt(u64), scaleFloor(std.math.maxInt(u64), std.math.maxInt(u64), 1));
    try std.testing.expectEqual(std.math.maxInt(u64), scaleCeil(std.math.maxInt(u64), std.math.maxInt(u64), 1));
}

test "deadline conversion preserves a never-early one-shot boundary" {
    try std.testing.expectEqual(@as(u64, 3), counterToTicks(30_000, 10_000_000, 1000));
    try std.testing.expectEqual(@as(u64, 30_000), ticksToCounterCeil(3, 10_000_000, 1000));
    try std.testing.expectEqual(@as(u64, 1), ticksToCounterCeil(1, 1, 1000));
}

test "finite deadlines saturate below the no-deadline sentinel" {
    const no_deadline = std.math.maxInt(u64);
    try std.testing.expectEqual(@as(u64, 125), finiteDeadline(100, 25));
    try std.testing.expectEqual(no_deadline - 1, finiteDeadline(no_deadline - 10, 20));
    try std.testing.expectEqual(@as(u64, 1), boundedDeadlineDelta(100, 99, 50));
    try std.testing.expectEqual(@as(u64, 50), boundedDeadlineDelta(100, 1000, 50));
}

test "rates are reduced without changing their value" {
    const rate = reducedRate(48_000_000, 48_000);
    try std.testing.expectEqual(@as(u64, 1000), rate.numerator);
    try std.testing.expectEqual(@as(u64, 1), rate.denominator);
}

test "32-bit counter extension is monotonic across wrap and stale readers" {
    const before_wrap: u64 = 0x0000_0000_FFFF_FFF0;
    const after_wrap = extendCounter32(before_wrap, 0x0000_0010);
    try std.testing.expectEqual(@as(u64, 0x0000_0001_0000_0010), after_wrap);
    try std.testing.expectEqual(after_wrap, extendCounter32(after_wrap, 0xFFFF_FFF8));
    try std.testing.expectEqual(after_wrap, extendCounter32(after_wrap, 0x0000_0008));
    try std.testing.expectEqual(@as(u64, 0x0000_0001_0000_0020), extendCounter32(after_wrap, 0x0000_0020));
}

test "ordered durations reject a later boot interval before the earlier origin" {
    var timing: OrderedDurations = .{};
    timing.add(475795, 88627501);
    timing.add(0, 0);
    try std.testing.expect(!timing.valid());
    try std.testing.expectEqual(@as(u32, 1), timing.unavailable);
    try std.testing.expectEqual(@as(?u64, 88627501), timing.last);
    try std.testing.expectEqual(@as(u64, 88151706), timing.total);
}

test "ordered durations retain disjoint intervals and unavailable spans" {
    var timing: OrderedDurations = .{};
    timing.add(0, 0);
    timing.add(10, 30);
    timing.add(30, 40);
    try std.testing.expect(timing.valid());
    try std.testing.expectEqual(@as(u64, 30), timing.total);
    timing.add(null, 50);
    timing.add(60, 55);
    try std.testing.expectEqual(@as(u32, 2), timing.unavailable);
    try std.testing.expect(!timing.valid());
}
