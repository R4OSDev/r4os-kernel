//! R4XStart metadata facade. Native Rust owns the original admission policy.
pub const MetadataState = enum(u8) { valid, missing, unknown, duplicate, conflicting };
extern fn r4os_rust_program_start_state([*]const u8, usize) u8;
extern fn r4os_rust_program_start_accepts([*]const u8, usize, bool, u32) bool;
pub fn metadataState(meta: []const u8) MetadataState {
    return @enumFromInt(r4os_rust_program_start_state(meta.ptr, meta.len));
}
pub fn accepts(meta: []const u8, has_entry_export_v1: bool, export_count: u32) bool {
    return r4os_rust_program_start_accepts(meta.ptr, meta.len, has_entry_export_v1, export_count);
}

test "current R4X start metadata matrix" {
    const std = @import("std");
    const Case = struct {
        name: []const u8,
        meta: []const u8,
        has_entry_export_v1: bool,
        export_count: u32,
        state: MetadataState,
        accepted: bool,
    };
    const cases = [_]Case{
        .{ .name = "valid", .meta = "r4x.name=OK\x00r4x.start=r4xstart\x00", .has_entry_export_v1 = true, .export_count = 1, .state = .valid, .accepted = true },
        .{ .name = "metadata missing", .meta = "r4x.name=MISSING\x00", .has_entry_export_v1 = true, .export_count = 1, .state = .missing, .accepted = false },
        .{ .name = "unknown value", .meta = "r4x.start=invalid\x00", .has_entry_export_v1 = true, .export_count = 1, .state = .unknown, .accepted = false },
        .{ .name = "duplicate value", .meta = "r4x.start=r4xstart\x00r4x.start=r4xstart\x00", .has_entry_export_v1 = true, .export_count = 1, .state = .duplicate, .accepted = false },
        .{ .name = "export missing", .meta = "r4x.start=r4xstart\x00", .has_entry_export_v1 = false, .export_count = 0, .state = .valid, .accepted = false },
        .{ .name = "conflicting values", .meta = "r4x.start=r4xstart\x00r4x.start=invalid\x00", .has_entry_export_v1 = true, .export_count = 1, .state = .conflicting, .accepted = false },
        .{ .name = "additional normal export", .meta = "r4x.start=r4xstart\x00", .has_entry_export_v1 = true, .export_count = 2, .state = .valid, .accepted = true },
    };

    for (cases) |case| {
        errdefer std.debug.print("R4X start parser case failed: {s}\n", .{case.name});
        try std.testing.expectEqual(case.state, metadataState(case.meta));
        try std.testing.expectEqual(case.accepted, accepts(case.meta, case.has_entry_export_v1, case.export_count));
    }
}
