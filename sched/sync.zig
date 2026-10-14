// Temporary private C facade. Native Rust owns every synchronization mutation;
// embedded records are fixed C storage, and callbacks use explicit C thunks.
const task = @import("task.zig");
const wait_node = @import("wait_node.zig");
const task_context = @import("task_context.zig");
pub const WAIT_FOREVER: u64 = @import("scheduler.zig").WAIT_FOREVER;
pub const WaitResult = task.WaitResult;
pub const WaitReleaseFn = *const fn (*anyopaque) void;
const Predicate = *const fn (*anyopaque) bool;
const CMode = enum(u8) { sleepable, no_sleep };
pub const LockMode = CMode;
const Text = extern struct {
    pointer: [*]const u8,
    len: usize,
    fn from(bytes: []const u8) Text {
        return .{ .pointer = bytes.ptr, .len = bytes.len };
    }
};
const WaitRequest = extern struct {
    timeout: u64,
    reason: Text,
    predicate: ?*const fn (*anyopaque) callconv(.c) bool = null,
    context: ?*anyopaque = null,
    release: ?*const fn (*anyopaque) callconv(.c) void = null,
    release_context: ?*anyopaque = null,
    flags: u64 = 0,
    consume_runtime: bool = false,
};
const Callbacks = struct {
    predicate: ?Predicate = null,
    context: ?*anyopaque = null,
    release: ?WaitReleaseFn = null,
    release_context: ?*anyopaque = null,
    fn needed(raw: *anyopaque) callconv(.c) bool {
        const ctx: *Callbacks = @ptrCast(@alignCast(raw));
        return ctx.predicate.?(ctx.context.?);
    }
    fn releaseOwner(raw: *anyopaque) callconv(.c) void {
        const ctx: *Callbacks = @ptrCast(@alignCast(raw));
        ctx.release.?(ctx.release_context.?);
    }
};
extern fn r4os_rust_sync_wait(*WaitQueue, *const WaitRequest) callconv(.c) u32;
extern fn r4os_rust_sync_queue(*WaitQueue, u8, ?*anyopaque, u64, ?*u64) callconv(.c) u64;
extern fn r4os_rust_sync_event(*EventV2, u8, u64) callconv(.c) u32;
extern fn r4os_rust_sync_completion(*Completion, u8, u64) callconv(.c) u32;
extern fn r4os_rust_sync_semaphore(*Semaphore, u8, u64, ?*const WaitRequest, ?*task_context.UnwindToken) callconv(.c) u64;
extern fn r4os_rust_sync_mutex(*Mutex, u8, u64) callconv(.c) bool;
extern fn r4os_rust_sync_guard(*UnwindGuard, u8, u64) callconv(.c) bool;
extern fn r4os_rust_sync_wait_summary(*WaitSummary) callconv(.c) void;
extern fn r4os_rust_sync_lock_summary(*LockSummary) callconv(.c) void;
extern fn r4os_rust_sync_note_sleep() callconv(.c) void;
extern fn r4os_rust_sync_timer(*WaitQueue, u64, [*]const u8, usize) callconv(.c) bool;
extern fn r4os_rust_sync_selftest() callconv(.c) bool;
pub fn noteSleepPoint() void {
    r4os_rust_sync_note_sleep();
}
pub fn summary() WaitSummary {
    var out: WaitSummary = undefined;
    r4os_rust_sync_wait_summary(&out);
    return out;
}
pub fn lockSummary() LockSummary {
    var out: LockSummary = undefined;
    r4os_rust_sync_lock_summary(&out);
    return out;
}
pub fn selfTest() bool {
    return r4os_rust_sync_selftest();
}
pub const WaitQueue = extern struct {
    core: wait_node.QueueCore = .{},
    pub fn init() WaitQueue {
        return .{};
    }
    pub fn wait(self: *WaitQueue, ticks: u64, reason: []const u8) WaitResult {
        return self.waitUnless(ticks, reason, null, null);
    }
    pub fn waitUnless(self: *WaitQueue, ticks: u64, reason: []const u8, predicate: ?Predicate, context: ?*anyopaque) WaitResult {
        var ctx = Callbacks{ .predicate = predicate, .context = context };
        const request = WaitRequest{ .timeout = ticks, .reason = Text.from(reason), .predicate = if (predicate != null) Callbacks.needed else null, .context = &ctx };
        return @enumFromInt(r4os_rust_sync_wait(self, &request));
    }
    pub fn waitUnlessRuntimeLocked(self: *WaitQueue, ticks: u64, reason: []const u8, predicate: ?Predicate, context: ?*anyopaque, flags: u64) WaitResult {
        var ctx = Callbacks{ .predicate = predicate, .context = context };
        const request = WaitRequest{ .timeout = ticks, .reason = Text.from(reason), .predicate = if (predicate != null) Callbacks.needed else null, .context = &ctx, .flags = flags, .consume_runtime = true };
        return @enumFromInt(r4os_rust_sync_wait(self, &request));
    }
    pub fn waitUnlessReleasing(self: *WaitQueue, ticks: u64, reason: []const u8, predicate: ?Predicate, context: ?*anyopaque, release: WaitReleaseFn, release_context: *anyopaque) WaitResult {
        var ctx = Callbacks{ .predicate = predicate, .context = context, .release = release, .release_context = release_context };
        const request = WaitRequest{ .timeout = ticks, .reason = Text.from(reason), .predicate = if (predicate != null) Callbacks.needed else null, .context = &ctx, .release = Callbacks.releaseOwner, .release_context = &ctx };
        return @enumFromInt(r4os_rust_sync_wait(self, &request));
    }
    pub fn wakeOne(self: *WaitQueue) u32 {
        return @intCast(r4os_rust_sync_queue(self, 0, null, 0, null));
    }
    pub fn wakeAll(self: *WaitQueue) u32 {
        return @intCast(r4os_rust_sync_queue(self, 1, null, 0, null));
    }
    pub fn cancelOne(self: *WaitQueue) u32 {
        return @intCast(r4os_rust_sync_queue(self, 2, null, 0, null));
    }
    pub fn cancelAll(self: *WaitQueue) u32 {
        return @intCast(r4os_rust_sync_queue(self, 3, null, 0, null));
    }
    pub fn close(self: *WaitQueue, result: WaitResult) u32 {
        return @intCast(r4os_rust_sync_queue(self, 4, null, @intFromEnum(result), null));
    }
    pub fn reopen(self: *WaitQueue) bool {
        return r4os_rust_sync_queue(self, 5, null, 0, null) != 0;
    }
    pub fn hasWaiters(self: *WaitQueue) bool {
        return r4os_rust_sync_queue(self, 6, null, 0, null) != 0;
    }
    pub fn cancelTask(self: *WaitQueue, target: *task.Task, generation: u64) bool {
        return r4os_rust_sync_queue(self, 7, target, generation, null) != 0;
    }
    pub fn bumpSequenceAndWakeAll(self: *WaitQueue, sequence: *u64) u32 {
        return @intCast(r4os_rust_sync_queue(self, 8, null, 0, sequence));
    }
    pub fn readSequence(self: *WaitQueue, sequence: *const u64) u64 {
        return r4os_rust_sync_queue(self, 9, null, 0, @constCast(sequence));
    }
};
pub const EventMode = enum(u8) { manual_reset, auto_reset };
pub const EventV2 = extern struct {
    signaled: bool = false,
    mode: EventMode = .manual_reset,
    queue: WaitQueue = WaitQueue.init(),
    pub fn init(signaled: bool) EventV2 {
        return .{ .signaled = signaled };
    }
    pub fn initMode(signaled: bool, mode: EventMode) EventV2 {
        return .{ .signaled = signaled, .mode = mode };
    }
    pub fn signal(self: *EventV2) void {
        _ = r4os_rust_sync_event(self, 0, 0);
    }
    pub fn reset(self: *EventV2) void {
        _ = r4os_rust_sync_event(self, 1, 0);
    }
    pub fn waitResult(self: *EventV2, ticks: u64) WaitResult {
        return @enumFromInt(r4os_rust_sync_event(self, 2, ticks));
    }
    pub fn wait(self: *EventV2, ticks: u64) bool {
        return self.waitResult(ticks) == .signaled;
    }
};
pub const Event = EventV2;
pub const Completion = extern struct {
    completed: u32 = 0,
    queue: WaitQueue = WaitQueue.init(),
    pub fn init() Completion {
        return .{};
    }
    pub fn complete(self: *Completion) void {
        _ = r4os_rust_sync_completion(self, 0, 0);
    }
    pub fn completeAll(self: *Completion) void {
        _ = self.completeAllCount();
    }
    pub fn completeAllCount(self: *Completion) u32 {
        return r4os_rust_sync_completion(self, 1, 0);
    }
    pub fn cancelAll(self: *Completion) u32 {
        return r4os_rust_sync_completion(self, 2, 0);
    }
    pub fn close(self: *Completion) u32 {
        return r4os_rust_sync_completion(self, 3, 0);
    }
    pub fn wait(self: *Completion, ticks: u64) WaitResult {
        return @enumFromInt(r4os_rust_sync_completion(self, 4, ticks));
    }
};
pub const Semaphore = extern struct {
    count: u32 = 0,
    max_count: u32 = 0,
    queue: WaitQueue = WaitQueue.init(),
    pub fn init(initial: u32, max_count: u32) Semaphore {
        return .{ .count = initial, .max_count = max_count };
    }
    pub fn tryAcquire(self: *Semaphore) bool {
        return r4os_rust_sync_semaphore(self, 0, 0, null, null) != 0;
    }
    pub fn acquire(self: *Semaphore, ticks: u64) WaitResult {
        return @enumFromInt(r4os_rust_sync_semaphore(self, 1, ticks, null, null));
    }
    pub fn acquireReleasing(self: *Semaphore, ticks: u64, release_fn: WaitReleaseFn, release_context: *anyopaque) WaitResult {
        var ctx = Callbacks{ .release = release_fn, .release_context = release_context };
        const request = WaitRequest{ .timeout = ticks, .reason = Text.from("semaphore"), .release = Callbacks.releaseOwner, .release_context = &ctx };
        return @enumFromInt(r4os_rust_sync_semaphore(self, 2, 0, &request, null));
    }
    pub fn acquireReleasingGuarded(self: *Semaphore, ticks: u64, release_fn: WaitReleaseFn, release_context: *anyopaque, unwind: *task_context.UnwindToken) WaitResult {
        var ctx = Callbacks{ .release = release_fn, .release_context = release_context };
        const request = WaitRequest{ .timeout = ticks, .reason = Text.from("semaphore"), .release = Callbacks.releaseOwner, .release_context = &ctx };
        return @enumFromInt(r4os_rust_sync_semaphore(self, 3, 0, &request, unwind));
    }
    pub fn release(self: *Semaphore, count: u32) u32 {
        return @intCast(r4os_rust_sync_semaphore(self, 4, count, null, null));
    }
};
pub const TimerWait = extern struct {
    queue: WaitQueue = WaitQueue.init(),
    pub fn init() TimerWait {
        return .{};
    }
    pub fn wait(self: *TimerWait, ticks: u64) bool {
        return self.waitReason(ticks, "timer");
    }
    pub fn waitReason(self: *TimerWait, ticks: u64, reason: []const u8) bool {
        return r4os_rust_sync_timer(&self.queue, ticks, reason.ptr, reason.len);
    }
};
pub const Mutex = extern struct {
    owner: u32 = 0,
    owner_generation: u64 = 0,
    depth: u32 = 0,
    rank: u16 = LockRank.local,
    mode: LockMode = .sleepable,
    name: Text = Text.from("local"),
    queue: WaitQueue = WaitQueue.init(),
    donation_rank: u8 = task.no_dispatch_rank,
    handoff_pending: bool = false,
    pub fn init() Mutex {
        return .{};
    }
    pub fn initClass(name: []const u8, rank: u16, mode: LockMode) Mutex {
        return .{ .name = Text.from(name), .rank = rank, .mode = mode };
    }
    pub fn tryLock(self: *Mutex) bool {
        return r4os_rust_sync_mutex(self, 0, 0);
    }
    pub fn lock(self: *Mutex, ticks: u64) bool {
        return r4os_rust_sync_mutex(self, 1, ticks);
    }
    pub fn unlock(self: *Mutex) bool {
        return r4os_rust_sync_mutex(self, 2, 0);
    }
};
pub const UnwindGuard = extern struct {
    owner: u32 = 0,
    owner_generation: u64 = 0,
    boot_owner: bool = false,
    depth: u32 = 0,
    name: Text = Text.from("unwind"),
    queue: WaitQueue = WaitQueue.init(),
    pub fn init(name: []const u8) UnwindGuard {
        return .{ .name = Text.from(name) };
    }
    pub fn ownedByCurrent(self: *const UnwindGuard) bool {
        return r4os_rust_sync_guard(@constCast(self), 0, 0);
    }
    pub fn tryEnter(self: *UnwindGuard) bool {
        return r4os_rust_sync_guard(self, 1, 0);
    }
    pub fn enter(self: *UnwindGuard, ticks: u64) bool {
        return r4os_rust_sync_guard(self, 2, ticks);
    }
    pub fn leave(self: *UnwindGuard) bool {
        return r4os_rust_sync_guard(self, 3, 0);
    }
};

