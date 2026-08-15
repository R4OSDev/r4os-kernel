pub const CAPACITY: usize = 256;

pub const TRANSFER_EVENT_TYPE: u8 = 32;
pub const COMMAND_COMPLETION_EVENT_TYPE: u8 = 33;
pub const EVENT_DATA_BIT: u32 = 1 << 2;
pub const MAX_TRANSFER_TRB_POINTERS: usize = 3;

const TRB_POINTER_MASK: u64 = ~@as(u64, 0x0f);

pub const Event = struct {
    event_type: u8 = 0,
    code: u8 = 0,
    slot_id: u8 = 0,
    endpoint_id: u8 = 0,
    parameter: u64 = 0,
    length: u32 = 0,
    control: u32 = 0,

    pub fn hasEventData(self: Event) bool {
        return self.event_type == TRANSFER_EVENT_TYPE and (self.control & EVENT_DATA_BIT) != 0;
    }

    pub fn trbPointer(self: Event) ?u64 {
        if (self.hasEventData()) return null;
        if (self.event_type != TRANSFER_EVENT_TYPE and self.event_type != COMMAND_COMPLETION_EVENT_TYPE) return null;
        return normalizeTrbPointer(self.parameter);
    }
};

pub const TransferMatch = struct {
    slot_id: u8,
    endpoint_id: u8,
    trb_phys: [MAX_TRANSFER_TRB_POINTERS]u64,
    trb_count: u8,
};

pub const CommandMatch = struct {
    trb_phys: u64,
};

pub const Match = union(enum) {
    transfer: TransferMatch,
    command: CommandMatch,
};

pub const EnqueueResult = enum {
    queued,
    overflow,
};

pub const Snapshot = struct {
    pending: usize = 0,
    queued: u64 = 0,
    delivered: u64 = 0,
    overflows: u64 = 0,
    purged: u64 = 0,
    high_water: usize = 0,
};

pub const Mailbox = struct {
    events: [CAPACITY]Event = .{Event{}} ** CAPACITY,
    count: usize = 0,
    queued: u64 = 0,
    delivered: u64 = 0,
    overflows: u64 = 0,
    purged: u64 = 0,
    high_water: usize = 0,

    pub fn init() Mailbox {
        return .{};
    }

    pub fn reset(self: *Mailbox) void {
        self.* = .{};
    }

    pub fn enqueue(self: *Mailbox, event: Event) EnqueueResult {
        if (self.count >= CAPACITY) {
            self.overflows += 1;
            return .overflow;
        }

        self.events[self.count] = event;
        self.count += 1;
        self.queued += 1;
        if (self.count > self.high_water) self.high_water = self.count;
        return .queued;
    }

    pub fn take(self: *Mailbox, expected: Match) ?Event {
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            if (!matches(self.events[index], expected)) continue;
            const event = self.removeAt(index);
            self.delivered += 1;
            return event;
        }
        return null;
    }

    pub fn purge(self: *Mailbox, expected: Match) usize {
        var removed: usize = 0;
        var index: usize = 0;
        while (index < self.count) {
            if (!matches(self.events[index], expected)) {
                index += 1;
                continue;
            }
            _ = self.removeAt(index);
            removed += 1;
        }
        self.purged += removed;
        return removed;
    }

    pub fn purgeSlot(self: *Mailbox, slot_id: u8) usize {
        var removed: usize = 0;
        var index: usize = 0;
        while (index < self.count) {
            if (self.events[index].slot_id != slot_id) {
                index += 1;
                continue;
            }
            _ = self.removeAt(index);
            removed += 1;
        }
        self.purged += removed;
        return removed;
    }

    pub fn clear(self: *Mailbox) usize {
        const removed = self.count;
        var index: usize = 0;
        while (index < self.count) : (index += 1) self.events[index] = .{};
        self.count = 0;
        self.purged += removed;
        return removed;
    }

    pub fn pendingCount(self: *const Mailbox) usize {
        return self.count;
    }

    pub fn snapshot(self: *const Mailbox) Snapshot {
        return .{
            .pending = self.count,
            .queued = self.queued,
            .delivered = self.delivered,
            .overflows = self.overflows,
            .purged = self.purged,
            .high_water = self.high_water,
        };
    }

    fn removeAt(self: *Mailbox, index: usize) Event {
        const removed = self.events[index];
        var cursor = index;
        while (cursor + 1 < self.count) : (cursor += 1) {
            self.events[cursor] = self.events[cursor + 1];
        }
        self.count -= 1;
        self.events[self.count] = .{};
        return removed;
    }
};

