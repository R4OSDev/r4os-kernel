// Scenarios inside the existing display owner group; no additional gate.
const std = @import("std");
const m = @import("output_runtime_state.zig");
const a = m.abi;
const target = @import("output_target.zig");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
pub fn run() !void {
    const t = std.testing;
    var state: m.Store = .{};
    const driver: lifetime.Owner = .{ .kind = .driver, .id = 3, .generation = 4 };
    var registration: a.GfxAdditionalOutput = .{ .backend = .{ .adapter_id = 7, .device_generation = 9, .reset_generation = 2,
        .milestone = a.gfx_queue_milestone_device_execution }, .output = .{ .adapter_id = 7, .connector_id = 2,
        .device_generation = 9, .connection_generation = 12 }, .head_id = 1, .width = 1920, .height = 1080,
        .format = a.gfx_buffer_format_xrgb8888 };
    var short = registration; short.job_size = 224;
    try t.expectError(error.Invalid, state.register(driver, short));
    const first = try state.register(driver, registration);
    try t.expect(target.valid(first));
    const entry = try state.find(first);
    try t.expect(state.at(7, 1) == null);
    try t.expect((try entry.info()).flags & a.display_presentation_info_occluded != 0);
    entry.active = true;
    var sample = try entry.info();
    sample.sequence += 1; sample.interval_ns = 16_666_667;
    try entry.publishInfo(driver, sample);
    registration.head_id = 2; registration.output.connector_id = 3;
    registration.width = 1080; registration.height = 1920;
    const second = try state.register(driver, registration);
    const other = try state.find(second);
    other.active = true;
    var second_info = try other.info();
    second_info.sequence += 1; second_info.interval_ns = 10_000_000;
    try other.publishInfo(driver, second_info);
    try t.expect(first.display_generation != second.display_generation);
    try t.expectEqual(@as(u64, 16_666_667), (try entry.info()).interval_ns);
    try t.expectEqual(@as(u64, 10_000_000), (try other.info()).interval_ns);
    entry.active = false;
    try t.expect(state.at(7, 1) == null and state.at(7, 2) == other);
    try t.expectEqual(@as(u64, 10_000_000), (try other.info()).interval_ns);
    try t.expectError(error.Busy, state.register(driver, registration));
    var stale = second; stale.connection_generation += 1;
    try t.expectError(error.Stale, state.find(stale));
    stale = second; stale.adapter_id = 0;
    try t.expectError(error.Stale, state.find(stale));
    stale = second; stale.head_id = 1;
    try t.expectError(error.Stale, state.find(stale));
    stale = second; stale.display_generation = first.display_generation;
    try t.expectError(error.Stale, state.find(stale));
    stale = second; stale.connector_id = 0;
    try t.expectError(error.Invalid, state.find(stale));
    try t.expectError(error.Stale, other.publishInfo(.{ .kind = .driver, .id = 3, .generation = 5 }, second_info));
    // Publishing a checked mode catalog may replace receiver identity while
    // the physical head stays retained. Old targets cannot submit to it.
    const held = other.target;
    registration.output.connection_generation += 1;
    try t.expectError(error.Busy, state.register(driver, registration));
    other.active = false;
    other.mode_blocked = true;
    try t.expectError(error.Busy, state.register(driver, registration));
    other.mode_blocked = false;
    const rebound = try state.register(driver, registration);
    try t.expect(rebound.display_generation == held.display_generation and rebound.connection_generation != held.connection_generation);
    try t.expectError(error.Stale, state.find(held));
    try t.expect(try state.find(rebound) == other);
    try t.expect((try other.info()).flags & a.display_presentation_info_occluded != 0);
    try t.expectEqual(@as(u64, 16_666_667), (try entry.info()).interval_ns);
    try t.expectEqual(@as(u64, 10_000_000), (try other.info()).interval_ns);
    other.mode_lost = true;
    try t.expect((try other.info()).flags & a.display_presentation_info_lost != 0);
    try t.expect((try entry.info()).flags & a.display_presentation_info_lost == 0);
    other.removing = true;
    try t.expectError(error.Busy, state.register(driver, registration));
    state.stop(3);
    try t.expectError(error.Stale, state.find(second));
    const replacement = try state.register(driver, registration);
    try t.expect(replacement.display_generation > second.display_generation);
    try t.expectError(error.Stale, state.find(second));
    // Same hardware head numbers on a different adapter remain distinct.
    registration.backend.adapter_id = 8; registration.output.adapter_id = 8;
    const third = try state.register(.{ .kind = .driver, .id = 4, .generation = 1 }, registration);
    try t.expect(!target.same(third, replacement));
}
