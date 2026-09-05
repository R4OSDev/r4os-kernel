// Boot-time physical media inventory shared by Recovery and the normal
// installation boot owner. Does not select C:, R: or an installation target.
const std = @import("std");
const block = @import("block.zig");
const tables = @import("partition_table.zig");
const installation = @import("installation.zig");
const fat = @import("../fs/fat/fat32.zig");
const ntfs = @import("../fs/ntfs/ntfs.zig");
const vfs = @import("../fs/vfs.zig");
const heap = @import("../memory/heap.zig");

pub const maximum_devices = 16;
pub const Filesystem = enum { unprobed, none, unknown, fat32, ntfs, invalid, unsupported, no_capacity };
pub const VolumeRecord = struct {
    filesystem: Filesystem = .unprobed,
    volume: ?vfs.Volume = null,
    letter: u8 = 0,
};
pub const DeviceRecord = struct {
    used: bool = false,
    index: usize = 0,
    bus: block.Bus = .unknown,
    name: []const u8 = "",
    model: []const u8 = "",
    driver: []const u8 = "",
    sector_bytes: u32 = 0,
    sectors: u64 = 0,
    writable: bool = false,
    table: tables.Table = .{},
    volumes: [tables.max_partitions]VolumeRecord = .{VolumeRecord{}} ** tables.max_partitions,
    installation: ?installation.Manifest = null,
    installation_conflict: bool = false,
    installation_reason: []const u8 = "not-found",
};
pub var devices: [maximum_devices]DeviceRecord = .{DeviceRecord{}} ** maximum_devices;

pub fn scan() void {
    // Call only before user sessions start. Later rescans must own the mount
    // transaction and preserve existing leases/generations.
    for (&devices) |*record| record.* = .{};
    for (0..@min(block.slotCount(), devices.len)) |index| {
        const device = block.get(index) orelse continue;
        if (device.bus == .ram) continue;
        const record = &devices[index];
        record.* = .{ .used = true, .index = index, .bus = device.bus, .name = device.name, .model = device.model, .driver = device.driver, .sector_bytes = device.sector_size, .sectors = device.sector_count, .writable = device.writable };
        var context = index;
        tables.scan(.{ .ctx = &context, .read = readSector, .sector_bytes = device.sector_size, .sectors = device.sector_count }, &record.table);
    }
}

pub fn probe(record: *DeviceRecord, part_index: usize) *VolumeRecord {
    const result = &record.volumes[part_index];
    if (result.filesystem != .unprobed) return result;
    result.filesystem = .invalid;
    if (!record.table.valid or part_index >= record.table.count) return result;
    const part = record.table.partitions[part_index];
    if (tables.guid.eql(part.type_guid, tables.bios_boot_guid)) {
        result.filesystem = .none;
        return result;
    }
    // The existing FS owners use u32 partition bases. Keep that explicit and
    // avoid narrowing a large foreign LBA into another partition.
    if (record.sector_bytes != 512 or part.first_lba > std.math.maxInt(u32)) {
        result.filesystem = .unsupported;
        return result;
    }
    var boot: [512]u8 = undefined;
    if (!block.read(record.index, part.first_lba, 1, &boot)) return result;
    if (std.mem.eql(u8, boot[3..11], "NTFS    ")) {
        if (ntfs.inspectBounded(record.index, @intCast(part.first_lba), part.sector_count)) |volume| {
            result.volume = .{ .ntfs = volume };
            result.filesystem = .ntfs;
        }
    } else if (std.mem.eql(u8, boot[82..90], "FAT32   ")) {
        if (fat.parseBounded(record.index, @intCast(part.first_lba), part.sector_count)) |volume| {
            result.volume = .{ .fat32 = volume };
            result.filesystem = .fat32;
        }
    } else result.filesystem = .unknown;
    return result;
}

pub fn readInstallation(record: *DeviceRecord) void {
    if (!record.table.valid or record.table.kind != .gpt) return;
    for (record.table.items(), 0..) |part, i| {
        if (!tables.guid.eql(part.type_guid, tables.esp_guid)) continue;
        const volume = probe(record, i).volume orelse continue;
        const entry = vfs.resolveEntry(volume, "/boot/r4os-installation.json") orelse continue;
        record.installation_reason = "invalid-manifest";
        if (entry.isDir() or entry.size == 0 or entry.size > installation.max_bytes) continue;
        const bytes = heap.alloc(@intCast(entry.size), 8) orelse {
            record.installation_reason = "out-of-memory";
            continue;
        };
        defer _ = heap.free(bytes);
        if (vfs.readFileRange(volume, entry, 0, bytes) != bytes.len) continue;
        // Bound JSON allocation independently of the image or file size.
        const scratch = heap.alloc(128 * 1024, 8) orelse continue;
        defer _ = heap.free(scratch);
        var allocator = std.heap.FixedBufferAllocator.init(scratch);
        const manifest = installation.parse(allocator.allocator(), bytes) catch |err| {
            record.installation_reason = @errorName(err);
            continue;
        };
        if (!installation.matches(&manifest, &record.table, part.unique_guid)) {
            record.installation_reason = "gpt-mismatch";
            continue;
        }
        if (record.installation != null) record.installation_conflict = true;
        record.installation = manifest;
        record.installation_reason = "ok";
    }
}

fn readSector(ctx: *anyopaque, lba: u64, out: *[512]u8) bool {
    const index: *const usize = @ptrCast(@alignCast(ctx));
    return block.read(index.*, lba, 1, out);
}
