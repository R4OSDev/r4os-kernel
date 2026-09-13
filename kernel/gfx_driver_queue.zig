// DriverApi passes its actual init/work/IRQ owner; caller-supplied IDs never
// authenticate a completion. All pageable inputs/outputs stay outside owners.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const queue = @import("../display/queue.zig");
const api = @import("../program/gfx_queue_api.zig");
const memory_api = @import("../program/gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const irq = @import("irq_router.zig");

fn binding(id: u32, input: abi.GfxBackendBinding) queue.Error!queue.model.Binding {
    if (input.version != 1 or input.size < @sizeOf(abi.GfxBackendBinding)) return error.Invalid;
    const value = queue.model.Binding{ .adapter = input.adapter_id, .device_generation = input.device_generation, .reset_generation = input.reset_generation };
    const milestone = try queue.nativeMilestone(id, value);
    if (input.milestone != @intFromEnum(milestone)) return error.Invalid;
    return value;
}
fn publicBinding(value: queue.model.Binding, milestone: u32) abi.GfxBackendBinding {
    return .{ .adapter_id = value.adapter, .device_generation = value.device_generation, .reset_generation = value.reset_generation, .milestone = milestone };
}
pub fn register(identity: buffers.Owner, input: *const abi.GfxBackendRegistration, output: *abi.GfxBackendBinding) i32 {
    return registerProfile(identity, input, &.{}, output);
}
pub fn registerProfile(identity: buffers.Owner, input: *const abi.GfxBackendRegistration, profile: *const abi.GfxBackendProfile, output: *abi.GfxBackendBinding) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(profile) == 0 or !memory_api.validOutput(abi.GfxBackendBinding, output)) return abi.gfx_queue_error_invalid;
    const request = input.*;
    if (request.version != 1 or request.size < @sizeOf(abi.GfxBackendRegistration) or request.notify_callback < 0xFFFF800000000000 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const milestone = std.enums.fromInt(queue.model.Milestone, request.milestone) orelse return abi.gfx_queue_error_unsupported;
    const value = queue.registerNative(identity, .{ .adapter = request.adapter_id, .milestone = milestone, .notify = @ptrFromInt(request.notify_callback), .context = request.context, .profile = profile.* }) catch |err| return api.errorCode(err);
    output.* = publicBinding(value, request.milestone);
    return abi.gfx_queue_ok;
}
pub fn unregister(id: u32, input: *const abi.GfxBackendBinding, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const value = binding(id, input.*) catch |err| return api.errorCode(err);
    queue.unregisterNative(id, value, quiesced == 1) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn take(id: u32, input: *const abi.GfxBackendBinding, output: *abi.GfxDriverJob) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxDriverJob, output) or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const value = binding(id, input.*) catch |err| return api.errorCode(err);
    const job = queue.takeNative(id, value) catch |err| return api.errorCode(err);
    output.* = .{
        .fence = api.publicFence(job.fence),
        .operation = @intFromEnum(job.operation),
        .source_buffer = if (job.uses[0]) |use| memory_api.publicHandle(use.buffer) else .{},
        .target_buffer = if (job.uses[1]) |use| memory_api.publicHandle(use.buffer) else .{},
        .byte_length = job.bytes,
        .source_offset = if (job.uses[0]) |use| use.range.offset else 0,
        .target_offset = if (job.uses[1]) |use| use.range.offset else 0,
    };
    return abi.gfx_queue_ok;
}
pub fn complete(id: u32, input: *const abi.GfxFence, result: u32, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1) return abi.gfx_queue_error_invalid;
    const value = api.fence(input.*);
    const terminal = std.enums.fromInt(queue.model.Result, result) orelse return abi.gfx_queue_error_invalid;
    queue.completeNative(id, value, terminal, quiesced == 1) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn reset(id: u32, input: *const abi.GfxBackendBinding, quiesced: u32, output: *abi.GfxBackendBinding) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1 or !memory_api.validOutput(abi.GfxBackendBinding, output) or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const request = input.*;
    const value = binding(id, request) catch |err| return api.errorCode(err);
    const changed = queue.resetNative(id, value, quiesced == 1) catch |err| return api.errorCode(err);
    output.* = publicBinding(changed, request.milestone);
    return abi.gfx_queue_ok;
}
pub fn segment(id: u32, input: *const abi.GfxFence, which: u32, offset: u64, mask: u64, output: *abi.GfxDmaSegment) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxDmaSegment, output)) return abi.gfx_queue_error_invalid;
    const value = queue.nativeSegment(id, api.fence(input.*), which, offset, mask) catch |err| return api.errorCode(err);
    output.* = value;
    return abi.gfx_queue_ok;
}
pub fn retain(identity: buffers.Owner, input: *const abi.GfxFence, which: u32, output: *abi.GfxBufferReference) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxBufferReference, output)) return abi.gfx_queue_error_invalid;
    const value = api.fence(input.*);
    const call = @import("../sched/task_context.zig").enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = @import("../sched/task_context.zig").leaveUnwind(call);
    const retained = queue.retainNative(identity, value, which) catch |err| return api.errorCode(err);
    // Pageable output is written after dropping the metadata owner.
    output.* = .{ .reference = memory_api.publicHandle(retained.reference), .buffer = memory_api.publicHandle(retained.buffer), .flags = abi.gfx_buffer_reference_mapping_only };
    return abi.gfx_queue_ok;
}
