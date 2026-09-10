const std = @import("std");
const a = @import("r4os_kernel_contract");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const io = @import("../arch/x86_64/io.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const task = @import("../sched/task.zig");
const task_context = @import("../sched/task_context.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const heap = @import("../memory/heap.zig");
const timer = @import("timer.zig");
const ownership = @import("driver_thread_owner.zig");
const callback_abort = @import("../arch/x86_64/callback_abort.zig");
const Handler = *const fn (usize) callconv(.c) i32;
const Payload = struct {
    handler: Handler,
    context: usize,
    flags: u32,
    task_ptr: ?*task.Task = null,
    task_id: u32 = 0,
    task_generation: u64 = 0,
    published: bool = false,
    abandoned: bool = false,
    cpu_index: u32 = std.math.maxInt(u32),
    completion: sync.WaitQueue = .{},
    sleep_queue: sync.WaitQueue = .{},
    waiting_on: ?*sync.WaitQueue = null,
    abort_frame: ?*callback_abort.Frame = null,
};
const State = ownership.State(@import("../driver/registry.zig").MAX_DRIVERS, Payload);
const Record = State.Record;
var state: State = .{};

// Only resident metadata and wait enrollment use the scheduler runtime
// owner. Task/stack/FPU construction, retirement, heap operations and module
// callbacks always run outside it. No global R4D lifecycle lock is borrowed.
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
    return if (value.closing) a.driver_thread_error_closed else a.driver_thread_ok;
}

pub fn currentOwner() u32 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = currentRecordLocked() orelse return 0;
    return record.owner;
}

pub fn current(owner: u32) u64 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = currentRecordLocked() orelse return 0;
    return if (record.owner == owner) record.handle else 0;
}

pub fn start(owner: u32, input: *const a.DriverThreadRequest, output: *u64) i32 {
    output.* = 0;
    const request = input.*;
    if (!valid(a.DriverThreadRequest, &request) or request.handler == 0 or request.reserved != 0 or
        request.flags & ~(a.driver_thread_flag_parallel | a.driver_thread_flag_abortable) != 0) return a.driver_thread_error_invalid;
    if (!canSleep()) return a.driver_thread_error_context;
    const unwind = enterOperation() orelse return a.driver_thread_error_busy;
    defer leaveOperation(unwind);
    var create = reserve: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :reserve state.begin(owner) catch |err| return code(err);
    };
    const bytes = heap.alloc(@sizeOf(Record), @alignOf(Record)) orelse {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.abort(&create) catch unreachable;
        return a.driver_thread_error_memory;
    };
    const record: *Record = @ptrCast(@alignCast(bytes.ptr));
    record.* = .{ .payload = .{ .handler = @ptrFromInt(request.handler), .context = request.context, .flags = request.flags } };
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.adopt(&create, record) catch unreachable;
    }
    // The initial unwind count is installed before the Task enters its
    // registry. Neither external kill nor a close race may free its context.
    const created = task.createDriverThreadBlocked(threadMain, record, request.flags & a.driver_thread_flag_parallel != 0);
    const result = publish: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (created) |value| {
            record.payload.task_ptr = value;
            record.payload.task_id = value.id;
            record.payload.task_generation = value.generation;
        }
        const failure = if (created == null) a.driver_thread_error_memory else a.driver_thread_error_closed;
        if (!(state.prepared(record, created != null, failure) catch unreachable)) break :publish failure;
        if (!scheduler.publishCreatedTask(created.?)) {
            state.failPublication(record, a.driver_thread_error_busy) catch unreachable;
            break :publish a.driver_thread_error_busy;
        }
        record.payload.published = true;
        output.* = record.handle;
        break :publish a.driver_thread_ok;
    };
    if (result != a.driver_thread_ok) {
        // A failed retirement/free remains indexed privately for owner cleanup.
        _ = remove(owner, create.handle, true);
    }
    return result;
}

pub fn stop(owner: u32, handle: u64) i32 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = publicRecord(owner, handle) catch |err| return code(err);
    _ = state.stop(owner, handle) catch unreachable;
    cancelWaitLocked(record);
    return a.driver_thread_ok;
}

