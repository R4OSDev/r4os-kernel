const std = @import("std");
const owner_test = if (@import("builtin").is_test) @import("rust_owner_test") else struct {};
comptime {
    if (@import("builtin").is_test) _ = owner_test;
}
pub const Guid = [16]u8;
pub const zero: Guid = .{0} ** 16;
extern fn r4os_rust_storage_guid_parse(text: [*]const u8, len: usize, out: *Guid) bool;
extern fn r4os_rust_storage_guid_format(input: *const Guid, out: *[36]u8) void;
extern fn r4os_rust_storage_guid_equal(left: *const Guid, right: *const Guid) bool;
pub fn parse(text: []const u8) ?Guid {
    var value: Guid = undefined;
    return if (r4os_rust_storage_guid_parse(text.ptr, text.len, &value)) value else null;
}
pub fn format(value: Guid) [36]u8 {
    var out: [36]u8 = undefined;
    r4os_rust_storage_guid_format(&value, &out);
    return out;
}
pub fn eql(a: Guid, b: Guid) bool {
    return r4os_rust_storage_guid_equal(&a, &b);
}
pub fn isZero(value: Guid) bool {
    return eql(value, zero);
}

test "UUID text uses GPT field byte order and exact canonical syntax" {
    const text = "00112233-4455-6677-8899-aabbccddeeff";
    const value = parse(text).?;
    try std.testing.expectEqualSlices(u8, &.{ 0x33, 0x22, 0x11, 0, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff }, &value);
    try std.testing.expectEqualStrings(text, &format(value));
    try std.testing.expect(parse("00112233-4455-6677-8899-AABBCCDDEEFF") == null);
}
