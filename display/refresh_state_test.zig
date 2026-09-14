//! Additional cases in the existing output identity/lifetime group.
const std = @import("std");
const t = std.testing;
const model = @import("refresh_state.zig");
const a = model.a;
pub fn check(store: anytype, identity: a.GfxOutputId) !void {
    const output: a.GfxOutputTarget = .{ .adapter_id = identity.adapter_id, .connector_id = identity.connector_id,
        .device_generation = identity.device_generation, .connection_generation = identity.connection_generation,
        .display_generation = 19, .head_id = 0 };
    const actor: model.Actor = .{ .kind = .program, .id = 5, .generation = 1 };
    const other: model.Actor = .{ .kind = .program, .id = 6, .generation = 1 };
    try t.expectError(error.Unsupported, store.refreshAt(output));
    var value: a.GfxOutputRefresh = .{ .target = output,
        .capabilities = .{ .flags = a.gfx_refresh_cap_known | a.gfx_refresh_cap_capable, .origin = 4,
            .min_millihz = 50000, .max_millihz = 60000, .nominal_millihz = 60000,
            .min_period_ns = 16666667, .max_period_ns = 20000000, .max_vtotal = 1350 }, .status = .{ .sequence = 1 } };
    const revision = store.revision;
    try t.expectError(error.Stale, store.publishRefresh(99, value));
    try t.expect(try store.publishRefresh(14, value));
    try t.expect(store.revision == revision and (try store.refreshAt(output)).status.phase == a.gfx_refresh_phase_fixed);
    try t.expect(!try store.publishRefresh(14, value));
    var request: a.GfxRefreshRequest = .{ .target = output, .policy = a.gfx_refresh_policy_fullscreen,
        .scene = a.gfx_refresh_scene_fullscreen | a.gfx_refresh_scene_animated };
    const accepted = try store.requestRefresh(actor, request, 100);
    try t.expect(accepted.sequence == 1 and accepted.deadline_ns == 100 + model.lease_ns);
    try t.expectError(error.Busy, store.requestRefresh(other, request, 101));
    try t.expect((try store.refreshAt(output)).status.phase == a.gfx_refresh_phase_fixed);
    const renewed = try store.requestRefresh(actor, request, 102);
    try t.expect(renewed.sequence == accepted.sequence and renewed.deadline_ns > accepted.deadline_ns);
    try t.expectError(error.Stale, store.readRefresh(99, output, 103));
    try t.expect(std.meta.eql(renewed, try store.readRefresh(14, output, 103)));
    value.status = .{ .sequence = 2, .request_sequence = 1, .phase = a.gfx_refresh_phase_active, .policy = request.policy,
        .scene = request.scene, .core_point = 9, .receipt = 10, .since_ns = 104 };
    try t.expect(try store.publishRefresh(14, value));
    try t.expect((try store.refreshAt(output)).measured.samples == 0);
    value.measured = .{ .sequence = 3, .samples = 2, .observed_ns = 100000000, .last_period_ns = 20000000,
        .min_period_ns = 16000000, .max_period_ns = 20000000, .mean_period_ns = 18000000, .millihz = 55555 };
    try t.expect(!try store.publishRefresh(14, value) and store.revision == revision);
    const prior = try store.refreshAt(output);
    var invalid = value;
    invalid.measured.sequence -= 1;
    try t.expectError(error.Stale, store.publishRefresh(14, invalid));
    invalid = value; invalid.status.core_point = 0;
    try t.expectError(error.Invalid, store.publishRefresh(14, invalid));
    invalid = value; invalid.status.request_sequence = 999; invalid.target.display_generation += 1;
    try t.expectError(error.Stale, store.publishRefresh(14, invalid));
    try t.expect(std.meta.eql(prior, try store.refreshAt(output)) and std.meta.eql(renewed, try store.readRefresh(14, output, 110)));
    const expired = try store.readRefresh(14, output, renewed.deadline_ns);
    try t.expect(expired.policy == 0 and expired.scene == 0 and expired.sequence == 2 and expired.deadline_ns == 0);
    // Expiry requests disable; only the device can report its completion.
    try t.expect((try store.refreshAt(output)).status.phase == a.gfx_refresh_phase_active);
    request.operation = a.gfx_refresh_operation_flicker; request.policy = 0; request.scene = 0;
    const flicker = try store.requestRefresh(other, request, renewed.deadline_ns + 1);
    try t.expect(flicker.sequence == 3 and flicker.operation == a.gfx_refresh_operation_flicker);
    const state = &(try store.refreshEntry(output)).refresh;
    try t.expect(!state.stopped(actor) and state.stopped(other));
    try t.expect((try store.readRefresh(14, output, renewed.deadline_ns + 2)).policy == 0);
    request.operation = a.gfx_refresh_operation_configure; request.policy = 2;
    _ = try store.requestRefresh(actor, request, renewed.deadline_ns + 3);
    _ = try store.pause(14, identity, true, true);
    try t.expectError(error.Stale, store.publishRefresh(14, value));
    try t.expectError(error.Stale, store.refreshAt(output));
    try t.expect((try store.readRefresh(14, output, renewed.deadline_ns + 4)).policy == 0);
    _ = try store.pause(14, identity, false, true);
    value.status = .{ .sequence = 3, .phase = a.gfx_refresh_phase_fixed };
    value.target.display_generation += 1;
    value.measured = .{};
    try t.expect(try store.publishRefresh(14, value));
    try t.expectError(error.Stale, store.requestRefresh(actor, request, renewed.deadline_ns + 5));
    request.target = value.target; request.operation = a.gfx_refresh_operation_clear_fault; request.policy = 0;
    const cleared = try store.requestRefresh(actor, request, renewed.deadline_ns + 5);
    try t.expect(cleared.operation == a.gfx_refresh_operation_clear_fault);
    request.operation = a.gfx_refresh_operation_release;
    const released = try store.requestRefresh(actor, request, renewed.deadline_ns + 6);
    try t.expect(released.policy == 0 and released.operation == 0 and released.deadline_ns == 0 and state.actor == null);
}
