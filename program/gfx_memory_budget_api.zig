// Common admission metadata only. Placement and physical capacity are R4D
// policy; a normal R4DRAW caller can only read an exact generation snapshot.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("gfx_buffer_api.zig");
const driver = @import("../kernel/gfx_driver_memory.zig");
const context = @import("../sched/task_context.zig");

fn valid(comptime T: type, value: *const T) bool {
    return @intFromPtr(value) != 0 and @intFromPtr(value) % @alignOf(T) == 0 and
        value.version == 1 and value.size >= @sizeOf(T);
}
fn request(input: *const abi.GfxDeviceBudgetRequest, output: *abi.GfxDeviceBudgetState) ?abi.GfxDeviceBudgetRequest {
    if (!valid(abi.GfxDeviceBudgetRequest, input) or !valid(abi.GfxDeviceBudgetState, output)) return null;
    const value = input.*;
    if (value.adapter_id == 0 or value.memory_generation == 0 or value.operation > abi.gfx_memory_budget_configure or
        (value.operation == abi.gfx_memory_budget_query and value.limit_bytes != 0) or value.limit_bytes % 4096 != 0) return null;
    return value;
}
fn snapshotLocked(value: abi.GfxDeviceBudgetRequest, identity: ?buffers.Owner) buffers.Error!abi.GfxDeviceBudgetState {
    const snapshot = if (identity) |owner|
        try buffers.store.deviceBudget(owner, .{ .adapter = value.adapter_id, .driver_owner = @intCast(owner.id), .device_generation = value.memory_generation })
    else try buffers.store.queryDeviceBudget(value.adapter_id, value.memory_generation);
    return .{ .adapter_id = value.adapter_id, .memory_generation = value.memory_generation,
        .flags = if (snapshot.closing) abi.gfx_memory_budget_closing else 0,
        .limit_bytes = snapshot.limit, .charged_bytes = snapshot.charged,
        .shared_limit_bytes = buffers.store.budget_bytes, .shared_charged_bytes = buffers.store.sharedChargedBytes(),
        .shared_producer_limit_bytes = buffers.store.producer_budget_bytes };
}
pub fn query(input: *const abi.GfxDeviceBudgetRequest, output: *abi.GfxDeviceBudgetState) callconv(.c) i32 {
    const value = request(input, output) orelse return abi.gfx_buffer_error_invalid;
    if (value.operation != abi.gfx_memory_budget_query) return abi.gfx_buffer_error_invalid;
    const snapshot = blk: {
        buffers.lock();
        defer buffers.unlock();
        break :blk snapshotLocked(value, null) catch |err| return api.status(err);
    };
    output.* = snapshot;
    return abi.gfx_buffer_result_ok;
}
pub fn provider(identity: buffers.Owner, input: *const abi.GfxDeviceBudgetRequest, output: *abi.GfxDeviceBudgetState) i32 {
    const value = request(input, output) orelse return abi.gfx_buffer_error_invalid;
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    const snapshot = blk: {
        buffers.lock();
        defer buffers.unlock();
        if (value.operation == abi.gfx_memory_budget_configure) {
            driver.admitLocked(identity) catch |err| return api.status(err);
            buffers.store.setDeviceBudget(identity, .{ .adapter = value.adapter_id, .driver_owner = @intCast(identity.id),
                .device_generation = value.memory_generation }, value.limit_bytes) catch |err| return api.status(err);
        }
        break :blk snapshotLocked(value, identity) catch |err| return api.status(err);
    };
    output.* = snapshot;
    return abi.gfx_buffer_result_ok;
}
