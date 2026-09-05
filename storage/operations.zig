// Kernel mechanism behind the public storage facade. Partition policy and
// filesystem construction belong to external tools; this owner coordinates
// identity, admission, cache durability and mount publication only.
const std = @import("std");
const api = @import("r4os_kernel_contract");
const access = @import("access_runtime.zig");
const core = @import("access_state.zig");
const block = @import("block.zig");
const table = @import("partition_table.zig");
const vfs = @import("../fs/vfs.zig");
const drive = @import("../fs/drive.zig");
const fat = @import("../fs/fat/fat32.zig");
const ntfs = @import("../fs/ntfs/ntfs.zig");
const cache = @import("../fs/page_cache.zig");
const sync = @import("../sched/sync.zig");
const context = @import("../sched/task_context.zig");
const task = @import("../sched/task.zig");
const scheduler = @import("../sched/scheduler.zig");

const Error = core.Error || error{ Io, Unsupported, Remount, Incomplete, NotFound };
const Record = struct {
    ready: bool = false,
    identity: block.Identity = undefined,
    generation: u64 = 0,
    layout: table.Table = .{},
    hints: [table.max_partitions]u32 = .{0} ** table.max_partitions,
    errors: [table.max_partitions]i32 = .{0} ** table.max_partitions,
    last_error: i32 = 0,
};
const SavedMount = struct {
    slot: u32,
    reference: access.MountRef,
    region: access.Region,
    partition: table.Partition,
    kind: drive.Kind,
    role: drive.Role,
    bytes: usize,
    name: [32]u8,
};
const Operation = struct {
    id: u64 = 0,
    owner: access.Owner = undefined,
    region: access.Region = undefined,
    saved: [core.max_mounts]SavedMount = undefined,
    saved_count: usize = 0,
    cleanup: bool = false,
};

// A sleepable, unwind-aware orchestration gate; it never spans the lifetime
// of a returned claim or ordinary file traffic on unaffected volumes.
var gate = sync.UnwindGuard.init("storage-topology");
var records: [core.max_devices]Record = .{Record{}} ** core.max_devices;
var operations: [core.max_claims]Operation = .{Operation{}} ** core.max_claims;
var scan_scratch: table.Table = .{};
var cleanup_started = false;

fn enter() bool {
    return gate.enter(2000);
}

fn code(err: Error) i32 {
    return switch (err) {
        error.Invalid => api.storage_error_invalid,
        error.Stale => api.storage_error_stale,
        error.Busy => api.storage_error_busy,
        error.Protected => api.storage_error_protected,
        error.Capacity => api.storage_error_capacity,
        error.WrongOwner => api.storage_error_owner,
        error.Io => api.storage_error_io,
        error.Unsupported => api.storage_error_unsupported,
        error.Remount => api.storage_error_remount,
        error.Incomplete => api.storage_error_incomplete,
        error.NotFound => api.storage_error_not_found,
    };
}

fn publicDevice(ref: access.DeviceRef) api.StorageDeviceRef {
    return .{ .slot = ref.slot, .generation = ref.generation };
}
fn privateDevice(ref: api.StorageDeviceRef) Error!access.DeviceRef {
    if (ref.reserved != 0 or ref.generation == 0 or ref.slot >= records.len) return error.Invalid;
    const found = block.identity(ref.slot) orelse return error.Stale;
    if (found.reference.generation != ref.generation) return error.Stale;
    return found.reference;
}
fn all(identity: block.Identity) access.Region {
    return .{ .device = identity.reference, .first = 0, .count = identity.sectors };
}
fn copyText(out: []u8, value: []const u8) void {
    @memset(out, 0);
    const len = @min(out.len - 1, value.len);
    @memcpy(out[0..len], value[0..len]);
}
fn span(value: []const u8) []const u8 {
    return value[0 .. std.mem.indexOfScalar(u8, value, 0) orelse value.len];
}
fn reader(ctx: *anyopaque, lba: u64, out: *[512]u8) bool {
    const slot: *const usize = @ptrCast(@alignCast(ctx));
    return block.read(slot.*, lba, 1, out);
}
fn incomplete(record: *const Record) bool {
    return std.mem.eql(u8, record.layout.reason, "TooManyPartitions") or
        std.mem.eql(u8, record.layout.reason, "UnsupportedEntryLayout");
}
fn unsupported(record: *const Record) bool {
    return record.identity.sector_bytes != 512 or record.identity.sectors == 0 or
        record.identity.sectors > std.math.maxInt(u64) / 512;
}

