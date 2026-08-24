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

pub const VmReclaimer = *const fn (Reason, u32) SourceResult;
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

var state: Summary = .{};
var vm_reclaimer: ?VmReclaimer = null;
var task_stack_reclaimer: ?TaskStackReclaimer = null;
var in_reclaim = false;

pub fn registerVmReclaimer(reclaimer: VmReclaimer) void {
    vm_reclaimer = reclaimer;
}

pub fn registerTaskStackReclaimer(reclaimer: TaskStackReclaimer) void {
    task_stack_reclaimer = reclaimer;
}

pub fn reclaimFrames(reason: Reason, requested_frames_raw: u32) Result {
    const requested_frames = if (requested_frames_raw == 0) 1 else requested_frames_raw;
    const before = phys.stats().free_frames;
    const start = timer.tickCount();
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) {
        const result = Result{
            .reason = reason,
            .requested_frames = requested_frames,
            .before_free_frames = before,
            .after_free_frames = before,
            .elapsed_ticks = elapsedTicks(start),
        };
        record(result);
        return result;
    }
    defer _ = task_context.leaveUnwind(unwind);
    if (in_reclaim) {
        const elapsed = elapsedTicks(start);
        const result = Result{
            .reason = reason,
            .requested_frames = requested_frames,
            .before_free_frames = before,
            .after_free_frames = before,
            .elapsed_ticks = elapsed,
        };
        record(result);
        return result;
    }

    in_reclaim = true;
    defer in_reclaim = false;
    // Decoded NTFS metadata is fixed-capacity and owns no sector payloads,
    // but its reusable entries are still first-class pressure candidates.
    // The NTFS owner bounds this pass independently of the frame request.
    _ = ntfs_fs.reclaimMetadataCacheEntries(requested_frames);
    var task_stack_result = SourceResult{};
    if (task_stack_reclaimer) |reclaimer| {
        const requested_stacks = (requested_frames +| (task_stack_frames - 1)) / task_stack_frames;
        const returned_stacks = reclaimer(requested_stacks);
        task_stack_result.returned_frames = returned_stacks *| task_stack_frames;
        task_stack_result.returned_bytes = @as(u64, returned_stacks) *| task_stack_bytes;
    }

    var cache_result = page_cache.ReclaimResult{};
    if (task_stack_result.returned_frames < requested_frames) {
        cache_result = page_cache.reclaimPayloadFrames(requested_frames - task_stack_result.returned_frames, true);
    }
    var vm_result = SourceResult{};
    const non_vm_frames = task_stack_result.returned_frames +| cache_result.returned_frames;
    if (non_vm_frames < requested_frames) {
        if (vm_reclaimer) |reclaimer| {
            vm_result = reclaimer(reason, requested_frames - non_vm_frames);
        }
    }
    const after = phys.stats().free_frames;
    const elapsed = elapsedTicks(start);
    const returned_frames = task_stack_result.returned_frames +| cache_result.returned_frames +| vm_result.returned_frames;

    const result = Result{
        .reason = reason,
        .requested_frames = requested_frames,
        .returned_frames = returned_frames,
        .returned_bytes = task_stack_result.returned_bytes +% cache_result.returned_bytes +% vm_result.returned_bytes,
        .fs_returned_frames = cache_result.returned_frames,
        .fs_returned_bytes = cache_result.returned_bytes,
        .task_stack_returned_frames = task_stack_result.returned_frames,
        .task_stack_returned_bytes = task_stack_result.returned_bytes,
        .vm_returned_frames = vm_result.returned_frames,
        .vm_returned_bytes = vm_result.returned_bytes,
        .vm_page_outs = vm_result.page_outs,
        .vm_failures = vm_result.failures,
        .dirty_drains = cache_result.dirty_drains,
        .failed_drains = cache_result.failed_drains +% vm_result.failures,
        .before_free_frames = before,
        .after_free_frames = after,
        .elapsed_ticks = elapsed,
    };
    record(result);
    return result;
}

pub fn summary() Summary {
    return state;
}

fn record(result: Result) void {
    state.enabled = true;
    state.attempts +%= 1;
    state.requested_frames +%= result.requested_frames;
    state.returned_frames +%= result.returned_frames;
    state.returned_bytes +%= result.returned_bytes;
    state.fs_returned_frames +%= result.fs_returned_frames;
    state.fs_returned_bytes +%= result.fs_returned_bytes;
    state.task_stack_returned_frames +%= result.task_stack_returned_frames;
    state.task_stack_returned_bytes +%= result.task_stack_returned_bytes;
    state.vm_returned_frames +%= result.vm_returned_frames;
    state.vm_returned_bytes +%= result.vm_returned_bytes;
    state.vm_page_outs +%= result.vm_page_outs;
    state.vm_failures +%= result.vm_failures;
    state.dirty_drains +%= result.dirty_drains;
    state.failed_drains +%= result.failed_drains;
    state.total_ticks +%= result.elapsed_ticks;
    state.last_ticks = result.elapsed_ticks;
    if (result.elapsed_ticks > state.max_ticks) state.max_ticks = result.elapsed_ticks;
    state.last_reason = @intFromEnum(result.reason);
    state.last_requested_frames = result.requested_frames;
    state.last_returned_frames = result.returned_frames;
    state.last_before_free_frames = result.before_free_frames;
    state.last_after_free_frames = result.after_free_frames;
    if (result.returned_frames > 0) {
        state.successes +%= 1;
    } else {
        state.failures +%= 1;
    }
}

fn elapsedTicks(start_tick: u64) u64 {
    const now = timer.tickCount();
    return if (now >= start_tick) now - start_tick else 0;
}
