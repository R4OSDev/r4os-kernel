const std = @import("std");

pub const sector_count: usize = 8;
pub const Owner = u64;
pub const no_owner: Owner = 0;

pub fn maskForOwner(dirty_mask: u8, owners: *const [sector_count]Owner, owner: Owner) u8 {
    var mask: u8 = 0;
    var sector: usize = 0;
    while (sector < sector_count) : (sector += 1) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((dirty_mask & bit) != 0 and owners[sector] == owner) mask |= bit;
    }
    return mask;
}

pub fn oldestSequenceForOwner(
    dirty_mask: u8,
    owners: *const [sector_count]Owner,
    sequences: *const [sector_count]u64,
    owner: Owner,
) ?u64 {
    var oldest: ?u64 = null;
    var sector: usize = 0;
    while (sector < sector_count) : (sector += 1) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((dirty_mask & bit) == 0 or owners[sector] != owner) continue;
        const sequence = sequences[sector];
        if (oldest == null or sequence < oldest.?) oldest = sequence;
    }
    return oldest;
}

pub fn clearOwnership(
    owners: *[sector_count]Owner,
    sequences: *[sector_count]u64,
    bits: u8,
) void {
    var sector: usize = 0;
    while (sector < sector_count) : (sector += 1) {
        const bit = @as(u8, 1) << @as(u3, @intCast(sector));
        if ((bits & bit) == 0) continue;
        owners[sector] = no_owner;
        sequences[sector] = 0;
    }
}

test "selective mask excludes foreign sectors on the same cache page" {
    const owners = [sector_count]Owner{ 11, 22, 11, 22, 0, 11, 22, 11 };
    try std.testing.expectEqual(@as(u8, 0b0010_0101), maskForOwner(0b0110_1111, &owners, 11));
    try std.testing.expectEqual(@as(u8, 0b0100_1010), maskForOwner(0b0110_1111, &owners, 22));
}

test "selective ordering uses only the requested owner's writes" {
    const owners = [sector_count]Owner{ 7, 9, 7, 9, 0, 7, 9, 7 };
    const sequences = [sector_count]u64{ 40, 1, 30, 2, 0, 20, 3, 10 };
    try std.testing.expectEqual(@as(?u64, 10), oldestSequenceForOwner(0xFF, &owners, &sequences, 7));
    try std.testing.expectEqual(@as(?u64, 1), oldestSequenceForOwner(0xFF, &owners, &sequences, 9));
    try std.testing.expectEqual(@as(?u64, null), oldestSequenceForOwner(0xFF, &owners, &sequences, 33));
}

test "clearing one commit preserves foreign ownership" {
    var owners = [sector_count]Owner{ 7, 9, 7, 9, 0, 7, 9, 7 };
    var sequences = [sector_count]u64{ 40, 1, 30, 2, 0, 20, 3, 10 };
    clearOwnership(&owners, &sequences, 0b0010_0101);
    try std.testing.expectEqual([sector_count]Owner{ 0, 9, 0, 9, 0, 0, 9, 7 }, owners);
    try std.testing.expectEqual([sector_count]u64{ 0, 1, 0, 2, 0, 0, 3, 10 }, sequences);
}
