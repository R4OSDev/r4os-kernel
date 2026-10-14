// Rust implements exact R4D thread admission, generation, leases and cleanup.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted };
pub const Phase = enum(u32) { preparing = 0, runnable = 1, running = 2, completed = 3 };
const Node = extern struct { key: u64, priority: usize, parent: ?*Node, children: [2]?*Node };
const Tree = extern struct { root: ?*Node = null, xorshift: usize = 0 };
pub const Stats = extern struct {
    epoch: u64 = 0,
    records: u64 = 0,
    active: u64 = 0,
    completed: u64 = 0,
    private: u64 = 0,
    starts: u64 = 0,
    start_failures: u64 = 0,
    releases: u64 = 0,
    release_failures: u64 = 0,
    pending_creates: u32 = 0,
    pending_releases: u32 = 0,
    leases: u32 = 0,
    closing: bool = false,
};
pub const Create = extern struct { epoch: u64, handle: u64, owner: u32, active: bool = true };
const NativeOwner = extern struct { stats: Stats = .{}, tree: Tree = .{} };
extern fn r4os_rust_thread_owner_mutate(base: [*]NativeOwner, cap: usize, id: u32, epoch: u64, op: u8) u8;
extern fn r4os_rust_thread_owner_stats(base: [*]NativeOwner, cap: usize, id: u32, out: *Stats) u8;
extern fn r4os_rust_thread_owner_begin(base: [*]NativeOwner, cap: usize, next: *u64, id: u32, out: *Create) u8;
extern fn r4os_rust_thread_owner_abort(base: [*]NativeOwner, cap: usize, c: *Create) u8;
extern fn r4os_rust_thread_owner_adopt(base: [*]NativeOwner, cap: usize, c: *Create, r: *anyopaque) u8;
extern fn r4os_rust_thread_owner_record(base: [*]NativeOwner, cap: usize, r: *anyopaque, op: u8, created: bool, result: i32, out: ?*bool) u8;
extern fn r4os_rust_thread_owner_lookup(base: [*]NativeOwner, cap: usize, id: u32, handle: u64, op: u8, out: *?*anyopaque) u8;
extern fn r4os_rust_thread_owner_remove(base: [*]NativeOwner, cap: usize, id: u32, handle: u64, out: *anyopaque) u8;
extern fn r4os_rust_thread_owner_finish_remove(base: [*]NativeOwner, cap: usize, t: *anyopaque, success: bool) u8;
extern fn r4os_rust_thread_owner_after(base: [*]NativeOwner, cap: usize, id: u32, cursor: u64, out: *?*anyopaque) u8;
comptime {
    if (@sizeOf(Stats) != 88 or @sizeOf(NativeOwner) != 104 or @sizeOf(Create) != 24) @compileError("Native thread metadata C layout drift");
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
        else => @panic("Invalid native thread owner status"),
    };
}
pub fn State(comptime capacity: usize, comptime Payload: type) type {
    return extern struct {
        const Self = @This();
        pub const Record = extern struct {
            node: Node = undefined,
            epoch: u64 = 0,
            handle: u64 = 0,
            owner: u32 = 0,
            phase: Phase = .preparing,
            references: u32 = 0,
            result: i32 = 0,
            private: bool = true,
            stop_requested: bool = false,
            padding: [6]u8 = @splat(0),
            payload: Payload,
        };
        pub const Removal = extern struct { record: *Record, epoch: u64, handle: u64, owner: u32, private: bool, active: bool = true };
        comptime {
            if (@offsetOf(Record, "payload") != 80 or @offsetOf(Record, "private") != 72 or @sizeOf(Removal) != 32) @compileError("Native thread record prefix drift");
        }
        owners: [capacity]NativeOwner = .{NativeOwner{}} ** capacity,
        next_handle: u64 = 1,
        pub fn bind(self: *Self, id: u32, epoch: u64) Error!void {
            try checked(r4os_rust_thread_owner_mutate(&self.owners, capacity, id, epoch, 0));
        }
        pub fn stats(self: *Self, id: u32) Error!Stats {
            var out: Stats = undefined;
            try checked(r4os_rust_thread_owner_stats(&self.owners, capacity, id, &out));
            return out;
        }
        pub fn begin(self: *Self, id: u32) Error!Create {
            var out: Create = undefined;
            try checked(r4os_rust_thread_owner_begin(&self.owners, capacity, &self.next_handle, id, &out));
            return out;
        }
        pub fn abort(self: *Self, c: *Create) Error!void {
            try checked(r4os_rust_thread_owner_abort(&self.owners, capacity, c));
        }
        pub fn adopt(self: *Self, c: *Create, r: *Record) Error!void {
            try checked(r4os_rust_thread_owner_adopt(&self.owners, capacity, c, r));
        }
        pub fn prepared(self: *Self, r: *Record, created: bool, failure: i32) Error!bool {
            var out = false;
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, r, 0, created, failure, &out));
            return out;
        }
        pub fn failPublication(self: *Self, r: *Record, result: i32) Error!void {
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, r, 1, false, result, null));
        }
        pub fn enter(self: *Self, r: *Record) Error!bool {
            var out = false;
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, r, 2, false, 0, &out));
            return out;
        }
        pub fn complete(self: *Self, r: *Record, result: i32) Error!void {
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, r, 3, false, result, null));
        }
        pub fn stopping(self: *Self, r: *const Record) Error!bool {
            var out = false;
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, @constCast(r), 4, false, 0, &out));
            return out;
        }
        fn record(self: *Self, id: u32, handle: u64, op: u8) Error!*Record {
            var out: ?*anyopaque = null;
            try checked(r4os_rust_thread_owner_lookup(&self.owners, capacity, id, handle, op, &out));
            return @ptrCast(@alignCast(out.?));
        }
        pub fn lookup(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 0);
        }
        pub fn stop(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 1);
        }
        pub fn retain(self: *Self, id: u32, handle: u64) Error!*Record {
            return self.record(id, handle, 2);
        }
        pub fn unretain(self: *Self, r: *Record) Error!void {
            try checked(r4os_rust_thread_owner_record(&self.owners, capacity, r, 5, false, 0, null));
        }
        pub fn beginRemove(self: *Self, id: u32, handle: u64) Error!Removal {
            var out: Removal = undefined;
            try checked(r4os_rust_thread_owner_remove(&self.owners, capacity, id, handle, &out));
            return out;
        }
        pub fn finishRemove(self: *Self, t: *Removal, success: bool) Error!void {
            try checked(r4os_rust_thread_owner_finish_remove(&self.owners, capacity, t, success));
        }
        pub fn close(self: *Self, id: u32) void {
            _ = r4os_rust_thread_owner_mutate(&self.owners, capacity, id, 0, 1);
        }
        pub fn after(self: *Self, id: u32, cursor: u64) Error!?*Record {
            var out: ?*anyopaque = null;
            try checked(r4os_rust_thread_owner_after(&self.owners, capacity, id, cursor, &out));
            return if (out) |r| @ptrCast(@alignCast(r)) else null;
        }
        pub fn finish(self: *Self, id: u32) Error!void {
            try checked(r4os_rust_thread_owner_mutate(&self.owners, capacity, id, 0, 2));
        }
    };
}

