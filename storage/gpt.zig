const std = @import("std");

pub const logical_sector_size: usize = 512;
pub const minimum_header_size: u32 = 92;
pub const minimum_entry_size: u32 = 128;
pub const maximum_entry_size: u32 = logical_sector_size;
pub const maximum_entry_count: u32 = 4096;

const signature = "EFI PART";
const revision_1_0: u32 = 0x0001_0000;

const efi_system_guid = [16]u8{
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
    0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
};

const microsoft_basic_data_guid = [16]u8{
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
    0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
};

pub const Header = struct {
    disk_guid: [16]u8 = .{0} ** 16,
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc32: u32,

    pub fn entryBytes(self: Header) u64 {
        return @as(u64, self.entry_count) * @as(u64, self.entry_size);
    }

    pub fn entrySectors(self: Header) u64 {
        return (self.entryBytes() + logical_sector_size - 1) / logical_sector_size;
    }
};

pub const PartitionType = enum {
    efi_system,
    microsoft_basic_data,
    other,
};

pub const Partition = struct {
    type_guid: [16]u8 = .{0} ** 16,
    unique_guid: [16]u8 = .{0} ** 16,
    name_utf16: [36]u16 = .{0} ** 36,
    partition_type: PartitionType,
    first_lba: u64,
    last_lba: u64,
    attributes: u64,

    pub fn sectorCount(self: Partition) u64 {
        return self.last_lba - self.first_lba + 1;
    }

    pub fn isBootCandidate(self: Partition) bool {
        // GPT attribute bit 2 is the legacy-BIOS bootable attribute.  An EFI
        // System Partition is a boot candidate independently of that bit.
        return self.partition_type == .efi_system or (self.attributes & 0x4) != 0;
    }
};

pub const ParseError = error{
    ShortSector,
    BadSignature,
    UnsupportedRevision,
    BadHeaderSize,
    NonzeroReserved,
    BadHeaderCrc,
    WrongHeaderLba,
    BadBackupLba,
    BadUsableRange,
    UnsupportedEntryLayout,
    EntryArrayOutOfRange,
    ShortEntry,
    BadPartitionRange,
};

pub fn parseHeader(sector: []const u8, header_lba: u64, device_sector_count: u64) ParseError!Header {
    if (sector.len < logical_sector_size) return error.ShortSector;
    if (!std.mem.eql(u8, sector[0..signature.len], signature)) return error.BadSignature;
    if (readLe32(sector[8..12]) != revision_1_0) return error.UnsupportedRevision;

    const header_size = readLe32(sector[12..16]);
    if (header_size < minimum_header_size or header_size > logical_sector_size) return error.BadHeaderSize;
    if (readLe32(sector[20..24]) != 0) return error.NonzeroReserved;

    var crc = Crc32{};
    crc.update(sector[0..16]);
    crc.update(&.{ 0, 0, 0, 0 });
    crc.update(sector[20..header_size]);
    if (crc.finish() != readLe32(sector[16..20])) return error.BadHeaderCrc;

    const current_lba = readLe64(sector[24..32]);
    const backup_lba = readLe64(sector[32..40]);
    const first_usable_lba = readLe64(sector[40..48]);
    const last_usable_lba = readLe64(sector[48..56]);
    const entries_lba = readLe64(sector[72..80]);
    const entry_count = readLe32(sector[80..84]);
    const entry_size = readLe32(sector[84..88]);

    if (device_sector_count < 3 or current_lba != header_lba) return error.WrongHeaderLba;
    if (backup_lba >= device_sector_count or backup_lba == current_lba) return error.BadBackupLba;
    if (first_usable_lba > last_usable_lba or last_usable_lba >= device_sector_count) return error.BadUsableRange;
    if (entry_count == 0 or entry_count > maximum_entry_count or
        entry_size < minimum_entry_size or entry_size > maximum_entry_size or
        entry_size % minimum_entry_size != 0)
    {
        return error.UnsupportedEntryLayout;
    }

    const header = Header{
        .disk_guid = sector[56..72].*,
        .current_lba = current_lba,
        .backup_lba = backup_lba,
        .first_usable_lba = first_usable_lba,
        .last_usable_lba = last_usable_lba,
        .entries_lba = entries_lba,
        .entry_count = entry_count,
        .entry_size = entry_size,
        .entries_crc32 = readLe32(sector[88..92]),
    };
    const entry_sectors = header.entrySectors();
    if (entry_sectors == 0 or entries_lba >= device_sector_count or
        entry_sectors > device_sector_count - entries_lba)
    {
        return error.EntryArrayOutOfRange;
    }
    const entries_end = entries_lba + entry_sectors;

    // The primary array lives between its header and the first usable LBA;
    // the backup array lives after the last usable LBA and before its header.
    // Accepting neither overlap prevents a valid CRC from legitimising GPT
    // metadata that aliases a filesystem partition.
    if (header_lba == 1) {
        if (backup_lba != device_sector_count - 1 or entries_lba <= header_lba or entries_end > first_usable_lba) {
            return error.EntryArrayOutOfRange;
        }
    } else if (header_lba == device_sector_count - 1) {
        if (backup_lba != 1 or entries_lba <= last_usable_lba or entries_end > header_lba) {
            return error.EntryArrayOutOfRange;
        }
    } else {
        return error.WrongHeaderLba;
    }

    return header;
}

