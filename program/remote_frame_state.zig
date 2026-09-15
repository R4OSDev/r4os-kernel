pub const history_capacity: usize = 64;
pub const snapshot_capacity = 3;
pub const consumer_capacity = 64;
pub const leases_per_consumer = 8;
pub const max_frame_bytes = 64 * 1024 * 1024;

pub const Owner = struct {
    id: u32 = 0, generation: u64 = 0,
    pub fn same(self: Owner, other: Owner) bool { return self.id == other.id and self.generation == other.generation; }
    fn valid(self: Owner) bool { return self.id != 0 and self.generation != 0; }
};
const Consumer = struct { owner: Owner = .{}, count: u32 = 0 };
pub const Lease = struct { id: u64 = 0, owner: Owner = .{}, snapshot: usize = 0, epoch: u64 = 0, started_ns: u64 = 0 };
pub const CaptureRegistry = struct {
    consumers: [consumer_capacity]Consumer = @splat(.{}),
    leases: [consumer_capacity]Lease = @splat(.{}),
    references: [snapshot_capacity]u32 = @splat(0),
    count: u32 = 0,
    next_lease: u64 = 1,
    acquired: u64 = 0,
    max_reader_ns: u64 = 0,
    pub const Error = error{ Invalid, Capacity, Stale, Busy };
    pub fn acquire(self: *CaptureRegistry, owner: Owner) Error!u32 {
        if (!owner.valid()) return error.Invalid;
        if (self.count == 0x7fffffff) return error.Capacity;
        const entry = for (&self.consumers) |*value| { if (value.count != 0 and value.owner.same(owner)) break value; }
            else for (&self.consumers) |*value| { if (value.count == 0) break value; } else return error.Capacity;
        entry.owner = owner; entry.count += 1; self.count += 1;
        return self.count;
    }
    pub fn release(self: *CaptureRegistry, owner: Owner) Error!u32 {
        const entry = for (&self.consumers) |*value| { if (value.count != 0 and value.owner.same(owner)) break value; } else return error.Stale;
        entry.count -= 1; self.count -= 1;
        if (entry.count == 0) entry.* = .{};
        return self.count;
    }
    pub fn hasConsumer(self: *const CaptureRegistry, owner: Owner) bool {
        for (&self.consumers) |*value| if (value.count != 0 and value.owner.same(owner)) return true;
        return false;
    }
    pub fn beginLease(self: *CaptureRegistry, owner: Owner, snapshot: usize, epoch: u64, now: u64) Error!Lease {
        if (!self.hasConsumer(owner) or snapshot >= snapshot_capacity or epoch == 0) return error.Invalid;
        var held: usize = 0;
        for (&self.leases) |*value| if (value.id != 0 and value.owner.same(owner)) { held += 1; };
        if (held >= leases_per_consumer) return error.Busy;
        if (self.next_lease == 0) return error.Capacity;
        const entry = for (&self.leases) |*value| { if (value.id == 0) break value; } else return error.Capacity;
        entry.* = .{ .id = self.next_lease, .owner = owner, .snapshot = snapshot, .epoch = epoch, .started_ns = now };
        self.next_lease +%= 1; self.references[snapshot] += 1; self.acquired +|= 1;
        return entry.*;
    }
    pub fn endLease(self: *CaptureRegistry, owner: Owner, id: u64, epoch: u64, now: u64) Error!void {
        const entry = for (&self.leases) |*value| {
            if (value.id != 0 and value.id == id and value.epoch == epoch and value.owner.same(owner)) break value;
        } else return error.Stale;
        self.references[entry.snapshot] -= 1;
        self.max_reader_ns = @max(self.max_reader_ns, now -| entry.started_ns);
        entry.* = .{};
    }
    pub fn stopped(self: *CaptureRegistry, owner: Owner, now: u64) void {
        for (&self.consumers) |*entry| if (entry.count != 0 and entry.owner.same(owner)) {
            self.count -= entry.count; entry.* = .{};
        };
        for (&self.leases) |*entry| if (entry.id != 0 and entry.owner.same(owner)) {
            self.endLease(owner, entry.id, entry.epoch, now) catch unreachable;
        };
    }
};

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
    try checkCaptureOwnership();
    var history: History = .{};
    history.record(1, .{ .w = 100, .h = 80 });
    history.record(2, .{ .x = 3, .y = 4, .w = 5, .h = 6 });
    history.record(3, .{ .x = 40, .y = 30, .w = 7, .h = 8 });

    const rect = history.unionSince(1, 3, 100, 80);
    try @import("std").testing.expectEqual(Rect{ .x = 3, .y = 4, .w = 44, .h = 34 }, rect);
}

fn checkCaptureOwnership() !void {
    const t = @import("std").testing;
    var state: CaptureRegistry = .{};
    const a: Owner = .{ .id = 1, .generation = 1 };
    const b: Owner = .{ .id = 2, .generation = 1 };
    try t.expect(try state.acquire(a) == 1 and try state.acquire(a) == 2 and try state.acquire(b) == 3);
    const first = try state.beginLease(a, 0, 1, 10);
    const second = try state.beginLease(b, 0, 1, 20);
    try t.expect(state.references[0] == 2);
    for (1..leases_per_consumer) |_| _ = try state.beginLease(a, 1, 1, 30);
    try t.expectError(error.Busy, state.beginLease(a, 1, 1, 30));
    try t.expectError(error.Stale, state.endLease(b, first.id, 1, 40));
    try t.expectError(error.Stale, state.endLease(.{ .id = 1, .generation = 2 }, first.id, 1, 40));
    state.stopped(a, 50);
    try t.expect(state.count == 1 and state.references[0] == 1 and state.references[1] == 0 and state.max_reader_ns == 40);
    try t.expect(try state.release(b) == 0 and state.references[0] == 1);
    // A retained snapshot survives the last demand release and publisher epoch.
    try state.endLease(b, second.id, 1, 80);
    try t.expect(state.references[0] == 0 and state.max_reader_ns == 60);
    try t.expectError(error.Stale, state.endLease(b, second.id, 1, 90));
    try t.expectError(error.Stale, state.release(a));
    try t.expect(try state.acquire(.{ .id = 1, .generation = 2 }) == 1);
    const next = try state.beginLease(.{ .id = 1, .generation = 2 }, 2, 2, 100);
    try t.expect(next.id != first.id);
    state.stopped(.{ .id = 1, .generation = 2 }, 110);
    try t.expect(state.count == 0 and state.references[2] == 0);
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