// Called with a transient complete-device read use or an exclusive claim.
// Only the partition scanner runs here; live NTFS/FAT state is never probed
// again during inventory enumeration.
fn scanRecord(record: *Record, identity: block.Identity, own: ?access.Region) Error!void {
    var slot: usize = identity.reference.slot;
    table.scan(.{ .ctx = &slot, .read = reader, .sector_bytes = identity.sector_bytes, .sectors = identity.sectors }, &scan_scratch);
    try access.topologyChanged();
    record.ready = true;
    record.identity = identity;
    record.generation = access.topologyGeneration();
    record.layout = scan_scratch;
    for (record.layout.items(), 0..) |part, i| {
        const region: access.Region = .{ .device = identity.reference, .first = part.first_lba, .count = part.sector_count };
        if (access.regionClaimed(region) and (own == null or !own.?.contains(region))) continue;
        record.hints[i] = api.storage_filesystem_unknown;
        if (!record.layout.valid or identity.sector_bytes != 512) continue;
        if (table.guid.eql(part.type_guid, table.bios_boot_guid)) {
            record.hints[i] = api.storage_filesystem_none;
            continue;
        }
        var sector: [512]u8 = undefined;
        if (!block.read(slot, part.first_lba, 1, &sector)) {
            record.errors[i] = api.storage_error_io;
            continue;
        }
        if (std.mem.eql(u8, sector[3..11], "NTFS    ")) record.hints[i] = api.storage_filesystem_ntfs;
        if (std.mem.eql(u8, sector[82..90], "FAT32   ")) record.hints[i] = api.storage_filesystem_fat32;
    }
}

fn ensureRecords() Error!void {
    for (0..records.len) |i| {
        const identity = block.identity(i) orelse {
            records[i].ready = false;
            continue;
        };
        const record = &records[i];
        if (record.ready and std.meta.eql(record.identity.reference, identity.reference)) continue;
        const use = try access.beginRawRead(all(identity));
        defer access.endUse(use) catch {};
        record.* = .{};
        try scanRecord(record, identity, null);
    }
}

fn wholeTarget(record: *const Record) api.StorageTarget {
    return .{ .device = publicDevice(record.identity.reference), .layout_generation = record.generation, .sector_count = record.identity.sectors, .kind = api.storage_target_device };
}
fn partitionTarget(record: *const Record, part: table.Partition) api.StorageTarget {
    return .{ .device = publicDevice(record.identity.reference), .layout_generation = record.generation, .first_lba = part.first_lba, .sector_count = part.sector_count, .partition_number = part.number, .kind = api.storage_target_partition, .partition_guid = part.unique_guid };
}
fn targetForRegion(record: *const Record, region: access.Region) api.StorageTarget {
    for (record.layout.items()) |part| if (part.first_lba == region.first and part.sector_count >= region.count)
        return partitionTarget(record, part);
    return wholeTarget(record);
}

fn validate(target: *const api.StorageTarget, mutation: bool) Error!access.Region {
    if (target.version != 1 or target.size != @sizeOf(api.StorageTarget)) return error.Invalid;
    const ref = try privateDevice(target.device);
    const record = &records[ref.slot];
    if (!record.ready or !std.meta.eql(record.identity.reference, ref) or record.generation != target.layout_generation) return error.Stale;
    if (unsupported(record)) return error.Unsupported;
    if (mutation and incomplete(record)) return error.Incomplete;
    if (mutation and !record.identity.writable) return error.Protected;
    if (target.kind == api.storage_target_device) {
        if (target.first_lba != 0 or target.sector_count != record.identity.sectors or target.partition_number != 0 or
            !table.guid.eql(target.partition_guid, table.guid.zero)) return error.Invalid;
        return all(record.identity);
    }
    if (target.kind != api.storage_target_partition) return error.Invalid;
    if (!record.layout.valid) return error.Incomplete;
    for (record.layout.items()) |part| {
        if (part.number != target.partition_number) continue;
        if (part.first_lba != target.first_lba or part.sector_count != target.sector_count or
            !table.guid.eql(part.unique_guid, target.partition_guid)) return error.Stale;
        return .{ .device = ref, .first = part.first_lba, .count = part.sector_count };
    }
    return error.Stale;
}

