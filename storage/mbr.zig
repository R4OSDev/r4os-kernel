const block = @import("block.zig");
const drive = @import("../fs/drive.zig");
const fat32 = @import("../fs/fat/fat32.zig");
const gpt = @import("gpt.zig");
const ntfs_fs = @import("../fs/ntfs/ntfs.zig");
const vfs = @import("../fs/vfs.zig");
const k = @import("../kernel/log.zig");
const std = @import("std");

const SECTOR_SIZE: usize = 512;
const PARTITION_TABLE_OFFSET: usize = 446;
const PARTITION_ENTRY_SIZE: usize = 16;
const SIGNATURE_OFFSET: usize = 510;

pub const Partition = struct {
    bootable: bool,
    type_id: u8,
    first_lba: u64,
    sector_count: u64,
};

const FileSystemHint = enum {
    fat32,
    ntfs,
    probe,
};

const GptEntryReader = struct {
    device_index: usize,
    cache_valid: bool = false,
    cache_lba: u64 = 0,
    cache: [SECTOR_SIZE]u8 = undefined,

    fn read(self: *GptEntryReader, header: gpt.Header, index: u32, out: []u8) bool {
        if (out.len < @as(usize, header.entry_size)) return false;
        var source_offset = @as(u64, index) * @as(u64, header.entry_size);
        var destination_offset: usize = 0;
        var remaining: usize = @intCast(header.entry_size);

        while (remaining > 0) {
            const lba = header.entries_lba + source_offset / SECTOR_SIZE;
            const offset_in_sector: usize = @intCast(source_offset % SECTOR_SIZE);
            if (!self.cache_valid or self.cache_lba != lba) {
                if (!block.read(self.device_index, lba, 1, self.cache[0..])) return false;
                self.cache_valid = true;
                self.cache_lba = lba;
            }
            const count = @min(remaining, SECTOR_SIZE - offset_in_sector);
            @memcpy(out[destination_offset .. destination_offset + count], self.cache[offset_in_sector .. offset_in_sector + count]);
            source_offset += @as(u64, count);
            destination_offset += count;
            remaining -= count;
        }
        return true;
    }
};

pub fn scan(device_index: usize) bool {
    const device = block.get(device_index) orelse {
        k.puts("  Partition scan: block device missing\r\n");
        return false;
    };
    const device_sector_size = device.sector_size;
    const device_sector_count = device.sector_count;
    if (device_sector_size != SECTOR_SIZE) {
        k.puts("  Partition scan: unsupported logical sector size=");
        k.putDec(device_sector_size);
        k.puts("\r\n");
        return false;
    }
    if (device_sector_count == 0) {
        k.puts("  Partition scan: device capacity unavailable\r\n");
        return false;
    }

    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!block.read(device_index, 0, 1, sector[0..])) {
        k.puts("  MBR: read failed\r\n");
        return false;
    }

    if (sector[SIGNATURE_OFFSET] != 0x55 or sector[SIGNATURE_OFFSET + 1] != 0xAA) {
        k.puts("  MBR: invalid signature\r\n");
        return false;
    }

    k.puts("  MBR ");
    k.puts("[OK]\r\n");

    var i: usize = 0;
    while (i < 4) : (i += 1) {
        const p = parsePartition(sector[PARTITION_TABLE_OFFSET + i * PARTITION_ENTRY_SIZE ..][0..PARTITION_ENTRY_SIZE]);
        if (isProtectiveGpt(p.type_id)) {
            k.puts("    partition ");
            k.putDec(i + 1);
            k.puts(": type=0xEE GPT-protective\r\n");
            return scanGpt(device_index, device_sector_count);
        }
    }

    i = 0;
    while (i < 4) : (i += 1) {
        const p = parsePartition(sector[PARTITION_TABLE_OFFSET + i * PARTITION_ENTRY_SIZE ..][0..PARTITION_ENTRY_SIZE]);
        if (p.type_id == 0 or p.sector_count == 0) continue;

        k.puts("    partition ");
        k.putDec(i + 1);
        k.puts(": type=0x");
        k.putHex(p.type_id, 2);
        k.puts(" ");
        k.puts(typeName(p.type_id));
        k.puts(" lba=");
        k.putDec(p.first_lba);
        k.puts(" sectors=");
        k.putDec(p.sector_count);
        k.puts("\r\n");

        if (driveKind(p.type_id)) |kind| {
            const hint: FileSystemHint = if (kind == .fat32) .fat32 else .ntfs;
            mountPartition(device_index, p.first_lba, p.sector_count, p.bootable, hint, typeName(p.type_id));
        }
    }

    return true;
}

