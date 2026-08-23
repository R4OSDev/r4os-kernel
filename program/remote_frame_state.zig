pub const history_capacity: usize = 64;

pub const Rect = struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,

    pub fn empty(self: Rect) bool {
        return self.w == 0 or self.h == 0;
    }
};

const Change = struct {
    revision: u32 = 0,
    rect: Rect = .{},
};

pub const History = struct {
    changes: [history_capacity]Change = .{Change{}} ** history_capacity,
    count: usize = 0,

    pub fn reset(self: *History) void {
        self.* = .{};
    }

    pub fn record(self: *History, revision: u32, rect: Rect) void {
        if (revision == 0) return;
        self.changes[indexFor(revision)] = .{ .revision = revision, .rect = rect };
        if (self.count < history_capacity) self.count += 1;
    }

    pub fn unionSince(self: *const History, last_revision: u32, current_revision: u32, width: u32, height: u32) Rect {
        const full = Rect{ .w = width, .h = height };
        if (width == 0 or height == 0 or current_revision == 0) return .{};
        if (last_revision == current_revision) return .{};
        if (last_revision == 0) return full;

        const distance = revisionDistance(last_revision, current_revision);
        if (distance == 0 or distance > history_capacity or distance > self.count) return full;

        var revision = last_revision;
        var merged = Rect{};
        var remaining = distance;
        while (remaining > 0) : (remaining -= 1) {
            revision = nextRevision(revision);
            const change = self.changes[indexFor(revision)];
            if (change.revision != revision or change.rect.empty()) return full;
            merged = if (merged.empty()) change.rect else merge(merged, change.rect, width, height);
        }
        return if (merged.empty()) full else merged;
    }
};

pub fn merge(a: Rect, b: Rect, width: u32, height: u32) Rect {
    if (a.empty()) return b;
    if (b.empty()) return a;
    const x0 = @min(a.x, b.x);
    const y0 = @min(a.y, b.y);
    const x1 = @min(@max(@as(u64, a.x) + a.w, @as(u64, b.x) + b.w), @as(u64, width));
    const y1 = @min(@max(@as(u64, a.y) + a.h, @as(u64, b.y) + b.h), @as(u64, height));
    return .{
        .x = x0,
        .y = y0,
        .w = @intCast(x1 - x0),
        .h = @intCast(y1 - y0),
    };
}

fn indexFor(revision: u32) usize {
    return @intCast(revision % history_capacity);
}

fn nextRevision(revision: u32) u32 {
    return if (revision == 0xffff_ffff) 1 else revision + 1;
}

fn revisionDistance(older: u32, newer: u32) usize {
    if (older == 0 or newer == 0 or older == newer) return 0;
    if (newer > older) return @intCast(newer - older);
    return @intCast((0xffff_ffff - older) + newer);
}

test "dirty history merges disjoint publications after a consumer revision" {
    var history: History = .{};
    history.record(1, .{ .w = 100, .h = 80 });
    history.record(2, .{ .x = 3, .y = 4, .w = 5, .h = 6 });
    history.record(3, .{ .x = 40, .y = 30, .w = 7, .h = 8 });

    const rect = history.unionSince(1, 3, 100, 80);
    try @import("std").testing.expectEqual(Rect{ .x = 3, .y = 4, .w = 44, .h = 34 }, rect);
}

test "dirty history falls back to a full frame when the consumer is too old" {
    var history: History = .{};
    var revision: u32 = 1;
    while (revision <= history_capacity + 2) : (revision += 1) {
        history.record(revision, .{ .x = revision, .y = 1, .w = 1, .h = 1 });
    }

    try @import("std").testing.expectEqual(
        Rect{ .w = 320, .h = 200 },
        history.unionSince(1, history_capacity + 2, 320, 200),
    );
}

test "dirty history follows the nonzero revision wrap" {
    var history: History = .{};
    history.record(0xffff_ffff, .{ .x = 1, .y = 2, .w = 3, .h = 4 });
    history.record(1, .{ .x = 10, .y = 12, .w = 5, .h = 6 });

    try @import("std").testing.expectEqual(
        Rect{ .x = 10, .y = 12, .w = 5, .h = 6 },
        history.unionSince(0xffff_ffff, 1, 100, 80),
    );
}
