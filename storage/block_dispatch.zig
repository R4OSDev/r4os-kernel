const std = @import("std");

pub const max_controllers: usize = 8;
pub const max_controller_name: usize = 32;
pub const no_lane: u8 = 0xFF;

pub const BufferOwnership = enum(u8) {
    none,
    bounce_owned,
    borrowed_resident,
};

pub const ActiveTimeoutAction = enum(u8) {
    detach,
    detach_with_buffer,
    wait_for_completion,
};

pub fn activeTimeoutAction(ownership: BufferOwnership) ActiveTimeoutAction {
    return switch (ownership) {
        .none => .detach,
        .bounce_owned => .detach_with_buffer,
        .borrowed_resident => .wait_for_completion,
    };
}

pub const Completion = struct {
    result: i32 = 0,
    bytes: u32 = 0,
};

// IRQ callbacks only claim this latch; the owning block worker consumes the
// publication and performs queue, buffer and waiter teardown.  A handle is
// never accepted twice and invalidation happens before a request slot can be
// reused.
pub const CompletionLatch = struct {
    handle: u64 = 0,
    claimed: bool = false,
    ready: bool = false,
    result: i32 = 0,
    bytes: u32 = 0,

    pub fn activate(self: *CompletionLatch, handle: u64) bool {
        if (handle == 0) return false;
        self.* = .{ .handle = handle };
        return true;
    }

    pub fn publish(self: *CompletionLatch, handle: u64, result: i32, bytes: u32) bool {
        if (handle == 0 or self.handle != handle or self.claimed) return false;
        self.claimed = true;
        self.ready = true;
        self.result = result;
        self.bytes = bytes;
        return true;
    }

    // Used when submit rejects a request before ownership reaches hardware.
    // An inline completion published by the backend wins the race.
    pub fn rejectSubmission(self: *CompletionLatch, handle: u64) bool {
        if (handle == 0 or self.handle != handle or self.claimed) return false;
        self.claimed = true;
        return true;
    }

    pub fn take(self: *CompletionLatch) ?Completion {
        if (!self.ready) return null;
        self.ready = false;
        return .{ .result = self.result, .bytes = self.bytes };
    }

    pub fn invalidate(self: *CompletionLatch) void {
        self.* = .{};
    }
};

pub fn submissionAllowed(
    active_count: u32,
    max_in_flight: u16,
    candidate_is_flush: bool,
    flush_in_flight: bool,
) bool {
    if (max_in_flight == 0 or active_count >= max_in_flight) return false;
    if (flush_in_flight) return false;
    if (candidate_is_flush and active_count != 0) return false;
    return true;
}

pub const ControllerMap = struct {
    used: [max_controllers]bool = .{false} ** max_controllers,
    names: [max_controllers][max_controller_name]u8 =
        .{.{0} ** max_controller_name} ** max_controllers,
    lengths: [max_controllers]u8 = .{0} ** max_controllers,

    pub fn assign(self: *ControllerMap, controller: []const u8) ?u8 {
        const name = normalizedName(controller) orelse return null;
        var free: ?usize = null;
        var index: usize = 0;
        while (index < max_controllers) : (index += 1) {
            if (!self.used[index]) {
                if (free == null) free = index;
                continue;
            }
            const len: usize = self.lengths[index];
            if (equalIgnoreCase(self.names[index][0..len], name)) return @intCast(index);
        }
        const target = free orelse return null;
        self.used[target] = true;
        self.lengths[target] = @intCast(name.len);
        @memcpy(self.names[target][0..name.len], name);
        return @intCast(target);
    }

    pub fn clear(self: *ControllerMap, lane: u8) void {
        if (lane >= max_controllers) return;
        self.used[lane] = false;
        self.lengths[lane] = 0;
        self.names[lane] = .{0} ** max_controller_name;
    }

    pub fn count(self: *const ControllerMap) u32 {
        var total: u32 = 0;
        for (self.used) |used| if (used) {
            total += 1;
        };
        return total;
    }
};

fn normalizedName(controller: []const u8) ?[]const u8 {
    if (controller.len == 0) return "unknown";
    if (controller.len > max_controller_name) return null;
    return controller;
}

fn equalIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |left, right| {
        const left_lower = if (left >= 'A' and left <= 'Z') left + ('a' - 'A') else left;
        const right_lower = if (right >= 'A' and right <= 'Z') right + ('a' - 'A') else right;
        if (left_lower != right_lower) return false;
    }
    return true;
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
