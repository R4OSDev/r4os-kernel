// Kernel ownership and wait bridge for asynchronous graphics transports.
// Rendering policy remains in R4GFX; native command encoding stays in R4D.
const std = @import("std");
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
var completions: [fence_capacity]sync.Event = .{sync.Event.init(false)} ** fence_capacity;
var releases: [fence_capacity]sync.Event = .{sync.Event.init(false)} ** fence_capacity;
pub const WaitFor = enum(u32) { completion, resources_released };
var worker_event = sync.Event.initMode(false, .auto_reset);
var started = false;
var next_copy: usize = 0;
const Waiter = struct { task_id: u32 = 0, generation: u64 = 0, fence: model.Fence = .{} };
var waiters: [256]Waiter = .{Waiter{}} ** 256;
pub const NativeConfig = struct { adapter: u32, milestone: model.Milestone, notify: work.WorkHandler, context: usize };
pub const BackendInfo = struct { binding: model.Binding, milestone: model.Milestone };
pub fn backendAt(index: u32) ?BackendInfo {
    if (!started or index > backends.len) return null;
    if (index == 0) return .{ .binding = software, .milestone = .cpu_stores };
    buffers.lock();
    defer buffers.unlock();
    const backend = backends[index - 1];
    if (backend.owner.id == 0 or backend.closing) return null;
    return .{ .binding = backend.binding, .milestone = backend.milestone };
}
const Backend = struct {
    owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    binding: model.Binding = .{},
    milestone: model.Milestone = .device_execution,
    notify: ?work.WorkHandler = null,
    context: usize = 0,
    closing: bool = false,
    notifying: bool = false,
    work_handle: u32 = 0,
    display_timeline: u64 = 0,
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

pub fn registerNative(identity: buffers.Owner, config: NativeConfig) Error!model.Binding {
    if (!started or irq.inDispatch()) return error.Unavailable;
    if (identity.kind != .driver or !identity.valid() or config.adapter == 0 or config.milestone == .cpu_stores) return error.Invalid;
    buffers.lock();
    defer buffers.unlock();
    if (backend_serial == std.math.maxInt(u64)) return error.Exhausted;
    for (&backends) |backend| if (backend.owner.id != 0 and backend.binding.adapter == config.adapter) return error.Busy;
    for (&backends, 0..) |*backend, index| if (backend.owner.id == 0) {
        backend_serial += 1;
        backend.* = .{ .owner = identity, .binding = .{ .adapter = config.adapter, .device_generation = backend_serial, .reset_generation = 1 }, .milestone = config.milestone, .notify = config.notify, .context = config.context };
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
pub fn takeNative(id: u32, binding: model.Binding) Error!resource_model.Entry {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    buffers.lock();
    defer buffers.unlock();
    const backend = try backendLocked(binding);
    if (backend.owner.id != id or id == 0) return error.WrongOwner;
    if (backend.closing) return error.DeviceLost;
    const fence = state.takeReadyFor(binding, instant) orelse return error.Busy;
    const flags = interrupts.saveAndDisableRuntime();
    ingress.arm(id, fence) catch unreachable;
    interrupts.restore(flags);
    buffers.visibility();
    return resources.entries[fence.slot - 1];
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
fn loseLocked(backend: *Backend, quiesced: bool, instant: u64) void {
    backend.closing = true;
    const slot = (@intFromPtr(backend) - @intFromPtr(&backends)) / @sizeOf(Backend);
    const wake_flags = interrupts.saveAndDisableRuntime();
    wakeups.close(slot);
    interrupts.restore(wake_flags);
    // Earlier reset generations also remain owned until a proven device stop.
    state.deviceLost(backend.binding, instant);
    if (quiesced) for (&state.jobs) |job| {
        if (job.fence.slot != 0 and job.active and job.fence.binding.adapter == backend.binding.adapter and
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
            ready = wakeups.take(i);
            interrupts.restore(flags);
            for (&state.jobs) |job| if (job.fence.slot != 0 and job.phase == .queued and std.meta.eql(job.fence.binding, backend.binding)) {
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
    if (!started or irq.inDispatch()) return error.Unavailable;
    const instant = now();
    const snapshot = blk: {
        buffers.lock();
        defer buffers.unlock();
        if (transport.operation == .upload) {
            if (!owner.eql(resource_model.display_owner)) return error.Unsupported;
            const config = try state.configuration(timeline, owner);
            const backend = try backendLocked(config.binding);
            if (backend.closing or backend.display_timeline != timeline) return error.Unsupported;
        }
        const accepted = try resources.submit(&state, &buffers.store, timeline, owner, request, transport, instant);
        completions[accepted.slot - 1] = sync.Event.init(false);
        releases[accepted.slot - 1] = sync.Event.init(false);
        buffers.visibility();
        break :blk state.query(accepted) catch unreachable;
    };
    worker_event.signal();
    return snapshot;
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
        state.released(ticket, true) catch unreachable;
        // A short publication reference prevents event storage reuse even
        // when all client references and external waiters have disappeared.
        state.retainWaiter(ticket.fence) catch unreachable;
        buffers.unlock();
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
    const count = if (status.phase == .terminal) 0 else @min(copy_slice_bytes, entry.bytes - entry.copied);
    if (count != 0) {
        const src = entry.uses[0].?;
        const dst = entry.uses[1].?;
        const source: [*]const u8 = @ptrFromInt(src.backing.cpu_address + src.range.offset + entry.copied);
        const target: [*]u8 = @ptrFromInt(dst.backing.cpu_address + dst.range.offset + entry.copied);
        @memcpy(target[0..count], source[0..count]);
    }
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
        _ = worker_event.waitResult(if (pending or native_pending) 1 else scheduler.WAIT_FOREVER);
    }
}
