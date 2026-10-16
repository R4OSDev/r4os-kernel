// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_net_rx_reset(*Queue) void;
extern fn r4os_rust_net_rx_enqueue(*Queue, usize, [*]const u8, usize, u64, *const Metadata) u8;
extern fn r4os_rust_net_rx_claim(*Queue, *Claim) bool;
extern fn r4os_rust_net_rx_frame(*const Queue, *const Claim, *usize) ?[*]const u8;
extern fn r4os_rust_net_rx_release(*Queue, *const Claim) bool;
extern fn r4os_rust_net_rx_cancel_all(*Queue) usize;
extern fn r4os_rust_net_rx_queued(*const Queue) usize;
extern fn r4os_rust_net_rx_occupied(*const Queue) usize;
pub const capacity: usize = 64;
pub const max_frame_bytes: usize = 1536;

const SlotState = enum(u8) {
    free,
    queued,
    processing,
};

pub const Metadata = extern struct {
    l4_checksum_valid: bool = false,
    software_fallback: bool = false,
};

const Slot = extern struct {
    state: SlotState = .free,
    generation: u32 = 0,
    adapter_index: usize = 0,
    len: u16 = 0,
    enqueued_ns: u64 = 0,
    metadata: Metadata = .{},
    data: [max_frame_bytes]u8 = .{0} ** max_frame_bytes,
};

pub const EnqueueResult = enum(u8) {
    accepted,
    invalid_frame,
    busy,
};

pub const Claim = extern struct {
    slot: u8,
    generation: u32,
    adapter_index: usize,
    len: u16,
    enqueued_ns: u64,
    metadata: Metadata,
};

/// Fixed-capacity ownership queue for the NIC-to-protocol handoff.
///
/// Synchronization is deliberately owned by Netcore: callers serialize every
/// method with the same IRQ/preemption critical section. A claimed slot stays
/// occupied until `release`, so a driver buffer is copied exactly once and a
/// nested protocol poll cannot let a producer overwrite the frame in flight.
pub const Queue = extern struct {
    slots: [capacity]Slot = .{Slot{}} ** capacity,
    ready: [capacity]u8 = .{0} ** capacity,
    ready_head: usize = 0,
    ready_tail: usize = 0,
    ready_count: usize = 0,
    free_slots: [capacity]u8 = .{0} ** capacity,
    free_count: usize = 0,
    accepted: u64 = 0,
    claimed: u64 = 0,
    released: u64 = 0,
    cancelled: u64 = 0,
    busy: u64 = 0,
    high_water: usize = 0,

    pub fn reset(self: *Queue) void {
        r4os_rust_net_rx_reset(self);
    }

    pub fn enqueue(self: *Queue, adapter_index: usize, frame_bytes: []const u8, enqueued_ns: u64) EnqueueResult {
        return self.enqueueWithMetadata(adapter_index, frame_bytes, enqueued_ns, .{});
    }

    pub fn enqueueWithMetadata(self: *Queue, adapter_index: usize, frame_bytes: []const u8, enqueued_ns: u64, metadata: Metadata) EnqueueResult {
        return @enumFromInt(r4os_rust_net_rx_enqueue(self, adapter_index, frame_bytes.ptr, frame_bytes.len, enqueued_ns, &metadata));
    }

    pub fn claim(self: *Queue) ?Claim {
        var out: Claim = undefined;
        return if (r4os_rust_net_rx_claim(self, &out)) out else null;
    }

    pub fn frame(self: *const Queue, claim_token: Claim) ?[]const u8 {
        var len: usize = 0;
        const data = r4os_rust_net_rx_frame(self, &claim_token, &len) orelse return null;
        return data[0..len];
    }

    pub fn release(self: *Queue, claim_token: Claim) bool {
        return r4os_rust_net_rx_release(self, &claim_token);
    }

    /// Cancels queued ownership during an explicit backend/system transition.
    /// Netcore calls this only after every producer and consumer callback has
    /// drained, so no processing slot can still be observed by a caller.
    pub fn cancelAll(self: *Queue) usize {
        return r4os_rust_net_rx_cancel_all(self);
    }

    pub fn queuedCount(self: *const Queue) usize {
        return r4os_rust_net_rx_queued(self);
    }

    pub fn occupiedCount(self: *const Queue) usize {
        return r4os_rust_net_rx_occupied(self);
    }
};