pub fn transferMatch(slot_id: u8, endpoint_id: u8, trb_phys: u64) Match {
    return .{ .transfer = .{
        .slot_id = slot_id,
        .endpoint_id = endpoint_id,
        .trb_phys = .{ normalizeTrbPointer(trb_phys), 0, 0 },
        .trb_count = 1,
    } };
}

pub fn transferTdMatch(
    slot_id: u8,
    endpoint_id: u8,
    trb_phys: [MAX_TRANSFER_TRB_POINTERS]u64,
    trb_count: u8,
) Match {
    var normalized: [MAX_TRANSFER_TRB_POINTERS]u64 = .{0} ** MAX_TRANSFER_TRB_POINTERS;
    const count: usize = @min(@as(usize, trb_count), MAX_TRANSFER_TRB_POINTERS);
    var index: usize = 0;
    while (index < count) : (index += 1) {
        normalized[index] = normalizeTrbPointer(trb_phys[index]);
    }
    return .{ .transfer = .{
        .slot_id = slot_id,
        .endpoint_id = endpoint_id,
        .trb_phys = normalized,
        .trb_count = @intCast(count),
    } };
}

pub fn commandMatch(trb_phys: u64) Match {
    return .{ .command = .{ .trb_phys = normalizeTrbPointer(trb_phys) } };
}

pub fn matches(event: Event, expected: Match) bool {
    return switch (expected) {
        .transfer => |transfer| matchesTransfer(event, transfer),
        .command => |command| event.event_type == COMMAND_COMPLETION_EVENT_TYPE and
            normalizeTrbPointer(event.parameter) == normalizeTrbPointer(command.trb_phys),
    };
}

fn matchesTransfer(event: Event, expected: TransferMatch) bool {
    if (event.event_type != TRANSFER_EVENT_TYPE or event.hasEventData()) return false;
    if (event.slot_id != expected.slot_id or event.endpoint_id != expected.endpoint_id) return false;
    const pointer = normalizeTrbPointer(event.parameter);
    const count: usize = @min(@as(usize, expected.trb_count), MAX_TRANSFER_TRB_POINTERS);
    var index: usize = 0;
    while (index < count) : (index += 1) {
        if (pointer == expected.trb_phys[index]) return true;
    }
    return false;
}

pub fn normalizeTrbPointer(value: u64) u64 {
    return value & TRB_POINTER_MASK;
}

pub fn selfTest() bool {
    var mailbox = Mailbox.init();
    const hid = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 1,
        .slot_id = 2,
        .endpoint_id = 3,
        .parameter = 0x2000,
        .control = 1,
    };
    const storage = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 1,
        .slot_id = 4,
        .endpoint_id = 5,
        .parameter = 0x4000,
        .control = 1,
    };
    const command = Event{
        .event_type = COMMAND_COMPLETION_EVENT_TYPE,
        .code = 1,
        .slot_id = 4,
        .parameter = 0x6000,
        .control = 1,
    };

    if (mailbox.enqueue(hid) != .queued) return false;
    if (mailbox.enqueue(storage) != .queued) return false;
    if (mailbox.enqueue(command) != .queued) return false;
    const routed_storage = mailbox.take(transferMatch(4, 5, 0x4000)) orelse return false;
    if (routed_storage.parameter != storage.parameter) return false;
    const routed_command = mailbox.take(commandMatch(0x6000)) orelse return false;
    if (routed_command.parameter != command.parameter) return false;
    const routed_hid = mailbox.take(transferMatch(2, 3, 0x2000)) orelse return false;
    return routed_hid.parameter == hid.parameter and mailbox.pendingCount() == 0;
}

