// Native allocation transport only. Placement and RM work remain in R4D.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const model = @import("../memory/gfx_allocation_state.zig");
const queue = @import("../display/queue.zig");
const memory = @import("gfx_driver_memory.zig");
const work = @import("driver_work.zig");
const sync = @import("../sched/sync.zig");
const scheduler = @import("../sched/scheduler.zig");
const irq = @import("irq_router.zig");
pub const Error = buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled };
const capacity = 128;
var state: model.Store(capacity) = .{};
var events: [capacity]sync.Event = .{sync.Event.init(false)} ** capacity;
const Waiter = struct { task_id: u32 = 0, generation: u64 = 0, request: buffers.Handle = .{} };
var waiters: [256]Waiter = .{Waiter{}} ** 256;
const Provider = struct {
    handle: buffers.Handle = .{},
    owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    config: abi.GfxNativeProvider = .{},
    closing: bool = false,
    retiring: bool = false,
    notifying: bool = false,
    work_handle: u32 = 0,
};
var providers: [16]Provider = .{Provider{}} ** 16;
var provider_serial: u64 = 0;
var service_needed = false;
fn now() u64 {
    return @import("../platform/monotonic.zig").nowNanoseconds() orelse 0;
}
pub fn errorCode(err: Error) i32 {
    return switch (err) {
        error.Unavailable => abi.gfx_buffer_error_unavailable,
        error.WaitTimeout => abi.gfx_queue_error_wait_timeout,
        error.WaitCancelled => abi.gfx_queue_error_wait_cancelled,
        else => |other| api.status(other),
    };
}
fn providerLocked(handle: buffers.Handle, owner: buffers.Owner) Error!*Provider {
    if (handle.id == 0 or handle.id > providers.len) return error.Stale;
    const value = &providers[handle.id - 1];
    if (!value.handle.eql(handle)) return error.Stale;
    if (!value.owner.eql(owner)) return error.WrongOwner;
    return value;
}
fn heldLocked(provider: *const Provider) bool {
    return provider.notifying or provider.work_handle != 0 or state.retainsProvider(provider.handle);
}
pub fn register(owner: buffers.Owner, config: abi.GfxNativeProvider) Error!buffers.Handle {
    if (!queue.available() or irq.inDispatch()) return error.Unavailable;
    if (config.version != 1 or config.size < @sizeOf(abi.GfxNativeProvider) or config.reserved0 != 0 or
        config.adapter_id == 0 or config.memory_generation == 0 or config.notify == 0) return error.Invalid;
    buffers.lock();
    defer buffers.unlock();
    try memory.admitLocked(owner);
    for (&providers) |provider| if (provider.handle.id != 0 and provider.config.adapter_id == config.adapter_id) return error.Busy;
    if (provider_serial == std.math.maxInt(u64)) return error.Exhausted;
    for (&providers, 0..) |*provider, i| if (provider.handle.id == 0) {
        provider_serial += 1;
        provider.* = .{ .handle = .{ .id = @intCast(i + 1), .generation = provider_serial }, .owner = owner, .config = config };
        return provider.handle;
    };
    return error.Capacity;
}
pub fn unregister(owner: buffers.Owner, handle: buffers.Handle) Error!void {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    const busy = blk: {
        buffers.lock();
        defer buffers.unlock();
        const provider = try providerLocked(handle, owner);
        provider.closing = true;
        service_needed = true;
        state.closeProvider(handle, instant);
        if (heldLocked(provider)) break :blk true;
        provider.* = .{};
        break :blk false;
    };
    queue.wake();
    if (busy) return error.Busy;
}
pub fn closingDriver(id: u32) void {
    const instant = now();
    buffers.lock();
    service_needed = true;
    for (&providers) |*provider| if (id != 0 and provider.owner.id == id) {
        provider.closing = true;
        provider.retiring = true;
        state.closeProvider(provider.handle, instant);
    };
    buffers.unlock();
    queue.wake();
}
pub fn retainsDriver(id: u32) bool {
    buffers.lock();
    defer buffers.unlock();
    for (&providers) |provider| if (id != 0 and provider.owner.id == id) return true;
    return false;
}
pub fn start(owner: buffers.Owner, input: abi.GfxNativeAllocation) Error!abi.GfxNativeStatus {
    if (!queue.available() or irq.inDispatch()) return error.Unavailable;
    const instant = now();
    try model.validate(input, instant);
    const result = blk: {
        buffers.lock();
        defer buffers.unlock();
        for (&providers) |provider| if (provider.handle.id != 0 and provider.config.adapter_id == input.adapter_id) {
            if (provider.closing) return error.Closed;
            if (provider.config.memory_generation != input.memory_generation) return error.Stale;
            try memory.admitLocked(provider.owner);
            const entry = try state.start(owner, provider.handle, input, instant);
            service_needed = true;
            events[entry.handle.id - 1] = sync.Event.init(false);
            break :blk entry.status();
        };
        return error.Unsupported;
    };
    queue.wake();
    return result;
}
pub fn query(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxNativeStatus {
    buffers.lock();
    defer buffers.unlock();
    return (try state.owned(handle, owner)).status();
}
pub fn receive(owner: buffers.Owner, handle: buffers.Handle, output: *abi.GfxBufferReference) i32 {
    const instant = now();
    const result = blk: {
        buffers.lock();
        defer buffers.unlock();
        const entry = state.owned(handle, owner) catch |err| return errorCode(err);
        if (entry.phase != .terminal) return abi.gfx_buffer_error_busy;
        if (entry.result != 1) return entry.result;
        const reference = buffers.store.share(entry.reference, owner) catch |err| return errorCode(err);
        const value = api.referenceLocked(reference, owner) catch unreachable;
        const retained = state.close(handle, owner, instant) catch unreachable;
        service_needed = true;
        buffers.store.drop(retained, model.result_owner) catch unreachable;
        break :blk value;
    };
    output.* = result;
    queue.wake();
    return 1;
}
fn closeLocked(owner: buffers.Owner, handle: buffers.Handle, instant: u64) Error!void {
    const reference = try state.close(handle, owner, instant);
    service_needed = true;
    if (reference.id != 0) buffers.store.drop(reference, model.result_owner) catch unreachable;
}
pub fn close(owner: buffers.Owner, handle: buffers.Handle) Error!void {
    const instant = now();
    buffers.lock();
    closeLocked(owner, handle, instant) catch |err| {
        buffers.unlock();
        return err;
    };
    buffers.unlock();
    queue.wake();
}
pub fn stopped(owner: buffers.Owner) void {
    const instant = now();
    buffers.lock();
    for (&state.entries) |*entry| if (entry.handle.id != 0 and entry.open and entry.owner.eql(owner))
        closeLocked(owner, entry.handle, instant) catch unreachable;
    buffers.unlock();
    queue.wake();
}
pub fn take(owner: buffers.Owner, provider_handle: buffers.Handle) Error!abi.GfxNativeJob {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    buffers.lock();
    defer buffers.unlock();
    try memory.admitLocked(owner);
    const provider = try providerLocked(provider_handle, owner);
    if (provider.closing) return error.Closed;
    const entry = state.take(provider_handle, instant) orelse return error.Busy;
    return .{ .request = api.publicHandle(entry.handle), .allocation = entry.allocation };
}
pub fn complete(owner: buffers.Owner, provider_handle: buffers.Handle, handle: buffers.Handle, result: i32, reference: abi.GfxBufferHandle) Error!void {
    if (irq.inDispatch()) return error.Unavailable;
    if (result == 0 or result > 1 or result < abi.gfx_queue_error_device_lost or reference.reserved0 != 0) return error.Invalid;
    if (result != 1 and (reference.id != 0 or reference.generation != 0)) return error.Invalid;
    const instant = now();
    {
        buffers.lock();
        defer buffers.unlock();
        _ = try providerLocked(provider_handle, owner);
        const entry = try state.claim(handle, provider_handle);
        state.expire(instant);
        var outcome = result;
        if (result == 1) {
            const source = try api.handle(reference);
            // Even a late completion must name a real, exact driver reference.
            const descriptor = try buffers.store.describe(source, owner);
            if (!model.matches(entry.allocation, descriptor, owner) or try buffers.store.readOnly(source, owner) or
                try buffers.store.mappingOnly(source, owner)) return error.Invalid;
            if (entry.phase != .terminal) {
                entry.reference = buffers.store.share(source, model.result_owner) catch |err| blk: {
                    outcome = api.status(err);
                    break :blk .{};
                };
            }
        }
        try state.finish(handle, provider_handle, outcome, instant);
        service_needed = true;
    }
    queue.wake();
}
pub fn retiredTask(id: u32, generation: u64) void {
    buffers.lock();
    service_needed = true;
    for (&waiters) |*waiter| if (waiter.task_id == id and waiter.generation == generation) {
        const entry = state.find(waiter.request) catch unreachable;
        std.debug.assert(entry.waiters != 0);
        entry.waiters -= 1;
        waiter.* = .{};
    };
    buffers.unlock();
    queue.wake();
}
pub fn wait(owner: buffers.Owner, handle: buffers.Handle, timeout_ticks: u64) Error!abi.GfxNativeStatus {
    if (irq.inDispatch()) return error.Unavailable;
    const current = scheduler.current() orelse return error.Unavailable;
    const slot = blk: {
        buffers.lock();
        defer buffers.unlock();
        const entry = try state.owned(handle, owner);
        if (entry.phase == .terminal) return entry.status();
        for (&waiters, 0..) |*waiter, i| if (waiter.task_id == 0) {
            entry.waiters += 1;
            waiter.* = .{ .task_id = current.id, .generation = current.generation, .request = handle };
            break :blk i;
        };
        return error.Capacity;
    };
    const waited = events[handle.id - 1].waitResult(timeout_ticks);
    const snapshot = blk: {
        buffers.lock();
        defer buffers.unlock();
        const entry = state.find(handle) catch unreachable;
        entry.waiters -= 1;
        waiters[slot] = .{};
        service_needed = true;
        break :blk entry.status();
    };
    queue.wake();
    if (snapshot.phase == @intFromEnum(model.Phase.terminal)) return snapshot;
    return if (waited == .timeout) error.WaitTimeout else error.WaitCancelled;
}
fn notifyDriver(index: usize) callconv(.c) i32 {
    if (index >= providers.len) return -1;
    const id = work.currentOwner();
    if (id == 0) return -1;
    if (!@import("driver_api.zig").enterOwnerBounded(id, @as(u64, @max(@import("timer.zig").frequency(), 1)) * 3)) return -1;
    defer _ = @import("driver_api.zig").leaveOwner();
    buffers.lock();
    const selected = providers[index];
    buffers.unlock();
    if (selected.owner.id != id or selected.closing) return 0;
    const callback: work.WorkHandler = @ptrFromInt(selected.config.notify);
    return callback(selected.config.context);
}
// Called by the sole existing gfx-work thread. Never invokes driver code or
// task wake-all while holding the BO metadata owner.
pub fn service() bool {
    buffers.lock();
    const needed = service_needed;
    buffers.unlock();
    if (!needed) return false;
    const instant = now();
    buffers.lock();
    state.expire(instant);
    buffers.unlock();
    for (0..capacity) |i| {
        buffers.lock();
        const entry = &state.entries[i];
        const publish = entry.handle.id != 0 and entry.notification_pending and !entry.publishing;
        if (publish) {
            entry.notification_pending = false;
            entry.publishing = true;
        }
        buffers.unlock();
        if (publish) {
            events[i].signal();
            buffers.lock();
            state.entries[i].publishing = false;
            buffers.unlock();
        }
    }
    for (0..providers.len) |i| {
        buffers.lock();
        const snapshot = providers[i];
        buffers.unlock();
        if (snapshot.handle.id == 0) continue;
        if (snapshot.work_handle != 0) {
            var status: work.CompletionStatus = .{};
            const rc = work.completionStatus(snapshot.work_handle, &status);
            if (rc != 0 or status.state == work.WORK_STATE_COMPLETED or status.state == work.WORK_STATE_CANCELLED) {
                if (rc != 0 or work.completionRelease(snapshot.work_handle) == 0) {
                    buffers.lock();
                    if (providers[i].handle.eql(snapshot.handle) and providers[i].work_handle == snapshot.work_handle) providers[i].work_handle = 0;
                    buffers.unlock();
                }
            }
            continue;
        }
        buffers.lock();
        const provider = &providers[i];
        var ready = false;
        if (!provider.closing and !provider.notifying and provider.handle.eql(snapshot.handle)) {
            for (&state.entries) |request| if (request.handle.id != 0 and request.phase == .queued and request.provider.eql(provider.handle)) {
                ready = true;
                break;
            };
        }
        if (ready) provider.notifying = true;
        buffers.unlock();
        if (!ready) continue;
        var handle: u32 = 0;
        const rc = work.submit(@intCast(snapshot.owner.id), notifyDriver, i, 0, &handle);
        buffers.lock();
        std.debug.assert(providers[i].handle.eql(snapshot.handle));
        providers[i].notifying = false;
        providers[i].work_handle = if (rc == 0) handle else 0;
        buffers.unlock();
    }
    buffers.lock();
    defer buffers.unlock();
    state.reap();
    var pending = false;
    for (&providers) |*provider| if (provider.handle.id != 0) {
        if (provider.retiring and !heldLocked(provider)) {
            provider.* = .{};
        } else if (provider.work_handle != 0 or provider.notifying) {
            pending = true;
        }
    };
    for (&state.entries) |entry| if (entry.handle.id != 0 and (entry.phase != .terminal or entry.notification_pending)) {
        pending = true;
    };
    service_needed = pending;
    return pending;
}
