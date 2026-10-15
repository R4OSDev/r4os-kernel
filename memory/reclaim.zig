const page_cache = @import("../fs/page_cache.zig");
const ntfs_fs = @import("../fs/ntfs/ntfs.zig");
const phys = @import("phys.zig");
const timer = @import("../kernel/timer.zig");
const task_context = @import("../sched/task_context.zig");

pub const Reason = enum(u32) {
    diagnostic = 1,
    vm_commit = 2,
    vm_fault = 3,
    loader_commit = 4,
};

pub const Result = struct {
    reason: Reason = .diagnostic,
    requested_frames: u32 = 0,
    returned_frames: u32 = 0,
    returned_bytes: u64 = 0,
    fs_returned_frames: u32 = 0,
    fs_returned_bytes: u64 = 0,
    task_stack_returned_frames: u32 = 0,
    task_stack_returned_bytes: u64 = 0,
    vm_returned_frames: u32 = 0,
    vm_returned_bytes: u64 = 0,
    vm_page_outs: u64 = 0,
    vm_failures: u64 = 0,
    dirty_drains: u64 = 0,
    failed_drains: u64 = 0,
    before_free_frames: u64 = 0,
    after_free_frames: u64 = 0,
    elapsed_ticks: u64 = 0,
};

pub const SourceResult = struct {
    returned_frames: u32 = 0,
    returned_bytes: u64 = 0,
    page_outs: u64 = 0,
    failures: u64 = 0,
};

pub const VmReclaimer = *const fn (Reason, u32, ?u64) SourceResult;
pub const TaskStackReclaimer = *const fn (u32) u32;

// A cached kernel stack owns 64 KB of committed memory; its guard page is
// reserved but deliberately uncommitted. The task layer reports released
// stack objects, while the global pressure contract accounts in frames.
const task_stack_bytes: u64 = 64 * 1024;
const task_stack_frames: u32 = @intCast(task_stack_bytes / 4096);

pub const Summary = struct {
    enabled: bool = true,
    attempts: u64 = 0,
    successes: u64 = 0,
    failures: u64 = 0,
    requested_frames: u64 = 0,
    returned_frames: u64 = 0,
    returned_bytes: u64 = 0,
    fs_returned_frames: u64 = 0,
    fs_returned_bytes: u64 = 0,
    task_stack_returned_frames: u64 = 0,
    task_stack_returned_bytes: u64 = 0,
    vm_returned_frames: u64 = 0,
    vm_returned_bytes: u64 = 0,
    vm_page_outs: u64 = 0,
    vm_failures: u64 = 0,
    dirty_drains: u64 = 0,
    failed_drains: u64 = 0,
    total_ticks: u64 = 0,
    max_ticks: u64 = 0,
    last_ticks: u64 = 0,
    last_reason: u32 = 0,
    last_requested_frames: u32 = 0,
    last_returned_frames: u32 = 0,
    last_before_free_frames: u64 = 0,
    last_after_free_frames: u64 = 0,
};


