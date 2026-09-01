const std = @import("std");

pub const Kind = enum(u8) {
    down,
    up,
    reset,
};

pub const Event = struct {
    kind: Kind = .reset,
    usage: u16 = 0,
    modifiers: u32 = 0,
    flags: u32 = 0,
    sequence: u64 = 0,
    tick: u64 = 0,
};

pub const Snapshot = struct {
    pending: u32 = 0,
    high_water: u32 = 0,
    transitions: u64 = 0,
    duplicate_transitions: u64 = 0,
    overflow_resets: u64 = 0,
};

const storage_len: usize = 257;
pub const capacity: u32 = storage_len - 1;
pub const flag_repeat: u32 = 1;

pub const Queue = struct {
    items: [storage_len]Event = [_]Event{.{}} ** storage_len,
    held: [256]bool = .{false} ** 256,
    head: usize = 0,
    tail: usize = 0,
    next_sequence: u64 = 1,
    high_water: u32 = 0,
    transitions: u64 = 0,
    duplicate_transitions: u64 = 0,
    overflow_resets: u64 = 0,

    pub fn init() Queue {
        return .{};
    }

    pub fn clear(self: *Queue) void {
        self.items = [_]Event{.{}} ** storage_len;
        self.held = .{false} ** 256;
        self.head = 0;
        self.tail = 0;
    }

    pub fn transition(self: *Queue, usage: u16, down: bool, tick: u64) bool {
        if (usage == 0 or usage >= @as(u16, @intCast(self.held.len))) return false;
        const index: usize = @intCast(usage);
        if (self.held[index] == down) {
            self.duplicate_transitions +%= 1;
            if (down) {
                if (self.full()) {
                    self.publishReset(tick);
                    return true;
                }
                self.pushAssumeCapacity(.{
                    .kind = .down,
                    .usage = usage,
                    .modifiers = self.modifierSnapshot(),
                    .flags = flag_repeat,
                    .sequence = self.takeSequence(),
                    .tick = tick,
                });
                return true;
            }
            return false;
        }
        if (self.full()) {
            self.publishReset(tick);
            return true;
        }
        self.held[index] = down;
        self.pushAssumeCapacity(.{
            .kind = if (down) .down else .up,
            .usage = usage,
            .modifiers = self.modifierSnapshot(),
            .sequence = self.takeSequence(),
            .tick = tick,
        });
        self.transitions +%= 1;
        return true;
    }

    pub fn pop(self: *Queue) ?Event {
        if (self.head == self.tail) return null;
        const event = self.items[self.tail];
        self.items[self.tail] = .{};
        self.tail = (self.tail + 1) % storage_len;
        return event;
    }

    pub fn pending(self: *const Queue) bool {
        return self.head != self.tail;
    }

    pub fn snapshot(self: *const Queue) Snapshot {
        return .{
            .pending = self.pendingCount(),
            .high_water = self.high_water,
            .transitions = self.transitions,
            .duplicate_transitions = self.duplicate_transitions,
            .overflow_resets = self.overflow_resets,
        };
    }

    fn full(self: *const Queue) bool {
        return (self.head + 1) % storage_len == self.tail;
    }

    fn pendingCount(self: *const Queue) u32 {
        return @intCast(if (self.head >= self.tail)
            self.head - self.tail
        else
            storage_len - self.tail + self.head);
    }

    fn pushAssumeCapacity(self: *Queue, event: Event) void {
        self.items[self.head] = event;
        self.head = (self.head + 1) % storage_len;
        self.high_water = @max(self.high_water, self.pendingCount());
    }

    fn publishReset(self: *Queue, tick: u64) void {
        self.items = [_]Event{.{}} ** storage_len;
        self.held = .{false} ** 256;
        self.head = 0;
        self.tail = 0;
        self.overflow_resets +%= 1;
        self.pushAssumeCapacity(.{
            .kind = .reset,
            .sequence = self.takeSequence(),
            .tick = tick,
        });
    }

    fn takeSequence(self: *Queue) u64 {
        const result = self.next_sequence;
        self.next_sequence +%= 1;
        if (self.next_sequence == 0) self.next_sequence = 1;
        return result;
    }

    fn modifierSnapshot(self: *const Queue) u32 {
        return (if (self.held[0xE0]) @as(u32, 1 << 0) else 0) |
            (if (self.held[0xE4]) @as(u32, 1 << 1) else 0) |
            (if (self.held[0xE2]) @as(u32, 1 << 2) else 0) |
            (if (self.held[0xE6]) @as(u32, 1 << 3) else 0) |
            (if (self.held[0xE1]) @as(u32, 1 << 4) else 0) |
            (if (self.held[0xE5]) @as(u32, 1 << 5) else 0) |
            (if (self.held[0xE3]) @as(u32, 1 << 6) else 0) |
            (if (self.held[0xE7]) @as(u32, 1 << 7) else 0);
    }
};

