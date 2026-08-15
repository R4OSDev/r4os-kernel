// Intrusive scheduler wait linkage.
//
// A Node is owned by one stable Task object and can be linked into at most one
// QueueCore. Queue operations allocate no memory and are intentionally free of
// scheduler/interrupt policy; callers serialize them with the scheduler's
// IRQ/preemption critical section.

pub const Node = struct {
    owner: *anyopaque = undefined,
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

pub const QueueCore = struct {
    head: ?*Node = null,
    tail: ?*Node = null,
    count: usize = 0,
    closing: bool = false,
    generation: u64 = 1,
};

/// Links `node` at the FIFO tail. The owner address must remain stable until
/// the node has been detached. A closing queue rejects new enrollment.
pub fn link(queue: *QueueCore, node: *Node, owner: *anyopaque, owner_generation: u64) bool {
    if (queue.closing or owner_generation == 0 or node.queue != null) return false;

    node.owner = owner;
    node.owner_generation = owner_generation;
    node.queue = queue;
    node.prev = queue.tail;
    node.next = null;
    if (queue.tail) |tail| {
        tail.next = node;
    } else {
        queue.head = node;
    }
    queue.tail = node;
    queue.count += 1;
    return true;
}

/// Removes a node from whichever queue currently owns it. This is the common
/// O(1) primitive used by signal, timeout, cancellation, kill and task detach.
/// Owner identity is retained so a caller that popped the node can still
/// validate and wake its task.
pub fn detach(node: *Node) bool {
    const queue = node.queue orelse return false;

    if (node.prev) |prev| {
        prev.next = node.next;
    } else {
        queue.head = node.next;
    }
    if (node.next) |next| {
        next.prev = node.prev;
    } else {
        queue.tail = node.prev;
    }
    if (queue.count != 0) queue.count -= 1;
    node.queue = null;
    node.prev = null;
    node.next = null;
    return true;
}

/// Removes and returns the oldest waiter. Removal happens before the caller
/// changes scheduler state, so no ready task retains queue ownership.
pub fn popFront(queue: *QueueCore) ?*Node {
    const node = queue.head orelse return null;
    _ = detach(node);
    return node;
}

/// Starts object teardown in O(1). Callers drain nodes with popFront and wake
/// them with the desired terminal result. Queue storage must not be reused
/// until `count == 0`.
pub fn close(queue: *QueueCore) void {
    if (queue.closing) return;
    queue.closing = true;
    advanceGeneration(queue);
}

/// Reopens drained stable storage for a new object generation.
pub fn reopen(queue: *QueueCore) bool {
    if (!queue.closing or queue.count != 0 or queue.head != null or queue.tail != null) return false;
    queue.closing = false;
    advanceGeneration(queue);
    return true;
}

/// Clears stale task identity only after the node is detached.
pub fn clearOwner(node: *Node) bool {
    if (node.queue != null) return false;
    node.owner_generation = 0;
    node.owner = undefined;
    node.prev = null;
    node.next = null;
    return true;
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

fn advanceGeneration(queue: *QueueCore) void {
    queue.generation +%= 1;
    if (queue.generation == 0) queue.generation = 1;
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
