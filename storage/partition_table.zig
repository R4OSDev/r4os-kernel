// Read-only media inventory. No drive letters, system heuristics or writes.
const std = @import("std");
pub const gpt = @import("gpt.zig");
pub const guid = @import("guid.zig");
pub const max_partitions = 128;
pub const bios_boot_guid = guid.parse("21686148-6449-6e6f-744e-656564454649").?;
pub const esp_guid = guid.parse("c12a7328-f81f-11d2-ba4b-00a0c93ec93b").?;
pub const basic_guid = guid.parse("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7").?;

pub const Reader = struct {
    ctx: *anyopaque,
    read: *const fn (*anyopaque, u64, *[512]u8) bool,
    sector_bytes: u32,
    sectors: u64,
    fn sector(self: Reader, lba: u64, out: *[512]u8) !void {
        if (lba >= self.sectors) return error.OutOfRange;
        if (!self.read(self.ctx, lba, out)) return error.ReadFailed;
    }
};

pub const Partition = struct {
    number: u32 = 0,
    first_lba: u64 = 0,
    sector_count: u64 = 0,
    unique_guid: guid.Guid = guid.zero,
    type_guid: guid.Guid = guid.zero,
    mbr_type: u8 = 0,
    attributes: u64 = 0,
    name: [36]u16 = .{0} ** 36,
    pub fn end(self: Partition) u64 {
        return self.first_lba + self.sector_count;
    }
};

pub const Table = struct {
    kind: enum { unknown, mbr, gpt } = .unknown,
    valid: bool = false,
    reason: []const u8 = "not-scanned",
    disk_guid: guid.Guid = guid.zero,
    mbr_disk_id: u32 = 0,
    first_usable: u64 = 0,
    last_usable: u64 = 0,
    partitions: [max_partitions]Partition = .{Partition{}} ** max_partitions,
    count: usize = 0,
    pub fn items(self: *const Table) []const Partition {
        return self.partitions[0..self.count];
    }
};

pub fn scan(reader: Reader, result: *Table) void {
    result.* = .{};
    scanChecked(reader, result) catch |err| {
        result.reason = @errorName(err);
        return;
    };
    result.valid = true;
    result.reason = "ok";
}

fn scanChecked(reader: Reader, result: *Table) !void {
    if (reader.sector_bytes != 512) return error.UnsupportedSectorSize;
    if (reader.sectors == 0 or reader.sectors > std.math.maxInt(u64) / 512) return error.UnsupportedCapacity;
    var mbr: [512]u8 = undefined;
    try reader.sector(0, &mbr);
    if (mbr[510] != 0x55 or mbr[511] != 0xaa) return error.NoPartitionTable;
    result.mbr_disk_id = le32(mbr[440..444]);
    var protective: usize = 0;
    var occupied: usize = 0;
    for (0..4) |i| {
        const entry = mbr[446 + i * 16 ..][0..16];
        if (entry[4] == 0) continue;
        occupied += 1;
        if (entry[4] == 0xee) {
            protective += 1;
            if (le32(entry[8..12]) != 1 or le32(entry[12..16]) != @min(reader.sectors - 1, std.math.maxInt(u32))) return error.BadProtectiveMbr;
        }
    }
    if (protective != 0) {
        result.kind = .gpt;
        if (protective != 1 or occupied != 1) return error.HybridMbr;
        return scanGpt(reader, result);
    }
    result.kind = .mbr;
    result.first_usable = 1;
    result.last_usable = reader.sectors - 1;
    var extended_base: u64 = 0;
    var extended_end: u64 = 0;
    for (0..4) |i| {
        const raw = mbr[446 + i * 16 ..][0..16];
        if (raw[4] == 0) continue;
        const p = Partition{ .number = @intCast(i + 1), .mbr_type = raw[4], .first_lba = le32(raw[8..12]), .sector_count = le32(raw[12..16]) };
        try bounds(p, reader.sectors);
        if (isExtended(p.mbr_type)) {
            if (extended_base != 0) return error.MultipleExtendedPartitions;
            extended_base = p.first_lba;
            extended_end = p.end();
        } else try append(result, p);
    }
    if (extended_base != 0) {
        for (result.items()) |p| if (p.first_lba < extended_end and p.end() > extended_base) return error.PartitionOverlap;
        var ebr_lba = extended_base;
        var seen: [max_partitions]u64 = undefined;
        var seen_count: usize = 0;
        while (true) {
            if (seen_count == seen.len) return error.TooManyPartitions;
            for (seen[0..seen_count]) |old| if (old == ebr_lba) return error.ExtendedCycle;
            for (result.items()) |p| if (ebr_lba >= p.first_lba and ebr_lba < p.end()) return error.ExtendedMetadataOverlap;
            seen[seen_count] = ebr_lba;
            seen_count += 1;
            var ebr: [512]u8 = undefined;
            try reader.sector(ebr_lba, &ebr);
            if (ebr[510] != 0x55 or ebr[511] != 0xaa) return error.BadExtendedSignature;
            const first = ebr[446..462];
            if (first[4] == 0 or isExtended(first[4])) return error.BadLogicalPartition;
            const p = Partition{ .number = @intCast(4 + seen_count), .mbr_type = first[4], .first_lba = ebr_lba + le32(first[8..12]), .sector_count = le32(first[12..16]) };
            try bounds(p, reader.sectors);
            if (p.first_lba <= ebr_lba or p.end() > extended_end) return error.BadLogicalPartition;
            try append(result, p);
            if (ebr[482 + 4] != 0 or ebr[498 + 4] != 0) return error.BadExtendedEntries;
            const next = ebr[462..478];
            if (next[4] == 0) break;
            if (!isExtended(next[4])) return error.BadExtendedLink;
            ebr_lba = extended_base + le32(next[8..12]);
            if (ebr_lba < extended_base or ebr_lba >= extended_end) return error.BadExtendedLink;
        }
    }
}

