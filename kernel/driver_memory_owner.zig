// Resident CPU allocations owned by an actual R4D start. This component only
// mutates metadata. The runtime wrapper allocates/frees backing outside its
// owner boundary and calls finishRelease only after the backend returns.
const std = @import("std");

pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted, Overflow };
const Tree = std.Treap(u64, std.math.order);

// One record lives in the same resident heap allocation as its CPU payload.
// A rejected heap.free leaves the complete allocation intact; a successful
// free may retain VM tail pages internally, under the kernel heap's ownership.
pub const Record = struct {
    node: Tree.Node = undefined,
    epoch: u64,
    requested_bytes: u64,
    heap_bytes: usize,
    payload_offset: usize,
    alignment: u32,
    private: bool,
    release_failed: bool = false,
};

pub const Layout = struct {
    requested_bytes: u64,
    heap_bytes: usize,
    payload_offset: usize,
    alignment: u32,

    pub fn init(bytes: u64, alignment: u32) Error!Layout {
        if (bytes == 0 or alignment == 0 or !std.math.isPowerOfTwo(alignment)) return error.Invalid;
        const normalized = @max(alignment, 16);
        const offset = std.mem.alignForward(usize, @sizeOf(Record), normalized);
        const total = std.math.add(usize, @intCast(bytes), offset) catch return error.Overflow;
        return .{ .requested_bytes = bytes, .heap_bytes = total, .payload_offset = offset, .alignment = normalized };
    }
};

pub const Allocation = struct { handle: u64, address: usize, bytes: u64, alignment: u32 };
pub const Create = struct { owner: u32, epoch: u64, handle: u64, layout: Layout, active: bool = true };
pub const Published = struct { allocation: Allocation, visible: bool };
pub const Release = struct {
    owner: u32,
    epoch: u64,
    handle: u64,
    record: *Record,
    bytes: u64,
    heap_bytes: usize,
    private: bool,
    active: bool = true,

    pub fn backing(self: Release) []u8 {
        std.debug.assert(self.active);
        const address: [*]u8 = @ptrCast(self.record);
        return address[0..self.heap_bytes];
    }
};

pub const Stats = struct {
    epoch: u64 = 0,
    closing: bool = false,
    allocations: u64 = 0,
    bytes: u64 = 0,
    backing_bytes: u64 = 0,
    peak_bytes: u64 = 0,
    private_allocations: u64 = 0,
    pending_creates: u32 = 0,
    pending_releases: u32 = 0,
    allocation_calls: u64 = 0,
    allocation_failures: u64 = 0,
    releases: u64 = 0,
    release_failures: u64 = 0,
};

