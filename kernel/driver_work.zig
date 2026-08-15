const interrupts = @import("../arch/x86_64/interrupts.zig");
const irq_router = @import("irq_router.zig");
const sched_task = @import("../sched/task.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("timer.zig");

pub const VERSION: u32 = 1;
pub const QUEUE_CAPACITY: u32 = 16;

pub const WORK_STATE_FREE: u32 = 0;
pub const WORK_STATE_QUEUED: u32 = 1;
pub const WORK_STATE_RUNNING: u32 = 2;
pub const WORK_STATE_COMPLETED: u32 = 3;
pub const WORK_STATE_CANCELLED: u32 = 4;

pub const WORK_FLAG_NONE: u32 = 0;
pub const WORK_FLAG_FROM_IRQ: u32 = 1 << 0;

pub const RESULT_CANCELLED: i32 = -7;

pub const WorkHandler = *const fn (usize) callconv(.c) i32;

pub const CompletionStatus = extern struct {
    version: u32 = VERSION,
    size: u32 = @sizeOf(CompletionStatus),
    handle: u32 = 0,
    state: u32 = WORK_STATE_FREE,
    owner: u32 = 0,
    flags: u32 = 0,
    result: i32 = 0,
    reserved0: u32 = 0,
    submitted_tick: u64 = 0,
    started_tick: u64 = 0,
    completed_tick: u64 = 0,
    queue_ticks: u64 = 0,
    run_ticks: u64 = 0,
};

pub const Summary = extern struct {
    version: u32 = VERSION,
    size: u32 = @sizeOf(Summary),
    initialized: u32 = 0,
    worker_started: u32 = 0,
    worker_task_id: u32 = 0,
    queue_capacity: u32 = QUEUE_CAPACITY,
    queue_depth: u32 = 0,
    queue_high_water: u32 = 0,
    active_workers: u32 = 0,
    reserved0: u32 = 0,
    submitted: u64 = 0,
    submitted_from_irq: u64 = 0,
    submitted_from_task: u64 = 0,
    started: u64 = 0,
    completed: u64 = 0,
    failed: u64 = 0,
    cancelled: u64 = 0,
    dropped: u64 = 0,
    waits: u64 = 0,
    wait_timeouts: u64 = 0,
    wait_denied_irq: u64 = 0,
    wait_total_ticks: u64 = 0,
    wait_max_ticks: u64 = 0,
    wait_last_ticks: u64 = 0,
    queue_total_ticks: u64 = 0,
    queue_max_ticks: u64 = 0,
    queue_last_ticks: u64 = 0,
    run_total_ticks: u64 = 0,
    run_max_ticks: u64 = 0,
    run_last_ticks: u64 = 0,
    releases: u64 = 0,
    invalid_handles: u64 = 0,
    cleanup_cancelled: u64 = 0,
    sleep_waits: u64 = 0,
    sleep_denied_irq: u64 = 0,
    sleep_total_ticks: u64 = 0,
};

const State = enum(u8) {
    free,
    queued,
    running,
    completed,
    cancelled,
};

const WorkItem = struct {
    state: State = .free,
    owner: u32 = 0,
    handle: u32 = 0,
    generation: u32 = 0,
    flags: u32 = 0,
    handler: ?WorkHandler = null,
    context: usize = 0,
    result: i32 = 0,
    submitted_tick: u64 = 0,
    started_tick: u64 = 0,
    completed_tick: u64 = 0,
    completion: sync.Completion = sync.Completion.init(),
};

var initialized = false;
var worker_started = false;
var worker_task_id: u32 = 0;
var next_generation: u32 = 1;
var items: [QUEUE_CAPACITY]WorkItem = .{WorkItem{}} ** QUEUE_CAPACITY;
var queue_event = sync.EventV2.initMode(false, .auto_reset);
var summary_state: Summary = .{};

pub fn init() bool {
    if (initialized and worker_started) return true;
    initialized = true;
    summary_state.initialized = 1;
    summary_state.queue_capacity = QUEUE_CAPACITY;
    const worker = sched_task.createKernelThread("r4d-work", workerMain) orelse {
        summary_state.worker_started = 0;
        return false;
    };
    worker_task_id = worker.id;
    worker_started = true;
    summary_state.worker_started = 1;
    summary_state.worker_task_id = worker_task_id;
    return true;
}

pub fn submit(owner: u32, handler: WorkHandler, context: usize, flags: u32, out_handle: *u32) i32 {
    out_handle.* = 0;
    if (!initialized or !worker_started) {
        recordDrop();
        return -1;
    }

    const from_irq = irq_router.inDispatch();
    const critical = enterCritical();
    const slot = findFreeSlotLocked() orelse {
        summary_state.dropped +%= 1;
        leaveCritical(critical);
        return -2;
    };
    const generation = allocateGenerationLocked();
    const handle = makeHandle(slot, generation);
    const now = timer.tickCount();
    items[slot] = .{
        .state = .queued,
        .owner = owner,
        .handle = handle,
        .generation = generation,
        .flags = flags | if (from_irq) WORK_FLAG_FROM_IRQ else WORK_FLAG_NONE,
        .handler = handler,
        .context = context,
        .submitted_tick = now,
        .completion = sync.Completion.init(),
    };
    out_handle.* = handle;
    summary_state.submitted +%= 1;
    if (from_irq) {
        summary_state.submitted_from_irq +%= 1;
    } else {
        summary_state.submitted_from_task +%= 1;
    }
    refreshDepthLocked();
    leaveCritical(critical);

    queue_event.signal();
    return 0;
}

pub fn cancel(handle: u32) i32 {
    const critical = enterCritical();
    const slot = validateHandleLocked(handle) orelse {
        summary_state.invalid_handles +%= 1;
        leaveCritical(critical);
        return -1;
    };
    const item = &items[slot];
    if (item.state != .queued) {
        leaveCritical(critical);
        return -2;
    }
    item.state = .cancelled;
    item.result = RESULT_CANCELLED;
    item.completed_tick = timer.tickCount();
    summary_state.cancelled +%= 1;
    refreshDepthLocked();
    item.completion.completeAll();
    leaveCritical(critical);
    return 0;
}

pub fn completionWait(handle: u32, timeout_ticks: u64, out_result: *i32) i32 {
    out_result.* = 0;
    if (irq_router.inDispatch()) {
        summary_state.wait_denied_irq +%= 1;
        return -6;
    }

    var completion: *sync.Completion = undefined;
    const start = timer.tickCount();
    {
        const critical = enterCritical();
        const slot = validateHandleLocked(handle) orelse {
            summary_state.invalid_handles +%= 1;
            leaveCritical(critical);
            return -1;
        };
        const item = &items[slot];
        switch (item.state) {
            .completed => {
                out_result.* = item.result;
                recordWaitLocked(elapsedTicks(start, timer.tickCount()), false);
                leaveCritical(critical);
                return 0;
            },
            .cancelled => {
                out_result.* = item.result;
                recordWaitLocked(elapsedTicks(start, timer.tickCount()), false);
                leaveCritical(critical);
                return RESULT_CANCELLED;
            },
            .queued, .running => {
                completion = &item.completion;
            },
            .free => {
                leaveCritical(critical);
                return -2;
            },
        }
        leaveCritical(critical);
    }

    const wait_result = completion.wait(timeout_ticks);
    const waited = elapsedTicks(start, timer.tickCount());
    const timed_out = wait_result == .timeout;
    {
        const critical = enterCritical();
        recordWaitLocked(waited, timed_out);
        if (timed_out) {
            leaveCritical(critical);
            return 1;
        }
        const slot = validateHandleLocked(handle) orelse {
            summary_state.invalid_handles +%= 1;
            leaveCritical(critical);
            return -1;
        };
        const item = &items[slot];
        out_result.* = item.result;
        const state = item.state;
        leaveCritical(critical);
        return if (state == .cancelled) RESULT_CANCELLED else 0;
    }
}

pub fn completionStatus(handle: u32, out: *CompletionStatus) i32 {
    const critical = enterCritical();
    const slot = validateHandleLocked(handle) orelse {
        out.* = .{};
        summary_state.invalid_handles +%= 1;
        leaveCritical(critical);
        return -1;
    };
    out.* = statusFromItem(items[slot]);
    leaveCritical(critical);
    return 0;
}

pub fn completionRelease(handle: u32) i32 {
    const critical = enterCritical();
    const slot = validateHandleLocked(handle) orelse {
        summary_state.invalid_handles +%= 1;
        leaveCritical(critical);
        return -1;
    };
    switch (items[slot].state) {
        .completed, .cancelled => {
            _ = items[slot].completion.close();
            items[slot] = .{};
            summary_state.releases +%= 1;
            refreshDepthLocked();
            leaveCritical(critical);
            return 0;
        },
        .queued, .running => {
            leaveCritical(critical);
            return -2;
        },
        .free => {
            leaveCritical(critical);
            return -3;
        },
    }
}

pub fn cleanupOwner(owner: u32) u32 {
    if (owner == 0) return 0;
    const critical = enterCritical();
    var removed: u32 = 0;
    var i: usize = 0;
    while (i < items.len) : (i += 1) {
        const item = &items[i];
        if (item.owner != owner) continue;
        switch (item.state) {
            .queued, .completed, .cancelled => {
                _ = item.completion.close();
                item.* = .{};
                removed += 1;
            },
            .running => {
                item.flags |= WORK_FLAG_FROM_IRQ;
            },
            .free => {},
        }
    }
    summary_state.cleanup_cancelled +%= removed;
    refreshDepthLocked();
    leaveCritical(critical);
    return removed;
}

pub fn noteSleepWait(ticks: u64) void {
    summary_state.sleep_waits +%= 1;
    summary_state.sleep_total_ticks +%= ticks;
}

pub fn noteSleepDeniedFromIrq() void {
    summary_state.sleep_denied_irq +%= 1;
}

pub fn summary() Summary {
    const critical = enterCritical();
    var out = summary_state;
    out.initialized = if (initialized) 1 else 0;
    out.worker_started = if (worker_started) 1 else 0;
    out.worker_task_id = worker_task_id;
    out.queue_capacity = QUEUE_CAPACITY;
    out.queue_depth = countStateLocked(.queued);
    out.active_workers = countStateLocked(.running);
    leaveCritical(critical);
    return out;
}

fn workerMain() callconv(.c) void {
    while (true) {
        if (takeNext()) |slot| {
            runSlot(slot);
            continue;
        }
        _ = queue_event.waitResult(scheduler.WAIT_FOREVER);
    }
}

fn takeNext() ?usize {
    const critical = enterCritical();
    defer leaveCritical(critical);

    var best: ?usize = null;
    var best_tick: u64 = 0;
    var i: usize = 0;
    while (i < items.len) : (i += 1) {
        const item = items[i];
        if (item.state != .queued) continue;
        if (best == null or item.submitted_tick < best_tick) {
            best = i;
            best_tick = item.submitted_tick;
        }
    }
    const slot = best orelse return null;
    items[slot].state = .running;
    items[slot].started_tick = timer.tickCount();
    summary_state.started +%= 1;
    refreshDepthLocked();
    return slot;
}

fn runSlot(slot: usize) void {
    const handler = items[slot].handler orelse {
        finishSlot(slot, -1);
        return;
    };
    const result = handler(items[slot].context);
    finishSlot(slot, result);
}

fn finishSlot(slot: usize, result: i32) void {
    const completion = &items[slot].completion;
    const critical = enterCritical();
    const now = timer.tickCount();
    const queue_ticks = elapsedTicks(items[slot].submitted_tick, items[slot].started_tick);
    const run_ticks = elapsedTicks(items[slot].started_tick, now);
    items[slot].state = .completed;
    items[slot].result = result;
    items[slot].completed_tick = now;
    summary_state.completed +%= 1;
    if (result != 0) summary_state.failed +%= 1;
    summary_state.queue_total_ticks +%= queue_ticks;
    summary_state.queue_last_ticks = queue_ticks;
    if (queue_ticks > summary_state.queue_max_ticks) summary_state.queue_max_ticks = queue_ticks;
    summary_state.run_total_ticks +%= run_ticks;
    summary_state.run_last_ticks = run_ticks;
    if (run_ticks > summary_state.run_max_ticks) summary_state.run_max_ticks = run_ticks;
    refreshDepthLocked();
    completion.completeAll();
    leaveCritical(critical);
}

fn findFreeSlotLocked() ?usize {
    var i: usize = 0;
    while (i < items.len) : (i += 1) {
        if (items[i].state == .free) return i;
    }
    return null;
}

fn validateHandleLocked(handle: u32) ?usize {
    const slot = slotFromHandle(handle) orelse return null;
    if (items[slot].state == .free or items[slot].handle != handle) return null;
    return slot;
}

fn slotFromHandle(handle: u32) ?usize {
    const slot_code = handle & 0xFF;
    if (slot_code == 0 or slot_code > QUEUE_CAPACITY) return null;
    return @intCast(slot_code - 1);
}

fn makeHandle(slot: usize, generation: u32) u32 {
    return ((generation & 0x00FF_FFFF) << 8) | @as(u32, @intCast(slot + 1));
}

fn allocateGenerationLocked() u32 {
    const out = next_generation;
    next_generation = (next_generation + 1) & 0x00FF_FFFF;
    if (next_generation == 0) next_generation = 1;
    return out;
}

fn refreshDepthLocked() void {
    const depth = countStateLocked(.queued);
    summary_state.queue_depth = depth;
    summary_state.active_workers = countStateLocked(.running);
    if (depth > summary_state.queue_high_water) summary_state.queue_high_water = depth;
}

fn countStateLocked(state: State) u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < items.len) : (i += 1) {
        if (items[i].state == state) count += 1;
    }
    return count;
}

