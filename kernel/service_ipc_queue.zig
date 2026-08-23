pub const State = enum(u8) {
    free,
    raw_ready,
    handler_queued,
    handler_running,
    handler_done,
    handler_ready,
};

pub const Mode = enum(u8) {
    raw,
    queued_response,
    direct_response,
};

pub const SlotMeta = struct {
    state: State = .free,
    mode: Mode = .raw,
    generation: u64 = 0,
    sequence: u64 = 0,
    len: u16 = 0,
    result: i32 = 0,
    abandoned: bool = false,
    completion_published: bool = false,
};

pub const Selection = struct {
    index: usize,
    visits: usize,
};

pub const ReceiveDecision = union(enum) {
    empty,
    deliver: Selection,
    drop_too_small: struct {
        selection: Selection,
        required: usize,
    },
};

pub const TimeoutAction = enum {
    none,
    reset,
    abandon,
};

pub const CompletionAction = enum {
    none,
    discard_error,
    publish_ready,
    deliver,
    drop_too_small,
};

pub fn firstFree(slots: anytype) ?Selection {
    var i: usize = 0;
    while (i < slots.len) : (i += 1) {
        if (slots[i].meta.state == .free) return .{ .index = i, .visits = i + 1 };
    }
    return null;
}

pub fn oldestReady(slots: anytype) ?Selection {
    var selected: ?usize = null;
    var selected_sequence: u64 = 0;
    var i: usize = 0;
    while (i < slots.len) : (i += 1) {
        const meta = slots[i].meta;
        if (meta.state != .raw_ready and meta.state != .handler_ready) continue;
        if (selected == null or meta.sequence < selected_sequence) {
            selected = i;
            selected_sequence = meta.sequence;
        }
    }
    return if (selected) |index| .{ .index = index, .visits = slots.len } else null;
}

pub fn oldestQueued(slots: anytype) ?Selection {
    var selected: ?usize = null;
    var selected_sequence: u64 = 0;
    var i: usize = 0;
    while (i < slots.len) : (i += 1) {
        const meta = slots[i].meta;
        if (meta.state != .handler_queued) continue;
        if (selected == null or meta.sequence < selected_sequence) {
            selected = i;
            selected_sequence = meta.sequence;
        }
    }
    return if (selected) |index| .{ .index = index, .visits = slots.len } else null;
}

pub fn receiveDecision(slots: anytype, capacity: usize) ReceiveDecision {
    const selection = oldestReady(slots) orelse return .empty;
    const required: usize = @intCast(slots[selection.index].meta.len);
    if (capacity < required) return .{ .drop_too_small = .{ .selection = selection, .required = required } };
    return .{ .deliver = selection };
}

pub fn countReady(slots: anytype) usize {
    var count: usize = 0;
    for (slots) |slot| {
        if (slot.meta.state == .raw_ready or slot.meta.state == .handler_ready) count += 1;
    }
    return count;
}

pub fn countUsed(slots: anytype) usize {
    var count: usize = 0;
    for (slots) |slot| {
        if (slot.meta.state != .free) count += 1;
    }
    return count;
}

pub fn countRunning(slots: anytype) usize {
    var count: usize = 0;
    for (slots) |slot| {
        if (slot.meta.state == .handler_running) count += 1;
    }
    return count;
}

pub fn matches(meta: SlotMeta, generation: u64, state: State) bool {
    return meta.generation == generation and meta.state == state;
}

pub fn channelGenerationMatches(current: u64, submitted: u64) bool {
    return current == submitted;
}

pub fn timeoutAction(meta: SlotMeta, generation: u64) TimeoutAction {
    if (meta.generation != generation) return .none;
    return switch (meta.state) {
        .handler_queued => .reset,
        .handler_running => .abandon,
        .handler_done => if (meta.completion_published) .reset else .abandon,
        else => .none,
    };
}

pub fn completionAction(meta: SlotMeta, generation: u64, capacity: ?usize) CompletionAction {
    if (meta.generation != generation or meta.state != .handler_done) return .none;
    if (meta.result < 0) return .discard_error;
    return switch (meta.mode) {
        .queued_response => .publish_ready,
        .direct_response => if (capacity == null or capacity.? < meta.len) .drop_too_small else .deliver,
        .raw => .discard_error,
    };
}

const TestSlot = struct {
    meta: SlotMeta = .{},
};

test "fragmented slots retain FIFO selection and bounded scans" {
    const testing = @import("std").testing;
    var slots = [_]TestSlot{.{}} ** 5;
    slots[0].meta = .{ .state = .handler_running, .sequence = 1 };
    slots[1].meta = .{ .state = .handler_ready, .sequence = 9, .len = 8 };
    slots[3].meta = .{ .state = .raw_ready, .sequence = 4, .len = 3 };
    slots[4].meta = .{ .state = .handler_queued, .sequence = 2 };

    const free = firstFree(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 2), free.index);
    try testing.expectEqual(@as(usize, 3), free.visits);
    const ready = oldestReady(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 3), ready.index);
    try testing.expectEqual(@as(usize, slots.len), ready.visits);
    const queued = oldestQueued(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 4), queued.index);
    try testing.expectEqual(@as(usize, 2), countReady(&slots));
    try testing.expectEqual(@as(usize, 4), countUsed(&slots));
    try testing.expectEqual(@as(usize, 1), countRunning(&slots));
}

