// Kernel ownership and wait bridge for asynchronous graphics transports.
// Rendering policy remains in R4GFX; native command encoding stays in R4D.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const resource_model = @import("queue_resources.zig");
pub const model = @import("queue_state.zig");
const sync = @import("../sched/sync.zig");
const task = @import("../sched/task.zig");
const scheduler = @import("../sched/scheduler.zig");
const monotonic = @import("../platform/monotonic.zig");
const irq = @import("../kernel/irq_router.zig");
const work = @import("../kernel/driver_work.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
pub const Error = resource_model.Error || buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled, DeviceLost };
pub const queue_capacity = 32;
pub const fence_capacity = 128;
const copy_slice_bytes = 64 * 1024;
const software = model.Binding{};
var state = model.Store(queue_capacity, fence_capacity){};
var ingress = @import("queue_ingress.zig").Ingress(fence_capacity){};
var resources = resource_model.Resources(fence_capacity){};
const native_model = @import("queue_native.zig");
var native_jobs: [fence_capacity]?*native_model.Job = @splat(null);
var completions: [fence_capacity]sync.Event = .{sync.Event.init(false)} ** fence_capacity;
var releases: [fence_capacity]sync.Event = .{sync.Event.init(false)} ** fence_capacity;
pub const WaitFor = enum(u32) { completion, resources_released };
var worker_event = sync.Event.initMode(false, .auto_reset);
var started = false;
var next_copy: usize = 0;
const Waiter = struct { task_id: u32 = 0, generation: u64 = 0, fence: model.Fence = .{} };
var waiters: [256]Waiter = .{Waiter{}} ** 256;
pub const NativeConfig = struct { adapter: u32, milestone: model.Milestone, notify: work.WorkHandler, context: usize, profile: abi.GfxBackendProfile = .{}, operations: u64 = 7, memory_generation: u64 = 0 };
pub const BackendInfo = struct { binding: model.Binding, milestone: model.Milestone, profile: abi.GfxBackendProfile = .{}, operations: u64 = 0, memory_generation: u64 = 0 };
pub fn backendAt(index: u32) ?BackendInfo {
    if (!started or index > backends.len) return null;
    if (index == 0) return .{ .binding = software, .milestone = .cpu_stores, .operations = 11 };
    buffers.lock();
    defer buffers.unlock();
    const backend = backends[index - 1];
    if (backend.owner.id == 0 or backend.closing) return null;
    return .{ .binding = backend.binding, .milestone = backend.milestone, .profile = backend.profile, .operations = backend.operations, .memory_generation = backend.memory_generation };
}
const Backend = struct {
    owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    binding: model.Binding = .{},
    milestone: model.Milestone = .device_execution,
    notify: ?work.WorkHandler = null,
    context: usize = 0,
    closing: bool = false,
    notifying: bool = false,
    lifecycle_dirty: bool = false,
    work_handle: u32 = 0,
    display_timeline: u64 = 0,
    profile: abi.GfxBackendProfile = .{},
    properties: ?abi.GfxBackendProperties = null,
    operations: u64 = 7,
    job_operations: u64 = 7,
    target_jobs: bool = false,
    memory_generation: u64 = 0,
};
var backends: [16]Backend = .{Backend{}} ** 16;
var wakeups = @import("queue_ingress.zig").Wakeups(backends.len){};
var backend_serial: u64 = 1;

pub fn init() bool {
    if (started) return true;
    _ = task.createKernelThreadWithRole("gfx-work", workerMain, .short_completion) orelse return false;
    started = true;
    return true;
}
pub fn available() bool { return started; }
pub fn wake() void { worker_event.signal(); }
fn now() u64 {
    return monotonic.nowNanoseconds() orelse 0;
}
pub fn open(owner: buffers.Owner, config: model.Config) Error!u64 {
    if (!started) return error.Unavailable;
    buffers.lock();
    defer buffers.unlock();
    if (config.binding.adapter == 0) {
        if (!std.meta.eql(config.binding, software) or config.milestone != .cpu_stores) return error.Unsupported;
    } else {
        const backend = try backendLocked(config.binding);
        if (backend.closing) return error.DeviceLost;
        if (backend.milestone != config.milestone) return error.Unsupported;
    }
    return state.open(owner, config);
}

