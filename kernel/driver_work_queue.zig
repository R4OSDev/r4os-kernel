const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const capacity: usize = 16;
pub const no_slot: u8 = 0xFF;
pub const irq_burst_limit: u32 = 4;
pub const deadline_queue_capacity: u32 = 4;
pub const deadline_reserved_capacity: u32 = 2;

pub const State = enum(u8) {
    free,
    queued,
    running,
    completed,
    cancelled,
};

pub const SourceClass = enum(u8) {
    task,
    irq,
    deadline,
};

pub const Selection = struct {
    slot: usize,
    source: SourceClass,
    irq_preferred: bool = false,
    task_fairness: bool = false,
};

pub const ReleaseDecision = enum {
    invalid,
    busy,
    publication_pending,
    waiters_present,
    already_claimed,
    claim,
};

pub const CleanupDecision = enum {
    skip,
    cancel_queued,
    wait_running,
    wait_publication,
    wait_waiters,
    release_final,
};

// Fixed private C records; the native policy owns every queue/ledger mutation.
// Work admission, runtime serialization and actual completion stay with Work.
const RawSelection = extern struct { slot: usize, source: SourceClass, irq_preferred: bool, task_fairness: bool };
extern fn r4os_rust_work_owner_ledger(book: *OwnerBookkeeping, operation: u8, value: u8) u32;
extern fn r4os_rust_work_owner_retained(book: *const OwnerBookkeeping) u32;
extern fn r4os_rust_work_queue(book: *Bookkeeping, operation: u8, value: usize) usize;
extern fn r4os_rust_work_queue_query(book: *const Bookkeeping, operation: u8, slot: usize) usize;
extern fn r4os_rust_work_queue_select(book: *Bookkeeping, deadline_lane: bool, deadlines: ?[*]const u64, len: usize, output: *RawSelection) bool;
extern fn r4os_rust_work_release_decision(state: u8, published: bool, waiters: u32, claimed: bool) u8;
extern fn r4os_rust_work_cleanup_decision(state: u8, published: bool, waiters: u32) u8;
extern fn r4os_rust_work_handle(operation: u8, slot_or_handle: usize, generation: u32) usize;
comptime {
    if (@sizeOf(OwnerBookkeeping) != 56 or @sizeOf(Bookkeeping) != 164 or @offsetOf(Bookkeeping, "free_count") != 108 or
        @sizeOf(RawSelection) != 16 or @offsetOf(RawSelection, "source") != 8)
        @compileError("Native Work queue C layout drift");
}
pub const OwnerBookkeeping = extern struct {
    used: u32 = 0,
    queued: u32 = 0,
    running: u32 = 0,
    deadline_running: u32 = 0,
    completed: u32 = 0,
    cancelled: u32 = 0,
    irq_queued: u32 = 0,
    task_queued: u32 = 0,
    deadline_queued: u32 = 0,
    waiters: u32 = 0,
    waiters_max: u32 = 0,
    used_high_water: u32 = 0,
    retained_high_water: u32 = 0,
    deadline_queue_high_water: u32 = 0,

    pub fn reserve(self: *OwnerBookkeeping, value: SourceClass) void {
        _ = r4os_rust_work_owner_ledger(self, 0, @intFromEnum(value));
    }
    pub fn start(self: *OwnerBookkeeping, value: SourceClass) void {
        _ = r4os_rust_work_owner_ledger(self, 1, @intFromEnum(value));
    }
    pub fn cancel(self: *OwnerBookkeeping, value: SourceClass) void {
        _ = r4os_rust_work_owner_ledger(self, 2, @intFromEnum(value));
    }
    pub fn complete(self: *OwnerBookkeeping, value: SourceClass) void {
        _ = r4os_rust_work_owner_ledger(self, 3, @intFromEnum(value));
    }
    pub fn release(self: *OwnerBookkeeping, value: State) void {
        _ = r4os_rust_work_owner_ledger(self, 4, @intFromEnum(value));
    }
    pub fn addWaiter(self: *OwnerBookkeeping) void {
        _ = r4os_rust_work_owner_ledger(self, 5, 0);
    }
    pub fn removeWaiter(self: *OwnerBookkeeping) void {
        _ = r4os_rust_work_owner_ledger(self, 6, 0);
    }
    pub fn retainedCount(self: *const OwnerBookkeeping) u32 {
        return r4os_rust_work_owner_retained(self);
    }
};
pub const Bookkeeping = extern struct {
    states: [capacity]State = [_]State{.free} ** capacity,
    classes: [capacity]SourceClass = [_]SourceClass{.task} ** capacity,
    queue_prev: [capacity]u8 = [_]u8{no_slot} ** capacity,
    queue_next: [capacity]u8 = [_]u8{no_slot} ** capacity,
    final_prev: [capacity]u8 = [_]u8{no_slot} ** capacity,
    final_next: [capacity]u8 = [_]u8{no_slot} ** capacity,
    irq_head: u8 = no_slot,
    irq_tail: u8 = no_slot,
    task_head: u8 = no_slot,
    task_tail: u8 = no_slot,
    deadline_head: u8 = no_slot,
    deadline_tail: u8 = no_slot,
    final_head: u8 = no_slot,
    final_tail: u8 = no_slot,
    free_mask: u16 = 0xFFFF,
    free_count: u32 = capacity,
    queued_count: u32 = 0,
    running_count: u32 = 0,
    deadline_running_count: u32 = 0,
    completed_count: u32 = 0,
    cancelled_count: u32 = 0,
    irq_queued_count: u32 = 0,
    task_queued_count: u32 = 0,
    deadline_queued_count: u32 = 0,
    queue_high_water: u32 = 0,
    used_high_water: u32 = 0,
    retained_high_water: u32 = 0,
    deadline_queue_high_water: u32 = 0,
    current_irq_burst: u32 = 0,

    pub fn reserveAndEnqueue(self: *Bookkeeping, value: SourceClass) ?usize {
        const slot = r4os_rust_work_queue(self, 0, @intFromEnum(value));
        return if (slot == std.math.maxInt(usize)) null else slot;
    }
    pub fn takeNext(self: *Bookkeeping) ?Selection {
        return self.takeNextNormal();
    }
    pub fn takeNextNormal(self: *Bookkeeping) ?Selection {
        var value: RawSelection = undefined;
        if (!r4os_rust_work_queue_select(self, false, null, 0, &value)) return null;
        return .{ .slot = value.slot, .source = value.source, .irq_preferred = value.irq_preferred, .task_fairness = value.task_fairness };
    }
    pub fn takeNextDeadline(self: *Bookkeeping, deadlines: []const u64) ?Selection {
        var value: RawSelection = undefined;
        if (!r4os_rust_work_queue_select(self, true, deadlines.ptr, deadlines.len, &value)) return null;
        return .{ .slot = value.slot, .source = value.source, .irq_preferred = value.irq_preferred, .task_fairness = value.task_fairness };
    }
    pub fn cancelQueued(self: *Bookkeeping, slot: usize) bool {
        return r4os_rust_work_queue(self, 1, slot) != 0;
    }
    pub fn completeRunning(self: *Bookkeeping, slot: usize) bool {
        return r4os_rust_work_queue(self, 2, slot) != 0;
    }
    pub fn releaseFinal(self: *Bookkeeping, slot: usize) bool {
        return r4os_rust_work_queue(self, 3, slot) != 0;
    }
    pub fn state(self: *const Bookkeeping, slot: usize) State {
        return @enumFromInt(r4os_rust_work_queue_query(self, 4, slot));
    }
    pub fn sourceClass(self: *const Bookkeeping, slot: usize) SourceClass {
        return @enumFromInt(r4os_rust_work_queue_query(self, 5, slot));
    }
    pub fn usedCount(self: *const Bookkeeping) u32 {
        return @intCast(r4os_rust_work_queue_query(self, 6, 0));
    }
    pub fn retainedCount(self: *const Bookkeeping) u32 {
        return @intCast(r4os_rust_work_queue_query(self, 7, 0));
    }
    pub fn oldestFinal(self: *const Bookkeeping) ?usize {
        const slot = r4os_rust_work_queue_query(self, 8, 0);
        return if (slot == std.math.maxInt(usize)) null else slot;
    }
};
pub fn releaseDecision(state: State, published: bool, waiters: u32, claimed: bool) ReleaseDecision {
    return @enumFromInt(r4os_rust_work_release_decision(@intFromEnum(state), published, waiters, claimed));
}
pub fn cleanupDecision(state: State, published: bool, waiters: u32) CleanupDecision {
    return @enumFromInt(r4os_rust_work_cleanup_decision(@intFromEnum(state), published, waiters));
}
pub fn makeHandle(slot: usize, generation: u32) u32 {
    return @intCast(r4os_rust_work_handle(0, slot, generation));
}
pub fn slotFromHandle(handle: u32) ?usize {
    const slot = r4os_rust_work_handle(1, handle, 0);
    return if (slot == std.math.maxInt(usize)) null else slot;
}
pub fn nextGeneration(current: u32) u32 {
    return @intCast(r4os_rust_work_handle(2, 0, current));
}

