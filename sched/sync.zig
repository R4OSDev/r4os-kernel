const scheduler = @import("scheduler.zig");
const task = @import("task.zig");
const wait_node = @import("wait_node.zig");
const task_context = @import("task_context.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const timer = @import("../kernel/timer.zig");
const config = @import("config");

pub const WAIT_FOREVER: u64 = scheduler.WAIT_FOREVER;
pub const WaitResult = task.WaitResult;
pub const WaitReleaseFn = *const fn (*anyopaque) void;

pub const WaitSummary = struct {
    queue_waits: u64 = 0,
    wake_one: u64 = 0,
    wake_all: u64 = 0,
    timeouts: u64 = 0,
    cancellations: u64 = 0,
    drops: u64 = 0,
    total_wait_ticks: u64 = 0,
    max_wait_ticks: u64 = 0,
    last_wait_ticks: u64 = 0,
};

var global_summary: WaitSummary = .{};

pub fn summary() WaitSummary {
    return global_summary;
}

pub const LockRank = struct {
    pub const local: u16 = 10;
    pub const irq_critical: u16 = 20;
    pub const program_instances: u16 = 100;
    // Dynamic ProgramInstance registry. IRQ/exception attribution uses stable
    // task-bound owner addresses and never acquires this sleepable lock.
    pub const program_registry: u16 = 110;
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
    pub const driver_registry: u16 = 300;
    pub const protocol_registry: u16 = 320;
    pub const display_state: u16 = 400;
};

pub const LockMode = enum(u8) {
    sleepable,
    no_sleep,
};

pub const LockSummary = struct {
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
};

var global_lock_summary: LockSummary = .{};

pub fn lockSummary() LockSummary {
    var out = global_lock_summary;
    out.held_slots_used = countHeldSlots();
    if (scheduler.current()) |current_task| {
        out.current_depth = heldDepthFor(current_task);
    }
    return out;
}

pub fn noteSleepPoint() void {
    // Lock tracking lives on the stable task object. It therefore grows with
    // the dynamic task registry instead of imposing a second global capacity.
    if (comptime !config.enable_metrics) return;
    const current_task = scheduler.current() orelse return;
    global_lock_summary.sleep_checks +%= 1;
    var held_any = false;
    var held_no_sleep = false;
    var i: usize = 0;
    while (i < current_task.held_locks.len) : (i += 1) {
        const held = current_task.held_locks[i];
        if (!held.active) continue;
        held_any = true;
        if (held.mode_no_sleep) held_no_sleep = true;
    }
    if (held_any) global_lock_summary.sleep_under_lock +%= 1;
    if (held_no_sleep) global_lock_summary.sleep_under_no_sleep_lock +%= 1;
}

pub const WaitQueue = struct {
    core: wait_node.QueueCore = .{},

    pub fn init() WaitQueue {
        return .{};
    }

    pub fn wait(self: *WaitQueue, timeout_ticks: u64, reason: []const u8) WaitResult {
        return self.waitUnless(timeout_ticks, reason, null, null);
    }

    // Predicate, intrusive enrollment and the scheduler blocked transition
    // share one IRQ/preemption critical section. Signal, timeout, cancel and
    // kill all detach the task-owned node before making the task runnable.
    // Consequently the resumed task only reads its Task result; it never
    // dereferences this queue or the predicate context after parkBlocked.
    pub fn waitUnless(self: *WaitQueue, timeout_ticks: u64, reason: []const u8, still_needed: ?*const fn (*anyopaque) bool, ctx: ?*anyopaque) WaitResult {
        const current_task = scheduler.current() orelse return .failed;
        const irq_flags = self.enterCritical();
        return self.waitUnlessCritical(timeout_ticks, reason, still_needed, ctx, current_task, irq_flags);
    }

    // Atomically transfers a caller-held owner lock into this wait queue.
    // The release callback runs after the queue IRQ/preemption critical
    // section has started but before the predicate and intrusive enrollment.
    // A producer that needs the same owner lock can therefore neither publish
    // and miss an unenrolled waiter nor recycle the predicate context in the
    // unlock-to-wait gap. The owner lock is already released when this method
    // parks, so no lock is held across the sleep.
    pub fn waitUnlessReleasing(
        self: *WaitQueue,
        timeout_ticks: u64,
        reason: []const u8,
        still_needed: ?*const fn (*anyopaque) bool,
        ctx: ?*anyopaque,
        release: WaitReleaseFn,
        release_ctx: *anyopaque,
    ) WaitResult {
        const current_task = scheduler.current();
        const irq_flags = self.enterCritical();
        release(release_ctx);
        const admitted_task = current_task orelse {
            self.leaveCritical(irq_flags);
            return .failed;
        };
        return self.waitUnlessCritical(timeout_ticks, reason, still_needed, ctx, admitted_task, irq_flags);
    }

    fn waitUnlessCritical(
        self: *WaitQueue,
        timeout_ticks: u64,
        reason: []const u8,
        still_needed: ?*const fn (*anyopaque) bool,
        ctx: ?*anyopaque,
        current_task: *task.Task,
        irq_flags: u64,
    ) WaitResult {
        if (self.core.closing) {
            self.leaveCritical(irq_flags);
            global_summary.cancellations +%= 1;
            return .cancelled;
        }
        if (still_needed) |pred| {
            if (!pred(ctx.?)) {
                self.leaveCritical(irq_flags);
                return .signaled;
            }
        }
        if (timeout_ticks == 0) {
            self.leaveCritical(irq_flags);
            global_summary.timeouts +%= 1;
            return .timeout;
        }
        noteSleepPoint();
        if (!wait_node.link(&self.core, &current_task.wait_node, @ptrCast(current_task), current_task.generation)) {
            self.leaveCritical(irq_flags);
            global_summary.drops +%= 1;
            return .failed;
        }
        const blocked_task = scheduler.blockCurrent(self.objectId(), timeout_ticks, reason) orelse {
            _ = wait_node.detach(&current_task.wait_node);
            self.leaveCritical(irq_flags);
            global_summary.drops +%= 1;
            return .failed;
        };
        self.leaveCritical(irq_flags);
        global_summary.queue_waits +%= 1;
        scheduler.parkBlocked(blocked_task);
        const result = blocked_task.wait_result;
        recordQueueWaitLatency(blocked_task.last_wait_ticks);
        if (result == .timeout) global_summary.timeouts +%= 1;
        if (result == .cancelled) global_summary.cancellations +%= 1;
        return result;
    }

    pub fn wakeOne(self: *WaitQueue) u32 {
        const id = self.wakeOneWith(.signaled);
        if (id != 0) global_summary.wake_one +%= 1;
        return id;
    }

    pub fn wakeAll(self: *WaitQueue) u32 {
        const count = self.wakeAllWith(.signaled);
        if (count != 0) global_summary.wake_all +%= 1;
        return count;
    }

    // A sequence predicate and its wake must share the same critical section.
    // Otherwise a waiter can observe the old sequence, while the producer has
    // already advanced it but has not yet reached wakeAll().
    pub fn bumpSequenceAndWakeAll(self: *WaitQueue, sequence: *u64) u32 {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        sequence.* +%= 1;
        const count = self.wakeAllWithLocked(.signaled);
        if (count != 0) global_summary.wake_all +%= 1;
        return count;
    }

    pub fn readSequence(self: *WaitQueue, sequence: *const u64) u64 {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        return sequence.*;
    }

    pub fn cancelOne(self: *WaitQueue) u32 {
        return self.wakeOneWith(.cancelled);
    }

    pub fn cancelAll(self: *WaitQueue) u32 {
        return self.wakeAllWith(.cancelled);
    }

    fn wakeAllWith(self: *WaitQueue, result: WaitResult) u32 {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        return self.wakeAllWithLocked(result);
    }

    fn wakeAllWithLocked(self: *WaitQueue, result: WaitResult) u32 {
        var count: u32 = 0;
        while (self.popAndWake(result) != 0) : (count +|= 1) {}
        return count;
    }

    pub fn close(self: *WaitQueue, result: WaitResult) u32 {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        wait_node.close(&self.core);
        return self.wakeAllWithLocked(result);
    }

    pub fn reopen(self: *WaitQueue) bool {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        return wait_node.reopen(&self.core);
    }

    pub fn hasWaiters(self: *WaitQueue) bool {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        return self.core.count != 0;
    }

    fn objectId(self: *const WaitQueue) u64 {
        return @intFromPtr(self);
    }

    fn wakeOneWith(self: *WaitQueue, result: WaitResult) u32 {
        const irq_flags = self.enterCritical();
        defer self.leaveCritical(irq_flags);
        return self.popAndWake(result);
    }

    fn popAndWake(self: *WaitQueue, result: WaitResult) u32 {
        while (wait_node.popFront(&self.core)) |node| {
            const target: *task.Task = @ptrCast(@alignCast(node.owner));
            if (target.generation != node.owner_generation) continue;
            if (scheduler.wakeTask(target, result)) return target.id;
        }
        return 0;
    }

    // A semaphore permit is owned as soon as its FIFO waiter is selected,
    // before that waiter can run again. Protect this short handoff so a hard
    // kill cannot strand the permit between wake and caller publication.
    fn popAndWakeWithHandoffGuard(self: *WaitQueue) u32 {
        while (wait_node.popFront(&self.core)) |node| {
            const target: *task.Task = @ptrCast(@alignCast(node.owner));
            if (target.generation != node.owner_generation) continue;
            if (target.wait_handoff_guard_pending or target.unwind_guard_count == 0xFFFF_FFFF) {
                _ = scheduler.wakeTask(target, .failed);
                continue;
            }
            target.unwind_guard_count += 1;
            target.wait_handoff_guard_pending = true;
            if (scheduler.wakeTask(target, .signaled)) return target.id;
            target.wait_handoff_guard_pending = false;
            target.unwind_guard_count -= 1;
        }
        return 0;
    }

    fn enterCritical(self: *WaitQueue) u64 {
        _ = self;
        const irq_flags = interrupts.saveAndDisable();
        scheduler.preemptDisable();
        return irq_flags;
    }

    fn leaveCritical(self: *WaitQueue, irq_flags: u64) void {
        _ = self;
        scheduler.preemptEnable();
        interrupts.restore(irq_flags);
    }
};

fn recordQueueWaitLatency(wait_ticks: u64) void {
    global_summary.total_wait_ticks +%= wait_ticks;
    global_summary.last_wait_ticks = wait_ticks;
    if (wait_ticks > global_summary.max_wait_ticks) global_summary.max_wait_ticks = wait_ticks;
}

pub const EventMode = enum(u8) {
    manual_reset,
    auto_reset,
};

pub const EventV2 = struct {
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
        // 0.56.13: signaled-Update und Wake atomar (Critical Section) -
        // im auto_reset-Pfad konnte zwischen wakeOne()==0 und
        // signaled=true ein Waiter eintreten und das Signal verlieren.
        const irq_flags = self.queue.enterCritical();
        if (self.mode == .auto_reset) {
            if (self.queue.wakeOneWith(.signaled) == 0) {
                self.signaled = true;
            } else {
                global_summary.wake_one +%= 1;
            }
            self.queue.leaveCritical(irq_flags);
            return;
        }
        self.signaled = true;
        self.queue.leaveCritical(irq_flags);
        _ = self.queue.wakeAll();
    }

    pub fn reset(self: *EventV2) void {
        self.signaled = false;
    }

    fn stillNeeded(raw: *anyopaque) bool {
        const self: *EventV2 = @ptrCast(@alignCast(raw));
        if (self.signaled) {
            if (self.mode == .auto_reset) self.signaled = false;
            return false;
        }
        return true;
    }

    pub fn waitResult(self: *EventV2, timeout_ticks: u64) WaitResult {
        // 0.56.13 (Befund 4.5b): signaled-Check laeuft atomar mit
        // addWaiter in waitUnless - kein Lost-Wakeup-Fenster mehr.
        return self.queue.waitUnless(timeout_ticks, "event", stillNeeded, self);
    }

    pub fn wait(self: *EventV2, timeout_ticks: u64) bool {
        return self.waitResult(timeout_ticks) == .signaled;
    }
};

