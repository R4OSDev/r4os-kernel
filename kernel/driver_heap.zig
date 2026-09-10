const std = @import("std");
const a = @import("r4os_kernel_contract");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const task_context = @import("../sched/task_context.zig");
const heap = @import("../memory/heap.zig");
const ownership = @import("driver_memory_owner.zig");
const owner_capacity = @import("../driver/registry.zig").MAX_DRIVERS;
var state: ownership.State(owner_capacity) = .{};

// The scheduler runtime owner protects only resident metadata and unwind
// enrollment. Never hold it across heap allocation/free, payload access or
// callbacks. Driver-work callbacks do not acquire the R4D lifecycle guard.
pub fn bind(owner: u32, epoch: u64) bool {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.activate(owner, epoch) catch return false;
    return true;
}

pub fn beginClose(owner: u32) void {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.close(owner);
}

pub fn query(owner: u32) i32 {
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    const snapshot = state.snapshot(owner) catch |err| return code(err);
    return if (snapshot.closing) a.driver_heap_error_closed else a.driver_heap_ok;
}

pub fn allocate(owner: u32, bytes: u64, alignment: u32, output: *a.DriverHeapAllocation) i32 {
    if (!valid(a.DriverHeapAllocation, output)) return a.driver_heap_error_invalid;
    output.* = .{};
    const layout = ownership.Layout.init(bytes, alignment) catch |err| return code(err);
    const unwind = enterOperation() orelse return a.driver_heap_error_busy;
    defer leaveOperation(unwind);
    var create = reserve: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :reserve state.begin(owner, layout) catch |err| return code(err);
    };
    const backing = heap.alloc(layout.heap_bytes, layout.alignment) orelse {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        state.abort(&create) catch unreachable;
        return a.driver_heap_error_memory;
    };
    const published = publish: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :publish state.adopt(&create, backing) catch unreachable;
    };
    if (!published.visible) return a.driver_heap_error_closed;
    output.* = .{
        .handle = published.allocation.handle,
        .cpu_address = published.allocation.address,
        .byte_length = published.allocation.bytes,
        .alignment = published.allocation.alignment,
    };
    return a.driver_heap_ok;
}

pub fn release(owner: u32, handle: u64) i32 {
    const unwind = enterOperation() orelse return a.driver_heap_error_busy;
    defer leaveOperation(unwind);
    var ticket = detach: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :detach state.beginRelease(owner, handle) catch |err| return code(err);
    };
    return releaseBacking(&ticket);
}

pub fn stats(owner: u32, output: *a.DriverHeapStats) i32 {
    if (!valid(a.DriverHeapStats, output)) return a.driver_heap_error_invalid;
    output.* = .{};
    const snapshot = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        break :capture state.snapshot(owner) catch |err| return code(err);
    };
    output.* = .{
        .owner_epoch = snapshot.epoch,
        .allocations = snapshot.allocations,
        .bytes = snapshot.bytes,
        .backing_bytes = snapshot.backing_bytes,
        .peak_bytes = snapshot.peak_bytes,
        .private_allocations = snapshot.private_allocations,
        .allocation_calls = snapshot.allocation_calls,
        .allocation_failures = snapshot.allocation_failures,
        .releases = snapshot.releases,
        .release_failures = snapshot.release_failures,
        .pending_creates = snapshot.pending_creates,
        .pending_releases = snapshot.pending_releases,
        .closing = @intFromBool(snapshot.closing),
    };
    return a.driver_heap_ok;
}

pub const Cleanup = struct { quiesced: bool = true, released: u64 = 0, bytes: u64 = 0 };

// Called only after the R4D shutdown and all generic callback/work/DMA
// teardown have succeeded. A failed backing release retains the closed owner
// and module; never turn it into successful unload or retry forever here.
pub fn cleanup(owner: u32) Cleanup {
    var result: Cleanup = .{};
    const unwind = enterOperation() orelse return .{ .quiesced = false };
    defer leaveOperation(unwind);
    const count = capture: {
        const flags = interrupts.saveAndDisableRuntime();
        defer interrupts.restore(flags);
        const snapshot = state.snapshot(owner) catch |err| return .{ .quiesced = err == error.Stale };
        if (!snapshot.closing or snapshot.pending_creates != 0 or snapshot.pending_releases != 0) return .{ .quiesced = false };
        break :capture snapshot.allocations;
    };
    var index: u64 = 0;
    while (index < count) : (index += 1) {
        var ticket = detach: {
            const flags = interrupts.saveAndDisableRuntime();
            defer interrupts.restore(flags);
            break :detach state.nextCleanup(owner) catch {
                result.quiesced = false;
                return result;
            } orelse break;
        };
        const bytes = ticket.bytes;
        if (releaseBacking(&ticket) != a.driver_heap_ok) {
            result.quiesced = false;
            return result;
        }
        result.released += 1;
        result.bytes += bytes;
    }
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    state.finish(owner) catch {
        result.quiesced = false;
    };
    return result;
}

fn releaseBacking(ticket: *ownership.Release) i32 {
    const released = heap.free(ticket.backing());
    const flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(flags);
    // On success, ticket.record is already invalid; finishRelease only uses
    // the copied identity/accounting. Heap's failure path leaves it intact.
    state.finishRelease(ticket, released == .ok) catch unreachable;
    return if (released == .ok) a.driver_heap_ok else a.driver_heap_error_release;
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
        error.Invalid => a.driver_heap_error_invalid,
        error.Owner => a.driver_heap_error_owner,
        error.Stale => a.driver_heap_error_stale,
        error.Closed => a.driver_heap_error_closed,
        error.Busy => a.driver_heap_error_busy,
        error.Exhausted => a.driver_heap_error_exhausted,
        error.Overflow => a.driver_heap_error_overflow,
    };
}