test "retained completions consume visible used capacity until release" {
    var book = Bookkeeping{};
    var slot: usize = 0;
    while (slot < capacity - deadline_reserved_capacity) : (slot += 1) {
        try std.testing.expectEqual(slot, book.reserveAndEnqueue(.task).?);
        const selected = book.takeNext().?;
        try std.testing.expectEqual(slot, selected.slot);
        try std.testing.expect(book.completeRunning(slot));
    }
    var deadlines = [_]u64{0} ** capacity;
    while (slot < capacity) : (slot += 1) {
        try std.testing.expectEqual(slot, book.reserveAndEnqueue(.deadline).?);
        deadlines[slot] = @intCast(slot + 1);
        const selected = book.takeNextDeadline(deadlines[0..]).?;
        try std.testing.expectEqual(slot, selected.slot);
        try std.testing.expect(book.completeRunning(slot));
    }
    try std.testing.expectEqual(@as(u32, 0), book.free_count);
    try std.testing.expectEqual(@as(u32, capacity), book.usedCount());
    try std.testing.expectEqual(@as(u32, 0), book.queued_count);
    try std.testing.expectEqual(@as(u32, capacity), book.completed_count);
    try std.testing.expect(book.reserveAndEnqueue(.task) == null);
    try std.testing.expect(book.releaseFinal(0));
    try std.testing.expectEqual(@as(u32, 1), book.free_count);
    try std.testing.expectEqual(@as(usize, 0), book.reserveAndEnqueue(.deadline).?);
}

