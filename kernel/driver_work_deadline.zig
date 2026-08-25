const std = @import("std");

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

pub fn validRequest(
    version: u32,
    size: u32,
    required_size: u32,
    work_class: u32,
    serial_key: u64,
    deadline_tick: u64,
    budget_ticks: u64,
) bool {
    return version == request_version and
        size >= required_size and
        work_class == class_audio_refill and
        serial_key != 0 and
        deadline_tick != 0 and
        budget_ticks != 0 and
        budget_ticks <= max_budget_ticks;
}

pub fn analyze(submitted_tick: u64, started_tick: u64, completed_tick: u64, deadline_tick: u64, budget_ticks: u64) Timing {
    const queue_ticks = elapsed(submitted_tick, started_tick);
    const run_ticks = elapsed(started_tick, completed_tick);
    const lateness_ticks = elapsed(deadline_tick, started_tick);
    return .{
        .queue_ticks = queue_ticks,
        .run_ticks = run_ticks,
        .lateness_ticks = lateness_ticks,
        .deadline_missed = started_tick > deadline_tick,
        .budget_overrun = run_ticks > budget_ticks,
    };
}

fn elapsed(start: u64, end: u64) u64 {
    return if (end >= start) end - start else 0;
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
