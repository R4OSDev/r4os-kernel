const metadata_prefix = "r4x.start=";
const current_metadata = "r4x.start=r4xstart";

pub const MetadataState = enum {
    valid,
    missing,
    unknown,
    duplicate,
    conflicting,
};

pub fn metadataState(meta: []const u8) MetadataState {
    var current_count: usize = 0;
    var unknown_count: usize = 0;
    var cursor: usize = 0;

    while (cursor < meta.len) {
        var end = cursor;
        while (end < meta.len and meta[end] != 0) : (end += 1) {}
        const item = meta[cursor..end];
        if (startsWith(item, metadata_prefix)) {
            if (eql(item, current_metadata)) {
                current_count += 1;
            } else {
                unknown_count += 1;
            }
        }
        cursor = end + 1;
    }

    if (current_count == 0 and unknown_count == 0) return .missing;
    if (current_count == 0) return if (unknown_count == 1) .unknown else .duplicate;
    if (unknown_count != 0) return .conflicting;
    if (current_count != 1) return .duplicate;
    return .valid;
}

pub fn accepts(meta: []const u8, has_entry_export_v1: bool, export_count: u32) bool {
    return metadataState(meta) == .valid and has_entry_export_v1 and export_count >= 1;
}

fn startsWith(value: []const u8, prefix: []const u8) bool {
    return value.len >= prefix.len and eql(value[0..prefix.len], prefix);
}

fn eql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |left, right| {
        if (left != right) return false;
    }
    return true;
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