test "IRQ queue is preferred but bounded by one task fairness selection" {
    var book = Bookkeeping{};
    const task_slot = book.reserveAndEnqueue(.task).?;
    var irq_slots: [5]usize = undefined;
    for (&irq_slots) |*entry| entry.* = book.reserveAndEnqueue(.irq).?;

    var index: usize = 0;
    while (index < irq_burst_limit) : (index += 1) {
        const selected = book.takeNext().?;
        try std.testing.expectEqual(irq_slots[index], selected.slot);
        try std.testing.expectEqual(SourceClass.irq, selected.source);
        try std.testing.expect(selected.irq_preferred);
        try std.testing.expect(book.completeRunning(selected.slot));
    }
    const fairness = book.takeNext().?;
    try std.testing.expectEqual(task_slot, fairness.slot);
    try std.testing.expect(fairness.task_fairness);
    try std.testing.expect(book.completeRunning(fairness.slot));
    const final_irq = book.takeNext().?;
    try std.testing.expectEqual(irq_slots[4], final_irq.slot);
}

test "cancel removes an arbitrary queued slot and preserves both FIFO chains" {
    var book = Bookkeeping{};
    const first = book.reserveAndEnqueue(.task).?;
    const cancelled = book.reserveAndEnqueue(.task).?;
    const third = book.reserveAndEnqueue(.task).?;
    try std.testing.expect(book.cancelQueued(cancelled));
    try std.testing.expectEqual(State.cancelled, book.state(cancelled));
    try std.testing.expectEqual(cancelled, book.oldestFinal().?);
    try std.testing.expectEqual(first, book.takeNext().?.slot);
    try std.testing.expect(book.completeRunning(first));
    try std.testing.expectEqual(third, book.takeNext().?.slot);
    try std.testing.expect(book.releaseFinal(cancelled));
    try std.testing.expectEqual(first, book.oldestFinal().?);
}

test "completion final list remains ordered across cancel complete and release" {
    var book = Bookkeeping{};
    const cancelled = book.reserveAndEnqueue(.irq).?;
    const completed = book.reserveAndEnqueue(.task).?;
    try std.testing.expect(book.cancelQueued(cancelled));
    try std.testing.expectEqual(completed, book.takeNext().?.slot);
    try std.testing.expect(book.completeRunning(completed));
    try std.testing.expectEqual(cancelled, book.oldestFinal().?);
    try std.testing.expect(book.releaseFinal(cancelled));
    try std.testing.expectEqual(completed, book.oldestFinal().?);
    try std.testing.expect(book.releaseFinal(completed));
    try std.testing.expect(book.oldestFinal() == null);
}

test "generation handles reject stale slot identity after reuse" {
    const old_handle = makeHandle(3, 7);
    const new_handle = makeHandle(3, nextGeneration(7));
    try std.testing.expectEqual(@as(usize, 3), slotFromHandle(old_handle).?);
    try std.testing.expectEqual(@as(usize, 3), slotFromHandle(new_handle).?);
    try std.testing.expect(old_handle != new_handle);
    try std.testing.expect(slotFromHandle(0) == null);
    try std.testing.expectEqual(@as(u32, 1), nextGeneration(0x00FF_FFFF));
}

test "release waits for publication and every enrolled waiter" {
    try std.testing.expectEqual(ReleaseDecision.busy, releaseDecision(.running, false, 0, false));
    try std.testing.expectEqual(ReleaseDecision.publication_pending, releaseDecision(.completed, false, 0, false));
    try std.testing.expectEqual(ReleaseDecision.waiters_present, releaseDecision(.completed, true, 2, false));
    try std.testing.expectEqual(ReleaseDecision.claim, releaseDecision(.cancelled, true, 0, false));
    try std.testing.expectEqual(ReleaseDecision.already_claimed, releaseDecision(.cancelled, true, 0, true));
}

