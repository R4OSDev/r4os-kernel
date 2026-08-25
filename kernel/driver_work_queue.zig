const std = @import("std");

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

pub const OwnerBookkeeping = struct {
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

    pub fn reserve(self: *OwnerBookkeeping, source: SourceClass) void {
        self.used +|= 1;
        self.queued +|= 1;
        switch (source) {
            .irq => self.irq_queued +|= 1,
            .task => self.task_queued +|= 1,
            .deadline => self.deadline_queued +|= 1,
        }
        if (self.deadline_queued > self.deadline_queue_high_water) {
            self.deadline_queue_high_water = self.deadline_queued;
        }
        self.noteHighWater();
    }

    pub fn start(self: *OwnerBookkeeping, source: SourceClass) void {
        if (self.queued != 0) self.queued -= 1;
        self.running +|= 1;
        if (source == .deadline) self.deadline_running +|= 1;
        switch (source) {
            .irq => if (self.irq_queued != 0) {
                self.irq_queued -= 1;
            },
            .task => if (self.task_queued != 0) {
                self.task_queued -= 1;
            },
            .deadline => if (self.deadline_queued != 0) {
                self.deadline_queued -= 1;
            },
        }
    }

    pub fn cancel(self: *OwnerBookkeeping, source: SourceClass) void {
        if (self.queued != 0) self.queued -= 1;
        self.cancelled +|= 1;
        switch (source) {
            .irq => if (self.irq_queued != 0) {
                self.irq_queued -= 1;
            },
            .task => if (self.task_queued != 0) {
                self.task_queued -= 1;
            },
            .deadline => if (self.deadline_queued != 0) {
                self.deadline_queued -= 1;
            },
        }
        self.noteHighWater();
    }

    pub fn complete(self: *OwnerBookkeeping, source: SourceClass) void {
        if (self.running != 0) self.running -= 1;
        if (source == .deadline and self.deadline_running != 0) self.deadline_running -= 1;
        self.completed +|= 1;
        self.noteHighWater();
    }

    pub fn release(self: *OwnerBookkeeping, state: State) void {
        if (self.used != 0) self.used -= 1;
        switch (state) {
            .completed => if (self.completed != 0) {
                self.completed -= 1;
            },
            .cancelled => if (self.cancelled != 0) {
                self.cancelled -= 1;
            },
            else => {},
        }
    }

    pub fn addWaiter(self: *OwnerBookkeeping) void {
        self.waiters +|= 1;
        if (self.waiters > self.waiters_max) self.waiters_max = self.waiters;
    }

    pub fn removeWaiter(self: *OwnerBookkeeping) void {
        if (self.waiters != 0) self.waiters -= 1;
    }

    pub fn retainedCount(self: *const OwnerBookkeeping) u32 {
        return self.completed + self.cancelled;
    }

    fn noteHighWater(self: *OwnerBookkeeping) void {
        if (self.used > self.used_high_water) self.used_high_water = self.used;
        const retained = self.retainedCount();
        if (retained > self.retained_high_water) self.retained_high_water = retained;
    }
};

