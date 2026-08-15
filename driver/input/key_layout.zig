const types = @import("key_layout_types.zig");
const en_en = @import("key_layouts/en_en.zig");
const de_de = @import("key_layouts/de_de.zig");

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
    return switch (layout) {
        .en_en => &en_en.definition,
        .de_de => &de_de.definition,
    };
}

pub fn name(layout: Layout) []const u8 {
    return definition(layout).name;
}

pub fn display(layout: Layout) []const u8 {
    return definition(layout).display;
}

pub fn parseName(value_raw: []const u8) ?Layout {
    const value = trim(value_raw);
    if (value.len == 0) return null;
    inline for (available) |layout| {
        const def = definition(layout);
        if (equalsLayoutName(value, def.name)) return layout;
        if (equalsLayoutName(value, def.display)) return layout;
        for (def.aliases) |alias| {
            if (equalsLayoutName(value, alias)) return layout;
        }
    }
    return null;
}

pub fn translate(layout: Layout, scancode: u8, modifiers: Modifiers) KeyChar {
    const entry = findEntry(definition(layout), scancode) orelse return .{};
    if (modifiers.altgr and modifiers.shift and entry.shift_altgr.hasCharacter()) return entry.shift_altgr;
    if (modifiers.altgr and entry.altgr.hasCharacter()) return entry.altgr;
    if (modifiers.shift and entry.shift.hasCharacter()) return entry.shift;
    return entry.normal;
}

pub fn ctrlChar(layout: Layout, scancode: u8) ?u8 {
    const ch = translate(layout, scancode, .{}).ascii;
    if (ch >= 'a' and ch <= 'z') return ch - 'a' + 1;
    if (ch >= 'A' and ch <= 'Z') return ch - 'A' + 1;
    return null;
}

const available = [_]Layout{ .en_en, .de_de };

pub fn count() usize {
    return available.len;
}

pub fn at(index: usize) ?Layout {
    if (index >= available.len) return null;
    return available[index];
}

pub fn indexOf(layout: Layout) ?usize {
    for (available, 0..) |candidate, index| {
        if (candidate == layout) return index;
    }
    return null;
}

fn findEntry(def: *const LayoutDefinition, scancode: u8) ?*const KeyEntry {
    for (def.entries) |*entry| {
        if (entry.scancode == scancode) return entry;
    }
    return null;
}

fn equalsLayoutName(a_raw: []const u8, b_raw: []const u8) bool {
    const a = trim(a_raw);
    const b = trim(b_raw);
    var ai: usize = 0;
    var bi: usize = 0;
    while (true) {
        while (ai < a.len and isNameSeparator(a[ai])) : (ai += 1) {}
        while (bi < b.len and isNameSeparator(b[bi])) : (bi += 1) {}
        if (ai >= a.len or bi >= b.len) return ai >= a.len and bi >= b.len;
        if (asciiUpper(a[ai]) != asciiUpper(b[bi])) return false;
        ai += 1;
        bi += 1;
    }
}

fn isNameSeparator(ch: u8) bool {
    return ch == '_' or ch == '-' or ch == ' ' or ch == '\t';
}

fn trim(value: []const u8) []const u8 {
    var start: usize = 0;
    var end: usize = value.len;
    while (start < end and (value[start] == ' ' or value[start] == '\t')) : (start += 1) {}
    while (end > start and (value[end - 1] == ' ' or value[end - 1] == '\t')) : (end -= 1) {}
    return value[start..end];
}

fn asciiUpper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
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
