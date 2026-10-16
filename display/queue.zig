//! Fixed projection of the sole native graphics queue runtime/worker.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const resource_model = @import("queue_resources.zig");
pub const model = @import("queue_state.zig");
const work = @import("../kernel/driver_work.zig");
pub const Error = resource_model.Error || buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled, DeviceLost };
pub const queue_capacity = 32;
pub const fence_capacity = 128;
pub const WaitFor = enum(u32) { completion, resources_released };
pub const NativeConfig = extern struct { adapter: u32, milestone: model.Milestone, notify: work.WorkHandler, context: usize, profile: abi.GfxBackendProfile = .{}, operations: u64 = 7, memory_generation: u64 = 0 };
pub const BackendInfo = extern struct { binding: model.Binding, milestone: model.Milestone, profile: abi.GfxBackendProfile = .{}, operations: u64 = 0, memory_generation: u64 = 0 };

const Args = extern struct {
    owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, config: model.Config = .{}, binding: model.Binding = .{}, fence: model.Fence = .{},
    timeline: u64 = 0, deadline: u64 = 0, frame_key: u64 = 0, value: u64 = 0, other: u64 = 0,
    id: u32 = 0, which: u32 = 0, bytes: u32 = 0, result: u32 = 0, flag: bool = false,
    dependencies: ?[*]const model.Fence = null, dependencies_len: usize = 0, input: ?*const anyopaque = null, request: ?*const resource_model.NativeRequest = null,
};
extern fn r4os_rust_display_queue(u32, *const Args, ?*anyopaque) i32;
fn checked(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 11 => error.OutOfMemory, 12 => error.Unavailable,
    13 => error.WaitTimeout, 14 => error.WaitCancelled, 16 => error.AlreadyCompleted, 20 => error.DeviceLost, else => unreachable,
}; }
fn call(op: u32, args: Args) Error!void { try checked(r4os_rust_display_queue(op, &args, null)); }
fn result(comptime T: type, op: u32, args: Args) Error!T { var out: T = undefined; try checked(r4os_rust_display_queue(op, &args, &out)); return out; }
const OptionalBackend = extern struct { has: bool, value: BackendInfo };
const OptionalProperties = extern struct { has: bool, value: abi.GfxBackendProperties };
const OptionalOwner = extern struct { has: bool, value: abi.GfxQueueOwnerInfo };
pub const Taken = extern struct { entry: resource_model.Entry, producer: model.Owner };
pub const RetainedResource = extern struct { reference: buffers.Handle, buffer: buffers.Handle };
pub const NativeData = extern struct { bytes: [abi.gfx_native_read_capacity]u8 = undefined };
pub fn init() bool { return result(bool, 0, .{}) catch unreachable; }
pub fn available() bool { return result(bool, 1, .{}) catch unreachable; }
pub fn wake() void { call(2, .{}) catch unreachable; }
pub fn backendAt(index: u32) ?BackendInfo { const out = result(OptionalBackend, 3, .{ .id = index }) catch unreachable; return if (out.has) out.value else null; }
pub fn open(owner: buffers.Owner, config: model.Config) Error!u64 { return result(u64, 4, .{ .owner = owner, .config = config }); }
pub fn validatedProfile(input: abi.GfxBackendProfile) Error!abi.GfxBackendProfile { return result(abi.GfxBackendProfile, 5, .{ .input = &input }); }
pub fn validatedProperties(input: abi.GfxBackendProperties) Error!abi.GfxBackendProperties { return result(abi.GfxBackendProperties, 6, .{ .input = &input }); }
pub fn publishNativeProperties(identity: buffers.Owner, binding: model.Binding, milestone: u32, input: abi.GfxBackendProperties) Error!void { try call(7, .{ .owner = identity, .binding = binding, .which = milestone, .input = &input }); }
pub fn backendProperties(binding: model.Binding, milestone: u32) Error!?abi.GfxBackendProperties { const out = try result(OptionalProperties, 8, .{ .binding = binding, .which = milestone }); return if (out.has) out.value else null; }
pub fn registerNative(identity: buffers.Owner, config: NativeConfig) Error!model.Binding { return result(model.Binding, 9, .{ .owner = identity, .input = &config }); }
pub fn nativeMilestone(id: u32, binding: model.Binding) Error!model.Milestone { return result(model.Milestone, 10, .{ .id = id, .binding = binding }); }
pub fn nativeOperations(id: u32, binding: model.Binding) Error!u64 { return result(u64, 11, .{ .id = id, .binding = binding }); }
pub fn nativeQueueOwnerInfo(id: u32, binding: model.Binding, timeline: u64) Error!?abi.GfxQueueOwnerInfo { const out = try result(OptionalOwner, 12, .{ .id = id, .binding = binding, .timeline = timeline }); return if (out.has) out.value else null; }
pub fn requireOutputTargetsLocked(identity: buffers.Owner, binding: model.Binding, job_size: u32) Error!void { try call(13, .{ .owner = identity, .binding = binding, .bytes = job_size }); }
pub fn outputBusyLocked(target: abi.GfxOutputTarget) bool { return result(bool, 14, .{ .input = &target }) catch unreachable; }
pub fn updateNativeOperations(id: u32, binding: model.Binding, operations: u64) Error!void { try call(15, .{ .id = id, .binding = binding, .value = operations }); }
pub fn bindDisplayQueue(id: u32, binding: model.Binding, timeline: u64) Error!void { try call(16, .{ .id = id, .binding = binding, .timeline = timeline }); }
pub fn unbindDisplayQueue(id: u32, binding: model.Binding, timeline: u64) void { call(17, .{ .id = id, .binding = binding, .timeline = timeline }) catch unreachable; }
pub fn validateOutputBinding(id: u32, input: abi.GfxBackendBinding) Error!void { try call(18, .{ .id = id, .input = &input }); }
pub fn takeNative(id: u32, binding: model.Binding, output_bytes: u32) Error!Taken { return result(Taken, 19, .{ .id = id, .binding = binding, .bytes = output_bytes }); }
pub fn nativeSegment(id: u32, fence: model.Fence, which: u32, offset: u64, mask: u64) Error!abi.GfxDmaSegment { return result(abi.GfxDmaSegment, 20, .{ .id = id, .fence = fence, .which = which, .value = offset, .other = mask }); }
pub fn nativeInfo(id: u32, fence: model.Fence) Error!abi.GfxNativeJobInfo { return result(abi.GfxNativeJobInfo, 21, .{ .id = id, .fence = fence }); }
pub fn nativeBinding(id: u32, fence: model.Fence, index: u32) Error!abi.GfxNativeBinding { return result(abi.GfxNativeBinding, 22, .{ .id = id, .fence = fence, .which = index }); }
pub fn nativeData(id: u32, fence: model.Fence, offset: u32, count: u32) Error!NativeData { return result(NativeData, 23, .{ .id = id, .fence = fence, .value = offset, .bytes = count }); }
pub fn nativeRenderList(id: u32, fence: model.Fence) Error!abi.GfxRenderList { return result(abi.GfxRenderList, 24, .{ .id = id, .fence = fence }); }
pub fn nativeRenderGridList(id: u32, fence: model.Fence) Error!abi.GfxRenderGridList { return result(abi.GfxRenderGridList, 25, .{ .id = id, .fence = fence }); }
pub fn nativeRenderColorList(id: u32, fence: model.Fence) Error!abi.GfxRenderColorList { return result(abi.GfxRenderColorList, 26, .{ .id = id, .fence = fence }); }
pub fn retainNative(identity: buffers.Owner, fence: model.Fence, which: u32) Error!RetainedResource { return result(RetainedResource, 27, .{ .owner = identity, .fence = fence, .which = which }); }
pub fn completeNative(id: u32, fence: model.Fence, value: model.Result, quiesced: bool) Error!void { try call(28, .{ .id = id, .fence = fence, .result = @intFromEnum(value), .flag = quiesced }); }
pub fn retainNativeScanout(identity: buffers.Owner, fence: model.Fence) Error!RetainedResource { return result(RetainedResource, 29, .{ .owner = identity, .fence = fence }); }
pub fn beginNativeScanout(id: u32, fence: model.Fence) Error!void { try call(30, .{ .id = id, .fence = fence }); }
pub fn nativeScanoutRetireRequested(id: u32, fence: model.Fence) Error!bool { return result(bool, 31, .{ .id = id, .fence = fence }); }
pub fn resetNative(id: u32, binding: model.Binding, quiesced: bool) Error!model.Binding { return result(model.Binding, 32, .{ .id = id, .binding = binding, .flag = quiesced }); }
pub fn unregisterNative(id: u32, binding: model.Binding, quiesced: bool) Error!void { try call(33, .{ .id = id, .binding = binding, .flag = quiesced }); }
pub fn closingDriver(id: u32) void { call(34, .{ .id = id }) catch unreachable; }
pub fn retainsDriver(id: u32) bool { return result(bool, 35, .{ .id = id }) catch unreachable; }
pub fn wakeNative(id: u32, binding: model.Binding) Error!void { try call(36, .{ .id = id, .binding = binding }); }
pub fn wakeOutput(id: u32, target: abi.GfxOutputTarget) Error!void { try call(37, .{ .id = id, .input = &target }); }
fn submissionArgs(owner: buffers.Owner, timeline: u64, request: model.Submission) Args { return .{ .owner = owner, .timeline = timeline,
    .deadline = request.deadline_ns, .frame_key = request.frame_key, .dependencies = request.dependencies.ptr, .dependencies_len = request.dependencies.len }; }
