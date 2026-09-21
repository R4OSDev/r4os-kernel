//! Cases in the existing output lifecycle group; no extra gate or guest run.
const std = @import("std");
const t = std.testing;
const model = @import("output_state.zig");
const a = model.abi;
pub fn check(store: *model.Store, identity: a.GfxOutputId) !void {
    const actor: model.DriverOwner = .{ .kind = .program, .id = 17, .generation = 3 };
    const other: model.DriverOwner = .{ .kind = .program, .id = 17, .generation = 4 };
    var value: a.GfxOutputPower = .{ .identity = identity, .capabilities = 3, .sequence = 1,
        .since_ns = 100, .core_point = 0x100000079, .window_point = 0x200000079 };
    try t.expectError(error.Unsupported, store.powerAt(identity));
    try t.expectError(error.Stale, store.publishPower(15, value));
    try t.expect(try store.publishPower(14, value));
    const request = try store.requestPower(actor, .{ .identity = identity, .off = 1 }, 200);
    try t.expect(request.sequence != 0 and request.deadline_ns == 200 + @import("power_state.zig").lease_ns);
    try t.expect((try store.powerAt(identity)).phase == a.gfx_power_phase_on); // Intent is not a receipt.
    try t.expectError(error.Busy, store.requestPower(other, .{ .identity = identity, .off = 1 }, 201));
    const renew = try store.requestPower(actor, .{ .identity = identity, .off = 1 }, 300);
    try t.expect(renew.sequence == request.sequence and renew.deadline_ns > request.deadline_ns);
    value.phase = a.gfx_power_phase_stopping; value.sequence += 1; value.request_sequence = renew.sequence;
    try t.expect(try store.publishPower(14, value));
    try t.expect(store.infoAt(0).?.flags & a.gfx_output_flag_sleeping != 0);
    value.phase = a.gfx_power_phase_off; value.sequence += 1;
    try t.expectError(error.Stale, store.publishPower(14, value));
    _ = try store.pause(14, identity, true, true);
    try t.expectError(error.Invalid, store.publishPower(14, value)); // No sink completion.
    value.control_receipt = 0x300000079;
    try t.expect(try store.publishPower(14, value));
    const state = &(try store.powerEntry(identity)).power;
    try t.expect(!state.stopped(other) and state.stopped(actor));
    const closed = try store.readPower(14, identity, 301);
    try t.expect(closed.off == 0 and closed.sequence > renew.sequence and state.actor == null);
    const again = try store.requestPower(other, .{ .identity = identity, .off = 1 }, 302);
    const expired = try store.readPower(14, identity, again.deadline_ns);
    try t.expect(expired.off == 0 and expired.sequence > again.sequence);
    _ = try store.requestPower(actor, .{ .identity = identity, .off = 1 }, again.deadline_ns + 1);
    const wake = try store.requestPower(other, .{ .identity = identity }, again.deadline_ns + 2);
    try t.expect(wake.off == 0 and state.actor == null);
    var stale = identity; stale.connection_generation += 1;
    try t.expectError(error.Stale, store.requestPower(actor, .{ .identity = stale, .off = 1 }, again.deadline_ns + 3));
    value.sequence += 1; value.phase = a.gfx_power_phase_waking; value.request_sequence = wake.sequence;
    try t.expect(try store.publishPower(14, value));
    var invalid = value; invalid.sequence += 1; invalid.reserved0 = 1;
    try t.expectError(error.Invalid, store.publishPower(14, invalid));
    try t.expectEqualDeep(value, try store.powerAt(identity));
    _ = try store.pause(14, identity, false, true);
    value.sequence += 1; value.phase = a.gfx_power_phase_on;
    try t.expect(try store.publishPower(14, value));
    try t.expect(store.infoAt(0).?.flags & a.gfx_output_flag_sleeping == 0);
    try brightness(store, identity, actor);
}
fn brightness(store: *model.Store, identity: a.GfxOutputId, actor: model.DriverOwner) !void {
    var value: a.GfxOutputBrightness = .{ .identity = identity, .path = a.gfx_brightness_path_aux16,
        .phase = a.gfx_brightness_phase_ready, .minimum = 512, .maximum = 65535, .current = 32768,
        .flags = a.gfx_brightness_flag_current_known, .sequence = 1, .since_ns = 100 };
    try t.expectError(error.Unsupported, store.brightnessAt(identity));
    try t.expectError(error.Stale, store.publishBrightness(15, value));
    try t.expect(try store.publishBrightness(14, value));
    const request = try store.requestBrightness(actor, .{ .identity = identity, .level = 50000 });
    try t.expect(request.sequence != 0 and (try store.brightnessAt(identity)).current == 32768);
    try t.expectEqualDeep(request, try store.readBrightness(14, identity));
    try t.expectError(error.Stale, store.readBrightness(15, identity));
    try t.expectError(error.Invalid, store.requestBrightness(actor, .{ .identity = identity, .level = 511 }));
    var stale = identity; stale.connection_generation += 1;
    try t.expectError(error.Stale, store.requestBrightness(actor, .{ .identity = stale, .level = 40000 }));
    var invalid = value; invalid.sequence += 1; invalid.request_sequence = request.sequence + 1;
    try t.expectError(error.Stale, store.publishBrightness(14, invalid));
    value.sequence += 1; value.request_sequence = request.sequence; value.phase = a.gfx_brightness_phase_failed;
    value.reason = a.gfx_brightness_reason_io; value.flags = 0;
    try t.expect(try store.publishBrightness(14, value));
    try t.expect(!(try store.publishBrightness(14, value)));
    try t.expectError(error.Stale, store.publishBrightness(14, .{ .identity = identity, .sequence = 1, .since_ns = 100,
        .path = 1, .phase = 1, .maximum = 65535, .flags = 1 }));
    const retry = try store.requestBrightness(actor, .{ .identity = identity, .level = request.level });
    try t.expect(retry.sequence > request.sequence);
    _ = try store.pause(14, identity, true, true);
    try t.expectError(error.Busy, store.requestBrightness(actor, .{ .identity = identity, .level = 20000 }));
    _ = try store.pause(14, identity, false, true);
    value.sequence += 1; value.request_sequence = retry.sequence; value.phase = a.gfx_brightness_phase_ready;
    value.current = retry.level; value.reason = 0; value.flags = 1;
    try t.expect(try store.publishBrightness(14, value));
    try t.expectEqualDeep(value, try store.brightnessAt(identity));
}
