//! Early normal-OS mount policy. The executable's actual installation owns
//! C:, the unlettered BOOT volume and D: before any other volume is admitted.
const std = @import("std");
const boot_info = @import("../bootloader/boot_info.zig");
const source_owner = @import("boot_source.zig");
const inventory = @import("media_inventory.zig");
const tables = @import("partition_table.zig");
const block = @import("block.zig");
const drive = @import("../fs/drive.zig");
const vfs = @import("../fs/vfs.zig");
const log = @import("../kernel/log.zig");

pub const Result = union(enum) { managed, legacy: usize, rejected: []const u8 };

pub fn mount() Result {
    inventory.scan();
    var views: [inventory.maximum_devices]source_owner.DeviceView = undefined;
    var count: usize = 0;
    for (&inventory.devices) |*record| {
        if (!record.used) continue;
        inventory.readInstallation(record);
        views[count] = .{ .index = record.index, .usb = record.bus == .usb, .local = isLocal(record.bus), .table = &record.table, .installation = if (record.installation) |*m| m else null, .installation_conflict = record.installation_conflict };
        count += 1;
    }
    const identity = &boot_info.get().executable_source;
    log.puts("[INSTALLBOOT] executable=");
    log.puts(identity.path());
    log.puts(" disk=");
    printGuid(identity.disk_guid);
    log.puts(" partition=");
    printGuid(identity.partition_guid);
    log.puts(" mbr-id=");
    log.putDec(identity.mbr_disk_id);
    log.puts(" partition-index=");
    log.putDec(identity.partition_index);
    log.puts("\r\n");
    // Explicit compatibility for the pre-0.76 two-partition MBR image. Its
    // nonzero disk signature must identify exactly one valid physical disk.
    // A missing, cloned or broken GPT identity never falls into this path.
    if (identity.present and !identity.path_truncated and identity.media_type == 0 and
        tables.guid.isZero(identity.disk_guid) and tables.guid.isZero(identity.partition_guid) and
        identity.mbr_disk_id != 0 and (std.ascii.eqlIgnoreCase(identity.path(), "/boot/r4os.elf") or std.ascii.eqlIgnoreCase(identity.path(), "/boot/r4os-prev.elf")))
    {
        var selected: ?usize = null;
        for (&inventory.devices) |*record| {
            if (!record.used or record.table.mbr_disk_id != identity.mbr_disk_id) continue;
            if (selected != null) return .{ .rejected = "Duplicate legacy boot disk signature" };
            if (!record.table.valid or record.table.kind != .mbr) return .{ .rejected = "Invalid legacy boot table" };
            selected = record.index;
        }
        if (selected) |index| {
            log.puts("[INSTALLBOOT] source=legacy-mbr device=");
            log.putDec(index);
            log.puts("\r\n");
            return .{ .legacy = index };
        }
        return .{ .rejected = "Legacy boot disk unavailable" };
    }
    const source = source_owner.resolveNormal(.{ .present = identity.present, .generic_media = identity.media_type == 0, .path = identity.path(), .path_truncated = identity.path_truncated, .disk_guid = identity.disk_guid, .partition_guid = identity.partition_guid }, views[0..count]);
    log.puts("[INSTALLBOOT] source=");
    log.puts(source.reason);
    log.puts("\r\n");
    if (!source.confirmed) return .{ .rejected = source.reason };
    const record = &inventory.devices[source.device_index];
    const boot_index = find(record, source.boot_guid) orelse return .{ .rejected = "BOOT role missing" };
    const system_index = find(record, source.system_guid) orelse return .{ .rejected = "SYSTEM role missing" };
    const data_index = find(record, source.data_guid) orelse return .{ .rejected = "DATA role missing" };
    const boot = inventory.probe(record, boot_index);
    const system = inventory.probe(record, system_index);
    const data = inventory.probe(record, data_index);
    if (boot.filesystem != .fat32 or system.filesystem != .ntfs or data.filesystem != .ntfs)
        return .{ .rejected = "Installation filesystem unavailable (BOOT/SYSTEM/DATA)" };
    if (!vfs.mountBootVolume(boot.volume.?) or !mountLetter(record, system_index, 'C', .system) or
        !mountLetter(record, data_index, 'D', .data)) return .{ .rejected = "Installation mount rejected" };
    _ = drive.setCurrent('C');
    log.puts("[INSTALLBOOT] installation=");
    printGuid(source.installation_id);
    log.puts(" C=");
    printGuid(source.system_guid);
    log.puts(" BOOT=");
    printGuid(source.boot_guid);
    log.puts(" D=");
    printGuid(source.data_guid);
    log.puts("\r\n");
    // Probe the selected filesystems first: a foreign disk cannot consume
    // their mount capacity. Other installations never acquire C:, D: or BOOT.
    for (&inventory.devices) |*other| {
        if (!other.used or !other.table.valid) continue;
        for (other.table.items(), 0..) |_, i| {
            if (other.index == source.device_index and (i == boot_index or i == system_index or i == data_index)) continue;
            const letter = freeLetter() orelse break;
            _ = mountLetter(other, i, letter, .none);
        }
    }
    log.puts("[INSTALLBOOT] mapping=verified C=SYSTEM D=DATA BOOT=unlettered\r\n");
    return .managed;
}

fn mountLetter(record: *inventory.DeviceRecord, index: usize, letter: u8, role: drive.Role) bool {
    if (drive.get(letter) != null) return false;
    const state = inventory.probe(record, index);
    const volume = state.volume orelse return false;
    const part = record.table.partitions[index];
    if (part.sector_count > std.math.maxInt(usize) / 512) return false;
    const kind: drive.Kind = switch (volume) {
        .fat32 => .fat32,
        .ntfs => .ntfs,
    };
    if (!drive.mountBlockRole(letter, kind, role, record.name, @intCast(part.sector_count * 512), record.index)) return false;
    if (!vfs.mountForDrive(letter, volume)) {
        drive.unmountLocked(letter);
        return false;
    }
    state.letter = letter;
    return true;
}
fn find(record: *const inventory.DeviceRecord, id: tables.guid.Guid) ?usize {
    for (record.table.items(), 0..) |part, i| if (tables.guid.eql(part.unique_guid, id)) return i;
    return null;
}
fn freeLetter() ?u8 {
    var letter: u8 = 'E';
    while (letter <= 'Z') : (letter += 1) if (drive.get(letter) == null) return letter;
    return null;
}
fn isLocal(bus: block.Bus) bool {
    return bus == .ata or bus == .ahci or bus == .nvme or bus == .virtio;
}
fn printGuid(value: tables.guid.Guid) void {
    log.puts(&tables.guid.format(value));
}
