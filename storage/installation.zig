// Stateless manifestation facade. Rust owns all installation rules/matching;
// Zig std JSON is only a temporary generic decoder with the original allocator.
const std = @import("std");
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}
const tables = @import("partition_table.zig");
pub const guid = tables.guid;
pub const max_bytes = 16384;
pub const Role = enum { BIOSBOOT, BOOT, SYSTEM, RECOVERY, DATA };
pub const Part = extern struct { partition_guid: guid.Guid = guid.zero, type_guid: guid.Guid = guid.zero, first_lba: u64 = 0, sector_count: u64 = 0 };
pub const Manifest = extern struct {
    installation_id: guid.Guid = guid.zero,
    disk_guid: guid.Guid = guid.zero,
    partitions: [5]Part = .{Part{}} ** 5,
    pub fn part(self: *const Manifest, role: Role) *const Part {
        return &self.partitions[@intFromEnum(role)];
    }
};
const Label = extern struct { ptr: [*]const u8, len: usize };
const TableView = extern struct { valid: bool, kind: u8, disk_guid: guid.Guid, partitions: [*]const tables.Partition, count: usize };
const JsonMeta = extern struct { found: bool = false, kind: u8 = 0, child: ?*const anyopaque = null, text: [*]const u8 = "", len: usize = 0 };
const JsonReader = *const fn (*const anyopaque, u8, [*]const u8, usize, usize, *JsonMeta) callconv(.c) void;
extern fn r4os_rust_installation_text([*]const u8, usize, *Label) callconv(.c) i32;
extern fn r4os_rust_installation_parse(*const anyopaque, JsonReader, *Manifest) callconv(.c) i32;
extern fn r4os_rust_installation_matches(*const Manifest, *const TableView, *const guid.Guid) callconv(.c) bool;
fn check(status: i32) !void {
    switch (status) {
        0 => {},
        1 => return error.ManifestSize,
        2 => return error.UnsupportedSchema,
        3 => return error.UnsupportedSectorSize,
        4 => return error.ExpectedObject,
        5 => return error.MissingField,
        6 => return error.ExpectedString,
        7 => return error.InvalidGuid,
        8 => return error.ExpectedInteger,
        9 => return error.IntegerOverflow,
        10 => return error.PartitionRoles,
        11 => return error.PartitionRange,
        12 => return error.PartitionType,
        13 => return error.DuplicatePartitionGuid,
        14 => return error.PartitionOverlap,
        15 => return error.BootFiles,
        16 => return error.BootPath,
        17 => return error.DuplicateBootPath,
        18 => return error.Version,
        else => unreachable,
    }
}
// Pure generic DOM projection: no R4OS field names, GUIDs, ranges or policy.
fn describe(node: *const std.json.Value, out: *JsonMeta) void {
    out.* = .{ .found = true, .child = node };
    switch (node.*) {
        .object => |object| {
            out.kind = 1;
            out.len = object.count();
        },
        .array => |array| {
            out.kind = 2;
            out.len = array.items.len;
        },
        .string => |text| {
            out.kind = 3;
            out.text = text.ptr;
            out.len = text.len;
        },
        .number_string => |text| {
            out.kind = 4;
            out.text = text.ptr;
            out.len = text.len;
        },
        else => {},
    }
}
fn readJson(raw: *const anyopaque, operation: u8, key: [*]const u8, key_len: usize, index: usize, out: *JsonMeta) callconv(.c) void {
    const node: *const std.json.Value = @ptrCast(@alignCast(raw));
    out.* = .{};
    switch (operation) {
        0 => describe(node, out),
        1 => if (node.* == .object) {
            if (node.object.getPtr(key[0..key_len])) |child| describe(child, out);
        },
        2 => if (node.* == .array and index < node.array.items.len) {
            describe(&node.array.items[index], out);
        },
        else => unreachable,
    }
}
pub fn parse(allocator: std.mem.Allocator, input: []const u8) !Manifest {
    var text: Label = undefined;
    try check(r4os_rust_installation_text(input.ptr, input.len, &text));
    const doc = try std.json.parseFromSlice(std.json.Value, allocator, text.ptr[0..text.len], .{ .parse_numbers = false, .duplicate_field_behavior = .@"error" });
    defer doc.deinit();
    var out: Manifest = undefined;
    try check(r4os_rust_installation_parse(&doc.value, readJson, &out));
    return out;
}
pub fn matches(manifest: *const Manifest, table: *const tables.Table, boot_guid: guid.Guid) bool {
    const view = TableView{ .valid = table.valid, .kind = @intFromEnum(table.kind), .disk_guid = table.disk_guid, .partitions = &table.partitions, .count = table.count };
    return r4os_rust_installation_matches(manifest, &view, &boot_guid);
}
comptime {
    if (@sizeOf(Part) != 48 or @sizeOf(Manifest) != 272 or @sizeOf(TableView) != 40 or @sizeOf(JsonMeta) != 32) @compileError("Native installation JSON/manifest C layout differs");
}
