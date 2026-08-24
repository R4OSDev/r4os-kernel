const std = @import("std");

pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const FORMAT_FLAG_S16LE: u32 = 1 << 0;
pub const FORMAT_FLAG_U8: u32 = 1 << 1;

pub const Limits = struct {
    formats: u32 = 0,
    min_rate: u32 = 0,
    max_rate: u32 = 0,
    max_channels: u16 = 0,

    pub fn accepts(self: Limits, rate: u32, channels: u16, format: u16) bool {
        if (rate == 0 or channels == 0 or self.max_channels == 0) return false;
        if (self.min_rate != 0 and rate < self.min_rate) return false;
        if (self.max_rate != 0 and rate > self.max_rate) return false;
        if (channels > self.max_channels) return false;
        const format_flag = formatFlag(format) orelse return false;
        return (self.formats & format_flag) != 0;
    }
};

pub fn formatFlag(format: u16) ?u32 {
    return switch (format) {
        FORMAT_S16LE => FORMAT_FLAG_S16LE,
        FORMAT_U8 => FORMAT_FLAG_U8,
        else => null,
    };
}

test "audio backend limits accept only the declared write domain" {
    const limits = Limits{
        .formats = FORMAT_FLAG_S16LE | FORMAT_FLAG_U8,
        .min_rate = 8_000,
        .max_rate = 192_000,
        .max_channels = 2,
    };

    try std.testing.expect(limits.accepts(8_000, 1, FORMAT_U8));
    try std.testing.expect(limits.accepts(48_000, 2, FORMAT_S16LE));
    try std.testing.expect(limits.accepts(192_000, 2, FORMAT_S16LE));
    try std.testing.expect(!limits.accepts(1, 2, FORMAT_S16LE));
    try std.testing.expect(!limits.accepts(192_001, 2, FORMAT_S16LE));
    try std.testing.expect(!limits.accepts(48_000, 3, FORMAT_S16LE));
    try std.testing.expect(!limits.accepts(48_000, 2, 3));
}

test "audio backend format mask is enforced independently" {
    const s16_only = Limits{
        .formats = FORMAT_FLAG_S16LE,
        .min_rate = 8_000,
        .max_rate = 48_000,
        .max_channels = 2,
    };

    try std.testing.expect(s16_only.accepts(48_000, 2, FORMAT_S16LE));
    try std.testing.expect(!s16_only.accepts(48_000, 2, FORMAT_U8));
}