test "cleanup cancels queued work and joins running work before release" {
    try std.testing.expectEqual(CleanupDecision.cancel_queued, cleanupDecision(.queued, false, 0));
    try std.testing.expectEqual(CleanupDecision.wait_running, cleanupDecision(.running, false, 0));
    try std.testing.expectEqual(CleanupDecision.wait_publication, cleanupDecision(.completed, false, 0));
    try std.testing.expectEqual(CleanupDecision.wait_waiters, cleanupDecision(.completed, true, 1));
    try std.testing.expectEqual(CleanupDecision.release_final, cleanupDecision(.cancelled, true, 0));
}

test "owner ledgers stay isolated across error completion cancel and multiple waiters" {
    var owners = [_]OwnerBookkeeping{ .{}, .{} };

    owners[0].reserve(.irq);
    owners[1].reserve(.task);
    owners[1].reserve(.task);
    owners[0].start(.irq);
    const handler_result: i32 = -42;
    try std.testing.expect(handler_result != 0);
    owners[0].complete(.irq);
    owners[1].cancel(.task);
    owners[1].start(.task);
    owners[1].complete(.task);
    owners[0].addWaiter();
    owners[0].addWaiter();

    try std.testing.expectEqual(@as(u32, 1), owners[0].used);
    try std.testing.expectEqual(@as(u32, 1), owners[0].completed);
    try std.testing.expectEqual(@as(u32, 2), owners[0].waiters);
    try std.testing.expectEqual(@as(u32, 2), owners[0].waiters_max);
    try std.testing.expectEqual(@as(u32, 2), owners[1].used);
    try std.testing.expectEqual(@as(u32, 1), owners[1].completed);
    try std.testing.expectEqual(@as(u32, 1), owners[1].cancelled);

    owners[0].removeWaiter();
    owners[0].removeWaiter();
    owners[0].release(.completed);
    owners[1].release(.cancelled);
    owners[1].release(.completed);
    try std.testing.expectEqual(@as(u32, 0), owners[0].used);
    try std.testing.expectEqual(@as(u32, 0), owners[0].waiters);
    try std.testing.expectEqual(@as(u32, 0), owners[1].used);
    try std.testing.expectEqual(@as(u32, 2), owners[1].used_high_water);
    try std.testing.expectEqual(@as(u32, 2), owners[1].retained_high_water);
}

test "normal admission leaves reserved capacity for deadline work" {
    var book = Bookkeeping{};
    var normal_count: u32 = 0;
    while (book.reserveAndEnqueue(.task) != null) normal_count += 1;
    try std.testing.expectEqual(@as(u32, capacity - deadline_reserved_capacity), normal_count);
    try std.testing.expectEqual(deadline_reserved_capacity, book.free_count);
    try std.testing.expect(book.reserveAndEnqueue(.deadline) != null);
    try std.testing.expect(book.reserveAndEnqueue(.deadline) != null);
    try std.testing.expectEqual(@as(u32, 0), book.free_count);
}

test "deadline lane selects earliest due item without entering normal FIFO" {
    var book = Bookkeeping{};
    const normal = book.reserveAndEnqueue(.irq).?;
    const late = book.reserveAndEnqueue(.deadline).?;
    const early = book.reserveAndEnqueue(.deadline).?;
    const tied = book.reserveAndEnqueue(.deadline).?;
    var deadlines = [_]u64{0} ** capacity;
    deadlines[late] = 30;
    deadlines[early] = 10;
    deadlines[tied] = 10;

    try std.testing.expectEqual(normal, book.takeNextNormal().?.slot);
    try std.testing.expectEqual(early, book.takeNextDeadline(deadlines[0..]).?.slot);
    try std.testing.expectEqual(tied, book.takeNextDeadline(deadlines[0..]).?.slot);
    try std.testing.expectEqual(late, book.takeNextDeadline(deadlines[0..]).?.slot);
    try std.testing.expect(book.takeNextDeadline(deadlines[0..]) == null);
}

test "deadline admission is bounded independently from free normal slots" {
    var book = Bookkeeping{};
    var index: u32 = 0;
    while (index < deadline_queue_capacity) : (index += 1) {
        try std.testing.expect(book.reserveAndEnqueue(.deadline) != null);
    }
    try std.testing.expect(book.reserveAndEnqueue(.deadline) == null);
    try std.testing.expect(book.free_count > deadline_reserved_capacity);
    try std.testing.expectEqual(deadline_queue_capacity, book.deadline_queue_high_water);
}
