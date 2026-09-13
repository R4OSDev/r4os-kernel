// Per-fence IRQ mailbox, serialized solely by the existing IRQ/runtime owner.
// No task mutex, allocation, callback, dependency walk or BO access is allowed.
const std = @import("std");
const queue = @import("queue_state.zig");
pub const Ack = struct { fence: queue.Fence, result: queue.Result, instant: u64 };
// One coalesced notification per registered backend, including idle display
// changes. Runtime ownership protects this mailbox independently of BO locks.
pub fn Wakeups(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        const Entry = struct { owner: u32 = 0, binding: queue.Binding = .{}, pending: bool = false };
        entries: [capacity]Entry = .{Entry{}} ** capacity,
        pub fn bind(self: *Self, slot: usize, owner: u32, binding: queue.Binding) void {
            self.entries[slot] = .{ .owner = owner, .binding = binding };
        }
        pub fn close(self: *Self, slot: usize) void { self.entries[slot] = .{}; }
        pub fn request(self: *Self, owner: u32, binding: queue.Binding) queue.Error!void {
            if (owner == 0 or binding.adapter == 0) return error.Invalid;
            for (&self.entries) |*entry| {
                if (entry.owner == 0 or entry.binding.adapter != binding.adapter) continue;
                if (entry.owner != owner) return error.WrongOwner;
                if (!std.meta.eql(entry.binding, binding)) return error.Stale;
                entry.pending = true;
                return;
            }
            return error.Stale;
        }
        pub fn take(self: *Self, slot: usize) bool {
            const pending = self.entries[slot].pending;
            self.entries[slot].pending = false;
            return pending;
        }
    };
}
pub fn Ingress(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        const Entry = struct {
            fence: queue.Fence = .{},
            owner: u32 = 0,
            active: bool = false,
            pending: bool = false,
            result: queue.Result = .pending,
            instant: u64 = 0,
        };
        entries: [capacity]Entry = .{Entry{}} ** capacity,
        pub fn arm(self: *Self, owner: u32, fence: queue.Fence) queue.Error!void {
            if (owner == 0 or fence.slot == 0 or fence.slot > capacity or fence.binding.adapter == 0) return error.Invalid;
            const entry = &self.entries[fence.slot - 1];
            if (entry.active or entry.pending) return error.Busy;
            entry.* = .{ .fence = fence, .owner = owner, .active = true };
        }
        pub fn acknowledge(self: *Self, owner: u32, fence: queue.Fence, result: queue.Result, quiesced: bool, instant: u64) queue.Error!void {
            if (fence.slot == 0 or fence.slot > capacity or (result != .complete and result != .failed and result != .cancelled)) return error.Invalid;
            const entry = &self.entries[fence.slot - 1];
            if (!std.meta.eql(entry.fence, fence)) return error.Stale;
            if (owner == 0 or entry.owner != owner) return error.WrongOwner;
            if (!entry.active) return error.AlreadyCompleted;
            if (!quiesced) return error.Busy;
            entry.active = false;
            entry.pending = true;
            entry.result = result;
            entry.instant = instant;
        }
        pub fn take(self: *Self, slot: usize) ?Ack {
            const entry = &self.entries[slot];
            if (!entry.pending) return null;
            entry.pending = false;
            return .{ .fence = entry.fence, .result = entry.result, .instant = entry.instant };
        }
        pub fn quiesce(self: *Self, fence: queue.Fence) void {
            const entry = &self.entries[fence.slot - 1];
            if (std.meta.eql(entry.fence, fence)) {
                entry.active = false;
                entry.pending = false;
            }
        }
    };
}
test "IRQ mailbox rejects wrong owners and stale resets and retains a single exact acknowledgement" {
    const t = std.testing;
    var ingress = Ingress(2){};
    const fence = queue.Fence{ .slot = 1, .timeline = 4, .point = 5, .binding = .{ .adapter = 9 } };
    try ingress.arm(7, fence);
    try t.expectError(error.Busy, ingress.acknowledge(7, fence, .complete, false, 1));
    try t.expectError(error.WrongOwner, ingress.acknowledge(8, fence, .complete, true, 1));
    var newer = fence;
    newer.binding.reset_generation += 1;
    try t.expectError(error.Stale, ingress.acknowledge(7, newer, .complete, true, 1));
    try ingress.acknowledge(7, fence, .complete, true, 2);
    try t.expectError(error.AlreadyCompleted, ingress.acknowledge(7, fence, .failed, true, 3));
    try t.expectError(error.Busy, ingress.arm(7, newer));
    const ack = ingress.take(0).?;
    try t.expectEqualDeep(fence, ack.fence);
    try t.expectEqual(@as(u64, 2), ack.instant);
    try t.expect(ingress.take(0) == null);
    try ingress.arm(7, newer);
    try t.expectError(error.Stale, ingress.acknowledge(7, fence, .complete, true, 4));
    ingress.quiesce(newer);
    try t.expectError(error.AlreadyCompleted, ingress.acknowledge(7, newer, .complete, true, 4));
    try ingress.arm(7, newer);
    try t.expectError(error.Busy, ingress.acknowledge(7, newer, .cancelled, false, 5));
    try t.expect(ingress.take(0) == null);
    try ingress.acknowledge(7, newer, .cancelled, true, 6);
    const cancelled = ingress.take(0).?;
    try t.expect(cancelled.result == .cancelled and std.meta.eql(cancelled.fence, newer));
    try t.expect(ingress.take(0) == null);
    var wake = Wakeups(2){};
    wake.bind(0, 7, fence.binding);
    try t.expectError(error.WrongOwner, wake.request(8, fence.binding));
    try wake.request(7, fence.binding);
    try wake.request(7, fence.binding);
    try t.expect(wake.take(0) and !wake.take(0));
    try wake.request(7, fence.binding); // Arrival while a prior callback runs.
    try t.expect(wake.take(0));
    wake.bind(0, 7, newer.binding);
    try t.expectError(error.Stale, wake.request(7, fence.binding));
    try wake.request(7, newer.binding);
    wake.close(0);
    try t.expect(!wake.take(0));
    try t.expectError(error.Stale, wake.request(7, newer.binding));
}