pub const Event = EventV2;

pub const Completion = struct {
    completed: u32 = 0,
    queue: WaitQueue = WaitQueue.init(),

    pub fn init() Completion {
        return .{};
    }

    pub fn complete(self: *Completion) void {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        if (self.queue.core.closing) return;
        if (self.queue.popAndWake(.signaled) == 0) {
            self.completed +|= 1;
        } else {
            global_summary.wake_one +%= 1;
        }
    }

    pub fn completeAll(self: *Completion) void {
        _ = self.completeAllCount();
    }

    pub fn completeAllCount(self: *Completion) u32 {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        if (self.queue.core.closing) return 0;
        const woke = self.queue.wakeAllWithLocked(.signaled);
        if (woke == 0) {
            self.completed +|= 1;
        } else {
            global_summary.wake_all +%= 1;
        }
        return woke;
    }

    pub fn cancelAll(self: *Completion) u32 {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        self.completed = 0;
        return self.queue.wakeAllWithLocked(.cancelled);
    }

    pub fn close(self: *Completion) u32 {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        self.completed = 0;
        wait_node.close(&self.queue.core);
        return self.queue.wakeAllWithLocked(.cancelled);
    }

    fn stillNeeded(raw: *anyopaque) bool {
        const self: *Completion = @ptrCast(@alignCast(raw));
        if (self.completed == 0) return true;
        self.completed -= 1;
        return false;
    }

    pub fn wait(self: *Completion, timeout_ticks: u64) WaitResult {
        return self.queue.waitUnless(timeout_ticks, "completion", stillNeeded, self);
    }
};