test "RX handoff preserves FIFO bytes and exactly-once release" {
    const std = @import("std");
    var queue: Queue = .{};
    queue.reset();

    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueueWithMetadata(2, &.{ 1, 2, 3 }, 11, .{ .l4_checksum_valid = true }));
    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(4, &.{ 9, 8 }, 12));

    const first = queue.claim().?;
    try std.testing.expectEqual(@as(usize, 2), first.adapter_index);
    try std.testing.expect(first.metadata.l4_checksum_valid);
    try std.testing.expect(!first.metadata.software_fallback);
    try std.testing.expectEqualSlices(u8, &.{ 1, 2, 3 }, queue.frame(first).?);
    try std.testing.expect(queue.release(first));
    try std.testing.expect(!queue.release(first));

    const second = queue.claim().?;
    try std.testing.expectEqual(@as(usize, 4), second.adapter_index);
    try std.testing.expectEqualSlices(u8, &.{ 9, 8 }, queue.frame(second).?);
    try std.testing.expect(queue.release(second));
    try std.testing.expectEqual(@as(u64, 2), queue.accepted);
    try std.testing.expectEqual(queue.accepted, queue.released);
    try std.testing.expectEqual(@as(usize, 0), queue.occupiedCount());
}

test "RX handoff never reuses a processing slot and recovers from busy" {
    const std = @import("std");
    var queue: Queue = .{};
    queue.reset();

    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(0, &.{0xA5}, 1));
    const held = queue.claim().?;
    var index: usize = 1;
    while (index < capacity) : (index += 1) {
        const byte: [1]u8 = .{@intCast(index)};
        try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(0, &byte, @intCast(index + 1)));
    }
    try std.testing.expectEqual(EnqueueResult.busy, queue.enqueue(0, &.{0xFF}, 99));
    try std.testing.expectEqual(@as(usize, capacity), queue.occupiedCount());
    try std.testing.expectEqualSlices(u8, &.{0xA5}, queue.frame(held).?);

    try std.testing.expect(queue.release(held));
    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(0, &.{0xFE}, 100));
    try std.testing.expectEqual(@as(u64, capacity + 1), queue.accepted);
    try std.testing.expectEqual(@as(u64, 1), queue.busy);
}

test "RX handoff cancellation accounts every accepted frame" {
    const std = @import("std");
    var queue: Queue = .{};
    queue.reset();
    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(0, &.{1}, 1));
    try std.testing.expectEqual(EnqueueResult.accepted, queue.enqueue(1, &.{2}, 2));
    try std.testing.expectEqual(@as(usize, 2), queue.cancelAll());
    try std.testing.expectEqual(queue.accepted, queue.released + queue.cancelled);
    try std.testing.expectEqual(@as(usize, 0), queue.occupiedCount());
}

comptime {
    if (@sizeOf(Metadata) != 2 or @alignOf(Metadata) != 1 or @offsetOf(Metadata, "l4_checksum_valid") != 0 or @offsetOf(Metadata, "software_fallback") != 1) @compileError("Native network Metadata layout");
}

comptime {
    if (@sizeOf(Slot) != 1576 or @alignOf(Slot) != 8 or @offsetOf(Slot, "state") != 0 or @offsetOf(Slot, "generation") != 4 or @offsetOf(Slot, "adapter_index") != 8 or @offsetOf(Slot, "len") != 16 or @offsetOf(Slot, "enqueued_ns") != 24 or @offsetOf(Slot, "metadata") != 32 or @offsetOf(Slot, "data") != 34) @compileError("Native network Slot layout");
}

comptime {
    if (@sizeOf(Claim) != 40 or @alignOf(Claim) != 8 or @offsetOf(Claim, "slot") != 0 or @offsetOf(Claim, "generation") != 4 or @offsetOf(Claim, "adapter_index") != 8 or @offsetOf(Claim, "len") != 16 or @offsetOf(Claim, "enqueued_ns") != 24 or @offsetOf(Claim, "metadata") != 32) @compileError("Native network Claim layout");
}

comptime {
    if (@sizeOf(Queue) != 101072 or @alignOf(Queue) != 8 or @offsetOf(Queue, "slots") != 0 or @offsetOf(Queue, "ready") != 100864 or @offsetOf(Queue, "ready_head") != 100928 or @offsetOf(Queue, "ready_tail") != 100936 or @offsetOf(Queue, "ready_count") != 100944 or @offsetOf(Queue, "free_slots") != 100952 or @offsetOf(Queue, "free_count") != 101016 or @offsetOf(Queue, "accepted") != 101024 or @offsetOf(Queue, "claimed") != 101032 or @offsetOf(Queue, "released") != 101040 or @offsetOf(Queue, "cancelled") != 101048 or @offsetOf(Queue, "busy") != 101056 or @offsetOf(Queue, "high_water") != 101064) @compileError("Native network Queue layout");
}