pub const LockRank = struct {
    pub const local: u16 = 10;
    pub const irq_critical: u16 = 20;
    pub const program_instances: u16 = 100;
    // Dynamic ProgramInstance registry. IRQ/exception attribution uses stable
    // task-bound owner addresses and never acquires this sleepable lock.
    pub const program_registry: u16 = 110;
    // The persistent R4R hive cache protects only already resident bytes and
    // parsed views. Registry file I/O is serialized by a separate untracked
    // transaction gate and never runs while this lock is held.
    pub const registry_state: u16 = 115;
    pub const service_registry: u16 = 120;
    // Stable service endpoints are pinned through service_registry and then
    // protect their queue, payload and waiter state independently.
    pub const service_endpoint: u16 = 130;
    pub const block_registry: u16 = 140;
    pub const fs_volume: u16 = 150;
    // 0.56.8: Page-Cache liegt zwischen FS (ruft den Cache) und
    // Block-Device (der Cache ruft block.read/write, die yielden).
    pub const fs_page_cache: u16 = 155;
    pub const block_device: u16 = 160;
    pub const audio_core: u16 = 280;
    pub const driver_registry: u16 = 300;
    pub const protocol_registry: u16 = 320;
    pub const display_state: u16 = 400;
};

pub const WaitSummary = extern struct {
    queue_waits: u64 = 0,
    wake_one: u64 = 0,
    wake_all: u64 = 0,
    timeouts: u64 = 0,
    cancellations: u64 = 0,
    drops: u64 = 0,
    total_wait_ticks: u64 = 0,
    max_wait_ticks: u64 = 0,
    last_wait_ticks: u64 = 0,
    directed_wakes: u64 = 0,
    directed_scans: u64 = 0,
    directed_fifo_bypasses: u64 = 0,
};

