const block = @import("block.zig");
const drive = @import("../fs/drive.zig");
const fat32 = @import("../fs/fat/fat32.zig");
const ntfs_fs = @import("../fs/ntfs/ntfs.zig");
const vfs = @import("../fs/vfs.zig");
const k = @import("../kernel/log.zig");

const SECTOR_SIZE: usize = 512;
const PARTITION_TABLE_OFFSET: usize = 446;
const PARTITION_ENTRY_SIZE: usize = 16;
const SIGNATURE_OFFSET: usize = 510;

pub const Partition = struct {
    bootable: bool,
    type_id: u8,
    first_lba: u32,
    sector_count: u32,
};

pub fn scan(device_index: usize) bool {
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

        if (isProtectiveGpt(p.type_id)) {
            k.puts("      GPT protective MBR detected; GPT partition parsing follows after 0.13.8\r\n");
            continue;
        }

        var volume: ?vfs.Volume = null;
        var system_score: u8 = 0;
        if (isFat32(p.type_id)) {
            if (fat32.inspect(device_index, p.first_lba)) |found_volume| {
                const wrapped = vfs.Volume{ .fat32 = found_volume };
                volume = wrapped;
            } else {
                k.puts("      FAT32: not mounted (invalid BPB or unsupported layout)\r\n");
                continue;
            }
        }

        if (isNtfs(p.type_id)) {
            if (ntfs_fs.inspect(device_index, p.first_lba)) |found_volume| {
                const wrapped = vfs.Volume{ .ntfs = found_volume };
                volume = wrapped;
            } else {
                k.puts("      NTFS: not mounted (invalid boot sector or unsupported layout)\r\n");
                continue;
            }
        }

        if (volume) |wrapped| {
            system_score = systemVolumeScore(wrapped);
            _ = vfs.listRoot(wrapped);
            k.puts("      R4OS system markers: ");
            k.putDec(system_score);
            k.puts("/6 ");
            k.puts(if (isSystemVolumeScore(system_score)) "system-candidate" else "data-volume");
            k.puts("\r\n");

            // The FAT32 boot partition (Limine + kernel, no R4OS tree) stays
            // unlettered by design: it is not a general-purpose drive.  It is
            // mounted internally so the /boot subtree of the system drive can
            // route to it (SYSUPD boot-kernel updates, 0.60.11).
            if (isBootPartition(wrapped, p, system_score)) {
                vfs.mountBootVolume(wrapped);
                k.puts("      boot partition (Limine): internal mount, unlettered by design\r\n");
                continue;
            }
        }

        if (driveKind(p.type_id)) |kind| {
            if (nextDriveLetterFor(system_score)) |letter| {
                const role = roleFor(system_score);
                if (drive.mountBlockRole(letter, kind, role, typeName(p.type_id), @as(usize, p.sector_count) * SECTOR_SIZE, device_index)) {
                    if (letter == 'C') _ = drive.setCurrent('C');
                    if (volume) |mounted_volume| vfs.mountForDrive(letter, mounted_volume);
                    k.puts("    mounted as ");
                    k.putc(letter);
                    k.puts(":\\ role=");
                    k.puts(drive.roleName(role));
                    k.puts("\r\n");
                }
            }
        }
    }

    return true;
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

/// Boot partition heuristic: a bootable FAT32 partition that carries the
/// Limine configuration but no R4OS system tree.
fn isBootPartition(volume: vfs.Volume, p: Partition, system_score: u8) bool {
    if (!p.bootable) return false;
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

fn isFat32(type_id: u8) bool {
    return switch (type_id) {
        0x0B, 0x0C => true,
        else => false,
    };
}

fn isNtfs(type_id: u8) bool {
    return type_id == 0x07;
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
