// Intrusive wait linkage belongs to the native owner; stable C records retain
// exact task generation and queue identity. Runtime admission stays at caller.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const Node = extern struct {
    owner: ?*anyopaque = null,
    owner_generation: u64 = 0,
    queue: ?*QueueCore = null,
    prev: ?*Node = null,
    next: ?*Node = null,

    pub fn detached(self: *const Node) bool {
        return self.queue == null;
    }
    pub fn reset(self: *Node) bool {
        return clearOwner(self);
    }
};
pub const QueueCore = extern struct {
    head: ?*Node = null,
    tail: ?*Node = null,
    count: usize = 0,
    closing: bool = false,
    generation: u64 = 1,
};
extern fn r4os_rust_wait_node_link(queue: *QueueCore, node: *Node, owner: *anyopaque, generation: u64) bool;
extern fn r4os_rust_wait_node_detach(node: *Node) bool;
extern fn r4os_rust_wait_node_pop_front(queue: *QueueCore) ?*Node;
extern fn r4os_rust_wait_node_close(queue: *QueueCore) void;
extern fn r4os_rust_wait_node_reopen(queue: *QueueCore) bool;
extern fn r4os_rust_wait_node_clear_owner(node: *Node) bool;
comptime {
    if (@sizeOf(Node) != 40 or @sizeOf(QueueCore) != 40 or
        @offsetOf(Node, "queue") != 16 or @offsetOf(QueueCore, "closing") != 24 or
        @offsetOf(QueueCore, "generation") != 32)
        @compileError("Native wait-node C layout differs");
}
pub fn link(queue: *QueueCore, node: *Node, owner: *anyopaque, generation: u64) bool {
    return r4os_rust_wait_node_link(queue, node, owner, generation);
}
pub fn detach(node: *Node) bool {
    return r4os_rust_wait_node_detach(node);
}
pub fn popFront(queue: *QueueCore) ?*Node {
    return r4os_rust_wait_node_pop_front(queue);
}
pub fn close(queue: *QueueCore) void {
    r4os_rust_wait_node_close(queue);
}
pub fn reopen(queue: *QueueCore) bool {
    return r4os_rust_wait_node_reopen(queue);
}
pub fn clearOwner(node: *Node) bool {
    return r4os_rust_wait_node_clear_owner(node);
}
pub fn reset(node: *Node) bool {
    return clearOwner(node);
}
pub fn isLinked(node: *const Node) bool {
    return node.queue != null;
}
pub fn isDetached(node: *const Node) bool {
    return node.queue == null;
}
pub fn isEmpty(queue: *const QueueCore) bool {
    return queue.count == 0;
}

test "intrusive queue links, unlinks and pops in FIFO order" {
    const testing = @import("std").testing;
    var queue = QueueCore{};
    var owner_a: u8 = 1;
    var owner_b: u8 = 2;
    var owner_c: u8 = 3;
    var a = Node{};
    var b = Node{};
    var c = Node{};

    try testing.expect(link(&queue, &a, &owner_a, 11));
    try testing.expect(link(&queue, &b, &owner_b, 12));
    try testing.expect(link(&queue, &c, &owner_c, 13));
    try testing.expectEqual(@as(usize, 3), queue.count);
    try testing.expect(detach(&b));
    try testing.expectEqual(@as(usize, 2), queue.count);
    try testing.expectEqual(&a, popFront(&queue).?);
    try testing.expectEqual(&c, popFront(&queue).?);
    try testing.expect(isEmpty(&queue));
    try testing.expect(!detach(&b));
}

test "close rejects enrollment and drained storage reopens with a new generation" {
    const testing = @import("std").testing;
    var queue = QueueCore{};
    var owner: u8 = 1;
    var node = Node{};
    const initial_generation = queue.generation;

    close(&queue);
    try testing.expect(queue.closing);
    try testing.expect(queue.generation != initial_generation);
    try testing.expect(!link(&queue, &node, &owner, 1));
    const closed_generation = queue.generation;
    try testing.expect(reopen(&queue));
    try testing.expect(!queue.closing);
    try testing.expect(queue.generation != closed_generation);
    try testing.expect(link(&queue, &node, &owner, 1));
}