test "too-small receive is an explicit drop decision" {
    const testing = @import("std").testing;
    var slots = [_]TestSlot{.{}} ** 3;
    slots[0].meta = .{ .state = .handler_ready, .sequence = 1, .len = 64 };
    slots[1].meta = .{ .state = .raw_ready, .sequence = 2, .len = 4 };

    switch (receiveDecision(&slots, 8)) {
        .drop_too_small => |decision| {
            try testing.expectEqual(@as(usize, 0), decision.selection.index);
            try testing.expectEqual(@as(usize, 64), decision.required);
        },
        else => return error.TestUnexpectedResult,
    }
    slots[0].meta = .{};
    switch (receiveDecision(&slots, 8)) {
        .deliver => |selection| try testing.expectEqual(@as(usize, 1), selection.index),
        else => return error.TestUnexpectedResult,
    }
}

test "generation and state jointly revalidate a worker target" {
    const testing = @import("std").testing;
    const meta = SlotMeta{ .state = .handler_running, .generation = 7 };
    try testing.expect(matches(meta, 7, .handler_running));
    try testing.expect(!matches(meta, 6, .handler_running));
    try testing.expect(!matches(meta, 7, .handler_done));
}

test "timeout never recycles a completion before its wake publication" {
    const testing = @import("std").testing;
    try testing.expectEqual(TimeoutAction.reset, timeoutAction(.{ .state = .handler_queued, .generation = 3 }, 3));
    try testing.expectEqual(TimeoutAction.abandon, timeoutAction(.{ .state = .handler_running, .generation = 3 }, 3));
    try testing.expectEqual(TimeoutAction.abandon, timeoutAction(.{ .state = .handler_done, .generation = 3 }, 3));
    try testing.expectEqual(TimeoutAction.reset, timeoutAction(.{ .state = .handler_done, .generation = 3, .completion_published = true }, 3));
    try testing.expectEqual(TimeoutAction.none, timeoutAction(.{ .state = .handler_queued, .generation = 4 }, 3));
}

test "handler completion has explicit error response and capacity outcomes" {
    const testing = @import("std").testing;
    try testing.expectEqual(CompletionAction.discard_error, completionAction(.{ .state = .handler_done, .mode = .direct_response, .generation = 5, .result = -1 }, 5, 64));
    try testing.expectEqual(CompletionAction.publish_ready, completionAction(.{ .state = .handler_done, .mode = .queued_response, .generation = 5, .result = 16, .len = 16 }, 5, null));
    try testing.expectEqual(CompletionAction.drop_too_small, completionAction(.{ .state = .handler_done, .mode = .direct_response, .generation = 5, .result = 16, .len = 16 }, 5, 8));
    try testing.expectEqual(CompletionAction.deliver, completionAction(.{ .state = .handler_done, .mode = .direct_response, .generation = 5, .result = 16, .len = 16 }, 5, 16));
    try testing.expectEqual(CompletionAction.none, completionAction(.{ .state = .handler_done, .mode = .direct_response, .generation = 6, .result = 16, .len = 16 }, 5, 16));
}

test "interleaved producers workers and consumers preserve bounded FIFO state" {
    const testing = @import("std").testing;
    var slots = [_]TestSlot{.{}} ** 4;

    // Zwei Produzenten belegen abwechselnd Slots; ein Worker und ein Raw-
    // Consumer bauen den Zustand in anderer Reihenfolge wieder ab.
    slots[0].meta = .{ .state = .handler_queued, .mode = .queued_response, .generation = 1, .sequence = 10, .len = 8 };
    slots[1].meta = .{ .state = .handler_queued, .mode = .direct_response, .generation = 2, .sequence = 11, .len = 8 };
    slots[2].meta = .{ .state = .raw_ready, .mode = .raw, .generation = 3, .sequence = 12, .len = 4 };
    slots[3].meta = .{ .state = .handler_queued, .mode = .queued_response, .generation = 4, .sequence = 13, .len = 8 };
    try testing.expect(firstFree(&slots) == null);
    try testing.expectEqual(@as(usize, 4), countUsed(&slots));

    const first_work = oldestQueued(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 0), first_work.index);
    slots[first_work.index].meta.state = .handler_running;
    slots[first_work.index].meta.state = .handler_done;
    slots[first_work.index].meta.result = 8;
    try testing.expectEqual(CompletionAction.publish_ready, completionAction(slots[first_work.index].meta, 1, null));
    slots[first_work.index].meta.state = .handler_ready;

    const first_response = oldestReady(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 0), first_response.index);
    slots[first_response.index].meta = .{};
    const free = firstFree(&slots) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 0), free.index);
    slots[free.index].meta = .{ .state = .handler_queued, .mode = .direct_response, .generation = 5, .sequence = 14, .len = 8 };

    try testing.expectEqual(@as(usize, 1), (oldestQueued(&slots) orelse return error.TestUnexpectedResult).index);
    try testing.expectEqual(@as(usize, 2), (oldestReady(&slots) orelse return error.TestUnexpectedResult).index);
    try testing.expectEqual(@as(usize, 4), countUsed(&slots));
}

test "logical close keeps running work valid while handler rebind invalidates it" {
    const testing = @import("std").testing;
    const submitted_channel_generation: u64 = 7;
    var meta = SlotMeta{
        .state = .handler_running,
        .mode = .direct_response,
        .generation = 9,
        .sequence = 21,
        .len = 24,
    };

    // Open/close ist nur eine logische Lebenszykluszaehlung und recycelt die
    // stabile Kanalidentitaet nicht: laufende Arbeit darf normal abschliessen.
    try testing.expect(channelGenerationMatches(7, submitted_channel_generation));
    try testing.expect(matches(meta, 9, .handler_running));
    meta.state = .handler_done;
    meta.result = 24;
    try testing.expectEqual(CompletionAction.deliver, completionAction(meta, 9, 24));

    // Eine echte Handler-Neubindung hebt dagegen die Generation an und macht
    // das alte Handlerresultat gezielt ungueltig.
    try testing.expect(!channelGenerationMatches(8, submitted_channel_generation));
}
