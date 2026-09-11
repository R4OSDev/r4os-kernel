const std = @import("std");

pub const Error = error{ Geometry, Capacity, Allocation, Initialize, Flush, Tail, Publish };
pub const Extension = struct { first: u32, count: usize };

/// The FAT owner holds its volume request lane. New bytes and FAT entries
/// become durable before the old tail can reach them. After any publication
/// attempt, even an error may have linked one mirror: retain the allocation.
pub fn extend(io: anytype, cluster_bytes: usize, missing_entries: usize, remaining_clusters: usize) Error!Extension {
    if (cluster_bytes < 512 or cluster_bytes > 65536 or !std.math.isPowerOfTwo(cluster_bytes) or
        missing_entries == 0 or missing_entries > 21) return error.Geometry;
    const count = (missing_entries * 32 + cluster_bytes - 1) / cluster_bytes;
    if (count > remaining_clusters) return error.Capacity;
    const first = io.allocate(count) orelse return error.Allocation;
    var may_be_linked = false;
    errdefer if (!may_be_linked) io.discard(first);
    if (!io.initialize(first, count)) return error.Initialize;
    if (!io.flush()) return error.Flush;
    if (!io.tailAvailable()) return error.Tail;
    may_be_linked = true;
    if (!io.publish(first)) return error.Publish;
    if (!io.flush()) return error.Flush;
    return .{ .first = first, .count = count };
}

test "directory extension initializes before visibility and retains storage after ambiguous FAT publication" {
    const Model = struct {
        fault: enum { none, allocation, initialize, first_flush, tail, before_link, between_mirrors, second_flush } = .none,
        allocated: usize = 0,
        discarded: bool = false,
        flushes: usize = 0,
        old_entries: [512]u8 = @splat(0x71),
        new_entries: [1024]u8 = @splat(0xa5),
        durable_entries: [1024]u8 = @splat(0xa5),
        mirrors: [2]u32 = @splat(0x0fffffff),
        pub fn allocate(self: *@This(), count: usize) ?u32 {
            if (self.fault == .allocation) return null;
            self.allocated = count;
            return 42;
        }
        pub fn initialize(self: *@This(), first: u32, count: usize) bool {
            std.debug.assert(first == 42 and count == self.allocated);
            if (self.fault == .initialize) return false;
            @memset(self.new_entries[0 .. count * 512], 0);
            return true;
        }
        pub fn flush(self: *@This()) bool {
            self.flushes += 1;
            if ((self.flushes == 1 and self.fault == .first_flush) or
                (self.flushes == 2 and self.fault == .second_flush)) return false;
            self.durable_entries = self.new_entries;
            return true;
        }
        pub fn tailAvailable(self: *@This()) bool {
            return self.fault != .tail;
        }
        pub fn publish(self: *@This(), first: u32) bool {
            std.debug.assert(std.mem.allEqual(u8, self.durable_entries[0 .. self.allocated * 512], 0));
            std.debug.assert(self.flushes == 1 and first == 42);
            if (self.fault == .before_link) return false;
            self.mirrors[0] = first;
            if (self.fault == .between_mirrors) return false;
            self.mirrors[1] = first;
            return true;
        }
        pub fn discard(self: *@This(), first: u32) void {
            std.debug.assert(first == 42 and self.mirrors[0] == 0x0fffffff);
            self.discarded = true;
            self.allocated = 0;
        }
    };
    const t = std.testing;
    for ([_]usize{ 1, 16, 21 }) |missing| {
        var io: Model = .{};
        const extension = try extend(&io, 512, missing, 2);
        try t.expectEqual(if (missing > 16) @as(usize, 2) else 1, extension.count);
        try t.expectEqualSlices(u32, &.{ 42, 42 }, &io.mirrors);
        try t.expect(std.mem.allEqual(u8, &io.old_entries, 0x71) and !io.discarded);
        if (extension.count == 1) try t.expect(std.mem.allEqual(u8, io.new_entries[512..], 0xa5));
    }
    inline for (.{ .allocation, .initialize, .first_flush, .tail, .before_link, .between_mirrors, .second_flush }) |fault| {
        var io: Model = .{ .fault = fault };
        if (extend(&io, 512, 21, 2)) |_| return error.ExpectedFailure else |_| {}
        const ambiguous = fault == .before_link or fault == .between_mirrors or fault == .second_flush;
        try t.expectEqual(ambiguous, io.allocated == 2);
        try t.expectEqual(!ambiguous and fault != .allocation, io.discarded);
        try t.expect(std.mem.allEqual(u8, &io.old_entries, 0x71));
        if (fault == .between_mirrors) try t.expectEqualSlices(u32, &.{ 42, 0x0fffffff }, &io.mirrors);
    }
    var io: Model = .{};
    try t.expectError(error.Capacity, extend(&io, 512, 21, 1));
    for ([_]usize{ 0, 256, 513, 131072 }) |bytes| try t.expectError(error.Geometry, extend(&io, bytes, 1, 2));
    try t.expectError(error.Geometry, extend(&io, 512, 0, 2));
    try t.expectError(error.Geometry, extend(&io, 512, 22, 2));
    try t.expectEqual(@as(usize, 0), io.allocated);
}