test "HID completion survives an MSC waiter" {
    const testing = @import("std").testing;
    var mailbox = Mailbox.init();
    const hid = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 1,
        .slot_id = 2,
        .endpoint_id = 3,
        .parameter = 0x20a0,
        .length = 0,
        .control = 0x0203_0001,
    };
    const storage = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 1,
        .slot_id = 4,
        .endpoint_id = 5,
        .parameter = 0x40b0,
        .length = 0,
        .control = 0x0405_0001,
    };
    const later_hid = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 1,
        .slot_id = 2,
        .endpoint_id = 3,
        .parameter = 0x20c0,
        .length = 2,
        .control = 0x0203_0001,
    };

    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(hid));
    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(storage));
    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(later_hid));

    const delivered_storage = mailbox.take(transferMatch(4, 5, 0x40bf)) orelse return error.MissingStorageCompletion;
    try testing.expectEqual(storage.parameter, delivered_storage.parameter);
    try testing.expectEqual(@as(usize, 2), mailbox.pendingCount());

    const delivered_hid = mailbox.take(transferMatch(2, 3, 0x20af)) orelse return error.MissingHidCompletion;
    try testing.expectEqual(hid.parameter, delivered_hid.parameter);
    const delivered_later = mailbox.take(transferMatch(2, 3, 0x20c0)) orelse return error.MissingLaterHidCompletion;
    try testing.expectEqual(later_hid.parameter, delivered_later.parameter);
    try testing.expectEqual(@as(usize, 0), mailbox.pendingCount());

    const stats = mailbox.snapshot();
    try testing.expectEqual(@as(u64, 3), stats.queued);
    try testing.expectEqual(@as(u64, 3), stats.delivered);
    try testing.expectEqual(@as(usize, 3), stats.high_water);
}

test "HID completion survives a command waiter" {
    const testing = @import("std").testing;
    var mailbox = Mailbox.init();
    const hid = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .slot_id = 7,
        .endpoint_id = 3,
        .parameter = 0x7100,
        .control = 0x0703_0001,
    };
    const command = Event{
        .event_type = COMMAND_COMPLETION_EVENT_TYPE,
        .code = 1,
        .slot_id = 8,
        .parameter = 0x8100,
        .control = 0x0800_0001,
    };

    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(hid));
    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(command));

    const delivered_command = mailbox.take(commandMatch(0x810f)) orelse return error.MissingCommandCompletion;
    try testing.expectEqual(command.parameter, delivered_command.parameter);
    const delivered_hid = mailbox.take(transferMatch(7, 3, 0x7100)) orelse return error.MissingHidCompletion;
    try testing.expectEqual(hid.parameter, delivered_hid.parameter);
    try testing.expectEqual(@as(usize, 0), mailbox.pendingCount());
}

test "transfer matching is exact and preserves Event Data semantics" {
    const testing = @import("std").testing;
    var mailbox = Mailbox.init();
    const event_data = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .slot_id = 2,
        .endpoint_id = 3,
        .parameter = 0x9000,
        .control = 0xa5a5_0001 | EVENT_DATA_BIT,
    };
    const normal = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .slot_id = 2,
        .endpoint_id = 3,
        .parameter = 0x9000,
        .control = 0xa5a5_0001,
    };

    try testing.expect(event_data.hasEventData());
    try testing.expect(event_data.trbPointer() == null);
    try testing.expect(!matches(event_data, transferMatch(2, 3, 0x9000)));
    try testing.expect(!matches(normal, transferMatch(3, 3, 0x9000)));
    try testing.expect(!matches(normal, transferMatch(2, 4, 0x9000)));
    try testing.expect(!matches(normal, transferMatch(2, 3, 0x9010)));

    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(event_data));
    try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(normal));
    const delivered = mailbox.take(transferMatch(2, 3, 0x900f)) orelse return error.MissingNormalCompletion;
    try testing.expectEqual(normal.control, delivered.control);
    try testing.expectEqual(@as(usize, 1), mailbox.pendingCount());
    try testing.expectEqual(@as(usize, 1), mailbox.purgeSlot(2));
}