// Synchronous self-abort only. The record belongs to the actual executing
// Task, never to a caller-supplied Task ID or a per-CPU guess. No kernel frame
// owning a lock, wait lease or extra unwind token may be skipped.
pub fn abortCurrent(owner: u32, result: i32) i32 {
    if (result >= 0) return a.driver_thread_error_invalid;
    if (!canSleep()) return a.driver_thread_error_context;
    const frame = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const record = currentRecordLocked() orelse return a.driver_thread_error_context;
        if (record.owner != owner) return a.driver_thread_error_owner;
        const current_task = scheduler.current() orelse unreachable;
        if (current_task.unwind_guard_count != 1 or record.payload.waiting_on != null)
            return a.driver_thread_error_busy;
        const frame = record.payload.abort_frame orelse return a.driver_thread_error_context;
        if (record.payload.flags & a.driver_thread_flag_abortable == 0 or frame.rsp == 0)
            return a.driver_thread_error_context;
        record.payload.flags |= a.driver_thread_flag_aborted;
        break :capture frame;
    };
    // The initial Task guard still retains the module, record, stack and FPU
    // state. The normal threadMain epilogue owns completion and retirement.
    callback_abort.returnToCaller(frame, result);
}

pub fn status(owner: u32, handle: u64, output: *a.DriverThreadStatus) i32 {
    if (!valid(a.DriverThreadStatus, output)) return a.driver_thread_error_invalid;
    output.* = .{};
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = publicRecord(owner, handle) catch |err| return code(err);
    output.* = .{
        .handle = record.handle,
        .owner_epoch = record.epoch,
        .task_generation = record.payload.task_generation,
        .task_id = record.payload.task_id,
        .cpu_index = record.payload.cpu_index,
        .state = @intFromEnum(record.phase),
        .stop_requested = @intFromBool(state.stopping(record) catch unreachable),
        .result = record.result,
        .flags = record.payload.flags,
        .waiters = record.references,
    };
    return a.driver_thread_ok;
}

pub fn stats(owner: u32, output: *a.DriverThreadStats) i32 {
    if (!valid(a.DriverThreadStats, output)) return a.driver_thread_error_invalid;
    output.* = .{};
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const value = state.stats(owner) catch |err| return code(err);
    output.* = .{
        .owner_epoch = value.epoch,
        .records = value.records,
        .active = value.active,
        .completed = value.completed,
        .private_records = value.private,
        .starts = value.starts,
        .start_failures = value.start_failures,
        .releases = value.releases,
        .release_retries = value.release_failures,
        .pending_creates = value.pending_creates,
        .pending_releases = value.pending_releases,
        .waiters = value.leases,
        .closing = @intFromBool(value.closing),
    };
    return a.driver_thread_ok;
}

const JoinWait = struct { target: *Record, caller: ?*Record };
pub fn join(owner: u32, handle: u64, timeout_ticks: u64, output: *i32) i32 {
    output.* = 0;
    if (timeout_ticks == sync.WAIT_FOREVER) return a.driver_thread_error_invalid;
    if (!canSleep()) return a.driver_thread_error_context;
    const unwind = enterOperation() orelse return a.driver_thread_error_busy;
    defer leaveOperation(unwind);
    var wait = enroll: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const target = publicRecord(owner, handle) catch |err| return code(err);
        const caller = currentRecordLocked();
        if (caller == target) return a.driver_thread_error_self_join;
        _ = state.retain(owner, handle) catch |err| return code(err);
        if (caller) |value| {
            std.debug.assert(value.payload.waiting_on == null);
            value.payload.waiting_on = &target.payload.completion;
        }
        break :enroll JoinWait{ .target = target, .caller = caller };
    };
    const result = wait.target.payload.completion.waitUnless(timeout_ticks, "r4d-join", joinNeeded, &wait);
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    if (wait.caller) |caller| caller.payload.waiting_on = null;
    defer state.unretain(wait.target) catch unreachable;
    if (wait.target.phase == .completed) {
        output.* = wait.target.result;
        return a.driver_thread_ok;
    }
    if (wait.caller) |caller| {
        if (state.stopping(caller) catch unreachable) return a.driver_thread_error_cancelled;
    }
    return switch (result) {
        .timeout => a.driver_thread_error_timeout,
        .cancelled => a.driver_thread_error_cancelled,
        else => a.driver_thread_error_context,
    };
}

