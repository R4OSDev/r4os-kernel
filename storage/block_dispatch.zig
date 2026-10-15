const std = @import("std");
const owner_test = if (@import("builtin").is_test) @import("rust_owner_test") else struct {};
comptime {
    if (@import("builtin").is_test) _ = owner_test;
}
pub const max_controllers: usize = 8;
pub const max_controller_name: usize = 32;
pub const no_lane: u8 = 0xFF;
pub const BufferOwnership = enum(u8) { none, bounce_owned, borrowed_resident };
pub const ActiveTimeoutAction = enum(u8) { detach, detach_with_buffer, wait_for_completion };
pub fn activeTimeoutAction(ownership: BufferOwnership) ActiveTimeoutAction {
    return @enumFromInt(r4os_rust_storage_timeout_action(@intFromEnum(ownership)));
}
pub const Completion = extern struct { result: i32 = 0, bytes: u32 = 0 };
pub const CompletionLatch = extern struct {
    handle: u64 = 0,
    claimed: bool = false,
    ready: bool = false,
    result: i32 = 0,
    bytes: u32 = 0,
    pub fn activate(self: *CompletionLatch, handle: u64) bool {
        return r4os_rust_storage_latch(self, 0, handle, 0, 0, null);
    }
    pub fn publish(self: *CompletionLatch, handle: u64, result: i32, bytes: u32) bool {
        return r4os_rust_storage_latch(self, 1, handle, result, bytes, null);
    }
    pub fn rejectSubmission(self: *CompletionLatch, handle: u64) bool {
        return r4os_rust_storage_latch(self, 2, handle, 0, 0, null);
    }
    pub fn take(self: *CompletionLatch) ?Completion {
        var out: Completion = undefined;
        return if (r4os_rust_storage_latch(self, 3, 0, 0, 0, &out)) out else null;
    }
    pub fn invalidate(self: *CompletionLatch) void {
        _ = r4os_rust_storage_latch(self, 4, 0, 0, 0, null);
    }
};
pub fn submissionAllowed(active: u32, max: u16, flush: bool, flush_in_flight: bool) bool {
    return r4os_rust_storage_submission(active, max, flush, flush_in_flight);
}
pub const ControllerMap = extern struct {
    used: [max_controllers]bool = .{false} ** max_controllers,
    names: [max_controllers][max_controller_name]u8 = .{.{0} ** max_controller_name} ** max_controllers,
    lengths: [max_controllers]u8 = .{0} ** max_controllers,
    pub fn assign(self: *ControllerMap, name: []const u8) ?u8 {
        const lane = r4os_rust_storage_controller_assign(self, name.ptr, name.len);
        return if (lane == no_lane) null else lane;
    }
    pub fn clear(self: *ControllerMap, lane: u8) void {
        _ = r4os_rust_storage_controller(self, true, lane);
    }
    pub fn count(self: *const ControllerMap) u32 {
        return r4os_rust_storage_controller(@constCast(self), false, 0);
    }
};
extern fn r4os_rust_storage_timeout_action(ownership: u8) u8;
extern fn r4os_rust_storage_submission(active: u32, max: u16, flush: bool, flush_in_flight: bool) bool;
extern fn r4os_rust_storage_latch(state: *CompletionLatch, op: u8, handle: u64, result: i32, bytes: u32, out: ?*Completion) bool;
extern fn r4os_rust_storage_controller_assign(state: *ControllerMap, input: [*]const u8, len: usize) u8;
extern fn r4os_rust_storage_controller(state: *ControllerMap, clear: bool, lane: u8) u32;
comptime {
    if (@sizeOf(CompletionLatch) != 24 or @offsetOf(CompletionLatch, "result") != 12 or @sizeOf(ControllerMap) != 272 or @sizeOf(Completion) != 8) @compileError("private block dispatch C layout drift");
}

test "controller map groups one owner and separates independent owners" {
    var map: ControllerMap = .{};
    const ahci = map.assign("ich9-ahci") orelse return error.NoLane;
    const same_ahci = map.assign("ICH9-AHCI") orelse return error.NoLane;
    const xhci = map.assign("xhci") orelse return error.NoLane;
    try std.testing.expectEqual(ahci, same_ahci);
    try std.testing.expect(ahci != xhci);
    try std.testing.expectEqual(@as(u32, 2), map.count());
}

test "controller lane can be reused only after explicit release" {
    var map: ControllerMap = .{};
    const first = map.assign("first") orelse return error.NoLane;
    map.clear(first);
    const replacement = map.assign("replacement") orelse return error.NoLane;
    try std.testing.expectEqual(first, replacement);
}

test "active timeout preserves borrowed resident buffer lifetime" {
    try std.testing.expectEqual(ActiveTimeoutAction.detach, activeTimeoutAction(.none));
    try std.testing.expectEqual(ActiveTimeoutAction.detach_with_buffer, activeTimeoutAction(.bounce_owned));
    try std.testing.expectEqual(ActiveTimeoutAction.wait_for_completion, activeTimeoutAction(.borrowed_resident));
}

test "completion latch accepts exactly one matching publication" {
    var latch: CompletionLatch = .{};
    try std.testing.expect(latch.activate(0x1234));
    try std.testing.expect(!latch.publish(0x4321, 0, 512));
    try std.testing.expect(latch.publish(0x1234, 0, 512));
    try std.testing.expect(!latch.publish(0x1234, -1, 0));
    const completion = latch.take() orelse return error.MissingCompletion;
    try std.testing.expectEqual(@as(i32, 0), completion.result);
    try std.testing.expectEqual(@as(u32, 512), completion.bytes);
    try std.testing.expect(latch.take() == null);
    latch.invalidate();
    try std.testing.expect(!latch.publish(0x1234, 0, 512));
}

test "inline completion wins over a conflicting submit rejection" {
    var latch: CompletionLatch = .{};
    try std.testing.expect(latch.activate(9));
    try std.testing.expect(latch.publish(9, 0, 4096));
    try std.testing.expect(!latch.rejectSubmission(9));
}

test "queue depth is in-flight capacity and flush is an exclusive barrier" {
    try std.testing.expect(submissionAllowed(0, 2, false, false));
    try std.testing.expect(submissionAllowed(1, 2, false, false));
    try std.testing.expect(!submissionAllowed(2, 2, false, false));
    try std.testing.expect(!submissionAllowed(1, 2, true, false));
    try std.testing.expect(submissionAllowed(0, 2, true, false));
    try std.testing.expect(!submissionAllowed(0, 2, false, true));
}