pub const Semaphore = struct {
    count: u32 = 0,
    max_count: u32 = 0,
    queue: WaitQueue = WaitQueue.init(),

    pub fn init(initial: u32, max_count: u32) Semaphore {
        return .{ .count = initial, .max_count = max_count };
    }

    pub fn tryAcquire(self: *Semaphore) bool {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        return self.tryAcquireLocked();
    }

    fn tryAcquireLocked(self: *Semaphore) bool {
        if (self.count == 0) return false;
        self.count -= 1;
        return true;
    }

    pub fn acquire(self: *Semaphore, timeout_ticks: u64) WaitResult {
        // Before scheduler start only the non-blocking consume is meaningful.
        if (scheduler.current() == null) return if (self.tryAcquire()) .signaled else .failed;
        // Token consumption and intrusive waiter enrollment happen under the
        // same queue critical section. A release can therefore neither place
        // a token between a stale pre-check and enrollment nor lose a wake.
        return finishUnguardedAcquire(self.queue.waitUnless(timeout_ticks, "semaphore", stillNeeded, self));
    }

    pub fn acquireReleasing(self: *Semaphore, timeout_ticks: u64, release_fn: WaitReleaseFn, release_ctx: *anyopaque) WaitResult {
        return finishUnguardedAcquire(self.queue.waitUnlessReleasing(timeout_ticks, "semaphore", stillNeeded, self, release_fn, release_ctx));
    }

    const GuardedAcquireContext = struct {
        semaphore: *Semaphore,
        unwind: *task_context.UnwindToken,
        admission_failed: bool = false,
    };

    // Like acquireReleasing, but returns an active unwind token with every
    // acquired permit. The token is armed inside the queue critical section
    // both for predicate consumption and FIFO wake handoff.
    pub fn acquireReleasingGuarded(
        self: *Semaphore,
        timeout_ticks: u64,
        release_fn: WaitReleaseFn,
        release_ctx: *anyopaque,
        unwind: *task_context.UnwindToken,
    ) WaitResult {
        unwind.* = .{};
        var ctx = GuardedAcquireContext{ .semaphore = self, .unwind = unwind };
        const result = self.queue.waitUnlessReleasing(
            timeout_ticks,
            "semaphore",
            guardedStillNeeded,
            &ctx,
            release_fn,
            release_ctx,
        );
        if (result != .signaled) return result;
        if (unwind.active) return .signaled;
        if (ctx.admission_failed) return .failed;
        return if (claimHandoffGuard(unwind)) .signaled else .failed;
    }

    pub fn release(self: *Semaphore, count: u32) u32 {
        const irq_flags = self.queue.enterCritical();
        defer self.queue.leaveCritical(irq_flags);
        var released: u32 = 0;
        var i: u32 = 0;
        while (i < count) : (i += 1) {
            if (self.queue.popAndWakeWithHandoffGuard() == 0) {
                if (self.count >= self.max_count) break;
                self.count += 1;
            } else {
                global_summary.wake_one +%= 1;
            }
            released += 1;
        }
        return released;
    }

    fn stillNeeded(raw: *anyopaque) bool {
        const self: *Semaphore = @ptrCast(@alignCast(raw));
        return !self.tryAcquireLocked();
    }

    fn guardedStillNeeded(raw: *anyopaque) bool {
        const ctx: *GuardedAcquireContext = @ptrCast(@alignCast(raw));
        if (ctx.semaphore.count == 0) return true;
        const unwind = task_context.enterUnwind();
        if (!unwind.admitted()) {
            ctx.admission_failed = true;
            return false;
        }
        ctx.semaphore.count -= 1;
        ctx.unwind.* = unwind;
        return false;
    }

    fn claimHandoffGuard(unwind: *task_context.UnwindToken) bool {
        const current_task = scheduler.current() orelse return false;
        const irq_flags = interrupts.saveAndDisable();
        scheduler.preemptDisable();
        defer {
            scheduler.preemptEnable();
            interrupts.restore(irq_flags);
        }
        if (!current_task.wait_handoff_guard_pending or current_task.unwind_guard_count == 0) return false;
        current_task.wait_handoff_guard_pending = false;
        unwind.* = .{
            .counter = &current_task.unwind_guard_count,
            .context_present = true,
            .active = true,
        };
        return true;
    }

    fn finishUnguardedAcquire(result: WaitResult) WaitResult {
        if (result != .signaled) return result;
        var unwind: task_context.UnwindToken = .{};
        if (claimHandoffGuard(&unwind)) _ = task_context.leaveUnwind(unwind);
        return result;
    }
};

