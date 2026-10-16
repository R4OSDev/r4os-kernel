// Native input helper; caller retains its existing queue/storage owner.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_input_codepoint_reset(*Queue) void;
extern fn r4os_rust_input_codepoint_clear(*Queue) void;
extern fn r4os_rust_input_codepoint_push(*Queue, u32) bool;
extern fn r4os_rust_input_codepoint_pop(*Queue, *u32) bool;
extern fn r4os_rust_input_codepoint_pending(*const Queue) bool;
extern fn r4os_rust_input_codepoint_count(*const Queue) u32;
extern fn r4os_rust_input_codepoint_free(*const Queue) u32;
extern fn r4os_rust_input_codepoint_accept(*const Queue, u32) bool;
extern fn r4os_rust_input_codepoint_snapshot(*const Queue, *Snapshot) void;
pub const STORAGE_LEN: usize = 256;
pub const CAPACITY: u32 = STORAGE_LEN - 1;

pub const Snapshot = extern struct {
    pending: u32 = 0,
    free: u32 = CAPACITY,
    high_water: u32 = 0,
    push_attempts: u64 = 0,
    dropped: u64 = 0,
};

pub const Queue = extern struct {
    items: [STORAGE_LEN]u32 = .{0} ** STORAGE_LEN,
    head: usize = 0,
    tail: usize = 0,
    high_water: u32 = 0,
    push_attempts: u64 = 0,
    dropped: u64 = 0,

    pub fn init() Queue {
        return .{};
    }

    pub fn reset(self: *Queue) void {
        r4os_rust_input_codepoint_reset(self);
    }

    pub fn clear(self: *Queue) void {
        r4os_rust_input_codepoint_clear(self);
    }

    pub fn tryPush(self: *Queue, codepoint: u32) bool {
        return r4os_rust_input_codepoint_push(self, codepoint);
    }

    pub fn pop(self: *Queue) ?u32 {
        var out: u32 = 0;
        return if (r4os_rust_input_codepoint_pop(self, &out)) out else null;
    }

    pub fn pending(self: *const Queue) bool {
        return r4os_rust_input_codepoint_pending(self);
    }

    pub fn pendingCount(self: *const Queue) u32 {
        return r4os_rust_input_codepoint_count(self);
    }

    pub fn freeCount(self: *const Queue) u32 {
        return r4os_rust_input_codepoint_free(self);
    }

    pub fn canAccept(self: *const Queue, count: u32) bool {
        return r4os_rust_input_codepoint_accept(self, count);
    }

    pub fn snapshot(self: *const Queue) Snapshot {
        var out: Snapshot = undefined;
        r4os_rust_input_codepoint_snapshot(self, &out);
        return out;
    }
};

test "burst preserves every accepted codepoint and reports overflow" {
    const testing = @import("std").testing;
    var queue = Queue.init();

    var value: u32 = 0;
    while (value < CAPACITY) : (value += 1) {
        try testing.expect(queue.tryPush(value));
    }
    try testing.expectEqual(@as(u32, 0), queue.freeCount());
    try testing.expect(!queue.canAccept(1));
    try testing.expect(!queue.tryPush(0xffff_ffff));

    const full = queue.snapshot();
    try testing.expectEqual(CAPACITY, full.pending);
    try testing.expectEqual(CAPACITY, full.high_water);
    try testing.expectEqual(@as(u64, CAPACITY + 1), full.push_attempts);
    try testing.expectEqual(@as(u64, 1), full.dropped);

    value = 0;
    while (value < CAPACITY) : (value += 1) {
        try testing.expectEqual(value, queue.pop() orelse return error.MissingCodepoint);
    }
    try testing.expect(queue.pop() == null);
    try testing.expectEqual(CAPACITY, queue.freeCount());
}

test "capacity reservation opens only after the complete report fits" {
    const testing = @import("std").testing;
    var queue = Queue.init();
    const report_reserve: u32 = 10;

    var value: u32 = 0;
    while (queue.freeCount() > report_reserve - 1) : (value += 1) {
        try testing.expect(queue.tryPush(value));
    }
    try testing.expectEqual(report_reserve - 1, queue.freeCount());
    try testing.expect(!queue.canAccept(report_reserve));

    _ = queue.pop();
    try testing.expect(queue.canAccept(report_reserve));
}

comptime {
    if (@sizeOf(Snapshot) != 32 or @alignOf(Snapshot) != 8 or @offsetOf(Snapshot, "pending") != 0 or @offsetOf(Snapshot, "free") != 4 or @offsetOf(Snapshot, "high_water") != 8 or @offsetOf(Snapshot, "push_attempts") != 16 or @offsetOf(Snapshot, "dropped") != 24) @compileError("Native input Snapshot layout");
}

comptime {
    if (@sizeOf(Queue) != 1064 or @alignOf(Queue) != 8 or @offsetOf(Queue, "items") != 0 or @offsetOf(Queue, "head") != 1024 or @offsetOf(Queue, "tail") != 1032 or @offsetOf(Queue, "high_water") != 1040 or @offsetOf(Queue, "push_attempts") != 1048 or @offsetOf(Queue, "dropped") != 1056) @compileError("Native input Queue layout");
}
