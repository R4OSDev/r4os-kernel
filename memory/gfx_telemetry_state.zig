//! Private fixed C facade. Native cache/demand lives only in these resident rows.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
const std = @import("std");
const a = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
pub const Error = bo.Error || error{Unavailable};
pub const max_age_ns: u64 = 3 * std.time.ns_per_s;
pub const demand_ns: u64 = 10 * std.time.ns_per_s;
const Identity = extern struct { kind: u32 = 0, id: u64 = 0, generation: u64 = 0 };
const Entry = extern struct { has_owner: bool = false, owner: Identity = .{}, data: a.GfxTelemetryState = .{}, demand: a.GfxTelemetryDemand = .{} };
extern fn r4os_rust_gfx_telemetry_validate(*const a.GfxTelemetryState, u64) i32;
extern fn r4os_rust_gfx_telemetry_publish([*]Entry, usize, *const Identity, *const a.GfxTelemetryState, u64, *a.GfxTelemetryDemand) i32;
extern fn r4os_rust_gfx_telemetry_query([*]Entry, usize, *const a.GfxTelemetryRequest, u64, *a.GfxTelemetryState) i32;
extern fn r4os_rust_gfx_telemetry_close([*]Entry, usize, u32) void;
fn status(code: i32) Error!void {
    return switch (code) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        12 => error.Unavailable,
        else => unreachable,
    };
}
pub fn validate(input: *const a.GfxTelemetryState, now: u64) Error!void {
    try status(r4os_rust_gfx_telemetry_validate(input, now));
}
pub fn Store(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = @splat(.{}),
        pub fn publish(self: *Self, owner: bo.Owner, input: *const a.GfxTelemetryState, now: u64) Error!a.GfxTelemetryDemand {
            const id = Identity{ .kind = @intFromEnum(owner.kind), .id = owner.id, .generation = owner.generation };
            var out: a.GfxTelemetryDemand = undefined;
            try status(r4os_rust_gfx_telemetry_publish(self.entries[0..].ptr, capacity, &id, input, now, &out));
            return out;
        }
        pub fn query(self: *Self, request: a.GfxTelemetryRequest, now: u64) Error!a.GfxTelemetryState {
            var out: a.GfxTelemetryState = undefined;
            try status(r4os_rust_gfx_telemetry_query(self.entries[0..].ptr, capacity, &request, now, &out));
            return out;
        }
        pub fn closeDriver(self: *Self, id: u32) void {
            r4os_rust_gfx_telemetry_close(self.entries[0..].ptr, capacity, id);
        }
    };
}
comptime {
    if (@sizeOf(Identity) != 24 or @alignOf(Identity) != 8 or @offsetOf(Identity, "kind") != 0 or @offsetOf(Identity, "id") != 8 or @offsetOf(Identity, "generation") != 16) @compileError("Native graphics identity C layout");
    if (@sizeOf(Entry) != 616 or @alignOf(Entry) != 8 or @offsetOf(Entry, "has_owner") != 0 or @offsetOf(Entry, "owner") != 8 or @offsetOf(Entry, "data") != 32 or @offsetOf(Entry, "demand") != 576) @compileError("Native graphics telemetry entry C layout");
}

test "common telemetry demand is finite and generation-bound, stale values are not measurements" {
    const t = std.testing;
    var state: Store(1) = .{};
    const owner: bo.Owner = .{ .kind = .driver, .id = 7, .generation = 8 };
    var input: a.GfxTelemetryState = .{ .adapter_id = 3, .memory_generation = 9, .sampled_ns = 100, .valid_until_ns = 200, .source = 1, .state = 1 };
    input.metrics[5] = .{ .status = a.gfx_telemetry_fresh, .source_stamp = 12, .values = .{ -12500, 0, 0, 0 } };
    try t.expect((try state.publish(owner, &input, 100)).metric_mask == 0);
    var request: a.GfxTelemetryRequest = .{ .adapter_id = 3, .memory_generation = 9, .metric_mask = 32 };
    try t.expect((try state.query(request, 110)).metrics[5].values[0] == -12500);
    request.metric_mask = 64;
    _ = try state.query(request, 120);
    const pending = try state.publish(owner, &input, 120);
    try t.expect(pending.metric_mask == 96 and pending.until_ns == 120 + demand_ns);
    request.metric_mask = 0;
    const expired = try state.query(request, 200);
    try t.expect(expired.metrics[5].status == a.gfx_telemetry_stale and expired.metrics[5].values[0] == 0 and expired.metrics[6].status == a.gfx_telemetry_unavailable);
    request.memory_generation += 1;
    try t.expectError(error.Stale, state.query(request, 200));
    try t.expectError(error.WrongOwner, state.publish(.{ .kind = .driver, .id = 8, .generation = 8 }, &input, 200));
    input.sampled_ns = 201;
    input.valid_until_ns = 301;
    try t.expectError(error.Invalid, state.publish(owner, &input, 200));
    input.sampled_ns = 120 + demand_ns;
    input.valid_until_ns = input.sampled_ns;
    try t.expect((try state.publish(owner, &input, input.sampled_ns)).metric_mask == 0);
    request.memory_generation = input.memory_generation;
    request.metric_mask = 32;
    _ = try state.query(request, input.sampled_ns);
    const old = input;
    input.memory_generation += 1;
    input.metrics = @splat(.{});
    const rebuilt = try state.publish(owner, &input, input.sampled_ns);
    try t.expect(rebuilt.metric_mask == 0 and rebuilt.until_ns == 0 and rebuilt.memory_generation == input.memory_generation);
    try t.expectError(error.Stale, state.query(request, input.sampled_ns));
    try t.expectError(error.Stale, state.publish(owner, &old, input.sampled_ns));
    request.memory_generation = input.memory_generation;
    try t.expect((try state.query(request, input.sampled_ns)).metrics[5].status == a.gfx_telemetry_unavailable);
    // Partial APU metrics never turn absent sensor components into zero readings.
    input.source = a.gfx_telemetry_source_smu10;
    input.metrics[0] = .{ .status = a.gfx_telemetry_fresh, .source_stamp = 1, .flags = a.gfx_telemetry_partial_values | a.gfx_telemetry_current_clocks | a.gfx_telemetry_fabric_clock | (3 << 8), .values = .{ 400000000, 800000000, 0, 0 } };
    try validate(&input, input.sampled_ns);
    input.metrics[0].values[2] = 1;
    try t.expectError(error.Invalid, validate(&input, input.sampled_ns));
    input.metrics[0].values[2] = 0;
    input.metrics[0].flags = 3 << 8;
    try t.expectError(error.Invalid, validate(&input, input.sampled_ns));
    input.metrics[0] = .{};
    state.closeDriver(7);
    try t.expectError(error.Unavailable, state.query(request, input.sampled_ns));
    input.memory_generation += 1;
    _ = try state.publish(.{ .kind = .driver, .id = 7, .generation = 10 }, &input, input.sampled_ns);
    try t.expectError(error.Stale, state.query(request, input.sampled_ns));
}