pub fn validatedProfile(input: abi.GfxBackendProfile) Error!abi.GfxBackendProfile {
    var profile = input;
    if (profile.version != 1 or profile.size < @sizeOf(abi.GfxBackendProfile) or profile.data_bytes > profile.data.len) return error.Invalid;
    const empty = profile.interface_id_lo == 0 and profile.interface_id_hi == 0;
    if ((empty and (profile.revision != 0 or profile.data_bytes != 0)) or (!empty and profile.revision == 0)) return error.Invalid;
    for (profile.data[profile.data_bytes..]) |byte| if (byte != 0) return error.Invalid;
    profile.size = @sizeOf(abi.GfxBackendProfile);
    return profile;
}
pub fn validatedProperties(input: abi.GfxBackendProperties) Error!abi.GfxBackendProperties {
    if (input.version != 1 or input.size < @sizeOf(abi.GfxBackendProperties) or
        (input.interface_id_lo == 0 and input.interface_id_hi == 0) or
        input.revision == 0 or input.data_bytes == 0 or input.data_bytes > input.data.len) return error.Invalid;
    for (input.data[input.data_bytes..]) |byte| if (byte != 0) return error.Invalid;
    var result = input;
    result.size = @sizeOf(abi.GfxBackendProperties);
    return result;
}
pub fn publishNativeProperties(identity: buffers.Owner, binding: model.Binding, milestone: u32, input: abi.GfxBackendProperties) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const value = try validatedProperties(input);
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (!backend.owner.eql(identity)) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    if (@intFromEnum(backend.milestone) != milestone) return error.Invalid;
    if (backend.properties != null) return error.Busy;
    backend.properties = value;
}
pub fn backendProperties(binding: model.Binding, milestone: u32) Error!?abi.GfxBackendProperties {
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (backend.closing) return error.DeviceLost;
    if (@intFromEnum(backend.milestone) != milestone) return error.Invalid;
    return backend.properties;
}
pub fn registerNative(identity: buffers.Owner, config: NativeConfig) Error!model.Binding {
    if (!started or irq.inDispatch()) return error.Unavailable;
    if (identity.kind != .driver or !identity.valid() or config.adapter == 0 or config.milestone == .cpu_stores or config.operations == 0 or config.operations & ~@as(u64, 4095) != 0) return error.Invalid;
    const profile = try validatedProfile(config.profile);
    if (config.operations & (@as(u64, 1) << abi.gfx_queue_operation_native) != 0 and
        profile.interface_id_lo == 0 and profile.interface_id_hi == 0) return error.Unsupported;
    buffers.lock();
    defer buffers.unlock();
    if (backend_serial == std.math.maxInt(u64)) return error.Exhausted;
    for (&backends) |backend| if (backend.owner.id != 0 and backend.binding.adapter == config.adapter) return error.Busy;
    for (&backends, 0..) |*backend, index| if (backend.owner.id == 0) {
        backend_serial += 1;
        backend.* = .{ .owner = identity, .binding = .{ .adapter = config.adapter, .device_generation = backend_serial, .reset_generation = 1 }, .milestone = config.milestone, .notify = config.notify, .context = config.context, .profile = profile, .operations = config.operations, .job_operations = config.operations,
            .memory_generation = if (config.memory_generation != 0) config.memory_generation else backend_serial };
        const flags = interrupts.saveAndDisableRuntime();
        wakeups.bind(index, @intCast(identity.id), backend.binding);
        interrupts.restore(flags);
        return backend.binding;
    };
    return error.Capacity;
}
fn backendLocked(binding: model.Binding) Error!*Backend {
    for (&backends) |*backend| {
        if (backend.owner.id == 0 or backend.binding.adapter != binding.adapter) continue;
        if (!std.meta.eql(backend.binding, binding)) return error.Stale;
        return backend;
    }
    return error.Unsupported;
}
fn jobBackendLocked(id: u32, fence: model.Fence) Error!*Backend {
    if (id == 0 or fence.binding.adapter == 0) return error.WrongOwner;
    _ = try state.query(fence);
    // A late physical completion may belong to the preceding reset. Its
    // exact fence stays valid, but an entirely new device cannot acknowledge it.
    for (&backends) |*backend| if (backend.owner.id == id and backend.binding.adapter == fence.binding.adapter and
        backend.binding.device_generation == fence.binding.device_generation) return backend;
    return error.WrongOwner;
}
pub fn nativeMilestone(id: u32, binding: model.Binding) Error!model.Milestone {
    buffers.lock();
    defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    return backend.milestone;
}
pub fn nativeOperations(id: u32, binding: model.Binding) Error!u64 {
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    return backend.operations;
}
pub fn nativeQueueOwnerInfo(id: u32, binding: model.Binding, timeline: u64) Error!?abi.GfxQueueOwnerInfo {
    if (irq.inDispatch()) return error.Unavailable;
    if (timeline == 0) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    for (&state.queues) |*queue| if (queue.timeline == timeline) {
        if (!std.meta.eql(queue.config.binding, binding)) return error.Stale;
        return .{ .timeline = timeline, .producer_kind = @as(u32, @intFromEnum(queue.owner.kind)) + 1,
            .closing = @intFromBool(queue.closing), .producer_id = queue.owner.id, .producer_generation = queue.owner.generation,
            .inflight_jobs = queue.inflight, .retained_jobs = queue.jobs };
    };
    return null;
}
// Resident hint only. The normal worker submits the callback after unlocking;
// no allocation, wakeup or driver call occurs under the BO owner.
fn queueClosingLocked(binding: model.Binding) void {
    for (&backends) |*backend| if (backend.owner.id != 0 and !backend.closing and std.meta.eql(backend.binding, binding)) {
        backend.lifecycle_dirty = true;
        return;
    };
}
fn nativeJobCapacity(backend: *const Backend) u32 {
    if (backend.job_operations & (@as(u64, 1) << abi.gfx_queue_operation_native) != 0) return @sizeOf(abi.GfxDriverJob);
    return if (backend.target_jobs) 272 else if (backend.job_operations & 976 != 0) 224 else if (backend.job_operations & 40 != 0) 136 else 112;
}
// Caller holds the BO/queue metadata mutex during output admission.
pub fn requireOutputTargetsLocked(identity: buffers.Owner, binding: model.Binding, job_size: u32) Error!void {
    if (job_size != 272 and job_size != @sizeOf(abi.GfxDriverJob)) return error.Invalid;
    const backend = try backendLocked(binding);
    if (!backend.owner.eql(identity)) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    if (backend.operations & (@as(u64, 1) << abi.gfx_queue_operation_present) == 0) return error.Unsupported;
    backend.target_jobs = true;
}
pub fn outputBusyLocked(target: abi.GfxOutputTarget) bool {
    for (&resources.entries) |*entry| if (entry.fence.slot != 0 and @import("output_target.zig").same(entry.display_target, target)) {
        const status = state.query(entry.fence) catch continue;
        if (status.phase != .terminal or status.device_active or status.resources_held) return true;
    };
    return false;
}
pub fn updateNativeOperations(id: u32, binding: model.Binding, operations: u64) Error!void {
    if (irq.inDispatch() or operations == 0 or operations & ~@as(u64, 4095) != 0) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    if (operations & (@as(u64, 1) << abi.gfx_queue_operation_native) != 0 and
        backend.profile.interface_id_lo == 0 and backend.profile.interface_id_hi == 0) return error.Unsupported;
    backend.operations = operations;
    // Disabling future admission cannot shrink the output needed to receive
    // work already queued under the former capability set.
    backend.job_operations |= operations;
}
// The extra operation is private to the retained display producer. Old R4D
// queue registrations cannot accidentally receive an operation they lack.
pub fn bindDisplayQueue(id: u32, binding: model.Binding, timeline: u64) Error!void {
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    if (backend.closing or backend.display_timeline != 0) return error.Busy;
    const config = try state.configuration(timeline, resource_model.display_owner);
    if (!std.meta.eql(config.binding, binding) or config.milestone != .device_execution) return error.Invalid;
    backend.display_timeline = timeline;
}
pub fn unbindDisplayQueue(id: u32, binding: model.Binding, timeline: u64) void {
    buffers.lock(); defer buffers.unlock();
    const backend = backendLocked(binding) catch return;
    if (id != 0 and backend.owner.id == id and backend.display_timeline == timeline) backend.display_timeline = 0;
}
pub fn validateOutputBinding(id: u32, input: @import("r4os_kernel_contract").GfxBackendBinding) Error!void {
    if (input.version != 1 or input.size < @sizeOf(@TypeOf(input)) or irq.inDispatch()) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    const backend = try backendLocked(.{ .adapter = input.adapter_id, .device_generation = input.device_generation, .reset_generation = input.reset_generation });
    if (id == 0 or backend.owner.id != id) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    if (@intFromEnum(backend.milestone) != input.milestone) return error.Invalid;
}
pub fn takeNative(id: u32, binding: model.Binding, output_bytes: u32) Error!struct { entry: resource_model.Entry, producer: model.Owner } {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    buffers.lock();
    defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (backend.owner.id != id or id == 0) return error.WrongOwner;
    // Check and dequeue under the same owner: capability updates cannot
    // admit a larger job between output validation and acquiring that job.
    if (output_bytes < nativeJobCapacity(backend)) return error.Invalid;
    if (backend.closing) return error.DeviceLost;
    const fence = state.takeReadyFor(binding, instant) orelse return error.Busy;
    const flags = interrupts.saveAndDisableRuntime();
    ingress.arm(id, fence) catch unreachable;
    interrupts.restore(flags);
    buffers.visibility();
    return .{ .entry = resources.entries[fence.slot - 1], .producer = try state.producer(fence) };
}
pub fn nativeSegment(id: u32, fence: model.Fence, which: u32, offset: u64, mask: u64) Error!@import("r4os_kernel_contract").GfxDmaSegment {
    if (irq.inDispatch() or which > 1) return error.Invalid;
    buffers.lock();
    defer buffers.unlock();
    _ = try jobBackendLocked(id, fence);
    if (!(try state.query(fence)).device_active) return error.AlreadyCompleted;
    const use = resources.entries[fence.slot - 1].uses[which] orelse return error.Invalid;
    // One page-bounded translation, under the BO pin and the established
    // program -> paging lock order. Never allocates a page list in an IRQ.
    return @import("../kernel/gfx_driver_memory.zig").dmaSegment(use, offset, mask);
}
fn nativeJobLocked(id: u32, fence: model.Fence) Error!*native_model.Job {
    _ = try jobBackendLocked(id, fence);
    if (!(try state.query(fence)).device_active) return error.AlreadyCompleted;
    const entry = &resources.entries[fence.slot - 1];
    if (!std.meta.eql(entry.fence, fence) or entry.operation != .native) return error.Invalid;
    return native_jobs[fence.slot - 1] orelse error.Stale;
}
pub fn nativeInfo(id: u32, fence: model.Fence) Error!abi.GfxNativeJobInfo {
    if (irq.inDispatch()) return error.Unavailable;
    buffers.lock(); defer buffers.unlock();
    return (try nativeJobLocked(id, fence)).info;
}
pub fn nativeBinding(id: u32, fence: model.Fence, index: u32) Error!abi.GfxNativeBinding {
    if (irq.inDispatch()) return error.Unavailable;
    buffers.lock(); defer buffers.unlock();
    const job = try nativeJobLocked(id, fence);
    if (index >= job.resources.len) return error.Invalid;
    return (job.resources[index].held orelse return error.Stale).binding;
}
pub const NativeData = struct { bytes: [abi.gfx_native_read_capacity]u8 = undefined };
pub fn nativeData(id: u32, fence: model.Fence, offset: u32, count: u32) Error!NativeData {
    if (irq.inDispatch()) return error.Unavailable;
    if (count == 0 or count > abi.gfx_native_read_capacity) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    const job = try nativeJobLocked(id, fence);
    if (count > job.commands.len or offset > job.commands.len - count) return error.Invalid;
    var result: NativeData = .{};
    @memcpy(result.bytes[0..count], job.commands[offset..][0..count]);
    return result;
}
pub fn nativeRenderList(id: u32, fence: model.Fence) Error!abi.GfxRenderList {
    if (irq.inDispatch()) return error.Unavailable;
    buffers.lock();
    defer buffers.unlock();
    _ = try jobBackendLocked(id, fence);
    return resources.renderList(&state, fence);
}
pub fn nativeRenderGridList(id: u32, fence: model.Fence) Error!abi.GfxRenderGridList {
    if (irq.inDispatch()) return error.Unavailable;
    buffers.lock(); defer buffers.unlock();
    _ = try jobBackendLocked(id, fence);
    return resources.renderGridList(&state, fence);
}
pub const RetainedResource = struct { reference: buffers.Handle, buffer: buffers.Handle };
pub fn nativeRenderColorList(id: u32, fence: model.Fence) Error!abi.GfxRenderColorList {
    if (irq.inDispatch()) return error.Unavailable;
    buffers.lock(); defer buffers.unlock();
    _ = try jobBackendLocked(id, fence);
    return resources.renderColorList(&state, fence);
}
pub fn retainNative(identity: buffers.Owner, fence: model.Fence, which: u32) Error!RetainedResource {
    if (irq.inDispatch() or which > 1) return error.Invalid;
    try buffers.lockPrepared(.{ .references = 1 });
    defer buffers.unlock();
    try @import("../kernel/gfx_driver_memory.zig").admitLocked(identity);
    const backend = try backendLocked(fence.binding);
    if (!backend.owner.eql(identity)) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    const reference = try resources.retain(&state, &buffers.store, fence, which, identity);
    return .{ .reference = reference, .buffer = buffers.store.bufferFor(reference, identity) catch unreachable };
}
pub fn completeNative(id: u32, fence: model.Fence, result: model.Result, quiesced: bool) Error!void {
    const instant = now();
    buffers.visibility();
    const flags = interrupts.saveAndDisableRuntime();
    ingress.acknowledge(id, fence, result, quiesced, instant) catch |err| {
        interrupts.restore(flags);
        return err;
    };
    interrupts.restore(flags);
    // Auto-reset wakes at most the one resident worker. Fence waiters,
    // callbacks, page tables and destruction are processed by that worker.
    worker_event.signal();
}
pub fn retainNativeScanout(identity: buffers.Owner, fence: model.Fence) Error!RetainedResource {
    if (irq.inDispatch()) return error.Invalid;
    try buffers.lockPrepared(.{ .references = 1 }); defer buffers.unlock();
    try @import("../kernel/gfx_driver_memory.zig").admitLocked(identity);
    const backend = try jobBackendLocked(@intCast(identity.id), fence);
    if (!backend.owner.eql(identity)) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    const reference = try resources.retainScanout(&state, &buffers.store, fence, identity);
    return .{ .reference = reference, .buffer = buffers.store.bufferFor(reference, identity) catch unreachable };
}
pub fn beginNativeScanout(id: u32, fence: model.Fence) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const instant = now();
    {
        buffers.lock(); defer buffers.unlock();
        _ = try jobBackendLocked(id, fence);
        const entry = &resources.entries[fence.slot - 1];
        if (!std.meta.eql(entry.fence, fence) or entry.operation != .direct_present) return error.Invalid;
        try state.beginScanout(fence, instant);
    }
    // The existing IRQ mailbox remains armed for the final physical receipt.
    worker_event.signal();
}
pub fn nativeScanoutRetireRequested(id: u32, fence: model.Fence) Error!bool {
    if (irq.inDispatch()) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    _ = try jobBackendLocked(id, fence);
    const entry = &resources.entries[fence.slot - 1];
    if (!std.meta.eql(entry.fence, fence) or entry.operation != .direct_present) return error.Invalid;
    return state.scanoutRetireRequested(fence);
}
fn loseLocked(backend: *Backend, quiesced: bool, instant: u64) void {
    backend.closing = true;
    const slot = (@intFromPtr(backend) - @intFromPtr(&backends)) / @sizeOf(Backend);
    const wake_flags = interrupts.saveAndDisableRuntime();
    wakeups.close(slot);
    interrupts.restore(wake_flags);
    // Earlier reset generations also remain owned until a proven device stop.
    state.deviceLost(backend.binding, instant);
    if (quiesced) for (&state.jobs) |job| {
        if (job.fence.slot != 0 and (job.active or job.scanout) and job.fence.binding.adapter == backend.binding.adapter and
            job.fence.binding.device_generation == backend.binding.device_generation)
        {
            const flags = interrupts.saveAndDisableRuntime();
            ingress.quiesce(job.fence);
            interrupts.restore(flags);
            state.complete(job.fence, .failed, true, instant) catch unreachable;
        }
    };
}
pub fn resetNative(id: u32, binding: model.Binding, quiesced: bool) Error!model.Binding {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    const next = blk: {
        buffers.lock();
        defer buffers.unlock();
        const backend = try backendLocked(binding);
        if (id == 0 or backend.owner.id != id) return error.WrongOwner;
        if (backend.binding.reset_generation == std.math.maxInt(u64)) return error.Exhausted;
        loseLocked(backend, quiesced, instant);
        if (!quiesced) break :blk @as(?model.Binding, null);
        backend.binding.reset_generation += 1;
        backend.properties = null;
        backend.display_timeline = 0;
        backend.closing = false;
        const slot = (@intFromPtr(backend) - @intFromPtr(&backends)) / @sizeOf(Backend);
        const flags = interrupts.saveAndDisableRuntime();
        wakeups.bind(slot, id, backend.binding);
        interrupts.restore(flags);
        break :blk backend.binding;
    };
    worker_event.signal();
    @import("outputs.zig").stoppedDriver(id);
    return next orelse error.Busy;
}
fn retainedLocked(backend: Backend) bool {
    if (backend.notifying or backend.work_handle != 0) return true;
    for (&state.jobs) |job| if (job.fence.slot != 0 and job.fence.binding.adapter == backend.binding.adapter and
        job.fence.binding.device_generation == backend.binding.device_generation and (job.active or job.resources_held)) return true;
    return false;
}
pub fn unregisterNative(id: u32, binding: model.Binding, quiesced: bool) Error!void {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    buffers.lock();
    const backend = backendLocked(binding) catch |err| {
        buffers.unlock();
        return err;
    };
    if (id == 0 or backend.owner.id != id) {
        buffers.unlock();
        return error.WrongOwner;
    }
    loseLocked(backend, quiesced, instant);
    const busy = retainedLocked(backend.*);
    if (!busy) backend.* = .{};
    buffers.unlock();
    worker_event.signal();
    @import("outputs.zig").stoppedDriver(id);
    if (busy) return error.Busy;
}
pub fn closingDriver(id: u32) void {
    const instant = now();
    buffers.lock();
    for (&backends) |*backend| if (backend.owner.id == id and id != 0) loseLocked(backend, false, instant);
    buffers.unlock();
    @import("outputs.zig").stoppedDriver(id);
    worker_event.signal();
}
pub fn retainsDriver(id: u32) bool {
    buffers.lock();
    defer buffers.unlock();
    for (&backends) |backend| if (backend.owner.id == id and id != 0) return true;
    return false;
}

