const std = @import("std");
extern fn r4os_rust_vm_metadata_case(case: u32) u32;

test "VM collision chain rejects retired identities after range slot reuse" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_vm_metadata_case(0));
}
test "VM full range catalogue retains sorted lookup and precise released slot reuse" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_vm_metadata_case(1));
}
test "VM commit merge and reserved split preserve holes and release every metadata slot" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_vm_metadata_case(2));
}
test "VM full commit catalogue rejects a missing split ticket before mutation" {
    try std.testing.expectEqual(@as(u32, 0), r4os_rust_vm_metadata_case(3));
}
