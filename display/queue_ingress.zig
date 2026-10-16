//! Fixed native IRQ mailbox/coalesced wake projection under the runtime owner.
comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
const std = @import("std");
const queue = @import("queue_state.zig");
pub const Ack = extern struct { fence: queue.Fence, result: queue.Result, instant: u64 };
pub const WakeEntry = extern struct { owner: u32 = 0, binding: queue.Binding = .{}, pending: bool = false };
pub const Entry = extern struct { fence: queue.Fence = .{}, owner: u32 = 0, active: bool = false, pending: bool = false, result: queue.Result = .pending, instant: u64 = 0 };
const Args = extern struct { owner: u32 = 0, binding: queue.Binding = .{}, fence: queue.Fence = .{}, result: u32 = 0, quiesced: bool = false, instant: u64 = 0 };
const OptionalAck = extern struct { has: bool, value: Ack };
extern fn r4os_rust_display_queue_ingress(u32, *anyopaque, usize, usize, *const Args, ?*anyopaque) i32;
fn status(code: i32) queue.Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 7 => error.Stale, 8 => error.Busy, 10 => error.WrongOwner, 16 => error.AlreadyCompleted, else => unreachable }; }
pub fn Wakeups(comptime capacity: usize) type {
    return struct {
        const Self = @This(); entries: [capacity]WakeEntry = @splat(.{}),
        pub fn bind(self: *Self, slot: usize, owner: u32, binding: queue.Binding) void { status(r4os_rust_display_queue_ingress(0, &self.entries, capacity, slot, &.{ .owner = owner, .binding = binding }, null)) catch unreachable; }
        pub fn close(self: *Self, slot: usize) void { status(r4os_rust_display_queue_ingress(1, &self.entries, capacity, slot, &.{}, null)) catch unreachable; }
        pub fn request(self: *Self, owner: u32, binding: queue.Binding) queue.Error!void { try status(r4os_rust_display_queue_ingress(2, &self.entries, capacity, 0, &.{ .owner = owner, .binding = binding }, null)); }
        pub fn take(self: *Self, slot: usize) bool { var value: bool = undefined; status(r4os_rust_display_queue_ingress(3, &self.entries, capacity, slot, &.{}, &value)) catch unreachable; return value; }
    };
}
pub fn Ingress(comptime capacity: usize) type {
    return struct {
        const Self = @This(); entries: [capacity]Entry = @splat(.{}),
        pub fn arm(self: *Self, owner: u32, fence: queue.Fence) queue.Error!void { try status(r4os_rust_display_queue_ingress(4, &self.entries, capacity, 0, &.{ .owner = owner, .fence = fence }, null)); }
        pub fn acknowledge(self: *Self, owner: u32, fence: queue.Fence, result: queue.Result, quiesced: bool, instant: u64) queue.Error!void {
            try status(r4os_rust_display_queue_ingress(5, &self.entries, capacity, 0, &.{ .owner = owner, .fence = fence, .result = @intFromEnum(result), .quiesced = quiesced, .instant = instant }, null));
        }
        pub fn take(self: *Self, slot: usize) ?Ack {
            var value: OptionalAck = undefined; status(r4os_rust_display_queue_ingress(6, &self.entries, capacity, slot, &.{}, &value)) catch unreachable; return if (value.has) value.value else null;
        }
        pub fn quiesce(self: *Self, fence: queue.Fence) void { status(r4os_rust_display_queue_ingress(7, &self.entries, capacity, 0, &.{ .fence = fence }, null)) catch unreachable; }
    };
}
comptime {
    if (@sizeOf(Ack) != 64 or @sizeOf(WakeEntry) != 40 or @sizeOf(Entry) != 72 or @alignOf(Entry) != 8 or @offsetOf(WakeEntry, "binding") != 8 or @offsetOf(WakeEntry, "pending") != 32 or @offsetOf(Entry, "result") != 56 or @offsetOf(Entry, "instant") != 64) @compileError("Native IRQ mailbox C layout");
    if (@sizeOf(Args) != 96 or @offsetOf(Args, "binding") != 8 or @offsetOf(Args, "fence") != 32 or @offsetOf(Args, "instant") != 88 or @sizeOf(OptionalAck) != 72) @compileError("Native IRQ mailbox call C layout");
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
