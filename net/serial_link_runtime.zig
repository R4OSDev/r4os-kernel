const std = @import("std");

pub const max_payload: usize = 256;
pub const message_queue_capacity: usize = 16;

pub const Message = struct {
    len: u16 = 0,
    data: [max_payload]u8 = .{0} ** max_payload,
};

pub const MessageQueue = struct {
    items: [message_queue_capacity]Message = .{Message{}} ** message_queue_capacity,
    head: usize = 0,
    tail: usize = 0,
    count: usize = 0,
    high_water: usize = 0,
    drops: u64 = 0,

    pub fn push(self: *MessageQueue, payload: []const u8) bool {
        if (payload.len > max_payload) return false;
        if (self.count == message_queue_capacity) {
            self.drops +%= 1;
            return false;
        }
        self.items[self.head] = .{ .len = @intCast(payload.len) };
        if (payload.len != 0) @memcpy(self.items[self.head].data[0..payload.len], payload);
        self.head = (self.head + 1) % message_queue_capacity;
        self.count += 1;
        self.high_water = @max(self.high_water, self.count);
        return true;
    }

    pub fn pop(self: *MessageQueue, out: *Message) bool {
        out.* = .{};
        if (self.count == 0) return false;
        out.* = self.items[self.tail];
        self.items[self.tail] = .{};
        self.tail = (self.tail + 1) % message_queue_capacity;
        self.count -= 1;
        return true;
    }

    pub fn clear(self: *MessageQueue) void {
        self.* = .{};
    }
};

pub const TxDeadline = struct {
    start_tick: u64,
    timeout_ticks: u64,

    pub fn begin(now: u64, timeout_ticks: u64) TxDeadline {
        return .{ .start_tick = now, .timeout_ticks = timeout_ticks };
    }

    pub fn expired(self: TxDeadline, now: u64) bool {
        return self.timeout_ticks == 0 or now -| self.start_tick >= self.timeout_ticks;
    }
};

test "serial message burst is consumed in FIFO order" {
    var queue = MessageQueue{};
    var index: usize = 0;
    while (index < message_queue_capacity) : (index += 1) {
        const payload = [_]u8{@intCast(index)};
        try std.testing.expect(queue.push(payload[0..]));
    }
    try std.testing.expectEqual(message_queue_capacity, queue.high_water);
    try std.testing.expect(!queue.push("overflow"));
    try std.testing.expectEqual(@as(u64, 1), queue.drops);

    index = 0;
    while (index < message_queue_capacity) : (index += 1) {
        var message = Message{};
        try std.testing.expect(queue.pop(&message));
        try std.testing.expectEqual(@as(u16, 1), message.len);
        try std.testing.expectEqual(@as(u8, @intCast(index)), message.data[0]);
    }
    var empty = Message{};
    try std.testing.expect(!queue.pop(&empty));
}

test "serial transmit deadline is monotone and saturating" {
    const deadline = TxDeadline.begin(100, 25);
    try std.testing.expect(!deadline.expired(124));
    try std.testing.expect(deadline.expired(125));
    try std.testing.expect(!deadline.expired(90));
    try std.testing.expect(TxDeadline.begin(100, 0).expired(100));
}
