// Resident R4D CPU allocation metadata is implemented in Rust. This fixed
// private C view owns no second index; backing allocation/free stays outside.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Error = error{ Invalid, Owner, Stale, Closed, Busy, Exhausted, Overflow };
const Node = extern struct { key: u64, priority: usize, parent: ?*Node, children: [2]?*Node };
const Tree = extern struct { root: ?*Node = null, xorshift: usize = 0 };
pub const Record = extern struct {
    node: Node = undefined,
    epoch: u64,
    requested_bytes: u64,
    heap_bytes: usize,
    payload_offset: usize,
    alignment: u32,
    private: bool,
    release_failed: bool = false,
};
pub const Layout = extern struct {
    requested_bytes: u64,
    heap_bytes: usize,
    payload_offset: usize,
    alignment: u32,
    pub fn init(bytes: u64, alignment: u32) Error!Layout {
        var out: Layout = undefined;
        try checked(r4os_rust_driver_memory_layout(bytes, alignment, &out));
        return out;
    }
};
pub const Allocation = extern struct { handle: u64, address: usize, bytes: u64, alignment: u32 };
pub const Create = extern struct { epoch: u64, handle: u64, layout: Layout, owner: u32, active: bool = true };
pub const Published = extern struct { allocation: Allocation, visible: bool };
pub const Release = extern struct {
    record: *Record,
    epoch: u64,
    handle: u64,
    bytes: u64,
    heap_bytes: usize,
    owner: u32,
    private: bool,
    active: bool = true,
    pub fn backing(self: Release) []u8 {
        std.debug.assert(self.active);
        const address: [*]u8 = @ptrCast(self.record);
        return address[0..self.heap_bytes];
    }
};
pub const Stats = extern struct {
    epoch: u64 = 0,
    allocations: u64 = 0,
    bytes: u64 = 0,
    backing_bytes: u64 = 0,
    peak_bytes: u64 = 0,
    private_allocations: u64 = 0,
    allocation_calls: u64 = 0,
    allocation_failures: u64 = 0,
    releases: u64 = 0,
    release_failures: u64 = 0,
    pending_creates: u32 = 0,
    pending_releases: u32 = 0,
    closing: bool = false,
};
const Owner = extern struct { stats: Stats = .{}, tree: Tree = .{} };
extern fn r4os_rust_driver_memory_layout(bytes: u64, alignment: u32, out: *Layout) u8;
extern fn r4os_rust_driver_memory_mutate(owners: [*]Owner, cap: usize, op: u8, id: u32, epoch: u64) u8;
extern fn r4os_rust_driver_memory_snapshot(owners: [*]Owner, cap: usize, id: u32, out: *Stats) u8;
extern fn r4os_rust_driver_memory_begin(owners: [*]Owner, cap: usize, next: *u64, id: u32, layout: *const Layout, out: *Create) u8;
extern fn r4os_rust_driver_memory_abort(owners: [*]Owner, cap: usize, create: *Create) u8;
extern fn r4os_rust_driver_memory_adopt(owners: [*]Owner, cap: usize, create: *Create, backing: [*]u8, len: usize, out: *Published) u8;
extern fn r4os_rust_driver_memory_release(owners: [*]Owner, cap: usize, id: u32, handle: u64, out: *Release) u8;
extern fn r4os_rust_driver_memory_finish_release(owners: [*]Owner, cap: usize, release: *Release, success: bool) u8;
extern fn r4os_rust_driver_memory_cleanup(owners: [*]Owner, cap: usize, id: u32, out: *Release, present: *bool) u8;
comptime {
    if (@sizeOf(Node) != 40 or @sizeOf(Tree) != 16 or @sizeOf(Record) != 80 or @sizeOf(Layout) != 32 or @sizeOf(Create) != 56 or @sizeOf(Release) != 48 or @sizeOf(Stats) != 96 or @sizeOf(Owner) != 112) @compileError("Native R4D CPU allocation C layout drift");
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
        7 => error.Overflow,
        else => @panic("Invalid native CPU allocation status"),
    };
}
pub fn State(comptime owner_capacity: usize) type {
    return extern struct {
        const Self = @This();
        owners: [owner_capacity]Owner = .{Owner{}} ** owner_capacity,
        next_handle: u64 = 1,
        pub fn activate(self: *Self, id: u32, epoch: u64) Error!void {
            try checked(r4os_rust_driver_memory_mutate(&self.owners, owner_capacity, 0, id, epoch));
        }
        pub fn snapshot(self: *Self, id: u32) Error!Stats {
            var out: Stats = undefined;
            try checked(r4os_rust_driver_memory_snapshot(&self.owners, owner_capacity, id, &out));
            return out;
        }
        pub fn begin(self: *Self, id: u32, layout: Layout) Error!Create {
            var out: Create = undefined;
            try checked(r4os_rust_driver_memory_begin(&self.owners, owner_capacity, &self.next_handle, id, &layout, &out));
            return out;
        }
        pub fn abort(self: *Self, create: *Create) Error!void {
            try checked(r4os_rust_driver_memory_abort(&self.owners, owner_capacity, create));
        }
        pub fn adopt(self: *Self, create: *Create, backing: []u8) Error!Published {
            var out: Published = undefined;
            try checked(r4os_rust_driver_memory_adopt(&self.owners, owner_capacity, create, backing.ptr, backing.len, &out));
            return out;
        }
        pub fn beginRelease(self: *Self, id: u32, handle: u64) Error!Release {
            var out: Release = undefined;
            try checked(r4os_rust_driver_memory_release(&self.owners, owner_capacity, id, handle, &out));
            return out;
        }
        pub fn finishRelease(self: *Self, release: *Release, success: bool) Error!void {
            try checked(r4os_rust_driver_memory_finish_release(&self.owners, owner_capacity, release, success));
        }
        pub fn close(self: *Self, id: u32) void {
            _ = r4os_rust_driver_memory_mutate(&self.owners, owner_capacity, 1, id, 0);
        }
        pub fn nextCleanup(self: *Self, id: u32) Error!?Release {
            var out: Release = undefined;
            var present = false;
            try checked(r4os_rust_driver_memory_cleanup(&self.owners, owner_capacity, id, &out, &present));
            return if (present) out else null;
        }
        pub fn finish(self: *Self, id: u32) Error!void {
            try checked(r4os_rust_driver_memory_mutate(&self.owners, owner_capacity, 2, id, 0));
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
