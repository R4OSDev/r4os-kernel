//! Runs in the existing display takeover test group.
const t = @import("std").testing;
const a = @import("r4os_kernel_contract");
const stats = @import("presentation_stats.zig");
const state = @import("backend_state.zig");

pub fn check() !void {
    var owner: stats.Owner = .{};
    const binding: a.GfxBackendBinding = .{ .adapter_id = 7, .device_generation = 0x100000009,
        .reset_generation = 0x20000000b, .milestone = a.gfx_queue_milestone_device_execution };
    const active: state.Snapshot = .{ .state = .software_native, .owner = 92, .generation = 0x30000000d, .adapter_id = 7 };
    try t.expectError(error.Unsupported, owner.read(3, active));
    try owner.bind(92, 17, binding, active.generation);
    var value: a.DisplayPresentationStats = .{ .flags = a.display_presentation_flag_available,
        .head_id = 3, .backend = binding, .display_generation = active.generation, .sequence = 1, .buffer_count = 3,
        .acquired_count = 1, .rendered_count = 1, .pending = a.display_presentation_pending_ready };
    try owner.publish(92, 17, value, active);
    try t.expectEqualDeep(value, try owner.read(3, active));
    try t.expectEqual(@as(u64, 0), (try owner.read(3, active)).visible_count);
    try t.expectError(error.Stale, owner.publish(91, 17, value, active));
    try t.expectError(error.Stale, owner.publish(92, 16, value, active));
    var malformed = value; malformed.backend.reset_generation += 1;
    try t.expectError(error.Stale, owner.publish(92, 17, malformed, active));
    malformed = value; malformed.display_generation += 1;
    try t.expectError(error.Stale, owner.publish(92, 17, malformed, active));
    malformed = value; malformed.visible_ns = 100;
    try t.expectError(error.Invalid, owner.publish(92, 17, malformed, active));
    malformed = value; malformed.rendered_count = 2;
    try t.expectError(error.Invalid, owner.publish(92, 17, malformed, active));
    try t.expectEqualDeep(value, try owner.read(3, active));
    value.sequence += 1; value.submitted_count = 1; value.pending = a.display_presentation_pending_flip;
    try owner.publish(92, 17, value, active);
    try t.expectEqual(@as(u64, 0), (try owner.read(3, active)).visible_count);
    value.sequence += 1; value.visible_count = 1; value.visible_sequence = 1;
    value.source_timeline = 0x40000000f; value.source_point = 0x500000011;
    value.render_point = 11; value.window_point = 17; value.submitted_ns = 100;
    value.visible_ns = 300; value.irq_sequence = 9; value.irq_observed_ns = 200; value.gpu_timestamp = 0xf0000000abcd0123;
    malformed = value; malformed.irq_sequence = 0;
    try t.expectError(error.Invalid, owner.publish(92, 17, malformed, active));
    malformed = value; malformed.irq_observed_ns = 99;
    try t.expectError(error.Invalid, owner.publish(92, 17, malformed, active));
    try owner.publish(92, 17, value, active);
    try owner.publish(92, 17, value, active); // Idempotent publication retry.
    try t.expectEqual(@as(u64, 0), (try owner.read(3, active)).released_count);
    malformed = value; malformed.sequence += 1; malformed.rendered_count = 0;
    try t.expectError(error.Stale, owner.publish(92, 17, malformed, active));
    malformed = value; malformed.sequence += 1; malformed.released_count = 1; malformed.released_ns = 299;
    try t.expectError(error.Invalid, owner.publish(92, 17, malformed, active));
    value.sequence += 1; value.released_count = 1; value.released_ns = 400; value.pending = 0;
    try owner.publish(92, 17, value, active);
    try t.expectEqualDeep(value, try owner.read(3, active));
    var recovering = active; recovering.state = .recovering;
    try t.expect((try owner.read(3, recovering)).flags & a.display_presentation_flag_lost != 0);
    value.sequence += 1; value.flags |= a.display_presentation_flag_lost;
    try owner.publish(92, 17, value, recovering);
    malformed = value; malformed.sequence += 1; malformed.flags &= ~a.display_presentation_flag_lost;
    try t.expectError(error.Stale, owner.publish(92, 17, malformed, active));
    try t.expectError(error.Unsupported, owner.read(3, .{ .state = .bootfb }));
    // Head IDs are keys, not array indices; the resident store is bounded.
    for (0..a.gfx_output_max_assignments - 1) |i| {
        var other = value; other.head_id = @intCast(100 + i); other.sequence = 1;
        try owner.publish(92, 17, other, active);
    }
    malformed = value; malformed.head_id = 999;
    try t.expectError(error.Capacity, owner.publish(92, 17, malformed, active));
    var next = active; next.generation += 1;
    try owner.bind(92, 18, binding, next.generation);
    try t.expectError(error.Unsupported, owner.read(3, next));
    try t.expectError(error.Stale, owner.publish(92, 17, value, next));
    value.display_generation = next.generation; value.sequence = 1; value.size += 8;
    try owner.publish(92, 18, value, next);
    try t.expectEqual(@as(u32, @sizeOf(a.DisplayPresentationStats)), (try owner.read(3, next)).size);
    var info: a.DisplayPresentationInfo = .{ .flags = a.display_presentation_info_active | a.display_presentation_info_native |
        a.display_presentation_info_synchronized | a.display_presentation_info_visibility,
        .head_id = 3, .backend = binding, .display_generation = next.generation, .sequence = 1,
        .width = 1920, .height = 1080, .format = a.gfx_buffer_format_xrgb8888, .policies = 3,
        .buffer_count = 3, .plane_count = 1, .interval_ns = 16_666_666, .path = 1 };
    try owner.publishInfo(92, 18, info, next);
    try t.expectEqualDeep(info, try owner.readInfo(3, next));
    try t.expectError(error.Stale, owner.publishInfo(92, 17, info, next));
    var bad_info = info; bad_info.path = 2; bad_info.sequence += 1;
    try t.expectError(error.Invalid, owner.publishInfo(92, 18, bad_info, next));
    bad_info = info; bad_info.observed_ns = 100;
    try t.expectError(error.Invalid, owner.publishInfo(92, 18, bad_info, next));
    info.sequence += 1; info.observed_sequence = 5; info.observed_ns = 100;
    try owner.publishInfo(92, 18, info, next);
    bad_info = info; bad_info.sequence += 1; bad_info.observed_sequence = 4;
    try t.expectError(error.Stale, owner.publishInfo(92, 18, bad_info, next));
    try t.expectEqualDeep(info, try owner.readInfo(3, next));
    try t.expectError(error.Unsupported, owner.readInfo(0, next));
    var history: stats.Owner = .{};
    try history.bind(92, 17, binding, active.generation);
    var receipt: a.DisplayPresentationStats = .{ .flags = a.display_presentation_flag_available, .head_id = 3,
        .backend = binding, .display_generation = active.generation, .buffer_count = 3, .source_timeline = 17,
        .render_point = 1, .window_point = 1, .submitted_ns = 50 };
    for (1..35) |i| {
        receipt.sequence = i; receipt.acquired_count = i; receipt.rendered_count = i; receipt.submitted_count = i;
        receipt.visible_count = i; receipt.released_count = i - 1; receipt.visible_sequence = i;
        receipt.source_point = i; receipt.irq_sequence = i; receipt.irq_observed_ns = 90 + i; receipt.visible_ns = 100 + i;
        try history.publish(92, 17, receipt, active);
    }
    var source: a.GfxFence = .{ .slot = 1, .timeline = 17, .point = 3, .adapter_id = binding.adapter_id,
        .device_generation = binding.device_generation, .reset_generation = binding.reset_generation };
    try t.expectEqual(@as(u64, 3), (try history.feedback(3, source, active)).source_point);
    try t.expectEqual(@as(u64, 34), (try history.read(3, active)).source_point);
    source.point = 1;
    try t.expectError(error.Unsupported, history.feedback(3, source, active)); // Bounded eviction is explicit.
    source.point = 34; source.reset_generation += 1;
    try t.expectError(error.Stale, history.feedback(3, source, active));
    // A direct source has no intervening CE copy point. Its real Window,
    // head IRQ and exact queue identity remain mandatory visibility proof.
    receipt.sequence = 35; receipt.acquired_count = 35; receipt.rendered_count = 35; receipt.submitted_count = 35;
    receipt.visible_count = 35; receipt.released_count = 34; receipt.visible_sequence = 35;
    receipt.source_point = 35; receipt.irq_sequence = 35; receipt.irq_observed_ns = 125; receipt.visible_ns = 135;
    receipt.render_point = 0;
    try t.expectError(error.Invalid, history.publish(92, 17, receipt, active));
    receipt.flags |= a.display_presentation_flag_direct;
    try history.publish(92, 17, receipt, active);
    source.point = 35; source.reset_generation = binding.reset_generation;
    try t.expectEqualDeep(receipt, try history.feedback(3, source, active));
}