// Calls into R4D go through the existing driver-work owner and its retained
// completion. At most one notification per backend may be queued/running.
pub fn wakeNative(id: u32, binding: model.Binding) Error!void {
    const flags = interrupts.saveAndDisableRuntime();
    wakeups.request(id, binding) catch |err| { interrupts.restore(flags); return err; };
    interrupts.restore(flags);
    worker_event.signal();
}
pub fn wakeOutput(id: u32, target: @import("r4os_kernel_contract").GfxOutputTarget) Error!void {
    const binding = blk: {
        buffers.lock(); defer buffers.unlock();
        for (&backends) |*backend| if (id != 0 and backend.owner.id == id and !backend.closing and
            backend.binding.adapter == target.adapter_id and backend.binding.device_generation == target.device_generation)
            break :blk backend.binding;
        return error.Stale;
    };
    try wakeNative(id, binding);
}
fn notifyNative() bool {
    var pending = false;
    for (0..backends.len) |i| {
        buffers.lock();
        const snapshot = backends[i];
        buffers.unlock();
        if (snapshot.owner.id == 0) continue;
        if (snapshot.work_handle != 0) {
            var status: work.CompletionStatus = .{};
            const rc = work.completionStatus(snapshot.work_handle, &status);
            if (rc != 0 or status.state == work.WORK_STATE_COMPLETED or status.state == work.WORK_STATE_CANCELLED) {
                if (rc != 0 or work.completionRelease(snapshot.work_handle) == 0) {
                    buffers.lock();
                    if (backends[i].binding.device_generation == snapshot.binding.device_generation and backends[i].work_handle == snapshot.work_handle) backends[i].work_handle = 0;
                    buffers.unlock();
                }
            }
            pending = true;
            continue;
        }
        buffers.lock();
        const backend = &backends[i];
        var ready = false;
        if (backend.owner.id != 0 and !backend.closing and !backend.notifying and backend.work_handle == 0) {
            const flags = interrupts.saveAndDisableRuntime();
            ready = wakeups.take(i) or backend.lifecycle_dirty;
            backend.lifecycle_dirty = false;
            interrupts.restore(flags);
            for (&state.jobs) |job| if (job.fence.slot != 0 and
                (job.phase == .queued or (job.scanout and (job.retire_requested or !job.client_reference or state.queues[job.queue].closing))) and
                std.meta.eql(job.fence.binding, backend.binding)) {
                ready = true;
                break;
            };
        }
        if (ready) backend.notifying = true;
        const selected = backend.*;
        buffers.unlock();
        if (!ready) continue;
        var handle: u32 = 0;
        const rc = work.submit(@intCast(selected.owner.id), notifyDriver, i, 0, &handle);
        buffers.lock();
        std.debug.assert(backends[i].binding.device_generation == selected.binding.device_generation);
        backends[i].work_handle = if (rc == 0) handle else 0;
        backends[i].notifying = false;
        if (rc != 0 and !backends[i].closing) {
            const flags = interrupts.saveAndDisableRuntime();
            wakeups.request(@intCast(selected.owner.id), selected.binding) catch {};
            interrupts.restore(flags);
        }
        buffers.unlock();
        pending = true;
    }
    return pending;
}
fn notifyDriver(index: usize) callconv(.c) i32 {
    if (index >= backends.len) return -1;
    const id = work.currentOwner();
    if (id == 0) return -1;
    if (!@import("../kernel/driver_api.zig").enterOwnerBounded(id, @as(u64, @max(@import("../kernel/timer.zig").frequency(), 1)) * 3)) {
        buffers.lock(); const retry = backends[index]; buffers.unlock();
        if (retry.owner.id == id and !retry.closing) wakeNative(id, retry.binding) catch {};
        return -1;
    }
    defer _ = @import("../kernel/driver_api.zig").leaveOwner();
    buffers.lock();
    const selected = backends[index];
    buffers.unlock();
    if (selected.owner.id != id or selected.closing or selected.notify == null) return 0;
    // Driver-work authenticates the caller; the bound DriverApi guard also
    // serializes task callbacks against init, display restore and shutdown.
    return selected.notify.?(selected.context);
}
pub fn submit(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request) Error!model.Status {
    if (transport.operation == .native or transport.operation == .present or transport.operation == .direct_present) return error.Unsupported;
    return submitImpl(owner, timeline, request, transport, null);
}
pub fn submitNative(owner: buffers.Owner, timeline: u64, request: model.Submission, input: abi.GfxNativeSubmission) Error!model.Status {
    if (!started or irq.inDispatch()) return error.Unavailable;
    const job = try native_model.Job.create(input);
    errdefer job.destroy();
    defer worker_event.signal();
    const instant = now();
    try buffers.lockPrepared(.{ .leases = job.resources.len });
    defer buffers.unlock();
    const config = try state.configuration(timeline, owner);
    if (config.binding.adapter == 0) return error.Unsupported;
    const backend = try backendLocked(config.binding);
    if (backend.closing) return error.DeviceLost;
    if (backend.operations & (@as(u64, 1) << abi.gfx_queue_operation_native) == 0 or
        job.info.interface_id_lo != backend.profile.interface_id_lo or job.info.interface_id_hi != backend.profile.interface_id_hi or
        job.info.revision != backend.profile.revision) return error.Unsupported;
    errdefer job.rollbackLocked();
    try job.acquireLocked(owner, .{ .adapter = backend.binding.adapter, .driver_owner = @intCast(backend.owner.id),
        .device_generation = backend.memory_generation }, &state, &resources, timeline, request.dependencies);
    const accepted = try submitLocked(owner, timeline, request, .{ .operation = .native }, null, instant);
    const index = accepted.fence.slot - 1;
    std.debug.assert(native_jobs[index] == null);
    resources.entries[index].native_uses = job.uses;
    native_jobs[index] = job;
    return accepted;
}
// Only the native display bridge calls this while holding DisplayExecution
// and its own lifetime guard. Public queue submission cannot skip geometry,
// active-output and concurrent-writer admission.
pub fn submitDisplayImage(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, binding: model.Binding) Error!model.Status {
    if (transport.operation != .present and transport.operation != .direct_present) return error.Invalid;
    return submitImpl(owner, timeline, request, transport, binding);
}
fn submitImpl(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, output_binding: ?model.Binding) Error!model.Status {
    if (!started or irq.inDispatch()) return error.Unavailable;
    const instant = now();
    const snapshot = blk: {
        try buffers.lockPrepared(.{ .leases = 2 });
        defer buffers.unlock();
        break :blk try submitLocked(owner, timeline, request, transport, output_binding, instant);
    };
    worker_event.signal();
    return snapshot;
}
// Additional outputs keep route validation, BO admission and their pending
// fence in this same critical section; no callbacks or waits are performed.
pub fn submitOutputLocked(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, binding: model.Binding) Error!model.Status {
    if (!started or irq.inDispatch()) return error.Unavailable;
    if (transport.operation != .present and transport.operation != .direct_present or transport.display_target.connector_id == 0) return error.Invalid;
    return submitLocked(owner, timeline, request, transport, binding, now());
}
pub fn queryLocked(fence: model.Fence) Error!model.Status { return state.query(fence); }
fn submitLocked(owner: buffers.Owner, timeline: u64, request: model.Submission, transport: resource_model.Request, output_binding: ?model.Binding, instant: u64) Error!model.Status {
        // BO/queue mutex precedes the short output state owner. Keep receiver
        // validation and queue admission atomic without reversing that order.
        const outputs_owner = @import("ownership.zig");
        const output_token = if (transport.display_target.connector_id != 0) outputs_owner.enterState() else null;
        defer if (output_token) |token| outputs_owner.leaveState(token);
        const config = try state.configuration(timeline, owner);
        if (output_binding) |binding| if (!std.meta.eql(binding, config.binding)) return error.Stale;
        const backend = if (config.binding.adapter == 0) null else try backendLocked(config.binding);
        if (transport.display_target.connector_id != 0) {
            const native = backend orelse return error.Unsupported;
            try @import("outputs.zig").validateTargetLocked(@intCast(native.owner.id), transport.display_target);
        }
        const operations = if (backend) |native| native.operations else @as(u64, 11);
        if (operations & (@as(u64, 1) << @intCast(@intFromEnum(transport.operation))) == 0) return error.Unsupported;
        if (transport.operation == .upload) {
            if (!owner.eql(resource_model.display_owner)) return error.Unsupported;
            const native = backend orelse return error.Unsupported;
            if (native.closing or native.display_timeline != timeline) return error.Unsupported;
        }
        var retained = transport;
        // Authenticated registry state, never caller-provided driver identity.
        retained.memory_binding = if (backend) |native| .{ .adapter = native.binding.adapter,
            .driver_owner = @intCast(native.owner.id), .device_generation = native.memory_generation } else null;
        const accepted = try resources.submit(&state, &buffers.store, timeline, owner, request, retained, instant);
        completions[accepted.slot - 1] = sync.Event.init(false);
        releases[accepted.slot - 1] = sync.Event.init(false);
        buffers.visibility();
        return state.query(accepted) catch unreachable;
}
pub fn query(fence: model.Fence) Error!model.Status {
    buffers.lock();
    defer buffers.unlock();
    return state.query(fence);
}
pub fn cancel(owner: buffers.Owner, fence: model.Fence) Error!void {
    const instant = now();
    buffers.lock();
    state.cancel(fence, owner, instant) catch |err| {
        buffers.unlock();
        return err;
    };
    buffers.unlock();
    worker_event.signal();
}
pub fn close(owner: buffers.Owner, timeline: u64) Error!void {
    const instant = now();
    buffers.lock();
    for (&state.queues) |*queue| if (queue.timeline == timeline and queue.owner.eql(owner)) {
        queueClosingLocked(queue.config.binding);
        break;
    };
    state.close(timeline, owner, instant) catch |err| {
        buffers.unlock();
        return err;
    };
    buffers.unlock();
    worker_event.signal();
}
pub fn drop(owner: buffers.Owner, fence: model.Fence) Error!void {
    buffers.lock();
    state.drop(fence, owner) catch |err| {
        buffers.unlock();
        return err;
    };
    buffers.unlock();
    worker_event.signal();
}
pub fn stopped(owner: buffers.Owner) void {
    const instant = now();
    buffers.lock();
    for (&state.queues) |*queue| if (queue.timeline != 0 and queue.owner.eql(owner)) queueClosingLocked(queue.config.binding);
    state.stopped(owner, instant);
    buffers.unlock();
    worker_event.signal();
}

