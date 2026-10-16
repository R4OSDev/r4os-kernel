// Native input helper; caller retains its existing queue/storage owner.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_input_physical_clear(*Queue) void;
extern fn r4os_rust_input_physical_transition(*Queue, u16, bool, u64) bool;
extern fn r4os_rust_input_physical_pop(*Queue, *Event) bool;
extern fn r4os_rust_input_physical_pending(*const Queue) bool;
extern fn r4os_rust_input_physical_snapshot(*const Queue, *Snapshot) void;
extern fn r4os_rust_input_set1_usage(u8, bool) u16;
const std = @import("std");

pub const Kind = enum(u8) {
    down,
    up,
    reset,
};

pub const Event = extern struct {
    kind: Kind = .reset,
    usage: u16 = 0,
    modifiers: u32 = 0,
    flags: u32 = 0,
    sequence: u64 = 0,
    tick: u64 = 0,
};

pub const Snapshot = extern struct {
    pending: u32 = 0,
    high_water: u32 = 0,
    transitions: u64 = 0,
    duplicate_transitions: u64 = 0,
    overflow_resets: u64 = 0,
};

const storage_len: usize = 257;
pub const capacity: u32 = storage_len - 1;
pub const flag_repeat: u32 = 1;

pub const Queue = extern struct {
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
        r4os_rust_input_physical_clear(self);
    }

    pub fn transition(self: *Queue, usage: u16, down: bool, tick: u64) bool {
        return r4os_rust_input_physical_transition(self, usage, down, tick);
    }

    pub fn pop(self: *Queue) ?Event {
        var out: Event = undefined;
        return if (r4os_rust_input_physical_pop(self, &out)) out else null;
    }

    pub fn pending(self: *const Queue) bool {
        return r4os_rust_input_physical_pending(self);
    }

    pub fn snapshot(self: *const Queue) Snapshot {
        var out: Snapshot = undefined;
        r4os_rust_input_physical_snapshot(self, &out);
        return out;
    }
};

/// Converts IBM PC/AT Set-1 scan codes to USB HID keyboard-page usages. The
/// result is layout-independent; character translation remains a separate
/// compatibility path in keyboard.zig.
pub fn set1Usage(scancode: u8, extended: bool) ?u16 {
    const result = r4os_rust_input_set1_usage(scancode, extended);
    return if (result == 0) null else result;
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

comptime {
    if (@sizeOf(Event) != 32 or @alignOf(Event) != 8 or @offsetOf(Event, "kind") != 0 or @offsetOf(Event, "usage") != 2 or @offsetOf(Event, "modifiers") != 4 or @offsetOf(Event, "flags") != 8 or @offsetOf(Event, "sequence") != 16 or @offsetOf(Event, "tick") != 24) @compileError("Native input Event layout");
}

comptime {
    if (@sizeOf(Snapshot) != 32 or @alignOf(Snapshot) != 8 or @offsetOf(Snapshot, "pending") != 0 or @offsetOf(Snapshot, "high_water") != 4 or @offsetOf(Snapshot, "transitions") != 8 or @offsetOf(Snapshot, "duplicate_transitions") != 16 or @offsetOf(Snapshot, "overflow_resets") != 24) @compileError("Native input Snapshot layout");
}

comptime {
    if (@sizeOf(Queue) != 8536 or @alignOf(Queue) != 8 or @offsetOf(Queue, "items") != 0 or @offsetOf(Queue, "held") != 8224 or @offsetOf(Queue, "head") != 8480 or @offsetOf(Queue, "tail") != 8488 or @offsetOf(Queue, "next_sequence") != 8496 or @offsetOf(Queue, "high_water") != 8504 or @offsetOf(Queue, "transitions") != 8512 or @offsetOf(Queue, "duplicate_transitions") != 8520 or @offsetOf(Queue, "overflow_resets") != 8528) @compileError("Native input Queue layout");
}
