const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
const runtime = @import("../display/queue.zig");
const model = runtime.model;
const resource = @import("../display/queue_resources.zig");
const buffer_api = @import("gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const task_context = @import("../sched/task_context.zig");

pub fn errorCode(err: runtime.Error) i32 {
    return switch (err) {
        error.Unavailable => abi.gfx_queue_error_unavailable,
        error.DeviceLost => abi.gfx_queue_error_device_lost,
        error.WaitTimeout => abi.gfx_queue_error_wait_timeout,
        error.WaitCancelled => abi.gfx_queue_error_wait_cancelled,
        error.AlreadyCompleted => abi.gfx_queue_error_already_completed,
        else => |other| buffer_api.status(other),
    };
}
pub fn fence(input: abi.GfxFence) model.Fence {
    return .{ .slot = input.slot, .timeline = input.timeline, .point = input.point, .binding = .{
        .adapter = input.adapter_id,
        .device_generation = input.device_generation,
        .reset_generation = input.reset_generation,
    } };
}
pub fn publicFence(value: model.Fence) abi.GfxFence {
    return .{ .slot = value.slot, .timeline = value.timeline, .point = value.point, .adapter_id = value.binding.adapter, .device_generation = value.binding.device_generation, .reset_generation = value.binding.reset_generation };
}
pub fn publicStatus(value: model.Status) abi.GfxFenceStatus {
    return .{
        .fence = publicFence(value.fence),
        .phase = @intFromEnum(value.phase),
        .result = @intFromEnum(value.result),
        .milestone = @intFromEnum(value.milestone),
        .flags = (if (value.device_active) abi.gfx_queue_flag_device_active else @as(u32, 0)) |
            (if (value.resources_held) abi.gfx_queue_flag_resources_held else @as(u32, 0)),
        .deadline_ns = value.deadline_ns,
        .completed_ns = value.completed_ns,
    };
}
fn header(comptime T: type, value: T) bool {
    return value.version == 1 and value.size >= @sizeOf(T);
}
pub fn backend(index: u32, output: *abi.GfxBackendBinding) callconv(.c) i32 {
    if (!buffer_api.validOutput(abi.GfxBackendBinding, output)) return abi.gfx_queue_error_invalid;
    const info = runtime.backendAt(index) orelse return 0;
    output.* = .{ .adapter_id = info.binding.adapter, .device_generation = info.binding.device_generation, .reset_generation = info.binding.reset_generation, .milestone = @intFromEnum(info.milestone) };
    return abi.gfx_queue_ok;
}
pub fn open(owner: buffers.Owner, input: *const abi.GfxQueueConfig, output: *abi.GfxQueueHandle) i32 {
    if (@intFromPtr(input) == 0 or !buffer_api.validOutput(abi.GfxQueueHandle, output)) return abi.gfx_queue_error_invalid;
    const value = input.*;
    if (!header(abi.GfxQueueConfig, value)) return abi.gfx_queue_error_invalid;
    const config = model.Config{
        .policy = std.enums.fromInt(model.Policy, value.policy) orelse return abi.gfx_queue_error_unsupported,
        .milestone = std.enums.fromInt(model.Milestone, value.milestone) orelse return abi.gfx_queue_error_unsupported,
        .capacity = value.capacity,
        .binding = .{ .adapter = value.adapter_id, .device_generation = value.device_generation, .reset_generation = value.reset_generation },
    };
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = task_context.leaveUnwind(call);
    const timeline = runtime.open(owner, config) catch |err| return errorCode(err);
    output.* = .{ .timeline = timeline };
    return abi.gfx_queue_ok;
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxQueueHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_queue_error_invalid;
    const value = input.*;
    if (!header(abi.GfxQueueHandle, value)) return abi.gfx_queue_error_invalid;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = task_context.leaveUnwind(call);
    runtime.close(owner, value.timeline) catch |err| return errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn submit(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, output: *abi.GfxFenceStatus) i32 {
    if (@intFromPtr(queue_ptr) == 0 or @intFromPtr(input) == 0 or !buffer_api.validOutput(abi.GfxFenceStatus, output)) return abi.gfx_queue_error_invalid;
    const queue = queue_ptr.*;
    const value = input.*;
    if (!header(abi.GfxQueueHandle, queue) or !header(abi.GfxSubmission, value) or value.dependency_count > model.max_dependencies or
        value.source.reserved0 != 0 or value.target.reserved0 != 0) return abi.gfx_queue_error_invalid;
    const operation = std.enums.fromInt(resource.Operation, value.operation) orelse return abi.gfx_queue_error_unsupported;
    var dependencies: [model.max_dependencies]model.Fence = undefined;
    for (value.dependencies, 0..) |dependency, i| {
        if (i < value.dependency_count) dependencies[i] = fence(dependency) else if (!std.meta.eql(dependency, abi.GfxFence{})) return abi.gfx_queue_error_invalid;
    }
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = task_context.leaveUnwind(call);
    const snapshot = runtime.submit(owner, queue.timeline, .{
        .deadline_ns = value.deadline_ns,
        .frame_key = value.frame_key,
        .dependencies = dependencies[0..value.dependency_count],
    }, .{
        .operation = operation,
        .source = .{ .id = value.source.id, .generation = value.source.generation },
        .target = .{ .id = value.target.id, .generation = value.target.generation },
        .source_offset = value.source_offset,
        .target_offset = value.target_offset,
        .bytes = value.byte_length,
    }) catch |err| return errorCode(err);
    output.* = publicStatus(snapshot);
    return abi.gfx_queue_ok;
}
pub fn query(input: *const abi.GfxFence, output: *abi.GfxFenceStatus) callconv(.c) i32 {
    if (@intFromPtr(input) == 0 or !buffer_api.validOutput(abi.GfxFenceStatus, output)) return abi.gfx_queue_error_invalid;
    const snapshot = runtime.query(fence(input.*)) catch |err| return errorCode(err);
    output.* = publicStatus(snapshot);
    return abi.gfx_queue_ok;
}
pub fn wait(input: *const abi.GfxFence, timeout_ticks: u64, wait_for: u32, output: *abi.GfxFenceStatus) callconv(.c) i32 {
    if (@intFromPtr(input) == 0 or !buffer_api.validOutput(abi.GfxFenceStatus, output)) return abi.gfx_queue_error_invalid;
    const identity = fence(input.*);
    const mode = std.enums.fromInt(runtime.WaitFor, wait_for) orelse return abi.gfx_queue_error_invalid;
    const snapshot = runtime.wait(identity, timeout_ticks, mode) catch |err| return errorCode(err);
    output.* = publicStatus(snapshot);
    return abi.gfx_queue_ok;
}
pub fn cancel(owner: buffers.Owner, input: *const abi.GfxFence) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_queue_error_invalid;
    const identity = fence(input.*);
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = task_context.leaveUnwind(call);
    runtime.cancel(owner, identity) catch |err| return errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn release(owner: buffers.Owner, input: *const abi.GfxFence) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_queue_error_invalid;
    const identity = fence(input.*);
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = task_context.leaveUnwind(call);
    runtime.drop(owner, identity) catch |err| return errorCode(err);
    return abi.gfx_queue_ok;
}
