// Resident cross-process transport. The common BO owner protects metadata;
// the existing gfx-work thread publishes events and notifies the sole R4D.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const model = @import("../memory/gfx_virtual_state.zig");
const heap = @import("../memory/heap.zig");
const memory = @import("gfx_driver_memory.zig");
const queue = @import("../display/queue.zig");
const work = @import("driver_work.zig");
const sync = @import("../sched/sync.zig");
const scheduler = @import("../sched/scheduler.zig");
const context = @import("../sched/task_context.zig");
const irq = @import("irq_router.zig");
pub const Error = buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled };
const Record = struct {
    value: model.Entry = .{},
    ready: sync.Event = sync.Event.init(false),
    retired: sync.Event = sync.Event.init(false),
    publishing: bool = false,
    waiters: usize = 0,
    reference: abi.GfxBufferReference = .{},
    driver: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
};
const Waiter = struct {
    next: ?*Waiter = null,
    previous: ?*Waiter = null,
    record: *Record,
    task_id: u32,
    generation: u64,
};
const Provider = struct {
    handle: buffers.Handle = .{},
    owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    config: abi.GfxNativeProvider = .{},
    closing: bool = false,
    retiring: bool = false,
    notifying: bool = false,
    work_handle: u32 = 0,
};
var state: model.Store = .{};
var waiters: ?*Waiter = null;
var providers: [16]Provider = .{Provider{}} ** 16;
var provider_serial: u64 = 0;
var service_needed = false;
fn now() u64 { return @import("../platform/monotonic.zig").nowNanoseconds() orelse 0; }
pub fn errorCode(err: Error) i32 { return @import("gfx_allocations.zig").errorCode(err); }
fn record(entry: *model.Entry) *Record { return @fieldParentPtr("value", entry); }
fn allocate(comptime T: type) Error!*T {
    const allocation = heap.alloc(@sizeOf(T), @alignOf(T)) orelse return error.OutOfMemory;
    return @ptrCast(@alignCast(allocation.ptr));
}
fn release(comptime T: type, value: *T) void {
    const bytes: [*]u8 = @ptrCast(value);
    std.debug.assert(heap.free(bytes[0..@sizeOf(T)]) == .ok);
}
fn providerLocked(handle: buffers.Handle, owner: buffers.Owner) Error!*Provider {
    if (handle.id == 0 or handle.id > providers.len) return error.Stale;
    const value = &providers[handle.id - 1];
    if (!value.handle.eql(handle)) return error.Stale;
    if (!value.owner.eql(owner)) return error.WrongOwner;
    return value;
}
fn heldLocked(provider: *const Provider) bool {
    if (provider.notifying or provider.work_handle != 0) return true;
    var iter = state.tree.inorderIterator();
    while (iter.next()) |node| {
        const entry: *model.Entry = @fieldParentPtr("index", node);
        if (entry.provider.eql(provider.handle) and (!entry.retired or record(entry).reference.reference.id != 0)) return true;
    }
    return false;
}
pub fn register(owner: buffers.Owner, config: abi.GfxNativeProvider) Error!buffers.Handle {
    if (!queue.available() or irq.inDispatch()) return error.Unavailable;
    if (config.version != 1 or config.size < @sizeOf(abi.GfxNativeProvider) or config.reserved0 != 0 or
        config.adapter_id == 0 or config.memory_generation == 0 or config.notify == 0) return error.Invalid;
    buffers.lock(); defer buffers.unlock();
    try memory.admitLocked(owner);
    for (&providers) |provider| if (provider.handle.id != 0 and provider.config.adapter_id == config.adapter_id) return error.Busy;
    if (provider_serial == std.math.maxInt(u64)) return error.Exhausted;
    for (&providers, 0..) |*provider, index| if (provider.handle.id == 0) {
        provider_serial += 1;
        provider.* = .{ .handle = .{ .id = @intCast(index + 1), .generation = provider_serial }, .owner = owner, .config = config };
        return provider.handle;
    };
    return error.Capacity;
}
pub fn unregister(owner: buffers.Owner, handle: buffers.Handle) Error!void {
    if (irq.inDispatch()) return error.Unavailable;
    const busy = blk: {
        buffers.lock(); defer buffers.unlock();
        const provider = try providerLocked(handle, owner);
        provider.closing = true;
        state.closeProvider(handle);
        service_needed = true;
        if (heldLocked(provider)) break :blk true;
        provider.* = .{};
        break :blk false;
    };
    queue.wake();
    if (busy) return error.Busy;
}
pub fn closingDriver(id: u32) void {
    buffers.lock();
    for (&providers) |*provider| if (id != 0 and provider.owner.id == id) {
        provider.closing = true; provider.retiring = true;
        state.closeProvider(provider.handle);
        service_needed = true;
    };
    buffers.unlock(); queue.wake();
}
pub fn retainsDriver(id: u32) bool {
    buffers.lock(); defer buffers.unlock();
    for (&providers) |provider| if (id != 0 and provider.owner.id == id) return true;
    return false;
}
// The program owner closes this bit before publishing exit. Admission reads
// it under the same BO owner; no process tombstones or caller pointer persist.
pub fn stopAdmission(closed: *bool) void {
    buffers.lock(); closed.* = true; buffers.unlock();
}
pub fn start(owner: buffers.Owner, closed: *const bool, input: abi.GfxVirtualRequest) Error!abi.GfxVirtualStatus {
    if (!queue.available() or irq.inDispatch()) return error.Unavailable;
    const instant = now();
    try model.validate(input, instant);
    const value = try allocate(Record);
    value.* = .{};
    errdefer release(Record, value);
    const result = blk: {
        try buffers.lockPrepared(.{ .references = if (input.kind == 2) 1 else 0 });
        defer buffers.unlock();
        if (closed.*) return error.Closed;
        const provider = for (&providers) |*candidate| {
            if (candidate.handle.id != 0 and candidate.config.adapter_id == input.adapter_id) break candidate;
        } else return error.Unsupported;
        if (provider.closing) return error.Closed;
        if (provider.config.memory_generation != input.memory_generation) return error.Stale;
        try memory.admitLocked(provider.owner);
        _ = try state.parent(owner, provider.handle, input);
        value.driver = provider.owner;
        if (input.kind == 2) {
            const reference = try api.handle(input.reference);
            const descriptor = try buffers.store.describe(reference, owner);
            if (try buffers.store.readOnly(reference, owner) or try buffers.store.mappingOnly(reference, owner)) return error.Unsupported;
            if (input.byte_length > descriptor.bytes or input.byte_offset > descriptor.bytes - input.byte_length) return error.Invalid;
            if (!descriptor.binding.portable() and (descriptor.binding.adapter != input.adapter_id or
                descriptor.binding.device_generation != input.memory_generation or descriptor.binding.driver_owner != provider.owner.id)) return error.Stale;
            const borrowed = try buffers.store.share(reference, provider.owner);
            value.reference = api.referenceLocked(borrowed, provider.owner) catch unreachable;
        }
        errdefer if (value.reference.reference.id != 0) buffers.store.drop(api.handle(value.reference.reference) catch unreachable, provider.owner) catch unreachable;
        try state.start(&value.value, owner, provider.handle, input, instant);
        service_needed = true;
        break :blk value.value.status();
    };
    queue.wake(); return result;
}
pub fn query(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxVirtualStatus {
    buffers.lock(); defer buffers.unlock();
    return (try state.owned(handle, owner)).status();
}
pub fn close(owner: buffers.Owner, handle: buffers.Handle, mode: u32) Error!void {
    if (mode > 1) return error.Invalid;
    buffers.lock();
    const entry = state.owned(handle, owner) catch |err| { buffers.unlock(); return err; };
    entry.close(mode == 1, abi.gfx_queue_error_wait_cancelled);
    service_needed = true;
    buffers.unlock(); queue.wake();
}
pub fn stopped(owner: buffers.Owner) void {
    buffers.lock();
    state.stopped(owner); service_needed = true;
    buffers.unlock(); queue.wake();
}
pub fn take(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxVirtualJob {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    buffers.lock(); defer buffers.unlock();
    const provider = try providerLocked(handle, owner);
    if (!provider.closing) try memory.admitLocked(owner);
    const entry = state.take(handle, instant) orelse return error.Busy;
    service_needed = true;
    return .{ .resource = api.publicHandle(entry.handle), .operation = @intFromEnum(entry.claim.?), .request = entry.request,
        .parent_token = if (entry.parent) |parent| parent.token else .{}, .token = entry.token, .reference = record(entry).reference };
}
pub fn complete(owner: buffers.Owner, handle: buffers.Handle, completion: abi.GfxVirtualCompletion) Error!void {
    if (irq.inDispatch()) return error.Unavailable;
    const instant = now();
    {
        buffers.lock(); defer buffers.unlock();
        _ = try providerLocked(handle, owner);
        state.expire(instant);
        try state.complete(handle, completion);
        service_needed = true;
    }
    queue.wake();
}
fn unlinkWaiterLocked(waiter: *Waiter) void {
    if (waiter.previous) |previous| previous.next = waiter.next else waiters = waiter.next;
    if (waiter.next) |next| next.previous = waiter.previous;
    std.debug.assert(waiter.record.waiters != 0);
    waiter.record.waiters -= 1;
    service_needed = true;
}
pub fn retiredTask(id: u32, generation: u64) void {
    var detached: ?*Waiter = null;
    buffers.lock();
    var next = waiters;
    while (next) |waiter| {
        next = waiter.next;
        if (waiter.task_id != id or waiter.generation != generation) continue;
        unlinkWaiterLocked(waiter);
        waiter.next = detached; detached = waiter;
    }
    buffers.unlock();
    while (detached) |waiter| { detached = waiter.next; release(Waiter, waiter); }
    queue.wake();
}
pub fn wait(owner: buffers.Owner, handle: buffers.Handle, until: u32, timeout: u64) Error!abi.GfxVirtualStatus {
    if (until > 1) return error.Invalid;
    if (irq.inDispatch()) return error.Unavailable;
    const current = scheduler.current() orelse return error.Unavailable;
    const waiter = blk: {
        const guard = context.enterUnwind();
        if (!guard.admitted()) return error.WaitCancelled;
        defer _ = context.leaveUnwind(guard);
        const value = try allocate(Waiter);
        errdefer release(Waiter, value);
        buffers.lock();
        const entry = state.owned(handle, owner) catch |err| { buffers.unlock(); return err; };
        if (entry.settled(until)) {
            const result = entry.status();
            buffers.unlock(); release(Waiter, value); return result;
        }
        const target = record(entry);
        if (target.waiters == std.math.maxInt(usize)) { buffers.unlock(); return error.Exhausted; }
        value.* = .{ .record = target, .task_id = current.id, .generation = current.generation, .next = waiters };
        if (waiters) |first| first.previous = value;
        waiters = value; target.waiters += 1;
        buffers.unlock();
        break :blk value;
    };
    // The registered waiter pins both events across hard kill and publication.
    const event = if (until == 0) &waiter.record.ready else &waiter.record.retired;
    const waited = event.waitResult(timeout);
    const guard = context.enterUnwind();
    if (!guard.admitted()) return error.WaitCancelled; // exact retirement owns it
    defer _ = context.leaveUnwind(guard);
    buffers.lock();
    const result = waiter.record.value.status();
    const done = waiter.record.value.settled(until);
    unlinkWaiterLocked(waiter);
    buffers.unlock();
    release(Waiter, waiter); queue.wake();
    if (done) return result;
    return if (waited == .timeout) error.WaitTimeout else error.WaitCancelled;
}
fn notifyDriver(index: usize) callconv(.c) i32 {
    if (index >= providers.len) return -1;
    const id = work.currentOwner();
    if (id == 0 or !@import("driver_api.zig").enterOwnerBounded(id, @as(u64, @max(@import("timer.zig").frequency(), 1)) * 3)) return -1;
    defer _ = @import("driver_api.zig").leaveOwner();
    buffers.lock(); const selected = providers[index]; buffers.unlock();
    if (selected.owner.id != id or selected.handle.id == 0) return 0;
    const callback: work.WorkHandler = @ptrFromInt(selected.config.notify);
    return callback(selected.config.context);
}
pub fn service() bool {
    buffers.lock(); const needed = service_needed; buffers.unlock();
    if (!needed) return false;
    const instant = now();
    buffers.lock(); state.expire(instant); var next = state.tree.getMin(); buffers.unlock();
    var dropped = false;
    // Only this worker unlinks resource nodes. Admissions append new serials;
    // caller close/retirement only changes state under the common owner.
    while (next) |node| {
        buffers.lock();
        const entry: *model.Entry = @fieldParentPtr("index", node);
        const value = record(entry);
        next = node.next();
        const publish = entry.notifications;
        entry.notifications = 0;
        value.publishing = publish != 0;
        buffers.unlock();
        if (publish & 1 != 0) value.ready.signal();
        if (publish & 2 != 0) value.retired.signal();
        buffers.lock();
        value.publishing = false;
        if (entry.retired and value.reference.reference.id != 0) {
            buffers.store.drop(api.handle(value.reference.reference) catch unreachable, value.driver) catch unreachable;
            value.reference = .{}; dropped = true;
        }
        const remove = model.Store.removable(entry) and value.waiters == 0 and !value.publishing;
        if (remove) state.remove(entry);
        buffers.unlock();
        if (remove) release(Record, value);
    }
    if (dropped) buffers.collect();
    for (0..providers.len) |index| {
        buffers.lock(); const snapshot = providers[index]; buffers.unlock();
        if (snapshot.handle.id == 0) continue;
        if (snapshot.work_handle != 0) {
            var status: work.CompletionStatus = .{};
            const rc = work.completionStatus(snapshot.work_handle, &status);
            if (rc != 0 or status.state == work.WORK_STATE_COMPLETED or status.state == work.WORK_STATE_CANCELLED) {
                if (rc != 0 or work.completionRelease(snapshot.work_handle) == 0) {
                    buffers.lock();
                    if (providers[index].handle.eql(snapshot.handle) and providers[index].work_handle == snapshot.work_handle) providers[index].work_handle = 0;
                    buffers.unlock();
                }
            }
            continue;
        }
        buffers.lock();
        const provider = &providers[index];
        var ready = false;
        if (!provider.notifying and provider.handle.eql(snapshot.handle)) {
            var iter = state.tree.inorderIterator();
            while (iter.next()) |node| {
                const entry: *model.Entry = @fieldParentPtr("index", node);
                if (entry.provider.eql(provider.handle) and entry.pending() != null) { ready = true; break; }
            }
        }
        if (ready) provider.notifying = true;
        buffers.unlock();
        if (!ready) continue;
        var handle: u32 = 0;
        const rc = work.submit(@intCast(snapshot.owner.id), notifyDriver, index, 0, &handle);
        buffers.lock();
        std.debug.assert(providers[index].handle.eql(snapshot.handle));
        providers[index].notifying = false;
        providers[index].work_handle = if (rc == 0) handle else 0;
        buffers.unlock();
    }
    buffers.lock(); defer buffers.unlock();
    var pending = false;
    for (&providers) |*provider| if (provider.handle.id != 0) {
        if (provider.retiring and !heldLocked(provider)) provider.* = .{}
        else if (provider.work_handle != 0 or provider.notifying) { pending = true; }
    };
    var iter = state.tree.inorderIterator();
    while (iter.next()) |node| {
        const entry: *model.Entry = @fieldParentPtr("index", node);
        if (entry.result == 0 or entry.claim != null or entry.closing and !entry.retired or entry.notifications != 0 or
            model.Store.removable(entry) and record(entry).waiters == 0) pending = true;
    }
    service_needed = pending;
    return pending;
}
