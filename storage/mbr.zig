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

pub const TableKind = enum {
    none,
    mbr,
    gpt_primary,
    gpt_backup,
};

pub const ScanResult = enum {
    device_missing,
    logical_sector_unsupported,
    capacity_unavailable,
    sector_zero_read_failed,
    mbr_signature_invalid,
    no_mountable_partition,
    unsupported_partition,
    partition_invalid,
    gpt_invalid,
    gpt_entry_read_failed,
    partition_lba_unsupported,
    filesystem_probe_read_failed,
    filesystem_lookup_failed,
    filesystem_unknown,
    fat32_invalid,
    ntfs_invalid,
    boot_volume_only,
    data_volume_only,
    partition_size_unsupported,
    drive_letters_exhausted,
    mount_rejected,
    system_mounted,
};

pub const ScanReport = struct {
    table: TableKind = .none,
    result: ScanResult = .device_missing,
    detail: []const u8 = "",
    table_valid: bool = false,
    partition_count: u16 = 0,
    mountable_count: u16 = 0,
    filesystem_count: u16 = 0,
    best_system_score: u8 = 0,
    marker_found_mask: u8 = 0,
    marker_io_mask: u8 = 0,
    mbr_type: u8 = 0,

    fn note(self: *ScanReport, result: ScanResult, detail: []const u8) void {
        if (resultPriority(result) < resultPriority(self.result)) return;
        self.result = result;
        self.detail = detail;
    }
};

const FileSystemHint = enum {
    fat32,
    ntfs,
    probe,
};

const SystemVolumeProbe = struct {
    score: u8 = 0,
    found_mask: u8 = 0,
    io_mask: u8 = 0,
};

const MARKER_BOOT_KERNEL: u8 = 1 << 0;
const MARKER_BOOT_CONFIG: u8 = 1 << 1;
const MARKER_ROOT_CONFIG: u8 = 1 << 2;
const MARKER_R4OS_ROOT: u8 = 1 << 3;
const MARKER_R4OS_CONFIG: u8 = 1 << 4;
const MARKER_R4OS_SOFTWARE: u8 = 1 << 5;

const GptEntryReader = struct {
    device_index: usize,
    cache: *[SECTOR_SIZE]u8,
    cache_valid: bool = false,
    cache_lba: u64 = 0,

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

pub fn scan(device_index: usize) ScanReport {
    var report = ScanReport{};
    const device = block.get(device_index) orelse {
        k.puts("  Partition scan: block device missing\r\n");
        return report;
    };
    const device_sector_size = device.sector_size;
    const device_sector_count = device.sector_count;
    if (device_sector_size != SECTOR_SIZE) {
        k.puts("  Partition scan: unsupported logical sector size=");
        k.putDec(device_sector_size);
        k.puts("\r\n");
        report.result = .logical_sector_unsupported;
        return report;
    }
    if (device_sector_count == 0) {
        k.puts("  Partition scan: device capacity unavailable\r\n");
        report.result = .capacity_unavailable;
        return report;
    }

    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!block.read(device_index, 0, 1, sector[0..])) {
        k.puts("  MBR: read failed\r\n");
        report.result = .sector_zero_read_failed;
        return report;
    }

    if (sector[SIGNATURE_OFFSET] != 0x55 or sector[SIGNATURE_OFFSET + 1] != 0xAA) {
        k.puts("  MBR: invalid signature\r\n");
        report.result = .mbr_signature_invalid;
        return report;
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
            scanGpt(device_index, device_sector_count, &report);
            return report;
        }
    }

    report.table = .mbr;
    report.table_valid = true;
    report.result = .no_mountable_partition;

    i = 0;
    while (i < 4) : (i += 1) {
        const p = parsePartition(sector[PARTITION_TABLE_OFFSET + i * PARTITION_ENTRY_SIZE ..][0..PARTITION_ENTRY_SIZE]);
        if (p.type_id == 0 or p.sector_count == 0) continue;
        increment(&report.partition_count);
        report.mbr_type = p.type_id;

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
            increment(&report.mountable_count);
            const hint: FileSystemHint = if (kind == .fat32) .fat32 else .ntfs;
            mountPartition(device_index, p.first_lba, p.sector_count, p.bootable, hint, typeName(p.type_id), &report);
        } else {
            report.note(.unsupported_partition, "mbr-type");
        }
    }

    return report;
}

