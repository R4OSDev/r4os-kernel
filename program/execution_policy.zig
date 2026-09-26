const std = @import("std");

/// Owner declaration, not a permission or affinity interface. Unknown and
/// duplicate declarations fail closed; old modules retain their BSP policy.
pub const Policy = enum { bsp, owned_v1 };

pub fn parse(metadata: []const u8) ?Policy {
    var found: ?Policy = null;
    var items = std.mem.splitScalar(u8, metadata, 0);
    while (items.next()) |item| {
        const prefix = "runtime.parallel=";
        if (!std.mem.startsWith(u8, item, prefix)) continue;
        if (found != null) return null;
        const value = item[prefix.len..];
        found = if (std.mem.eql(u8, value, "owned-v1")) .owned_v1 else if (std.mem.eql(u8, value, "bsp")) .bsp else return null;
    }
    return found orelse .bsp;
}

test "parallel ownership metadata is explicit unique and independent of module name" {
    try std.testing.expectEqual(Policy.bsp, parse("r4x.name=LSTRX\x00").?);
    try std.testing.expectEqual(Policy.bsp, parse("runtime.parallel=bsp\x00").?);
    try std.testing.expectEqual(Policy.owned_v1, parse("r4x.name=CALC\x00runtime.parallel=owned-v1\x00").?);
    try std.testing.expectEqual(Policy.owned_v1, parse("runtime.parallel=owned-v1").?);
    try std.testing.expect(parse("runtime.parallel=all\x00") == null);
    try std.testing.expect(parse("runtime.parallel=owned-v1\x00runtime.parallel=owned-v1\x00") == null);
    try std.testing.expect(parse("runtime.parallel=bsp\x00runtime.parallel=owned-v1\x00") == null);
}
