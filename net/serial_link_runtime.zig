// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_net_serial_push(*MessageQueue, [*]const u8, usize) bool;
extern fn r4os_rust_net_serial_pop(*MessageQueue, *Message) bool;
extern fn r4os_rust_net_serial_clear(*MessageQueue) void;
extern fn r4os_rust_net_serial_expired(u64, u64, u64) bool;
const std = @import("std");

pub const max_payload: usize = 256;
pub const message_queue_capacity: usize = 16;

pub const Message = extern struct {
    len: u16 = 0,
    data: [max_payload]u8 = .{0} ** max_payload,
};

pub const MessageQueue = extern struct {
    items: [message_queue_capacity]Message = .{Message{}} ** message_queue_capacity,
    head: usize = 0,
    tail: usize = 0,
    count: usize = 0,
    high_water: usize = 0,
    drops: u64 = 0,

    pub fn push(self: *MessageQueue, payload: []const u8) bool {
        return r4os_rust_net_serial_push(self, payload.ptr, payload.len);
    }

    pub fn pop(self: *MessageQueue, out: *Message) bool {
        return r4os_rust_net_serial_pop(self, out);
    }

    pub fn clear(self: *MessageQueue) void {
        r4os_rust_net_serial_clear(self);
    }
};

pub const TxDeadline = extern struct {
    start_tick: u64,
    timeout_ticks: u64,

    pub fn begin(now: u64, timeout_ticks: u64) TxDeadline {
        return .{ .start_tick = now, .timeout_ticks = timeout_ticks };
    }

    pub fn expired(self: TxDeadline, now: u64) bool {
        return r4os_rust_net_serial_expired(self.start_tick, self.timeout_ticks, now);
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

comptime {
    if (@sizeOf(Message) != 258 or @alignOf(Message) != 2 or @offsetOf(Message, "len") != 0 or @offsetOf(Message, "data") != 2) @compileError("Native network Message layout");
}

comptime {
    if (@sizeOf(MessageQueue) != 4168 or @alignOf(MessageQueue) != 8 or @offsetOf(MessageQueue, "items") != 0 or @offsetOf(MessageQueue, "head") != 4128 or @offsetOf(MessageQueue, "tail") != 4136 or @offsetOf(MessageQueue, "count") != 4144 or @offsetOf(MessageQueue, "high_water") != 4152 or @offsetOf(MessageQueue, "drops") != 4160) @compileError("Native network MessageQueue layout");
}

comptime {
    if (@sizeOf(TxDeadline) != 16 or @alignOf(TxDeadline) != 8 or @offsetOf(TxDeadline, "start_tick") != 0 or @offsetOf(TxDeadline, "timeout_ticks") != 8) @compileError("Native network TxDeadline layout");
}