pub const TimerWait = struct {
    queue: WaitQueue = WaitQueue.init(),

    pub fn init() TimerWait {
        return .{};
    }

    pub fn wait(self: *TimerWait, ticks: u64) bool {
        return self.waitReason(ticks, "timer");
    }

    pub fn waitReason(self: *TimerWait, ticks: u64, reason: []const u8) bool {
        if (ticks == 0) {
            noteSleepPoint();
            scheduler.yield();
            return true;
        }
        return self.queue.wait(ticks, reason) == .timeout;
    }
};

pub const Mutex = struct {
    owner: u32 = 0,
    owner_generation: u64 = 0,
    depth: u32 = 0,
    rank: u16 = LockRank.local,
    mode: LockMode = .sleepable,
    name: []const u8 = "local",
    queue: WaitQueue = WaitQueue.init(),

    pub fn init() Mutex {
        return .{};
    }

    pub fn initClass(name: []const u8, rank: u16, mode: LockMode) Mutex {
        return .{ .rank = rank, .mode = mode, .name = name };
    }

    pub fn tryLock(self: *Mutex) bool {
        const current_task = scheduler.current() orelse return false;
        const current_id = current_task.id;
        const current_generation = current_task.generation;
        scheduler.preemptDisable();
        defer scheduler.preemptEnable();

        if (self.owner == 0 and self.owner_generation == 0) {
            if (current_task.held_lock_count == 0xFFFF_FFFF) return false;
            recordLockOrder(current_task, self.objectId(), self.rank);
            self.owner = current_id;
            self.owner_generation = current_generation;
            self.depth = 1;
            current_task.held_lock_count += 1;
            recordLockAcquire(current_task, self.objectId(), self.rank, self.mode);
            return true;
        }
        if (self.owner == current_id and self.owner_generation == current_generation) {
            if (self.depth == 0xFFFF_FFFF) return false;
            self.depth += 1;
            global_lock_summary.acquires +%= 1;
            global_lock_summary.recursive_acquires +%= 1;
            return true;
        }
        return false;
    }

    pub fn lock(self: *Mutex, timeout_ticks: u64) bool {
        if (self.tryLock()) return true;
        if (self.permanentAdmissionFailure() or timeout_ticks == 0) return false;
        global_lock_summary.contention_waits +%= 1;
        // 0.56.13 (Befund 4.5a): Restzeit-Budget statt vollem Timeout je
        // Wakeup-Retry. Vorher bekam jeder Retry (geweckt, Lock aber vom
        // naechsten Bewerber weggeschnappt) wieder volle timeout_ticks -
        // unter Dauer-Contention wartete lock() unbegrenzt.
        const bounded = timeout_ticks != WAIT_FOREVER;
        const deadline = if (bounded) timer.tickCount() +% timeout_ticks else 0;
        var remaining = timeout_ticks;
        while (true) {
            const result = self.queue.waitUnless(remaining, "mutex", stillOwned, self);
            if (result != .signaled) {
                global_lock_summary.contention_timeouts +%= 1;
                return false;
            }
            if (self.tryLock()) return true;
            if (self.permanentAdmissionFailure()) return false;
            if (bounded) {
                const now = timer.tickCount();
                if (now >= deadline) {
                    global_lock_summary.contention_timeouts +%= 1;
                    return false;
                }
                remaining = deadline - now;
            }
        }
    }

    pub fn unlock(self: *Mutex) bool {
        const current_task = scheduler.current() orelse return false;
        const current_id = current_task.id;
        const current_generation = current_task.generation;
        var wake_waiter = false;

        scheduler.preemptDisable();
        defer {
            // A selected waiter does not own the mutex yet. If it is hard-
            // killed before its retry, wakeOne would strand every remaining
            // waiter on a free mutex with no future unlock as wake source.
            // Drain the queue while the release publication is still covered
            // by the outer preemption critical section; losers retry/enrol
            // through waitUnless.
            if (wake_waiter) _ = self.queue.wakeAll();
            scheduler.preemptEnable();
        }

        if (self.owner != current_id or self.owner_generation != current_generation or self.depth == 0) {
            global_lock_summary.unlock_mismatches +%= 1;
            return false;
        }
        self.depth -= 1;
        global_lock_summary.releases +%= 1;
        if (self.depth == 0) {
            recordLockRelease(current_task, self.objectId());
            if (current_task.held_lock_count == 0) {
                // Preserve forward progress even after detecting corrupted
                // accounting: the exact matching owner must still release.
                global_lock_summary.unlock_mismatches +%= 1;
            } else {
                current_task.held_lock_count -= 1;
            }
            self.owner = 0;
            self.owner_generation = 0;
            wake_waiter = true;
        }
        return true;
    }

    fn objectId(self: *const Mutex) u64 {
        return @intFromPtr(self);
    }

    fn permanentAdmissionFailure(self: *const Mutex) bool {
        const current_task = scheduler.current() orelse return true;
        if (self.owner == 0 and self.owner_generation == 0) {
            return current_task.held_lock_count == 0xFFFF_FFFF;
        }
        return self.owner == current_task.id and
            self.owner_generation == current_task.generation and
            self.depth == 0xFFFF_FFFF;
    }

    fn stillOwned(raw: *anyopaque) bool {
        const self: *Mutex = @ptrCast(@alignCast(raw));
        return self.owner != 0 or self.owner_generation != 0;
    }
};

