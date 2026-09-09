const std = @import("std");
const Policy = @import("../display/backend_state.zig").Policy;

pub fn configValue(value: []const u8) ?Policy {
    if (std.ascii.eqlIgnoreCase(value, "AUTO")) return .automatic;
    if (std.ascii.eqlIgnoreCase(value, "SOFTWARE")) return .software;
    return null;
}

// Limine selection/edit applies to this boot only and does not rewrite CONFIG.
pub fn softwareOnce(command_line: []const u8) bool {
    var tokens = std.mem.tokenizeAny(u8, command_line, " \t\r\n");
    while (tokens.next()) |token| {
        if (std.ascii.eqlIgnoreCase(token, "r4os.graphics=software")) return true;
    }
    return false;
}

pub fn effective(config: Policy, once: bool) Policy {
    return if (once) .software_once else config;
}

test "display software boot selection is exact and overrides persistent auto" {
    const t = std.testing;
    try t.expect(softwareOnce("debug=1\tr4os.graphics=SOFTWARE quiet"));
    try t.expect(!softwareOnce("x-r4os.graphics=software r4os.graphics=software-extra"));
    try t.expectEqual(Policy.software_once, effective(.automatic, true));
    try t.expectEqual(Policy.software, effective(.software, false));
    try t.expectEqual(@as(?Policy, null), configValue("SW"));
}
