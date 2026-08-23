pub const STORAGE_LEN: usize = 256;
pub const CAPACITY: u32 = STORAGE_LEN - 1;

pub const Snapshot = struct {
    pending: u32 = 0,
    free: u32 = CAPACITY,
    high_water: u32 = 0,
    push_attempts: u64 = 0,
    dropped: u64 = 0,
};

pub const Queue = struct {
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
        self.* = .{};
    }

    pub fn clear(self: *Queue) void {
        self.items = .{0} ** STORAGE_LEN;
        self.head = 0;
        self.tail = 0;
    }

    pub fn tryPush(self: *Queue, codepoint: u32) bool {
        self.push_attempts +%= 1;
        const next = (self.head + 1) % STORAGE_LEN;
        if (next == self.tail) {
            self.dropped +%= 1;
            return false;
        }
        self.items[self.head] = codepoint;
        self.head = next;
        const queued = self.pendingCount();
        if (queued > self.high_water) self.high_water = queued;
        return true;
    }

    pub fn pop(self: *Queue) ?u32 {
        if (self.head == self.tail) return null;
        const codepoint = self.items[self.tail];
        self.items[self.tail] = 0;
        self.tail = (self.tail + 1) % STORAGE_LEN;
        return codepoint;
    }

    pub fn pending(self: *const Queue) bool {
        return self.head != self.tail;
    }

    pub fn pendingCount(self: *const Queue) u32 {
        return @intCast(if (self.head >= self.tail)
            self.head - self.tail
        else
            STORAGE_LEN - self.tail + self.head);
    }

    pub fn freeCount(self: *const Queue) u32 {
        return CAPACITY - self.pendingCount();
    }

    pub fn canAccept(self: *const Queue, count: u32) bool {
        return count <= self.freeCount();
    }

    pub fn snapshot(self: *const Queue) Snapshot {
        return .{
            .pending = self.pendingCount(),
            .free = self.freeCount(),
            .high_water = self.high_water,
            .push_attempts = self.push_attempts,
            .dropped = self.dropped,
        };
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
