// Rust owns the canonical Work state and complete scheduling/completion runtime.
const work_queue = @import("driver_work_queue.zig");
const deadline_policy = @import("driver_work_deadline.zig");
comptime {
    _ = @import("driver_work_bridge.zig");
}
pub const VERSION: u32 = 1;
pub const PERFORMANCE_VERSION: u32 = 2;
pub const QUEUE_CAPACITY: u32 = @intCast(work_queue.capacity);
pub const OWNER_CAPACITY: u32 = 16;
pub const WORKER_COUNT: u32 = 2;
pub const DEADLINE_WORKER_COUNT: u32 = 1;
pub const DEADLINE_QUEUE_CAPACITY: u32 = work_queue.deadline_queue_capacity;
pub const DEADLINE_RESERVED_CAPACITY: u32 = work_queue.deadline_reserved_capacity;
pub const DEADLINE_MAX_BUDGET_TICKS: u64 = deadline_policy.max_budget_ticks;
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

// R4D DriverApi v20. Audio refill work declares an absolute one-shot tick
// deadline, a bounded callback budget, and an opaque stable device key.
pub const WorkRequest = extern struct {
    version: u32 = deadline_policy.request_version,
    size: u32 = @sizeOf(WorkRequest),
    handler: WorkHandler,
    context: usize = 0,
    flags: u32 = WORK_FLAG_NONE,
    work_class: u32 = deadline_policy.class_audio_refill,
    serial_key: u64 = 0,
    deadline_tick: u64 = 0,
    budget_ticks: u64 = 0,
};

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
    deadline_worker_started: u32 = 0,
    deadline_worker_task_id: u32 = 0,
    deadline_worker_count: u32 = DEADLINE_WORKER_COUNT,
    deadline_queue_capacity: u32 = DEADLINE_QUEUE_CAPACITY,
    deadline_queued_slots: u32 = 0,
    deadline_running_slots: u32 = 0,
    deadline_queue_high_water: u32 = 0,
    owner_deadline_queued_slots: u32 = 0,
    owner_deadline_running_slots: u32 = 0,
    owner_deadline_queue_high_water: u32 = 0,
    deadline_submitted: u64 = 0,
    deadline_started: u64 = 0,
    deadline_completed: u64 = 0,
    deadline_misses: u64 = 0,
    deadline_budget_overruns: u64 = 0,
    deadline_queue_rejections: u64 = 0,
    deadline_queue_total_ticks: u64 = 0,
    deadline_queue_max_ticks: u64 = 0,
    deadline_lateness_total_ticks: u64 = 0,
    deadline_lateness_max_ticks: u64 = 0,
};

pub const CleanupResult = extern struct { removed: u32 = 0, quiesced: bool = true };
extern fn r4os_rust_driver_work_init() bool;
extern fn r4os_rust_driver_work_submit(owner: u32, handler: WorkHandler, context: usize, flags: u32, owned: bool, out: *u32) i32;
extern fn r4os_rust_driver_work_request(owner: u32, request: *const WorkRequest, out: *u32) i32;
extern fn r4os_rust_driver_work_cancel(handle: u32) i32;
extern fn r4os_rust_driver_work_wait(handle: u32, ticks: u64, out: *i32) i32;
extern fn r4os_rust_driver_work_status(handle: u32, out: *CompletionStatus) i32;
extern fn r4os_rust_driver_work_release(handle: u32) i32;
extern fn r4os_rust_driver_work_cleanup(owner: u32, out: *CleanupResult) void;
extern fn r4os_rust_driver_work_sleep_note(denied: bool, ticks: u64) void;
extern fn r4os_rust_driver_work_summary(out: *Summary) void;
extern fn r4os_rust_driver_work_performance(owner: u32, out: *Performance) void;
extern fn r4os_rust_driver_work_current() u32;
comptime {
    if (@sizeOf(WorkRequest) != 56 or @sizeOf(CompletionStatus) != 72 or @sizeOf(Summary) != 248 or @sizeOf(PerformanceMetrics) != 640 or @sizeOf(Performance) != 928 or @sizeOf(CleanupResult) != 8) @compileError("Native Work C layout drift");
}
pub fn init() bool {
    return r4os_rust_driver_work_init();
}
pub fn submit(owner: u32, handler: WorkHandler, context: usize, flags: u32, out: *u32) i32 {
    return r4os_rust_driver_work_submit(owner, handler, context, flags, false, out);
}
pub fn submitOwned(owner: u32, handler: WorkHandler, context: usize, out: *u32) i32 {
    return r4os_rust_driver_work_submit(owner, handler, context, 0, true, out);
}
pub fn submitRequest(owner: u32, request: *const WorkRequest, out: *u32) i32 {
    return r4os_rust_driver_work_request(owner, request, out);
}
pub fn cancel(handle: u32) i32 {
    return r4os_rust_driver_work_cancel(handle);
}
pub fn completionWait(handle: u32, ticks: u64, out: *i32) i32 {
    return r4os_rust_driver_work_wait(handle, ticks, out);
}
pub fn completionStatus(handle: u32, out: *CompletionStatus) i32 {
    return r4os_rust_driver_work_status(handle, out);
}
pub fn completionRelease(handle: u32) i32 {
    return r4os_rust_driver_work_release(handle);
}
pub fn cleanupOwner(owner: u32) CleanupResult {
    var out: CleanupResult = undefined;
    r4os_rust_driver_work_cleanup(owner, &out);
    return out;
}
pub fn noteSleepWait(ticks: u64) void {
    r4os_rust_driver_work_sleep_note(false, ticks);
}
pub fn noteSleepDeniedFromIrq() void {
    r4os_rust_driver_work_sleep_note(true, 0);
}
pub fn summary() Summary {
    var out: Summary = undefined;
    r4os_rust_driver_work_summary(&out);
    return out;
}
pub fn performance(owner: u32) Performance {
    var out: Performance = undefined;
    r4os_rust_driver_work_performance(owner, &out);
    return out;
}
pub fn currentOwner() u32 {
    return r4os_rust_driver_work_current();
}