// Only the existing driver registry bounds owner slots. Allocations have no
// separate fixed slot pool: records grow with real resident heap storage.
// Intrusive treaps provide expected logarithmic lookup without allocating
// index storage or moving a record while its payload is borrowed.
pub fn State(comptime owner_capacity: usize) type {
    return struct {
        const Self = @This();
        const Owner = struct { stats: Stats = .{}, tree: Tree = .{} };
        owners: [owner_capacity]Owner = .{Owner{}} ** owner_capacity,
        next_handle: u64 = 1,

        pub fn activate(self: *Self, id: u32, epoch: u64) Error!void {
            if (epoch == 0) return error.Invalid;
            const entry = try self.slot(id);
            if (entry.stats.epoch != 0) {
                if (entry.stats.epoch != epoch) return error.Busy;
                if (entry.stats.closing) return error.Closed;
                return;
            }
            entry.* = .{ .stats = .{ .epoch = epoch } };
        }

        pub fn snapshot(self: *Self, id: u32) Error!Stats {
            return (try self.current(id)).stats;
        }

        pub fn begin(self: *Self, id: u32, layout: Layout) Error!Create {
            const entry = try self.current(id);
            if (entry.stats.closing) return error.Closed;
            if (self.next_handle == 0) return error.Exhausted;
            if (entry.stats.pending_creates == std.math.maxInt(u32)) return error.Busy;
            const handle = self.next_handle;
            self.next_handle = if (handle == std.math.maxInt(u64)) 0 else handle + 1;
            entry.stats.pending_creates += 1;
            entry.stats.allocation_calls +|= 1;
            return .{ .owner = id, .epoch = entry.stats.epoch, .handle = handle, .layout = layout };
        }

        pub fn abort(self: *Self, create: *Create) Error!void {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            std.debug.assert(entry.stats.pending_creates != 0);
            entry.stats.pending_creates -= 1;
            entry.stats.allocation_failures +|= 1;
            create.active = false;
        }

        // A close racing with the actual heap allocation cannot lose its
        // backing. Adopt it privately, refuse the caller's publication and
        // let owner cleanup reclaim it after callbacks and DMA have drained.
        pub fn adopt(self: *Self, create: *Create, backing: []u8) Error!Published {
            if (!create.active) return error.Stale;
            const entry = try self.matching(create.owner, create.epoch);
            const layout = create.layout;
            std.debug.assert(backing.len == layout.heap_bytes and @intFromPtr(backing.ptr) % layout.alignment == 0);
            const record: *Record = @ptrCast(@alignCast(backing.ptr));
            record.* = .{
                .epoch = create.epoch,
                .requested_bytes = layout.requested_bytes,
                .heap_bytes = layout.heap_bytes,
                .payload_offset = layout.payload_offset,
                .alignment = layout.alignment,
                .private = entry.stats.closing,
            };
            var target = entry.tree.getEntryFor(create.handle);
            std.debug.assert(target.node == null);
            target.set(&record.node);
            entry.stats.pending_creates -= 1;
            entry.stats.allocations += 1;
            entry.stats.bytes += layout.requested_bytes;
            entry.stats.backing_bytes += layout.heap_bytes;
            entry.stats.peak_bytes = @max(entry.stats.peak_bytes, entry.stats.bytes);
            if (record.private) {
                entry.stats.private_allocations += 1;
                entry.stats.allocation_failures +|= 1;
            }
            create.active = false;
            return .{ .allocation = .{ .handle = create.handle, .address = @intFromPtr(backing.ptr) + layout.payload_offset, .bytes = layout.requested_bytes, .alignment = layout.alignment }, .visible = !record.private };
        }

        pub fn beginRelease(self: *Self, id: u32, handle: u64) Error!Release {
            if (handle == 0) return error.Invalid;
            const entry = try self.current(id);
            if (entry.stats.pending_releases == std.math.maxInt(u32)) return error.Busy;
            var target = entry.tree.getEntryFor(handle);
            const node = target.node orelse return error.Stale;
            const record: *Record = @fieldParentPtr("node", node);
            std.debug.assert(record.epoch == entry.stats.epoch);
            const release: Release = .{ .owner = id, .epoch = record.epoch, .handle = handle, .record = record, .bytes = record.requested_bytes, .heap_bytes = record.heap_bytes, .private = record.private };
            target.set(null);
            entry.stats.pending_releases += 1;
            return release;
        }

        // No dereference of record after a successful backing release. Failed
        // heap release means the complete original allocation remains valid;
        // return that exact identity to the index, retaining all accounting.
        pub fn finishRelease(self: *Self, release: *Release, success: bool) Error!void {
            if (!release.active) return error.Stale;
            const entry = try self.matching(release.owner, release.epoch);
            std.debug.assert(entry.stats.pending_releases != 0);
            entry.stats.pending_releases -= 1;
            if (success) {
                entry.stats.allocations -= 1;
                entry.stats.bytes -= release.bytes;
                entry.stats.backing_bytes -= release.heap_bytes;
                if (release.private) entry.stats.private_allocations -= 1;
                entry.stats.releases +|= 1;
            } else {
                release.record.release_failed = true;
                var target = entry.tree.getEntryFor(release.handle);
                std.debug.assert(target.node == null);
                target.set(&release.record.node);
                entry.stats.release_failures +|= 1;
            }
            release.active = false;
        }

        pub fn close(self: *Self, id: u32) void {
            const entry = self.current(id) catch return;
            entry.stats.closing = true;
        }

        pub fn nextCleanup(self: *Self, id: u32) Error!?Release {
            const entry = try self.current(id);
            if (!entry.stats.closing or entry.stats.pending_creates != 0 or entry.stats.pending_releases != 0) return error.Busy;
            const node = entry.tree.getMin() orelse return null;
            return try self.beginRelease(id, node.key);
        }

        pub fn finish(self: *Self, id: u32) Error!void {
            const entry = try self.slot(id);
            if (entry.stats.epoch == 0) return;
            if (!entry.stats.closing or entry.stats.allocations != 0 or entry.stats.pending_creates != 0 or entry.stats.pending_releases != 0) return error.Busy;
            std.debug.assert(entry.tree.root == null and entry.stats.bytes == 0 and entry.stats.backing_bytes == 0);
            entry.* = .{};
        }

        fn slot(self: *Self, id: u32) Error!*Owner {
            if (id == 0 or id > owner_capacity) return error.Owner;
            return &self.owners[id - 1];
        }

        fn current(self: *Self, id: u32) Error!*Owner {
            const entry = try self.slot(id);
            if (entry.stats.epoch == 0) return error.Stale;
            return entry;
        }

        fn matching(self: *Self, id: u32, epoch: u64) Error!*Owner {
            const entry = try self.current(id);
            if (entry.stats.epoch != epoch) return error.Stale;
            return entry;
        }
    };
}

