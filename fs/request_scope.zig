// Private C facade to the single native filesystem policy owner.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const lane_count: usize = 26;
pub const Plan = extern struct {
    lanes: [lane_count]u8 = .{0} ** lane_count,
    count: u8 = 0,
    pub fn isGlobal(self: Plan) bool {
        return r4os_rust_fs_scope_global(&self);
    }
};
extern fn r4os_rust_fs_scope_plan(u8, u8, u8, *Plan) callconv(.c) void;
extern fn r4os_rust_fs_scope_global(*const Plan) callconv(.c) bool;
extern fn r4os_rust_fs_drive_code(u8) callconv(.c) u32;
pub fn single(letter: u8) Plan {
    var out: Plan = undefined;
    r4os_rust_fs_scope_plan(0, letter, 0, &out);
    return out;
}
pub fn pair(first: u8, second: u8) Plan {
    var out: Plan = undefined;
    r4os_rust_fs_scope_plan(1, first, second, &out);
    return out;
}
pub fn all() Plan {
    var out: Plan = undefined;
    r4os_rust_fs_scope_plan(2, 0, 0, &out);
    return out;
}
pub fn driveCode(letter: u8) u32 {
    return r4os_rust_fs_drive_code(letter);
}
comptime {
    if (@sizeOf(Plan) != 27 or @offsetOf(Plan, "count") != 26) @compileError("private request scope layout drift");
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