fn scanGpt(device_index: usize, device_sector_count: u64, report: *ScanReport) void {
    k.puts("  GPT protective MBR detected\r\n");
    report.result = .gpt_invalid;
    report.detail = "no-valid-header";

    // Keep the same DMA destination for header/CRC validation and the later
    // entry walk.  Besides avoiding a second 512-byte stack object, this
    // preserves the exact mapping that has already completed successfully on
    // the controller before any partition metadata is consumed.
    var sector: [SECTOR_SIZE]u8 = undefined;
    var using_backup = false;
    var header = loadGptHeader(device_index, 1, device_sector_count, &sector, report);
    if (header == null and device_sector_count > 1) {
        using_backup = true;
        k.puts("  GPT: primary invalid; trying backup header\r\n");
        header = loadGptHeader(device_index, device_sector_count - 1, device_sector_count, &sector, report);
    }
    const valid_header = header orelse {
        k.puts("  GPT: no valid partition table\r\n");
        return;
    };

    report.table = if (using_backup) .gpt_backup else .gpt_primary;
    report.table_valid = true;
    report.result = .no_mountable_partition;
    report.detail = if (using_backup) "primary-fallback" else "";

    k.puts("  GPT [OK] header=");
    k.puts(if (using_backup) "backup" else "primary");
    k.puts(" entries=");
    k.putDec(valid_header.entry_count);
    k.puts(" entry-size=");
    k.putDec(valid_header.entry_size);
    k.puts("\r\n");

    var reader = GptEntryReader{ .device_index = device_index, .cache = &sector };
    var raw: [gpt.maximum_entry_size]u8 = undefined;
    var index: u32 = 0;
    while (index < valid_header.entry_count) : (index += 1) {
        const entry_size: usize = @intCast(valid_header.entry_size);
        if (!reader.read(valid_header, index, raw[0..entry_size])) {
            k.puts("    GPT entry read failed index=");
            k.putDec(index + 1);
            k.puts("\r\n");
            report.note(.gpt_entry_read_failed, "entry-read");
            return;
        }
        const parsed = gpt.parsePartition(raw[0..entry_size], valid_header) catch |err| {
            k.puts("    GPT partition ");
            k.putDec(index + 1);
            k.puts(": skipped reason=");
            k.puts(gpt.errorLabel(err));
            k.puts("\r\n");
            report.note(.partition_invalid, gpt.errorLabel(err));
            continue;
        };
        const partition = parsed orelse continue;
        increment(&report.partition_count);

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
            .efi_system => {
                increment(&report.mountable_count);
                mountPartition(
                    device_index,
                    partition.first_lba,
                    partition.sectorCount(),
                    partition.isBootCandidate(),
                    .fat32,
                    "GPT EFI-system",
                    report,
                );
            },
            .microsoft_basic_data => {
                increment(&report.mountable_count);
                mountPartition(
                    device_index,
                    partition.first_lba,
                    partition.sectorCount(),
                    partition.isBootCandidate(),
                    .probe,
                    "GPT basic-data",
                    report,
                );
            },
            .other => {
                k.puts("      unsupported GPT partition type; not mounted\r\n");
                report.note(.unsupported_partition, "gpt-guid");
            },
        }
    }
}

fn loadGptHeader(
    device_index: usize,
    header_lba: u64,
    device_sector_count: u64,
    sector: *[SECTOR_SIZE]u8,
    report: *ScanReport,
) ?gpt.Header {
    if (!block.read(device_index, header_lba, 1, sector[0..])) {
        k.puts("  GPT header: read failed lba=");
        k.putDec(header_lba);
        k.puts("\r\n");
        report.detail = "header-read";
        return null;
    }
    const header = gpt.parseHeader(sector[0..], header_lba, device_sector_count) catch |err| {
        k.puts("  GPT header: invalid lba=");
        k.putDec(header_lba);
        k.puts(" reason=");
        k.puts(gpt.errorLabel(err));
        k.puts("\r\n");
        report.detail = gpt.errorLabel(err);
        return null;
    };
    if (!gptEntryArrayCrcValid(device_index, header, sector, report)) return null;
    return header;
}