pub fn parsePartition(raw: []const u8, header: Header) ParseError!?Partition {
    if (raw.len < header.entry_size or raw.len < minimum_entry_size) return error.ShortEntry;
    const type_guid = raw[0..16];
    if (allZero(type_guid)) return null;

    const first_lba = readLe64(raw[32..40]);
    const last_lba = readLe64(raw[40..48]);
    if (first_lba < header.first_usable_lba or first_lba > last_lba or last_lba > header.last_usable_lba) {
        return error.BadPartitionRange;
    }

    var name: [36]u16 = undefined;
    for (&name, 0..) |*unit, i| unit.* = std.mem.readInt(u16, raw[56 + i * 2 ..][0..2], .little);
    return .{
        .type_guid = raw[0..16].*,
        .unique_guid = raw[16..32].*,
        .name_utf16 = name,
        .partition_type = classifyType(type_guid),
        .first_lba = first_lba,
        .last_lba = last_lba,
        .attributes = readLe64(raw[48..56]),
    };
}

pub fn errorLabel(err: ParseError) []const u8 {
    return switch (err) {
        error.ShortSector => "short-sector",
        error.BadSignature => "bad-signature",
        error.UnsupportedRevision => "unsupported-revision",
        error.BadHeaderSize => "bad-header-size",
        error.NonzeroReserved => "reserved-not-zero",
        error.BadHeaderCrc => "header-crc",
        error.WrongHeaderLba => "header-lba",
        error.BadBackupLba => "backup-lba",
        error.BadUsableRange => "usable-range",
        error.UnsupportedEntryLayout => "entry-layout",
        error.EntryArrayOutOfRange => "entry-array-range",
        error.ShortEntry => "short-entry",
        error.BadPartitionRange => "partition-range",
    };
}

pub fn partitionTypeLabel(partition_type: PartitionType) []const u8 {
    return switch (partition_type) {
        .efi_system => "EFI-system",
        .microsoft_basic_data => "Microsoft-basic-data",
        .other => "unsupported-guid",
    };
}

pub const Crc32 = struct {
    state: u32 = 0xFFFF_FFFF,

    pub fn update(self: *Crc32, bytes: []const u8) void {
        for (bytes) |byte| {
            self.state ^= byte;
            var bit: u4 = 0;
            while (bit < 8) : (bit += 1) {
                self.state = (self.state >> 1) ^ (0xEDB8_8320 & (0 -% (self.state & 1)));
            }
        }
    }

    pub fn finish(self: Crc32) u32 {
        return ~self.state;
    }
};

fn classifyType(type_guid: []const u8) PartitionType {
    if (std.mem.eql(u8, type_guid, efi_system_guid[0..])) return .efi_system;
    if (std.mem.eql(u8, type_guid, microsoft_basic_data_guid[0..])) return .microsoft_basic_data;
    return .other;
}