// A generation-safe recursive owner for invariants that intentionally span
// scheduler waits. Unlike Mutex it does not participate in lock-order or
// sleep-under-lock diagnostics. Its exact task-owned count exists solely to
// defer hard kill/reap until Zig defers can unwind the protected operation.
pub const UnwindGuard = struct {
    owner: u32 = 0,
    owner_generation: u64 = 0,
    boot_owner: bool = false,
    depth: u32 = 0,
    name: []const u8 = "unwind",
    queue: WaitQueue = WaitQueue.init(),

    pub fn init(name: []const u8) UnwindGuard {
        return .{ .name = name };
    }

    pub fn ownedByCurrent(self: *const UnwindGuard) bool {
        const current_task = scheduler.current() orelse return self.boot_owner and self.depth != 0;
        return self.owner == current_task.id and
            self.owner_generation == current_task.generation and
            self.depth != 0;
    }

    pub fn tryEnter(self: *UnwindGuard) bool {
        const current_task = scheduler.current() orelse return self.tryEnterBoot();
        scheduler.preemptDisable();
        defer scheduler.preemptEnable();

        if (self.isFree()) {
            if (current_task.unwind_guard_count == 0xFFFF_FFFF) return false;
            self.owner = current_task.id;
            self.owner_generation = current_task.generation;
            self.depth = 1;
            current_task.unwind_guard_count += 1;
            return true;
        }
        if (self.owner == current_task.id and self.owner_generation == current_task.generation) {
            if (self.depth == 0xFFFF_FFFF) return false;
            self.depth += 1;
            return true;
        }
        return false;
    }

    pub fn enter(self: *UnwindGuard, timeout_ticks: u64) bool {
        if (self.tryEnter()) return true;
        if (self.permanentAdmissionFailure() or timeout_ticks == 0) return false;
        const bounded = timeout_ticks != WAIT_FOREVER;
        const deadline = if (bounded) timer.tickCount() +% timeout_ticks else 0;
        var remaining = timeout_ticks;
        while (true) {
            const result = self.queue.waitUnless(remaining, self.name, stillOwned, self);
            if (result != .signaled) return false;
            if (self.tryEnter()) return true;
            if (self.permanentAdmissionFailure()) return false;
            if (bounded) {
                const now = timer.tickCount();
                if (now >= deadline) return false;
                remaining = deadline - now;
            }
        }
    }

    pub fn leave(self: *UnwindGuard) bool {
        const current_task = scheduler.current() orelse return self.leaveBoot();
        var wake_waiter = false;
        scheduler.preemptDisable();
        defer {
            // As with Mutex, a signalled task has not acquired the guard yet.
            // Wake every contender before publishing the end of the outer
            // critical section so killing one selected waiter cannot strand
            // the rest on an already free guard.
            if (wake_waiter) _ = self.queue.wakeAll();
            scheduler.preemptEnable();
        }

        if (self.owner != current_task.id or
            self.owner_generation != current_task.generation or
            self.depth == 0)
        {
            return false;
        }
        self.depth -= 1;
        if (self.depth == 0) {
            if (current_task.unwind_guard_count != 0) current_task.unwind_guard_count -= 1;
            self.owner = 0;
            self.owner_generation = 0;
            wake_waiter = true;
        }
        return true;
    }

    fn permanentAdmissionFailure(self: *const UnwindGuard) bool {
        const current_task = scheduler.current() orelse {
            return self.boot_owner and self.depth == 0xFFFF_FFFF;
        };
        if (self.isFree()) {
            return current_task.unwind_guard_count == 0xFFFF_FFFF;
        }
        return self.owner == current_task.id and
            self.owner_generation == current_task.generation and
            self.depth == 0xFFFF_FFFF;
    }

    fn stillOwned(raw: *anyopaque) bool {
        const self: *UnwindGuard = @ptrCast(@alignCast(raw));
        return !self.isFree();
    }

    fn isFree(self: *const UnwindGuard) bool {
        return self.owner == 0 and self.owner_generation == 0 and !self.boot_owner and self.depth == 0;
    }

    fn tryEnterBoot(self: *UnwindGuard) bool {
        if (self.isFree()) {
            self.boot_owner = true;
            self.depth = 1;
            return true;
        }
        if (self.boot_owner) {
            if (self.depth == 0xFFFF_FFFF) return false;
            self.depth += 1;
            return true;
        }
        return false;
    }

    fn leaveBoot(self: *UnwindGuard) bool {
        if (!self.boot_owner or self.depth == 0) return false;
        self.depth -= 1;
        if (self.depth == 0) {
            self.boot_owner = false;
            _ = self.queue.wakeAll();
        }
        return true;
    }
};

