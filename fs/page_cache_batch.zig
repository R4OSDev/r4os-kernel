// Private C facade to the single native filesystem policy owner.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const sector_count: usize = 8;
pub const Owner = u64;
pub const no_owner: Owner = 0;
extern fn r4os_rust_fs_batch_mask(u8, *const [sector_count]Owner, Owner) callconv(.c) u8;
extern fn r4os_rust_fs_batch_oldest(u8, *const [sector_count]Owner, *const [sector_count]u64, Owner, *u64) callconv(.c) bool;
extern fn r4os_rust_fs_batch_clear(*[sector_count]Owner, *[sector_count]u64, u8) callconv(.c) void;
pub fn maskForOwner(dirty_mask: u8, owners: *const [sector_count]Owner, owner: Owner) u8 {
    return r4os_rust_fs_batch_mask(dirty_mask, owners, owner);
}
pub fn oldestSequenceForOwner(dirty_mask: u8, owners: *const [sector_count]Owner, sequences: *const [sector_count]u64, owner: Owner) ?u64 {
    var out: u64 = undefined;
    return if (r4os_rust_fs_batch_oldest(dirty_mask, owners, sequences, owner, &out)) out else null;
}
pub fn clearOwnership(owners: *[sector_count]Owner, sequences: *[sector_count]u64, bits: u8) void {
    r4os_rust_fs_batch_clear(owners, sequences, bits);
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