fn allZero(bytes: []const u8) bool {
    for (bytes) |byte| if (byte != 0) return false;
    return true;
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

fn readLe64(bytes: []const u8) u64 {
    return @as(u64, readLe32(bytes[0..4])) |
        (@as(u64, readLe32(bytes[4..8])) << 32);
}

fn writeLe32(bytes: []u8, value: u32) void {
    bytes[0] = @truncate(value);
    bytes[1] = @truncate(value >> 8);
    bytes[2] = @truncate(value >> 16);
    bytes[3] = @truncate(value >> 24);
}

fn writeLe64(bytes: []u8, value: u64) void {
    writeLe32(bytes[0..4], @truncate(value));
    writeLe32(bytes[4..8], @truncate(value >> 32));
}

fn makeHeader(sector: *[logical_sector_size]u8, current_lba: u64, backup_lba: u64, entries_lba: u64) void {
    sector.* = .{0} ** logical_sector_size;
    @memcpy(sector[0..8], signature);
    writeLe32(sector[8..12], revision_1_0);
    writeLe32(sector[12..16], minimum_header_size);
    writeLe64(sector[24..32], current_lba);
    writeLe64(sector[32..40], backup_lba);
    writeLe64(sector[40..48], 34);
    writeLe64(sector[48..56], 4062);
    sector[56] = 1;
    writeLe64(sector[72..80], entries_lba);
    writeLe32(sector[80..84], 128);
    writeLe32(sector[84..88], minimum_entry_size);
    writeLe32(sector[88..92], 0x1234_5678);
    var crc = Crc32{};
    crc.update(sector[0..minimum_header_size]);
    writeLe32(sector[16..20], crc.finish());
}

test "GPT CRC32 matches the standard vector and supports streaming" {
    var crc = Crc32{};
    crc.update("1234");
    crc.update("56789");
    try std.testing.expectEqual(@as(u32, 0xCBF4_3926), crc.finish());
}

test "GPT primary and backup headers validate exact geometry and CRC" {
    var primary: [logical_sector_size]u8 = undefined;
    makeHeader(&primary, 1, 4095, 2);
    const parsed_primary = try parseHeader(primary[0..], 1, 4096);
    try std.testing.expectEqual(@as(u64, 16_384), parsed_primary.entryBytes());
    try std.testing.expectEqual(@as(u64, 32), parsed_primary.entrySectors());

    var backup: [logical_sector_size]u8 = undefined;
    makeHeader(&backup, 4095, 1, 4063);
    const parsed_backup = try parseHeader(backup[0..], 4095, 4096);
    try std.testing.expectEqual(@as(u64, 4063), parsed_backup.entries_lba);
}

test "GPT header rejects corruption and filesystem overlap" {
    var sector: [logical_sector_size]u8 = undefined;
    makeHeader(&sector, 1, 4095, 2);
    sector[60] ^= 0x80;
    try std.testing.expectError(error.BadHeaderCrc, parseHeader(sector[0..], 1, 4096));

    makeHeader(&sector, 1, 4095, 34);
    try std.testing.expectError(error.EntryArrayOutOfRange, parseHeader(sector[0..], 1, 4096));
}

test "GPT entries classify ESP and basic data and reject invalid ranges" {
    var sector: [logical_sector_size]u8 = undefined;
    makeHeader(&sector, 1, 4095, 2);
    const header = try parseHeader(sector[0..], 1, 4096);

    var entry: [minimum_entry_size]u8 = .{0} ** minimum_entry_size;
    @memcpy(entry[0..16], efi_system_guid[0..]);
    entry[16] = 1;
    writeLe64(entry[32..40], 40);
    writeLe64(entry[40..48], 100);
    const esp = (try parsePartition(entry[0..], header)).?;
    try std.testing.expectEqual(PartitionType.efi_system, esp.partition_type);
    try std.testing.expect(esp.isBootCandidate());
    try std.testing.expectEqual(@as(u64, 61), esp.sectorCount());

    @memcpy(entry[0..16], microsoft_basic_data_guid[0..]);
    const basic = (try parsePartition(entry[0..], header)).?;
    try std.testing.expectEqual(PartitionType.microsoft_basic_data, basic.partition_type);
    try std.testing.expect(!basic.isBootCandidate());

    writeLe64(entry[40..48], 5000);
    try std.testing.expectError(error.BadPartitionRange, parsePartition(entry[0..], header));
}
