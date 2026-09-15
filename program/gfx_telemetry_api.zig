//! Transport only. Existing Driver Work owns collection and sensor policy.
const a = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const model = @import("../memory/gfx_telemetry_state.zig");
const api = @import("gfx_buffer_api.zig");
const memory = @import("../kernel/gfx_driver_memory.zig");
const context = @import("../sched/task_context.zig");
var state: model.Store(16) = .{};
fn valid(comptime T: type, value: *const T) bool {
    return @intFromPtr(value) != 0 and @intFromPtr(value) % @alignOf(T) == 0 and value.version == 1 and value.size >= @sizeOf(T);
}
fn now() u64 { return @import("../platform/monotonic.zig").nowNanoseconds() orelse 0; }
fn status(err: model.Error) i32 {
    return switch (err) { error.Unavailable => a.gfx_buffer_error_unavailable, else => |other| api.status(other) };
}
pub fn query(input: *const a.GfxTelemetryRequest, output: *a.GfxTelemetryState) callconv(.c) i32 {
    if (!valid(a.GfxTelemetryRequest, input) or !valid(a.GfxTelemetryState, output)) return a.gfx_buffer_error_invalid;
    const instant = now();
    const result = blk: {
        buffers.lock(); defer buffers.unlock();
        break :blk state.query(input.*, instant) catch |err| return status(err);
    };
    output.* = result;
    return a.gfx_buffer_result_ok;
}
pub fn publish(owner: buffers.Owner, input: *const a.GfxTelemetryState, output: *a.GfxTelemetryDemand) i32 {
    if (!valid(a.GfxTelemetryState, input) or !valid(a.GfxTelemetryDemand, output)) return a.gfx_buffer_error_invalid;
    const call = context.enterUnwind();
    if (!call.admitted()) return a.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    const instant = now();
    const result = blk: {
        buffers.lock(); defer buffers.unlock();
        memory.admitLocked(owner) catch |err| return api.status(err);
        break :blk state.publish(owner, input, instant) catch |err| return status(err);
    };
    output.* = result;
    return a.gfx_buffer_result_ok;
}
pub fn closeDriver(id: u32) void {
    buffers.lock(); defer buffers.unlock();
    state.closeDriver(id);
}