fn recordLockOrder(owner_task: *task.Task, object_id: u64, rank: u16) void {
    if (comptime !config.enable_metrics) return;
    var i: usize = 0;
    while (i < owner_task.held_locks.len) : (i += 1) {
        const held = owner_task.held_locks[i];
        if (!held.active or held.object_id == object_id) continue;
        if (held.rank > rank or held.rank == rank) {
            global_lock_summary.order_violations +%= 1;
            return;
        }
    }
}

fn recordLockAcquire(owner_task: *task.Task, object_id: u64, rank: u16, mode: LockMode) void {
    global_lock_summary.acquires +%= 1;
    if (comptime !config.enable_metrics) return;
    var free_slot: ?usize = null;
    var i: usize = 0;
    while (i < owner_task.held_locks.len) : (i += 1) {
        if (owner_task.held_locks[i].active and owner_task.held_locks[i].object_id == object_id) return;
        if (!owner_task.held_locks[i].active and free_slot == null) free_slot = i;
    }
    const slot = free_slot orelse {
        global_lock_summary.tracking_drops +%= 1;
        return;
    };
    owner_task.held_locks[slot] = .{
        .object_id = object_id,
        .rank = rank,
        .mode_no_sleep = mode == .no_sleep,
        .active = true,
    };
    const depth = heldDepthFor(owner_task);
    if (depth > global_lock_summary.max_depth) global_lock_summary.max_depth = depth;
}