// Native pressure owner; these C projections retain no second state.
const RawResult = extern struct {
    reason: u32,
    requested_frames: u32,
    returned_frames: u32,
    returned_bytes: u64,
    fs_returned_frames: u32,
    fs_returned_bytes: u64,
    task_stack_returned_frames: u32,
    task_stack_returned_bytes: u64,
    vm_returned_frames: u32,
    vm_returned_bytes: u64,
    vm_page_outs: u64,
    vm_failures: u64,
    dirty_drains: u64,
    failed_drains: u64,
    before_free_frames: u64,
    after_free_frames: u64,
    elapsed_ticks: u64,
};
fn rawResult(value: Result) RawResult {
    return .{
        .reason = @intFromEnum(value.reason),
        .requested_frames = value.requested_frames,
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .fs_returned_frames = value.fs_returned_frames,
        .fs_returned_bytes = value.fs_returned_bytes,
        .task_stack_returned_frames = value.task_stack_returned_frames,
        .task_stack_returned_bytes = value.task_stack_returned_bytes,
        .vm_returned_frames = value.vm_returned_frames,
        .vm_returned_bytes = value.vm_returned_bytes,
        .vm_page_outs = value.vm_page_outs,
        .vm_failures = value.vm_failures,
        .dirty_drains = value.dirty_drains,
        .failed_drains = value.failed_drains,
        .before_free_frames = value.before_free_frames,
        .after_free_frames = value.after_free_frames,
        .elapsed_ticks = value.elapsed_ticks,
    };
}
fn fromRawResult(value: RawResult) Result {
    return .{
        .reason = @enumFromInt(value.reason),
        .requested_frames = value.requested_frames,
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .fs_returned_frames = value.fs_returned_frames,
        .fs_returned_bytes = value.fs_returned_bytes,
        .task_stack_returned_frames = value.task_stack_returned_frames,
        .task_stack_returned_bytes = value.task_stack_returned_bytes,
        .vm_returned_frames = value.vm_returned_frames,
        .vm_returned_bytes = value.vm_returned_bytes,
        .vm_page_outs = value.vm_page_outs,
        .vm_failures = value.vm_failures,
        .dirty_drains = value.dirty_drains,
        .failed_drains = value.failed_drains,
        .before_free_frames = value.before_free_frames,
        .after_free_frames = value.after_free_frames,
        .elapsed_ticks = value.elapsed_ticks,
    };
}
const RawSourceResult = extern struct {
    returned_frames: u32,
    returned_bytes: u64,
    page_outs: u64,
    failures: u64,
};
fn rawSourceResult(value: SourceResult) RawSourceResult {
    return .{
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .page_outs = value.page_outs,
        .failures = value.failures,
    };
}
fn fromRawSourceResult(value: RawSourceResult) SourceResult {
    return .{
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .page_outs = value.page_outs,
        .failures = value.failures,
    };
}
const RawSummary = extern struct {
    enabled: bool,
    attempts: u64,
    successes: u64,
    failures: u64,
    requested_frames: u64,
    returned_frames: u64,
    returned_bytes: u64,
    fs_returned_frames: u64,
    fs_returned_bytes: u64,
    task_stack_returned_frames: u64,
    task_stack_returned_bytes: u64,
    vm_returned_frames: u64,
    vm_returned_bytes: u64,
    vm_page_outs: u64,
    vm_failures: u64,
    dirty_drains: u64,
    failed_drains: u64,
    total_ticks: u64,
    max_ticks: u64,
    last_ticks: u64,
    last_reason: u32,
    last_requested_frames: u32,
    last_returned_frames: u32,
    last_before_free_frames: u64,
    last_after_free_frames: u64,
};
fn rawSummary(value: Summary) RawSummary {
    return .{
        .enabled = value.enabled,
        .attempts = value.attempts,
        .successes = value.successes,
        .failures = value.failures,
        .requested_frames = value.requested_frames,
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .fs_returned_frames = value.fs_returned_frames,
        .fs_returned_bytes = value.fs_returned_bytes,
        .task_stack_returned_frames = value.task_stack_returned_frames,
        .task_stack_returned_bytes = value.task_stack_returned_bytes,
        .vm_returned_frames = value.vm_returned_frames,
        .vm_returned_bytes = value.vm_returned_bytes,
        .vm_page_outs = value.vm_page_outs,
        .vm_failures = value.vm_failures,
        .dirty_drains = value.dirty_drains,
        .failed_drains = value.failed_drains,
        .total_ticks = value.total_ticks,
        .max_ticks = value.max_ticks,
        .last_ticks = value.last_ticks,
        .last_reason = value.last_reason,
        .last_requested_frames = value.last_requested_frames,
        .last_returned_frames = value.last_returned_frames,
        .last_before_free_frames = value.last_before_free_frames,
        .last_after_free_frames = value.last_after_free_frames,
    };
}
fn fromRawSummary(value: RawSummary) Summary {
    return .{
        .enabled = value.enabled,
        .attempts = value.attempts,
        .successes = value.successes,
        .failures = value.failures,
        .requested_frames = value.requested_frames,
        .returned_frames = value.returned_frames,
        .returned_bytes = value.returned_bytes,
        .fs_returned_frames = value.fs_returned_frames,
        .fs_returned_bytes = value.fs_returned_bytes,
        .task_stack_returned_frames = value.task_stack_returned_frames,
        .task_stack_returned_bytes = value.task_stack_returned_bytes,
        .vm_returned_frames = value.vm_returned_frames,
        .vm_returned_bytes = value.vm_returned_bytes,
        .vm_page_outs = value.vm_page_outs,
        .vm_failures = value.vm_failures,
        .dirty_drains = value.dirty_drains,
        .failed_drains = value.failed_drains,
        .total_ticks = value.total_ticks,
        .max_ticks = value.max_ticks,
        .last_ticks = value.last_ticks,
        .last_reason = value.last_reason,
        .last_requested_frames = value.last_requested_frames,
        .last_returned_frames = value.last_returned_frames,
        .last_before_free_frames = value.last_before_free_frames,
        .last_after_free_frames = value.last_after_free_frames,
    };
}
extern fn r4os_rust_reclaim_register_vm(usize) callconv(.c) void;
extern fn r4os_rust_reclaim_register_stack(usize) callconv(.c) void;
extern fn r4os_rust_reclaim_frames(u32, u32, u64, bool, *RawResult) callconv(.c) void;
extern fn r4os_rust_reclaim_summary(*RawSummary) callconv(.c) void;
pub fn registerVmReclaimer(callback: VmReclaimer) void { r4os_rust_reclaim_register_vm(@intFromPtr(callback)); }
pub fn registerTaskStackReclaimer(callback: TaskStackReclaimer) void { r4os_rust_reclaim_register_stack(@intFromPtr(callback)); }
pub fn reclaimFrames(reason: Reason, frames: u32) Result {
    var result: RawResult = undefined;
    r4os_rust_reclaim_frames(@intFromEnum(reason), frames, 0, false, &result);
    return fromRawResult(result);
}
pub fn reclaimDiagnosticFrames(frames: u32, owner: u64) Result {
    var result: RawResult = undefined;
    r4os_rust_reclaim_frames(@intFromEnum(Reason.diagnostic), frames, owner, true, &result);
    return fromRawResult(result);
}
pub fn summary() Summary {
    var result: RawSummary = undefined;
    r4os_rust_reclaim_summary(&result);
    return fromRawSummary(result);
}
export fn r4os_zig_reclaim_ntfs(frames: u32) callconv(.c) void {
    _ = ntfs_fs.reclaimMetadataCacheEntries(frames);
}
export fn r4os_zig_reclaim_vm_invoke(callback: usize, reason: u32, frames: u32, owner: u64, owner_present: bool, output: *RawSourceResult) callconv(.c) void {
    const invoke: VmReclaimer = @ptrFromInt(callback);
    output.* = rawSourceResult(invoke(@enumFromInt(reason), frames, if (owner_present) owner else null));
}
export fn r4os_zig_reclaim_stack_invoke(callback: usize, stacks: u32) callconv(.c) u32 {
    const invoke: TaskStackReclaimer = @ptrFromInt(callback);
    return invoke(stacks);
}