/// Converts IBM PC/AT Set-1 scan codes to USB HID keyboard-page usages. The
/// result is layout-independent; character translation remains a separate
/// compatibility path in keyboard.zig.
pub fn set1Usage(scancode: u8, extended: bool) ?u16 {
    if (extended) return switch (scancode) {
        0x1C => 0x58,
        0x1D => 0xE4,
        0x35 => 0x54,
        0x37 => 0x46,
        0x38 => 0xE6,
        0x47 => 0x4A,
        0x48 => 0x52,
        0x49 => 0x4B,
        0x4B => 0x50,
        0x4D => 0x4F,
        0x4F => 0x4D,
        0x50 => 0x51,
        0x51 => 0x4E,
        0x52 => 0x49,
        0x53 => 0x4C,
        0x5B => 0xE3,
        0x5C => 0xE7,
        0x5D => 0x65,
        else => null,
    };
    return switch (scancode) {
        0x01 => 0x29,
        0x02...0x0B => @as(u16, scancode) + 0x1C,
        0x0C => 0x2D,
        0x0D => 0x2E,
        0x0E => 0x2A,
        0x0F => 0x2B,
        0x10 => 0x14,
        0x11 => 0x1A,
        0x12 => 0x08,
        0x13 => 0x15,
        0x14 => 0x17,
        0x15 => 0x1C,
        0x16 => 0x18,
        0x17 => 0x0C,
        0x18 => 0x12,
        0x19 => 0x13,
        0x1A => 0x2F,
        0x1B => 0x30,
        0x1C => 0x28,
        0x1D => 0xE0,
        0x1E => 0x04,
        0x1F => 0x16,
        0x20 => 0x07,
        0x21 => 0x09,
        0x22 => 0x0A,
        0x23 => 0x0B,
        0x24 => 0x0D,
        0x25 => 0x0E,
        0x26 => 0x0F,
        0x27 => 0x33,
        0x28 => 0x34,
        0x29 => 0x35,
        0x2A => 0xE1,
        0x2B => 0x31,
        0x2C => 0x1D,
        0x2D => 0x1B,
        0x2E => 0x06,
        0x2F => 0x19,
        0x30 => 0x05,
        0x31 => 0x11,
        0x32 => 0x10,
        0x33 => 0x36,
        0x34 => 0x37,
        0x35 => 0x38,
        0x36 => 0xE5,
        0x37 => 0x55,
        0x38 => 0xE2,
        0x39 => 0x2C,
        0x3A => 0x39,
        0x3B...0x44 => @as(u16, scancode) - 1,
        0x45 => 0x53,
        0x46 => 0x47,
        0x47 => 0x5F,
        0x48 => 0x60,
        0x49 => 0x61,
        0x4A => 0x56,
        0x4B => 0x5C,
        0x4C => 0x5D,
        0x4D => 0x5E,
        0x4E => 0x57,
        0x4F => 0x59,
        0x50 => 0x5A,
        0x51 => 0x5B,
        0x52 => 0x62,
        0x53 => 0x63,
        0x56 => 0x64,
        0x57 => 0x44,
        0x58 => 0x45,
        else => null,
    };
}