fn recordLockRelease(owner_task: *task.Task, object_id: u64) void {
    if (comptime !config.enable_metrics) return;
    var i: usize = 0;
    while (i < owner_task.held_locks.len) : (i += 1) {
        if (owner_task.held_locks[i].active and owner_task.held_locks[i].object_id == object_id) {
            owner_task.held_locks[i] = .{};
            return;
        }
    }
    global_lock_summary.unlock_mismatches +%= 1;
}

fn heldDepthFor(owner_task: *const task.Task) u32 {
    var depth: u32 = 0;
    var i: usize = 0;
    while (i < owner_task.held_locks.len) : (i += 1) {
        if (owner_task.held_locks[i].active) depth += 1;
    }
    return depth;
}

fn countHeldSlots() u32 {
    const irq_flags = interrupts.saveAndDisable();
    scheduler.preemptDisable();
    defer {
        scheduler.preemptEnable();
        interrupts.restore(irq_flags);
    }
    var count: u32 = 0;
    var cursor = task.first();
    while (cursor) |candidate| : (cursor = task.next(candidate)) {
        var held_index: usize = 0;
        while (held_index < candidate.held_locks.len) : (held_index += 1) {
            if (candidate.held_locks[held_index].active) count +|= 1;
        }
    }
    return count;
}

// -----------------------------------------------------------------------------
// 0.56.13: Boot-Selbsttest der Korrektheitsfixe (ein COM1-Marker):
//   SYNCCHECK OK mutex_elapsed=<ticks> event_iters=<n>
//   SYNCCHECK FAIL reason=... (Gate-serial-markers schlagen an)
// Laeuft NACH initTaskRuntime (braucht Kernel-Threads + Scheduler).
//   (a) Mutex-Timeout unter Dauer-Contention: ein Holder-Task klaut den
//       Mutex nach jedem Unlock sofort zurueck; lock(50) muss trotzdem in
//       begrenzter Zeit aufgeben (Alt-Bug: jeder Wakeup-Retry bekam
//       wieder volle 50 Ticks -> unbegrenzt).
//   (b) Event-Ping-Pong (auto_reset, 200 Runden, beide Ordnungen
//       signal-vor-wait und wait-vor-signal): jede Runde muss .signaled
//       liefern (Lost-Wakeup wuerde als Timeout sichtbar).
// -----------------------------------------------------------------------------

const sync_log = @import("../kernel/log.zig");

var st_mutex: Mutex = .{};
var st_event: EventV2 = .{ .mode = .auto_reset };
var st_stop: bool = false;
var st_holder_done: bool = false;
var st_partner_done: bool = false;
var st_if_event: EventV2 = .{ .mode = .auto_reset };
var st_if_partner_done: bool = false;
const ST_EVENT_ITERS: u32 = 200;

fn stMutexHolderMain() callconv(.c) void {
    _ = st_mutex.lock(WAIT_FOREVER);
    while (!@as(*volatile bool, &st_stop).*) {
        _ = st_mutex.unlock();
        if (!st_mutex.tryLock()) {
            _ = st_mutex.lock(WAIT_FOREVER);
        }
        scheduler.yield();
    }
    _ = st_mutex.unlock();
    @as(*volatile bool, &st_holder_done).* = true;
    scheduler.exitCurrent();
}

