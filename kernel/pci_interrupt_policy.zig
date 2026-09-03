const std = @import("std");

pub const MsixTableGeometry = struct {
    bir: u8,
    offset: u32,
    mapping_bytes: u32,
};

/// Decode the MSI-X table descriptor for the single-vector DriverApi path.
/// PCI only defines BAR indicators 0..5. The complete first 16-byte entry
/// must fit both the kernel MMIO policy and the u32 DriverApi map request.
pub fn msixTableGeometry(descriptor: u32, max_mapping_bytes: u64) ?MsixTableGeometry {
    const bir: u8 = @truncate(descriptor & 0x7);
    if (bir >= 6) return null;

    const offset = descriptor & 0xFFFF_FFF8;
    const required = @as(u64, offset) + 16;
    if (required > max_mapping_bytes or required > std.math.maxInt(u32)) return null;

    return .{
        .bir = bir,
        .offset = offset,
        .mapping_bytes = @intCast(required),
    };
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