test "control TD matches setup data and status pointers only" {
    const testing = @import("std").testing;
    // Setup liegt am Ringende, Data und Status nach dem Link-TRB wieder am
    // Ringanfang. Die Match-Menge darf daher keine zusammenhaengende Range
    // annehmen.
    const expected = transferTdMatch(6, 1, .{ 0x4fe0, 0x4000, 0x4010 }, 3);

    const setup_error = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 5,
        .slot_id = 6,
        .endpoint_id = 1,
        .parameter = 0x4fef,
    };
    const data_error = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 4,
        .slot_id = 6,
        .endpoint_id = 1,
        .parameter = 0x400f,
    };
    const status_error = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 6,
        .slot_id = 6,
        .endpoint_id = 1,
        .parameter = 0x4010,
    };
    const foreign = Event{
        .event_type = TRANSFER_EVENT_TYPE,
        .code = 4,
        .slot_id = 6,
        .endpoint_id = 1,
        .parameter = 0x4020,
    };

    try testing.expect(matches(setup_error, expected));
    try testing.expect(matches(data_error, expected));
    try testing.expect(matches(status_error, expected));
    try testing.expect(!matches(foreign, expected));
    try testing.expect(!matches(setup_error, transferMatch(6, 1, 0x4010)));
}

test "overflow never overwrites a queued completion" {
    const testing = @import("std").testing;
    var mailbox = Mailbox.init();
    var index: usize = 0;
    while (index < CAPACITY) : (index += 1) {
        try testing.expectEqual(EnqueueResult.queued, mailbox.enqueue(.{
            .event_type = TRANSFER_EVENT_TYPE,
            .slot_id = 1,
            .endpoint_id = 3,
            .parameter = 0x1000 + (@as(u64, index) * 0x10),
            .control = 1,
        }));
    }
    try testing.expectEqual(EnqueueResult.overflow, mailbox.enqueue(.{
        .event_type = TRANSFER_EVENT_TYPE,
        .slot_id = 9,
        .endpoint_id = 9,
        .parameter = 0xffff,
    }));

    const first = mailbox.take(transferMatch(1, 3, 0x1000)) orelse return error.FirstCompletionWasOverwritten;
    try testing.expectEqual(@as(u64, 0x1000), first.parameter);
    const stats = mailbox.snapshot();
    try testing.expectEqual(@as(u64, 1), stats.overflows);
    try testing.expectEqual(@as(usize, CAPACITY), stats.high_water);
}

test "purge helpers preserve unrelated event order and counters" {
    const testing = @import("std").testing;
    var mailbox = Mailbox.init();
    _ = mailbox.enqueue(.{ .event_type = TRANSFER_EVENT_TYPE, .slot_id = 1, .endpoint_id = 3, .parameter = 0x1000 });
    _ = mailbox.enqueue(.{ .event_type = TRANSFER_EVENT_TYPE, .slot_id = 2, .endpoint_id = 3, .parameter = 0x2000 });
    _ = mailbox.enqueue(.{ .event_type = TRANSFER_EVENT_TYPE, .slot_id = 1, .endpoint_id = 5, .parameter = 0x3000 });
    _ = mailbox.enqueue(.{ .event_type = COMMAND_COMPLETION_EVENT_TYPE, .slot_id = 3, .parameter = 0x4000 });

    try testing.expectEqual(@as(usize, 1), mailbox.purge(transferMatch(1, 3, 0x1000)));
    try testing.expectEqual(@as(usize, 1), mailbox.purgeSlot(1));
    const slot_two = mailbox.take(transferMatch(2, 3, 0x2000)) orelse return error.UnrelatedOrderWasLost;
    try testing.expectEqual(@as(u64, 0x2000), slot_two.parameter);
    const command = mailbox.take(commandMatch(0x4000)) orelse return error.CommandOrderWasLost;
    try testing.expectEqual(@as(u64, 0x4000), command.parameter);
    try testing.expectEqual(@as(u64, 2), mailbox.snapshot().purged);
}

test "router self test" {
    const testing = @import("std").testing;
    try testing.expect(selfTest());
}