test "driver CPU heap owners retain close races, release failures and exact generations" {
    var state: State(2) = .{};
    try state.activate(1, 11);
    try state.activate(2, 12);
    try std.testing.expectError(error.Owner, state.activate(0, 1));
    try std.testing.expectError(error.Busy, state.activate(1, 12));
    const layout = try Layout.init(64, 16);
    var create = try state.begin(1, layout);
    const memory = try std.testing.allocator.alignedAlloc(u8, .@"16", layout.heap_bytes);
    const allocation = (try state.adopt(&create, memory)).allocation;
    try std.testing.expectError(error.Stale, state.beginRelease(2, allocation.handle));
    try std.testing.expectError(error.Stale, state.abort(&create));
    @memset(@as([*]u8, @ptrFromInt(allocation.address))[0..64], 0x57);
    var release = try state.beginRelease(1, allocation.handle);
    state.close(1);
    try std.testing.expectError(error.Closed, state.begin(1, layout));
    try std.testing.expectError(error.Busy, state.finish(1));
    try std.testing.expectError(error.Busy, state.nextCleanup(1));
    try state.finishRelease(&release, false);
    try std.testing.expectError(error.Stale, state.finishRelease(&release, false));
    const retained = try state.snapshot(1);
    try std.testing.expectEqual(@as(u64, 64), retained.bytes);
    try std.testing.expectEqual(@as(u64, 1), retained.release_failures);
    try std.testing.expectEqual(@as(u8, 0x57), @as(*const u8, @ptrFromInt(allocation.address)).*);
    release = (try state.nextCleanup(1)).?;
    std.testing.allocator.free(memory);
    try state.finishRelease(&release, true);
    try state.finish(1);
    try state.activate(1, 13);
    try std.testing.expectError(error.Stale, state.beginRelease(1, allocation.handle));

    create = try state.begin(1, layout);
    state.close(1);
    try std.testing.expectError(error.Busy, state.finish(1));
    try std.testing.expectError(error.Busy, state.nextCleanup(1));
    const late = try std.testing.allocator.alignedAlloc(u8, .@"16", layout.heap_bytes);
    const unpublished = try state.adopt(&create, late);
    try std.testing.expect(!unpublished.visible);
    try std.testing.expectEqual(@as(u64, 1), (try state.snapshot(1)).private_allocations);
    try std.testing.expectError(error.Closed, state.activate(1, 13));
    release = (try state.nextCleanup(1)).?;
    std.testing.allocator.free(late);
    try state.finishRelease(&release, true);
    try std.testing.expectEqual(@as(?Release, null), try state.nextCleanup(1));
    try state.finish(1);
    try state.finish(1);
}

test "driver CPU heap layout, allocation failure and handle exhaustion" {
    try std.testing.expectError(error.Invalid, Layout.init(0, 16));
    try std.testing.expectError(error.Invalid, Layout.init(1, 0));
    try std.testing.expectError(error.Invalid, Layout.init(1, 3));
    try std.testing.expectError(error.Overflow, Layout.init(std.math.maxInt(u64), 16));
    for ([_]u32{ 1, 16, 32, 4096 }) |alignment| {
        const layout = try Layout.init(137, alignment);
        try std.testing.expectEqual(@as(usize, 0), layout.payload_offset % layout.alignment);
        try std.testing.expect(layout.heap_bytes - layout.payload_offset == 137);
    }
    var state: State(1) = .{};
    try state.activate(1, 101);
    state.next_handle = std.math.maxInt(u64);
    var create = try state.begin(1, try Layout.init(1, 16));
    try std.testing.expectEqual(std.math.maxInt(u64), create.handle);
    try std.testing.expectError(error.Exhausted, state.begin(1, create.layout));
    state.close(1);
    try state.abort(&create);
    try std.testing.expectEqual(@as(u64, 1), (try state.snapshot(1)).allocation_failures);
    try state.finish(1);
    try state.activate(1, 102);
    try std.testing.expectError(error.Exhausted, state.begin(1, create.layout));
}

test "driver CPU heap index scales with actual backing and survives reordered frees" {
    var state: State(1) = .{};
    try state.activate(1, 1);
    const count = 4096;
    const layout = try Layout.init(37, 16);
    const stride = std.mem.alignForward(usize, layout.heap_bytes, 16);
    const backing = try std.testing.allocator.alignedAlloc(u8, .@"16", count * stride);
    defer std.testing.allocator.free(backing);
    for (0..count) |i| {
        var create = try state.begin(1, layout);
        const publication = try state.adopt(&create, backing[i * stride ..][0..layout.heap_bytes]);
        try std.testing.expect(publication.visible and publication.allocation.handle == i + 1);
        const data: [*]u8 = @ptrFromInt(publication.allocation.address);
        @memset(data[0..37], @truncate(i));
    }
    try std.testing.expectEqual(@as(u64, count), (try state.snapshot(1)).allocations);
    for (0..count) |i| {
        const index = (i * 73) % count;
        var release = try state.beginRelease(1, index + 1);
        try std.testing.expectEqual(@intFromPtr(backing.ptr) + index * stride, @intFromPtr(release.backing().ptr));
        const data = release.backing()[layout.payload_offset..];
        for (data) |byte| try std.testing.expectEqual(@as(u8, @truncate(index)), byte);
        // The shared test arena remains mapped; individual production frees
        // occur outside this component and are covered by the runtime owner.
        try state.finishRelease(&release, true);
        try std.testing.expectError(error.Stale, state.beginRelease(1, index + 1));
    }
    const empty = try state.snapshot(1);
    try std.testing.expect(empty.allocations == 0 and empty.bytes == 0 and empty.backing_bytes == 0);
    state.close(1);
    try state.finish(1);
}
