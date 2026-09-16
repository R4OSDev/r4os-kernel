// DriverApi passes its actual init/work/IRQ owner; caller-supplied IDs never
// authenticate a completion. All pageable inputs/outputs stay outside owners.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const queue = @import("../display/queue.zig");
const api = @import("../program/gfx_queue_api.zig");
const memory_api = @import("../program/gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const irq = @import("irq_router.zig");
const wire = @import("../program/gfx_queue_wire.zig");

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
    const request = wire.read(abi.GfxBackendRegistration, input) orelse return abi.gfx_queue_error_invalid;
    if (request.notify_callback < 0xFFFF800000000000 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const milestone = std.enums.fromInt(queue.model.Milestone, request.milestone) orelse return abi.gfx_queue_error_unsupported;
    const value = queue.registerNative(identity, .{ .adapter = request.adapter_id, .milestone = milestone, .notify = @ptrFromInt(request.notify_callback), .context = request.context, .profile = profile.*, .operations = request.operations, .memory_generation = request.memory_generation }) catch |err| return api.errorCode(err);
    output.* = publicBinding(value, request.milestone);
    return abi.gfx_queue_ok;
}
pub fn unregister(id: u32, input: *const abi.GfxBackendBinding, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const value = binding(id, input.*) catch |err| return api.errorCode(err);
    queue.unregisterNative(id, value, quiesced == 1) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn publishProperties(identity: buffers.Owner, input: *const abi.GfxBackendBinding, properties: *const abi.GfxBackendProperties) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(properties) == 0 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const value = input.*;
    if (value.version != 1 or value.size < @sizeOf(abi.GfxBackendBinding)) return abi.gfx_queue_error_invalid;
    queue.publishNativeProperties(identity, .{ .adapter = value.adapter_id, .device_generation = value.device_generation,
        .reset_generation = value.reset_generation }, value.milestone, properties.*) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn updateOperations(id: u32, input: *const abi.GfxBackendBinding, operations: u64) i32 {
    if (@intFromPtr(input) == 0 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const value = binding(id, input.*) catch |err| return api.errorCode(err);
    queue.updateNativeOperations(id, value, operations) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn take(id: u32, input: *const abi.GfxBackendBinding, output: *abi.GfxDriverJob) i32 {
    if (@intFromPtr(input) == 0 or irq.inDispatch()) return abi.gfx_queue_error_invalid;
    const bytes = wire.capacity(abi.GfxDriverJob, output) orelse return abi.gfx_queue_error_invalid;
    const value = binding(id, input.*) catch |err| return api.errorCode(err);
    const taken = queue.takeNative(id, value, bytes) catch |err| return api.errorCode(err);
    const job = taken.entry;
    const result: abi.GfxDriverJob = .{
        .size = bytes,
        .fence = api.publicFence(job.fence),
        .operation = @intFromEnum(job.operation),
        .source_buffer = if (job.uses[0]) |use| memory_api.publicHandle(use.buffer) else .{},
        .target_buffer = if (job.uses[1]) |use| memory_api.publicHandle(use.buffer) else .{},
        .byte_length = if (job.operation == .copy_rows or job.operation == .present or job.operation == .direct_present) job.row_bytes else job.bytes,
        .source_offset = job.source_offset,
        .target_offset = job.target_offset,
        .row_count = job.row_count,
        .source_pitch = job.source_pitch,
        .target_pitch = job.target_pitch,
        .render = job.render,
        .deadline_ns = job.deadline_ns,
        .display_target = job.display_target,
        .producer_kind = @as(u32, @intFromEnum(taken.producer.kind)) + 1,
        .producer_id = taken.producer.id,
        .producer_generation = taken.producer.generation,
    };
    wire.write(abi.GfxDriverJob, output, bytes, result);
    return abi.gfx_queue_ok;
}
pub fn complete(id: u32, input: *const abi.GfxFence, result: u32, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1) return abi.gfx_queue_error_invalid;
    const value = api.fence(input.*);
    const terminal = std.enums.fromInt(queue.model.Result, result) orelse return abi.gfx_queue_error_invalid;
    queue.completeNative(id, value, terminal, quiesced == 1) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn readRenderList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderList) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxRenderList, output)) return abi.gfx_queue_error_invalid;
    const value = queue.nativeRenderList(id, api.fence(input.*)) catch |err| return api.errorCode(err);
    output.* = value;
    return abi.gfx_queue_ok;
}
pub fn readRenderGridList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderGridList) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxRenderGridList, output)) return abi.gfx_queue_error_invalid;
    const value = queue.nativeRenderGridList(id, api.fence(input.*)) catch |err| return api.errorCode(err);
    output.* = value;
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
pub fn readRenderColorList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderColorList) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxRenderColorList, output)) return abi.gfx_queue_error_invalid;
    const value = queue.nativeRenderColorList(id, api.fence(input.*)) catch |err| return api.errorCode(err);
    output.* = value;
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
pub fn retainScanout(identity: buffers.Owner, input: *const abi.GfxFence, output: *abi.GfxBufferReference) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxBufferReference, output)) return abi.gfx_queue_error_invalid;
    const value = api.fence(input.*);
    const call = @import("../sched/task_context.zig").enterUnwind();
    if (!call.admitted()) return abi.gfx_queue_error_busy;
    defer _ = @import("../sched/task_context.zig").leaveUnwind(call);
    const retained = queue.retainNativeScanout(identity, value) catch |err| return api.errorCode(err);
    output.* = .{ .reference = memory_api.publicHandle(retained.reference), .buffer = memory_api.publicHandle(retained.buffer), .flags = abi.gfx_buffer_reference_immutable };
    return abi.gfx_queue_ok;
}
pub fn readNativeInfo(id: u32, input: *const abi.GfxFence, output: *abi.GfxNativeJobInfo) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxNativeJobInfo, output)) return abi.gfx_queue_error_invalid;
    const result = queue.nativeInfo(id, api.fence(input.*)) catch |err| return api.errorCode(err);
    output.* = result;
    return abi.gfx_queue_ok;
}
pub fn readNativeData(id: u32, input: *const abi.GfxFence, offset: u32, output: [*]u8, count: u32) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(output) == 0 or @intFromPtr(output) > std.math.maxInt(usize) - @as(usize, count)) return abi.gfx_queue_error_invalid;
    const result = queue.nativeData(id, api.fence(input.*), offset, count) catch |err| return api.errorCode(err);
    // Pageable caller storage is touched after releasing the shared owner.
    @memcpy(output[0..count], result.bytes[0..count]);
    return abi.gfx_queue_ok;
}
pub fn readNativeBinding(id: u32, input: *const abi.GfxFence, index: u32, output: *abi.GfxNativeBinding) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxNativeBinding, output)) return abi.gfx_queue_error_invalid;
    const result = queue.nativeBinding(id, api.fence(input.*), index) catch |err| return api.errorCode(err);
    output.* = result;
    return abi.gfx_queue_ok;
}
pub fn beginScanout(id: u32, input: *const abi.GfxFence) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_queue_error_invalid;
    queue.beginNativeScanout(id, api.fence(input.*)) catch |err| return api.errorCode(err);
    return abi.gfx_queue_ok;
}
pub fn scanoutRetireRequested(id: u32, input: *const abi.GfxFence) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_queue_error_invalid;
    return @intFromBool(queue.nativeScanoutRetireRequested(id, api.fence(input.*)) catch |err| return api.errorCode(err));
}