comptime {
    if (@sizeOf(RawResult) != 128 or @alignOf(RawResult) != 8) @compileError("Native reclaim Result geometry");
    if (@offsetOf(RawResult, "reason") != 0) @compileError("Native reclaim Result.reason offset");
    if (@offsetOf(RawResult, "requested_frames") != 4) @compileError("Native reclaim Result.requested_frames offset");
    if (@offsetOf(RawResult, "returned_frames") != 8) @compileError("Native reclaim Result.returned_frames offset");
    if (@offsetOf(RawResult, "returned_bytes") != 16) @compileError("Native reclaim Result.returned_bytes offset");
    if (@offsetOf(RawResult, "fs_returned_frames") != 24) @compileError("Native reclaim Result.fs_returned_frames offset");
    if (@offsetOf(RawResult, "fs_returned_bytes") != 32) @compileError("Native reclaim Result.fs_returned_bytes offset");
    if (@offsetOf(RawResult, "task_stack_returned_frames") != 40) @compileError("Native reclaim Result.task_stack_returned_frames offset");
    if (@offsetOf(RawResult, "task_stack_returned_bytes") != 48) @compileError("Native reclaim Result.task_stack_returned_bytes offset");
    if (@offsetOf(RawResult, "vm_returned_frames") != 56) @compileError("Native reclaim Result.vm_returned_frames offset");
    if (@offsetOf(RawResult, "vm_returned_bytes") != 64) @compileError("Native reclaim Result.vm_returned_bytes offset");
    if (@offsetOf(RawResult, "vm_page_outs") != 72) @compileError("Native reclaim Result.vm_page_outs offset");
    if (@offsetOf(RawResult, "vm_failures") != 80) @compileError("Native reclaim Result.vm_failures offset");
    if (@offsetOf(RawResult, "dirty_drains") != 88) @compileError("Native reclaim Result.dirty_drains offset");
    if (@offsetOf(RawResult, "failed_drains") != 96) @compileError("Native reclaim Result.failed_drains offset");
    if (@offsetOf(RawResult, "before_free_frames") != 104) @compileError("Native reclaim Result.before_free_frames offset");
    if (@offsetOf(RawResult, "after_free_frames") != 112) @compileError("Native reclaim Result.after_free_frames offset");
    if (@offsetOf(RawResult, "elapsed_ticks") != 120) @compileError("Native reclaim Result.elapsed_ticks offset");
    if (@sizeOf(RawSourceResult) != 32 or @alignOf(RawSourceResult) != 8) @compileError("Native reclaim SourceResult geometry");
    if (@offsetOf(RawSourceResult, "returned_frames") != 0) @compileError("Native reclaim SourceResult.returned_frames offset");
    if (@offsetOf(RawSourceResult, "returned_bytes") != 8) @compileError("Native reclaim SourceResult.returned_bytes offset");
    if (@offsetOf(RawSourceResult, "page_outs") != 16) @compileError("Native reclaim SourceResult.page_outs offset");
    if (@offsetOf(RawSourceResult, "failures") != 24) @compileError("Native reclaim SourceResult.failures offset");
    if (@sizeOf(RawSummary) != 192 or @alignOf(RawSummary) != 8) @compileError("Native reclaim Summary geometry");
    if (@offsetOf(RawSummary, "enabled") != 0) @compileError("Native reclaim Summary.enabled offset");
    if (@offsetOf(RawSummary, "attempts") != 8) @compileError("Native reclaim Summary.attempts offset");
    if (@offsetOf(RawSummary, "successes") != 16) @compileError("Native reclaim Summary.successes offset");
    if (@offsetOf(RawSummary, "failures") != 24) @compileError("Native reclaim Summary.failures offset");
    if (@offsetOf(RawSummary, "requested_frames") != 32) @compileError("Native reclaim Summary.requested_frames offset");
    if (@offsetOf(RawSummary, "returned_frames") != 40) @compileError("Native reclaim Summary.returned_frames offset");
    if (@offsetOf(RawSummary, "returned_bytes") != 48) @compileError("Native reclaim Summary.returned_bytes offset");
    if (@offsetOf(RawSummary, "fs_returned_frames") != 56) @compileError("Native reclaim Summary.fs_returned_frames offset");
    if (@offsetOf(RawSummary, "fs_returned_bytes") != 64) @compileError("Native reclaim Summary.fs_returned_bytes offset");
    if (@offsetOf(RawSummary, "task_stack_returned_frames") != 72) @compileError("Native reclaim Summary.task_stack_returned_frames offset");
    if (@offsetOf(RawSummary, "task_stack_returned_bytes") != 80) @compileError("Native reclaim Summary.task_stack_returned_bytes offset");
    if (@offsetOf(RawSummary, "vm_returned_frames") != 88) @compileError("Native reclaim Summary.vm_returned_frames offset");
    if (@offsetOf(RawSummary, "vm_returned_bytes") != 96) @compileError("Native reclaim Summary.vm_returned_bytes offset");
    if (@offsetOf(RawSummary, "vm_page_outs") != 104) @compileError("Native reclaim Summary.vm_page_outs offset");
    if (@offsetOf(RawSummary, "vm_failures") != 112) @compileError("Native reclaim Summary.vm_failures offset");
    if (@offsetOf(RawSummary, "dirty_drains") != 120) @compileError("Native reclaim Summary.dirty_drains offset");
    if (@offsetOf(RawSummary, "failed_drains") != 128) @compileError("Native reclaim Summary.failed_drains offset");
    if (@offsetOf(RawSummary, "total_ticks") != 136) @compileError("Native reclaim Summary.total_ticks offset");
    if (@offsetOf(RawSummary, "max_ticks") != 144) @compileError("Native reclaim Summary.max_ticks offset");
    if (@offsetOf(RawSummary, "last_ticks") != 152) @compileError("Native reclaim Summary.last_ticks offset");
    if (@offsetOf(RawSummary, "last_reason") != 160) @compileError("Native reclaim Summary.last_reason offset");
    if (@offsetOf(RawSummary, "last_requested_frames") != 164) @compileError("Native reclaim Summary.last_requested_frames offset");
    if (@offsetOf(RawSummary, "last_returned_frames") != 168) @compileError("Native reclaim Summary.last_returned_frames offset");
    if (@offsetOf(RawSummary, "last_before_free_frames") != 176) @compileError("Native reclaim Summary.last_before_free_frames offset");
    if (@offsetOf(RawSummary, "last_after_free_frames") != 184) @compileError("Native reclaim Summary.last_after_free_frames offset");
}