test "driver threads preserve start, stop, join leases, release failure and exact owner epoch" {
    const t = std.testing;
    const Owner = State(2, u64);
    var state: Owner = .{};
    try state.bind(1, 41);
    try state.bind(2, 42);
    var ticket = try state.begin(1);
    const record = try t.allocator.create(Owner.Record);
    record.* = .{ .payload = 0x123456789a };
    try state.adopt(&ticket, record);
    try t.expectError(error.Busy, state.retain(1, record.handle));
    try t.expect(try state.prepared(record, true, -1));
    const handle = record.handle;
    try t.expectError(error.Stale, state.lookup(2, handle));
    try t.expect(try state.enter(record));
    _ = try state.stop(1, handle);
    try t.expect(try state.stopping(record));
    const lease = try state.retain(1, handle);
    try state.complete(record, 73);
    try t.expectError(error.Busy, state.beginRemove(1, handle));
    try t.expectEqual(@as(i32, 73), lease.result);
    try state.unretain(lease);
    var removal = try state.beginRemove(1, handle);
    state.close(1);
    try t.expectError(error.Busy, state.finish(1));
    try t.expectError(error.Closed, state.begin(1));
    try state.finishRemove(&removal, false);
    try t.expectEqual(@as(u64, 0x123456789a), (try state.lookup(1, handle)).payload);
    removal = try state.beginRemove(1, handle);
    t.allocator.destroy(record);
    try state.finishRemove(&removal, true);
    try state.finish(1);
    try state.bind(1, 43);
    try t.expectError(error.Stale, state.lookup(1, handle));
    var later = try state.begin(1);
    try t.expect(later.handle > handle);
    try state.abort(&later);
}

test "driver threads retain close-race and failed-publication backing without exposing a handle" {
    const t = std.testing;
    const Owner = State(1, void);
    var state: Owner = .{};
    try state.bind(1, 1);
    var ticket = try state.begin(1);
    state.close(1);
    try t.expectError(error.Busy, state.finish(1));
    var record: Owner.Record = .{ .payload = {} };
    try state.adopt(&ticket, &record);
    try t.expect(!(try state.prepared(&record, true, -7)));
    try t.expect(record.private and record.phase == .completed);
    var removal = try state.beginRemove(1, record.handle);
    try state.finishRemove(&removal, true);
    try state.finish(1);
    try state.bind(1, 2);
    ticket = try state.begin(1);
    record = .{ .payload = {} };
    try state.adopt(&ticket, &record);
    try t.expect(try state.prepared(&record, true, -1));
    try state.failPublication(&record, -9);
    try t.expectEqual(@as(u64, 0), (try state.stats(1)).active);
    try t.expectEqual(@as(u64, 1), (try state.stats(1)).private);
    removal = try state.beginRemove(1, record.handle);
    try state.finishRemove(&removal, true);
    state.close(1);
    try state.finish(1);
    try state.bind(1, 3);
    state.next_handle = std.math.maxInt(u64);
    ticket = try state.begin(1);
    try state.abort(&ticket);
    try t.expectError(error.Exhausted, state.begin(1));
}

test "driver thread metadata scales with resident records and cursor survives deletion" {
    const t = std.testing;
    const Owner = State(1, u32);
    var state: Owner = .{};
    try state.bind(1, 5);
    const records = try t.allocator.alloc(Owner.Record, 4096);
    defer t.allocator.free(records);
    for (records, 0..) |*record, index| {
        record.* = .{ .payload = @intCast(index) };
        var ticket = try state.begin(1);
        try state.adopt(&ticket, record);
        try t.expect(try state.prepared(record, true, -1));
        try t.expect(try state.enter(record));
        try state.complete(record, @intCast(index));
    }
    state.close(1);
    var cursor: u64 = 0;
    var seen: usize = 0;
    while (try state.after(1, cursor)) |record| {
        cursor = record.handle;
        try t.expectEqual(@as(u32, @intCast(seen)), record.payload);
        var removal = try state.beginRemove(1, cursor);
        try state.finishRemove(&removal, true);
        seen += 1;
    }
    try t.expectEqual(records.len, seen);
    try state.finish(1);
}