fn scanGpt(device_index: usize, device_sector_count: u64) bool {
    k.puts("  GPT protective MBR detected\r\n");

    var using_backup = false;
    var header = loadGptHeader(device_index, 1, device_sector_count);
    if (header == null and device_sector_count > 1) {
        using_backup = true;
        k.puts("  GPT: primary invalid; trying backup header\r\n");
        header = loadGptHeader(device_index, device_sector_count - 1, device_sector_count);
    }
    const valid_header = header orelse {
        k.puts("  GPT: no valid partition table\r\n");
        return false;
    };

    k.puts("  GPT [OK] header=");
    k.puts(if (using_backup) "backup" else "primary");
    k.puts(" entries=");
    k.putDec(valid_header.entry_count);
    k.puts(" entry-size=");
    k.putDec(valid_header.entry_size);
    k.puts("\r\n");

    var reader = GptEntryReader{ .device_index = device_index };
    var raw: [gpt.maximum_entry_size]u8 = undefined;
    var index: u32 = 0;
    while (index < valid_header.entry_count) : (index += 1) {
        const entry_size: usize = @intCast(valid_header.entry_size);
        if (!reader.read(valid_header, index, raw[0..entry_size])) {
            k.puts("    GPT entry read failed index=");
            k.putDec(index + 1);
            k.puts("\r\n");
            return false;
        }
        const parsed = gpt.parsePartition(raw[0..entry_size], valid_header) catch |err| {
            k.puts("    GPT partition ");
            k.putDec(index + 1);
            k.puts(": skipped reason=");
            k.puts(gpt.errorLabel(err));
            k.puts("\r\n");
            continue;
        };
        const partition = parsed orelse continue;

        k.puts("    GPT partition ");
        k.putDec(index + 1);
        k.puts(": type=");
        k.puts(gpt.partitionTypeLabel(partition.partition_type));
        k.puts(" lba=");
        k.putDec(partition.first_lba);
        k.puts(" sectors=");
        k.putDec(partition.sectorCount());
        k.puts("\r\n");

        switch (partition.partition_type) {
            .efi_system => mountPartition(
                device_index,
                partition.first_lba,
                partition.sectorCount(),
                partition.isBootCandidate(),
                .fat32,
                "GPT EFI-system",
            ),
            .microsoft_basic_data => mountPartition(
                device_index,
                partition.first_lba,
                partition.sectorCount(),
                partition.isBootCandidate(),
                .probe,
                "GPT basic-data",
            ),
            .other => k.puts("      unsupported GPT partition type; not mounted\r\n"),
        }
    }
    return true;
}

fn loadGptHeader(device_index: usize, header_lba: u64, device_sector_count: u64) ?gpt.Header {
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!block.read(device_index, header_lba, 1, sector[0..])) {
        k.puts("  GPT header: read failed lba=");
        k.putDec(header_lba);
        k.puts("\r\n");
        return null;
    }
    const header = gpt.parseHeader(sector[0..], header_lba, device_sector_count) catch |err| {
        k.puts("  GPT header: invalid lba=");
        k.putDec(header_lba);
        k.puts(" reason=");
        k.puts(gpt.errorLabel(err));
        k.puts("\r\n");
        return null;
    };
    if (!gptEntryArrayCrcValid(device_index, header)) return null;
    return header;
}

fn gptEntryArrayCrcValid(device_index: usize, header: gpt.Header) bool {
    var crc = gpt.Crc32{};
    var sector: [SECTOR_SIZE]u8 = undefined;
    var remaining = header.entryBytes();
    var lba = header.entries_lba;
    while (remaining > 0) : (lba += 1) {
        if (!block.read(device_index, lba, 1, sector[0..])) {
            k.puts("  GPT entries: read failed lba=");
            k.putDec(lba);
            k.puts("\r\n");
            return false;
        }
        const count: usize = @intCast(@min(remaining, SECTOR_SIZE));
        crc.update(sector[0..count]);
        remaining -= count;
    }
    if (crc.finish() != header.entries_crc32) {
        k.puts("  GPT entries: CRC mismatch\r\n");
        return false;
    }
    return true;
}

