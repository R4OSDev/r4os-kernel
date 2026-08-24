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
