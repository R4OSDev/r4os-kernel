const std = @import("std");

pub const Guid = [16]u8;
pub const zero: Guid = .{0} ** 16;
const order = [_]usize{ 3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15 };

pub fn parse(text: []const u8) ?Guid {
    if (text.len != 36) return null;
    var result: Guid = undefined;
    var pos: usize = 0;
    for (order, 0..) |destination, i| {
        if (i == 4 or i == 6 or i == 8 or i == 10) {
            if (text[pos] != '-') return null;
            pos += 1;
        }
        const hi = nibble(text[pos]) orelse return null;
        const lo = nibble(text[pos + 1]) orelse return null;
        result[destination] = hi * 16 + lo;
        pos += 2;
    }
    return result;
}

pub fn format(value: Guid) [36]u8 {
    const hex = "0123456789abcdef";
    var result: [36]u8 = undefined;
    var pos: usize = 0;
    for (order, 0..) |source, i| {
        if (i == 4 or i == 6 or i == 8 or i == 10) {
            result[pos] = '-';
            pos += 1;
        }
        result[pos] = hex[value[source] >> 4];
        result[pos + 1] = hex[value[source] & 15];
        pos += 2;
    }
    return result;
}

pub fn eql(a: Guid, b: Guid) bool {
    return std.mem.eql(u8, &a, &b);
}
pub fn isZero(value: Guid) bool {
    return eql(value, zero);
}

fn nibble(c: u8) ?u8 {
    return switch (c) {
        '0'...'9' => c - '0',
        'a'...'f' => c - 'a' + 10,
        else => null,
    };
}

test "UUID text uses GPT field byte order and exact canonical syntax" {
    const text = "00112233-4455-6677-8899-aabbccddeeff";
    const value = parse(text).?;
    try std.testing.expectEqualSlices(u8, &.{ 0x33, 0x22, 0x11, 0, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff }, &value);
    try std.testing.expectEqualStrings(text, &format(value));
    try std.testing.expect(parse("00112233-4455-6677-8899-AABBCCDDEEFF") == null);
}