pub const Bookkeeping = struct {
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

    pub fn reserveAndEnqueue(self: *Bookkeeping, source: SourceClass) ?usize {
        if (self.free_mask == 0) return null;
        if (source == .deadline) {
            if (self.deadline_queued_count >= deadline_queue_capacity) return null;
        } else if (self.free_count <= deadline_reserved_capacity) {
            return null;
        }
        const slot: usize = @intCast(@ctz(self.free_mask));
        self.free_mask &= ~slotBit(slot);
        self.free_count -= 1;
        self.states[slot] = .queued;
        self.classes[slot] = source;
        self.queued_count += 1;
        switch (source) {
            .irq => self.irq_queued_count += 1,
            .task => self.task_queued_count += 1,
            .deadline => self.deadline_queued_count += 1,
        }
        self.appendQueue(slot, source);
        self.noteHighWater();
        return slot;
    }

    pub fn takeNext(self: *Bookkeeping) ?Selection {
        return self.takeNextNormal();
    }

    pub fn takeNextNormal(self: *Bookkeeping) ?Selection {
        const has_irq = self.irq_head != no_slot;
        const has_task = self.task_head != no_slot;
        if (!has_irq and !has_task) return null;

        var selection = Selection{ .slot = 0, .source = .task };
        if (has_irq and (!has_task or self.current_irq_burst < irq_burst_limit)) {
            selection.slot = self.irq_head;
            selection.source = .irq;
            selection.irq_preferred = has_task;
            self.current_irq_burst +|= 1;
        } else {
            selection.slot = self.task_head;
            selection.source = .task;
            selection.task_fairness = has_irq;
            self.current_irq_burst = 0;
        }

        self.startSelection(selection);
        return selection;
    }

    pub fn takeNextDeadline(self: *Bookkeeping, deadlines: []const u64) ?Selection {
        if (self.deadline_head == no_slot or deadlines.len < capacity) return null;
        var best = self.deadline_head;
        var cursor = self.queue_next[best];
        while (cursor != no_slot) : (cursor = self.queue_next[cursor]) {
            if (deadlines[cursor] < deadlines[best]) best = cursor;
        }
        const selection = Selection{ .slot = best, .source = .deadline };
        self.startSelection(selection);
        return selection;
    }

    pub fn cancelQueued(self: *Bookkeeping, slot: usize) bool {
        if (slot >= capacity or self.states[slot] != .queued) return false;
        const source = self.classes[slot];
        self.removeQueue(slot);
        self.states[slot] = .cancelled;
        self.queued_count -= 1;
        self.cancelled_count += 1;
        switch (source) {
            .irq => self.irq_queued_count -= 1,
            .task => self.task_queued_count -= 1,
            .deadline => self.deadline_queued_count -= 1,
        }
        self.appendFinal(slot);
        self.noteHighWater();
        return true;
    }

    pub fn completeRunning(self: *Bookkeeping, slot: usize) bool {
        if (slot >= capacity or self.states[slot] != .running) return false;
        const source = self.classes[slot];
        self.states[slot] = .completed;
        self.running_count -= 1;
        if (source == .deadline and self.deadline_running_count != 0) self.deadline_running_count -= 1;
        self.completed_count += 1;
        self.appendFinal(slot);
        self.noteHighWater();
        return true;
    }

    pub fn releaseFinal(self: *Bookkeeping, slot: usize) bool {
        if (slot >= capacity) return false;
        switch (self.states[slot]) {
            .completed => self.completed_count -= 1,
            .cancelled => self.cancelled_count -= 1,
            else => return false,
        }
        self.removeFinal(slot);
        self.states[slot] = .free;
        self.classes[slot] = .task;
        self.free_mask |= slotBit(slot);
        self.free_count += 1;
        return true;
    }

    pub fn state(self: *const Bookkeeping, slot: usize) State {
        if (slot >= capacity) return .free;
        return self.states[slot];
    }

    pub fn sourceClass(self: *const Bookkeeping, slot: usize) SourceClass {
        if (slot >= capacity) return .task;
        return self.classes[slot];
    }

    pub fn usedCount(self: *const Bookkeeping) u32 {
        return @as(u32, @intCast(capacity)) - self.free_count;
    }

    pub fn retainedCount(self: *const Bookkeeping) u32 {
        return self.completed_count + self.cancelled_count;
    }

    pub fn oldestFinal(self: *const Bookkeeping) ?usize {
        if (self.final_head == no_slot) return null;
        return self.final_head;
    }

    fn appendQueue(self: *Bookkeeping, slot: usize, source: SourceClass) void {
        const slot_u8: u8 = @intCast(slot);
        self.queue_prev[slot] = no_slot;
        self.queue_next[slot] = no_slot;
        const tail = switch (source) {
            .irq => self.irq_tail,
            .task => self.task_tail,
            .deadline => self.deadline_tail,
        };
        if (tail == no_slot) {
            switch (source) {
                .irq => self.irq_head = slot_u8,
                .task => self.task_head = slot_u8,
                .deadline => self.deadline_head = slot_u8,
            }
        } else {
            self.queue_next[tail] = slot_u8;
            self.queue_prev[slot] = tail;
        }
        switch (source) {
            .irq => self.irq_tail = slot_u8,
            .task => self.task_tail = slot_u8,
            .deadline => self.deadline_tail = slot_u8,
        }
    }

    fn removeQueue(self: *Bookkeeping, slot: usize) void {
        const source = self.classes[slot];
        const prev = self.queue_prev[slot];
        const next = self.queue_next[slot];
        if (prev == no_slot) {
            switch (source) {
                .irq => self.irq_head = next,
                .task => self.task_head = next,
                .deadline => self.deadline_head = next,
            }
        } else {
            self.queue_next[prev] = next;
        }
        if (next == no_slot) {
            switch (source) {
                .irq => self.irq_tail = prev,
                .task => self.task_tail = prev,
                .deadline => self.deadline_tail = prev,
            }
        } else {
            self.queue_prev[next] = prev;
        }
        self.queue_prev[slot] = no_slot;
        self.queue_next[slot] = no_slot;
    }

    fn appendFinal(self: *Bookkeeping, slot: usize) void {
        const slot_u8: u8 = @intCast(slot);
        self.final_prev[slot] = self.final_tail;
        self.final_next[slot] = no_slot;
        if (self.final_tail == no_slot) {
            self.final_head = slot_u8;
        } else {
            self.final_next[self.final_tail] = slot_u8;
        }
        self.final_tail = slot_u8;
    }

    fn removeFinal(self: *Bookkeeping, slot: usize) void {
        const prev = self.final_prev[slot];
        const next = self.final_next[slot];
        if (prev == no_slot) {
            self.final_head = next;
        } else {
            self.final_next[prev] = next;
        }
        if (next == no_slot) {
            self.final_tail = prev;
        } else {
            self.final_prev[next] = prev;
        }
        self.final_prev[slot] = no_slot;
        self.final_next[slot] = no_slot;
    }

    fn noteHighWater(self: *Bookkeeping) void {
        if (self.queued_count > self.queue_high_water) self.queue_high_water = self.queued_count;
        const used = self.usedCount();
        if (used > self.used_high_water) self.used_high_water = used;
        const retained = self.retainedCount();
        if (retained > self.retained_high_water) self.retained_high_water = retained;
        if (self.deadline_queued_count > self.deadline_queue_high_water) {
            self.deadline_queue_high_water = self.deadline_queued_count;
        }
    }

    fn startSelection(self: *Bookkeeping, selection: Selection) void {
        self.removeQueue(selection.slot);
        self.states[selection.slot] = .running;
        self.queued_count -= 1;
        self.running_count += 1;
        if (selection.source == .deadline) self.deadline_running_count += 1;
        switch (selection.source) {
            .irq => self.irq_queued_count -= 1,
            .task => self.task_queued_count -= 1,
            .deadline => self.deadline_queued_count -= 1,
        }
    }
};