fn joinNeeded(context: *anyopaque) bool {
    const value: *JoinWait = @ptrCast(@alignCast(context));
    if (value.target.phase == .completed) return false;
    return if (value.caller) |caller| !(state.stopping(caller) catch unreachable) else true;
}

pub fn sleepTicks(owner: u32, ticks: u64) i32 {
    if (!canSleep()) return a.driver_thread_error_context;
    const record = enroll: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const value = currentRecordLocked() orelse return a.driver_thread_error_context;
        if (value.owner != owner) return a.driver_thread_error_owner;
        if (state.stopping(value) catch unreachable) return a.driver_thread_error_cancelled;
        std.debug.assert(value.payload.waiting_on == null);
        value.payload.waiting_on = &value.payload.sleep_queue;
        break :enroll value;
    };
    const result: sync.WaitResult = if (ticks == 0) yielded: {
        scheduler.yield();
        break :yielded .timeout;
    } else record.payload.sleep_queue.waitUnless(ticks, "r4d-sleep", sleepNeeded, record);
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    record.payload.waiting_on = null;
    record.payload.cpu_index = percpu.currentIndex();
    if (state.stopping(record) catch unreachable) return a.driver_thread_error_cancelled;
    return switch (result) {
        .timeout => a.driver_thread_ok,
        .cancelled => a.driver_thread_error_cancelled,
        else => a.driver_thread_error_context,
    };
}

fn sleepNeeded(context: *anyopaque) bool {
    const record: *Record = @ptrCast(@alignCast(context));
    return !(state.stopping(record) catch unreachable);
}

pub fn release(owner: u32, handle: u64) i32 {
    if (!canSleep()) return a.driver_thread_error_context;
    const unwind = enterOperation() orelse return a.driver_thread_error_busy;
    defer leaveOperation(unwind);
    return remove(owner, handle, false);
}

fn remove(owner: u32, handle: u64, allow_private: bool) i32 {
    var ticket = detach: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (!allow_private) _ = publicRecord(owner, handle) catch |err| return code(err);
        const caller = currentRecordLocked();
        if (caller != null and caller.?.handle == handle) return a.driver_thread_error_self_join;
        break :detach state.beginRemove(owner, handle) catch |err| return code(err);
    };
    const result = releaseBacking(ticket.record);
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    // On success record backing is already invalid; use copied accounting.
    state.finishRemove(&ticket, result == a.driver_thread_ok) catch unreachable;
    return result;
}

fn releaseBacking(record: *Record) i32 {
    const value = &record.payload;
    if (value.task_id != 0) {
        if (!value.published and !value.abandoned) {
            if (!task.abandonDriverThread(value.task_id, value.task_generation, record)) return a.driver_thread_error_busy;
            value.abandoned = true;
        }
        if (task.retireIdentity(value.task_id, value.task_generation) == .pending) return a.driver_thread_error_busy;
    }
    const bytes: [*]u8 = @ptrCast(record);
    return if (heap.free(bytes[0..@sizeOf(Record)]) == .ok) a.driver_thread_ok else a.driver_thread_error_release;
}

pub fn beginClose(owner: u32) void {
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.close(owner);
    }
    var cursor: u64 = 0;
    while (true) {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const record = (state.after(owner, cursor) catch return) orelse return;
        cursor = record.handle;
        cancelWaitLocked(record);
    }
}

// Top-level shutdown must return from every module callback before generic
// backend, IRQ, work, DMA or CPU-memory teardown is allowed to begin.
pub fn callbacksQuiesced(owner: u32) bool {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const value = state.stats(owner) catch |err| return err == error.Stale;
    return value.closing and value.active == 0 and value.pending_creates == 0;
}

