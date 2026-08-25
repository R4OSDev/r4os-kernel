pub const capacity: usize = 64;
pub const max_frame_bytes: usize = 1536;

const SlotState = enum(u8) {
    free,
    queued,
    processing,
};

pub const Metadata = struct {
    l4_checksum_valid: bool = false,
    software_fallback: bool = false,
};

const Slot = struct {
    state: SlotState = .free,
    generation: u32 = 0,
    adapter_index: usize = 0,
    len: u16 = 0,
    enqueued_ns: u64 = 0,
    metadata: Metadata = .{},
    data: [max_frame_bytes]u8 = .{0} ** max_frame_bytes,
};

pub const EnqueueResult = enum {
    accepted,
    invalid_frame,
    busy,
};

pub const Claim = struct {
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
pub const Queue = struct {
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
        self.* = .{};
        self.resetStorage();
    }

    pub fn enqueue(self: *Queue, adapter_index: usize, frame_bytes: []const u8, enqueued_ns: u64) EnqueueResult {
        return self.enqueueWithMetadata(adapter_index, frame_bytes, enqueued_ns, .{});
    }

    pub fn enqueueWithMetadata(self: *Queue, adapter_index: usize, frame_bytes: []const u8, enqueued_ns: u64, metadata: Metadata) EnqueueResult {
        if (frame_bytes.len == 0 or frame_bytes.len > max_frame_bytes) return .invalid_frame;
        if (self.free_count == 0) {
            self.busy +%= 1;
            return .busy;
        }

        self.free_count -= 1;
        const slot_index: usize = self.free_slots[self.free_count];
        const slot = &self.slots[slot_index];
        slot.generation +%= 1;
        if (slot.generation == 0) slot.generation = 1;
        slot.adapter_index = adapter_index;
        slot.len = @intCast(frame_bytes.len);
        slot.enqueued_ns = enqueued_ns;
        slot.metadata = metadata;
        @memcpy(slot.data[0..frame_bytes.len], frame_bytes);
        slot.state = .queued;

        self.ready[self.ready_tail] = @intCast(slot_index);
        self.ready_tail = (self.ready_tail + 1) % capacity;
        self.ready_count += 1;
        self.accepted +%= 1;
        const occupied = self.occupiedCount();
        if (occupied > self.high_water) self.high_water = occupied;
        return .accepted;
    }

    pub fn claim(self: *Queue) ?Claim {
        if (self.ready_count == 0) return null;
        const slot_index: usize = self.ready[self.ready_head];
        self.ready_head = (self.ready_head + 1) % capacity;
        self.ready_count -= 1;
        const slot = &self.slots[slot_index];
        if (slot.state != .queued) return null;
        slot.state = .processing;
        self.claimed +%= 1;
        return .{
            .slot = @intCast(slot_index),
            .generation = slot.generation,
            .adapter_index = slot.adapter_index,
            .len = slot.len,
            .enqueued_ns = slot.enqueued_ns,
            .metadata = slot.metadata,
        };
    }

    pub fn frame(self: *const Queue, claim_token: Claim) ?[]const u8 {
        const slot_index: usize = claim_token.slot;
        if (slot_index >= capacity) return null;
        const slot = &self.slots[slot_index];
        if (slot.state != .processing or slot.generation != claim_token.generation or slot.len != claim_token.len) return null;
        return slot.data[0..slot.len];
    }

    pub fn release(self: *Queue, claim_token: Claim) bool {
        const slot_index: usize = claim_token.slot;
        if (slot_index >= capacity) return false;
        const slot = &self.slots[slot_index];
        if (slot.state != .processing or slot.generation != claim_token.generation) return false;
        slot.state = .free;
        slot.adapter_index = 0;
        slot.len = 0;
        slot.enqueued_ns = 0;
        slot.metadata = .{};
        self.free_slots[self.free_count] = @intCast(slot_index);
        self.free_count += 1;
        self.released +%= 1;
        return true;
    }

    /// Cancels queued ownership during an explicit backend/system transition.
    /// Netcore calls this only after every producer and consumer callback has
    /// drained, so no processing slot can still be observed by a caller.
    pub fn cancelAll(self: *Queue) usize {
        const cancelled_now = self.occupiedCount();
        self.cancelled +%= cancelled_now;
        self.resetStorage();
        return cancelled_now;
    }

    pub fn queuedCount(self: *const Queue) usize {
        return self.ready_count;
    }

    pub fn occupiedCount(self: *const Queue) usize {
        return capacity - self.free_count;
    }

    fn resetStorage(self: *Queue) void {
        self.ready_head = 0;
        self.ready_tail = 0;
        self.ready_count = 0;
        self.free_count = capacity;
        for (&self.slots) |*slot| {
            slot.state = .free;
            slot.adapter_index = 0;
            slot.len = 0;
            slot.enqueued_ns = 0;
            slot.metadata = .{};
        }
        for (&self.free_slots, 0..) |*slot, index| slot.* = @intCast(index);
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