fn scanGpt(reader: Reader, result: *Table) !void {
    var a: [512]u8 = undefined;
    var b: [512]u8 = undefined;
    try reader.sector(1, &a);
    const primary = try gpt.parseHeader(&a, 1, reader.sectors);
    if (512 % primary.entry_size != 0) return error.UnsupportedEntryLayout;
    result.disk_guid = primary.disk_guid;
    result.first_usable = primary.first_usable_lba;
    result.last_usable = primary.last_usable_lba;
    if (guid.isZero(primary.disk_guid)) return error.MissingDiskGuid;
    try reader.sector(reader.sectors - 1, &b);
    const backup = try gpt.parseHeader(&b, reader.sectors - 1, reader.sectors);
    if (!guid.eql(primary.disk_guid, backup.disk_guid) or primary.first_usable_lba != backup.first_usable_lba or
        primary.last_usable_lba != backup.last_usable_lba or primary.entry_count != backup.entry_count or
        primary.entry_size != backup.entry_size or primary.entries_crc32 != backup.entries_crc32) return error.GptCopiesDiffer;
    var crc = gpt.Crc32{};
    var remaining = primary.entryBytes();
    var index: u32 = 0;
    var sector_index: u64 = 0;
    while (remaining != 0) : (sector_index += 1) {
        try reader.sector(primary.entries_lba + sector_index, &a);
        try reader.sector(backup.entries_lba + sector_index, &b);
        const take: usize = @intCast(@min(remaining, 512));
        if (!std.mem.eql(u8, a[0..take], b[0..take])) return error.GptCopiesDiffer;
        crc.update(a[0..take]);
        var offset: usize = 0;
        while (offset < take) : (offset += primary.entry_size) {
            index += 1;
            const parsed = try gpt.parsePartition(a[offset..][0..primary.entry_size], primary) orelse continue;
            if (guid.isZero(parsed.unique_guid)) return error.MissingPartitionGuid;
            try append(result, .{ .number = index, .first_lba = parsed.first_lba, .sector_count = parsed.sectorCount(), .unique_guid = parsed.unique_guid, .type_guid = parsed.type_guid, .attributes = parsed.attributes, .name = parsed.name_utf16 });
        }
        remaining -= take;
    }
    if (crc.finish() != primary.entries_crc32) return error.BadEntriesCrc;
}

fn append(table: *Table, p: Partition) !void {
    for (table.items()) |old| {
        if (p.first_lba < old.end() and p.end() > old.first_lba) return error.PartitionOverlap;
        if (!guid.isZero(p.unique_guid) and guid.eql(old.unique_guid, p.unique_guid)) return error.DuplicatePartitionGuid;
    }
    if (table.count == table.partitions.len) return error.TooManyPartitions;
    table.partitions[table.count] = p;
    table.count += 1;
}

fn bounds(p: Partition, sectors: u64) !void {
    if (p.first_lba == 0 or p.first_lba >= sectors or p.sector_count == 0 or p.sector_count > sectors - p.first_lba) return error.PartitionOutOfRange;
}
fn isExtended(kind: u8) bool {
    return kind == 5 or kind == 0x0f or kind == 0x85;
}
fn le32(p: *const [4]u8) u32 {
    return std.mem.readInt(u32, p, .little);
}