pub const Cleanup = struct { quiesced: bool = true, released: u64 = 0 };
pub fn cleanup(owner: u32) Cleanup {
    var result: Cleanup = .{};
    const count = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const value = state.stats(owner) catch |err| return .{ .quiesced = err == error.Stale };
        if (!value.closing or value.active != 0 or value.pending_creates != 0 or value.pending_releases != 0 or value.leases != 0)
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
    // One finite logical-tick budget for retirement of already returned
    // callbacks. It neither stops a running module nor resets per record.
    const deadline = timer.deadlineAfterNow(1000);
    while (result.released < count) {
        const handle = select: {
            const flags = interrupts.saveAndDisableRuntime();
            defer interrupts.restore(flags);
            const record = (state.after(owner, 0) catch unreachable) orelse break;
            break :select record.handle;
        };
        const removed = remove(owner, handle, true);
        if (removed == a.driver_thread_error_busy and timer.remainingUntil(timer.tickCount(), deadline) != 0) {
            scheduler.sleepTicksWithReason(1, "r4d-retire");
            continue;
        }
        if (removed != a.driver_thread_ok) {
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

fn threadMain() callconv(.c) void {
    const current_task = scheduler.current() orelse unreachable;
    const record: *Record = @ptrCast(@alignCast(task.executionOwner(current_task).context orelse unreachable));
    const admitted = enter: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        std.debug.assert(record.payload.task_id == current_task.id and record.payload.task_generation == current_task.generation);
        std.debug.assert(current_task.unwind_guard_count == 1);
        record.payload.cpu_index = percpu.currentIndex();
        break :enter state.enter(record) catch unreachable;
    };
    var abort_frame: callback_abort.Frame = .{};
    const result = if (!admitted) a.driver_thread_error_cancelled else result: {
        if (record.payload.flags & a.driver_thread_flag_abortable == 0)
            break :result record.payload.handler(record.payload.context);
        record.payload.abort_frame = &abort_frame;
        const result = callback_abort.invoke(record.payload.handler, record.payload.context, &abort_frame);
        record.payload.abort_frame = null;
        break :result result;
    };
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        // Follow the existing scheduler exit invariant. A broken callback
        // must never publish successful quiescence with synchronization held.
        std.debug.assert(current_task.held_lock_count == 0 and current_task.unwind_guard_count == 1);
        std.debug.assert(record.payload.waiting_on == null);
        state.complete(record, result) catch unreachable;
    }
    // The initial Task guard retains the record through these bounded wakes.
    // A completed handle can be observed now, but Task retirement still fails.
    while (record.payload.completion.wakeOne() != 0) {}
    const flags = interrupts.saveAndDisableRuntime();
    current_task.unwind_guard_count -= 1;
    // No access to record after dropping this last execution-lifetime guard.
    interrupts.restore(flags);
}

fn cancelWaitLocked(record: *Record) void {
    if (record.phase != .runnable and record.phase != .running) return;
    const queue = record.payload.waiting_on orelse return;
    _ = queue.cancelTask(record.payload.task_ptr.?, record.payload.task_generation);
}

fn currentRecordLocked() ?*Record {
    const current_task = scheduler.current() orelse return null;
    const execution = task.executionOwner(current_task);
    if (execution.kind != .driver_thread) return null;
    const record: *Record = @ptrCast(@alignCast(execution.context orelse return null));
    if (record.phase != .running or record.payload.task_id != current_task.id or record.payload.task_generation != current_task.generation) return null;
    return record;
}

fn publicRecord(owner: u32, handle: u64) ownership.Error!*Record {
    const record = try state.lookup(owner, handle);
    return if (record.private) error.Stale else record;
}

fn canSleep() bool {
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
fn valid(comptime T: type, output: *const T) bool {
    return output.version == 1 and output.size >= @sizeOf(T);
}
fn code(err: ownership.Error) i32 {
    return switch (err) {
        error.Invalid => a.driver_thread_error_invalid,
        error.Owner => a.driver_thread_error_owner,
        error.Stale => a.driver_thread_error_stale,
        error.Closed => a.driver_thread_error_closed,
        error.Busy => a.driver_thread_error_busy,
        error.Exhausted => a.driver_thread_error_exhausted,
    };
}
