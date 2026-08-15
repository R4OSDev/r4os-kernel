pub const Addresses = struct {
    local_ip: [4]u8,
    netmask: [4]u8,
    gateway_ip: [4]u8,
    dns_ip: [4]u8 = .{ 0, 0, 0, 0 },
    dns_configured: bool = false,
};

pub fn rewriteConfig(input: []const u8, output: []u8, addresses: Addresses) ?usize {
    var out_len: usize = 0;
    var inserted = false;
    var start: usize = 0;
    var data = input;
    if (data.len >= 3 and data[0] == 0xEF and data[1] == 0xBB and data[2] == 0xBF) {
        appendBytes(output, &out_len, data[0..3]) orelse return null;
        data = data[3..];
    }

    while (start < data.len) {
        var end = start;
        while (end < data.len and data[end] != '\n' and data[end] != '\r') : (end += 1) {}
        const line = data[start..end];
        if (isManagedNetOption(line)) {
            if (!inserted) {
                appendNetConfigLines(output, &out_len, addresses) orelse return null;
                inserted = true;
            }
        } else {
            appendBytes(output, &out_len, line) orelse return null;
            appendBytes(output, &out_len, "\r\n") orelse return null;
        }
        start = end;
        while (start < data.len and (data[start] == '\n' or data[start] == '\r')) : (start += 1) {}
    }

    if (!inserted) {
        if (out_len > 0 and output[out_len - 1] != '\n') appendBytes(output, &out_len, "\r\n") orelse return null;
        if (out_len > 0) appendBytes(output, &out_len, "\r\n") orelse return null;
        appendNetConfigLines(output, &out_len, addresses) orelse return null;
    }
    return out_len;
}

pub fn isManagedNetOption(line_raw: []const u8) bool {
    const comment = indexOfByte(line_raw, '#') orelse line_raw.len;
    const line = trimAscii(line_raw[0..comment]);
    if (!startsWithIgnoreCase(line, "OPTION ")) return false;
    const rest = trimAscii(line[7..]);
    const space = indexOfByte(rest, ' ') orelse return false;
    if (!equalsIgnoreCase(rest[0..space], "NET")) return false;
    const body = trimAscii(rest[space + 1 ..]);
    const eq = indexOfByte(body, '=') orelse return false;
    const key = trimAscii(body[0..eq]);
    return equalsIgnoreCase(key, "dhcp") or
        equalsIgnoreCase(key, "ip") or equalsIgnoreCase(key, "addr") or equalsIgnoreCase(key, "address") or
        equalsIgnoreCase(key, "mask") or equalsIgnoreCase(key, "netmask") or
        equalsIgnoreCase(key, "gateway") or equalsIgnoreCase(key, "gw") or
        equalsIgnoreCase(key, "dns") or equalsIgnoreCase(key, "nameserver");
}

fn appendNetConfigLines(out: []u8, len: *usize, addresses: Addresses) ?void {
    appendBytes(out, len, "OPTION NET ip=") orelse return null;
    appendIp(out, len, addresses.local_ip) orelse return null;
    appendBytes(out, len, "\r\nOPTION NET mask=") orelse return null;
    appendIp(out, len, addresses.netmask) orelse return null;
    appendBytes(out, len, "\r\nOPTION NET gateway=") orelse return null;
    appendIp(out, len, addresses.gateway_ip) orelse return null;
    if (addresses.dns_configured) {
        appendBytes(out, len, "\r\nOPTION NET dns=") orelse return null;
        appendIp(out, len, addresses.dns_ip) orelse return null;
    }
    appendBytes(out, len, "\r\n") orelse return null;
}

fn appendIp(out: []u8, len: *usize, ip: [4]u8) ?void {
    appendDecByte(out, len, ip[0]) orelse return null;
    appendBytes(out, len, ".") orelse return null;
    appendDecByte(out, len, ip[1]) orelse return null;
    appendBytes(out, len, ".") orelse return null;
    appendDecByte(out, len, ip[2]) orelse return null;
    appendBytes(out, len, ".") orelse return null;
    appendDecByte(out, len, ip[3]) orelse return null;
}

fn appendDecByte(out: []u8, len: *usize, value: u8) ?void {
    var tmp: [3]u8 = undefined;
    var pos = tmp.len;
    var n = value;
    if (n == 0) return appendBytes(out, len, "0");
    while (n > 0) {
        pos -= 1;
        tmp[pos] = '0' + (n % 10);
        n /= 10;
    }
    return appendBytes(out, len, tmp[pos..]);
}

fn appendBytes(out: []u8, len: *usize, text: []const u8) ?void {
    if (len.* + text.len > out.len) return null;
    if (text.len > 0) @memcpy(out[len.* .. len.* + text.len], text);
    len.* += text.len;
}

fn trimAscii(s: []const u8) []const u8 {
    var start: usize = 0;
    var end: usize = s.len;
    while (start < end and (s[start] == ' ' or s[start] == '\t')) : (start += 1) {}
    while (end > start and (s[end - 1] == ' ' or s[end - 1] == '\t')) : (end -= 1) {}
    return s[start..end];
}

fn startsWithIgnoreCase(s: []const u8, prefix: []const u8) bool {
    return s.len >= prefix.len and equalsIgnoreCase(s[0..prefix.len], prefix);
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        const ca = if (a[i] >= 'A' and a[i] <= 'Z') a[i] + 32 else a[i];
        const cb = if (b[i] >= 'A' and b[i] <= 'Z') b[i] + 32 else b[i];
        if (ca != cb) return false;
    }
    return true;
}

fn indexOfByte(s: []const u8, ch: u8) ?usize {
    var i: usize = 0;
    while (i < s.len) : (i += 1) if (s[i] == ch) return i;
    return null;
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
