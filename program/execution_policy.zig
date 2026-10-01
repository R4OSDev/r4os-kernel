const std = @import("std");

/// Owner declaration, not a permission or affinity interface. Unknown and
/// duplicate declarations fail closed; old modules retain their BSP policy.
pub const Policy = enum { bsp, owned_v1 };

/// The declaration identifies a desktop owner independently of its launch
/// role. Admission limits this marker to GUI processes; it grants no rights.
pub fn desktopHost(metadata: []const u8) ?bool {
    var found: ?bool = null;
    var items = std.mem.splitScalar(u8, metadata, 0);
    while (items.next()) |item| {
        const prefix = "app.role=";
        if (!std.mem.startsWith(u8, item, prefix)) continue;
        if (found != null) return null;
        found = std.mem.eql(u8, item[prefix.len..], "desktop_host");
    }
    return found orelse false;
}

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
    try std.testing.expect(!desktopHost("r4x.name=R4DESK\x00").?);
    try std.testing.expect(desktopHost("app.role=desktop_host\x00").?);
    try std.testing.expect(!desktopHost("app.role=diagnostic\x00").?);
    try std.testing.expect(desktopHost("app.role=desktop_host\x00app.role=desktop_host\x00") == null);
    try std.testing.expect(desktopHost("app.role=diagnostic\x00app.role=desktop_host\x00") == null);
    try std.testing.expectEqual(Policy.bsp, parse("r4x.name=LSTRX\x00").?);
    try std.testing.expectEqual(Policy.bsp, parse("runtime.parallel=bsp\x00").?);
    try std.testing.expectEqual(Policy.owned_v1, parse("r4x.name=CALC\x00runtime.parallel=owned-v1\x00").?);
    try std.testing.expectEqual(Policy.owned_v1, parse("runtime.parallel=owned-v1").?);
    try std.testing.expect(parse("runtime.parallel=all\x00") == null);
    try std.testing.expect(parse("runtime.parallel=owned-v1\x00runtime.parallel=owned-v1\x00") == null);
    try std.testing.expect(parse("runtime.parallel=bsp\x00runtime.parallel=owned-v1\x00") == null);
}
