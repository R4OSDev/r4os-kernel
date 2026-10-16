//! Projection of the sole native bounded capture/dirty-history models.
pub const history_capacity: usize = 64;
pub const snapshot_capacity = 3;
pub const consumer_capacity = 64;
pub const leases_per_consumer = 8;
pub const max_frame_bytes = 64 * 1024 * 1024;
pub const Owner = extern struct {
    id: u32 = 0, generation: u64 = 0,
    pub fn same(self: Owner, other: Owner) bool { return self.id == other.id and self.generation == other.generation; }
};
const Consumer = extern struct { owner: Owner = .{}, count: u32 = 0 };
pub const Lease = extern struct { id: u64 = 0, owner: Owner = .{}, snapshot: usize = 0, epoch: u64 = 0, started_ns: u64 = 0 };
const Args = extern struct { owner: Owner = .{}, snapshot: usize = 0, id: u64 = 0, epoch: u64 = 0, now: u64 = 0 };
extern fn r4os_rust_remote_capture(u32, *CaptureRegistry, *const Args, ?*anyopaque) i32;
pub const CaptureRegistry = extern struct {
    consumers: [consumer_capacity]Consumer = @splat(.{}), leases: [consumer_capacity]Lease = @splat(.{}),
    references: [snapshot_capacity]u32 = @splat(0), count: u32 = 0, next_lease: u64 = 1, acquired: u64 = 0, max_reader_ns: u64 = 0,
    pub const Error = error{ Invalid, Capacity, Stale, Busy };
    fn checked(code: i32) Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 6 => error.Capacity, 7 => error.Stale, 8 => error.Busy, else => unreachable }; }
    fn call(self: *CaptureRegistry, op: u32, args: Args) Error!void { try checked(r4os_rust_remote_capture(op, self, &args, null)); }
    fn result(self: *CaptureRegistry, comptime T: type, op: u32, args: Args) Error!T {
        var out: T = undefined; try checked(r4os_rust_remote_capture(op, self, &args, &out)); return out;
    }
    pub fn acquire(self: *CaptureRegistry, owner: Owner) Error!u32 { return self.result(u32, 0, .{ .owner = owner }); }
    pub fn release(self: *CaptureRegistry, owner: Owner) Error!u32 { return self.result(u32, 1, .{ .owner = owner }); }
    pub fn hasConsumer(self: *const CaptureRegistry, owner: Owner) bool { return @constCast(self).result(bool, 2, .{ .owner = owner }) catch unreachable; }
    pub fn beginLease(self: *CaptureRegistry, owner: Owner, snapshot: usize, epoch: u64, now: u64) Error!Lease { return self.result(Lease, 3, .{ .owner = owner, .snapshot = snapshot, .epoch = epoch, .now = now }); }
    pub fn endLease(self: *CaptureRegistry, owner: Owner, id: u64, epoch: u64, now: u64) Error!void { try self.call(4, .{ .owner = owner, .id = id, .epoch = epoch, .now = now }); }
    pub fn stopped(self: *CaptureRegistry, owner: Owner, now: u64) void { self.call(5, .{ .owner = owner, .now = now }) catch unreachable; }
};
pub const Rect = extern struct { x: u32 = 0, y: u32 = 0, w: u32 = 0, h: u32 = 0,
    pub fn empty(self: Rect) bool { return self.w == 0 or self.h == 0; }
};
const Change = extern struct { revision: u32 = 0, rect: Rect = .{} };
const HistoryArgs = extern struct { revision: u32 = 0, current: u32 = 0, rect: Rect = .{}, other: Rect = .{}, width: u32 = 0, height: u32 = 0 };
extern fn r4os_rust_remote_history(u32, ?*History, *const HistoryArgs, ?*Rect) void;
pub const History = extern struct {
    changes: [history_capacity]Change = @splat(.{}), count: usize = 0,
    pub fn reset(self: *History) void { r4os_rust_remote_history(0, self, &.{}, null); }
    pub fn record(self: *History, revision: u32, rect: Rect) void { r4os_rust_remote_history(1, self, &.{ .revision = revision, .rect = rect }, null); }
    pub fn unionSince(self: *const History, last_revision: u32, current_revision: u32, width: u32, height: u32) Rect {
        var out: Rect = undefined; r4os_rust_remote_history(2, @constCast(self), &.{ .revision = last_revision, .current = current_revision, .width = width, .height = height }, &out); return out;
    }
};
pub fn merge(a: Rect, b: Rect, width: u32, height: u32) Rect {
    var out: Rect = undefined; r4os_rust_remote_history(3, null, &.{ .rect = a, .other = b, .width = width, .height = height }, &out); return out;
}
comptime { if (@sizeOf(Owner) != 16 or @sizeOf(Lease) != 48 or @sizeOf(CaptureRegistry) != 4648 or @sizeOf(History) != 1288) @compileError("Native capture/history C layout"); }

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