pub fn inventory(out: *api.StorageInventory) callconv(.c) i32 {
    if (out.version != 1 or out.size != @sizeOf(api.StorageInventory)) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    ensureRecords() catch |err| return code(err);
    if (access.topologyGeneration() == std.math.maxInt(u64)) return api.storage_error_capacity;
    var flags: u32 = 0;
    for (&records) |*record| if (record.ready and incomplete(record)) {
        flags |= api.storage_inventory_partial;
    };
    out.* = .{ .generation = access.topologyGeneration(), .device_slots = @intCast(block.slotCount()), .volume_slots = core.max_mounts, .flags = flags };
    return api.storage_result_ok;
}

pub fn deviceInfo(generation: u64, slot: u32, out: *api.StorageDeviceInfo) callconv(.c) i32 {
    if (out.version != 1 or out.size != @sizeOf(api.StorageDeviceInfo) or slot >= records.len) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    if (generation != access.topologyGeneration()) return api.storage_error_stale;
    const identity = block.identity(slot) orelse return api.storage_result_absent;
    const record = &records[slot];
    if (!record.ready or !std.meta.eql(record.identity.reference, identity.reference)) return api.storage_error_stale;
    var flags: u32 = if (identity.writable) api.storage_device_writable else 0;
    if (record.layout.valid) flags |= api.storage_device_table_valid;
    if (record.layout.kind == .gpt) flags |= api.storage_device_gpt;
    if (record.layout.kind == .mbr) flags |= api.storage_device_mbr;
    if (identity.bus == .ram) flags |= api.storage_device_ram;
    if (access.regionClaimed(all(identity))) flags |= api.storage_device_claimed;
    if (record.last_error != 0) flags |= api.storage_device_failed;
    if (unsupported(record)) flags |= api.storage_device_unsupported;
    if (incomplete(record)) flags |= api.storage_device_partial;
    out.* = .{ .reference = publicDevice(identity.reference), .layout_generation = record.generation, .sector_count = identity.sectors, .disk_guid = record.layout.disk_guid, .first_usable = record.layout.first_usable, .last_usable = record.layout.last_usable, .bus = @intFromEnum(identity.bus), .flags = flags, .sector_bytes = identity.sector_bytes, .partition_slots = @intCast(record.layout.count), .last_error = record.last_error, .model = identity.model, .name = identity.name, .driver = identity.driver };
    copyText(&out.reason, record.layout.reason);
    return api.storage_result_present;
}

pub fn partitionInfo(generation: u64, device: *const api.StorageDeviceRef, slot: u32, out: *api.StoragePartitionInfo) callconv(.c) i32 {
    if (out.version != 1 or out.size != @sizeOf(api.StoragePartitionInfo)) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    if (generation != access.topologyGeneration()) return api.storage_error_stale;
    const ref = privateDevice(device.*) catch |err| return code(err);
    const record = &records[ref.slot];
    if (!record.ready or !std.meta.eql(record.identity.reference, ref)) return api.storage_error_stale;
    if (slot >= record.layout.count) return api.storage_result_absent;
    const part = record.layout.partitions[slot];
    const region: access.Region = .{ .device = ref, .first = part.first_lba, .count = part.sector_count };
    var flags: u32 = if (access.regionClaimed(region)) api.storage_partition_claimed else 0;
    for (0..core.max_mounts) |i| {
        const mounted_ref = access.mountReference(@intCast(i)) orelse continue;
        const value = access.mountSnapshot(mounted_ref) catch continue;
        if (value.region.overlaps(region)) flags |= api.storage_partition_mounted;
    }
    if (record.errors[slot] != 0) flags |= api.storage_partition_failed;
    out.* = .{ .target = partitionTarget(record, part), .type_guid = part.type_guid, .attributes = part.attributes, .filesystem = record.hints[slot], .flags = flags, .last_error = record.errors[slot], .mbr_type = part.mbr_type, .name = part.name };
    return api.storage_result_present;
}

