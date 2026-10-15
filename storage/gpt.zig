const std = @import("std");
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}

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

// Same parser and error contract; only private fixed C records cross the facade.
pub const Header = extern struct {
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
        return r4os_rust_gpt_header_value(&self, false);
    }
    pub fn entrySectors(self: Header) u64 {
        return r4os_rust_gpt_header_value(&self, true);
    }
};
pub const PartitionType = enum(u8) { efi_system, microsoft_basic_data, other };
pub const Partition = extern struct {
    type_guid: [16]u8 = .{0} ** 16,
    unique_guid: [16]u8 = .{0} ** 16,
    name_utf16: [36]u16 = .{0} ** 36,
    partition_type: PartitionType,
    first_lba: u64,
    last_lba: u64,
    attributes: u64,
    pub fn sectorCount(self: Partition) u64 {
        return r4os_rust_gpt_partition_value(&self, false);
    }
    pub fn isBootCandidate(self: Partition) bool {
        return r4os_rust_gpt_partition_value(&self, true) != 0;
    }
};
pub const ParseError = error{ ShortSector, BadSignature, UnsupportedRevision, BadHeaderSize, NonzeroReserved, BadHeaderCrc, WrongHeaderLba, BadBackupLba, BadUsableRange, UnsupportedEntryLayout, EntryArrayOutOfRange, ShortEntry, BadPartitionRange };
fn check(status: i32) ParseError!void {
    switch (status) {
        0 => {},
        1 => return error.ShortSector,
        2 => return error.BadSignature,
        3 => return error.UnsupportedRevision,
        4 => return error.BadHeaderSize,
        5 => return error.NonzeroReserved,
        6 => return error.BadHeaderCrc,
        7 => return error.WrongHeaderLba,
        8 => return error.BadBackupLba,
        9 => return error.BadUsableRange,
        10 => return error.UnsupportedEntryLayout,
        11 => return error.EntryArrayOutOfRange,
        12 => return error.ShortEntry,
        13 => return error.BadPartitionRange,
        else => unreachable,
    }
}
fn code(err: ParseError) u8 {
    return switch (err) {
        error.ShortSector => 1,
        error.BadSignature => 2,
        error.UnsupportedRevision => 3,
        error.BadHeaderSize => 4,
        error.NonzeroReserved => 5,
        error.BadHeaderCrc => 6,
        error.WrongHeaderLba => 7,
        error.BadBackupLba => 8,
        error.BadUsableRange => 9,
        error.UnsupportedEntryLayout => 10,
        error.EntryArrayOutOfRange => 11,
        error.ShortEntry => 12,
        error.BadPartitionRange => 13,
    };
}

const Label = extern struct { ptr: [*]const u8, len: usize };
extern fn r4os_rust_gpt_header([*]const u8, usize, u64, u64, *Header) callconv(.c) i32;
extern fn r4os_rust_gpt_partition([*]const u8, usize, *const Header, *Partition) callconv(.c) i32;
extern fn r4os_rust_gpt_header_value(*const Header, bool) callconv(.c) u64;
extern fn r4os_rust_gpt_partition_value(*const Partition, bool) callconv(.c) u64;
extern fn r4os_rust_gpt_label(bool, u8) callconv(.c) Label;
extern fn r4os_rust_gpt_crc(*u32, [*]const u8, usize) callconv(.c) void;
extern fn r4os_rust_gpt_crc_finish(u32) callconv(.c) u32;
pub fn parseHeader(sector: []const u8, header_lba: u64, device_sector_count: u64) ParseError!Header {
    var out: Header = undefined;
    try check(r4os_rust_gpt_header(sector.ptr, sector.len, header_lba, device_sector_count, &out));
    return out;
}
pub fn parsePartition(raw: []const u8, header: Header) ParseError!?Partition {
    var out: Partition = undefined;
    const status = r4os_rust_gpt_partition(raw.ptr, raw.len, &header, &out);
    if (status == -1) return null;
    try check(status);
    return out;
}
pub fn errorLabel(err: ParseError) []const u8 {
    const result = r4os_rust_gpt_label(false, code(err));
    return result.ptr[0..result.len];
}
pub fn partitionTypeLabel(kind: PartitionType) []const u8 {
    const result = r4os_rust_gpt_label(true, @intFromEnum(kind));
    return result.ptr[0..result.len];
}
pub const Crc32 = extern struct {
    state: u32 = 0xFFFF_FFFF,
    pub fn update(self: *Crc32, bytes: []const u8) void {
        r4os_rust_gpt_crc(&self.state, bytes.ptr, bytes.len);
    }
    pub fn finish(self: Crc32) u32 {
        return r4os_rust_gpt_crc_finish(self.state);
    }
};
comptime {
    if (@sizeOf(Header) != 72 or @offsetOf(Header, "entries_crc32") != 64 or @sizeOf(Partition) != 136 or @offsetOf(Partition, "first_lba") != 112 or @sizeOf(Crc32) != 4) @compileError("Native GPT C layout differs");
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
