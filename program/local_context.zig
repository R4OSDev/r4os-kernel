// Process-local publication for shared runtime libraries. Userland owns the
// pointed-to data and its allocator; the kernel owns only this association.
const std = @import("std");
const a = @import("r4os_kernel_contract");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const io = @import("../arch/x86_64/io.zig");
const scheduler = @import("../sched/scheduler.zig");
const task_context = @import("../sched/task_context.zig");
const heap = @import("../memory/heap.zig");
const Tree = std.Treap(u64, std.math.order);
const Record = struct {
    node: Tree.Node = undefined,
    retired_next: ?*Record = null,
    key: u64,
    value: u64,
};
pub const Owner = struct {
    tree: Tree = .{},
    retired: ?*Record = null,
    closing: bool = false,
};

fn find(owner: *Owner, key: u64) ?*Record {
    const node = owner.tree.getEntryFor(key).node orelse return null;
    return @fieldParentPtr("node", node);
}
fn operation() ?task_context.UnwindToken {
    if (!interrupts.wereEnabled(io.readRflags()) or interrupts.inRuntimeCriticalSection()) return null;
    const current = scheduler.current() orelse return null;
    if (current.preempt_disable_depth != 0 or current.held_lock_count != 0) return null;
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const token = task_context.enterUnwind();
    return if (token.admitted()) token else null;
}
fn finish(token: task_context.UnwindToken) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    std.debug.assert(task_context.leaveUnwind(token));
}
fn free(record: *Record) bool {
    const bytes: [*]u8 = @ptrCast(record);
    return heap.free(bytes[0..@sizeOf(Record)]) == .ok;
}
fn retain(owner: *Owner, record: *Record) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    record.retired_next = owner.retired;
    owner.retired = record;
}

pub fn get(owner: *Owner, key: u64, output: *u64) i32 {
    if (key == 0 or @intFromPtr(output) == 0) return a.program_local_error_invalid;
    const value = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (owner.closing) return a.program_local_error_closed;
        break :capture if (find(owner, key)) |record| record.value else 0;
    };
    output.* = value;
    return a.program_local_ok;
}

pub fn publish(owner: *Owner, key: u64, candidate_value: u64, output: *u64) i32 {
    if (key == 0 or candidate_value == 0 or @intFromPtr(output) == 0) return a.program_local_error_invalid;
    const token = operation() orelse return a.program_local_error_context;
    defer finish(token);
    const previous = lookup: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        if (owner.closing) return a.program_local_error_closed;
        break :lookup if (find(owner, key)) |record| record.value else 0;
    };
    if (previous != 0) {
        output.* = previous;
        return a.program_local_existing;
    }
    const bytes = heap.alloc(@sizeOf(Record), @alignOf(Record)) orelse return a.program_local_error_memory;
    const candidate: *Record = @ptrCast(@alignCast(bytes.ptr));
    candidate.* = .{ .key = key, .value = candidate_value };
    const winner = insert: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        // The unwind token prevents exact process retirement until publication
        // or disposal is complete. No caller-owned memory is touched here.
        std.debug.assert(!owner.closing);
        var entry = owner.tree.getEntryFor(key);
        if (entry.node) |node| {
            const record: *Record = @fieldParentPtr("node", node);
            break :insert record;
        }
        entry.set(&candidate.node);
        break :insert candidate;
    };
    const value = winner.value;
    if (winner != candidate and !free(candidate)) retain(owner, candidate);
    output.* = value;
    return if (winner == candidate) a.program_local_ok else a.program_local_existing;
}

pub fn cleanup(owner: *Owner) bool {
    // Userland data remains in the process VM until its later VM teardown.
    // No user destructor/callback runs from the kernel or the process reaper.
    {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        owner.closing = true;
        if (owner.tree.root == null and owner.retired == null) return true;
    }
    const token = operation() orelse return false;
    defer finish(token);
    while (true) {
        const record = detach: {
            const flags = interrupts.saveAndDisableRuntime();
            defer interrupts.restore(flags);
            if (owner.retired) |retired| {
                owner.retired = retired.retired_next;
                retired.retired_next = null;
                break :detach retired;
            }
            const node = owner.tree.root orelse return true;
            const value: *Record = @fieldParentPtr("node", node);
            var entry = owner.tree.getEntryForExisting(node);
            entry.set(null);
            break :detach value;
        };
        if (!free(record)) {
            retain(owner, record);
            return false;
        }
    }
}
