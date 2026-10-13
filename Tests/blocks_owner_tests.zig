const std = @import("std");
extern fn r4os_rust_blocks_owner_case(case: u32) u32;

test "physical metadata rolls back a late split under full catalogue pressure" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_blocks_owner_case(0));
}
test "physical release preflight keeps full catalogue state and admits adjacent edge release" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_blocks_owner_case(1));
}
test "physical merge overflow keeps both original owner and free range" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_blocks_owner_case(2));
}
test "physical transaction ID exhaustion rolls back without freeing the claimed range" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_blocks_owner_case(3));
}
test "physical ID collision chain survives released slot reuse" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_blocks_owner_case(4));
}