fn stEventPartnerMain() callconv(.c) void {
    var i: u32 = 0;
    while (i < ST_EVENT_ITERS) : (i += 1) {
        st_event.signal();
        var spin: u32 = 0;
        while (@as(*volatile bool, &st_event.signaled).* and spin < 200) : (spin += 1) {
            scheduler.yield();
        }
        scheduler.yield();
    }
    @as(*volatile bool, &st_partner_done).* = true;
    scheduler.exitCurrent();
}

fn stIfEventPartnerMain() callconv(.c) void {
    scheduler.sleepTicksWithReason(2, "sync-st-if-signal");
    @as(*volatile bool, &st_if_partner_done).* = true;
    st_if_event.signal();
    scheduler.exitCurrent();
}

fn stSleepPreservesInterruptState(enabled: bool) bool {
    const original_flags = interrupts.saveAndDisable();
    if (enabled) interrupts.enable();
    scheduler.sleepTicksWithReason(1, if (enabled) "sync-st-if-sleep-on" else "sync-st-if-sleep-off");
    const after_flags = interrupts.saveAndDisable();
    interrupts.restore(original_flags);
    return interrupts.wereEnabled(after_flags) == enabled;
}

fn stEventPreservesInterruptState(enabled: bool) bool {
    st_if_event = .{ .mode = .auto_reset };
    st_if_partner_done = false;
    if (task.createKernelThread("sync-st-if-evt", stIfEventPartnerMain) == null) return false;

    const original_flags = interrupts.saveAndDisable();
    if (enabled) interrupts.enable();
    const result = st_if_event.waitResult(200);
    const after_flags = interrupts.saveAndDisable();
    interrupts.restore(original_flags);
    return result == .signaled and
        @as(*volatile bool, &st_if_partner_done).* and
        interrupts.wereEnabled(after_flags) == enabled;
}

pub fn selfTest() bool {
    // (a) Mutex-Timeout bleibt unter Dauer-Contention begrenzt.
    st_stop = false;
    st_holder_done = false;
    if (task.createKernelThread("sync-st-hold", stMutexHolderMain) == null) {
        return selfTestFail("holder-spawn");
    }
    scheduler.sleepTicksWithReason(3, "sync-st-warm");
    const t0 = timer.tickCount();
    const got = st_mutex.lock(50);
    const mutex_elapsed = timer.tickCount() -% t0;
    if (got) _ = st_mutex.unlock();
    @as(*volatile bool, &st_stop).* = true;
    var guard: u32 = 0;
    while (!@as(*volatile bool, &st_holder_done).* and guard < 2000) : (guard += 1) {
        scheduler.sleepTicksWithReason(1, "sync-st-join");
    }
    if (guard >= 2000) return selfTestFail("holder-join");
    // Budget 50 + grosszuegige Toleranz fuer Scheduling; der Alt-Bug lag
    // um Groessenordnungen darueber (jede Weckung = neues Vollbudget).
    if (mutex_elapsed > 200) return selfTestFail("mutex-unbounded");

    // (b) Event-Ping-Pong ohne verlorene Signale.
    st_event = .{ .mode = .auto_reset };
    st_partner_done = false;
    if (task.createKernelThread("sync-st-evt", stEventPartnerMain) == null) {
        return selfTestFail("partner-spawn");
    }
    var i: u32 = 0;
    while (i < ST_EVENT_ITERS) : (i += 1) {
        const r = st_event.waitResult(200);
        if (r != .signaled) return selfTestFail("event-lost");
    }
    guard = 0;
    while (!@as(*volatile bool, &st_partner_done).* and guard < 2000) : (guard += 1) {
        scheduler.sleepTicksWithReason(1, "sync-st-join2");
    }
    if (guard >= 2000) return selfTestFail("partner-join");

    // (c) Every blocking path must return with the exact caller IF state.
    // The IF-off event probe is the regression for the storage worker:
    // parkBlocked may borrow interrupts for HLT, but cannot leak its CLI.
    if (!stSleepPreservesInterruptState(true)) return selfTestFail("if-sleep-on");
    if (!stSleepPreservesInterruptState(false)) return selfTestFail("if-sleep-off");
    if (!stEventPreservesInterruptState(true)) return selfTestFail("if-event-on");
    if (!stEventPreservesInterruptState(false)) return selfTestFail("if-event-off");

    sync_log.puts("SYNCCHECK OK mutex_elapsed=");
    sync_log.putDec(mutex_elapsed);
    sync_log.puts(" event_iters=");
    sync_log.putDec(ST_EVENT_ITERS);
    sync_log.puts(" if_preserve=on/off");
    sync_log.puts("\r\n");
    return true;
}

fn selfTestFail(reason: []const u8) bool {
    sync_log.puts("SYNCCHECK FAIL reason=");
    sync_log.puts(reason);
    sync_log.puts("\r\n");
    return false;
}
