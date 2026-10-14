const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const request_version: u32 = 1;
pub const class_audio_refill: u32 = 1;
pub const max_budget_ticks: u64 = 4;
pub const Timing = struct {
    queue_ticks: u64 = 0,
    run_ticks: u64 = 0,
    lateness_ticks: u64 = 0,
    deadline_missed: bool = false,
    budget_overrun: bool = false,
};
const RawTiming = extern struct {
    queue_ticks: u64,
    run_ticks: u64,
    lateness_ticks: u64,
    deadline_missed: bool,
    budget_overrun: bool,
};
extern fn r4os_rust_driver_deadline_valid(version: u32, size: u32, required_size: u32, work_class: u32, serial_key: u64, deadline_tick: u64, budget_ticks: u64) bool;
extern fn r4os_rust_driver_deadline_analyze(submitted: u64, started: u64, completed: u64, deadline: u64, budget: u64, output: *RawTiming) void;
pub fn validRequest(version: u32, size: u32, required_size: u32, work_class: u32, serial_key: u64, deadline_tick: u64, budget_ticks: u64) bool {
    return r4os_rust_driver_deadline_valid(version, size, required_size, work_class, serial_key, deadline_tick, budget_ticks);
}
pub fn analyze(submitted_tick: u64, started_tick: u64, completed_tick: u64, deadline_tick: u64, budget_ticks: u64) Timing {
    var value: RawTiming = undefined;
    r4os_rust_driver_deadline_analyze(submitted_tick, started_tick, completed_tick, deadline_tick, budget_ticks, &value);
    return .{ .queue_ticks = value.queue_ticks, .run_ticks = value.run_ticks, .lateness_ticks = value.lateness_ticks, .deadline_missed = value.deadline_missed, .budget_overrun = value.budget_overrun };
}

test "deadline request requires class key due tick and bounded budget" {
    try std.testing.expect(validRequest(request_version, 56, 56, class_audio_refill, 0xA0, 100, 2));
    try std.testing.expect(!validRequest(request_version, 55, 56, class_audio_refill, 0xA0, 100, 2));
    try std.testing.expect(!validRequest(request_version, 56, 56, class_audio_refill, 0, 100, 2));
    try std.testing.expect(!validRequest(request_version, 56, 56, class_audio_refill, 0xA0, 100, max_budget_ticks + 1));
}

test "deadline timing distinguishes queue miss and callback budget overrun" {
    const on_time = analyze(90, 99, 101, 100, 2);
    try std.testing.expect(!on_time.deadline_missed);
    try std.testing.expect(!on_time.budget_overrun);
    try std.testing.expectEqual(@as(u64, 9), on_time.queue_ticks);

    const missed = analyze(90, 104, 109, 100, 2);
    try std.testing.expect(missed.deadline_missed);
    try std.testing.expect(missed.budget_overrun);
    try std.testing.expectEqual(@as(u64, 4), missed.lateness_ticks);
    try std.testing.expectEqual(@as(u64, 5), missed.run_ticks);
}
