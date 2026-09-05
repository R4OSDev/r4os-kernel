const std = @import("std");
const tables = @import("storage/partition_table.zig");
const installation = @import("storage/installation.zig");
const source = @import("storage/boot_source.zig");
const guid = tables.guid;
const a = std.testing.allocator;

const Fixture = struct {
    const sectors: u64 = 4194304;
    bytes: [67][512]u8 = .{.{0} ** 512} ** 67,
    fn init() Fixture {
        var f = Fixture{};
        f.bytes[0][510] = 0x55;
        f.bytes[0][511] = 0xaa;
        f.bytes[0][450] = 0xee;
        put32(f.bytes[0][454..458], 1);
        put32(f.bytes[0][458..462], sectors - 1);
        const starts = [_]u64{ 2048, 4096, 266240, 2363392, 3411968 };
        const lengths = [_]u64{ 2048, 262144, 2097152, 1048576, sectors - 34 - 3411968 + 1 };
        for (starts, lengths, 0..) |start, len, i| {
            const p = f.bytes[2 + i / 4][(i % 4) * 128 ..][0..128];
            const kind = if (i == 0) tables.bios_boot_guid else if (i == 1) tables.esp_guid else tables.basic_guid;
            @memcpy(p[0..16], &kind);
            p[16] = @intCast(40 + i);
            put64(p[32..40], start);
            put64(p[40..48], start + len - 1);
        }
        f.sync();
        return f;
    }
    fn sync(f: *Fixture) void {
        for (0..32) |i| f.bytes[34 + i] = f.bytes[2 + i];
        var crc = std.hash.Crc32.init();
        for (f.bytes[2..34]) |sector| crc.update(&sector);
        for ([_]usize{ 1, 66 }) |index| {
            const h = &f.bytes[index];
            h.* = .{0} ** 512;
            @memcpy(h[0..8], "EFI PART");
            put32(h[8..12], 0x10000);
            put32(h[12..16], 92);
            put64(h[24..32], if (index == 1) 1 else sectors - 1);
            put64(h[32..40], if (index == 1) sectors - 1 else 1);
            put64(h[40..48], 34);
            put64(h[48..56], sectors - 34);
            h[56] = 12;
            put64(h[72..80], if (index == 1) 2 else sectors - 33);
            put32(h[80..84], 128);
            put32(h[84..88], 128);
            put32(h[88..92], crc.final());
            put32(h[16..20], std.hash.Crc32.hash(h[0..92]));
        }
    }
    fn reader(f: *Fixture) tables.Reader {
        return .{ .ctx = f, .read = read, .sector_bytes = 512, .sectors = sectors };
    }
    fn read(ctx: *anyopaque, lba: u64, out: *[512]u8) bool {
        const f: *const Fixture = @ptrCast(@alignCast(ctx));
        const index = if (lba < 34) lba else if (lba >= sectors - 33 and lba < sectors) 34 + lba - (sectors - 33) else return false;
        out.* = f.bytes[@intCast(index)];
        return true;
    }
};
fn put32(out: *[4]u8, value: u32) void {
    std.mem.writeInt(u32, out, value, .little);
}
fn put64(out: *[8]u8, value: u64) void {
    std.mem.writeInt(u64, out, value, .little);
}

fn manifestJson(t: *const tables.Table) ![]u8 {
    const P = struct { partitionGuid: []const u8, typeGuid: []const u8, firstLba: u64, sectorCount: u64 };
    var ids: [5][36]u8 = undefined;
    var types: [5][36]u8 = undefined;
    var parts: [5]P = undefined;
    for (t.items(), 0..) |p, i| {
        ids[i] = guid.format(p.unique_guid);
        types[i] = guid.format(p.type_guid);
        parts[i] = .{ .partitionGuid = &ids[i], .typeGuid = &types[i], .firstLba = p.first_lba, .sectorCount = p.sector_count };
    }
    const disk = guid.format(t.disk_guid);
    return std.json.Stringify.valueAlloc(a, .{ .schema = @as(u32, 1), .installationId = "11111111-2222-3333-4444-555555555555", .diskGuid = @as([]const u8, &disk), .logicalSectorBytes = @as(u32, 512), .partitions = .{ .BIOSBOOT = parts[0], .BOOT = parts[1], .SYSTEM = parts[2], .RECOVERY = parts[3], .DATA = parts[4] }, .bootFiles = [_][]const u8{ "boot/kernel.elf", "boot/PRELOAD.R4I" }, .releaseVersion = "0.76.4", .kernelVersion = "0.1.87" }, .{});
}

test "two GPT copies retain GUIDs, all five roles and real media geometry" {
    var f = Fixture.init();
    var t = tables.Table{};
    tables.scan(f.reader(), &t);
    try std.testing.expect(t.valid);
    try std.testing.expectEqual(@as(usize, 5), t.count);
    const json = try manifestJson(&t);
    defer a.free(json);
    const m = try installation.parse(a, json);
    try std.testing.expect(installation.matches(&m, &t, t.partitions[1].unique_guid));
    t.partitions[2].sector_count -= 1;
    try std.testing.expect(!installation.matches(&m, &t, t.partitions[1].unique_guid));
}

