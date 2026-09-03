const std = @import("std");

pub fn targetMask(online_mask: u64, current_cpu: u32) u64 {
    if (current_cpu >= 64) return online_mask;
    return online_mask & ~(@as(u64, 1) << @intCast(current_cpu));
}

pub fn completed(target_mask: u64, acknowledgement_mask: u64) bool {
    return (acknowledgement_mask & target_mask) == target_mask;
}

pub fn offlineSinceSnapshot(target_mask: u64, online_mask: u64) u64 {
    return target_mask & ~online_mask;
}

test "shootdown excludes issuer and offline CPUs" {
    try std.testing.expectEqual(@as(u64, 0b1010), targetMask(0b1011, 0));
    try std.testing.expectEqual(@as(u64, 0b0011), targetMask(0b1011, 3));
    try std.testing.expectEqual(@as(u64, 0), targetMask(0b0001, 0));
}

test "ack completion ignores unrelated and accepts CPUs that went offline" {
    const targets: u64 = 0b1110;
    try std.testing.expect(!completed(targets, 0b1010));
    try std.testing.expect(completed(targets, 0b1111));
    try std.testing.expectEqual(@as(u64, 0b0100), offlineSinceSnapshot(targets, 0b1011));
    try std.testing.expect(completed(targets, 0b1010 | offlineSinceSnapshot(targets, 0b1011)));
}