fn mountPartition(
    device_index: usize,
    first_lba: u64,
    sector_count: u64,
    boot_candidate: bool,
    requested_hint: FileSystemHint,
    type_name: []const u8,
) void {
    if (first_lba > std.math.maxInt(u32)) {
        k.puts("      partition starts beyond current filesystem LBA limit; not mounted\r\n");
        return;
    }
    const first_lba32: u32 = @intCast(first_lba);
    const hint = if (requested_hint == .probe)
        probeFileSystem(device_index, first_lba)
    else
        requested_hint;

    const volume: vfs.Volume = switch (hint) {
        .fat32 => if (fat32.inspect(device_index, first_lba32)) |found|
            .{ .fat32 = found }
        else {
            k.puts("      FAT32: not mounted (invalid BPB or unsupported layout)\r\n");
            return;
        },
        .ntfs => if (ntfs_fs.inspect(device_index, first_lba32)) |found|
            .{ .ntfs = found }
        else {
            k.puts("      NTFS: not mounted (invalid boot sector or unsupported layout)\r\n");
            return;
        },
        .probe => {
            k.puts("      unsupported filesystem signature; not mounted\r\n");
            return;
        },
    };

    const system_score = systemVolumeScore(volume);
    _ = vfs.listRoot(volume);
    k.puts("      R4OS system markers: ");
    k.putDec(system_score);
    k.puts("/6 ");
    k.puts(if (isSystemVolumeScore(system_score)) "system-candidate" else "data-volume");
    k.puts("\r\n");

    // The FAT32 boot partition (Limine + kernel, no R4OS tree) stays
    // unlettered by design.  MBR's active flag, the GPT ESP type or GPT's
    // legacy-boot attribute supplies the scheme-specific boot indication.
    if (isBootPartition(volume, boot_candidate, system_score)) {
        vfs.mountBootVolume(volume);
        k.puts("      boot partition (Limine): internal mount, unlettered by design\r\n");
        return;
    }

    if (sector_count > std.math.maxInt(u64) / SECTOR_SIZE) {
        k.puts("      partition byte size overflow; not mounted\r\n");
        return;
    }
    const byte_count = sector_count * SECTOR_SIZE;
    if (byte_count > std.math.maxInt(usize)) {
        k.puts("      partition exceeds addressable drive size; not mounted\r\n");
        return;
    }
    const kind: drive.Kind = switch (volume) {
        .fat32 => .fat32,
        .ntfs => .ntfs,
    };
    const letter = nextDriveLetterFor(system_score) orelse return;
    const role = roleFor(system_score);
    if (drive.mountBlockRole(letter, kind, role, type_name, @intCast(byte_count), device_index)) {
        if (letter == 'C') _ = drive.setCurrent('C');
        vfs.mountForDrive(letter, volume);
        k.puts("    mounted as ");
        k.putc(letter);
        k.puts(":\\ role=");
        k.puts(drive.roleName(role));
        k.puts("\r\n");
    }
}

fn probeFileSystem(device_index: usize, first_lba: u64) FileSystemHint {
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!block.read(device_index, first_lba, 1, sector[0..])) {
        k.puts("      filesystem probe: boot sector read failed\r\n");
        return .probe;
    }
    if (bytesEqual(sector[3..11], "NTFS    ")) return .ntfs;
    if (sector[510] == 0x55 and sector[511] == 0xAA) return .fat32;
    return .probe;
}

/// The system volume is recognized FS-neutrally by its markers; C: goes to
/// the first system candidate regardless of filesystem.
fn nextDriveLetterFor(system_score: u8) ?u8 {
    if (isSystemVolumeScore(system_score) and drive.get('C') == null) return 'C';
    var letter: u8 = 'D';
    while (letter <= 'Z') : (letter += 1) {
        if (drive.get(letter) == null) return letter;
    }
    if (drive.get('C') == null) return 'C';
    return null;
}

/// Boot partition heuristic: a scheme-designated FAT32 partition that
/// carries the Limine configuration but no R4OS system tree.
fn isBootPartition(volume: vfs.Volume, boot_candidate: bool, system_score: u8) bool {
    if (!boot_candidate) return false;
    if (isSystemVolumeScore(system_score)) return false;
    return switch (volume) {
        .fat32 => entryExists(volume, "/BOOT/LIMINE.CONF") or entryExists(volume, "/LIMINE-BIOS.SYS"),
        .ntfs => false,
    };
}

fn parsePartition(raw: []const u8) Partition {
    return .{
        .bootable = raw[0] == 0x80,
        .type_id = raw[4],
        .first_lba = readLe32(raw[8..12]),
        .sector_count = readLe32(raw[12..16]),
    };
}

fn driveKind(type_id: u8) ?drive.Kind {
    return switch (type_id) {
        0x0B, 0x0C => .fat32,
        0x07 => .ntfs,
        else => null,
    };
}

fn isProtectiveGpt(type_id: u8) bool {
    return type_id == 0xEE;
}

fn systemVolumeScore(volume: vfs.Volume) u8 {
    var score: u8 = 0;
    if (entryExists(volume, "/BOOT/R4OS.ELF")) score += 1;
    if (entryExists(volume, "/BOOT/LIMINE.CONF")) score += 1;
    if (entryExists(volume, "/CONFIG.R4S")) score += 1;
    if (dirExists(volume, "/R4OS")) score += 1;
    if (dirExists(volume, "/R4OS/CONFIG")) score += 1;
    if (dirExists(volume, "/R4OS/SOFTWARE")) score += 1;
    return score;
}

fn isSystemVolumeScore(score: u8) bool {
    return score >= 3;
}

fn roleFor(system_score: u8) drive.Role {
    if (isSystemVolumeScore(system_score)) return .system;
    return .data;
}

fn entryExists(volume: vfs.Volume, path: []const u8) bool {
    _ = vfs.resolveEntry(volume, path) orelse return false;
    return true;
}

fn dirExists(volume: vfs.Volume, path: []const u8) bool {
    const entry = vfs.resolveEntry(volume, path) orelse return false;
    return entry.isDir();
}

fn typeName(type_id: u8) []const u8 {
    return switch (type_id) {
        0x06 => "FAT16",
        0x0E => "FAT16-LBA",
        0x0B => "FAT32",
        0x0C => "FAT32-LBA",
        0x07 => "NTFS/exFAT",
        0xEE => "GPT-protective",
        else => "unknown",
    };
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

fn bytesEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, 0..) |byte, index| {
        if (byte != b[index]) return false;
    }
    return true;
}
