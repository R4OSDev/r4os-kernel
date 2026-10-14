const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const MsixTableGeometry = struct { bir: u8, offset: u32, mapping_bytes: u32 };
const RawGeometry = extern struct { bir: u8, offset: u32, mapping_bytes: u32 };
extern fn r4os_rust_driver_msix_geometry(descriptor: u32, max_mapping_bytes: u64, output: *RawGeometry) bool;
pub fn msixTableGeometry(descriptor: u32, max_mapping_bytes: u64) ?MsixTableGeometry {
    var value: RawGeometry = undefined;
    if (!r4os_rust_driver_msix_geometry(descriptor, max_mapping_bytes, &value)) return null;
    return .{ .bir = value.bir, .offset = value.offset, .mapping_bytes = value.mapping_bytes };
}

test "MSI-X table geometry retains BIR and aligned offset" {
    const geometry = msixTableGeometry(0x0000_2003, 16 * 1024 * 1024) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(u8, 3), geometry.bir);
    try std.testing.expectEqual(@as(u32, 0x2000), geometry.offset);
    try std.testing.expectEqual(@as(u32, 0x2010), geometry.mapping_bytes);
}

test "MSI-X table geometry rejects reserved BARs and excessive mappings" {
    try std.testing.expect(msixTableGeometry(0x0000_2006, 16 * 1024 * 1024) == null);
    try std.testing.expect(msixTableGeometry(0x0200_0000, 16 * 1024 * 1024) == null);
    try std.testing.expect(msixTableGeometry(0xFFFF_FFF8, std.math.maxInt(u64)) == null);
}
