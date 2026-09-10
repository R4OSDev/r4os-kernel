// Pure ownership for dedicated R4D threads. Runtime objects, stacks, waits
// and real backing release belong to driver_threads.zig, outside this index.
const std = @import("std");
const Tree = std.Treap(u64, std.math.order);
pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted };
pub const Phase = enum(u32) { preparing = 0, runnable = 1, running = 2, completed = 3 };
pub const Stats = struct {
    epoch: u64 = 0,
    closing: bool = false,
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
};
pub const Create = struct { owner: u32, epoch: u64, handle: u64, active: bool = true };

pub fn State(comptime capacity: usize, comptime Payload: type) type {
    return struct {
        const Self = @This();
        pub const Record = struct {
            node: Tree.Node = undefined,
            owner: u32 = 0,
            epoch: u64 = 0,
            handle: u64 = 0,
            phase: Phase = .preparing,
            private: bool = true,
            stop_requested: bool = false,
            references: u32 = 0,
            result: i32 = 0,
            payload: Payload,
        };
        pub const Removal = struct {
            record: *Record,
            owner: u32,
            epoch: u64,
            handle: u64,
            private: bool,
            active: bool = true,
        };
        const Owner = struct { stats: Stats = .{}, tree: Tree = .{} };
        owners: [capacity]Owner = .{Owner{}} ** capacity,
        next_handle: u64 = 1,

        pub fn bind(self: *Self, owner: u32, epoch: u64) Error!void {
            if (epoch == 0) return error.Invalid;
            const entry = try self.slot(owner);
            if (entry.stats.epoch != 0) {
                if (entry.stats.epoch != epoch) return error.Busy;
                if (entry.stats.closing) return error.Closed;
                return;
            }
            entry.* = .{ .stats = .{ .epoch = epoch } };
        }

        pub fn stats(self: *Self, owner: u32) Error!Stats {
            return (try self.current(owner)).stats;
        }

        pub fn begin(self: *Self, owner: u32) Error!Create {
            const entry = try self.current(owner);
            if (entry.stats.closing) return error.Closed;
            if (self.next_handle == 0) return error.Exhausted;
            if (entry.stats.pending_creates == std.math.maxInt(u32)) return error.Busy;
            const handle = self.next_handle;
            self.next_handle = if (handle == std.math.maxInt(u64)) 0 else handle + 1;
            entry.stats.pending_creates += 1;
            entry.stats.starts +|= 1;
            return .{ .owner = owner, .epoch = entry.stats.epoch, .handle = handle };
        }

        pub fn abort(self: *Self, create: *Create) Error!void {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            std.debug.assert(entry.stats.pending_creates != 0);
            entry.stats.pending_creates -= 1;
            entry.stats.start_failures +|= 1;
            create.active = false;
        }

        // Caller initializes payload before adoption. A create stays pending
        // until actual Task construction and ready publication have resolved.
        pub fn adopt(self: *Self, create: *Create, record: *Record) Error!void {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            std.debug.assert(record.owner == 0 and record.references == 0);
            record.owner = create.owner;
            record.epoch = create.epoch;
            record.handle = create.handle;
            record.phase = .preparing;
            record.private = true;
            var index = entry.tree.getEntryFor(create.handle);
            std.debug.assert(index.node == null);
            index.set(&record.node);
            entry.stats.records += 1;
            entry.stats.private += 1;
            create.active = false;
        }

        // Run under the same runtime boundary as scheduler publication. If
        // ready publication fails, failPublication restores a private record.
        pub fn prepared(self: *Self, record: *Record, task_created: bool, failure: i32) Error!bool {
            const entry = try self.matching(record.owner, record.epoch);
            if (record.phase != .preparing) return error.Stale;
            std.debug.assert(entry.stats.pending_creates != 0);
            entry.stats.pending_creates -= 1;
            if (!task_created or entry.stats.closing) {
                record.phase = .completed;
                record.result = failure;
                record.stop_requested = true;
                entry.stats.start_failures +|= 1;
                entry.stats.completed += 1;
                return false;
            }
            record.phase = .runnable;
            record.private = false;
            entry.stats.private -= 1;
            entry.stats.active += 1;
            return true;
        }

        pub fn failPublication(self: *Self, record: *Record, result: i32) Error!void {
            const entry = try self.matching(record.owner, record.epoch);
            if (record.phase != .runnable) return error.Stale;
            record.phase = .completed;
            record.result = result;
            record.private = true;
            record.stop_requested = true;
            entry.stats.active -= 1;
            entry.stats.completed += 1;
            entry.stats.private += 1;
            entry.stats.start_failures +|= 1;
        }

        pub fn enter(self: *Self, record: *Record) Error!bool {
            _ = try self.matching(record.owner, record.epoch);
            if (record.phase != .runnable) return error.Stale;
            record.phase = .running;
            return !(try self.stopping(record));
        }

        pub fn complete(self: *Self, record: *Record, result: i32) Error!void {
            const entry = try self.matching(record.owner, record.epoch);
            if (record.phase != .running) return error.Stale;
            record.phase = .completed;
            record.result = result;
            entry.stats.active -= 1;
            entry.stats.completed += 1;
        }

        pub fn lookup(self: *Self, owner: u32, handle: u64) Error!*Record {
            if (handle == 0) return error.Invalid;
            const entry = try self.current(owner);
            const node = entry.tree.getEntryFor(handle).node orelse return error.Stale;
            const record: *Record = @fieldParentPtr("node", node);
            std.debug.assert(record.owner == owner and record.epoch == entry.stats.epoch);
            return record;
        }

        pub fn stopping(self: *Self, record: *const Record) Error!bool {
            const entry = try self.matching(record.owner, record.epoch);
            return record.stop_requested or entry.stats.closing;
        }

        pub fn stop(self: *Self, owner: u32, handle: u64) Error!*Record {
            const record = try self.lookup(owner, handle);
            record.stop_requested = true;
            return record;
        }

        pub fn retain(self: *Self, owner: u32, handle: u64) Error!*Record {
            const record = try self.lookup(owner, handle);
            const entry = try self.matching(owner, record.epoch);
            if (record.phase == .preparing or record.references == std.math.maxInt(u32) or entry.stats.leases == std.math.maxInt(u32)) return error.Busy;
            record.references += 1;
            entry.stats.leases += 1;
            return record;
        }

        pub fn unretain(self: *Self, record: *Record) Error!void {
            const entry = try self.matching(record.owner, record.epoch);
            if (record.references == 0) return error.Stale;
            record.references -= 1;
            entry.stats.leases -= 1;
        }

        pub fn beginRemove(self: *Self, owner: u32, handle: u64) Error!Removal {
            const record = try self.lookup(owner, handle);
            const entry = try self.matching(owner, record.epoch);
            if (record.phase != .completed or record.references != 0 or entry.stats.pending_releases == std.math.maxInt(u32)) return error.Busy;
            const result: Removal = .{ .record = record, .owner = owner, .epoch = record.epoch, .handle = handle, .private = record.private };
            var index = entry.tree.getEntryForExisting(&record.node);
            index.set(null);
            entry.stats.pending_releases += 1;
            return result;
        }

        // Caller confirms actual Task retirement and then backing free. Never
        // dereference a successfully freed record; failure retains everything.
        pub fn finishRemove(self: *Self, removal: *Removal, success: bool) Error!void {
            if (!removal.active) return error.Stale;
            const entry = try self.matching(removal.owner, removal.epoch);
            std.debug.assert(entry.stats.pending_releases != 0);
            entry.stats.pending_releases -= 1;
            if (success) {
                entry.stats.records -= 1;
                entry.stats.completed -= 1;
                if (removal.private) entry.stats.private -= 1;
                entry.stats.releases +|= 1;
            } else {
                var index = entry.tree.getEntryFor(removal.handle);
                std.debug.assert(index.node == null);
                index.set(&removal.record.node);
                entry.stats.release_failures +|= 1;
            }
            removal.active = false;
        }

        pub fn close(self: *Self, owner: u32) void {
            const entry = self.current(owner) catch return;
            entry.stats.closing = true;
        }

        // Numeric cursor survives releases and allows one bounded record
        // action per runtime-owner acquisition instead of a lock-spanning walk.
        pub fn after(self: *Self, owner: u32, cursor: u64) Error!?*Record {
            const entry = try self.current(owner);
            var node = entry.tree.root;
            var found: ?*Tree.Node = null;
            while (node) |current_node| {
                if (current_node.key > cursor) {
                    found = current_node;
                    node = current_node.children[0];
                } else node = current_node.children[1];
            }
            return if (found) |value| @fieldParentPtr("node", value) else null;
        }

        pub fn finish(self: *Self, owner: u32) Error!void {
            const entry = try self.current(owner);
            if (!entry.stats.closing or entry.stats.records != 0 or entry.stats.pending_creates != 0 or entry.stats.pending_releases != 0 or entry.stats.leases != 0) return error.Busy;
            entry.* = .{};
        }

        fn slot(self: *Self, owner: u32) Error!*Owner {
            if (owner == 0 or owner > capacity) return error.Owner;
            return &self.owners[owner - 1];
        }
        fn current(self: *Self, owner: u32) Error!*Owner {
            const entry = try self.slot(owner);
            if (entry.stats.epoch == 0) return error.Stale;
            return entry;
        }
        fn matching(self: *Self, owner: u32, epoch: u64) Error!*Owner {
            const entry = try self.current(owner);
            if (entry.stats.epoch != epoch) return error.Stale;
            return entry;
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
