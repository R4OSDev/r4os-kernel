// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const Addresses = extern struct {
    local_ip: [4]u8,
    netmask: [4]u8,
    gateway_ip: [4]u8,
    dns_ip: [4]u8 = .{ 0, 0, 0, 0 },
    dns_configured: bool = false,
};

extern fn r4os_rust_net_config_rewrite([*]const u8, usize, [*]u8, usize, *const Addresses, *usize) bool;
extern fn r4os_rust_net_config_managed([*]const u8, usize) bool;
pub fn rewriteConfig(input: []const u8, output: []u8, addresses: Addresses) ?usize {
    var len: usize = 0;
    return if (r4os_rust_net_config_rewrite(input.ptr, input.len, output.ptr, output.len, &addresses, &len)) len else null;
}
pub fn isManagedNetOption(line_raw: []const u8) bool {
    return r4os_rust_net_config_managed(line_raw.ptr, line_raw.len);
}

test "rewrite replaces managed network lines and preserves unrelated lines" {
    const std = @import("std");
    const input =
        "SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\r\n" ++
        "OPTION NET ip=1.2.3.4 mask=255.0.255.0 # old compact line\r\n" ++
        "# keep me\r\n" ++
        "DRIVER=RTL8139\r\n" ++
        "DRIVER=RTL8168\r\n" ++
        "OPTION NET dns=bad\r\n";
    var out: [512]u8 = undefined;
    const len = rewriteConfig(input, out[0..], sampleAddresses(true)) orelse return error.RewriteFailed;
    try std.testing.expectEqualStrings(
        "SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\r\n" ++
            "OPTION NET ip=10.0.2.15\r\n" ++
            "OPTION NET mask=255.255.255.0\r\n" ++
            "OPTION NET gateway=10.0.2.2\r\n" ++
            "OPTION NET dns=10.0.2.3\r\n" ++
            "# keep me\r\n" ++
            "DRIVER=RTL8139\r\n" ++
            "DRIVER=RTL8168\r\n",
        out[0..len],
    );
}

test "rewrite appends missing network lines after existing config" {
    const std = @import("std");
    const input = "SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\r\nDRIVER=HDA\r\n";
    var out: [512]u8 = undefined;
    const len = rewriteConfig(input, out[0..], sampleAddresses(true)) orelse return error.RewriteFailed;
    try std.testing.expectEqualStrings(
        "SHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\r\n" ++
            "DRIVER=HDA\r\n" ++
            "\r\n" ++
            "OPTION NET ip=10.0.2.15\r\n" ++
            "OPTION NET mask=255.255.255.0\r\n" ++
            "OPTION NET gateway=10.0.2.2\r\n" ++
            "OPTION NET dns=10.0.2.3\r\n",
        out[0..len],
    );
}

test "rewrite handles empty files and optional dns" {
    const std = @import("std");
    var out: [256]u8 = undefined;
    const len = rewriteConfig("", out[0..], sampleAddresses(false)) orelse return error.RewriteFailed;
    try std.testing.expectEqualStrings(
        "OPTION NET ip=10.0.2.15\r\n" ++
            "OPTION NET mask=255.255.255.0\r\n" ++
            "OPTION NET gateway=10.0.2.2\r\n",
        out[0..len],
    );
}

test "rewrite preserves utf8 bom and strips stale dns aliases" {
    const std = @import("std");
    const input = "\xEF\xBB\xBFSHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\nOPTION NET nameserver=1.1.1.1\n";
    var out: [512]u8 = undefined;
    const len = rewriteConfig(input, out[0..], sampleAddresses(false)) orelse return error.RewriteFailed;
    try std.testing.expectEqualStrings(
        "\xEF\xBB\xBFSHELL=/R4OS/SOFTWARE/DESKTOP/R4DESK.R4X\r\n" ++
            "OPTION NET ip=10.0.2.15\r\n" ++
            "OPTION NET mask=255.255.255.0\r\n" ++
            "OPTION NET gateway=10.0.2.2\r\n",
        out[0..len],
    );
}

test "rewrite rejects undersized output buffer" {
    const std = @import("std");
    var out: [16]u8 = undefined;
    try std.testing.expectEqual(@as(?usize, null), rewriteConfig("", out[0..], sampleAddresses(true)));
}

test "managed option detector is narrow" {
    const std = @import("std");
    try std.testing.expect(isManagedNetOption(" OPTION NET gateway=10.0.2.2 # old"));
    try std.testing.expect(isManagedNetOption("OPTION NET addr=10.0.2.15"));
    try std.testing.expect(isManagedNetOption("OPTION NET dhcp=on"));
    try std.testing.expect(!isManagedNetOption("# OPTION NET ip=1.2.3.4"));
    try std.testing.expect(!isManagedNetOption("OPTION SID model=8580"));
}

fn sampleAddresses(dns_configured: bool) Addresses {
    return .{
        .local_ip = .{ 10, 0, 2, 15 },
        .netmask = .{ 255, 255, 255, 0 },
        .gateway_ip = .{ 10, 0, 2, 2 },
        .dns_ip = .{ 10, 0, 2, 3 },
        .dns_configured = dns_configured,
    };
}

comptime {
    if (@sizeOf(Addresses) != 17 or @alignOf(Addresses) != 1 or @offsetOf(Addresses, "local_ip") != 0 or @offsetOf(Addresses, "netmask") != 4 or @offsetOf(Addresses, "gateway_ip") != 8 or @offsetOf(Addresses, "dns_ip") != 12 or @offsetOf(Addresses, "dns_configured") != 16) @compileError("Native network Addresses layout");
}
