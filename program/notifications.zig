// Resident wait transport for userland runtimes. Mutex/condition-variable
// policy stays in the calling library; only scheduler enrollment lives here.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const io = @import("../arch/x86_64/io.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task_context = @import("../sched/task_context.zig");
const heap = @import("../memory/heap.zig");
const Tree = std.Treap(u64, std.math.order);

const Record = struct {
    node: Tree.Node = undefined,
    handle: u64,
    sequence: u64 = 0,
    closed: bool = false,
    queue: sync.WaitQueue = .{},
};

// Embedded in the stable ProgramInstance, never in an R4L or caller buffer.
// All its tasks must retire before cleanup; a failed backing release keeps
// the closed record and prevents ProgramInstance reuse.
pub const Owner = struct {
    tree: Tree = .{},
    closing: bool = false,
};
var serial: u64 = 0;

fn find(owner: *Owner, handle: u64) ?*Record {
    const node = owner.tree.getEntryFor(handle).node orelse return null;
    return @fieldParentPtr("node", node);
}
fn active(owner: *Owner, handle: u64) ?*Record {
    const record = find(owner, handle) orelse return null;
    return if (!record.closed and !owner.closing) record else null;
}
fn canSleep() bool {
    if (!interrupts.wereEnabled(io.readRflags()) or interrupts.inRuntimeCriticalSection()) return false;
    const current = scheduler.current() orelse return false;
    return current.preempt_disable_depth == 0 and current.held_lock_count == 0;
}
fn operation() ?task_context.UnwindToken {
    if (!canSleep()) return null;
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const token = task_context.enterUnwind();
    return if (token.admitted()) token else null;
}
fn finishOperation(token: task_context.UnwindToken) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    std.debug.assert(task_context.leaveUnwind(token));
}

pub fn create(owner: *Owner, output: *u64) i32 {
    if (@intFromPtr(output) == 0) return a.notification_error_invalid;
    const token = operation() orelse return a.notification_error_context;
    defer finishOperation(token);
    const handle = reserve: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (owner.closing) return a.notification_error_closed;
        if (serial == std.math.maxInt(u64)) return a.notification_error_exhausted;
        serial += 1;
        break :reserve serial;
    };
    const bytes = heap.alloc(@sizeOf(Record), @alignOf(Record)) orelse return a.notification_error_memory;
    const record: *Record = @ptrCast(@alignCast(bytes.ptr));
    record.* = .{ .handle = handle };
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        // The execution task's unwind token prevents owner retirement across
        // heap allocation and publication. There is no public owner-close.
        std.debug.assert(!owner.closing);
        var entry = owner.tree.getEntryFor(handle);
        std.debug.assert(entry.node == null);
        entry.set(&record.node);
    }
    output.* = handle;
    return a.notification_ok;
}

pub fn query(owner: *Owner, handle: u64, output: *u64) i32 {
    if (handle == 0 or @intFromPtr(output) == 0) return a.notification_error_invalid;
    const value = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const record = find(owner, handle) orelse return a.notification_error_stale;
        if (record.closed or owner.closing) return a.notification_error_closed;
        break :capture record.sequence;
    };
    // The caller may be pageable. Never write it under the runtime owner.
    output.* = value;
    return a.notification_ok;
}

pub fn notify(owner: *Owner, handle: u64, count: u32) i32 {
    if (handle == 0 or count == 0) return a.notification_error_invalid;
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const record = find(owner, handle) orelse return a.notification_error_stale;
    if (record.closed or owner.closing) return a.notification_error_closed;
    if (record.sequence == std.math.maxInt(u64)) {
        record.closed = true;
        _ = record.queue.close(.cancelled);
        return a.notification_error_exhausted;
    }
    record.sequence += 1;
    // The revision and every wake share the same scheduler owner. A signal
    // between a caller's predicate check and enrollment cannot be lost.
    var remaining = count;
    while (remaining != 0 and record.queue.wakeOne() != 0) remaining -= 1;
    return a.notification_ok;
}

const Wait = struct {
    record: *Record,
    observed: u64,
};
fn needed(raw: *anyopaque) bool {
    const request: *const Wait = @ptrCast(@alignCast(raw));
    return request.record.sequence == request.observed;
}
pub fn wait(owner: *Owner, handle: u64, observed: u64, timeout_ticks: u64) i32 {
    if (handle == 0) return a.notification_error_invalid;
    if (!canSleep()) return a.notification_error_context;
    const flags = interrupts.saveAndDisableRuntime();
    const record = active(owner, handle) orelse {
        const code = if (find(owner, handle) != null) a.notification_error_closed else a.notification_error_stale;
        interrupts.restore(flags);
        return code;
    };
    var request: Wait = .{ .record = record, .observed = observed };
    // Transfer the outer runtime owner directly into WaitQueue admission.
    // No unwind token spans this wait: hard kill must remain possible.
    // WaitQueue reads only the stable Task after parking, so close may drain
    // and free this record without leaving a returning waiter with its pointer.
    const result = record.queue.waitUnlessRuntimeLocked(timeout_ticks, "program-notification", needed, &request, flags);
    return switch (result) {
        .signaled => a.notification_ok,
        .timeout => a.notification_timeout,
        .cancelled, .killed => a.notification_error_closed,
        .none, .failed => a.notification_error_context,
    };
}

fn remove(owner: *Owner, handle: u64) i32 {
    const record = detach: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const value = find(owner, handle) orelse return a.notification_error_stale;
        value.closed = true;
        _ = value.queue.close(.cancelled);
        std.debug.assert(value.queue.core.count == 0);
        var entry = owner.tree.getEntryForExisting(&value.node);
        entry.set(null);
        break :detach value;
    };
    const bytes: [*]u8 = @ptrCast(record);
    if (heap.free(bytes[0..@sizeOf(Record)]) == .ok) return a.notification_ok;
    // The failed free kept its backing. Reinsert the same terminal identity
    // for explicit retry or the existing program reaper, never for reuse.
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    var entry = owner.tree.getEntryFor(handle);
    std.debug.assert(entry.node == null);
    entry.set(&record.node);
    return a.notification_error_release;
}
pub fn close(owner: *Owner, handle: u64) i32 {
    if (handle == 0) return a.notification_error_invalid;
    const token = operation() orelse return a.notification_error_context;
    defer finishOperation(token);
    return remove(owner, handle);
}

pub fn cleanup(owner: *Owner) bool {
    // Called by the program reaper after exact retirement of all its tasks.
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        owner.closing = true;
        if (owner.tree.root == null) return true;
    }
    const token = operation() orelse return false;
    defer finishOperation(token);
    while (true) {
        const handle = select: {
            const flags = interrupts.saveAndDisableRuntime();
            defer interrupts.restore(flags);
            owner.closing = true;
            const node = owner.tree.root orelse return true;
            const record: *Record = @fieldParentPtr("node", node);
            std.debug.assert(record.queue.core.count == 0);
            break :select record.handle;
        };
        if (remove(owner, handle) != a.notification_ok) return false;
    }
}