fn recordDrop() void {
    summary_state.dropped +%= 1;
}

fn recordWaitLocked(waited: u64, timed_out: bool) void {
    summary_state.waits +%= 1;
    if (timed_out) summary_state.wait_timeouts +%= 1;
    summary_state.wait_total_ticks +%= waited;
    summary_state.wait_last_ticks = waited;
    if (waited > summary_state.wait_max_ticks) summary_state.wait_max_ticks = waited;
}

fn statusFromItem(item: WorkItem) CompletionStatus {
    const queue_ticks = switch (item.state) {
        .running, .completed, .cancelled => elapsedTicks(item.submitted_tick, item.started_tick),
        else => 0,
    };
    const run_ticks = switch (item.state) {
        .completed, .cancelled => elapsedTicks(item.started_tick, item.completed_tick),
        .running => elapsedTicks(item.started_tick, timer.tickCount()),
        else => 0,
    };
    return .{
        .handle = item.handle,
        .state = stateCode(item.state),
        .owner = item.owner,
        .flags = item.flags,
        .result = item.result,
        .submitted_tick = item.submitted_tick,
        .started_tick = item.started_tick,
        .completed_tick = item.completed_tick,
        .queue_ticks = queue_ticks,
        .run_ticks = run_ticks,
    };
}

fn stateCode(state: State) u32 {
    return switch (state) {
        .free => WORK_STATE_FREE,
        .queued => WORK_STATE_QUEUED,
        .running => WORK_STATE_RUNNING,
        .completed => WORK_STATE_COMPLETED,
        .cancelled => WORK_STATE_CANCELLED,
    };
}

fn enterCritical() bool {
    if (irq_router.inDispatch()) return false;
    interrupts.disable();
    return true;
}

fn leaveCritical(enable_interrupts: bool) void {
    if (enable_interrupts) interrupts.enable();
}

fn elapsedTicks(start: u64, end: u64) u64 {
    if (end < start) return 0;
    return end - start;
}