pub fn volumeInfo(generation: u64, slot: u32, out: *api.StorageVolumeInfo) callconv(.c) i32 {
    if (out.version != 1 or out.size != @sizeOf(api.StorageVolumeInfo) or slot >= core.max_mounts) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    if (generation != access.topologyGeneration()) return api.storage_error_stale;
    const ref = access.mountReference(slot) orelse return api.storage_result_absent;
    const value = access.mountSnapshot(ref) catch |err| return code(err);
    const record = &records[value.region.device.slot];
    if (!record.ready) return api.storage_error_stale;
    const volume = if (slot == 26) vfs.bootVolume() else vfs.volumeForDrive(@intCast('A' + slot));
    const mounted = volume orelse return api.storage_error_stale;
    const d = if (slot == 26) null else drive.get(@intCast('A' + slot));
    out.* = .{ .reference = .{ .slot = slot, .generation = ref.generation }, .target = targetForRegion(record, value.region), .letter = if (slot == 26) 0 else 'A' + slot, .filesystem = switch (mounted) {
        .fat32 => api.storage_filesystem_fat32,
        .ntfs => api.storage_filesystem_ntfs,
    }, .role = if (d) |item| @intFromEnum(item.role) else 0, .flags = (if (value.runtime_required) @as(u32, api.storage_volume_required) else 0) |
        (if (access.regionClaimed(value.region)) @as(u32, api.storage_volume_claimed) else 0) };
    return api.storage_result_present;
}

fn rememberMounts(operation: *Operation) Error!void {
    const record = &records[operation.region.device.slot];
    for (0..core.max_mounts) |i| {
        const ref = access.mountReference(@intCast(i)) orelse continue;
        const mounted = try access.mountSnapshot(ref);
        if (!mounted.region.overlaps(operation.region)) continue;
        if (!operation.region.contains(mounted.region)) return error.Invalid;
        if (mounted.runtime_required or i == 26) return error.Protected;
        const d = drive.get(@intCast('A' + i)) orelse return error.Stale;
        var part: ?table.Partition = null;
        for (record.layout.items()) |candidate| if (candidate.first_lba == mounted.region.first and candidate.sector_count >= mounted.region.count) {
            part = candidate;
            break;
        };
        var saved = SavedMount{ .slot = @intCast(i), .reference = ref, .region = mounted.region, .partition = part orelse return error.Incomplete, .kind = d.kind, .role = d.role, .bytes = d.bytes, .name = .{0} ** 32 };
        copyText(&saved.name, d.name);
        operation.saved[operation.saved_count] = saved;
        operation.saved_count += 1;
    }
}

fn forget(region: access.Region) void {
    fat.forgetStorage(region.device.slot, region.first, region.count);
    ntfs.forgetStorage(region.device.slot, region.first, region.count);
}

fn prepare(target: *const api.StorageTarget) Error!*Operation {
    const region = try validate(target, true);
    const owner = access.currentOwner() orelse return error.Invalid;
    var operation: *Operation = blk: {
        for (&operations) |*candidate| if (candidate.id == 0) break :blk candidate;
        return error.Capacity;
    };
    operation.* = .{ .owner = owner, .region = region };
    try rememberMounts(operation);
    const id = try access.prepareClaim(region);
    operation.id = id;
    var accepted = false;
    defer if (!accepted) {
        access.finishClaim(id, owner) catch {};
        access.releaseClaim(id, owner) catch {};
        operation.id = 0;
    };
    if (!cache.flushInvalidateRange(region.device.slot, region.first, region.count)) {
        records[region.device.slot].last_error = api.storage_error_io;
        return error.Io;
    }
    var detached: usize = 0;
    for (operation.saved[0..operation.saved_count]) |saved| {
        if (!vfs.unmount(saved.reference)) {
            // Admission was closed before the snapshot; only exhausted
            // generation capacity can fail here. Restore what was detached.
            for (operation.saved[0..detached]) |old| restore(old) catch {};
            return error.Busy;
        }
        detached += 1;
    }
    forget(region);
    try access.activateClaim(id, owner);
    accepted = true;
    return operation;
}

