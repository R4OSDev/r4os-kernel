const std = @import("std");

pub const Admission = struct {
    drop_existing: usize = 0,
    skip_input: usize = 0,

    pub fn dropped(self: Admission) usize {
        return self.drop_existing + self.skip_input;
    }
};

// Keep the newest `capacity` bytes.  Existing ring bytes are older than the
// complete incoming span, so they are discarded first; an oversized span
// then skips only its own oldest prefix.
pub fn planAdmission(pending: usize, capacity: usize, incoming: usize) Admission {
    if (capacity == 0) return .{ .skip_input = incoming };
    const retained_pending = @min(pending, capacity);
    const overflow = (retained_pending +| incoming) -| capacity;
    const drop_existing = @min(retained_pending, overflow);
    return .{
        .drop_existing = drop_existing,
        .skip_input = overflow - drop_existing,
    };
}

pub fn drainCount(pending: usize, fifo_ready: bool, fifo_depth: usize) usize {
    if (!fifo_ready) return 0;
    return @min(pending, fifo_depth);
}

test "span admission preserves byte order and newest bounded window" {
    try std.testing.expectEqual(Admission{}, planAdmission(3, 8, 4));
    try std.testing.expectEqual(
        Admission{ .drop_existing = 3, .skip_input = 0 },
        planAdmission(7, 8, 4),
    );
    try std.testing.expectEqual(
        Admission{ .drop_existing = 5, .skip_input = 4 },
        planAdmission(5, 8, 12),
    );
    try std.testing.expectEqual(@as(usize, 9), planAdmission(5, 8, 12).dropped());
}

test "one status decision bounds a fifo drain" {
    try std.testing.expectEqual(@as(usize, 0), drainCount(20, false, 16));
    try std.testing.expectEqual(@as(usize, 7), drainCount(7, true, 16));
    try std.testing.expectEqual(@as(usize, 16), drainCount(20, true, 16));
}
