const std = @import("std");

/// Intrusive AVL links: stable task storage owns every node. All mutations
/// belong to the scheduler runtime owner and never allocate or call out.
pub fn Links(comptime T: type) type {
    return struct {
        parent: ?*T = null,
        left: ?*T = null,
        right: ?*T = null,
        height: u8 = 1,
    };
}

pub fn Index(comptime T: type, comptime field: []const u8, comptime less: fn (*const T, *const T) bool) type {
    return struct {
        const Self = @This();
        pub const Neighbors = struct { previous: ?*T = null, following: ?*T = null };
        root: ?*T = null,
        first: ?*T = null,
        count: usize = 0,
        visits: u64 = 0,

        fn link(node: *T) *Links(T) {
            return &@field(node, field);
        }
        fn height(node: ?*T) u8 {
            return if (node) |n| link(n).height else 0;
        }
        fn update(node: *T) void {
            link(node).height = 1 + @max(height(link(node).left), height(link(node).right));
        }
        fn balance(node: *T) i16 {
            return @as(i16, height(link(node).left)) - @as(i16, height(link(node).right));
        }

        fn replace(self: *Self, old: *T, replacement: ?*T) void {
            const parent = link(old).parent;
            if (parent) |p| {
                if (link(p).left == old) link(p).left = replacement else link(p).right = replacement;
            } else self.root = replacement;
            if (replacement) |n| link(n).parent = parent;
        }

        fn rotateLeft(self: *Self, node: *T) *T {
            const pivot = link(node).right.?;
            const middle = link(pivot).left;
            self.replace(node, pivot);
            link(pivot).left = node;
            link(node).parent = pivot;
            link(node).right = middle;
            if (middle) |m| link(m).parent = node;
            update(node);
            update(pivot);
            return pivot;
        }

        fn rotateRight(self: *Self, node: *T) *T {
            const pivot = link(node).left.?;
            const middle = link(pivot).right;
            self.replace(node, pivot);
            link(pivot).right = node;
            link(node).parent = pivot;
            link(node).left = middle;
            if (middle) |m| link(m).parent = node;
            update(node);
            update(pivot);
            return pivot;
        }

        fn rebalance(self: *Self, start: ?*T) void {
            var cursor = start;
            while (cursor) |node| {
                self.visits +%= 1;
                update(node);
                var top = node;
                if (balance(node) > 1) {
                    if (balance(link(node).left.?) < 0) _ = self.rotateLeft(link(node).left.?);
                    top = self.rotateRight(node);
                } else if (balance(node) < -1) {
                    if (balance(link(node).right.?) > 0) _ = self.rotateRight(link(node).right.?);
                    top = self.rotateLeft(node);
                }
                cursor = link(top).parent;
            }
        }

        pub fn insert(self: *Self, node: *T) Neighbors {
            link(node).* = .{};
            var parent: ?*T = null;
            var cursor = self.root;
            var neighbors: Neighbors = .{};
            while (cursor) |candidate| {
                self.visits +%= 1;
                parent = candidate;
                if (less(node, candidate)) {
                    neighbors.following = candidate;
                    cursor = link(candidate).left;
                } else {
                    neighbors.previous = candidate;
                    cursor = link(candidate).right;
                }
            }
            link(node).parent = parent;
            if (parent) |p| {
                if (less(node, p)) link(p).left = node else link(p).right = node;
            } else self.root = node;
            if (neighbors.previous == null) self.first = node;
            self.count += 1;
            self.rebalance(parent);
            return neighbors;
        }

        fn minimum(self: *Self, node: *T) *T {
            var cursor = node;
            while (link(cursor).left) |left| {
                self.visits +%= 1;
                cursor = left;
            }
            return cursor;
        }

        pub fn next(self: *Self, node: *T) ?*T {
            if (link(node).right) |right| return self.minimum(right);
            var child = node;
            var parent = link(child).parent;
            while (parent) |p| {
                self.visits +%= 1;
                if (link(p).left == child) return p;
                child = p;
                parent = link(p).parent;
            }
            return null;
        }

        pub fn remove(self: *Self, node: *T) void {
            if (self.first == node) self.first = self.next(node);
            var start = link(node).parent;
            if (link(node).left == null) {
                self.replace(node, link(node).right);
            } else if (link(node).right == null) {
                self.replace(node, link(node).left);
            } else {
                // Move the successor node, never task payloads or identities.
                const successor = self.minimum(link(node).right.?);
                if (link(successor).parent != node) {
                    start = link(successor).parent;
                    self.replace(successor, link(successor).right);
                    link(successor).right = link(node).right;
                    link(link(successor).right.?).parent = successor;
                } else start = successor;
                self.replace(node, successor);
                link(successor).left = link(node).left;
                link(link(successor).left.?).parent = successor;
                update(successor);
            }
            link(node).* = .{};
            self.count -= 1;
            self.rebalance(start);
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