fn findOperation(id: u64, owner: access.Owner) Error!*Operation {
    for (&operations) |*operation| if (operation.id == id and id != 0) {
        if (!operation.owner.eql(owner)) return error.WrongOwner;
        return operation;
    };
    return error.Stale;
}

pub fn claimBegin(target: *const api.StorageTarget, out: *u64) callconv(.c) i32 {
    out.* = 0;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    if (!cleanup_started) {
        if (task.createKernelThreadCriticalWithRole("storage-cleanup", cleanupMain, .batch) == null) return api.storage_error_capacity;
        cleanup_started = true;
    }
    const operation = prepare(target) catch |err| return code(err);
    out.* = operation.id;
    return api.storage_result_ok;
}

fn probe(record: *Record, index: usize) Error!vfs.Volume {
    const part = record.layout.partitions[index];
    if (part.first_lba > std.math.maxInt(u32)) return error.Unsupported;
    return switch (record.hints[index]) {
        api.storage_filesystem_fat32 => .{ .fat32 = fat.parseBounded(record.identity.reference.slot, @intCast(part.first_lba), part.sector_count) orelse return error.Remount },
        api.storage_filesystem_ntfs => .{ .ntfs = ntfs.inspectBounded(record.identity.reference.slot, @intCast(part.first_lba), part.sector_count) orelse return error.Remount },
        else => error.Unsupported,
    };
}

fn publishMount(record: *Record, index: usize, letter: u8, role: drive.Role, name: []const u8) Error!access.MountRef {
    if (drive.get(letter) != null or vfs.volumeForDrive(letter) != null) return error.Busy;
    const volume = try probe(record, index);
    const kind: drive.Kind = switch (volume) {
        .fat32 => .fat32,
        .ntfs => .ntfs,
    };
    const part = record.layout.partitions[index];
    if (!drive.mountBlockRole(letter, kind, role, name, @intCast(part.sector_count * 512), record.identity.reference.slot)) return error.Capacity;
    if (!vfs.mountForDrive(letter, volume)) {
        drive.unmountLocked(letter);
        return error.Capacity;
    }
    record.errors[index] = 0;
    return vfs.volumeForDrive(letter).?.accessReference().?;
}

fn restore(saved: SavedMount) Error!void {
    const record = &records[saved.region.device.slot];
    if (!record.layout.valid) return error.Remount;
    for (record.layout.items(), 0..) |part, i| {
        if (part.number != saved.partition.number or part.first_lba != saved.partition.first_lba or
            part.sector_count != saved.partition.sector_count or !table.guid.eql(part.unique_guid, saved.partition.unique_guid)) continue;
        _ = publishMount(record, i, @intCast('A' + saved.slot), saved.role, span(&record.identity.name)) catch |err| {
            record.errors[i] = code(err);
            return err;
        };
        return;
    }
    return error.Remount;
}

fn finish(operation: *Operation, keep_unmounted: bool) Error!void {
    try access.finishClaim(operation.id, operation.owner);
    // Every path below consumes the claim after recording the final outcome.
    defer {
        access.releaseClaim(operation.id, operation.owner) catch {};
        operation.id = 0;
    }
    const region = operation.region;
    const record = &records[region.device.slot];
    if (!cache.flushInvalidateRange(region.device.slot, region.first, region.count)) {
        record.last_error = api.storage_error_io;
        return error.Io;
    }
    const identity = block.identity(region.device.slot) orelse {
        record.last_error = api.storage_error_io;
        return error.Io;
    };
    scanRecord(record, identity, region) catch |err| {
        record.last_error = code(err);
        return error.Remount;
    };
    var failed = false;
    if (!keep_unmounted) for (operation.saved[0..operation.saved_count]) |saved| {
        restore(saved) catch {
            failed = true;
        };
    };
    record.last_error = if (failed) api.storage_error_remount else 0;
    if (failed) return error.Remount;
}