fn gptEntryArrayCrcValid(
    device_index: usize,
    header: gpt.Header,
    sector: *[SECTOR_SIZE]u8,
    report: *ScanReport,
) bool {
    var crc = gpt.Crc32{};
    var remaining = header.entryBytes();
    var lba = header.entries_lba;
    while (remaining > 0) : (lba += 1) {
        if (!block.read(device_index, lba, 1, sector[0..])) {
            k.puts("  GPT entries: read failed lba=");
            k.putDec(lba);
            k.puts("\r\n");
            report.detail = "entry-array-read";
            return false;
        }
        const count: usize = @intCast(@min(remaining, SECTOR_SIZE));
        crc.update(sector[0..count]);
        remaining -= count;
    }
    if (crc.finish() != header.entries_crc32) {
        k.puts("  GPT entries: CRC mismatch\r\n");
        report.detail = "entry-array-crc";
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
    report: *ScanReport,
) void {
    if (first_lba > std.math.maxInt(u32)) {
        k.puts("      partition starts beyond current filesystem LBA limit; not mounted\r\n");
        report.note(.partition_lba_unsupported, "first-lba-u32");
        return;
    }
    const first_lba32: u32 = @intCast(first_lba);
    const hint = if (requested_hint == .probe)
        probeFileSystem(device_index, first_lba, report)
    else
        requested_hint;

    const volume: vfs.Volume = switch (hint) {
        .fat32 => if (fat32.inspect(device_index, first_lba32)) |found|
            .{ .fat32 = found }
        else {
            k.puts("      FAT32: not mounted (invalid BPB or unsupported layout)\r\n");
            report.note(.fat32_invalid, "fat32-inspect");
            return;
        },
        .ntfs => if (ntfs_fs.inspect(device_index, first_lba32)) |found|
            .{ .ntfs = found }
        else {
            k.puts("      NTFS: not mounted (invalid boot sector or unsupported layout)\r\n");
            report.note(.ntfs_invalid, "ntfs-inspect");
            return;
        },
        .probe => {
            k.puts("      unsupported filesystem signature; not mounted\r\n");
            return;
        },
    };

    increment(&report.filesystem_count);
    const system_probe = probeSystemVolume(volume);
    const system_score = system_probe.score;
    report.best_system_score = @max(report.best_system_score, system_score);
    report.marker_found_mask |= system_probe.found_mask;
    report.marker_io_mask |= system_probe.io_mask;
    _ = vfs.listRoot(volume);
    k.puts("      R4OS system markers: ");
    k.putDec(system_score);
    k.puts("/6 found=0x");
    k.putHex(system_probe.found_mask, 2);
    k.puts(" io=0x");
    k.putHex(system_probe.io_mask, 2);
    k.puts(" ");
    k.puts(if (isSystemVolumeScore(system_score)) "system-candidate" else "data-volume");
    k.puts("\r\n");

    // A metadata/I/O failure is not evidence that a marker is absent. Do not
    // misclassify and expose a possibly damaged R4OS system volume as a data
    // drive; continue scanning other partitions and retain the exact marker
    // bits for the fatal-screen diagnosis.
    if (system_probe.io_mask != 0) {
        k.puts("      system-marker lookup failed; not mounted\r\n");
        report.note(.filesystem_lookup_failed, "system-marker-io");
        return;
    }

    // The FAT32 boot partition (Limine + kernel, no R4OS tree) stays
    // unlettered by design.  MBR's active flag, the GPT ESP type or GPT's
    // legacy-boot attribute supplies the scheme-specific boot indication.
    if (isBootPartition(volume, boot_candidate, system_score)) {
        vfs.mountBootVolume(volume);
        k.puts("      boot partition (Limine): internal mount, unlettered by design\r\n");
        report.note(.boot_volume_only, "limine-volume");
        return;
    }

    if (sector_count > std.math.maxInt(u64) / SECTOR_SIZE) {
        k.puts("      partition byte size overflow; not mounted\r\n");
        report.note(.partition_size_unsupported, "byte-overflow");
        return;
    }
    const byte_count = sector_count * SECTOR_SIZE;
    if (byte_count > std.math.maxInt(usize)) {
        k.puts("      partition exceeds addressable drive size; not mounted\r\n");
        report.note(.partition_size_unsupported, "address-space");
        return;
    }
    const kind: drive.Kind = switch (volume) {
        .fat32 => .fat32,
        .ntfs => .ntfs,
    };
    const letter = nextDriveLetterFor(system_score) orelse {
        k.puts("      no drive letter available; not mounted\r\n");
        report.note(.drive_letters_exhausted, "letters-a-z");
        return;
    };
    const role = roleFor(system_score);
    if (drive.mountBlockRole(letter, kind, role, type_name, @intCast(byte_count), device_index)) {
        if (letter == 'C') _ = drive.setCurrent('C');
        vfs.mountForDrive(letter, volume);
        k.puts("    mounted as ");
        k.putc(letter);
        k.puts(":\\ role=");
        k.puts(drive.roleName(role));
        k.puts("\r\n");
        report.note(
            if (isSystemVolumeScore(system_score)) .system_mounted else .data_volume_only,
            if (isSystemVolumeScore(system_score)) "system-markers" else "insufficient-markers",
        );
    } else {
        k.puts("      drive registry rejected mount\r\n");
        report.note(.mount_rejected, "drive-registry");
    }
}

fn probeFileSystem(device_index: usize, first_lba: u64, report: *ScanReport) FileSystemHint {
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!block.read(device_index, first_lba, 1, sector[0..])) {
        k.puts("      filesystem probe: boot sector read failed\r\n");
        report.note(.filesystem_probe_read_failed, "boot-sector-read");
        return .probe;
    }
    if (bytesEqual(sector[3..11], "NTFS    ")) return .ntfs;
    if (sector[510] == 0x55 and sector[511] == 0xAA) return .fat32;
    report.note(.filesystem_unknown, "boot-signature");
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

fn probeSystemVolume(volume: vfs.Volume) SystemVolumeProbe {
    var probe = SystemVolumeProbe{};
    probeMarker(volume, "/BOOT/R4OS.ELF", false, MARKER_BOOT_KERNEL, &probe);
    probeMarker(volume, "/BOOT/LIMINE.CONF", false, MARKER_BOOT_CONFIG, &probe);
    probeMarker(volume, "/CONFIG.R4S", false, MARKER_ROOT_CONFIG, &probe);
    probeMarker(volume, "/R4OS", true, MARKER_R4OS_ROOT, &probe);
    probeMarker(volume, "/R4OS/CONFIG", true, MARKER_R4OS_CONFIG, &probe);
    probeMarker(volume, "/R4OS/SOFTWARE", true, MARKER_R4OS_SOFTWARE, &probe);
    return probe;
}

fn probeMarker(
    volume: vfs.Volume,
    path: []const u8,
    require_directory: bool,
    mask: u8,
    probe: *SystemVolumeProbe,
) void {
    var entry: vfs.Entry = undefined;
    switch (vfs.resolveEntryStatus(volume, path, &entry)) {
        .found => {
            if (require_directory and !entry.isDir()) return;
            probe.score += 1;
            probe.found_mask |= mask;
        },
        .not_found => {},
        .io => probe.io_mask |= mask,
    }
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

fn increment(value: *u16) void {
    if (value.* != std.math.maxInt(u16)) value.* += 1;
}

fn resultPriority(result: ScanResult) u8 {
    return switch (result) {
        .device_missing,
        .logical_sector_unsupported,
        .capacity_unavailable,
        .sector_zero_read_failed,
        .mbr_signature_invalid,
        .gpt_invalid,
        => 0,
        .no_mountable_partition => 1,
        .unsupported_partition => 10,
        .partition_invalid => 20,
        .boot_volume_only => 30,
        .filesystem_unknown => 40,
        .filesystem_probe_read_failed => 50,
        .fat32_invalid, .ntfs_invalid => 60,
        .partition_lba_unsupported, .partition_size_unsupported => 70,
        .gpt_entry_read_failed => 75,
        .data_volume_only => 80,
        .filesystem_lookup_failed => 85,
        .drive_letters_exhausted => 90,
        .mount_rejected => 100,
        .system_mounted => 255,
    };
}