pub fn releaseDecision(state: State, published: bool, waiters: u32, claimed: bool) ReleaseDecision {
    return switch (state) {
        .free => .invalid,
        .queued, .running => .busy,
        .completed, .cancelled => if (!published)
            .publication_pending
        else if (waiters != 0)
            .waiters_present
        else if (claimed)
            .already_claimed
        else
            .claim,
    };
}

pub fn cleanupDecision(state: State, published: bool, waiters: u32) CleanupDecision {
    return switch (state) {
        .free => .skip,
        .queued => .cancel_queued,
        .running => .wait_running,
        .completed, .cancelled => if (!published)
            .wait_publication
        else if (waiters != 0)
            .wait_waiters
        else
            .release_final,
    };
}

pub fn makeHandle(slot: usize, generation: u32) u32 {
    return ((generation & 0x00FF_FFFF) << 8) | @as(u32, @intCast(slot + 1));
}

pub fn slotFromHandle(handle: u32) ?usize {
    const slot_code = handle & 0xFF;
    if (slot_code == 0 or slot_code > capacity) return null;
    return @intCast(slot_code - 1);
}

pub fn nextGeneration(current: u32) u32 {
    var next = (current + 1) & 0x00FF_FFFF;
    if (next == 0) next = 1;
    return next;
}

fn slotBit(slot: usize) u16 {
    return @as(u16, 1) << @intCast(slot);
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
