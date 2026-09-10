// Dynamic R4D semaphore ownership. Actual queues, heap backing and scheduler
// waits belong to driver_semaphores.zig; this index mutates resident metadata.
const std = @import("std");
const Tree = std.Treap(u64, std.math.order);
pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted };
pub const Stats = struct {
    epoch: u64 = 0,
    closing: bool = false,
    records: u64 = 0,
    private: u64 = 0,
    creates: u64 = 0,
    create_failures: u64 = 0,
    destroys: u64 = 0,
    destroy_failures: u64 = 0,
    pending_creates: u32 = 0,
    pending_destroys: u32 = 0,
    acquires: u32 = 0,
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
            private: bool = true,
            acquires: u32 = 0,
            payload: Payload,
        };
        pub const Destroy = struct {
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
            if (entry.stats.epoch != 0) return error.Busy;
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
            entry.stats.creates +|= 1;
            return .{ .owner = owner, .epoch = entry.stats.epoch, .handle = handle };
        }
        pub fn abort(self: *Self, create: *Create) Error!void {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            std.debug.assert(entry.stats.pending_creates != 0);
            entry.stats.pending_creates -= 1;
            entry.stats.create_failures +|= 1;
            create.active = false;
        }
        pub fn adopt(self: *Self, create: *Create, record: *Record) Error!bool {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            std.debug.assert(record.owner == 0 and entry.stats.pending_creates != 0);
            record.owner = create.owner;
            record.epoch = create.epoch;
            record.handle = create.handle;
            record.private = entry.stats.closing;
            record.acquires = 0;
            var index = entry.tree.getEntryFor(create.handle);
            std.debug.assert(index.node == null);
            index.set(&record.node);
            entry.stats.records += 1;
            entry.stats.pending_creates -= 1;
            if (record.private) {
                entry.stats.private += 1;
                entry.stats.create_failures +|= 1;
            }
            create.active = false;
            return !record.private;
        }
        pub fn lookup(self: *Self, owner: u32, handle: u64) Error!*Record {
            if (handle == 0) return error.Invalid;
            const entry = try self.current(owner);
            const node = entry.tree.getEntryFor(handle).node orelse return error.Stale;
            const record: *Record = @fieldParentPtr("node", node);
            std.debug.assert(record.owner == owner and record.epoch == entry.stats.epoch);
            return record;
        }
        pub fn visible(self: *Self, owner: u32, handle: u64) Error!*Record {
            const record = try self.lookup(owner, handle);
            return if (record.private) error.Stale else record;
        }
        // Close stops creation, not existing semaphore operations. In
        // particular it never grants a permit or aborts uninterruptible down.
        pub fn acquire(self: *Self, owner: u32, handle: u64) Error!*Record {
            const record = try self.visible(owner, handle);
            const entry = try self.matching(owner, record.epoch);
            if (record.acquires == std.math.maxInt(u32) or entry.stats.acquires == std.math.maxInt(u32)) return error.Busy;
            record.acquires += 1;
            entry.stats.acquires += 1;
            return record;
        }
        pub fn acquired(self: *Self, record: *Record) Error!void {
            const entry = try self.matching(record.owner, record.epoch);
            if (record.acquires == 0) return error.Stale;
            record.acquires -= 1;
            entry.stats.acquires -= 1;
        }
        pub fn beginDestroy(self: *Self, owner: u32, handle: u64) Error!Destroy {
            const record = try self.lookup(owner, handle);
            const entry = try self.matching(owner, record.epoch);
            if (record.acquires != 0 or entry.stats.pending_destroys == std.math.maxInt(u32)) return error.Busy;
            const ticket: Destroy = .{ .record = record, .owner = owner, .epoch = record.epoch, .handle = handle, .private = record.private };
            var index = entry.tree.getEntryForExisting(&record.node);
            index.set(null);
            entry.stats.pending_destroys += 1;
            return ticket;
        }
        // A successful caller free already invalidated record. Only copied
        // accounting may be read; failure reinserts the exact live identity.
        pub fn finishDestroy(self: *Self, ticket: *Destroy, success: bool) Error!void {
            if (!ticket.active) return error.Stale;
            const entry = try self.matching(ticket.owner, ticket.epoch);
            std.debug.assert(entry.stats.pending_destroys != 0);
            entry.stats.pending_destroys -= 1;
            if (success) {
                entry.stats.records -= 1;
                if (ticket.private) entry.stats.private -= 1;
                entry.stats.destroys +|= 1;
            } else {
                var index = entry.tree.getEntryFor(ticket.handle);
                std.debug.assert(index.node == null);
                index.set(&ticket.record.node);
                entry.stats.destroy_failures +|= 1;
            }
            ticket.active = false;
        }
        pub fn first(self: *Self, owner: u32) Error!?*Record {
            const entry = try self.current(owner);
            var node = entry.tree.root orelse return null;
            while (node.children[0]) |child| node = child;
            return @fieldParentPtr("node", node);
        }
        pub fn close(self: *Self, owner: u32) void {
            const entry = self.current(owner) catch return;
            entry.stats.closing = true;
        }
        pub fn finish(self: *Self, owner: u32) Error!void {
            const entry = try self.current(owner);
            if (!entry.stats.closing or entry.stats.records != 0 or entry.stats.pending_creates != 0 or entry.stats.pending_destroys != 0 or entry.stats.acquires != 0) return error.Busy;
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
