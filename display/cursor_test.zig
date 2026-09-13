//! Existing display-owner group: one asynchronous cursor lifecycle and the
//! actual common BO owner across producer death; no new test gate.
const std = @import("std");
const t = std.testing;
const model = @import("cursor_state.zig");
const sources = @import("cursor_source.zig");
const memory = @import("../memory/gfx_buffer_owner.zig");
const a = model.a;
pub fn check() !void {
    const driver: memory.Owner = .{ .kind = .driver, .id = 7, .generation = 0x100000003 };
    const caller: memory.Owner = .{ .kind = .program, .id = 81, .generation = 0x200000009 };
    const foreign: memory.Owner = .{ .kind = .program, .id = 81, .generation = 0x20000000a };
    var state: model.State = .{};
    const info: a.DisplayCursorInfo = .{ .flags = 15, .head_id = 3, .display_generation = 0x300000079,
        .backend = .{ .adapter_id = 17, .device_generation = 0x400000079, .reset_generation = 0x500000079,
            .milestone = a.gfx_queue_milestone_device_execution }, .max_width = 256, .max_height = 256,
        .min_x = -32768, .min_y = -32768, .max_x = 32767, .max_y = 32767 };
    try t.expect(!state.available());
    try state.configure(driver, info);
    var request: a.DisplayCursorRequest = .{ .display_generation = info.display_generation, .head_id = 3,
        .reference = .{ .id = 1, .generation = 2 }, .width = 32, .height = 32, .pitch = 128, .byte_length = 4096 };
    var invalid = request; invalid.hotspot_x = 32;
    try t.expectError(error.Invalid, state.validate(caller, invalid));
    invalid = request; invalid.display_generation -= 1;
    try t.expectError(error.Stale, state.validate(caller, invalid));
    try state.begin(caller, request, .{ .id = 3, .generation = 4 }, 0, 0, 10);
    try t.expectError(error.Busy, state.validate(foreign, request));
    const upload = (try state.take(driver, info.backend, 20)).?;
    try t.expect(upload.request.reference.id == 3 and upload.deadline_ns == 10 + model.operation_ns and !state.visible());
    try t.expect((try state.take(driver, info.backend, 21)) == null);
    var reply: a.GfxDriverCursorCompletion = .{ .sequence = upload.sequence, .display_generation = info.display_generation,
        .outcome = a.gfx_output_outcome_applied };
    try t.expect(try state.validateReply(driver, reply));
    state.finish(reply);
    try t.expect(!try state.validateReply(driver, reply));
    request = .{ .display_generation = info.display_generation, .head_id = 3, .operation = a.display_cursor_operation_show,
        .image_sequence = upload.sequence, .x = -17, .y = 29 };
    try state.begin(caller, request, .{}, 0x100000003, 99, 30);
    const shown = (try state.take(driver, info.backend, 31)).?;
    try t.expect(shown.barrier_point == 99 and shown.barrier_timeline == 0x100000003 and !state.visible());
    reply.sequence = shown.sequence; reply.visibility = a.display_cursor_visibility_visible;
    try t.expect(try state.validateReply(driver, reply)); state.finish(reply);
    try t.expect(state.visible() and state.status.x == -17 and state.status.y == 29);
    state.suspended = true;
    const hidden = (try state.take(driver, info.backend, 40)).?;
    try t.expect(hidden.request.operation == a.display_cursor_operation_hide and state.visible());
    reply.sequence = hidden.sequence; reply.visibility = a.display_cursor_visibility_hidden;
    try t.expect(try state.validateReply(driver, reply)); state.finish(reply);
    try t.expect(!state.visible() and state.actor.?.eql(caller) and state.status.image_sequence == upload.sequence);
    state.suspended = false;
    try state.begin(caller, request, .{}, 77, 101, 50);
    _ = try state.take(driver, info.backend, 51);
    state.markLost(a.gfx_output_error_timeout);
    try t.expect(state.job != null and state.status.flags & a.display_cursor_state_unknown != 0 and !state.available());
    // A late exact receipt resolves uncertainty; expiration itself did not.
    reply.sequence = state.job.?.sequence; reply.visibility = a.display_cursor_visibility_visible;
    try t.expect(try state.validateReply(driver, reply)); state.finish(reply);
    try t.expect(state.available() and state.visible() and state.status.flags & a.display_cursor_state_unknown == 0);
    try t.expect(state.close(caller));
    const release = (try state.take(driver, info.backend, 60)).?;
    try t.expect(release.request.operation == a.display_cursor_operation_release and state.visible());
    reply.sequence = release.sequence; reply.visibility = a.display_cursor_visibility_hidden;
    try t.expect(try state.validateReply(driver, reply)); state.finish(reply);
    try t.expect(state.actor == null and !state.visible() and state.status.image_sequence == 0);
    try checkSource(caller, driver);
}
fn checkSource(caller: memory.Owner, driver: memory.Owner) !void {
    var store = memory.Table(2, 8, 4){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const allocation = try store.begin(caller, .{ .bytes = 4096, .width = 32, .height = 32, .format = .argb8888,
        .plane_count = 1, .planes = .{.{ .pitch = 128 }, .{}, .{}, .{}} });
    try store.publish(allocation, .{ .cookie = 7, .bytes = 4096, .cpu_address = 0x100000, .cache = .write_back });
    const request: a.DisplayCursorRequest = .{ .reference = .{ .id = allocation.reference.id, .generation = allocation.reference.generation },
        .width = 32, .height = 32, .pitch = 128, .byte_length = 4096 };
    var source: sources.Source = .{};
    try source.open(&store, caller, driver, request);
    try t.expectError(error.Busy, store.use(allocation.reference, caller, .cpu_write, 0, 4096));
    store.stoppedOwner(caller);
    try t.expect(store.pendingRelease() == null and store.stats().references == 2 and store.stats().leases == 1);
    try t.expectEqual(@as(u64, 4096), (try store.describe(source.borrowed, driver)).bytes);
    try source.close(&store);
    try source.close(&store);
    try t.expect(store.stats().references == 0 and store.stats().leases == 0);
    try t.expect(store.pendingRelease() != null);
}
