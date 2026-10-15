//! Typed facade for the unchanged native program-owner declarations.
const std = @import("std");
pub const Policy = enum(u8) { bsp, owned_v1 };
extern fn r4os_rust_program_parallel([*]const u8, usize) u8;
extern fn r4os_rust_program_desktop_host([*]const u8, usize) u8;
pub fn desktopHost(metadata: []const u8) ?bool {
    return switch (r4os_rust_program_desktop_host(metadata.ptr, metadata.len)) {
        0 => false,
        1 => true,
        255 => null,
        else => unreachable,
    };
}
pub fn parse(metadata: []const u8) ?Policy {
    const value = r4os_rust_program_parallel(metadata.ptr, metadata.len);
    return if (value == 255) null else @enumFromInt(value);
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
