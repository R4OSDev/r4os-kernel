//! Exact config/one-boot policy projection to its native implementation.
comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
const std = @import("std");
const Policy = @import("../display/backend_state.zig").Policy;
extern fn r4os_rust_graphics_policy(u32, [*]const u8, usize, u32, bool) i32;
pub fn configValue(value: []const u8) ?Policy {
    const code = r4os_rust_graphics_policy(0, value.ptr, value.len, 0, false);
    return if (code < 0) null else @enumFromInt(@as(u32, @intCast(code)));
}
pub fn softwareOnce(value: []const u8) bool { return r4os_rust_graphics_policy(1, value.ptr, value.len, 0, false) != 0; }
pub fn effective(config: Policy, once: bool) Policy { return @enumFromInt(@as(u32, @intCast(r4os_rust_graphics_policy(2, "".ptr, 0, @intFromEnum(config), once)))); }

test "display software boot selection is exact and overrides persistent auto" {
    const t = std.testing;
    try t.expect(softwareOnce("debug=1\tr4os.graphics=SOFTWARE quiet"));
    try t.expect(!softwareOnce("x-r4os.graphics=software r4os.graphics=software-extra"));
    try t.expectEqual(Policy.software_once, effective(.automatic, true));
    try t.expectEqual(Policy.software, effective(.software, false));
    try t.expectEqual(@as(?Policy, null), configValue("SW"));
}