pub fn submit(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request) Error!model.Status {
    var args = submissionArgs(owner, timeline, request); const retained = resource_model.NativeRequest.from(transport); args.request = &retained; return result(model.Status, 38, args);
}
pub fn submitNative(owner: buffers.Owner, timeline: u64, request: model.Submission, input: abi.GfxNativeSubmission) Error!model.Status {
    var args = submissionArgs(owner, timeline, request); args.input = &input; return result(model.Status, 39, args);
}
pub fn submitDisplayImage(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, binding: model.Binding) Error!model.Status {
    var args = submissionArgs(owner, timeline, request); const retained = resource_model.NativeRequest.from(transport); args.request = &retained; args.binding = binding; return result(model.Status, 40, args);
}
pub fn submitOutputLocked(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, binding: model.Binding) Error!model.Status {
    var args = submissionArgs(owner, timeline, request); const retained = resource_model.NativeRequest.from(transport); args.request = &retained; args.binding = binding; return result(model.Status, 41, args);
}
pub fn queryLocked(fence: model.Fence) Error!model.Status { return result(model.Status, 42, .{ .fence = fence }); }
pub fn query(fence: model.Fence) Error!model.Status { return result(model.Status, 43, .{ .fence = fence }); }
pub fn cancel(owner: buffers.Owner, fence: model.Fence) Error!void { try call(44, .{ .owner = owner, .fence = fence }); }
pub fn close(owner: buffers.Owner, timeline: u64) Error!void { try call(45, .{ .owner = owner, .timeline = timeline }); }
pub fn drop(owner: buffers.Owner, fence: model.Fence) Error!void { try call(46, .{ .owner = owner, .fence = fence }); }
pub fn stopped(owner: buffers.Owner) void { call(47, .{ .owner = owner }) catch unreachable; }
pub fn retiredTask(id: u32, generation: u64) void { call(48, .{ .id = id, .value = generation }) catch unreachable; }
pub fn wait(fence: model.Fence, timeout_ticks: u64, wait_for: WaitFor) Error!model.Status { return result(model.Status, 49, .{ .fence = fence, .value = timeout_ticks, .which = @intFromEnum(wait_for) }); }