test "set one conversion distinguishes modifier sides and cursor keys" {
    try std.testing.expectEqual(@as(?u16, 0xE0), set1Usage(0x1D, false));
    try std.testing.expectEqual(@as(?u16, 0xE4), set1Usage(0x1D, true));
    try std.testing.expectEqual(@as(?u16, 0xE2), set1Usage(0x38, false));
    try std.testing.expectEqual(@as(?u16, 0xE6), set1Usage(0x38, true));
    try std.testing.expectEqual(@as(?u16, 0x50), set1Usage(0x4B, true));
    try std.testing.expectEqual(@as(?u16, 0x4F), set1Usage(0x4D, true));
}

test "keypad make break repeat and Num Lock stay distinct from navigation" {
    const keypad = [_]struct { scan: u8, usage: u16 }{
        .{ .scan = 0x50, .usage = 0x5A },
        .{ .scan = 0x4B, .usage = 0x5C },
        .{ .scan = 0x4D, .usage = 0x5E },
        .{ .scan = 0x47, .usage = 0x5F },
        .{ .scan = 0x48, .usage = 0x60 },
        .{ .scan = 0x49, .usage = 0x61 },
    };
    for (keypad) |entry| {
        try std.testing.expectEqual(@as(?u16, entry.usage), set1Usage(entry.scan, false));
        try std.testing.expect(set1Usage(entry.scan, true).? != entry.usage);
    }
    try std.testing.expectEqual(@as(?u16, 0x53), set1Usage(0x45, false));

    var queue = Queue.init();
    try std.testing.expect(queue.transition(0x60, true, 1));
    try std.testing.expect(queue.transition(0x60, true, 2));
    try std.testing.expect(queue.transition(0x60, false, 3));
    try std.testing.expectEqual(Kind.down, queue.pop().?.kind);
    const repeated = queue.pop().?;
    try std.testing.expectEqual(Kind.down, repeated.kind);
    try std.testing.expectEqual(flag_repeat, repeated.flags);
    try std.testing.expectEqual(Kind.up, queue.pop().?.kind);
}

test "ordered transitions publish post-transition side-specific modifiers" {
    var queue = Queue.init();
    try std.testing.expect(queue.transition(0xE2, true, 10));
    try std.testing.expect(queue.transition(0xE4, true, 11));
    try std.testing.expect(queue.transition(0xE4, true, 12));
    try std.testing.expect(queue.transition(0xE2, false, 13));

    const left_alt_down = queue.pop().?;
    const right_control_down = queue.pop().?;
    const right_control_repeat = queue.pop().?;
    const left_alt_up = queue.pop().?;
    try std.testing.expectEqual(Kind.down, left_alt_down.kind);
    try std.testing.expectEqual(@as(u32, 1 << 2), left_alt_down.modifiers);
    try std.testing.expectEqual(@as(u32, (1 << 2) | (1 << 1)), right_control_down.modifiers);
    try std.testing.expectEqual(Kind.down, right_control_repeat.kind);
    try std.testing.expectEqual(flag_repeat, right_control_repeat.flags);
    try std.testing.expectEqual(@as(u32, 1 << 1), left_alt_up.modifiers);
    try std.testing.expect(left_alt_down.sequence < right_control_down.sequence);
    try std.testing.expect(right_control_down.sequence < right_control_repeat.sequence);
    try std.testing.expect(right_control_repeat.sequence < left_alt_up.sequence);
    try std.testing.expect(queue.pop() == null);
    try std.testing.expectEqual(@as(u64, 1), queue.snapshot().duplicate_transitions);
}

test "overflow collapses uncertain held state to one fail-safe reset" {
    var queue = Queue.init();
    var usage: u16 = 1;
    while (usage < 256) : (usage += 1) try std.testing.expect(queue.transition(usage, true, usage));
    try std.testing.expect(queue.transition(1, false, 300));
    try std.testing.expect(queue.transition(2, false, 301));
    const reset = queue.pop().?;
    try std.testing.expectEqual(Kind.reset, reset.kind);
    try std.testing.expect(queue.pop() == null);
    try std.testing.expectEqual(@as(u64, 1), queue.snapshot().overflow_resets);
}