pub const LockSummary = extern struct {
    acquires: u64 = 0,
    releases: u64 = 0,
    recursive_acquires: u64 = 0,
    contention_waits: u64 = 0,
    contention_timeouts: u64 = 0,
    order_violations: u64 = 0,
    sleep_checks: u64 = 0,
    sleep_under_lock: u64 = 0,
    sleep_under_no_sleep_lock: u64 = 0,
    unlock_mismatches: u64 = 0,
    held_slots_used: u32 = 0,
    current_depth: u32 = 0,
    max_depth: u32 = 0,
    tracking_drops: u32 = 0,
    role_inversions: u64 = 0,
    role_donations: u64 = 0,
    role_donation_releases: u64 = 0,
};

comptime {
    if (@sizeOf(WaitQueue) != 40) @compileError("Native sync C layout mismatch: WaitQueue");
    if (@sizeOf(EventV2) != 48) @compileError("Native sync C layout mismatch: EventV2");
    if (@sizeOf(Completion) != 48) @compileError("Native sync C layout mismatch: Completion");
    if (@sizeOf(Semaphore) != 48) @compileError("Native sync C layout mismatch: Semaphore");
    if (@sizeOf(Mutex) != 88) @compileError("Native sync C layout mismatch: Mutex");
    if (@sizeOf(UnwindGuard) != 80) @compileError("Native sync C layout mismatch: UnwindGuard");
    if (@sizeOf(WaitRequest) != 72) @compileError("Native sync C layout mismatch: WaitRequest");
    if (@sizeOf(WaitSummary) != 96) @compileError("Native sync C layout mismatch: WaitSummary");
    if (@sizeOf(LockSummary) != 120) @compileError("Native sync C layout mismatch: LockSummary");
}