test "valid CRC cannot authorize overlapping or duplicate GPT identities" {
    var f = Fixture.init();
    var t = tables.Table{};
    f.bytes[2][128 + 16] = f.bytes[2][16];
    f.sync();
    tables.scan(f.reader(), &t);
    try std.testing.expect(!t.valid);
    try std.testing.expectEqualStrings("DuplicatePartitionGuid", t.reason);
    f = Fixture.init();
    put64(f.bytes[2][128 + 32 ..][0..8], 2048);
    f.sync();
    tables.scan(f.reader(), &t);
    try std.testing.expect(!t.valid);
    try std.testing.expectEqualStrings("PartitionOverlap", t.reason);
    f = Fixture.init();
    f.bytes[34][60] ^= 1;
    tables.scan(f.reader(), &t);
    try std.testing.expect(!t.valid);
    try std.testing.expectEqualStrings("GptCopiesDiffer", t.reason);
    f = Fixture.init();
    f.bytes[66][16] ^= 1;
    tables.scan(f.reader(), &t);
    try std.testing.expect(!t.valid);
    try std.testing.expectEqualStrings("BadHeaderCrc", t.reason);
    var reader = f.reader();
    reader.sector_bytes = 4096;
    tables.scan(reader, &t);
    try std.testing.expect(!t.valid);
    try std.testing.expectEqualStrings("UnsupportedSectorSize", t.reason);
}

test "manifest rejects duplicate keys, fractional geometry, unknown schema and traversal" {
    var f = Fixture.init();
    var t = tables.Table{};
    tables.scan(f.reader(), &t);
    const json = try manifestJson(&t);
    defer a.free(json);
    const cases = [_][2][]const u8{
        .{ "\"schema\":1", "\"schema\":1,\"schema\":1" },                  .{ "\"schema\":1", "\"schema\":2" },
        .{ "\"logicalSectorBytes\":512", "\"logicalSectorBytes\":512.0" }, .{ "boot/kernel.elf", "../kernel.elf" },
        .{ "\"firstLba\":2048", "\"firstLba\":18446744073709551616" },     .{ "boot/PRELOAD.R4I", "BOOT/KERNEL.ELF" },
    };
    for (cases) |pair| {
        const changed = try std.mem.replaceOwned(u8, a, json, pair[0], pair[1]);
        defer a.free(changed);
        if (installation.parse(a, changed)) |_| return error.InvalidManifestAccepted else |_| {}
    }
}

test "actual loaded partition selects CURRENT or PREVIOUS and protects only own USB install" {
    var f = Fixture.init();
    var t = tables.Table{};
    tables.scan(f.reader(), &t);
    const json = try manifestJson(&t);
    defer a.free(json);
    const m = try installation.parse(a, json);
    var devices = [_]source.DeviceView{.{ .index = 7, .usb = true, .local = false, .table = &t, .installation = &m }};
    var identity = source.Identity{ .present = true, .generic_media = true, .path = "/CURRENT/recovery.elf", .disk_guid = t.disk_guid, .partition_guid = t.partitions[3].unique_guid };
    var result = source.resolve(identity, &devices);
    try std.testing.expect(result.confirmed and result.usb and !result.permitsInstall(7) and result.permitsInstall(3) and result.permitsRecoveryUpdate(7));
    devices[0].usb = false;
    devices[0].local = true;
    identity.path = "/PREVIOUS/recovery.elf";
    result = source.resolve(identity, &devices);
    try std.testing.expect(result.confirmed and result.slot == .previous and result.permitsInstall(7) and !result.exposesUsb());
    identity.partition_guid = t.partitions[2].unique_guid;
    try std.testing.expect(!source.resolve(identity, &devices).confirmed);
    identity.partition_guid = t.partitions[3].unique_guid;
    var duplicate = devices[0];
    duplicate.index = 8;
    result = source.resolve(identity, &.{ devices[0], duplicate });
    try std.testing.expect(!result.confirmed and !result.permitsInstall(7));
}

test "foreign MBR partitions remain enumerable without a mounted filesystem" {
    var f = Fixture.init();
    f.bytes[0][450] = 7;
    put32(f.bytes[0][454..458], 2048);
    put32(f.bytes[0][458..462], 10000);
    var t = tables.Table{};
    tables.scan(f.reader(), &t);
    try std.testing.expect(t.valid and t.kind == .mbr and t.count == 1 and t.partitions[0].mbr_type == 7);
    put32(f.bytes[0][458..462], std.math.maxInt(u32));
    tables.scan(f.reader(), &t);
    try std.testing.expect(!t.valid);
}