// A hard-killed task does not run Zig defers. Retain the exact fence in an
// explicit task-generation record; the program reaper drains this only after
// the scheduler has detached that task and its intrusive wait node.
pub fn retiredTask(id: u32, generation: u64) void {
    buffers.lock();
    for (&waiters) |*waiter| if (waiter.task_id == id and waiter.generation == generation) {
        state.releaseWaiter(waiter.fence) catch unreachable;
        waiter.* = .{};
    };
    buffers.unlock();
    worker_event.signal();
}
pub fn wait(fence: model.Fence, timeout_ticks: u64, wait_for: WaitFor) Error!model.Status {
    if (irq.inDispatch()) return error.Unavailable;
    const current = scheduler.current() orelse return error.Unavailable;
    const slot = blk: {
        buffers.lock();
        defer buffers.unlock();
        const status = try state.query(fence);
        if (waitSatisfied(status, wait_for)) return status;
        for (&waiters, 0..) |*waiter, i| if (waiter.task_id == 0) {
            try state.retainWaiter(fence);
            waiter.* = .{ .task_id = current.id, .generation = current.generation, .fence = fence };
            break :blk i;
        };
        return error.Capacity;
    };
    // Manual-reset terminal events are sticky. Completion in the enrollment
    // gap or a second waiter arriving after publication cannot lose a wake.
    const event = if (wait_for == .completion) &completions[fence.slot - 1] else &releases[fence.slot - 1];
    const result = event.waitResult(timeout_ticks);
    const status = blk: {
        buffers.lock();
        defer buffers.unlock();
        const snapshot = state.query(fence) catch unreachable;
        state.releaseWaiter(fence) catch unreachable;
        waiters[slot] = .{};
        break :blk snapshot;
    };
    worker_event.signal();
    if (waitSatisfied(status, wait_for)) return status;
    return if (result == .timeout) error.WaitTimeout else error.WaitCancelled;
}
fn waitSatisfied(status: model.Status, wait_for: WaitFor) bool {
    return status.phase == .terminal and (wait_for == .completion or !status.resources_held);
}

