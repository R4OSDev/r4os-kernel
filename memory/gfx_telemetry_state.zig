//! Bounded common cache and demand metadata; no hardware policy or callbacks.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
pub const Error = bo.Error || error{ Unavailable };
pub const max_age_ns: u64 = 3 * std.time.ns_per_s;
pub const demand_ns: u64 = 10 * std.time.ns_per_s;
const Entry = struct {
    owner: ?bo.Owner = null,
    data: a.GfxTelemetryState = .{},
    demand: a.GfxTelemetryDemand = .{},
};
pub fn validate(input: *const a.GfxTelemetryState, now: u64) Error!void {
    if (input.version != 1 or input.size < @sizeOf(a.GfxTelemetryState) or input.adapter_id == 0 or input.memory_generation == 0 or
        input.reserved0 != 0 or input.reserved1 != 0 or input.source > 1 or input.state > 4 or input.policy > 9 or input.boost > 2 or
        now == 0 or input.sampled_ns == 0 or input.sampled_ns > now or input.valid_until_ns < input.sampled_ns or
        input.valid_until_ns - input.sampled_ns > max_age_ns) return error.Invalid;
    for (&input.metrics, 0..) |*metric, i| {
        if (metric.status > a.gfx_telemetry_stale or metric.flags & ~@as(u32, if (i == 9 and metric.status == a.gfx_telemetry_fresh) 1 else 0) != 0) return error.Invalid;
        if (metric.status == a.gfx_telemetry_fresh) {
            if (metric.source_stamp == 0) return error.Invalid;
        } else if (!std.mem.allEqual(i64, &metric.values, 0)) return error.Invalid;
    }
}
pub fn Store(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = .{Entry{}} ** capacity,
        pub fn publish(self: *Self, owner: bo.Owner, input: *const a.GfxTelemetryState, now: u64) Error!a.GfxTelemetryDemand {
            if (owner.kind != .driver or !owner.valid()) return error.WrongOwner;
            try validate(input, now);
            var empty: ?*Entry = null;
            for (&self.entries) |*entry| {
                if (entry.owner == null) { if (empty == null) empty = entry; continue; }
                if (entry.data.adapter_id != input.adapter_id) continue;
                if (!entry.owner.?.eql(owner)) return error.WrongOwner;
                if (entry.data.memory_generation != input.memory_generation or input.sampled_ns < entry.data.sampled_ns) return error.Stale;
                entry.data = input.*;
                return demand(entry, now);
            }
            const entry = empty orelse return error.Capacity;
            entry.* = .{ .owner = owner, .data = input.*, .demand = .{ .adapter_id = input.adapter_id, .memory_generation = input.memory_generation } };
            return entry.demand;
        }
        fn demand(entry: *Entry, now: u64) a.GfxTelemetryDemand {
            if (now >= entry.demand.until_ns) { entry.demand.metric_mask = 0; entry.demand.until_ns = 0; }
            return entry.demand;
        }
        pub fn query(self: *Self, request: a.GfxTelemetryRequest, now: u64) Error!a.GfxTelemetryState {
            if (request.version != 1 or request.size < @sizeOf(a.GfxTelemetryRequest) or request.reserved0 != 0 or
                request.adapter_id == 0 or request.memory_generation == 0 or request.metric_mask & ~a.gfx_telemetry_metric_mask != 0 or now == 0) return error.Invalid;
            for (&self.entries) |*entry| {
                if (entry.owner == null or entry.data.adapter_id != request.adapter_id) continue;
                if (entry.data.memory_generation != request.memory_generation) return error.Stale;
                if (now < entry.data.sampled_ns) return error.Stale;
                _ = demand(entry, now);
                if (request.metric_mask != 0) {
                    entry.demand.metric_mask |= request.metric_mask;
                    entry.demand.until_ns = now +| demand_ns;
                }
                var output = entry.data;
                if (now >= output.valid_until_ns) for (&output.metrics) |*metric| {
                    if (metric.status == a.gfx_telemetry_fresh) {
                        metric.status = a.gfx_telemetry_stale; metric.flags = 0; metric.values = @splat(0);
                    }
                };
                return output;
            }
            return error.Unavailable;
        }
        pub fn closeDriver(self: *Self, id: u32) void {
            for (&self.entries) |*entry| if (entry.owner) |owner| if (owner.id == id) { entry.* = .{}; };
        }
    };
}
test "common telemetry demand is finite and generation-bound, stale values are not measurements" {
    const t = std.testing;
    var state: Store(1) = .{};
    const owner: bo.Owner = .{ .kind = .driver, .id = 7, .generation = 8 };
    var input: a.GfxTelemetryState = .{ .adapter_id = 3, .memory_generation = 9, .sampled_ns = 100, .valid_until_ns = 200, .source = 1, .state = 1 };
    input.metrics[5] = .{ .status = a.gfx_telemetry_fresh, .source_stamp = 12, .values = .{-12500,0,0,0} };
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
    input.sampled_ns = 201; input.valid_until_ns = 301;
    try t.expectError(error.Invalid, state.publish(owner, &input, 200));
    input.sampled_ns = 120 + demand_ns; input.valid_until_ns = input.sampled_ns;
    try t.expect((try state.publish(owner, &input, input.sampled_ns)).metric_mask == 0);
    state.closeDriver(7);
    request.memory_generation = 9;
    try t.expectError(error.Unavailable, state.query(request, input.sampled_ns));
    input.memory_generation += 1;
    _ = try state.publish(.{ .kind = .driver, .id = 7, .generation = 10 }, &input, input.sampled_ns);
    try t.expectError(error.Stale, state.query(request, input.sampled_ns));
}
