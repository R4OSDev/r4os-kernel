const std = @import("std");

pub const nanoseconds_per_second: u64 = 1_000_000_000;
pub const Rate = struct {
    numerator: u64,
    denominator: u64,
};

pub fn scaleFloor(value: u64, numerator: u64, denominator: u64) u64 {
    if (value == 0 or numerator == 0 or denominator == 0) return 0;
    const result = (@as(u128, value) * numerator) / denominator;
    return if (result > std.math.maxInt(u64)) std.math.maxInt(u64) else @intCast(result);
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

test "event rate preserves effective rational frequency" {
    try std.testing.expectEqual(@as(u64, 999_847), rateToNanoseconds(1, 1_193_182, 1193));
    try std.testing.expectEqual(@as(u32, 1000), roundedRate(1_193_182, 1193));
    try std.testing.expectEqual(@as(u64, 999_848), resolutionNanoseconds(1_193_182, 1193));
}

test "conversion saturates instead of wrapping" {
    try std.testing.expectEqual(std.math.maxInt(u64), scaleFloor(std.math.maxInt(u64), std.math.maxInt(u64), 1));
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
