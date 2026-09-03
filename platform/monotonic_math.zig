const std = @import("std");

pub const nanoseconds_per_second: u64 = 1_000_000_000;
pub const Rate = struct {
    numerator: u64,
    denominator: u64,
};

/// Precomputed floor conversion for hot clock paths. Construction may divide;
/// applying the scale uses only one 64x64->128 multiply and a shift.
pub const FixedScale = struct {
    multiplier: u64 = 0,
    shift: u8 = 0,

    pub fn initFloor(numerator: u64, denominator: u64) FixedScale {
        if (numerator == 0 or denominator == 0) return .{};
        var candidate_shift: u8 = 63;
        while (true) {
            const scaled = (@as(u128, numerator) << @intCast(candidate_shift)) / denominator;
            if (scaled != 0 and scaled <= std.math.maxInt(u64)) {
                return .{ .multiplier = @intCast(scaled), .shift = candidate_shift };
            }
            if (candidate_shift == 0) return .{};
            candidate_shift -= 1;
        }
    }

    pub fn valid(self: FixedScale) bool {
        return self.multiplier != 0;
    }

    pub fn apply(self: FixedScale, value: u64) u64 {
        if (!self.valid() or value == 0) return 0;
        const result = (@as(u128, value) * self.multiplier) >> @intCast(self.shift);
        return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
    }
};

pub const HardwareClockChoice = enum {
    unavailable,
    tsc,
    hpet,
};

/// A non-invariant TSC is accepted only after an independent HPET calibration.
/// Any failed per-CPU qualification demotes the whole clock to the common HPET.
pub fn chooseHardwareClock(
    tsc_present: bool,
    tsc_frequency_valid: bool,
    invariant_tsc: bool,
    hpet_valid: bool,
    hpet_calibration_valid: bool,
    cpu_offsets_valid: bool,
) HardwareClockChoice {
    if (tsc_present and tsc_frequency_valid and cpu_offsets_valid and
        (invariant_tsc or hpet_calibration_valid)) return .tsc;
    if (hpet_valid) return .hpet;
    return .unavailable;
}

pub fn frequencyErrorPpm(measured_hz: u64, expected_hz: u64) u64 {
    if (expected_hz == 0) return std.math.maxInt(u64);
    const difference = if (measured_hz >= expected_hz)
        measured_hz - expected_hz
    else
        expected_hz - measured_hz;
    const result = (@as(u128, difference) * 1_000_000) / expected_hz;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
}

pub fn signedCounterCorrection(expected: u64, observed: u64) ?i64 {
    const difference = @as(i128, expected) - @as(i128, observed);
    if (difference < std.math.minInt(i64) or difference > std.math.maxInt(i64)) return null;
    return @intCast(difference);
}

pub fn applySignedCorrection(raw: u64, correction: i64) u64 {
    if (correction >= 0) return raw +% @as(u64, @intCast(correction));
    return raw -% @as(u64, @intCast(-@as(i128, correction)));
}

pub fn absoluteCorrection(correction: i64) u64 {
    return @intCast(if (correction >= 0) @as(i128, correction) else -@as(i128, correction));
}

/// Accepts a normal forward sample and the one legitimate 64-bit wrap, while
/// rejecting a small backwards discontinuity.
pub fn counterAdvanced(previous: u64, current: u64) bool {
    return current -% previous < (@as(u64, 1) << 63);
}

pub fn scaleFloor(value: u64, numerator: u64, denominator: u64) u64 {
    if (value == 0 or numerator == 0 or denominator == 0) return 0;
    const result = (@as(u128, value) * numerator) / denominator;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
}

pub fn scaleCeil(value: u64, numerator: u64, denominator: u64) u64 {
    if (value == 0 or numerator == 0 or denominator == 0) return 0;
    const product = @as(u128, value) * numerator;
    const result = (product + denominator - 1) / denominator;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
}

pub fn counterToTicks(counter_delta: u64, counter_frequency_hz: u64, tick_frequency_hz: u32) u64 {
    return scaleFloor(counter_delta, tick_frequency_hz, counter_frequency_hz);
}

pub fn ticksToCounterCeil(ticks: u64, counter_frequency_hz: u64, tick_frequency_hz: u32) u64 {
    return scaleCeil(ticks, counter_frequency_hz, tick_frequency_hz);
}

pub fn finiteDeadline(now: u64, duration: u64) u64 {
    const no_deadline = std.math.maxInt(u64);
    const max_finite = no_deadline - 1;
    if (duration >= max_finite -| now) return max_finite;
    return now + duration;
}

pub fn boundedDeadlineDelta(now: u64, deadline: u64, maximum: u64) u64 {
    if (maximum == 0) return 0;
    if (deadline <= now) return 1;
    return @min(deadline - now, maximum);
}

pub fn cyclesToNanoseconds(cycles: u64, frequency_hz: u64) u64 {
    return scaleFloor(cycles, nanoseconds_per_second, frequency_hz);
}

pub fn rateToNanoseconds(events: u64, numerator_hz: u64, denominator_hz: u64) u64 {
    if (numerator_hz == 0 or denominator_hz == 0) return 0;
    const scale = @as(u128, nanoseconds_per_second) * denominator_hz;
    const result = (@as(u128, events) * scale) / numerator_hz;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
}

pub fn resolutionNanoseconds(numerator_hz: u64, denominator_hz: u64) u64 {
    if (numerator_hz == 0 or denominator_hz == 0) return 0;
    const scaled = @as(u128, nanoseconds_per_second) * denominator_hz;
    const result = (scaled + numerator_hz - 1) / numerator_hz;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
}

pub fn roundedRate(numerator_hz: u64, denominator_hz: u64) u32 {
    if (numerator_hz == 0 or denominator_hz == 0) return 0;
    const result = (@as(u128, numerator_hz) + denominator_hz / 2) / denominator_hz;
    return if (result > std.math.maxInt(u32)) std.math.maxInt(u32) else @intCast(result);
}

pub fn gcd(left: u64, right: u64) u64 {
    var a = left;
    var b = right;
    while (b != 0) {
        const remainder = a % b;
        a = b;
        b = remainder;
    }
    return a;
}

pub fn reducedRate(numerator_hz: u64, denominator_hz: u64) Rate {
    if (numerator_hz == 0 or denominator_hz == 0) return .{ .numerator = 0, .denominator = 0 };
    const divisor = gcd(numerator_hz, denominator_hz);
    return .{
        .numerator = numerator_hz / divisor,
        .denominator = denominator_hz / divisor,
    };
}

pub fn extendCounter32(published: u64, low: u32) u64 {
    const low_mask: u64 = 0xFFFF_FFFF;
    const half_range: u64 = 1 << 31;
    var candidate = (published & ~low_mask) | low;
    if (candidate < published) {
        if (published - candidate > half_range) {
            candidate +|= 1 << 32;
        } else {
            return published;
        }
    } else if (candidate - published > half_range) {
        // A concurrent reader already published the wrap while this reader
        // still holds a sample from the preceding 32-bit epoch.
        return published;
    }
    return candidate;
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
