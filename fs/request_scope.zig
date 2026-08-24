const std = @import("std");

pub const lane_count: usize = 26;

pub const Plan = struct {
    lanes: [lane_count]u8 = .{0} ** lane_count,
    count: u8 = 0,

    pub fn isGlobal(self: Plan) bool {
        return self.count == lane_count;
    }
};

pub fn single(drive_letter: u8) Plan {
    const lane = driveLane(drive_letter) orelse return all();
    var plan: Plan = .{};
    plan.lanes[0] = lane;
    plan.count = 1;
    return plan;
}

pub fn pair(first_drive: u8, second_drive: u8) Plan {
    const first = driveLane(first_drive) orelse return all();
    const second = driveLane(second_drive) orelse return all();
    var plan: Plan = .{};
    if (first == second) {
        plan.lanes[0] = first;
        plan.count = 1;
        return plan;
    }
    plan.lanes[0] = @min(first, second);
    plan.lanes[1] = @max(first, second);
    plan.count = 2;
    return plan;
}

pub fn all() Plan {
    var plan: Plan = .{};
    while (plan.count < lane_count) : (plan.count += 1) {
        plan.lanes[plan.count] = plan.count;
    }
    return plan;
}

pub fn driveCode(drive_letter: u8) u32 {
    return if (drive_letter >= 'a' and drive_letter <= 'z')
        @as(u32, drive_letter - ('a' - 'A'))
    else
        @as(u32, drive_letter);
}

fn driveLane(drive_letter: u8) ?u8 {
    const upper = driveCode(drive_letter);
    if (upper < 'A' or upper > 'Z') return null;
    return @intCast(upper - 'A');
}

test "single drive scopes are case insensitive" {
    const upper = single('C');
    const lower = single('c');
    try std.testing.expectEqual(@as(u8, 1), upper.count);
    try std.testing.expectEqualSlices(u8, upper.lanes[0..upper.count], lower.lanes[0..lower.count]);
    try std.testing.expectEqual(@as(u8, 2), upper.lanes[0]);
}

test "cross drive scopes use one canonical order" {
    const forward = pair('C', 'F');
    const reverse = pair('F', 'C');
    try std.testing.expectEqual(@as(u8, 2), forward.count);
    try std.testing.expectEqualSlices(u8, forward.lanes[0..forward.count], reverse.lanes[0..reverse.count]);
    try std.testing.expect(forward.lanes[0] < forward.lanes[1]);

    const same = pair('C', 'c');
    try std.testing.expectEqual(@as(u8, 1), same.count);
}

test "unknown ownership conservatively covers every drive" {
    const global = single(0);
    try std.testing.expect(global.isGlobal());
    for (global.lanes, 0..) |lane, index| {
        try std.testing.expectEqual(@as(u8, @intCast(index)), lane);
    }
}
