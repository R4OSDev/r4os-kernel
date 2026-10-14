const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
const RawIndex = extern struct {
    root: ?*anyopaque = null,
    first: ?*anyopaque = null,
    count: usize = 0,
    visits: u64 = 0,
};
const RawNeighbors = extern struct { previous: ?*anyopaque = null, following: ?*anyopaque = null };
const Less = *const fn (*const anyopaque, *const anyopaque) callconv(.c) bool;
extern fn r4os_rust_task_index_insert(index: *RawIndex, node: *anyopaque, offset: usize, less: Less, output: *RawNeighbors) void;
extern fn r4os_rust_task_index_next(index: *RawIndex, node: *anyopaque, offset: usize) ?*anyopaque;
extern fn r4os_rust_task_index_remove(index: *RawIndex, node: *anyopaque, offset: usize) void;

// The stable node contains a fixed private C link record; the native owner
// receives only the link offset and pure ordering function, no Task layout.
pub fn Links(comptime T: type) type {
    return extern struct {
        parent: ?*T = null,
        left: ?*T = null,
        right: ?*T = null,
        height: u8 = 1,
    };
}
pub fn Index(comptime T: type, comptime field: []const u8, comptime less: fn (*const T, *const T) bool) type {
    return extern struct {
        const Self = @This();
        pub const Neighbors = extern struct { previous: ?*T = null, following: ?*T = null };
        root: ?*T = null,
        first: ?*T = null,
        count: usize = 0,
        visits: u64 = 0,

        comptime {
            if (@sizeOf(Links(T)) != 32 or @sizeOf(Self) != 32 or @sizeOf(Neighbors) != 16)
                @compileError("Native intrusive AVL C layout differs");
        }
        fn lessBridge(a: *const anyopaque, b: *const anyopaque) callconv(.c) bool {
            return less(@ptrCast(@alignCast(a)), @ptrCast(@alignCast(b)));
        }
        pub fn insert(self: *Self, node: *T) Neighbors {
            var result: Neighbors = .{};
            r4os_rust_task_index_insert(@ptrCast(self), @ptrCast(node), @offsetOf(T, field), lessBridge, @ptrCast(&result));
            return result;
        }
        pub fn next(self: *Self, node: *T) ?*T {
            const result = r4os_rust_task_index_next(@ptrCast(self), @ptrCast(node), @offsetOf(T, field)) orelse return null;
            return @ptrCast(@alignCast(result));
        }
        pub fn remove(self: *Self, node: *T) void {
            r4os_rust_task_index_remove(@ptrCast(self), @ptrCast(node), @offsetOf(T, field));
        }
    };
}

const TestNode = struct {
    key: u128 = 0,
    links: Links(TestNode) = .{},
    fn less(a: *const TestNode, b: *const TestNode) bool {
        return a.key < b.key;
    }
};
const TestIndex = Index(TestNode, "links", TestNode.less);

fn validate(node: ?*TestNode, parent: ?*TestNode, low: ?u128, high: ?u128) !usize {
    const n = node orelse return 0;
    try std.testing.expect(n.links.parent == parent);
    if (low) |v| try std.testing.expect(n.key > v);
    if (high) |v| try std.testing.expect(n.key < v);
    const left = try validate(n.links.left, n, low, n.key);
    const right = try validate(n.links.right, n, n.key, high);
    const lh = if (n.links.left) |l| l.links.height else 0;
    const rh = if (n.links.right) |r| r.links.height else 0;
    try std.testing.expectEqual(1 + @max(lh, rh), n.links.height);
    try std.testing.expect(@abs(@as(i16, lh) - @as(i16, rh)) <= 1);
    return 1 + left + right;
}

test "ordered and reversed deadlines have logarithmic insertion and cancellation" {
    for ([_]usize{ 32, 256, 2048 }) |size| {
        var nodes: [2048]TestNode = .{TestNode{}} ** 2048;
        for ([_]bool{ false, true }) |reverse| {
            var index: TestIndex = .{};
            var maximum: u64 = 0;
            for (nodes[0..size], 0..) |*node, i| {
                node.key = if (reverse) size - i else i;
                const before = index.visits;
                _ = index.insert(node);
                maximum = @max(maximum, index.visits - before);
            }
            try std.testing.expectEqual(size, try validate(index.root, null, null, null));
            try std.testing.expect(maximum <= 4 * (1 + std.math.log2_int(usize, size)));
            // Odd permutation includes head, root and nodes with two children.
            for (0..size) |i| {
                const node = &nodes[(i * 631) % size];
                const identity = node.key;
                index.remove(node);
                try std.testing.expectEqual(identity, node.key);
                try std.testing.expectEqual(size - i - 1, try validate(index.root, null, null, null));
            }
            try std.testing.expect(index.first == null and index.root == null);
        }
    }
}

test "rank changes preserve original FIFO order and removal permits reuse" {
    var indices = [_]TestIndex{ .{}, .{} };
    var nodes = [_]TestNode{ .{ .key = 3 }, .{ .key = 1 }, .{ .key = 2 } };
    _ = indices[0].insert(&nodes[0]);
    _ = indices[1].insert(&nodes[1]);
    _ = indices[0].insert(&nodes[2]);
    indices[1].remove(&nodes[1]);
    _ = indices[0].insert(&nodes[1]);
    try std.testing.expect(indices[0].first == &nodes[1]);
    for ([_]usize{ 1, 2, 0 }) |i| {
        try std.testing.expect(indices[0].first == &nodes[i]);
        indices[0].remove(&nodes[i]);
    }
    nodes[1].key = 4;
    _ = indices[1].insert(&nodes[1]);
    try std.testing.expectEqual(@as(usize, 1), try validate(indices[1].root, null, null, null));
}

test "deadline order and stable ties cross the 64-bit sequence boundary" {
    var index: TestIndex = .{};
    const boundary: u128 = std.math.maxInt(u64);
    var nodes = [_]TestNode{ .{ .key = boundary + 2 }, .{ .key = boundary }, .{ .key = boundary + 1 } };
    for (&nodes) |*node| _ = index.insert(node);
    var cursor = index.first;
    for ([_]usize{ 1, 2, 0 }) |i| {
        try std.testing.expect(cursor == &nodes[i]);
        cursor = index.next(cursor.?);
    }
    try std.testing.expect(cursor == null);
}
