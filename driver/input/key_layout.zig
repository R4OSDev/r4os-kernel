// Immutable tables/translation owned by Rust; fixed private C views only.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_input_layout_definition(u32) *const LayoutDefinition;
extern fn r4os_rust_input_layout_parse([*]const u8, usize) u32;
extern fn r4os_rust_input_layout_translate(u32, u8, bool, bool, *KeyChar) void;
extern fn r4os_rust_input_layout_ctrl(u32, u8) u16;
extern fn r4os_rust_input_layout_count() usize;
const types = @import("key_layout_types.zig");

pub const Layout = types.Layout;
pub const KeyChar = types.KeyChar;
pub const KeyEntry = types.KeyEntry;
pub const LayoutDefinition = types.LayoutDefinition;

pub const default_layout: Layout = .en_en;

pub const Modifiers = struct {
    shift: bool = false,
    altgr: bool = false,
};

pub fn definition(layout: Layout) *const LayoutDefinition {
    return r4os_rust_input_layout_definition(@intFromEnum(layout));
}

pub fn name(layout: Layout) []const u8 {
    return definition(layout).name.slice();
}

pub fn display(layout: Layout) []const u8 {
    return definition(layout).display.slice();
}

pub fn parseName(value_raw: []const u8) ?Layout {
    const id = r4os_rust_input_layout_parse(value_raw.ptr, value_raw.len);
    return if (id == 0xffffffff) null else @enumFromInt(id);
}

pub fn translate(layout: Layout, scancode: u8, modifiers: Modifiers) KeyChar {
    var out: KeyChar = undefined;
    r4os_rust_input_layout_translate(@intFromEnum(layout), scancode, modifiers.shift, modifiers.altgr, &out);
    return out;
}

pub fn ctrlChar(layout: Layout, scancode: u8) ?u8 {
    const value = r4os_rust_input_layout_ctrl(@intFromEnum(layout), scancode);
    return if (value == 256) null else @intCast(value);
}

pub fn count() usize {
    return r4os_rust_input_layout_count();
}

pub fn at(index: usize) ?Layout {
    return if (index >= count()) null else @enumFromInt(index);
}

pub fn indexOf(layout: Layout) ?usize {
    return @intFromEnum(layout);
}

test "layout name parser accepts stable ids and legacy aliases" {
    const testing = @import("std").testing;
    try testing.expectEqual(Layout.en_en, parseName("en_en").?);
    try testing.expectEqual(Layout.en_en, parseName("US").?);
    try testing.expectEqual(Layout.en_en, parseName("en-us").?);
    try testing.expectEqual(Layout.de_de, parseName("de_de").?);
    try testing.expectEqual(Layout.de_de, parseName("DE").?);
    try testing.expectEqual(Layout.de_de, parseName("German").?);
    try testing.expect(parseName("unknown") == null);
}

test "available layout list is stable and indexable" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(usize, 2), count());
    try testing.expectEqual(Layout.en_en, at(0).?);
    try testing.expectEqual(Layout.de_de, at(1).?);
    try testing.expect(at(2) == null);
    try testing.expectEqual(@as(usize, 0), indexOf(.en_en).?);
    try testing.expectEqual(@as(usize, 1), indexOf(.de_de).?);
}

test "en_en maps common normal and shifted ASCII keys" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(u8, 'y'), translate(.en_en, 0x15, .{}).ascii);
    try testing.expectEqual(@as(u8, 'Z'), translate(.en_en, 0x2C, .{ .shift = true }).ascii);
    try testing.expectEqual(@as(u8, '@'), translate(.en_en, 0x03, .{ .shift = true }).ascii);
    try testing.expectEqual(@as(u8, '|'), translate(.en_en, 0x2B, .{ .shift = true }).ascii);
}

test "de_de maps qwertz, punctuation and ascii altgr keys" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(u8, 'z'), translate(.de_de, 0x15, .{}).ascii);
    try testing.expectEqual(@as(u8, 'y'), translate(.de_de, 0x2C, .{}).ascii);
    try testing.expectEqual(@as(u8, '#'), translate(.de_de, 0x2B, .{}).ascii);
    try testing.expectEqual(@as(u8, '\''), translate(.de_de, 0x2B, .{ .shift = true }).ascii);
    try testing.expectEqual(@as(u8, '@'), translate(.de_de, 0x10, .{ .altgr = true }).ascii);
    try testing.expectEqual(@as(u8, '{'), translate(.de_de, 0x08, .{ .altgr = true }).ascii);
    try testing.expectEqual(@as(u8, '|'), translate(.de_de, 0x56, .{ .altgr = true }).ascii);
}

test "de_de keeps non ASCII targets without legacy ASCII lies" {
    const testing = @import("std").testing;
    const sharp_s = translate(.de_de, 0x0C, .{});
    try testing.expectEqual(@as(u21, 0x00DF), sharp_s.codepoint);
    try testing.expectEqual(@as(u8, 0), sharp_s.ascii);

    const umlaut = translate(.de_de, 0x1A, .{});
    try testing.expectEqual(@as(u21, 0x00FC), umlaut.codepoint);
    try testing.expectEqual(@as(u8, 0), umlaut.ascii);

    const euro = translate(.de_de, 0x12, .{ .altgr = true });
    try testing.expectEqual(@as(u21, 0x20AC), euro.codepoint);
    try testing.expectEqual(@as(u8, 0), euro.ascii);
}

test "ctrl chars follow the active letter layout" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(u8, 25), ctrlChar(.en_en, 0x15).?);
    try testing.expectEqual(@as(u8, 26), ctrlChar(.de_de, 0x15).?);
    try testing.expectEqual(@as(u8, 25), ctrlChar(.de_de, 0x2C).?);
}
