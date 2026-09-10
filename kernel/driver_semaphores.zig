const std = @import("std");
const a = @import("r4os_kernel_contract");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const io = @import("../arch/x86_64/io.zig");
const scheduler = @import("../sched/scheduler.zig");
const task_context = @import("../sched/task_context.zig");
const sync = @import("../sched/sync.zig");
const heap = @import("../memory/heap.zig");
const ownership = @import("driver_semaphore_owner.zig");
const State = ownership.State(@import("../driver/registry.zig").MAX_DRIVERS, sync.Semaphore);
const Record = State.Record;
var state: State = .{};

// Resident identity/count/queue metadata uses the scheduler runtime owner.
// Heap operations and actual parking happen outside it. The acquiring Task
// retains both an unwind token and this exact record until its wait returns.
pub fn bind(owner: u32, epoch: u64) bool {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.bind(owner, epoch) catch return false;
    return true;
}
pub fn discardEmptyBinding(owner: u32, epoch: u64) bool {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const value = state.stats(owner) catch return false;
    if (value.epoch != epoch or value.records != 0 or value.pending_creates != 0) return false;
    state.close(owner);
    state.finish(owner) catch return false;
    return true;
}
pub fn query(owner: u32) i32 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const value = state.stats(owner) catch |err| return code(err);
    return if (value.closing) a.driver_semaphore_error_closed else a.driver_semaphore_ok;
}
pub fn create(owner: u32, initial: u32, maximum: u32, output: *u64) i32 {
    output.* = 0;
    if (maximum == 0 or initial > maximum) return a.driver_semaphore_error_invalid;
    if (!canSleep()) return a.driver_semaphore_error_context;
    const unwind = enterOperation() orelse return a.driver_semaphore_error_busy;
    defer leaveOperation(unwind);
    var ticket = reserve: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :reserve state.begin(owner) catch |err| return code(err);
    };
    const bytes = heap.alloc(@sizeOf(Record), @alignOf(Record)) orelse {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.abort(&ticket) catch unreachable;
        return a.driver_semaphore_error_memory;
    };
    const record: *Record = @ptrCast(@alignCast(bytes.ptr));
    record.* = .{ .payload = sync.Semaphore.init(initial, maximum) };
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    // A close race retains this unpublished backing for quiesced cleanup.
    if (!(state.adopt(&ticket, record) catch unreachable)) return a.driver_semaphore_error_closed;
    output.* = record.handle;
    return a.driver_semaphore_ok;
}
pub fn acquire(owner: u32, handle: u64, timeout_ticks: u64) i32 {
    if (timeout_ticks == 0) {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const record = state.visible(owner, handle) catch |err| return code(err);
        return if (record.payload.tryAcquire()) a.driver_semaphore_ok else a.driver_semaphore_error_timeout;
    }
    if (!canSleep()) return a.driver_semaphore_error_context;
    const unwind = enterOperation() orelse return a.driver_semaphore_error_busy;
    defer leaveOperation(unwind);
    const record = retain: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :retain state.acquire(owner, handle) catch |err| return code(err);
    };
    const result = record.payload.acquire(timeout_ticks);
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.acquired(record) catch unreachable;
    }
    return switch (result) {
        .signaled => a.driver_semaphore_ok,
        .timeout => a.driver_semaphore_error_timeout,
        .cancelled, .killed => a.driver_semaphore_error_cancelled,
        .none, .failed => a.driver_semaphore_error_context,
    };
}
pub fn release(owner: u32, handle: u64) i32 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = state.visible(owner, handle) catch |err| return code(err);
    // Exactly one grant bounds the IRQ path. The underlying semaphore hands
    // a permit directly to one FIFO waiter, with its existing unwind guard.
    return if (record.payload.release(1) == 1) a.driver_semaphore_ok else a.driver_semaphore_error_overflow;
}
pub fn destroy(owner: u32, handle: u64) i32 {
    if (!canSleep()) return a.driver_semaphore_error_context;
    const unwind = enterOperation() orelse return a.driver_semaphore_error_busy;
    defer leaveOperation(unwind);
    return remove(owner, handle, false);
}
fn remove(owner: u32, handle: u64, private: bool) i32 {
    var ticket = detach: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (!private) _ = state.visible(owner, handle) catch |err| return code(err);
        const value = state.beginDestroy(owner, handle) catch |err| return code(err);
        std.debug.assert(value.record.payload.queue.core.count == 0);
        break :detach value;
    };
    const bytes: [*]u8 = @ptrCast(ticket.record);
    const success = heap.free(bytes[0..@sizeOf(Record)]) == .ok;
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.finishDestroy(&ticket, success) catch unreachable;
    return if (success) a.driver_semaphore_ok else a.driver_semaphore_error_release;
}
pub fn status(owner: u32, handle: u64, output: *a.DriverSemaphoreStatus) i32 {
    if (!valid(a.DriverSemaphoreStatus, output)) return a.driver_semaphore_error_invalid;
    output.* = .{};
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = state.visible(owner, handle) catch |err| return code(err);
    output.* = .{ .handle = handle, .owner_epoch = record.epoch, .available = record.payload.count, .maximum = record.payload.max_count, .queued_waiters = @intCast(record.payload.queue.core.count), .active_acquires = record.acquires };
    return a.driver_semaphore_ok;
}
pub fn stats(owner: u32, output: *a.DriverSemaphoreStats) i32 {
    if (!valid(a.DriverSemaphoreStats, output)) return a.driver_semaphore_error_invalid;
    output.* = .{};
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const value = state.stats(owner) catch |err| return code(err);
    output.* = .{ .owner_epoch = value.epoch, .records = value.records, .private_records = value.private, .creates = value.creates, .create_failures = value.create_failures, .destroys = value.destroys, .destroy_failures = value.destroy_failures, .pending_creates = value.pending_creates, .pending_destroys = value.pending_destroys, .active_acquires = value.acquires, .closing = @intFromBool(value.closing) };
    return a.driver_semaphore_ok;
}
pub fn beginClose(owner: u32) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.close(owner);
    // Never close/cancel queues: stop is not a successful semaphore acquire.
}
pub const Cleanup = struct { quiesced: bool = true, released: u64 = 0 };
pub fn cleanup(owner: u32) Cleanup {
    var result: Cleanup = .{};
    const count = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const value = state.stats(owner) catch |err| return .{ .quiesced = err == error.Stale };
        if (!value.closing or value.pending_creates != 0 or value.pending_destroys != 0 or value.acquires != 0)
            return .{ .quiesced = false };
        if (value.records == 0) {
            state.finish(owner) catch unreachable;
            return result;
        }
        break :capture value.records;
    };
    if (!canSleep()) return .{ .quiesced = false };
    const unwind = enterOperation() orelse return .{ .quiesced = false };
    defer leaveOperation(unwind);
    // Call only after actual driver callback/IRQ/work/task quiescence. One
    // record per critical section, no forced cancellation and no retry spin.
    while (result.released < count) {
        const handle = select: {
            const flags = interrupts.saveAndDisableRuntime();
            defer interrupts.restore(flags);
            const record = (state.first(owner) catch unreachable) orelse break;
            break :select record.handle;
        };
        if (remove(owner, handle, true) != a.driver_semaphore_ok) {
            result.quiesced = false;
            return result;
        }
        result.released += 1;
    }
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.finish(owner) catch {
        result.quiesced = false;
    };
    return result;
}
pub fn canSleep() bool {
    if (!interrupts.wereEnabled(io.readRflags()) or interrupts.inRuntimeCriticalSection()) return false;
    const value = scheduler.current() orelse return false;
    return value.preempt_disable_depth == 0 and value.held_lock_count == 0;
}
fn enterOperation() ?task_context.UnwindToken {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const token = task_context.enterUnwind();
    return if (token.admitted()) token else null;
}
fn leaveOperation(token: task_context.UnwindToken) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    std.debug.assert(task_context.leaveUnwind(token));
}
fn valid(comptime T: type, value: *const T) bool {
    return value.version == 1 and value.size >= @sizeOf(T);
}
fn code(err: ownership.Error) i32 {
    return switch (err) {
        error.Invalid => a.driver_semaphore_error_invalid,
        error.Owner => a.driver_semaphore_error_owner,
        error.Stale => a.driver_semaphore_error_stale,
        error.Closed => a.driver_semaphore_error_closed,
        error.Busy => a.driver_semaphore_error_busy,
        error.Exhausted => a.driver_semaphore_error_exhausted,
    };
}
