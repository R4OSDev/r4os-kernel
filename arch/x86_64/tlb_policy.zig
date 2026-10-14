const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_tlb_mask(operation: u8, first: u64, second: u64) u64;

pub fn targetMask(online_mask: u64, current_cpu: u32) u64 {
    return r4os_rust_tlb_mask(0, online_mask, current_cpu);
}

pub fn completed(target_mask: u64, acknowledgement_mask: u64) bool {
    return r4os_rust_tlb_mask(1, target_mask, acknowledgement_mask) != 0;
}

pub fn offlineSinceSnapshot(target_mask: u64, online_mask: u64) u64 {
    return r4os_rust_tlb_mask(2, target_mask, online_mask);
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
