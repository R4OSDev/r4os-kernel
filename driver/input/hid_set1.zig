// Native input helper; caller retains its existing queue/storage owner.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_input_hid_code(u8, bool, *Code) bool;
extern fn r4os_rust_input_hid_released([*]const u8, usize, usize, [*]const u8, usize, usize, [*]u8, usize) usize;
const std = @import("std");
const physical_key = @import("physical_key.zig");

/// One IBM PC/AT Set-1 make code. `extended` emits the E0 prefix for both the
/// make and break transition. Keeping this bridge pure makes USB and PS/2
/// keyboard identity directly comparable in host tests.
pub const Code = extern struct {
    make: u8,
    extended: bool = false,
};

pub fn usageToCode(usage: u8) ?Code {
    var out: Code = undefined;
    return if (r4os_rust_input_hid_code(usage, false, &out)) out else null;
}

pub fn modifierToCode(bit: u8) ?Code {
    var out: Code = undefined;
    return if (r4os_rust_input_hid_code(bit, true, &out)) out else null;
}

/// Returns usages present in the previous boot-keyboard key array but absent
/// from the current one. Offsets point at each report's modifier byte and also
/// support the protocol's bounded report-ID heuristic.
pub fn collectReleasedUsages(
    previous: []const u8,
    previous_offset: usize,
    current: []const u8,
    current_offset: usize,
    output: []u8,
) usize {
    return r4os_rust_input_hid_released(previous.ptr, previous.len, previous_offset, current.ptr, current.len, current_offset, output.ptr, output.len);
}

test "USB keypad usages round-trip to the same identities as PS/2 Set-1" {
    const usages = [_]u8{ 0x5A, 0x5C, 0x5E, 0x5F, 0x60, 0x61 };
    for (usages) |usage| {
        const code = usageToCode(usage).?;
        try std.testing.expect(!code.extended);
        try std.testing.expectEqual(@as(?u16, usage), physical_key.set1Usage(code.make, code.extended));
    }
}

test "keypad navigation and numeric row keep distinct physical identities" {
    const keypad_8 = usageToCode(0x60).?;
    const arrow_up = usageToCode(0x52).?;
    const row_8 = usageToCode(0x25).?;
    try std.testing.expect(!keypad_8.extended);
    try std.testing.expect(arrow_up.extended);
    try std.testing.expect(!row_8.extended);
    try std.testing.expectEqual(@as(?u16, 0x60), physical_key.set1Usage(keypad_8.make, keypad_8.extended));
    try std.testing.expectEqual(@as(?u16, 0x52), physical_key.set1Usage(arrow_up.make, arrow_up.extended));
    try std.testing.expectEqual(@as(?u16, 0x25), physical_key.set1Usage(row_8.make, row_8.extended));
}

test "USB modifier bridge preserves right control and right alt E0 identity" {
    const left_control = modifierToCode(0x01).?;
    const right_control = modifierToCode(0x10).?;
    const right_alt = modifierToCode(0x40).?;
    try std.testing.expect(!left_control.extended);
    try std.testing.expect(right_control.extended);
    try std.testing.expect(right_alt.extended);
    try std.testing.expectEqual(@as(?u16, 0xE0), physical_key.set1Usage(left_control.make, left_control.extended));
    try std.testing.expectEqual(@as(?u16, 0xE4), physical_key.set1Usage(right_control.make, right_control.extended));
    try std.testing.expectEqual(@as(?u16, 0xE6), physical_key.set1Usage(right_alt.make, right_alt.extended));
}

test "Num Lock and the complete keypad range remain representable" {
    try std.testing.expectEqual(@as(?u16, 0x53), physical_key.set1Usage(0x45, false));
    var usage: u8 = 0x59;
    while (usage <= 0x63) : (usage += 1) {
        const code = usageToCode(usage).?;
        try std.testing.expect(!code.extended);
        try std.testing.expectEqual(@as(?u16, usage), physical_key.set1Usage(code.make, false));
    }
}

test "boot reports expose exact keypad release transitions with or without report ID" {
    const previous = [_]u8{ 0, 0, 0x60, 0x5A, 0, 0, 0, 0 };
    const current = [_]u8{ 0, 0, 0x5A, 0x5E, 0, 0, 0, 0 };
    var released: [6]u8 = undefined;
    const count = collectReleasedUsages(previous[0..], 0, current[0..], 0, released[0..]);
    try std.testing.expectEqual(@as(usize, 1), count);
    try std.testing.expectEqual(@as(u8, 0x60), released[0]);

    const previous_with_id = [_]u8{ 3, 0, 0, 0x5C, 0x61, 0, 0, 0, 0 };
    const current_with_id = [_]u8{ 3, 0, 0, 0x61, 0, 0, 0, 0, 0 };
    const id_count = collectReleasedUsages(previous_with_id[0..], 1, current_with_id[0..], 1, released[0..]);
    try std.testing.expectEqual(@as(usize, 1), id_count);
    try std.testing.expectEqual(@as(u8, 0x5C), released[0]);
}

comptime {
    if (@sizeOf(Code) != 2 or @alignOf(Code) != 1 or @offsetOf(Code, "make") != 0 or @offsetOf(Code, "extended") != 1) @compileError("Native input Code layout");
}
