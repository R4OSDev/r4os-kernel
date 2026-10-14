// Dynamic semaphore ownership is native Rust; actual semaphore queues and
// backing stay resident under the existing runtime owner and unwind token.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted };
const Node = extern struct { key: u64, priority: usize, parent: ?*Node, children: [2]?*Node };
const Tree = extern struct { root: ?*Node = null, xorshift: usize = 0 };
pub const Stats = extern struct { epoch: u64 = 0, records: u64 = 0, private: u64 = 0, creates: u64 = 0, create_failures: u64 = 0, destroys: u64 = 0, destroy_failures: u64 = 0, pending_creates: u32 = 0, pending_destroys: u32 = 0, acquires: u32 = 0, closing: bool = false };
pub const Create = extern struct { epoch: u64, handle: u64, owner: u32, active: bool = true };
const Owner = extern struct { stats: Stats = .{}, tree: Tree = .{} };
extern fn r4os_rust_semaphore_owner_mutate(base: [*]Owner, cap: usize, id: u32, epoch: u64, op: u8) u8;
extern fn r4os_rust_semaphore_owner_stats(base: [*]Owner, cap: usize, id: u32, out: *Stats) u8;
extern fn r4os_rust_semaphore_owner_begin(base: [*]Owner, cap: usize, next: *u64, id: u32, out: *Create) u8;
extern fn r4os_rust_semaphore_owner_abort(base: [*]Owner, cap: usize, c: *Create) u8;
extern fn r4os_rust_semaphore_owner_adopt(base: [*]Owner, cap: usize, c: *Create, record: *anyopaque, visible: *bool) u8;
extern fn r4os_rust_semaphore_owner_lookup(base: [*]Owner, cap: usize, id: u32, handle: u64, op: u8, out: *?*anyopaque) u8;
extern fn r4os_rust_semaphore_owner_acquired(base: [*]Owner, cap: usize, record: *anyopaque) u8;
extern fn r4os_rust_semaphore_owner_destroy(base: [*]Owner, cap: usize, id: u32, handle: u64, out: *anyopaque) u8;
extern fn r4os_rust_semaphore_owner_finish_destroy(base: [*]Owner, cap: usize, ticket: *anyopaque, success: bool) u8;
extern fn r4os_rust_semaphore_owner_first(base: [*]Owner, cap: usize, id: u32, out: *?*anyopaque) u8;
comptime {
    if (@sizeOf(Stats) != 72 or @sizeOf(Owner) != 88 or @sizeOf(Create) != 24) @compileError("Native semaphore metadata C layout drift");
}
fn checked(code: u8) Error!void {
    return switch (code) {
        0 => {},
        1 => error.Invalid,
        2 => error.Owner,
        3 => error.Stale,
        4 => error.Closed,
        5 => error.Busy,
        6 => error.Exhausted,
        else => @panic("Invalid native semaphore owner status"),
    };
}
pub fn State(comptime capacity: usize, comptime Payload: type) type {
    return extern struct {
        const Self = @This();
        pub const Record = extern struct { node: Node = undefined, epoch: u64 = 0, handle: u64 = 0, owner: u32 = 0, acquires: u32 = 0, private: bool = true, padding: [7]u8 = @splat(0), payload: Payload };
        pub const Destroy = extern struct { record: *Record, epoch: u64, handle: u64, owner: u32, private: bool, active: bool = true };
        comptime {
            if (@offsetOf(Record, "payload") != 72 or @offsetOf(Record, "private") != 64 or @sizeOf(Destroy) != 32) @compileError("Native semaphore record prefix drift");
        }
        owners: [capacity]Owner = .{Owner{}} ** capacity,
        next_handle: u64 = 1,
        pub fn bind(self: *Self, id: u32, epoch: u64) Error!void {
            try checked(r4os_rust_semaphore_owner_mutate(&self.owners, capacity, id, epoch, 0));
        }
        pub fn stats(self: *Self, id: u32) Error!Stats {
            var out: Stats = undefined;
            try checked(r4os_rust_semaphore_owner_stats(&self.owners, capacity, id, &out));
            return out;
        }
        pub fn begin(self: *Self, id: u32) Error!Create {
            var out: Create = undefined;
            try checked(r4os_rust_semaphore_owner_begin(&self.owners, capacity, &self.next_handle, id, &out));
            return out;
        }
        pub fn abort(self: *Self, c: *Create) Error!void {
            try checked(r4os_rust_semaphore_owner_abort(&self.owners, capacity, c));
        }
        pub fn adopt(self: *Self, c: *Create, r: *Record) Error!bool {
            var publish_visible = false;
            try checked(r4os_rust_semaphore_owner_adopt(&self.owners, capacity, c, r, &publish_visible));
            return publish_visible;
        }
        fn record(self: *Self, id: u32, handle: u64, op: u8) Error!*Record {
            var out: ?*anyopaque = null;
            try checked(r4os_rust_semaphore_owner_lookup(&self.owners, capacity, id, handle, op, &out));
            return @ptrCast(@alignCast(out.?));
        }
        pub fn lookup(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 0);
        }
        pub fn visible(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 1);
        }
        pub fn acquire(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 2);
        }
        pub fn acquired(self: *Self, r: *Record) Error!void {
            try checked(r4os_rust_semaphore_owner_acquired(&self.owners, capacity, r));
        }
        pub fn beginDestroy(self: *Self, id: u32, handle: u64) Error!Destroy {
            var out: Destroy = undefined;
            try checked(r4os_rust_semaphore_owner_destroy(&self.owners, capacity, id, handle, &out));
            return out;
        }
        pub fn finishDestroy(self: *Self, t: *Destroy, success: bool) Error!void {
            try checked(r4os_rust_semaphore_owner_finish_destroy(&self.owners, capacity, t, success));
        }
        pub fn first(self: *Self, id: u32) Error!?*Record {
            var out: ?*anyopaque = null;
            try checked(r4os_rust_semaphore_owner_first(&self.owners, capacity, id, &out));
            return if (out) |r| @ptrCast(@alignCast(r)) else null;
        }
        pub fn close(self: *Self, id: u32) void {
            _ = r4os_rust_semaphore_owner_mutate(&self.owners, capacity, id, 0, 1);
        }
        pub fn finish(self: *Self, id: u32) Error!void {
            try checked(r4os_rust_semaphore_owner_mutate(&self.owners, capacity, id, 0, 2));
        }
    };
}