pub fn claimEnd(id: u64, flags: u32) callconv(.c) i32 {
    if (flags & ~@as(u32, api.storage_claim_end_keep_unmounted) != 0) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    const owner = access.currentOwner() orelse return api.storage_error_invalid;
    const operation = findOperation(id, owner) catch |err| return code(err);
    finish(operation, flags != 0) catch |err| return code(err);
    return api.storage_result_ok;
}

fn rawRegion(region: access.Region, relative: u64, sectors: u32, len: u32) Error!access.Region {
    if (sectors == 0 or sectors > api.storage_raw_max_sectors or len < sectors * 512 or
        relative >= region.count or sectors > region.count - relative) return error.Invalid;
    return .{ .device = region.device, .first = region.first + relative, .count = sectors };
}

pub fn read(target: *const api.StorageTarget, relative: u64, sectors: u32, out: [*]u8, len: u32) callconv(.c) i32 {
    const unwind = context.enterUnwind();
    if (!unwind.admitted()) return api.storage_error_capacity;
    defer _ = context.leaveUnwind(unwind);
    if (!enter()) return api.storage_error_busy;
    const scope = validate(target, false) catch |err| {
        _ = gate.leave();
        return code(err);
    };
    const region = rawRegion(scope, relative, sectors, len) catch |err| {
        _ = gate.leave();
        return code(err);
    };
    const use = access.beginRawRead(region) catch |err| {
        _ = gate.leave();
        return code(err);
    };
    _ = gate.leave();
    defer access.endUse(use) catch {};
    return if (block.read(region.device.slot, region.first, @intCast(sectors), out[0 .. sectors * 512])) api.storage_result_ok else api.storage_error_io;
}

fn claimRaw(id: u64, relative: u64, sectors: u32, data: [*]u8, len: u32, write: bool) i32 {
    const unwind = context.enterUnwind();
    if (!unwind.admitted()) return api.storage_error_capacity;
    defer _ = context.leaveUnwind(unwind);
    const owner = access.currentOwner() orelse return api.storage_error_invalid;
    const claim = access.claimSnapshot(id, owner) catch |err| return code(err);
    const region = rawRegion(claim.region, relative, sectors, len) catch |err| return code(err);
    const use = access.beginClaimIo(id, region) catch |err| return code(err);
    defer access.endUse(use) catch {};
    const bytes = data[0 .. sectors * 512];
    const ok = if (write) block.write(region.device.slot, region.first, @intCast(sectors), bytes) else block.read(region.device.slot, region.first, @intCast(sectors), bytes);
    return if (ok) api.storage_result_ok else api.storage_error_io;
}
pub fn claimRead(id: u64, relative: u64, sectors: u32, data: [*]u8, len: u32) callconv(.c) i32 {
    return claimRaw(id, relative, sectors, data, len, false);
}
pub fn claimWrite(id: u64, relative: u64, sectors: u32, data: [*]const u8, len: u32) callconv(.c) i32 {
    return claimRaw(id, relative, sectors, @constCast(data), len, true);
}
pub fn claimFlush(id: u64) callconv(.c) i32 {
    const unwind = context.enterUnwind();
    if (!unwind.admitted()) return api.storage_error_capacity;
    defer _ = context.leaveUnwind(unwind);
    const owner = access.currentOwner() orelse return api.storage_error_invalid;
    const claim = access.claimSnapshot(id, owner) catch |err| return code(err);
    const use = access.beginClaimIo(id, claim.region) catch |err| return code(err);
    defer access.endUse(use) catch {};
    return if (block.flush(claim.region.device.slot)) api.storage_result_ok else api.storage_error_io;
}

pub fn rescan(device: *const api.StorageDeviceRef) callconv(.c) i32 {
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    const ref = privateDevice(device.*) catch |err| return code(err);
    const record = &records[ref.slot];
    if (!record.ready) return api.storage_error_stale;
    const target = wholeTarget(record);
    const operation = prepare(&target) catch |err| return code(err);
    finish(operation, false) catch |err| return code(err);
    return api.storage_result_ok;
}

