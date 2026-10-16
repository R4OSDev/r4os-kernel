const std = @import("std");
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const FORMAT_FLAG_S16LE: u32 = 1;
pub const FORMAT_FLAG_U8: u32 = 2;
pub const EndMode = enum { idle, stop };
pub const EndCallback = *const fn (?*anyopaque) callconv(.c) i32;
extern fn r4os_rust_audio_contract(op: u32, limits: ?*const Limits, rate: u32, channels: u16, format: u16, closing: bool, context: ?*anyopaque, stop: ?EndCallback, idle: ?EndCallback) i32;
pub fn finishPcm(mode: EndMode, closing: bool, context: ?*anyopaque, stop: ?EndCallback, idle: ?EndCallback) i32 {
    return r4os_rust_audio_contract(if (mode == .idle) 0 else 1, null, 0, 0, 0, closing, context, stop, idle);
}
pub const Limits = extern struct {
    formats: u32 = 0,
    min_rate: u32 = 0,
    max_rate: u32 = 0,
    max_channels: u16 = 0,
    pub fn accepts(self: @This(), rate: u32, channels: u16, format: u16) bool {
        return r4os_rust_audio_contract(2, &self, rate, channels, format, false, null, null, null) != 0;
    }
};
pub fn formatFlag(format: u16) ?u32 {
    const flag = r4os_rust_audio_contract(3, null, 0, 0, format, false, null, null, null);
    return if (flag == 0) null else @intCast(flag);
}
comptime {
    if (@sizeOf(Limits) != 16 or @offsetOf(Limits, "max_channels") != 12) @compileError("private audio limits layout");
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

test "PCM idle drains separately while switch and closing owner require physical stop" {
    const Probe = struct {
        idles: usize = 0,
        stops: usize = 0,
        result: i32 = 0,
        fn idle(raw: ?*anyopaque) callconv(.c) i32 {
            const self: *@This() = @ptrCast(@alignCast(raw.?));
            self.idles += 1;
            return self.result;
        }
        fn stop(raw: ?*anyopaque) callconv(.c) i32 {
            const self: *@This() = @ptrCast(@alignCast(raw.?));
            self.stops += 1;
            return self.result;
        }
    };
    var probe: Probe = .{};
    try std.testing.expectEqual(@as(i32, 0), finishPcm(.idle, false, &probe, Probe.stop, Probe.idle));
    try std.testing.expectEqual(@as(usize, 1), probe.idles);
    try std.testing.expectEqual(@as(usize, 0), probe.stops);
    probe.result = -6;
    try std.testing.expectEqual(@as(i32, -6), finishPcm(.idle, false, &probe, Probe.stop, Probe.idle));
    try std.testing.expectEqual(@as(usize, 0), probe.stops);
    probe.result = 0;
    _ = finishPcm(.stop, false, &probe, Probe.stop, Probe.idle);
    _ = finishPcm(.idle, true, &probe, Probe.stop, Probe.idle);
    _ = finishPcm(.idle, false, &probe, Probe.stop, null);
    try std.testing.expectEqual(@as(usize, 2), probe.idles);
    try std.testing.expectEqual(@as(usize, 3), probe.stops);
}
