// Versioned BOOT:/boot/r4os-installation.json. Parsing is host-independent;
// disk I/O and drive assignment belong to the boot/mount owner.
const std = @import("std");
const tables = @import("partition_table.zig");
pub const guid = tables.guid;
pub const max_bytes = 16384;
pub const Role = enum { BIOSBOOT, BOOT, SYSTEM, RECOVERY, DATA };

pub const Part = struct {
    partition_guid: guid.Guid = guid.zero,
    type_guid: guid.Guid = guid.zero,
    first_lba: u64 = 0,
    sector_count: u64 = 0,
};
pub const Manifest = struct {
    installation_id: guid.Guid = guid.zero,
    disk_guid: guid.Guid = guid.zero,
    partitions: [5]Part = .{Part{}} ** 5,
    pub fn part(self: *const Manifest, role: Role) *const Part {
        return &self.partitions[@intFromEnum(role)];
    }
};

pub fn parse(allocator: std.mem.Allocator, input: []const u8) !Manifest {
    if (input.len == 0 or input.len > max_bytes) return error.ManifestSize;
    const text = if (std.mem.startsWith(u8, input, "\xef\xbb\xbf")) input[3..] else input;
    const doc = try std.json.parseFromSlice(std.json.Value, allocator, text, .{ .parse_numbers = false, .duplicate_field_behavior = .@"error" });
    defer doc.deinit();
    const root = doc.value;
    if (try uint(try field(root, "schema")) != 1) return error.UnsupportedSchema;
    if (try uint(try field(root, "logicalSectorBytes")) != 512) return error.UnsupportedSectorSize;
    var result = Manifest{
        .installation_id = try uuid(try field(root, "installationId")),
        .disk_guid = try uuid(try field(root, "diskGuid")),
    };
    const parts = try field(root, "partitions");
    if (parts != .object or parts.object.count() != 5) return error.PartitionRoles;
    inline for (std.meta.fields(Role)) |role| {
        const obj = try field(parts, role.name);
        result.partitions[role.value] = .{
            .partition_guid = try uuid(try field(obj, "partitionGuid")),
            .type_guid = try uuid(try field(obj, "typeGuid")),
            .first_lba = try uint(try field(obj, "firstLba")),
            .sector_count = try uint(try field(obj, "sectorCount")),
        };
        const p = result.partitions[role.value];
        if (p.first_lba == 0 or p.sector_count == 0 or p.sector_count > std.math.maxInt(u64) - p.first_lba) return error.PartitionRange;
        const expected = switch (@as(Role, @enumFromInt(role.value))) {
            .BIOSBOOT => tables.bios_boot_guid,
            .BOOT => tables.esp_guid,
            else => tables.basic_guid,
        };
        if (!guid.eql(p.type_guid, expected)) return error.PartitionType;
    }
    for (result.partitions, 0..) |p, i| for (result.partitions[0..i]) |other| {
        if (guid.eql(p.partition_guid, other.partition_guid)) return error.DuplicatePartitionGuid;
        if (p.first_lba < other.first_lba + other.sector_count and other.first_lba < p.first_lba + p.sector_count) return error.PartitionOverlap;
    };
    const files = try field(root, "bootFiles");
    if (files != .array or files.array.items.len == 0 or files.array.items.len > 64) return error.BootFiles;
    for (files.array.items, 0..) |value, i| {
        const path = try string(value);
        if (!validBootPath(path)) return error.BootPath;
        for (files.array.items[0..i]) |other| if (std.ascii.eqlIgnoreCase(path, try string(other))) return error.DuplicateBootPath;
    }
    if (!validVersion(try string(try field(root, "releaseVersion"))) or !validVersion(try string(try field(root, "kernelVersion")))) return error.Version;
    return result;
}

pub fn matches(manifest: *const Manifest, table: *const tables.Table, boot_guid: guid.Guid) bool {
    if (!table.valid or table.kind != .gpt or !guid.eql(manifest.disk_guid, table.disk_guid) or !guid.eql(manifest.part(.BOOT).partition_guid, boot_guid)) return false;
    for (manifest.partitions) |part| {
        var found = false;
        for (table.items()) |p| {
            if (!guid.eql(part.partition_guid, p.unique_guid)) continue;
            if (!guid.eql(part.type_guid, p.type_guid) or part.first_lba != p.first_lba or part.sector_count != p.sector_count) return false;
            found = true;
        }
        if (!found) return false;
    }
    return true;
}

fn field(value: std.json.Value, key: []const u8) !std.json.Value {
    if (value != .object) return error.ExpectedObject;
    return value.object.get(key) orelse error.MissingField;
}
fn string(value: std.json.Value) ![]const u8 {
    if (value != .string) return error.ExpectedString;
    return value.string;
}
fn uuid(value: std.json.Value) !guid.Guid {
    const result = guid.parse(try string(value)) orelse return error.InvalidGuid;
    if (guid.isZero(result)) return error.InvalidGuid;
    return result;
}
fn uint(value: std.json.Value) !u64 {
    if (value != .number_string) return error.ExpectedInteger;
    const text = value.number_string;
    if (text.len == 0) return error.ExpectedInteger;
    for (text) |c| if (c < '0' or c > '9') return error.ExpectedInteger;
    return std.fmt.parseInt(u64, text, 10) catch error.IntegerOverflow;
}
fn validVersion(text: []const u8) bool {
    if (text.len == 0 or text.len > 63) return false;
    var dots: usize = 0;
    var digits: usize = 0;
    for (text) |c| {
        if (c == '.') {
            if (digits == 0) return false;
            dots += 1;
            digits = 0;
        } else if (c >= '0' and c <= '9') {
            digits += 1;
        } else return false;
    }
    return dots == 2 and digits != 0;
}
fn validBootPath(path: []const u8) bool {
    if (path.len == 0 or path.len > 255) return false;
    var components = std.mem.splitScalar(u8, path, '/');
    while (components.next()) |part| {
        if (part.len == 0 or std.mem.eql(u8, part, ".") or std.mem.eql(u8, part, "..")) return false;
        for (part) |c| if (c < 32 or c == ':' or c == '\\') return false;
    }
    return true;
}