pub fn mount(target: *const api.StorageTarget, letter_arg: u32, out: *api.StorageVolumeRef) callconv(.c) i32 {
    out.* = .{};
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    const region = validate(target, true) catch |err| return code(err);
    if (target.kind != api.storage_target_partition) return api.storage_error_unsupported;
    var letter = letter_arg;
    if (letter == 0) {
        letter = 'D';
        while (letter <= 'Z' and (letter == 'R' or drive.get(@intCast(letter)) != null)) : (letter += 1) {}
    }
    if (letter < 'D' or letter > 'Z' or letter == 'R') return api.storage_error_protected;
    if (drive.get(@intCast(letter)) != null) return api.storage_error_busy;
    for (0..core.max_mounts) |i| {
        const ref = access.mountReference(@intCast(i)) orelse continue;
        const value = access.mountSnapshot(ref) catch continue;
        if (value.region.overlaps(region)) return api.storage_error_busy;
    }
    const operation = prepare(target) catch |err| return code(err);
    defer {
        access.finishClaim(operation.id, operation.owner) catch {};
        access.releaseClaim(operation.id, operation.owner) catch {};
        operation.id = 0;
    }
    const record = &records[region.device.slot];
    for (record.layout.items(), 0..) |part, i| if (part.number == target.partition_number) {
        const ref = publishMount(record, i, @intCast(letter), .none, span(&record.identity.name)) catch |err| {
            record.errors[i] = code(err);
            record.last_error = code(err);
            return code(err);
        };
        out.* = .{ .slot = ref.slot, .generation = ref.generation };
        return api.storage_result_ok;
    };
    return api.storage_error_stale;
}

pub fn unmount(volume: *const api.StorageVolumeRef) callconv(.c) i32 {
    if (volume.reserved != 0) return api.storage_error_invalid;
    if (!enter()) return api.storage_error_busy;
    defer _ = gate.leave();
    ensureRecords() catch |err| return code(err);
    const value = access.mountSnapshot(.{ .slot = volume.slot, .generation = volume.generation }) catch |err| return code(err);
    const record = &records[value.region.device.slot];
    if (!record.ready or !std.meta.eql(record.identity.reference, value.region.device)) return api.storage_error_stale;
    const target = targetForRegion(record, value.region);
    const operation = prepare(&target) catch |err| return code(err);
    finish(operation, true) catch |err| return code(err);
    return api.storage_result_ok;
}

pub fn useEnd(id: u64) callconv(.c) i32 {
    const owner = access.currentOwner() orelse return api.storage_error_invalid;
    access.endUse(.{ .id = id, .owner = owner }) catch |err| return code(err);
    return api.storage_result_ok;
}

// Called only after caller-bound asynchronous I/O and stream owners drained.
// Cleanup is deferred to its own worker; the program reaper never waits on
// storage I/O or a filesystem lane belonging to another process.
pub fn releaseOwner(program: u32, generation: u64, task_id: u32, task_generation: u64) bool {
    access.releaseOwnerLeases(program, generation, task_id, task_generation);
    if (!access.ownerHasClaims(program, generation, task_id, task_generation)) return true;
    if (!gate.tryEnter()) return false;
    defer _ = gate.leave();
    var complete = true;
    for (&operations) |*operation| {
        if (operation.id == 0 or operation.owner.program != program or operation.owner.program_generation != generation) continue;
        if (task_id != 0 and (operation.owner.task != task_id or operation.owner.task_generation != task_generation)) continue;
        operation.cleanup = true;
        complete = false;
    }
    return complete;
}

fn cleanupMain() callconv(.c) noreturn {
    while (true) {
        if (gate.tryEnter()) {
            for (&operations) |*operation| if (operation.id != 0 and operation.cleanup) {
                finish(operation, false) catch {};
                break;
            };
            _ = gate.leave();
        }
        scheduler.sleepTicks(100);
    }
}
