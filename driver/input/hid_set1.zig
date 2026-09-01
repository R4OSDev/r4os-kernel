const std = @import("std");
const physical_key = @import("physical_key.zig");

/// One IBM PC/AT Set-1 make code. `extended` emits the E0 prefix for both the
/// make and break transition. Keeping this bridge pure makes USB and PS/2
/// keyboard identity directly comparable in host tests.
pub const Code = struct {
    make: u8,
    extended: bool = false,
};

pub fn usageToCode(usage: u8) ?Code {
    return switch (usage) {
        0x04 => .{ .make = 0x1E }, // A
        0x05 => .{ .make = 0x30 },
        0x06 => .{ .make = 0x2E },
        0x07 => .{ .make = 0x20 },
        0x08 => .{ .make = 0x12 },
        0x09 => .{ .make = 0x21 },
        0x0A => .{ .make = 0x22 },
        0x0B => .{ .make = 0x23 },
        0x0C => .{ .make = 0x17 },
        0x0D => .{ .make = 0x24 },
        0x0E => .{ .make = 0x25 },
        0x0F => .{ .make = 0x26 },
        0x10 => .{ .make = 0x32 },
        0x11 => .{ .make = 0x31 },
        0x12 => .{ .make = 0x18 },
        0x13 => .{ .make = 0x19 },
        0x14 => .{ .make = 0x10 },
        0x15 => .{ .make = 0x13 },
        0x16 => .{ .make = 0x1F },
        0x17 => .{ .make = 0x14 },
        0x18 => .{ .make = 0x16 },
        0x19 => .{ .make = 0x2F },
        0x1A => .{ .make = 0x11 },
        0x1B => .{ .make = 0x2D },
        0x1C => .{ .make = 0x15 },
        0x1D => .{ .make = 0x2C }, // Z
        0x1E => .{ .make = 0x02 }, // 1
        0x1F => .{ .make = 0x03 },
        0x20 => .{ .make = 0x04 },
        0x21 => .{ .make = 0x05 },
        0x22 => .{ .make = 0x06 },
        0x23 => .{ .make = 0x07 },
        0x24 => .{ .make = 0x08 },
        0x25 => .{ .make = 0x09 },
        0x26 => .{ .make = 0x0A },
        0x27 => .{ .make = 0x0B }, // 0
        0x28 => .{ .make = 0x1C }, // Enter
        0x29 => .{ .make = 0x01 },
        0x2A => .{ .make = 0x0E },
        0x2B => .{ .make = 0x0F },
        0x2C => .{ .make = 0x39 },
        0x2D => .{ .make = 0x0C },
        0x2E => .{ .make = 0x0D },
        0x2F => .{ .make = 0x1A },
        0x30 => .{ .make = 0x1B },
        0x31 => .{ .make = 0x2B },
        0x33 => .{ .make = 0x27 },
        0x34 => .{ .make = 0x28 },
        0x35 => .{ .make = 0x29 },
        0x36 => .{ .make = 0x33 },
        0x37 => .{ .make = 0x34 },
        0x38 => .{ .make = 0x35 },
        0x3C => .{ .make = 0x3D }, // F3
        0x3D => .{ .make = 0x3E }, // F4

        0x4A => .{ .make = 0x47, .extended = true }, // Home
        0x4B => .{ .make = 0x49, .extended = true }, // Page Up
        0x4C => .{ .make = 0x53, .extended = true }, // Delete
        0x4D => .{ .make = 0x4F, .extended = true }, // End
        0x4E => .{ .make = 0x51, .extended = true }, // Page Down
        0x4F => .{ .make = 0x4D, .extended = true }, // Right
        0x50 => .{ .make = 0x4B, .extended = true }, // Left
        0x51 => .{ .make = 0x50, .extended = true }, // Down
        0x52 => .{ .make = 0x48, .extended = true }, // Up

        0x59 => .{ .make = 0x4F }, // Keypad 1
        0x5A => .{ .make = 0x50 }, // Keypad 2
        0x5B => .{ .make = 0x51 }, // Keypad 3
        0x5C => .{ .make = 0x4B }, // Keypad 4
        0x5D => .{ .make = 0x4C }, // Keypad 5
        0x5E => .{ .make = 0x4D }, // Keypad 6
        0x5F => .{ .make = 0x47 }, // Keypad 7
        0x60 => .{ .make = 0x48 }, // Keypad 8
        0x61 => .{ .make = 0x49 }, // Keypad 9
        0x62 => .{ .make = 0x52 }, // Keypad 0
        0x63 => .{ .make = 0x53 }, // Keypad decimal
        else => null,
    };
}

pub fn modifierToCode(bit: u8) ?Code {
    return switch (bit) {
        0x01 => .{ .make = 0x1D }, // left control
        0x02 => .{ .make = 0x2A }, // left shift
        0x04 => .{ .make = 0x38 }, // left alt
        0x08 => .{ .make = 0x5B, .extended = true }, // left GUI
        0x10 => .{ .make = 0x1D, .extended = true }, // right control
        0x20 => .{ .make = 0x36 }, // right shift
        0x40 => .{ .make = 0x38, .extended = true }, // right alt
        0x80 => .{ .make = 0x5C, .extended = true }, // right GUI
        else => null,
    };
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
    var count: usize = 0;
    var index: usize = @min(previous_offset + 2, previous.len);
    while (index < previous.len and count < output.len) : (index += 1) {
        const usage = previous[index];
        if (usage <= 1 or reportContains(current, current_offset, usage)) continue;
        output[count] = usage;
        count += 1;
    }
    return count;
}

fn reportContains(report: []const u8, offset: usize, usage: u8) bool {
    var index: usize = @min(offset + 2, report.len);
    while (index < report.len) : (index += 1) {
        if (report[index] == usage) return true;
    }
    return false;
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
