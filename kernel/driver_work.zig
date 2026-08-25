const interrupts = @import("../arch/x86_64/interrupts.zig");
const monotonic = @import("../platform/monotonic.zig");
const irq_router = @import("irq_router.zig");
const sched_task = @import("../sched/task.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("timer.zig");
const work_queue = @import("driver_work_queue.zig");

pub const VERSION: u32 = 1;
pub const PERFORMANCE_VERSION: u32 = 1;
pub const QUEUE_CAPACITY: u32 = @intCast(work_queue.capacity);
pub const OWNER_CAPACITY: u32 = 16;
pub const WORKER_COUNT: u32 = 1;
pub const LONG_CALLBACK_THRESHOLD_NS: u64 = 1_000_000;
pub const CLEANUP_JOIN_TIMEOUT_TICKS: u64 = 1000;

pub const WORK_STATE_FREE: u32 = 0;
pub const WORK_STATE_QUEUED: u32 = 1;
pub const WORK_STATE_RUNNING: u32 = 2;
pub const WORK_STATE_COMPLETED: u32 = 3;
pub const WORK_STATE_CANCELLED: u32 = 4;

pub const WORK_FLAG_NONE: u32 = 0;
pub const WORK_FLAG_FROM_IRQ: u32 = 1 << 0;

pub const RESULT_CANCELLED: i32 = -7;

pub const WorkHandler = *const fn (usize) callconv(.c) i32;

// R4D ABI v1. Keep this fixed-layout prefix compatible.
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

// R4D ABI v1. The richer append-only performance view is exposed through
// R4DEV and deliberately does not grow this driver-facing structure.
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

// All fields are scalar and append-only. The same layout is used for the
// aggregate owner=0 view and each exact registry owner 1..16.
pub const PerformanceMetrics = extern struct {
    submitted: u64 = 0,
    submitted_actual_irq: u64 = 0,
    submitted_actual_task: u64 = 0,
    submitted_irq_class: u64 = 0,
    submitted_task_class: u64 = 0,
    started: u64 = 0,
    started_irq_class: u64 = 0,
    started_task_class: u64 = 0,
    completed: u64 = 0,
    completed_irq_class: u64 = 0,
    completed_task_class: u64 = 0,
    failed: u64 = 0,
    cancelled: u64 = 0,
    dropped: u64 = 0,
    full_rejections: u64 = 0,
    retained_full_rejections: u64 = 0,
    waits: u64 = 0,
    wait_timeouts: u64 = 0,
    wait_denied_irq: u64 = 0,
    wait_failed: u64 = 0,
    wait_total_ns: u64 = 0,
    wait_max_ns: u64 = 0,
    wait_last_ns: u64 = 0,
    wake_publications: u64 = 0,
    wake_waiters: u64 = 0,
    wake_misses: u64 = 0,
    releases: u64 = 0,
    release_busy: u64 = 0,
    release_wakes: u64 = 0,
    invalid_handles: u64 = 0,
    stale_handles: u64 = 0,
    publication_pending_releases: u64 = 0,
    waiter_blocked_releases: u64 = 0,
    claimed_releases: u64 = 0,
    queue_total_ns: u64 = 0,
    queue_max_ns: u64 = 0,
    queue_last_ns: u64 = 0,
    run_total_ns: u64 = 0,
    run_max_ns: u64 = 0,
    run_last_ns: u64 = 0,
    e2e_total_ns: u64 = 0,
    e2e_max_ns: u64 = 0,
    e2e_last_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    completion_age_current_ns: u64 = 0,
    completion_age_max_ns: u64 = 0,
    selection_irq: u64 = 0,
    selection_task: u64 = 0,
    selection_irq_preferred: u64 = 0,
    selection_task_fairness: u64 = 0,
    selection_empty: u64 = 0,
    scan_passes: u64 = 0,
    scan_slots: u64 = 0,
    critical_sections: u64 = 0,
    critical_from_irq: u64 = 0,
    critical_timing_samples: u64 = 0,
    critical_timing_unavailable: u64 = 0,
    critical_total_ns: u64 = 0,
    critical_max_ns: u64 = 0,
    critical_last_ns: u64 = 0,
    cleanup_calls: u64 = 0,
    cleanup_quiesced: u64 = 0,
    cleanup_failed_context: u64 = 0,
    cleanup_queued_cancelled: u64 = 0,
    cleanup_waits: u64 = 0,
    cleanup_wait_timeouts: u64 = 0,
    cleanup_wait_failures: u64 = 0,
    cleanup_wait_total_ns: u64 = 0,
    cleanup_wait_max_ns: u64 = 0,
    cleanup_released: u64 = 0,
    cleanup_late_finishes: u64 = 0,
    cleanup_scan_passes: u64 = 0,
    cleanup_scan_slots: u64 = 0,
    long_callbacks: u64 = 0,
    waiter_enrollments: u64 = 0,
    waiter_wake_returns: u64 = 0,
    waiter_cancel_returns: u64 = 0,
    sleep_waits: u64 = 0,
    sleep_denied_irq: u64 = 0,
    sleep_total_ticks: u64 = 0,
};

pub const Performance = extern struct {
    version: u32 = PERFORMANCE_VERSION,
    size: u32 = @sizeOf(Performance),
    selected_owner: u32 = 0,
    owner_present: u32 = 0,
    initialized: u32 = 0,
    worker_started: u32 = 0,
    worker_task_id: u32 = 0,
    worker_count: u32 = WORKER_COUNT,
    queue_capacity: u32 = QUEUE_CAPACITY,
    free_slots: u32 = QUEUE_CAPACITY,
    used_slots: u32 = 0,
    queued_slots: u32 = 0,
    running_slots: u32 = 0,
    completed_slots: u32 = 0,
    cancelled_slots: u32 = 0,
    irq_queued_slots: u32 = 0,
    task_queued_slots: u32 = 0,
    queue_high_water: u32 = 0,
    used_high_water: u32 = 0,
    retained_high_water: u32 = 0,
    irq_burst_limit: u32 = work_queue.irq_burst_limit,
    current_irq_burst: u32 = 0,
    waiters_current: u32 = 0,
    waiters_max: u32 = 0,
    last_submitted_owner: u32 = 0,
    last_started_owner: u32 = 0,
    last_completed_owner: u32 = 0,
    last_cleanup_owner: u32 = 0,
    owner_used_slots: u32 = 0,
    owner_queued_slots: u32 = 0,
    owner_running_slots: u32 = 0,
    owner_completed_slots: u32 = 0,
    owner_cancelled_slots: u32 = 0,
    owner_irq_queued_slots: u32 = 0,
    owner_task_queued_slots: u32 = 0,
    owner_used_high_water: u32 = 0,
    owner_retained_high_water: u32 = 0,
    monotonic_clock_flags: u32 = 0,
    owner_waiters_current: u32 = 0,
    owner_waiters_max: u32 = 0,
    long_callback_threshold_ns: u64 = LONG_CALLBACK_THRESHOLD_NS,
    metrics: PerformanceMetrics = .{},
};

pub const CleanupResult = struct {
    removed: u32 = 0,
    quiesced: bool = true,
};

const WorkItem = struct {
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
    submitted_at: monotonic.Stamp = .{},
    started_at: monotonic.Stamp = .{},
    completed_at: monotonic.Stamp = .{},
    completion_published: bool = false,
    release_claimed: bool = false,
    cleanup_waiting: bool = false,
    waiter_count: u32 = 0,
    owner_final_prev: u8 = work_queue.no_slot,
    owner_final_next: u8 = work_queue.no_slot,
    completion: sync.Completion = sync.Completion.init(),
};

const OwnerCurrent = work_queue.OwnerBookkeeping;

const CriticalGuard = struct {
    irq_flags: u64,
    started_at: monotonic.Stamp,
    from_irq: bool,
};

const HandleLookup = union(enum) {
    valid: usize,
    invalid,
    stale,
};

const OwnerSearch = struct {
    slot: ?usize,
    scanned: u32,
};

var initialized = false;
var worker_started = false;
var worker_task_id: u32 = 0;
var items: [work_queue.capacity]WorkItem = .{WorkItem{}} ** work_queue.capacity;
var book = work_queue.Bookkeeping{};
var queue_event = sync.EventV2.initMode(false, .auto_reset);
var summary_state: Summary = .{};
var global_metrics: PerformanceMetrics = .{};
var owner_metrics: [OWNER_CAPACITY]PerformanceMetrics = .{PerformanceMetrics{}} ** OWNER_CAPACITY;
var owner_current: [OWNER_CAPACITY]OwnerCurrent = .{OwnerCurrent{}} ** OWNER_CAPACITY;
var owner_final_head: [OWNER_CAPACITY]u8 = .{work_queue.no_slot} ** OWNER_CAPACITY;
var owner_final_tail: [OWNER_CAPACITY]u8 = .{work_queue.no_slot} ** OWNER_CAPACITY;
var global_waiters: u32 = 0;
var global_waiters_max: u32 = 0;
var last_submitted_owner: u32 = 0;
var last_started_owner: u32 = 0;
var last_completed_owner: u32 = 0;
var last_cleanup_owner: u32 = 0;
var running_callback_owner: u32 = 0;

pub fn init() bool {
    if (initialized and worker_started) return true;
    initialized = true;
    summary_state.initialized = 1;
    summary_state.queue_capacity = QUEUE_CAPACITY;
    const worker = sched_task.createKernelThreadWithRole("r4d-work", workerMain, .short_completion) orelse {
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
    const actual_irq = irq_router.inDispatch();
    if (!initialized or !worker_started) {
        const critical = enterCritical();
        addMetric(owner, "dropped", 1);
        summary_state.dropped +%= 1;
        leaveCritical(critical, owner);
        return -1;
    }

    const source: work_queue.SourceClass = if (actual_irq or (flags & WORK_FLAG_FROM_IRQ) != 0) .irq else .task;
    const critical = enterCritical();
    const slot = book.reserveAndEnqueue(source) orelse {
        addMetric(owner, "dropped", 1);
        addMetric(owner, "full_rejections", 1);
        if (book.retainedCount() != 0) addMetric(owner, "retained_full_rejections", 1);
        summary_state.dropped +%= 1;
        syncLegacyCurrentLocked();
        leaveCritical(critical, owner);
        return -2;
    };

    const generation = work_queue.nextGeneration(items[slot].generation);
    const handle = work_queue.makeHandle(slot, generation);
    items[slot] = .{
        .owner = owner,
        .handle = handle,
        .generation = generation,
        .flags = flags | if (actual_irq) WORK_FLAG_FROM_IRQ else WORK_FLAG_NONE,
        .handler = handler,
        .context = context,
        .submitted_tick = timer.tickCount(),
        .submitted_at = monotonic.capture(),
        .completion = sync.Completion.init(),
    };
    noteOwnerReserveLocked(owner, source);
    out_handle.* = handle;
    last_submitted_owner = owner;
    addMetric(owner, "submitted", 1);
    if (actual_irq) {
        addMetric(owner, "submitted_actual_irq", 1);
    } else {
        addMetric(owner, "submitted_actual_task", 1);
    }
    switch (source) {
        .irq => addMetric(owner, "submitted_irq_class", 1),
        .task => addMetric(owner, "submitted_task_class", 1),
    }
    summary_state.submitted +%= 1;
    if (actual_irq) {
        summary_state.submitted_from_irq +%= 1;
    } else {
        summary_state.submitted_from_task +%= 1;
    }
    syncLegacyCurrentLocked();
    leaveCritical(critical, owner);

    queue_event.signal();
    return 0;
}

pub fn cancel(handle: u32) i32 {
    var slot: usize = 0;
    var owner: u32 = 0;
    {
        const critical = enterCritical();
        slot = validateHandleLocked(handle) orelse {
            leaveCritical(critical, 0);
            return -1;
        };
        owner = items[slot].owner;
        if (book.state(slot) != .queued) {
            leaveCritical(critical, owner);
            return -2;
        }
        const source = book.sourceClass(slot);
        if (!book.cancelQueued(slot)) {
            leaveCritical(critical, owner);
            return -2;
        }
        items[slot].result = RESULT_CANCELLED;
        items[slot].completed_tick = timer.tickCount();
        items[slot].completed_at = monotonic.capture();
        items[slot].completion_published = false;
        noteOwnerCancelledLocked(owner, source);
        appendOwnerFinalLocked(owner, slot);
        addMetric(owner, "cancelled", 1);
        summary_state.cancelled +%= 1;
        syncLegacyCurrentLocked();
        leaveCritical(critical, owner);
    }
    publishCompletion(slot, handle);
    return 0;
}

pub fn completionWait(handle: u32, timeout_ticks: u64, out_result: *i32) i32 {
    out_result.* = 0;
    const wait_started_tick = timer.tickCount();
    const wait_started_at = monotonic.capture();

    if (irq_router.inDispatch()) {
        const critical = enterCritical();
        const slot = validateHandleLocked(handle) orelse {
            leaveCritical(critical, 0);
            return -1;
        };
        const owner = items[slot].owner;
        addMetric(owner, "wait_denied_irq", 1);
        summary_state.wait_denied_irq +%= 1;
        leaveCritical(critical, owner);
        return -6;
    }

    var completion: *sync.Completion = undefined;
    var owner: u32 = 0;
    {
        const critical = enterCritical();
        const slot = validateHandleLocked(handle) orelse {
            leaveCritical(critical, 0);
            return -1;
        };
        owner = items[slot].owner;
        switch (book.state(slot)) {
            .completed => {
                out_result.* = items[slot].result;
                recordWaitLocked(owner, wait_started_tick, wait_started_at, false);
                leaveCritical(critical, owner);
                return 0;
            },
            .cancelled => {
                out_result.* = items[slot].result;
                recordWaitLocked(owner, wait_started_tick, wait_started_at, false);
                leaveCritical(critical, owner);
                return RESULT_CANCELLED;
            },
            .queued, .running => {
                items[slot].waiter_count +|= 1;
                noteWaiterAddedLocked(owner);
                addMetric(owner, "waiter_enrollments", 1);
                completion = &items[slot].completion;
            },
            .free => {
                leaveCritical(critical, owner);
                return -2;
            },
        }
        leaveCritical(critical, owner);
    }

    const wait_result = completion.wait(timeout_ticks);
    const timed_out = wait_result == .timeout;
    const critical = enterCritical();
    const slot = validateHandleLocked(handle) orelse {
        // A live waiter prevents release; reaching this branch is therefore
        // an invariant failure, but fail closed if the handle was corrupted.
        leaveCritical(critical, 0);
        return -1;
    };
    owner = items[slot].owner;
    if (items[slot].waiter_count != 0) {
        items[slot].waiter_count -= 1;
        noteWaiterRemovedLocked(owner);
    }
    recordWaitLocked(owner, wait_started_tick, wait_started_at, timed_out);
    if (timed_out) {
        leaveCritical(critical, owner);
        return 1;
    }
    if (wait_result != .signaled) {
        addMetric(owner, "wait_failed", 1);
        addMetric(owner, "waiter_cancel_returns", 1);
        leaveCritical(critical, owner);
        return -5;
    }
    addMetric(owner, "waiter_wake_returns", 1);
    out_result.* = items[slot].result;
    const state = book.state(slot);
    leaveCritical(critical, owner);
    return if (state == .cancelled) RESULT_CANCELLED else 0;
}

pub fn completionStatus(handle: u32, out: *CompletionStatus) i32 {
    const critical = enterCritical();
    const slot = validateHandleLocked(handle) orelse {
        out.* = .{};
        leaveCritical(critical, 0);
        return -1;
    };
    const owner = items[slot].owner;
    out.* = statusFromItemLocked(slot);
    leaveCritical(critical, owner);
    return 0;
}

pub fn completionRelease(handle: u32) i32 {
    var slot: usize = 0;
    var owner: u32 = 0;
    var final_state: work_queue.State = .free;
    var completed_at: monotonic.Stamp = .{};
    {
        const critical = enterCritical();
        slot = validateHandleLocked(handle) orelse {
            leaveCritical(critical, 0);
            return -1;
        };
        owner = items[slot].owner;
        final_state = book.state(slot);
        switch (work_queue.releaseDecision(
            final_state,
            items[slot].completion_published,
            items[slot].waiter_count,
            items[slot].release_claimed,
        )) {
            .claim => {
                items[slot].release_claimed = true;
                addMetric(owner, "claimed_releases", 1);
                completed_at = items[slot].completed_at;
            },
            .publication_pending => {
                addMetric(owner, "release_busy", 1);
                addMetric(owner, "publication_pending_releases", 1);
                leaveCritical(critical, owner);
                return -2;
            },
            .waiters_present => {
                addMetric(owner, "release_busy", 1);
                addMetric(owner, "waiter_blocked_releases", 1);
                leaveCritical(critical, owner);
                return -2;
            },
            .busy, .already_claimed => {
                addMetric(owner, "release_busy", 1);
                leaveCritical(critical, owner);
                return -2;
            },
            .invalid => {
                leaveCritical(critical, owner);
                return -3;
            },
        }
        leaveCritical(critical, owner);
    }

    // Closing a completion may wake/cancel scheduler waiters. It must never
    // run inside the driver-work IRQ/preemption critical section.
    const release_wakes = items[slot].completion.close();

    const critical = enterCritical();
    const checked_slot = validateHandleLocked(handle) orelse {
        leaveCritical(critical, 0);
        return -1;
    };
    if (checked_slot != slot or !items[slot].release_claimed) {
        leaveCritical(critical, owner);
        return -1;
    }
    if (release_wakes != 0) addMetric(owner, "release_wakes", release_wakes);
    noteCompletionAgeLocked(owner, completed_at);
    if (!book.releaseFinal(slot)) {
        items[slot].release_claimed = false;
        leaveCritical(critical, owner);
        return -2;
    }
    removeOwnerFinalLocked(owner, slot);
    noteOwnerReleaseLocked(owner, final_state);
    const generation = items[slot].generation;
    items[slot] = .{ .generation = generation };
    addMetric(owner, "releases", 1);
    summary_state.releases +%= 1;
    syncLegacyCurrentLocked();
    leaveCritical(critical, owner);
    return 0;
}

pub fn cleanupOwner(owner: u32) CleanupResult {
    if (owner == 0) return .{};
    {
        const critical = enterCritical();
        addMetric(owner, "cleanup_calls", 1);
        last_cleanup_owner = owner;
        if (ownerIndex(owner) == null or irq_router.inDispatch() or scheduler.current() == null or currentIsWorker()) {
            addMetric(owner, "cleanup_failed_context", 1);
            leaveCritical(critical, owner);
            return .{ .quiesced = false };
        }
        leaveCritical(critical, owner);
    }

    const cleanup_started_tick = timer.tickCount();
    var removed: u32 = 0;
    while (true) {
        var slot: usize = 0;
        var handle: u32 = 0;
        var state: work_queue.State = .free;
        var completion: *sync.Completion = undefined;
        var wait_for_running = false;

        {
            const critical = enterCritical();
            const found = findOwnerSlotLocked(owner);
            addMetric(owner, "scan_passes", 1);
            addMetric(owner, "scan_slots", found.scanned);
            addMetric(owner, "cleanup_scan_passes", 1);
            addMetric(owner, "cleanup_scan_slots", found.scanned);
            slot = found.slot orelse {
                addMetric(owner, "cleanup_quiesced", 1);
                summary_state.cleanup_cancelled +%= removed;
                leaveCritical(critical, owner);
                return .{ .removed = removed, .quiesced = true };
            };
            handle = items[slot].handle;
            state = book.state(slot);

            if (cleanupExpired(cleanup_started_tick)) {
                addMetric(owner, "cleanup_wait_timeouts", 1);
                leaveCritical(critical, owner);
                return .{ .removed = removed, .quiesced = false };
            }

            if (state == .running) {
                items[slot].cleanup_waiting = true;
                items[slot].waiter_count +|= 1;
                noteWaiterAddedLocked(owner);
                addMetric(owner, "waiter_enrollments", 1);
                addMetric(owner, "cleanup_waits", 1);
                completion = &items[slot].completion;
                wait_for_running = true;
            }
            leaveCritical(critical, owner);
        }

        switch (state) {
            .queued => {
                if (cancel(handle) == 0) {
                    const critical = enterCritical();
                    addMetric(owner, "cleanup_queued_cancelled", 1);
                    leaveCritical(critical, owner);
                }
            },
            .running => {
                if (!wait_for_running) continue;
                const remaining = cleanupRemainingTicks(cleanup_started_tick);
                const wait_started_at = monotonic.capture();
                const wait_result = completion.wait(remaining);
                const wait_ended_at = monotonic.capture();

                const critical = enterCritical();
                switch (lookupHandleLocked(handle)) {
                    .valid => |current_slot| {
                        if (items[current_slot].waiter_count != 0) {
                            items[current_slot].waiter_count -= 1;
                            noteWaiterRemovedLocked(owner);
                        }
                    },
                    .invalid, .stale => {},
                }
                recordDurationLocked(owner, "cleanup_wait_total_ns", "cleanup_wait_max_ns", null, wait_started_at, wait_ended_at);
                if (wait_result == .timeout) {
                    addMetric(owner, "cleanup_wait_timeouts", 1);
                    leaveCritical(critical, owner);
                    return .{ .removed = removed, .quiesced = false };
                }
                if (wait_result != .signaled) {
                    addMetric(owner, "cleanup_wait_failures", 1);
                    leaveCritical(critical, owner);
                    return .{ .removed = removed, .quiesced = false };
                }
                addMetric(owner, "waiter_wake_returns", 1);
                leaveCritical(critical, owner);
            },
            .completed, .cancelled => {
                const release_result = completionRelease(handle);
                if (release_result == 0) {
                    removed +|= 1;
                    const critical = enterCritical();
                    addMetric(owner, "cleanup_released", 1);
                    leaveCritical(critical, owner);
                } else {
                    scheduler.yield();
                }
            },
            .free => {},
        }
    }
}

pub fn noteSleepWait(ticks: u64) void {
    const critical = enterCritical();
    addMetric(0, "sleep_waits", 1);
    addMetric(0, "sleep_total_ticks", ticks);
    summary_state.sleep_waits +%= 1;
    summary_state.sleep_total_ticks +%= ticks;
    leaveCritical(critical, 0);
}

pub fn noteSleepDeniedFromIrq() void {
    const critical = enterCritical();
    addMetric(0, "sleep_denied_irq", 1);
    summary_state.sleep_denied_irq +%= 1;
    leaveCritical(critical, 0);
}

pub fn summary() Summary {
    const critical = enterCritical();
    syncLegacyCurrentLocked();
    var out = summary_state;
    out.initialized = if (initialized) 1 else 0;
    out.worker_started = if (worker_started) 1 else 0;
    out.worker_task_id = worker_task_id;
    out.queue_capacity = QUEUE_CAPACITY;
    leaveCritical(critical, 0);
    return out;
}

pub fn performance(owner: u32) Performance {
    const clock = monotonic.snapshot();
    const now = monotonic.capture();
    const critical = enterCritical();
    var zero_metrics = PerformanceMetrics{};
    const selected_metrics: *const PerformanceMetrics = if (owner == 0)
        &global_metrics
    else
        ownerMetricsConst(owner) orelse &zero_metrics;
    var current = OwnerCurrent{};
    if (ownerCurrentConst(owner)) |selected_current| current = selected_current.*;

    var out = Performance{
        .selected_owner = owner,
        .owner_present = if (owner == 0 or ownerHasHistoryLocked(owner, selected_metrics, current)) 1 else 0,
        .initialized = if (initialized) 1 else 0,
        .worker_started = if (worker_started) 1 else 0,
        .worker_task_id = worker_task_id,
        .free_slots = book.free_count,
        .used_slots = book.usedCount(),
        .queued_slots = book.queued_count,
        .running_slots = book.running_count,
        .completed_slots = book.completed_count,
        .cancelled_slots = book.cancelled_count,
        .irq_queued_slots = book.irq_queued_count,
        .task_queued_slots = book.task_queued_count,
        .queue_high_water = book.queue_high_water,
        .used_high_water = book.used_high_water,
        .retained_high_water = book.retained_high_water,
        .current_irq_burst = book.current_irq_burst,
        .waiters_current = global_waiters,
        .waiters_max = global_waiters_max,
        .last_submitted_owner = last_submitted_owner,
        .last_started_owner = last_started_owner,
        .last_completed_owner = last_completed_owner,
        .last_cleanup_owner = last_cleanup_owner,
        .owner_used_slots = current.used,
        .owner_queued_slots = current.queued,
        .owner_running_slots = current.running,
        .owner_completed_slots = current.completed,
        .owner_cancelled_slots = current.cancelled,
        .owner_irq_queued_slots = current.irq_queued,
        .owner_task_queued_slots = current.task_queued,
        .owner_used_high_water = current.used_high_water,
        .owner_retained_high_water = current.retained_high_water,
        .monotonic_clock_flags = clock.flags,
        .owner_waiters_current = current.waiters,
        .owner_waiters_max = current.waiters_max,
        .metrics = selected_metrics.*,
    };
    const final_slot = oldestFinalForOwnerLocked(owner);
    if (final_slot) |index| {
        if (monotonic.elapsedNanoseconds(items[index].completed_at, now)) |age| {
            out.metrics.completion_age_current_ns = age;
            if (age > out.metrics.completion_age_max_ns) out.metrics.completion_age_max_ns = age;
        } else {
            out.metrics.timing_unavailable +%= 1;
        }
    }
    leaveCritical(critical, owner);
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
    const selection = book.takeNext() orelse {
        addMetric(0, "selection_empty", 1);
        syncLegacyCurrentLocked();
        leaveCritical(critical, 0);
        return null;
    };
    const slot = selection.slot;
    const owner = items[slot].owner;
    items[slot].started_tick = timer.tickCount();
    items[slot].started_at = monotonic.capture();
    noteOwnerStartedLocked(owner, selection.source);
    last_started_owner = owner;
    addMetric(owner, "started", 1);
    switch (selection.source) {
        .irq => {
            addMetric(owner, "started_irq_class", 1);
            addMetric(owner, "selection_irq", 1);
        },
        .task => {
            addMetric(owner, "started_task_class", 1);
            addMetric(owner, "selection_task", 1);
        },
    }
    if (selection.irq_preferred) addMetric(owner, "selection_irq_preferred", 1);
    if (selection.task_fairness) addMetric(owner, "selection_task_fairness", 1);
    recordDurationLocked(owner, "queue_total_ns", "queue_max_ns", "queue_last_ns", items[slot].submitted_at, items[slot].started_at);

    const queue_ticks = elapsedTicks(items[slot].submitted_tick, items[slot].started_tick);
    summary_state.started +%= 1;
    summary_state.queue_total_ticks +%= queue_ticks;
    summary_state.queue_last_ticks = queue_ticks;
    if (queue_ticks > summary_state.queue_max_ticks) summary_state.queue_max_ticks = queue_ticks;
    syncLegacyCurrentLocked();
    leaveCritical(critical, owner);
    return slot;
}

fn runSlot(slot: usize) void {
    const handler = items[slot].handler orelse {
        finishSlot(slot, -1);
        return;
    };
    running_callback_owner = items[slot].owner;
    defer running_callback_owner = 0;
    const result = handler(items[slot].context);
    finishSlot(slot, result);
}

pub fn currentOwner() u32 {
    if (!currentIsWorker()) return 0;
    return running_callback_owner;
}

fn finishSlot(slot: usize, result: i32) void {
    const completed_tick = timer.tickCount();
    const completed_at = monotonic.capture();
    var handle: u32 = 0;
    var owner: u32 = 0;
    {
        const critical = enterCritical();
        if (slot >= items.len or book.state(slot) != .running) {
            leaveCritical(critical, 0);
            return;
        }
        owner = items[slot].owner;
        handle = items[slot].handle;
        const source = book.sourceClass(slot);
        if (!book.completeRunning(slot)) {
            leaveCritical(critical, owner);
            return;
        }
        items[slot].result = result;
        items[slot].completed_tick = completed_tick;
        items[slot].completed_at = completed_at;
        items[slot].completion_published = false;
        noteOwnerCompletedLocked(owner);
        appendOwnerFinalLocked(owner, slot);
        last_completed_owner = owner;
        addMetric(owner, "completed", 1);
        switch (source) {
            .irq => addMetric(owner, "completed_irq_class", 1),
            .task => addMetric(owner, "completed_task_class", 1),
        }
        if (result != 0) addMetric(owner, "failed", 1);
        if (items[slot].cleanup_waiting) addMetric(owner, "cleanup_late_finishes", 1);
        recordDurationLocked(owner, "run_total_ns", "run_max_ns", "run_last_ns", items[slot].started_at, completed_at);
        recordDurationLocked(owner, "e2e_total_ns", "e2e_max_ns", "e2e_last_ns", items[slot].submitted_at, completed_at);
        if (monotonic.elapsedNanoseconds(items[slot].started_at, completed_at)) |run_ns| {
            if (run_ns >= LONG_CALLBACK_THRESHOLD_NS) addMetric(owner, "long_callbacks", 1);
        }

        const run_ticks = elapsedTicks(items[slot].started_tick, completed_tick);
        summary_state.completed +%= 1;
        if (result != 0) summary_state.failed +%= 1;
        summary_state.run_total_ticks +%= run_ticks;
        summary_state.run_last_ticks = run_ticks;
        if (run_ticks > summary_state.run_max_ticks) summary_state.run_max_ticks = run_ticks;
        syncLegacyCurrentLocked();
        leaveCritical(critical, owner);
    }
    publishCompletion(slot, handle);
}

fn publishCompletion(slot: usize, handle: u32) void {
    const wake_count = items[slot].completion.completeAllCount();
    const critical = enterCritical();
    switch (lookupHandleLocked(handle)) {
        .valid => |checked_slot| {
            if (checked_slot == slot and isFinal(book.state(slot)) and !items[slot].completion_published) {
                const owner = items[slot].owner;
                items[slot].completion_published = true;
                addMetric(owner, "wake_publications", 1);
                if (wake_count == 0) {
                    addMetric(owner, "wake_misses", 1);
                } else {
                    addMetric(owner, "wake_waiters", wake_count);
                }
                leaveCritical(critical, owner);
                _ = scheduler.safeReschedulePoint();
                return;
            }
        },
        .invalid, .stale => {},
    }
    leaveCritical(critical, 0);
    _ = scheduler.safeReschedulePoint();
}

fn validateHandleLocked(handle: u32) ?usize {
    return switch (lookupHandleLocked(handle)) {
        .valid => |slot| slot,
        .invalid => {
            addMetric(0, "invalid_handles", 1);
            summary_state.invalid_handles +%= 1;
            return null;
        },
        .stale => {
            addMetric(0, "stale_handles", 1);
            summary_state.invalid_handles +%= 1;
            return null;
        },
    };
}

fn lookupHandleLocked(handle: u32) HandleLookup {
    const slot = work_queue.slotFromHandle(handle) orelse return .invalid;
    const handle_generation = (handle >> 8) & 0x00FF_FFFF;
    if (items[slot].generation != handle_generation) return .stale;
    if (book.state(slot) == .free or items[slot].handle != handle) return .invalid;
    return .{ .valid = slot };
}

fn recordWaitLocked(owner: u32, started_tick: u64, started_at: monotonic.Stamp, timed_out: bool) void {
    const waited_ticks = elapsedTicks(started_tick, timer.tickCount());
    addMetric(owner, "waits", 1);
    if (timed_out) addMetric(owner, "wait_timeouts", 1);
    recordDurationLocked(owner, "wait_total_ns", "wait_max_ns", "wait_last_ns", started_at, monotonic.capture());
    summary_state.waits +%= 1;
    if (timed_out) summary_state.wait_timeouts +%= 1;
    summary_state.wait_total_ticks +%= waited_ticks;
    summary_state.wait_last_ticks = waited_ticks;
    if (waited_ticks > summary_state.wait_max_ticks) summary_state.wait_max_ticks = waited_ticks;
}

fn recordDurationLocked(
    owner: u32,
    comptime total_field: []const u8,
    comptime max_field: []const u8,
    comptime last_field: ?[]const u8,
    start: monotonic.Stamp,
    end: monotonic.Stamp,
) void {
    const elapsed = monotonic.elapsedNanoseconds(start, end) orelse {
        addMetric(owner, "timing_unavailable", 1);
        return;
    };
    addMetric(owner, total_field, elapsed);
    maxMetric(owner, max_field, elapsed);
    if (last_field) |field| setMetric(owner, field, elapsed);
}

fn noteCompletionAgeLocked(owner: u32, completed_at: monotonic.Stamp) void {
    const age = monotonic.elapsedSince(completed_at) orelse {
        addMetric(owner, "timing_unavailable", 1);
        return;
    };
    maxMetric(owner, "completion_age_max_ns", age);
}

fn statusFromItemLocked(slot: usize) CompletionStatus {
    const item = items[slot];
    const state = book.state(slot);
    const queue_end = switch (state) {
        .running, .completed => item.started_tick,
        .cancelled => item.completed_tick,
        .free, .queued => 0,
    };
    const queue_ticks = if (queue_end == 0) 0 else elapsedTicks(item.submitted_tick, queue_end);
    const run_ticks = switch (state) {
        .completed => elapsedTicks(item.started_tick, item.completed_tick),
        .running => elapsedTicks(item.started_tick, timer.tickCount()),
        .free, .queued, .cancelled => 0,
    };
    return .{
        .handle = item.handle,
        .state = stateCode(state),
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

fn noteOwnerReserveLocked(owner: u32, source: work_queue.SourceClass) void {
    const current = ownerCurrent(owner) orelse return;
    current.reserve(source);
}

fn noteOwnerStartedLocked(owner: u32, source: work_queue.SourceClass) void {
    const current = ownerCurrent(owner) orelse return;
    current.start(source);
}

fn noteOwnerCancelledLocked(owner: u32, source: work_queue.SourceClass) void {
    const current = ownerCurrent(owner) orelse return;
    current.cancel(source);
}

fn noteOwnerCompletedLocked(owner: u32) void {
    const current = ownerCurrent(owner) orelse return;
    current.complete();
}

fn noteOwnerReleaseLocked(owner: u32, state: work_queue.State) void {
    const current = ownerCurrent(owner) orelse return;
    current.release(state);
}

fn noteWaiterAddedLocked(owner: u32) void {
    global_waiters +|= 1;
    if (global_waiters > global_waiters_max) global_waiters_max = global_waiters;
    if (ownerCurrent(owner)) |current| {
        current.addWaiter();
    }
}

fn noteWaiterRemovedLocked(owner: u32) void {
    if (global_waiters != 0) global_waiters -= 1;
    if (ownerCurrent(owner)) |current| {
        current.removeWaiter();
    }
}

fn appendOwnerFinalLocked(owner: u32, slot: usize) void {
    const index = ownerIndex(owner) orelse return;
    const slot_u8: u8 = @intCast(slot);
    const tail = owner_final_tail[index];
    items[slot].owner_final_prev = tail;
    items[slot].owner_final_next = work_queue.no_slot;
    if (tail == work_queue.no_slot) {
        owner_final_head[index] = slot_u8;
    } else {
        items[tail].owner_final_next = slot_u8;
    }
    owner_final_tail[index] = slot_u8;
}

fn removeOwnerFinalLocked(owner: u32, slot: usize) void {
    const index = ownerIndex(owner) orelse return;
    const previous = items[slot].owner_final_prev;
    const next = items[slot].owner_final_next;
    if (previous == work_queue.no_slot) {
        owner_final_head[index] = next;
    } else {
        items[previous].owner_final_next = next;
    }
    if (next == work_queue.no_slot) {
        owner_final_tail[index] = previous;
    } else {
        items[next].owner_final_prev = previous;
    }
    items[slot].owner_final_prev = work_queue.no_slot;
    items[slot].owner_final_next = work_queue.no_slot;
}

fn oldestFinalForOwnerLocked(owner: u32) ?usize {
    const raw = if (owner == 0)
        book.final_head
    else if (ownerIndex(owner)) |index|
        owner_final_head[index]
    else
        work_queue.no_slot;
    if (raw == work_queue.no_slot) return null;
    return raw;
}

fn findOwnerSlotLocked(owner: u32) OwnerSearch {
    var scanned: u32 = 0;
    var running_slot: ?usize = null;
    var final_slot: ?usize = null;
    var index: usize = 0;
    while (index < items.len) : (index += 1) {
        scanned +|= 1;
        if (items[index].owner != owner) continue;
        switch (book.state(index)) {
            .queued => return .{ .slot = index, .scanned = scanned },
            .running => if (running_slot == null) {
                running_slot = index;
            },
            .completed, .cancelled => if (final_slot == null) {
                final_slot = index;
            },
            .free => {},
        }
    }
    return .{ .slot = running_slot orelse final_slot, .scanned = scanned };
}

fn syncLegacyCurrentLocked() void {
    summary_state.queue_depth = book.queued_count;
    summary_state.active_workers = book.running_count;
    summary_state.queue_high_water = book.queue_high_water;
}

fn cleanupExpired(started_tick: u64) bool {
    return elapsedTicks(started_tick, timer.tickCount()) >= CLEANUP_JOIN_TIMEOUT_TICKS;
}

fn cleanupRemainingTicks(started_tick: u64) u64 {
    const elapsed = elapsedTicks(started_tick, timer.tickCount());
    if (elapsed >= CLEANUP_JOIN_TIMEOUT_TICKS) return 0;
    return CLEANUP_JOIN_TIMEOUT_TICKS - elapsed;
}

fn currentIsWorker() bool {
    const current_id = scheduler.currentId() orelse return false;
    return current_id == worker_task_id;
}

fn ownerHasHistoryLocked(owner: u32, metrics: *const PerformanceMetrics, current: OwnerCurrent) bool {
    return ownerIndex(owner) != null and
        (current.used != 0 or current.used_high_water != 0 or metrics.submitted != 0 or metrics.cleanup_calls != 0);
}

fn ownerIndex(owner: u32) ?usize {
    if (owner == 0 or owner > OWNER_CAPACITY) return null;
    return @intCast(owner - 1);
}

fn ownerMetrics(owner: u32) ?*PerformanceMetrics {
    const index = ownerIndex(owner) orelse return null;
    return &owner_metrics[index];
}

fn ownerMetricsConst(owner: u32) ?*const PerformanceMetrics {
    const index = ownerIndex(owner) orelse return null;
    return &owner_metrics[index];
}

fn ownerCurrent(owner: u32) ?*OwnerCurrent {
    const index = ownerIndex(owner) orelse return null;
    return &owner_current[index];
}

fn ownerCurrentConst(owner: u32) ?*const OwnerCurrent {
    const index = ownerIndex(owner) orelse return null;
    return &owner_current[index];
}

fn addMetric(owner: u32, comptime field: []const u8, value: anytype) void {
    @field(global_metrics, field) +%= @as(u64, @intCast(value));
    if (ownerMetrics(owner)) |metrics| {
        @field(metrics, field) +%= @as(u64, @intCast(value));
    }
}

fn setMetric(owner: u32, comptime field: []const u8, value: u64) void {
    @field(global_metrics, field) = value;
    if (ownerMetrics(owner)) |metrics| @field(metrics, field) = value;
}

fn maxMetric(owner: u32, comptime field: []const u8, value: u64) void {
    if (value > @field(global_metrics, field)) @field(global_metrics, field) = value;
    if (ownerMetrics(owner)) |metrics| {
        if (value > @field(metrics, field)) @field(metrics, field) = value;
    }
}

fn stateCode(state: work_queue.State) u32 {
    return switch (state) {
        .free => WORK_STATE_FREE,
        .queued => WORK_STATE_QUEUED,
        .running => WORK_STATE_RUNNING,
        .completed => WORK_STATE_COMPLETED,
        .cancelled => WORK_STATE_CANCELLED,
    };
}

fn isFinal(state: work_queue.State) bool {
    return state == .completed or state == .cancelled;
}

fn enterCritical() CriticalGuard {
    const from_irq = irq_router.inDispatch();
    const irq_flags = interrupts.saveAndDisable();
    return .{
        .irq_flags = irq_flags,
        .started_at = monotonic.capture(),
        .from_irq = from_irq,
    };
}

fn leaveCritical(guard: CriticalGuard, owner: u32) void {
    const ended_at = monotonic.capture();
    addMetric(owner, "critical_sections", 1);
    if (guard.from_irq) addMetric(owner, "critical_from_irq", 1);
    if (monotonic.elapsedNanoseconds(guard.started_at, ended_at)) |elapsed| {
        addMetric(owner, "critical_timing_samples", 1);
        addMetric(owner, "critical_total_ns", elapsed);
        maxMetric(owner, "critical_max_ns", elapsed);
        setMetric(owner, "critical_last_ns", elapsed);
    } else {
        addMetric(owner, "critical_timing_unavailable", 1);
    }
    interrupts.restore(guard.irq_flags);
}

fn elapsedTicks(start: u64, end: u64) u64 {
    if (end < start) return 0;
    return end - start;
}