fn publishAndRelease() void {
    var count: usize = 0;
    while (count < fence_capacity) : (count += 1) {
        buffers.lock();
        const notification = state.takeNotification();
        buffers.unlock();
        const fence = notification orelse break;
        // Arbitrarily many task wakeups belong in worker context, never IRQ.
        completions[fence.slot - 1].signal();
        buffers.lock();
        state.published(fence) catch unreachable;
        buffers.unlock();
    }
    count = 0;
    while (count < fence_capacity) : (count += 1) {
        buffers.lock();
        const ticket = state.takeRelease() orelse {
            buffers.unlock();
            break;
        };
        resources.release(&buffers.store, ticket) catch {
            state.released(ticket, false) catch unreachable;
            buffers.unlock();
            break;
        };
        const native = native_jobs[ticket.fence.slot - 1];
        if (native) |job| job.releaseBindingsLocked();
        native_jobs[ticket.fence.slot - 1] = null;
        state.released(ticket, true) catch unreachable;
        // A short publication reference prevents event storage reuse even
        // when all client references and external waiters have disappeared.
        state.retainWaiter(ticket.fence) catch unreachable;
        buffers.unlock();
        if (native) |job| job.destroy();
        releases[ticket.fence.slot - 1].signal();
        buffers.lock();
        state.releaseWaiter(ticket.fence) catch unreachable;
        buffers.unlock();
    }
    buffers.lock();
    while (state.reapOne() != null) {}
    buffers.unlock();
    // VM/TLB destruction, including failure quarantine, stays outside owner.
    buffers.collect();
}