test "driver semaphore owner retains waits and failed destruction across close and epochs" {
    const S = State(2, u32);
    var state: S = .{};
    try state.bind(1, 79);
    try state.bind(2, 80);
    var create = try state.begin(1);
    const record = try std.testing.allocator.create(S.Record);
    record.* = .{ .payload = 123 };
    try std.testing.expect(try state.adopt(&create, record));
    const handle = record.handle;
    try std.testing.expectError(error.Stale, state.visible(2, handle));
    try std.testing.expectEqual(record, try state.acquire(1, handle));
    state.close(1);
    try std.testing.expectError(error.Closed, state.begin(1));
    try std.testing.expectError(error.Busy, state.beginDestroy(1, handle));
    try std.testing.expectError(error.Busy, state.finish(1));
    // An existing semaphore remains usable after close, without cancellation.
    _ = try state.acquire(1, handle);
    try std.testing.expectEqual(@as(u32, 2), (try state.stats(1)).acquires);
    try state.acquired(record);
    try state.acquired(record);
    var removal = try state.beginDestroy(1, handle);
    try std.testing.expectError(error.Stale, state.visible(1, handle));
    try std.testing.expectError(error.Busy, state.finish(1));
    try state.finishDestroy(&removal, false);
    try std.testing.expectEqual(@as(u32, 123), (try state.visible(1, handle)).payload);
    try std.testing.expectEqual(@as(u64, 1), (try state.stats(1)).destroy_failures);
    removal = try state.beginDestroy(1, handle);
    std.testing.allocator.destroy(record);
    try state.finishDestroy(&removal, true);
    try state.finish(1);
    try state.bind(1, 81);
    try std.testing.expectError(error.Stale, state.visible(1, handle));
    create = try state.begin(1);
    try std.testing.expect(create.handle > handle and create.epoch == 81);
    try state.abort(&create);
}

test "driver semaphore owner retains close races and rejects exhausted identities" {
    const S = State(1, u8);
    var state: S = .{};
    try state.bind(1, 1);
    var create = try state.begin(1);
    state.close(1);
    try std.testing.expectError(error.Busy, state.finish(1));
    var record: S.Record = .{ .payload = 7 };
    try std.testing.expect(!(try state.adopt(&create, &record)));
    try std.testing.expectError(error.Stale, state.visible(1, record.handle));
    try std.testing.expectEqual(@as(u64, 1), (try state.stats(1)).private);
    var removal = try state.beginDestroy(1, record.handle);
    try state.finishDestroy(&removal, true);
    try state.finish(1);
    try state.bind(1, 2);
    state.next_handle = std.math.maxInt(u64);
    create = try state.begin(1);
    try std.testing.expectEqual(std.math.maxInt(u64), create.handle);
    try state.abort(&create);
    try std.testing.expectError(error.Exhausted, state.begin(1));
    try std.testing.expectError(error.Owner, state.begin(0));
}

test "driver semaphore owner indexes 4096 actual records without a fixed slot pool" {
    const S = State(1, usize);
    var state: S = .{};
    try state.bind(1, 79);
    const records = try std.testing.allocator.alloc(S.Record, 4096);
    defer std.testing.allocator.free(records);
    for (records, 0..) |*record, index| {
        record.* = .{ .payload = index };
        var create = try state.begin(1);
        try std.testing.expect(try state.adopt(&create, record));
    }
    for (0..records.len) |iteration| {
        const index = iteration * 37 % records.len;
        const record = try state.acquire(1, records[index].handle);
        try std.testing.expectEqual(index, record.payload);
        try state.acquired(record);
    }
    state.close(1);
    var removed: usize = 0;
    while (try state.first(1)) |record| {
        var ticket = try state.beginDestroy(1, record.handle);
        try state.finishDestroy(&ticket, true);
        removed += 1;
    }
    try std.testing.expectEqual(records.len, removed);
    try state.finish(1);
}
