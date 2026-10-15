// Private C facade to the single native filesystem policy owner.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Error = error{ Geometry, Capacity, Allocation, Initialize, Flush, Tail, Publish };
pub const Extension = extern struct { first: u32, count: usize };
const Io = extern struct {
    context: *anyopaque,
    allocate: *const fn (*anyopaque, usize, *u32) callconv(.c) bool,
    initialize: *const fn (*anyopaque, u32, usize) callconv(.c) bool,
    flush: *const fn (*anyopaque) callconv(.c) bool,
    tail_available: *const fn (*anyopaque) callconv(.c) bool,
    publish: *const fn (*anyopaque, u32) callconv(.c) bool,
    discard: *const fn (*anyopaque, u32) callconv(.c) void,
};
extern fn r4os_rust_fs_directory_extend(*const Io, usize, usize, usize, *Extension) callconv(.c) u8;
pub fn extend(io: anytype, cluster_bytes: usize, missing_entries: usize, remaining_clusters: usize) Error!Extension {
    const Binding = struct {
        fn instance(raw: *anyopaque) *@TypeOf(io) {
            return @ptrCast(@alignCast(raw));
        }
        fn allocate(raw: *anyopaque, count: usize, out: *u32) callconv(.c) bool {
            out.* = instance(raw).*.allocate(count) orelse return false;
            return true;
        }
        fn initialize(raw: *anyopaque, first: u32, count: usize) callconv(.c) bool {
            return instance(raw).*.initialize(first, count);
        }
        fn flush(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.flush();
        }
        fn tailAvailable(raw: *anyopaque) callconv(.c) bool {
            return instance(raw).*.tailAvailable();
        }
        fn publish(raw: *anyopaque, first: u32) callconv(.c) bool {
            return instance(raw).*.publish(first);
        }
        fn discard(raw: *anyopaque, first: u32) callconv(.c) void {
            instance(raw).*.discard(first);
        }
    };
    var binding = io;
    const callbacks: Io = .{ .context = @ptrCast(&binding), .allocate = Binding.allocate, .initialize = Binding.initialize, .flush = Binding.flush, .tail_available = Binding.tailAvailable, .publish = Binding.publish, .discard = Binding.discard };
    var out: Extension = undefined;
    switch (r4os_rust_fs_directory_extend(&callbacks, cluster_bytes, missing_entries, remaining_clusters, &out)) {
        0 => return out,
        1 => return error.Geometry,
        2 => return error.Capacity,
        3 => return error.Allocation,
        4 => return error.Initialize,
        5 => return error.Flush,
        6 => return error.Tail,
        7 => return error.Publish,
        else => unreachable,
    }
}
comptime {
    if (@sizeOf(Extension) != 16 or @offsetOf(Extension, "count") != 8 or @sizeOf(Io) != 56) @compileError("private FAT extension layout drift");
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