fn copySlice() bool {
    const instant = now();
    buffers.lock();
    state.expire(instant);
    _ = state.takeReadyFor(software, instant);
    var selected: ?resource_model.Entry = null;
    for (0..fence_capacity) |offset| {
        const slot = (next_copy + offset) % fence_capacity;
        const entry = resources.entries[slot];
        if (entry.fence.slot == 0 or !std.meta.eql(entry.fence.binding, software)) continue;
        const status = state.query(entry.fence) catch unreachable;
        if (!status.device_active) continue;
        selected = entry;
        next_copy = (slot + 1) % fence_capacity;
        break;
    }
    buffers.unlock();
    const entry = selected orelse return false;
    const status = query(entry.fence) catch unreachable;
    // Only this worker executes software copies. Cancellation between chunks
    // is therefore a provable stop; it never implies native DMA quiescence.
    const count = if (status.phase == .terminal) 0 else resource_model.copyChunk(&entry, copy_slice_bytes, 256);
    buffers.visibility();
    const finished_at = now();
    buffers.lock();
    resources.entries[entry.fence.slot - 1].copied += count;
    if (status.phase == .terminal or entry.copied + count == entry.bytes) {
        state.complete(entry.fence, .complete, true, finished_at) catch unreachable;
    }
    buffers.unlock();
    return true;
}
fn workerMain() callconv(.c) void {
    while (true) {
        for (0..fence_capacity) |slot| {
            const flags = interrupts.saveAndDisableRuntime();
            const ack = ingress.take(slot);
            interrupts.restore(flags);
            if (ack) |value| {
                buffers.lock();
                // A proven concurrent reset may already have retired this
                // exact operation. It cannot acknowledge a reused fence.
                state.complete(value.fence, value.result, true, value.instant) catch |err| switch (err) {
                    error.Stale, error.AlreadyCompleted => {},
                    else => unreachable,
                };
                buffers.unlock();
            }
        }
        publishAndRelease();
        const allocation_pending = @import("../kernel/gfx_allocations.zig").service();
        const virtual_pending = @import("../kernel/gfx_virtual.zig").service();
        const native_pending = notifyNative();
        if (copySlice()) {
            scheduler.yield();
            continue;
        }
        // Finite deadlines are also serviced when native/dependency work is
        // waiting. With no pending work the worker sleeps without polling.
        buffers.lock();
        var pending = false;
        for (&state.jobs) |job| if (job.fence.slot != 0 and (job.phase != .terminal or job.notification_pending)) {
            pending = true;
            break;
        };
        buffers.unlock();
        _ = worker_event.waitResult(if (pending or native_pending or allocation_pending or virtual_pending) 1 else scheduler.WAIT_FOREVER);
    }
}
