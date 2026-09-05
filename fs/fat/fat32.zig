const k = @import("../../kernel/log.zig");
const page_cache = @import("../page_cache.zig");
const scheduler = @import("../../sched/scheduler.zig");
const timer = @import("../../kernel/timer.zig");
const time_core = @import("../../platform/time.zig");
const block = @import("../../storage/block.zig");

const SECTOR_SIZE: usize = 512;
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
const ATTR_LONG_NAME: u8 = 0x0F;
const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
const FAT_RESERVED_START: u32 = 0x0FFF_FFF0;
const EOC: u32 = 0x0FFF_FFF8;
const EOC_MARK: u32 = 0x0FFF_FFFF;
// Windows-parity name limits (0.60.19): FAT LFN carries at most 255 UTF-16
// units (20 LFN entries); the UTF-8 (BMP) worst case is 765 bytes, buffered
// as 768.  NAME_MAX stays the BYTE limit for UTF-8 name buffers; unit
// buffers use NAME_UNITS_MAX.
const NAME_UNITS_MAX: usize = 255;
const NAME_MAX: usize = 768;
const MAX_LFN_ENTRIES: usize = (NAME_UNITS_MAX + 12) / 13;
const MAX_DIR_ENTRIES_PER_NAME: usize = MAX_LFN_ENTRIES + 1;
const COOPERATE_STEP_INTERVAL: u32 = 16;
const READ_RANGE_EXTENT_CACHE_SLOTS: usize = 2048;
const READ_RANGE_EXTENT_PREFETCH_CLUSTERS: usize = 256;
const MAX_TRACKED_VOLUME_CLUSTERS: usize = 262144;
const VOLUME_INUSE_MAP_BYTES: usize = (MAX_TRACKED_VOLUME_CLUSTERS + 7) / 8;
const FSINFO_LEAD_SIGNATURE: u32 = 0x4161_5252;
const FSINFO_STRUCT_SIGNATURE: u32 = 0x6141_7272;
const FSINFO_TRAIL_SIGNATURE: u32 = 0xAA55_0000;
const FSINFO_UNKNOWN: u32 = 0xFFFF_FFFF;

pub const Volume = struct {
    device_index: usize,
    partition_lba: u32,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_count: u8,
    sectors_per_fat: u32,
    total_sectors: u32,
    root_cluster: u32,
    fs_info_sector: u16,
    backup_boot_sector: u16,
    // Non-zero only in a regular mutation. Recovery/ownership operations
    // deliberately retain zero and therefore keep their full-device flush
    // boundary for lost-completion reconciliation.
    write_batch: page_cache.WriteBatch = page_cache.NO_WRITE_BATCH,

    fn firstDataSector(self: Volume) u32 {
        return self.partition_lba + self.reserved_sectors + @as(u32, self.fat_count) * self.sectors_per_fat;
    }

    fn clusterLba(self: Volume, cluster: u32) u32 {
        return self.firstDataSector() + (cluster - 2) * self.sectors_per_cluster;
    }

    fn fatLba(self: Volume, cluster: u32) u32 {
        return self.partition_lba + self.reserved_sectors + (cluster * 4) / self.bytes_per_sector;
    }

    fn fatOffset(self: Volume, cluster: u32) usize {
        return @intCast((cluster * 4) % self.bytes_per_sector);
    }

    pub fn clusterBytes(self: Volume) u32 {
        return @as(u32, self.bytes_per_sector) * @as(u32, self.sectors_per_cluster);
    }

    pub fn totalClusters(self: Volume) u32 {
        const metadata_sectors = @as(u32, self.reserved_sectors) + @as(u32, self.fat_count) * self.sectors_per_fat;
        if (self.total_sectors <= metadata_sectors or self.sectors_per_cluster == 0) return 0;
        return (self.total_sectors - metadata_sectors) / self.sectors_per_cluster;
    }
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    first_cluster: u32 = 0,
    size: u32 = 0,
    created_time: u16 = 0,
    created_date: u16 = 0,
    access_date: u16 = 0,
    modified_time: u16 = 0,
    modified_date: u16 = 0,

    pub fn isDir(self: Entry) bool {
        return (self.attr & ATTR_DIRECTORY) != 0;
    }

    pub fn isReadOnly(self: Entry) bool {
        return (self.attr & ATTR_READ_ONLY) != 0;
    }

    pub fn isHiddenOrSystem(self: Entry) bool {
        return (self.attr & (ATTR_HIDDEN | ATTR_SYSTEM)) != 0;
    }
};

pub const AppendStatus = enum(u8) {
    ok,
    invalid,
    not_found,
    offset_mismatch,
    too_large,
    io,
};

pub const LookupStatus = enum(u8) {
    found,
    not_found,
    io,
};

pub const RenameStatus = enum(u8) {
    ok,
    not_found,
    not_atomic,
    conflict,
    read_only,
    io,
};

/// Result of the update-only same-directory ownership transfer.  This
/// primitive never copies payload bytes and never frees a cluster chain.  A
/// caller-owned durable journal decides whether the retained backup is later
/// kept, restored, or deleted.
pub const AtomicReplaceResult = enum(i32) {
    ok = 0,
    invalid = -1,
    not_found = -2,
    alias = -3,
    conflict = -4,
    read_only = -5,
    io = -6,
    not_atomic = -7,
};

pub const DeleteIfIdentityResult = enum(u8) {
    deleted,
    not_found,
    mismatch,
    io,
};

const EntryLocation = struct {
    lba: u32,
    offset: usize,
    entry: Entry,
    lfn_slots: [MAX_LFN_ENTRIES]DirectorySlot = undefined,
    lfn_slot_count: usize = 0,
};

const AppendCache = struct {
    valid: bool = false,
    device_index: usize = 0,
    partition_lba: u32 = 0,
    parent_cluster: u32 = 0,
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    lba: u32 = 0,
    offset: usize = 0,
    attr: u8 = 0,
    first_cluster: u32 = 0,
    size: u32 = 0,
    last_cluster: u32 = 0,
};

const ReadRangeExtent = struct {
    valid: bool = false,
    device_index: usize = 0,
    partition_lba: u32 = 0,
    first_cluster: u32 = 0,
    size: u32 = 0,
    cluster_index: usize = 0,
    cluster: u32 = 0,
    count: usize = 0,
    last_used: u64 = 0,
};

const ReadRangeCursor = struct {
    cluster_index: usize = 0,
    cluster: u32 = 0,
    contiguous_remaining: usize = 1,
};

const VolumeAllocationHint = struct {
    valid: bool = false,
    device_index: usize = 0,
    partition_lba: u32 = 0,
    next_free_cluster: u32 = 3,
};

const FsInfoSnapshot = struct {
    valid: bool = false,
    free_count: u32 = FSINFO_UNKNOWN,
    next_free: u32 = FSINFO_UNKNOWN,
};

const VolumeRuntimeState = struct {
    valid: bool = false,
    device_index: usize = 0,
    partition_lba: u32 = 0,
    total_clusters: u32 = 0,
    fs_info_sector: u16 = 0,
    backup_boot_sector: u16 = 0,
    fsinfo_present: bool = false,
    fsinfo_valid: bool = false,
    inuse_map_ready: bool = false,
    // 0.56.10: FAT-Vollscan bei gueltigem FSINFO auf die erste
    // Cluster-Allokation verschoben (Mount blockiert nicht mehr).
    scan_pending: bool = false,
    map_capable: bool = false,
    free_clusters: u32 = 0,
    next_free_cluster: u32 = 3,
    inuse_map: [VOLUME_INUSE_MAP_BYTES]u8 = .{0} ** VOLUME_INUSE_MAP_BYTES,
};

const ChainAllocation = struct {
    first: u32 = 0,
    last: u32 = 0,
};

const ClusterRun = struct {
    first: u32 = 0,
    count: usize = 0,
};

const AppendCommitMode = enum {
    flush,
    deferred,
};

const MAX_MOUNTED_VOLUMES: usize = 26;

var append_cache: AppendCache = .{};
var read_range_extents: [READ_RANGE_EXTENT_CACHE_SLOTS]ReadRangeExtent = .{ReadRangeExtent{}} ** READ_RANGE_EXTENT_CACHE_SLOTS;
var read_range_cache_clock: u64 = 0;
var allocation_hints: [MAX_MOUNTED_VOLUMES]VolumeAllocationHint = .{VolumeAllocationHint{}} ** MAX_MOUNTED_VOLUMES;
var volume_states: [MAX_MOUNTED_VOLUMES]VolumeRuntimeState = .{VolumeRuntimeState{}} ** MAX_MOUNTED_VOLUMES;

pub const Operation = enum(u32) {
    none = 0,
    resolve_entry = 1,
    resolve_path = 2,
    read_file = 3,
    read_range = 4,
    write_range = 5,
    write_file = 6,
    append_file = 7,
    copy_file = 8,
    make_directory = 9,
    delete_file = 10,
    remove_directory = 11,
    rename_entry = 12,
    set_attributes = 13,
};

pub const Summary = struct {
    read_sectors: u64 = 0,
    write_sectors: u64 = 0,
    read_failures: u64 = 0,
    write_failures: u64 = 0,
    flushes: u64 = 0,
    flush_failures: u64 = 0,
    flush_total_ticks: u64 = 0,
    flush_max_ticks: u64 = 0,
    flush_last_ticks: u64 = 0,
    file_writes: u64 = 0,
    file_appends: u64 = 0,
    file_write_ranges: u64 = 0,
    file_write_bytes: u64 = 0,
    file_append_bytes: u64 = 0,
    dir_scans: u64 = 0,
    dir_entries_scanned: u64 = 0,
    dir_entry_updates: u64 = 0,
    cluster_walk_steps: u64 = 0,
    fat_reads: u64 = 0,
    fat_writes: u64 = 0,
    fat_mirror_writes: u64 = 0,
    alloc_chain_calls: u64 = 0,
    alloc_clusters: u64 = 0,
    alloc_search_steps: u64 = 0,
    alloc_runs: u64 = 0,
    alloc_run_clusters: u64 = 0,
    alloc_run_max_clusters: u64 = 0,
    fat_sector_writes: u64 = 0,
    read_extent_cache_hits: u64 = 0,
    read_extent_cache_misses: u64 = 0,
    read_extent_cache_stores: u64 = 0,
    read_extent_cache_clusters: u64 = 0,
    fsinfo_reads: u64 = 0,
    fsinfo_valid_mounts: u64 = 0,
    fsinfo_rebuilds: u64 = 0,
    fsinfo_writes: u64 = 0,
    inusemap_builds: u64 = 0,
    inusemap_deferred_builds: u64 = 0,
    inusemap_clusters: u64 = 0,
    inusemap_alloc_hits: u64 = 0,
    inusemap_alloc_misses: u64 = 0,
    operation_failures: u64 = 0,
    operation_total_ticks: u64 = 0,
    operation_max_ticks: u64 = 0,
    operation_last_ticks: u64 = 0,
    active_operation: u32 = @intFromEnum(Operation.none),
    last_operation: u32 = @intFromEnum(Operation.none),
    yield_points: u64 = 0,
    yields: u64 = 0,
    yield_skips: u64 = 0,
};

var stats: Summary = .{};

pub fn summary() Summary {
    return stats;
}

fn readSector(device_index: usize, lba: u64, sectors: u16, out: []u8) bool {
    if (sectors == 0) return false;
    if (sectors == 1) {
        if (page_cache.readSector(device_index, lba, out)) {
            stats.read_sectors +%= 1;
            return true;
        }
        stats.read_failures +%= 1;
        return false;
    }
    // 0.56.10: Bulk-Pfad - ein Cache-Zugriff pro Seite statt pro Sektor.
    if (page_cache.readSectors(device_index, lba, sectors, out)) {
        stats.read_sectors +%= @intCast(sectors);
        return true;
    }
    stats.read_failures +%= 1;
    return false;
}

fn writeSector(volume: Volume, lba: u64, sectors: u16, data: []const u8) bool {
    if (sectors == 0) return false;
    if (sectors == 1 and page_cache.writeSectorInBatch(volume.device_index, lba, data, volume.write_batch)) {
        stats.write_sectors +%= 1;
        return true;
    }
    if (sectors > 1 and page_cache.writeSectorsDirect(volume.device_index, lba, sectors, data)) {
        stats.write_sectors +%= @intCast(sectors);
        return true;
    }
    stats.write_failures +%= 1;
    return false;
}

fn beginMutation(volume: Volume) ?Volume {
    var scoped = volume;
    scoped.write_batch = page_cache.beginWriteBatch() orelse return null;
    return scoped;
}

fn flushDevice(device_index: usize) bool {
    const start_tick = timer.tickCount();
    const ok = page_cache.flushDevice(device_index);
    const elapsed = elapsedTicks(start_tick);
    stats.flushes +%= 1;
    if (!ok) stats.flush_failures +%= 1;
    stats.flush_last_ticks = elapsed;
    stats.flush_total_ticks +%= elapsed;
    if (elapsed > stats.flush_max_ticks) stats.flush_max_ticks = elapsed;
    return ok;
}

fn flushMutation(volume: Volume) bool {
    const start_tick = timer.tickCount();
    const ok = page_cache.flushDeviceBatch(volume.device_index, volume.write_batch);
    const elapsed = elapsedTicks(start_tick);
    stats.flushes +%= 1;
    if (!ok) stats.flush_failures +%= 1;
    stats.flush_last_ticks = elapsed;
    stats.flush_total_ticks +%= elapsed;
    if (elapsed > stats.flush_max_ticks) stats.flush_max_ticks = elapsed;
    return ok;
}

fn beginOperation(operation: Operation) u64 {
    stats.active_operation = @intFromEnum(operation);
    return timer.tickCount();
}

fn finishOperation(operation: Operation, start_tick: u64, ok: bool) void {
    const elapsed = elapsedTicks(start_tick);
    stats.operation_last_ticks = elapsed;
    stats.operation_total_ticks +%= elapsed;
    if (elapsed > stats.operation_max_ticks) stats.operation_max_ticks = elapsed;
    stats.last_operation = @intFromEnum(operation);
    stats.active_operation = @intFromEnum(Operation.none);
    if (!ok) stats.operation_failures +%= 1;
}

fn elapsedTicks(start_tick: u64) u64 {
    const now = timer.tickCount();
    return if (now >= start_tick) now - start_tick else 0;
}

fn cooperate(step_counter: *u32) void {
    step_counter.* +%= 1;
    if (step_counter.* < COOPERATE_STEP_INTERVAL) return;
    step_counter.* = 0;

    stats.yield_points +%= 1;
    const before = scheduler.stats().yields;
    scheduler.yield();
    const after = scheduler.stats().yields;
    if (after != before) {
        stats.yields +%= 1;
    } else {
        stats.yield_skips +%= 1;
    }
}

pub fn inspect(device_index: usize, first_lba: u32) ?Volume {
    const volume = parse(device_index, first_lba) orelse return null;
    printInfo(volume);
    return volume;
}

pub fn parse(device_index: usize, first_lba: u32) ?Volume {
    const device = block.get(device_index) orelse return null;
    if (first_lba >= device.sector_count) return null;
    return parseBounded(device_index, first_lba, device.sector_count - first_lba);
}

/// Admit a filesystem only within the selected partition. FAT32's current
/// address calculations are u32; reject incompatible geometry before use.
pub fn parseBounded(device_index: usize, first_lba: u32, partition_sectors: u64) ?Volume {
    const device = block.get(device_index) orelse return null;
    if (device.sector_size != SECTOR_SIZE or first_lba >= device.sector_count or partition_sectors > device.sector_count - first_lba) return null;
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(device_index, first_lba, 1, sector[0..])) {
        k.puts("      FAT BPB: read failed\r\n");
        return null;
    }

    if (sector[510] != 0x55 or sector[511] != 0xAA) {
        k.puts("      FAT BPB: invalid signature\r\n");
        return null;
    }

    const bytes_per_sector = readLe16(sector[11..13]);
    const sectors_per_cluster = sector[13];
    const reserved_sectors = readLe16(sector[14..16]);
    const fat_count = sector[16];
    const root_entry_count = readLe16(sector[17..19]);
    const fat_sectors_16 = readLe16(sector[22..24]);
    const total_sectors_16 = readLe16(sector[19..21]);
    const total_sectors_32 = readLe32(sector[32..36]);
    const fat_sectors_32 = readLe32(sector[36..40]);
    const root_cluster = readLe32(sector[44..48]);
    const fs_info_sector = readLe16(sector[48..50]);
    const backup_boot_sector = readLe16(sector[50..52]);
    const total_sectors = if (total_sectors_16 != 0) @as(u32, total_sectors_16) else total_sectors_32;

    if (bytes_per_sector != SECTOR_SIZE) {
        k.puts("      FAT32: unsupported sector size\r\n");
        return null;
    }
    if (sectors_per_cluster == 0 or fat_count == 0 or fat_sectors_32 == 0 or total_sectors == 0 or root_entry_count != 0 or fat_sectors_16 != 0) {
        k.puts("      FAT32: unsupported BPB layout\r\n");
        return null;
    }

    const volume: Volume = .{
        .device_index = device_index,
        .partition_lba = first_lba,
        .bytes_per_sector = bytes_per_sector,
        .sectors_per_cluster = sectors_per_cluster,
        .reserved_sectors = reserved_sectors,
        .fat_count = fat_count,
        .sectors_per_fat = fat_sectors_32,
        .total_sectors = total_sectors,
        .root_cluster = root_cluster,
        .fs_info_sector = fs_info_sector,
        .backup_boot_sector = backup_boot_sector,
    };
    const metadata = @as(u64, reserved_sectors) + @as(u64, fat_count) * fat_sectors_32;
    if (sectors_per_cluster > 128 or (sectors_per_cluster & (sectors_per_cluster - 1)) != 0 or
        reserved_sectors == 0 or fat_count > 2 or metadata >= total_sectors or
        total_sectors > partition_sectors or @as(u64, first_lba) + total_sectors > 0x1_0000_0000) return null;
    const clusters = volume.totalClusters();
    if (clusters < 65525 or clusters >= FAT_RESERVED_START - 2 or root_cluster < 2 or root_cluster >= clusters + 2 or
        (@as(u64, clusters) + 2) * 4 > @as(u64, fat_sectors_32) * SECTOR_SIZE) return null;
    if (runtimeStateFor(volume, true) == null) return null;
    initializeVolumeState(volume);
    return volume;
}

pub fn freeClusterCount(volume: Volume) ?u32 {
    if (runtimeStateFor(volume, false)) |state| return state.free_clusters;
    initializeVolumeState(volume);
    return if (runtimeStateFor(volume, false)) |state| state.free_clusters else null;
}

fn initializeVolumeState(volume: Volume) void {
    const total_clusters = volume.totalClusters();
    if (total_clusters == 0) return;
    const state = runtimeStateFor(volume, true) orelse return;
    const fsinfo = readFsInfo(volume);

    const mount_start = timer.tickCount();
    const map_capable = total_clusters <= MAX_TRACKED_VOLUME_CLUSTERS;
    state.* = .{
        .valid = true,
        .device_index = volume.device_index,
        .partition_lba = volume.partition_lba,
        .total_clusters = total_clusters,
        .fs_info_sector = volume.fs_info_sector,
        .backup_boot_sector = volume.backup_boot_sector,
        .fsinfo_present = fsInfoSectorValid(volume),
        .fsinfo_valid = fsinfo.valid,
        .inuse_map_ready = false,
        .map_capable = map_capable,
        .free_clusters = 0,
        .next_free_cluster = 3,
    };

    // 0.56.10: Lazy-Scan-Versuch REVERTIERT. Der auf die erste Allokation
    // verschobene FAT-Vollscan lief dort unter dem fs_volume-Lock und
    // riss das MEMSUITE-STRICT-Kriterium sleep_under_lock==0
    // (pagerstress: lock sleep=21). Der Mount-Scan laeuft in der
    // Boot-Phase ohne Lock-Tracking und ist seit Page-Cache v2 ohnehin
    // 8x billiger (Seiten-Fills); der Marker unten macht die Dauer
    // dauerhaft messbar.
    state.inuse_map_ready = map_capable;
    if (!scanFatIntoRuntimeState(volume, state)) {
        state.valid = false;
        return;
    }
    finishScanCrossCheck(volume, state, fsinfo);
    logMountMarker(volume, total_clusters, false, timer.tickCount() -% mount_start);
}

fn finishScanCrossCheck(volume: Volume, state: *VolumeRuntimeState, fsinfo: FsInfoSnapshot) void {
    var needs_rebuild = state.fsinfo_present and !fsinfo.valid;
    if (fsinfo.valid) {
        if (fsinfo.free_count != FSINFO_UNKNOWN and fsinfo.free_count != state.free_clusters) needs_rebuild = true;
        if (validNextFreeCluster(state, fsinfo.next_free)) {
            state.next_free_cluster = fsinfo.next_free;
        } else if (fsinfo.next_free != FSINFO_UNKNOWN) {
            needs_rebuild = true;
        }
        if (!needs_rebuild) stats.fsinfo_valid_mounts +%= 1;
    } else if (state.fsinfo_present) {
        needs_rebuild = true;
    }

    if (needs_rebuild) {
        stats.fsinfo_rebuilds +%= 1;
        _ = writeFsInfoState(volume, state);
    }
    seedAllocationHint(volume, state.next_free_cluster);
}

// 0.56.10: Verschobenen FAT-Scan vor der ersten Allokation nachholen.
fn ensureFatScanned(volume: Volume) void {
    const state = runtimeStateFor(volume, false) orelse return;
    if (!state.scan_pending) return;
    state.scan_pending = false;
    state.inuse_map_ready = state.map_capable;
    const fsinfo = FsInfoSnapshot{
        .valid = state.fsinfo_valid,
        .free_count = state.free_clusters,
        .next_free = state.next_free_cluster,
    };
    state.free_clusters = 0;
    state.next_free_cluster = 3;
    if (!scanFatIntoRuntimeState(volume, state)) {
        // Scan fehlgeschlagen: FSINFO-Werte zuruecksetzen und ohne Map
        // weiterarbeiten (FAT-Fallback der Allokation).
        state.inuse_map_ready = false;
        state.free_clusters = fsinfo.free_count;
        state.next_free_cluster = fsinfo.next_free;
        return;
    }
    stats.inusemap_deferred_builds +%= 1;
    finishScanCrossCheck(volume, state, fsinfo);
}

fn logMountMarker(volume: Volume, total_clusters: u32, deferred: bool, ticks: u64) void {
    k.puts("FAT32 mount dev=");
    k.putDec(@intCast(volume.device_index));
    k.puts(" clusters=");
    k.putDec(total_clusters);
    k.puts(" scan=");
    k.puts(if (deferred) "deferred" else "full");
    k.puts(" ticks=");
    k.putDec(ticks);
    k.puts("\r\n");
}

fn scanFatIntoRuntimeState(volume: Volume, state: *VolumeRuntimeState) bool {
    if (state.inuse_map_ready) {
        @memset(state.inuse_map[0..inuseMapBytes(state.total_clusters)], 0);
    }

    const end = state.total_clusters + 2;
    var cluster: u32 = 2;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var found_next = false;
    var coop_steps: u32 = 0;
    while (cluster < end) {
        if (!readSector(volume.device_index, volume.fatLba(cluster), 1, sector[0..])) return false;
        var offset = volume.fatOffset(cluster);
        while (cluster < end and offset + 4 <= SECTOR_SIZE) : ({
            cluster += 1;
            offset += 4;
        }) {
            stats.fat_reads +%= 1;
            const used = (readLe32(sector[offset..][0..4]) & 0x0FFF_FFFF) != 0;
            if (used) {
                markInuseBit(state, cluster);
            } else {
                state.free_clusters +%= 1;
                if (!found_next) {
                    state.next_free_cluster = cluster;
                    found_next = true;
                }
            }
            cooperate(&coop_steps);
        }
    }
    if (!found_next) state.next_free_cluster = end;
    if (state.inuse_map_ready) {
        stats.inusemap_builds +%= 1;
        stats.inusemap_clusters +%= state.total_clusters;
    }
    return true;
}

fn runtimeStateFor(volume: Volume, create: bool) ?*VolumeRuntimeState {
    var fallback: ?*VolumeRuntimeState = null;
    for (&volume_states) |*state| {
        if (state.valid and state.device_index == volume.device_index and state.partition_lba == volume.partition_lba) return state;
        if (!state.valid and fallback == null) fallback = state;
    }
    if (!create) return null;
    return fallback;
}

fn fsInfoSectorValid(volume: Volume) bool {
    return volume.fs_info_sector > 0 and volume.fs_info_sector < volume.reserved_sectors;
}

fn backupFsInfoSectorValid(volume: Volume) bool {
    return fsInfoSectorValid(volume) and
        volume.backup_boot_sector > 0 and
        @as(u32, volume.backup_boot_sector) + @as(u32, volume.fs_info_sector) < volume.reserved_sectors;
}

fn fsInfoLba(volume: Volume) u64 {
    return @as(u64, volume.partition_lba) + volume.fs_info_sector;
}

fn backupFsInfoLba(volume: Volume) u64 {
    return @as(u64, volume.partition_lba) + @as(u32, volume.backup_boot_sector) + volume.fs_info_sector;
}

fn readFsInfo(volume: Volume) FsInfoSnapshot {
    if (!fsInfoSectorValid(volume)) return .{};
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, fsInfoLba(volume), 1, sector[0..])) return .{};
    stats.fsinfo_reads +%= 1;
    if (readLe32(sector[0..4]) != FSINFO_LEAD_SIGNATURE or
        readLe32(sector[484..488]) != FSINFO_STRUCT_SIGNATURE or
        readLe32(sector[508..512]) != FSINFO_TRAIL_SIGNATURE)
    {
        return .{};
    }
    return .{
        .valid = true,
        .free_count = readLe32(sector[488..492]),
        .next_free = readLe32(sector[492..496]),
    };
}

fn writeFsInfoState(volume: Volume, state: *const VolumeRuntimeState) bool {
    if (!fsInfoSectorValid(volume)) return true;
    var sector: [SECTOR_SIZE]u8 = .{0} ** SECTOR_SIZE;
    writeLe32(sector[0..4], FSINFO_LEAD_SIGNATURE);
    writeLe32(sector[484..488], FSINFO_STRUCT_SIGNATURE);
    writeLe32(sector[488..492], state.free_clusters);
    writeLe32(sector[492..496], if (state.next_free_cluster >= 2 and state.next_free_cluster < allocClusterEnd(volume)) state.next_free_cluster else FSINFO_UNKNOWN);
    writeLe32(sector[508..512], FSINFO_TRAIL_SIGNATURE);
    if (!writeSector(volume, fsInfoLba(volume), 1, sector[0..])) return false;
    if (backupFsInfoSectorValid(volume) and backupFsInfoLba(volume) != fsInfoLba(volume)) {
        if (!writeSector(volume, backupFsInfoLba(volume), 1, sector[0..])) return false;
    }
    stats.fsinfo_writes +%= 1;
    return true;
}

fn validNextFreeCluster(state: *const VolumeRuntimeState, cluster: u32) bool {
    if (cluster == FSINFO_UNKNOWN) return false;
    if (cluster < 2 or cluster >= state.total_clusters + 2) return false;
    return stateClusterFree(state, cluster);
}

fn inuseMapBytes(total_clusters: u32) usize {
    const clusters: usize = @intCast(total_clusters);
    return (clusters + 7) / 8;
}

fn inuseMapIndex(cluster: u32) ?usize {
    if (cluster < 2) return null;
    const index: usize = @intCast(cluster - 2);
    if (index >= MAX_TRACKED_VOLUME_CLUSTERS) return null;
    return index;
}

fn markInuseBit(state: *VolumeRuntimeState, cluster: u32) void {
    if (!state.inuse_map_ready) return;
    const index = inuseMapIndex(cluster) orelse return;
    state.inuse_map[index / 8] |= @as(u8, 1) << @as(u3, @intCast(index % 8));
}

fn clearInuseBit(state: *VolumeRuntimeState, cluster: u32) void {
    if (!state.inuse_map_ready) return;
    const index = inuseMapIndex(cluster) orelse return;
    state.inuse_map[index / 8] &= ~(@as(u8, 1) << @as(u3, @intCast(index % 8)));
}

fn stateClusterFree(state: *const VolumeRuntimeState, cluster: u32) bool {
    if (!state.inuse_map_ready) return false;
    const index = inuseMapIndex(cluster) orelse return false;
    if (index >= state.total_clusters) return false;
    return (state.inuse_map[index / 8] & (@as(u8, 1) << @as(u3, @intCast(index % 8)))) == 0;
}

pub fn printInfo(volume: Volume) void {
    k.puts("      FAT32 BPB: bps=");
    k.putDec(volume.bytes_per_sector);
    k.puts(" spc=");
    k.putDec(volume.sectors_per_cluster);
    k.puts(" reserved=");
    k.putDec(volume.reserved_sectors);
    k.puts(" fats=");
    k.putDec(volume.fat_count);
    k.puts("\r\n");

    k.puts("      FAT32 layout: fat_sectors=");
    k.putDec(volume.sectors_per_fat);
    k.puts(" root_cluster=");
    k.putDec(volume.root_cluster);
    k.puts(" fsinfo=");
    k.putDec(volume.fs_info_sector);
    k.puts(" backup=");
    k.putDec(volume.backup_boot_sector);
    k.puts(" data_lba=");
    k.putDec(volume.firstDataSector());
    k.puts("\r\n");
}

pub fn listRoot(volume: Volume) bool {
    k.puts("      FAT32 root:\r\n");
    return listDirectoryCluster(volume, volume.root_cluster, 12);
}

pub fn listDirectory(volume: Volume, start_cluster: u32) bool {
    return listDirectoryCluster(volume, start_cluster, 64);
}

pub fn readDirectory(volume: Volume, start_cluster: u32, out: []u8) ?usize {
    var cursor: usize = 0;
    if (!appendBytes(out, &cursor, ".\r\n")) return null;
    if (!appendBytes(out, &cursor, "..\r\n")) return null;
    if (!readDirectoryCluster(volume, start_cluster, out, &cursor, 64)) return null;
    if (cursor < out.len) out[cursor] = 0;
    return cursor;
}

pub fn readDirectoryEntry(volume: Volume, start_cluster: u32, index: usize, out: []u8) ?Entry {
    var entry: Entry = undefined;
    return if (readDirectoryEntryStatus(volume, start_cluster, index, out, &entry) == .found)
        entry
    else
        null;
}

pub fn readDirectoryEntryStatus(volume: Volume, start_cluster: u32, index: usize, out: []u8, entry_out: *Entry) LookupStatus {
    if (index < 2) {
        if (out.len < 3) return .io;
        var entry: Entry = .{ .name_len = index + 1, .attr = ATTR_DIRECTORY, .first_cluster = start_cluster };
        entry.name[0] = '.';
        out[0] = '.';
        if (index == 1) {
            entry.name[1] = '.';
            out[1] = '.';
        }
        out[entry.name_len] = 0;
        entry_out.* = entry;
        return .found;
    }
    return readDirectoryEntryClusterStatus(volume, start_cluster, index - 2, out, entry_out);
}

pub fn resolveEntryStatus(volume: Volume, path: []const u8, out: *Entry) LookupStatus {
    if (!validDataCluster(volume, volume.root_cluster)) return .io;
    var cluster = volume.root_cluster;
    var start: usize = 0;
    var result: ?Entry = null;

    while (start < path.len) {
        while (start < path.len and (path[start] == '\\' or path[start] == '/')) : (start += 1) {}
        if (start >= path.len) break;

        var end = start;
        while (end < path.len and path[end] != '\\' and path[end] != '/') : (end += 1) {}
        const segment = path[start..end];
        var entry: Entry = undefined;
        switch (findEntryStatus(volume, cluster, segment, &entry)) {
            .found => {},
            .not_found => return .not_found,
            .io => return .io,
        }
        result = entry;

        start = end;
        if (start < path.len) {
            if (!entry.isDir()) return .not_found;
            cluster = entry.first_cluster;
        }
    }

    out.* = result orelse return .not_found;
    return .found;
}

pub fn resolveEntry(volume: Volume, path: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (resolveEntryStatus(volume, path, &out) == .found) out else null;
}

pub fn resolvePathStatus(volume: Volume, path: []const u8, out: *u32) LookupStatus {
    if (!validDataCluster(volume, volume.root_cluster)) return .io;
    var cluster = volume.root_cluster;
    var start: usize = 0;

    while (start < path.len) {
        while (start < path.len and (path[start] == '\\' or path[start] == '/')) : (start += 1) {}
        if (start >= path.len) break;

        var end = start;
        while (end < path.len and path[end] != '\\' and path[end] != '/') : (end += 1) {}
        const segment = path[start..end];
        if (segment.len != 0) {
            var entry: Entry = undefined;
            switch (findEntryStatus(volume, cluster, segment, &entry)) {
                .found => {
                    if (!entry.isDir()) return .not_found;
                    cluster = entry.first_cluster;
                },
                .not_found => return .not_found,
                .io => return .io,
            }
        }
        start = end;
    }

    out.* = cluster;
    return .found;
}

pub fn resolvePath(volume: Volume, path: []const u8) ?u32 {
    var out: u32 = undefined;
    return if (resolvePathStatus(volume, path, &out) == .found) out else null;
}

pub fn findDirectory(volume: Volume, start_cluster: u32, name: []const u8) ?u32 {
    const entry = findEntry(volume, start_cluster, name) orelse return null;
    if (!entry.isDir()) return null;
    return entry.first_cluster;
}

// 0.56.34: EIN Ketten-Iterator fuer alle Verzeichnis-Scans (vorher fuenf
// identische Schleifen mit hartem guard<128, die grosse Verzeichnisse
// STILL abschnitten). DIR_CHAIN_MAX_CLUSTERS ist reiner Loop-/
// Korruptionsschutz weit oberhalb realer Verzeichnisgroessen; bei
// Erreichen wird sichtbar gewarnt statt still abgebrochen.
pub const DIR_CHAIN_MAX_CLUSTERS: usize = 4096;
pub var dir_chain_truncations: u64 = 0;

const DirChainIterator = struct {
    volume: Volume,
    cluster: u32,
    steps: usize = 0,
    fat_error: bool = false,

    fn init(volume: Volume, start_cluster: u32) DirChainIterator {
        return .{
            .volume = volume,
            .cluster = start_cluster,
            // A corrupt start cluster must never look like an empty
            // directory and must never reach clusterLba()/fatLba().
            .fat_error = !validDataCluster(volume, start_cluster),
        };
    }

    // Naechster Cluster der Kette oder null am Ende. FAT-Lesefehler setzen
    // fat_error (Aufrufer mit Fehler!=Ende pruefen das Flag nach der
    // Schleife); Loop-Schutz warnt sichtbar auf COM1/Konsole.
    fn next(self: *DirChainIterator) ?u32 {
        if (self.fat_error) return null;
        if (fatValueIsEoc(self.cluster)) return null;
        if (!validDataCluster(self.volume, self.cluster)) {
            self.fat_error = true;
            return null;
        }
        if (self.steps >= DIR_CHAIN_MAX_CLUSTERS) {
            dir_chain_truncations +%= 1;
            self.fat_error = true;
            k.puts("FAT32 WARN: dir cluster chain > ");
            k.putDec(DIR_CHAIN_MAX_CLUSTERS);
            k.puts(" (loop/corruption?), scan truncated\r\n");
            return null;
        }
        const current = self.cluster;
        self.steps += 1;
        if (readFatEntry(self.volume, current)) |nxt| {
            if (fatValueIsEoc(nxt)) {
                self.cluster = EOC;
            } else if (validDataCluster(self.volume, nxt)) {
                self.cluster = nxt;
            } else {
                // Free (0), reserved (1/0x0ffffff0..6), bad
                // (0x0ffffff7), or an address beyond this volume is
                // corruption.  Do not turn it into end-of-chain.
                self.fat_error = true;
                self.cluster = EOC;
            }
        } else {
            self.fat_error = true;
            self.cluster = EOC;
        }
        return current;
    }
};

fn validDataCluster(volume: Volume, cluster: u32) bool {
    if (cluster < 2 or cluster >= FAT_RESERVED_START or volume.sectors_per_cluster == 0) return false;

    const metadata_sectors = @as(u64, volume.reserved_sectors) +
        @as(u64, volume.fat_count) * @as(u64, volume.sectors_per_fat);
    const total_sectors = @as(u64, volume.total_sectors);
    if (metadata_sectors >= total_sectors) return false;
    const data_clusters = (total_sectors - metadata_sectors) / @as(u64, volume.sectors_per_cluster);
    const fat_entries = (@as(u64, volume.sectors_per_fat) * @as(u64, volume.bytes_per_sector)) / 4;
    const end = @min(data_clusters + 2, fat_entries);
    if (@as(u64, cluster) >= end) return false;

    // clusterLba() returns u32.  Reject a BPB/partition combination whose
    // otherwise in-volume relative address would wrap that representation.
    const relative_end = metadata_sectors +
        (@as(u64, cluster) - 1) * @as(u64, volume.sectors_per_cluster);
    return @as(u64, volume.partition_lba) + relative_end <= 0x1_0000_0000;
}

fn fatValueIsEoc(value: u32) bool {
    return value >= EOC and value <= 0x0FFF_FFFF;
}

pub fn findEntry(volume: Volume, start_cluster: u32, name: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (findEntryStatus(volume, start_cluster, name, &out) == .found) out else null;
}

pub fn findEntryStatus(volume: Volume, start_cluster: u32, name: []const u8, out: *Entry) LookupStatus {
    if (!validInputName(name)) return .io;
    var chain = DirChainIterator.init(volume, start_cluster);
    var lfn_state: LfnState = .{};
    while (chain.next()) |cluster| {
        // next() validates the FAT link before yielding the current cluster.
        // Consequently a corrupt successor must dominate even a match or an
        // end marker in the otherwise readable current cluster.
        if (chain.fat_error) return .io;
        switch (findEntryInClusterStatus(volume, cluster, name, out, &lfn_state)) {
            .found => return .found,
            .next_cluster => {},
            .end_directory => return .not_found,
            .io => return .io,
        }
    }

    return if (chain.fat_error or lfn_state.active) .io else .not_found;
}

pub fn printFile(volume: Volume, entry: Entry, max_bytes: usize) bool {
    if (entry.isDir()) return false;
    var cluster = entry.first_cluster;
    var remaining: usize = entry.size;
    var printed: usize = 0;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var guard: usize = 0;
    var coop_steps: u32 = 0;

    while (cluster >= 2 and cluster < EOC and remaining > 0 and printed < max_bytes and guard < 4096) : (guard += 1) {
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster and remaining > 0 and printed < max_bytes) : (i += 1) {
            if (!readSector(volume.device_index, volume.clusterLba(cluster) + i, 1, sector[0..])) return false;
            const count = min3(SECTOR_SIZE, remaining, max_bytes - printed);
            printTextBytes(sector[0..count]);
            remaining -= count;
            printed += count;
            cooperate(&coop_steps);
        }

        if (remaining == 0 or printed >= max_bytes) break;
        const next = readFatEntry(volume, cluster) orelse return false;
        if (next >= EOC) break;
        stats.cluster_walk_steps +%= 1;
        cluster = next;
    }

    if (printed >= max_bytes and remaining > 0) k.puts("\r\n[output truncated]\r\n");
    return true;
}

pub fn readFile(volume: Volume, entry: Entry, out: []u8) ?usize {
    const start_tick = beginOperation(.read_file);
    var ok = false;
    defer finishOperation(.read_file, start_tick, ok);

    if (entry.isDir()) return null;
    if (out.len < entry.size) return null;
    var cluster = entry.first_cluster;
    var remaining: usize = entry.size;
    var copied: usize = 0;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var guard: usize = 0;
    var coop_steps: u32 = 0;

    while (cluster >= 2 and cluster < EOC and remaining > 0 and guard < 4096) : (guard += 1) {
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster and remaining > 0) : (i += 1) {
            if (!readSector(volume.device_index, volume.clusterLba(cluster) + i, 1, sector[0..])) return null;
            const count = if (remaining < SECTOR_SIZE) remaining else SECTOR_SIZE;
            @memcpy(out[copied .. copied + count], sector[0..count]);
            copied += count;
            remaining -= count;
            cooperate(&coop_steps);
        }
        if (remaining == 0) break;
        const next = readFatEntry(volume, cluster) orelse return null;
        if (next >= EOC) break;
        stats.cluster_walk_steps +%= 1;
        cluster = next;
    }

    ok = true;
    return copied;
}

pub fn readFileRange(volume: Volume, entry: Entry, offset: usize, out: []u8) ?usize {
    const start_tick = beginOperation(.read_range);
    var ok = false;
    defer finishOperation(.read_range, start_tick, ok);

    if (entry.isDir()) return null;
    if (offset > entry.size) return null;
    if (out.len == 0 or offset == entry.size) {
        ok = true;
        return 0;
    }

    const wanted = @min(out.len, @as(usize, @intCast(entry.size)) - offset);
    const cluster_size = @as(usize, volume.clusterBytes());
    const target_cluster_index = offset / cluster_size;
    const file_clusters = fileClusterCount(entry, cluster_size);
    var cluster = entry.first_cluster;
    var cluster_index: usize = 0;
    var contiguous_remaining: usize = 1;
    if (cachedReadRangeCluster(volume, entry, target_cluster_index)) |cached| {
        cluster = cached.cluster;
        cluster_index = cached.cluster_index;
        contiguous_remaining = cached.contiguous_remaining;
    }
    if (cluster < 2) return if (wanted == 0) 0 else null;

    var skip_clusters = target_cluster_index - cluster_index;
    var guard: usize = 0;
    var coop_steps: u32 = 0;
    while (skip_clusters > 0 and cluster >= 2 and cluster < EOC and guard < 4096) : (guard += 1) {
        if (contiguous_remaining > 1) {
            const step = @min(skip_clusters, contiguous_remaining - 1);
            cluster += @intCast(step);
            cluster_index += step;
            skip_clusters -= step;
            contiguous_remaining -= step;
        } else {
            cluster = readFatEntry(volume, cluster) orelse return null;
            stats.cluster_walk_steps +%= 1;
            cluster_index += 1;
            skip_clusters -= 1;
            contiguous_remaining = 1;
            cooperate(&coop_steps);
        }
    }
    if (cluster < 2 or cluster >= EOC) return null;
    if (contiguous_remaining <= 1) {
        contiguous_remaining = cacheReadRangeExtentFrom(volume, entry, file_clusters, cluster_index, cluster, &coop_steps);
    }

    var in_cluster = offset % cluster_size;
    var copied: usize = 0;
    var sector: [SECTOR_SIZE]u8 = undefined;
    guard = 0;
    while (copied < wanted and cluster >= 2 and cluster < EOC and guard < 4096) : (guard += 1) {
        var sector_index: u8 = @intCast(in_cluster / SECTOR_SIZE);
        var sector_offset = in_cluster % SECTOR_SIZE;
        while (sector_index < volume.sectors_per_cluster and copied < wanted) {
            // 0.56.10: Voll-Sektor-Spans bulk DIREKT in den Zielpuffer
            // (ein Cache-Zugriff pro Seite, kein Umweg ueber den
            // Sektor-Stackpuffer). Rand-Sektoren laufen wie bisher.
            if (sector_offset == 0 and wanted - copied >= SECTOR_SIZE) {
                const sectors_left: usize = volume.sectors_per_cluster - sector_index;
                const full: usize = @min(sectors_left, (wanted - copied) / SECTOR_SIZE);
                if (!readSector(volume.device_index, volume.clusterLba(cluster) + sector_index, @intCast(full), out[copied .. copied + full * SECTOR_SIZE])) return null;
                copied += full * SECTOR_SIZE;
                sector_index += @intCast(full);
                cooperate(&coop_steps);
                continue;
            }
            if (!readSector(volume.device_index, volume.clusterLba(cluster) + sector_index, 1, sector[0..])) return null;
            const count = @min(SECTOR_SIZE - sector_offset, wanted - copied);
            @memcpy(out[copied .. copied + count], sector[sector_offset .. sector_offset + count]);
            copied += count;
            sector_offset = 0;
            sector_index += 1;
            cooperate(&coop_steps);
        }
        if (copied >= wanted) break;
        if (contiguous_remaining > 1) {
            cluster += 1;
            cluster_index += 1;
            contiguous_remaining -= 1;
        } else {
            cluster = readFatEntry(volume, cluster) orelse return null;
            stats.cluster_walk_steps +%= 1;
            cluster_index += 1;
            contiguous_remaining = cacheReadRangeExtentFrom(volume, entry, file_clusters, cluster_index, cluster, &coop_steps);
        }
        in_cluster = 0;
    }

    ok = true;
    return copied;
}

pub fn writeFileRange(original_volume: Volume, entry: Entry, offset: usize, data: []const u8) ?usize {
    const start_tick = beginOperation(.write_range);
    var ok = false;
    defer finishOperation(.write_range, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return null;

    if (entry.isDir() or entry.isReadOnly()) return null;
    if (offset > entry.size) return null;
    if (data.len == 0) {
        ok = true;
        return 0;
    }
    invalidateReadRangeCache();
    const available = @as(usize, @intCast(entry.size)) - offset;
    if (data.len > available) return null;
    if (entry.first_cluster < 2) return null;
    if (!writeRangeInChain(volume, entry.first_cluster, offset, data)) return null;
    if (!flushMutation(volume)) return null;
    stats.file_write_ranges +%= 1;
    stats.file_write_bytes +%= @intCast(data.len);
    ok = true;
    return data.len;
}

pub fn writeFile(original_volume: Volume, parent_cluster: u32, name: []const u8, data: []const u8) bool {
    const start_tick = beginOperation(.write_file);
    var ok = false;
    defer finishOperation(.write_file, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return false;

    invalidateAppendCache();
    if (!validInputName(name)) return false;
    var existing: EntryLocation = undefined;
    switch (findEntryLocationStatus(volume, parent_cluster, name, &existing)) {
        .found => {
            if (existing.entry.isDir()) return false;
            if (existing.entry.isReadOnly()) return false;
            if (!deleteAt(volume, existing)) return false;
        },
        .not_found => {},
        .io => return false,
    }

    const first_cluster = if (data.len == 0) 0 else allocateChain(volume, clustersForBytes(volume, data.len)) orelse return false;
    if (first_cluster != 0 and !writeClusterData(volume, first_cluster, data)) {
        freeChain(volume, first_cluster);
        return false;
    }

    var raw_entries: [MAX_DIR_ENTRIES_PER_NAME * 32]u8 = undefined;
    const entries = buildNameDirectoryEntries(volume, parent_cluster, name, ATTR_ARCHIVE, first_cluster, @intCast(data.len), raw_entries[0..]) orelse {
        if (first_cluster != 0) freeChain(volume, first_cluster);
        return false;
    };
    if (!writeDirectoryEntries(volume, parent_cluster, entries)) {
        if (first_cluster != 0) freeChain(volume, first_cluster);
        return false;
    }
    ok = flushMutation(volume);
    if (ok) {
        stats.file_writes +%= 1;
        stats.file_write_bytes +%= @intCast(data.len);
    }
    return ok;
}

pub fn appendFile(volume: Volume, parent_cluster: u32, name: []const u8, data: []const u8) bool {
    const start_tick = beginOperation(.append_file);
    var ok = false;
    defer finishOperation(.append_file, start_tick, ok);

    ok = appendFileInternal(volume, parent_cluster, name, null, true, .flush, data) == .ok;
    return ok;
}

pub fn appendFileAtOffset(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) bool {
    return appendFileAtOffsetStatus(volume, parent_cluster, name, expected_size, data) == .ok;
}

pub fn appendFileAtOffsetStatus(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) AppendStatus {
    const start_tick = beginOperation(.append_file);
    var ok = false;
    defer finishOperation(.append_file, start_tick, ok);

    const status = appendFileInternal(volume, parent_cluster, name, expected_size, false, .flush, data);
    ok = status == .ok;
    return status;
}

pub fn appendFileAtOffsetStatusDeferred(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: u32, data: []const u8) AppendStatus {
    const start_tick = beginOperation(.append_file);
    var ok = false;
    defer finishOperation(.append_file, start_tick, ok);

    const status = appendFileInternal(volume, parent_cluster, name, expected_size, false, .deferred, data);
    ok = status == .ok;
    return status;
}

fn appendFileInternal(original_volume: Volume, parent_cluster: u32, name: []const u8, expected_size: ?u32, create_missing: bool, commit_mode: AppendCommitMode, data: []const u8) AppendStatus {
    const volume = beginMutation(original_volume) orelse return .io;
    invalidateReadRangeCache();
    if (!validInputName(name)) return .invalid;
    const loc = cachedAppendLocation(volume, parent_cluster, name, expected_size) orelse find_blk: {
        var found: EntryLocation = undefined;
        switch (findEntryLocationStatus(volume, parent_cluster, name, &found)) {
            .found => break :find_blk found,
            .io => return .io,
            .not_found => {
                if (!create_missing) return .not_found;
                if (!validShortInput(name)) return .invalid;
                if (!createEmptyFile(volume, parent_cluster, name)) return .io;
                switch (findEntryLocationStatus(volume, parent_cluster, name, &found)) {
                    .found => break :find_blk found,
                    .not_found, .io => return .io,
                }
            },
        }
    };
    if (loc.entry.isDir() or loc.entry.isReadOnly()) return .invalid;
    if (expected_size) |wanted_size| {
        if (loc.entry.size != wanted_size) return .offset_mismatch;
    }
    if (data.len == 0) {
        rememberAppendLocation(volume, parent_cluster, name, loc, loc.entry.first_cluster, loc.entry.size, cachedLastCluster(volume, parent_cluster, name, loc.entry.size) orelse 0);
        return .ok;
    }

    const old_size: usize = @intCast(loc.entry.size);
    if (data.len > 0xFFFF_FFFF - old_size) return .too_large;
    const new_size = old_size + data.len;
    const cluster_size = @as(usize, volume.clusterBytes());
    const old_clusters = clustersForBytes(volume, old_size);
    const needed_clusters = clustersForBytes(volume, new_size);
    var first_cluster = loc.entry.first_cluster;
    var last_existing_cluster: u32 = 0;
    if (old_clusters > 0) {
        last_existing_cluster = cachedLastCluster(volume, parent_cluster, name, loc.entry.size) orelse
            nthCluster(volume, first_cluster, old_clusters - 1) orelse return .io;
    }

    var added: ChainAllocation = .{};
    if (needed_clusters > old_clusters) {
        const search_start = if (last_existing_cluster >= 2 and last_existing_cluster < EOC - 1)
            last_existing_cluster + 1
        else
            nextFreeStart(volume, 3);
        added = allocateChainPreferDetailed(volume, needed_clusters - old_clusters, search_start) orelse return .io;
        if (first_cluster < 2) {
            first_cluster = added.first;
        } else {
            if (!writeFatEntryAll(volume, last_existing_cluster, added.first)) {
                freeChain(volume, added.first);
                return .io;
            }
        }
    }

    if (first_cluster < 2) return .io;
    const write_start_cluster = if (old_size == 0)
        first_cluster
    else if ((old_size % cluster_size) == 0)
        added.first
    else
        last_existing_cluster;
    if (write_start_cluster < 2) return .io;
    if (!writeRangeFromCluster(volume, write_start_cluster, old_size % cluster_size, data)) return .io;
    if (!updateFileLocation(volume, loc.lba, loc.offset, first_cluster, @intCast(new_size))) return .io;
    if (commit_mode == .flush and !flushMutation(volume)) return .io;

    const last_cluster = if (added.last >= 2) added.last else last_existing_cluster;
    rememberAppendLocation(volume, parent_cluster, name, loc, first_cluster, @intCast(new_size), last_cluster);
    stats.file_appends +%= 1;
    stats.file_append_bytes +%= @intCast(data.len);
    return .ok;
}

pub fn copyFile(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent_cluster: u32, dst_name: []const u8) bool {
    return copyFileWithMode(src_volume, dst_volume, src_entry, dst_parent_cluster, dst_name, true);
}

pub fn copyFileNoReplace(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_parent_cluster: u32, dst_name: []const u8) bool {
    return copyFileWithMode(src_volume, dst_volume, src_entry, dst_parent_cluster, dst_name, false);
}

fn copyFileWithMode(src_volume: Volume, original_dst_volume: Volume, src_entry: Entry, dst_parent_cluster: u32, dst_name: []const u8, replace_existing: bool) bool {
    const start_tick = beginOperation(.copy_file);
    var ok = false;
    defer finishOperation(.copy_file, start_tick, ok);
    const dst_volume = beginMutation(original_dst_volume) orelse return false;

    invalidateAppendCache();
    if (src_entry.isDir() or !validInputName(dst_name)) return false;
    var existing: EntryLocation = undefined;
    switch (findEntryLocationStatus(dst_volume, dst_parent_cluster, dst_name, &existing)) {
        .found => {
            if (!replace_existing) return false;
            if (existing.entry.isDir()) return false;
            if (existing.entry.isReadOnly()) return false;
            if (!deleteAt(dst_volume, existing)) return false;
        },
        .not_found => {},
        .io => return false,
    }

    const file_size: usize = @intCast(src_entry.size);
    const first_cluster = if (file_size == 0) 0 else allocateChain(dst_volume, clustersForBytes(dst_volume, file_size)) orelse return false;
    if (first_cluster != 0 and !copyClusterData(src_volume, dst_volume, src_entry, first_cluster)) {
        freeChain(dst_volume, first_cluster);
        return false;
    }

    var raw_entries: [MAX_DIR_ENTRIES_PER_NAME * 32]u8 = undefined;
    const entries = buildNameDirectoryEntries(dst_volume, dst_parent_cluster, dst_name, ATTR_ARCHIVE, first_cluster, @intCast(file_size), raw_entries[0..]) orelse {
        if (first_cluster != 0) freeChain(dst_volume, first_cluster);
        return false;
    };
    if (!writeDirectoryEntries(dst_volume, dst_parent_cluster, entries)) {
        if (first_cluster != 0) freeChain(dst_volume, first_cluster);
        return false;
    }
    ok = flushMutation(dst_volume);
    if (ok) stats.file_write_bytes +%= @intCast(file_size);
    return ok;
}

pub fn makeDirectory(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    const start_tick = beginOperation(.make_directory);
    var ok = false;
    defer finishOperation(.make_directory, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return false;

    invalidateAppendCache();
    if (!validInputName(name)) return false;
    var existing: Entry = undefined;
    switch (findEntryStatus(volume, parent_cluster, name, &existing)) {
        .found => return false,
        .not_found => {},
        .io => return false,
    }

    const new_cluster = allocateChain(volume, 1) orelse return false;
    if (!initDirectoryCluster(volume, new_cluster, parent_cluster)) {
        freeChain(volume, new_cluster);
        return false;
    }

    var raw_entries: [MAX_DIR_ENTRIES_PER_NAME * 32]u8 = undefined;
    const entries = buildNameDirectoryEntries(volume, parent_cluster, name, ATTR_DIRECTORY, new_cluster, 0, raw_entries[0..]) orelse {
        freeChain(volume, new_cluster);
        return false;
    };
    if (!writeDirectoryEntries(volume, parent_cluster, entries)) {
        freeChain(volume, new_cluster);
        return false;
    }
    ok = flushMutation(volume);
    return ok;
}

pub fn deleteFile(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    const start_tick = beginOperation(.delete_file);
    var ok = false;
    defer finishOperation(.delete_file, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return false;

    invalidateAppendCache();
    var loc: EntryLocation = undefined;
    if (findEntryLocationStatus(volume, parent_cluster, name, &loc) != .found) return false;
    if (loc.entry.isDir() or loc.entry.isReadOnly()) return false;
    if (!deleteAt(volume, loc)) return false;
    ok = flushMutation(volume);
    return ok;
}

/// Deletes `name` only while it still resolves to the exact entry observed by
/// the caller under the shared filesystem-request gate.  The gate is part of
/// the identity proof for empty FAT files (cluster zero is not unique);
/// rechecking all durable entry fields here prevents a stale caller from
/// deleting a subsequently published sibling.
pub fn deleteFileIfIdentity(
    volume: Volume,
    parent_cluster: u32,
    name: []const u8,
    expected: Entry,
) DeleteIfIdentityResult {
    const start_tick = beginOperation(.delete_file);
    var ok = false;
    defer finishOperation(.delete_file, start_tick, ok);

    invalidateAppendCache();
    invalidateReadRangeCache();
    var loc: EntryLocation = undefined;
    switch (findEntryLocationStatus(volume, parent_cluster, name, &loc)) {
        .found => {},
        .not_found => {
            // Also establish a durability boundary for an idempotent retry
            // after an earlier ambiguous delete completion.
            ok = flushVolume(volume);
            return if (ok) .not_found else .io;
        },
        .io => return .io,
    }
    if (loc.entry.isDir() or loc.entry.isReadOnly()) return .mismatch;
    if (!sameEntryIdentity(loc.entry, expected)) return .mismatch;

    if (!deleteAt(volume, loc)) {
        var after: EntryLocation = undefined;
        switch (findEntryLocationStatus(volume, parent_cluster, name, &after)) {
            .not_found => {
                ok = flushVolume(volume);
                return if (ok) .deleted else .io;
            },
            .found => return if (sameEntryIdentity(after.entry, expected)) .io else .mismatch,
            .io => return .io,
        }
    }
    ok = flushVolume(volume);
    return if (ok) .deleted else .io;
}

pub fn removeDirectory(original_volume: Volume, parent_cluster: u32, name: []const u8) bool {
    const start_tick = beginOperation(.remove_directory);
    var ok = false;
    defer finishOperation(.remove_directory, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return false;

    invalidateAppendCache();
    var loc: EntryLocation = undefined;
    if (findEntryLocationStatus(volume, parent_cluster, name, &loc) != .found) return false;
    if (!loc.entry.isDir() or !directoryIsEmpty(volume, loc.entry.first_cluster)) return false;
    if (!deleteAt(volume, loc)) return false;
    ok = flushMutation(volume);
    return ok;
}

/// Reports whether `name` can be represented by exactly one FAT short-name
/// directory entry.  SYSUPD deliberately uses only these names for its
/// private stage and backup siblings so an ownership transfer never depends
/// on a partially written LFN run.
pub fn validateShortName83(name: []const u8) bool {
    return validShortInput(name);
}

/// Atomically changes the target directory entry from its old cluster chain
/// to the staged chain.  The old chain is first published under `backup_name`
/// and flushed.  The target entry is then updated and flushed as the single
/// visibility point; finally the staged directory entry is detached without
/// freeing its now target-owned chain.  Re-entering after any flush boundary
/// is safe and finishes the same ownership transfer.
///
/// Stage and backup must be 8.3 siblings in `parent_cluster`.  The target may
/// use an existing LFN run: only its already durable short directory entry is
/// updated at the visibility point, so the LFN slots remain untouched.  A new
/// long target is rejected because publishing a fresh multi-entry LFN run is
/// not atomic.  There is no copy/delete fallback.  `consume_stage` must be
/// true because leaving two live directory entries owning the staged chain
/// would create a cross-link.
pub fn replaceFileAtomic(
    volume: Volume,
    parent_cluster: u32,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
    consume_stage: bool,
) AtomicReplaceResult {
    invalidateAppendCache();
    invalidateReadRangeCache();
    if (!consume_stage) return .not_atomic;
    if (!validInputName(target_name) or !validateShortName83(staged_name) or !validateShortName83(backup_name)) return .invalid;
    if (entryNameEqualAscii(target_name, staged_name) or entryNameEqualAscii(target_name, backup_name) or entryNameEqualAscii(staged_name, backup_name)) return .alias;

    const target_is_short = validateShortName83(target_name);
    var target: ?EntryLocation = null;
    var staged: ?EntryLocation = null;
    var backup: ?EntryLocation = null;
    if (findOptionalEntryLocationStatus(volume, parent_cluster, target_name, &target) == .io or
        findOptionalEntryLocationStatus(volume, parent_cluster, staged_name, &staged) == .io or
        findOptionalEntryLocationStatus(volume, parent_cluster, backup_name, &backup) == .io)
        return .io;

    // Creating a new LFN would require publishing several directory entries.
    // Existing LFNs are safe because their short entry is the sole ownership
    // and visibility point and remains at a stable location.
    if (target == null and !target_is_short) return .not_atomic;

    if (target) |target_loc| {
        if (target_loc.entry.isDir() or target_loc.entry.isReadOnly()) return .read_only;
    }
    if (backup) |backup_loc| {
        if (backup_loc.entry.isDir() or backup_loc.entry.isReadOnly()) return .conflict;
    }

    // A missing stage after a completed detach is an idempotent success only
    // when a regular target is present.  Package-level checksum verification
    // binds that target to the expected payload before cleanup is accepted.
    if (staged == null) return if (target != null) .ok else .not_found;
    const staged_loc = staged.?;
    if (staged_loc.entry.isDir() or staged_loc.entry.isReadOnly()) return .read_only;

    if (target) |target_loc| {
        if (sameFileOwnership(target_loc.entry, staged_loc.entry)) {
            if (!detachEntryNoFree(volume, staged_loc)) return .io;
            return if (flushVolume(volume)) .ok else .io;
        }

        if (backup) |backup_loc| {
            if (!sameFileOwnership(backup_loc.entry, target_loc.entry)) return .conflict;
        } else {
            if (!createOwnershipAlias(volume, parent_cluster, backup_name, target_loc.entry)) return .io;
            if (!flushVolume(volume)) return .io;
            if (findOptionalEntryLocationStatus(volume, parent_cluster, backup_name, &backup) != .found) return .io;
            if (!sameFileOwnership(backup.?.entry, target_loc.entry)) return .io;
        }

        if (!updateFileLocation(volume, target_loc.lba, target_loc.offset, staged_loc.entry.first_cluster, staged_loc.entry.size)) return .io;
        if (!flushVolume(volume)) return .io;
        if (findOptionalEntryLocationStatus(volume, parent_cluster, target_name, &target) != .found) return .io;
        if (!sameFileOwnership(target.?.entry, staged_loc.entry)) return .io;
    } else {
        // A new target has no last-good backup.  Publishing the target entry
        // is still the visibility point; recovery may remove it again when a
        // package-wide rollback returns to the original missing state.
        if (backup != null) return .conflict;
        if (!createOwnershipAlias(volume, parent_cluster, target_name, staged_loc.entry)) return .io;
        if (!flushVolume(volume)) return .io;
    }

    const staged_status = findOptionalEntryLocationStatus(volume, parent_cluster, staged_name, &staged);
    if (staged_status == .io) return .io;
    if (staged) |stage_after_publish| {
        if (!sameFileOwnership(stage_after_publish.entry, staged_loc.entry)) return .conflict;
        if (!detachEntryNoFree(volume, stage_after_publish)) return .io;
        if (!flushVolume(volume)) return .io;
    }
    return .ok;
}

/// Create-only ownership transfer used by SFTP staging. The caller must keep
/// the filesystem gate across this call and its one permitted immediate
/// retry. That boundary is essential for empty FAT files, whose cluster zero
/// is not a globally unique identity.
pub fn replaceFileAtomicCreateOnly(
    volume: Volume,
    parent_cluster: u32,
    target_name: []const u8,
    staged_name: []const u8,
    backup_name: []const u8,
) AtomicReplaceResult {
    invalidateAppendCache();
    invalidateReadRangeCache();
    if (!validInputName(target_name) or
        !validateShortName83(staged_name) or
        !validateShortName83(backup_name))
        return .invalid;
    if (!validateShortName83(target_name)) return .not_atomic;
    if (entryNameEqualAscii(target_name, staged_name) or
        entryNameEqualAscii(target_name, backup_name) or
        entryNameEqualAscii(staged_name, backup_name))
        return .alias;

    var target: ?EntryLocation = null;
    var staged: ?EntryLocation = null;
    var backup: ?EntryLocation = null;
    if (findOptionalEntryLocationStatus(volume, parent_cluster, target_name, &target) == .io or
        findOptionalEntryLocationStatus(volume, parent_cluster, staged_name, &staged) == .io or
        findOptionalEntryLocationStatus(volume, parent_cluster, backup_name, &backup) == .io)
        return .io;
    if (backup != null) return .conflict;

    const stage_loc = staged orelse return if (target == null) .not_found else .conflict;
    if (stage_loc.entry.isDir() or stage_loc.entry.isReadOnly()) return .read_only;

    if (target) |target_loc| {
        if (!sameFileOwnership(target_loc.entry, stage_loc.entry)) return .conflict;
    } else {
        if (!createOwnershipAlias(volume, parent_cluster, target_name, stage_loc.entry)) {
            if (findOptionalEntryLocationStatus(volume, parent_cluster, target_name, &target) != .found)
                return .io;
            if (!sameFileOwnership(target.?.entry, stage_loc.entry)) return .conflict;
        }
        if (!flushOwnershipBoundary(volume)) return .io;
        if (findOptionalEntryLocationStatus(volume, parent_cluster, target_name, &target) != .found)
            return .io;
        if (!sameFileOwnership(target.?.entry, stage_loc.entry)) return .conflict;
    }

    var stage_after: ?EntryLocation = null;
    const stage_status = findOptionalEntryLocationStatus(volume, parent_cluster, staged_name, &stage_after);
    if (stage_status == .io) return .io;
    if (stage_after) |owned_alias| {
        if (!sameFileOwnership(owned_alias.entry, stage_loc.entry)) return .conflict;
        if (!detachEntryNoFree(volume, owned_alias)) {
            const reconcile = findOptionalEntryLocationStatus(volume, parent_cluster, staged_name, &stage_after);
            if (reconcile != .not_found) return .io;
        }
    }
    if (!flushOwnershipBoundary(volume)) return .io;
    return .ok;
}

fn flushOwnershipBoundary(volume: Volume) bool {
    if (flushVolume(volume)) return true;
    return flushVolume(volume);
}

fn createOwnershipAlias(volume: Volume, parent_cluster: u32, name: []const u8, source: Entry) bool {
    var existing: ?EntryLocation = null;
    if (findOptionalEntryLocationStatus(volume, parent_cluster, name, &existing) != .not_found) return false;
    var raw_entries: [MAX_DIR_ENTRIES_PER_NAME * 32]u8 = undefined;
    const entries = buildNameDirectoryEntries(volume, parent_cluster, name, source.attr, source.first_cluster, source.size, raw_entries[0..]) orelse return false;
    // An ownership alias is another name for the exact same object, not a
    // newly created file. Preserve every timestamp represented by Entry so
    // empty FAT files (cluster zero) retain a recoverable identity across a
    // lost completion and reboot.
    const short_offset = entries.len - 32;
    writeLe16(raw_entries[short_offset + 14 .. short_offset + 16], source.created_time);
    writeLe16(raw_entries[short_offset + 16 .. short_offset + 18], source.created_date);
    writeLe16(raw_entries[short_offset + 18 .. short_offset + 20], source.access_date);
    writeLe16(raw_entries[short_offset + 22 .. short_offset + 24], source.modified_time);
    writeLe16(raw_entries[short_offset + 24 .. short_offset + 26], source.modified_date);
    return writeDirectoryEntries(volume, parent_cluster, entries);
}

fn detachEntryNoFree(volume: Volume, loc: EntryLocation) bool {
    var slot_index: usize = 0;
    while (slot_index < loc.lfn_slot_count) : (slot_index += 1) {
        if (!markDirectorySlotDeleted(volume, loc.lfn_slots[slot_index])) return false;
    }
    return markDirectorySlotDeleted(volume, .{ .lba = loc.lba, .offset = loc.offset });
}

fn sameFileOwnership(a: Entry, b: Entry) bool {
    return !a.isDir() and !b.isDir() and a.first_cluster == b.first_cluster and a.size == b.size;
}

fn sameEntryIdentity(a: Entry, b: Entry) bool {
    return a.attr == b.attr and
        a.first_cluster == b.first_cluster and
        a.size == b.size and
        a.created_time == b.created_time and
        a.created_date == b.created_date and
        a.access_date == b.access_date and
        a.modified_time == b.modified_time and
        a.modified_date == b.modified_date;
}

fn entryNameEqualAscii(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |left, right| {
        if (upper(left) != upper(right)) return false;
    }
    return true;
}

pub fn renameEntry(volume: Volume, parent_cluster: u32, old_name: []const u8, new_name: []const u8) bool {
    return renameEntryStatus(volume, parent_cluster, old_name, new_name) == .ok;
}

/// Status-bearing, same-directory FAT rename.  It deliberately supports
/// only a single short-entry visibility update.  In particular, an entry
/// carrying LFN slots is rejected as not_atomic: rewriting its short alias
/// alone would leave a checksum-invalid LFN chain, while rebuilding/deleting
/// that chain would require a multi-entry transaction.
pub fn renameEntryStatus(original_volume: Volume, parent_cluster: u32, old_name: []const u8, new_name: []const u8) RenameStatus {
    const start_tick = beginOperation(.rename_entry);
    var ok = false;
    defer finishOperation(.rename_entry, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return .io;

    invalidateAppendCache();
    if (!validShortInput(old_name) or !validShortInput(new_name)) return .not_atomic;
    var destination: Entry = undefined;
    switch (findEntryStatus(volume, parent_cluster, new_name, &destination)) {
        .found => return .conflict,
        .not_found => {},
        .io => return .io,
    }
    var loc: EntryLocation = undefined;
    switch (findEntryLocationStatus(volume, parent_cluster, old_name, &loc)) {
        .found => {},
        .not_found => return .not_found,
        .io => return .io,
    }
    if (loc.entry.isReadOnly()) return .read_only;
    if (loc.lfn_slot_count != 0) return .not_atomic;

    var short: [11]u8 = undefined;
    if (!buildShortName(new_name, &short)) return .not_atomic;

    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, loc.lba, 1, sector[0..])) return .io;
    @memcpy(sector[loc.offset .. loc.offset + 11], &short);
    if (!writeSector(volume, loc.lba, 1, sector[0..])) return .io;
    stats.dir_entry_updates +%= 1;
    if (!flushMutation(volume)) return .io;
    ok = true;
    return .ok;
}

pub fn setAttributes(original_volume: Volume, parent_cluster: u32, name: []const u8, set_mask: u8, clear_mask: u8) bool {
    const start_tick = beginOperation(.set_attributes);
    var ok = false;
    defer finishOperation(.set_attributes, start_tick, ok);
    const volume = beginMutation(original_volume) orelse return false;

    invalidateAppendCache();
    var loc: EntryLocation = undefined;
    if (findEntryLocationStatus(volume, parent_cluster, name, &loc) != .found) return false;
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, loc.lba, 1, sector[0..])) return false;
    var attr = sector[loc.offset + 11];
    attr |= set_mask & (ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_ARCHIVE);
    attr &= ~(clear_mask & (ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_ARCHIVE));
    if (loc.entry.isDir()) attr |= ATTR_DIRECTORY;
    sector[loc.offset + 11] = attr;
    if (!writeSector(volume, loc.lba, 1, sector[0..])) return false;
    stats.dir_entry_updates +%= 1;
    ok = flushMutation(volume);
    return ok;
}

fn listDirectoryCluster(volume: Volume, start_cluster: u32, max_entries: usize) bool {
    var printed: usize = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return false;
        if (!listDirectorySectorRange(volume, cluster, &printed, max_entries)) return false;
        if (printed >= max_entries) return true;
    }
    if (chain.fat_error) return false;

    return true;
}

fn readDirectoryCluster(volume: Volume, start_cluster: u32, out: []u8, cursor: *usize, max_entries: usize) bool {
    var copied: usize = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return false;
        if (!readDirectorySectorRange(volume, cluster, out, cursor, &copied, max_entries)) return false;
        if (copied >= max_entries) return true;
    }
    if (chain.fat_error) return false;

    return true;
}

fn readDirectoryEntryClusterStatus(volume: Volume, start_cluster: u32, wanted: usize, out: []u8, entry_out: *Entry) LookupStatus {
    var seen: usize = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return .io;
        switch (readDirectoryEntrySectorRangeStatus(volume, cluster, wanted, &seen, out, entry_out)) {
            .found => return .found,
            .next_cluster => {},
            .end_directory => return .not_found,
            .io => return .io,
        }
    }
    return if (chain.fat_error) .io else .not_found;
}

const ClusterLookupStatus = enum(u8) {
    found,
    next_cluster,
    end_directory,
    io,
};

/// Validated VFAT long-name chain.  The physical order is LAST|N, N-1, ...
/// 1, followed immediately by the owning short entry.  Keeping this state
/// outside a single directory cluster also makes lookup consistent with the
/// writer, which may reserve a contiguous run across a cluster boundary.
const LfnState = struct {
    units: [NAME_UNITS_MAX]u16 = .{0} ** NAME_UNITS_MAX,
    len: usize = 0,
    active: bool = false,
    total: u8 = 0,
    expected: u8 = 0,
    checksum: u8 = 0,

    fn reset(self: *LfnState) void {
        self.* = .{};
    }

    fn consume(self: *LfnState, raw: []const u8) bool {
        if (raw.len < 32 or raw[11] != ATTR_LONG_NAME or raw[12] != 0 or readLe16(raw[26..28]) != 0) return false;
        // Only the LAST flag (0x40) is defined in addition to the ordinal.
        if ((raw[0] & 0xA0) != 0) return false;
        const ordinal = raw[0] & 0x1F;
        if (ordinal == 0 or @as(usize, ordinal) > MAX_LFN_ENTRIES) return false;
        const is_last = (raw[0] & 0x40) != 0;

        if (!self.active) {
            if (!is_last) return false;
            self.reset();
            self.active = true;
            self.total = ordinal;
            self.expected = ordinal;
            self.checksum = raw[13];
        } else if (is_last) {
            return false;
        }
        if (ordinal != self.expected or raw[13] != self.checksum) return false;

        const start = @as(usize, ordinal - 1) * 13;
        const slots = [_]usize{ 1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30 };
        var terminated = false;
        var i: usize = 0;
        while (i < slots.len) : (i += 1) {
            const unit = readLe16(raw[slots[i]..][0..2]);
            const dst = start + i;
            if (!is_last) {
                // Every non-LAST entry precedes the tail and therefore must
                // contain thirteen real name units.
                if (dst >= self.units.len or unit == 0 or unit == 0xFFFF) return false;
                self.units[dst] = unit;
                if (dst + 1 > self.len) self.len = dst + 1;
                continue;
            }

            // Only the highest ordinal may contain the NUL terminator and
            // 0xffff padding.  A 255-unit name uses the first out-of-buffer
            // slot for that mandatory terminator.
            if (terminated) {
                if (unit != 0xFFFF) return false;
            } else if (unit == 0) {
                terminated = true;
            } else if (unit == 0xFFFF or dst >= self.units.len) {
                return false;
            } else {
                self.units[dst] = unit;
                if (dst + 1 > self.len) self.len = dst + 1;
            }
        }

        self.expected -= 1;
        return true;
    }

    fn completeFor(self: *const LfnState, short_raw: []const u8) bool {
        if (!self.active or self.expected != 0 or self.len == 0 or short_raw.len < 11) return false;
        var short: [11]u8 = undefined;
        @memcpy(short[0..], short_raw[0..11]);
        if (lfnChecksum(short) != self.checksum) return false;
        return utf16SequenceWellFormed(self.units[0..self.len]);
    }
};

fn utf16SequenceWellFormed(units: []const u16) bool {
    var i: usize = 0;
    while (i < units.len) : (i += 1) {
        const unit = units[i];
        if (unit >= 0xD800 and unit <= 0xDBFF) {
            if (i + 1 >= units.len or units[i + 1] < 0xDC00 or units[i + 1] > 0xDFFF) return false;
            i += 1;
        } else if (unit >= 0xDC00 and unit <= 0xDFFF) {
            return false;
        }
    }
    return true;
}

fn entryClusterValid(volume: Volume, entry: Entry) bool {
    if (entry.isDir()) return validDataCluster(volume, entry.first_cluster);
    if (entry.first_cluster == 0) return entry.size == 0;
    return validDataCluster(volume, entry.first_cluster);
}

fn findEntryInClusterStatus(volume: Volume, cluster: u32, name: []const u8, out: *Entry, lfn_state: *LfnState) ClusterLookupStatus {
    stats.dir_scans +%= 1;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var i: u8 = 0;
    var coop_steps: u32 = 0;
    while (i < volume.sectors_per_cluster) : (i += 1) {
        const lba = volume.clusterLba(cluster) + i;
        if (!readSector(volume.device_index, lba, 1, sector[0..])) return .io;

        var off: usize = 0;
        while (off < SECTOR_SIZE) : (off += 32) {
            stats.dir_entries_scanned +%= 1;
            cooperate(&coop_steps);
            const entry = sector[off .. off + 32];
            if (entry[0] == 0x00) return if (lfn_state.active) .io else .end_directory;
            if (entry[0] == 0xE5) {
                if (lfn_state.active) return .io;
                continue;
            }
            if (entry[11] == ATTR_LONG_NAME) {
                if (!lfn_state.consume(entry)) return .io;
                continue;
            }
            if ((entry[11] & ATTR_LONG_NAME) == ATTR_LONG_NAME) return .io;
            if (entry[0] == '.') {
                if (lfn_state.active) return .io;
                continue;
            }

            const lfn_len = if (lfn_state.active) blk: {
                if (!lfn_state.completeFor(entry)) return .io;
                break :blk lfn_state.len;
            } else 0;
            const found = makeEntry(entry, &lfn_state.units, lfn_len);
            if (!entryClusterValid(volume, found)) return .io;
            lfn_state.reset();
            if (entryNameEquals(found, name) or shortNameEquals(entry[0..11], name)) {
                out.* = found;
                return .found;
            }
        }
    }
    return .next_cluster;
}

fn findEntryLocation(volume: Volume, start_cluster: u32, name: []const u8) ?EntryLocation {
    var out: EntryLocation = undefined;
    return if (findEntryLocationStatus(volume, start_cluster, name, &out) == .found) out else null;
}

fn findOptionalEntryLocationStatus(
    volume: Volume,
    start_cluster: u32,
    name: []const u8,
    out: *?EntryLocation,
) LookupStatus {
    var location: EntryLocation = undefined;
    const status = findEntryLocationStatus(volume, start_cluster, name, &location);
    out.* = if (status == .found) location else null;
    return status;
}

fn findEntryLocationStatus(volume: Volume, start_cluster: u32, name: []const u8, out: *EntryLocation) LookupStatus {
    if (!validInputName(name)) return .io;
    stats.dir_scans +%= 1;
    var coop_steps: u32 = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    var lfn_state: LfnState = .{};
    var lfn_slots: [MAX_LFN_ENTRIES]DirectorySlot = undefined;
    var lfn_slot_count: usize = 0;

    while (chain.next()) |cluster| {
        if (chain.fat_error) return .io;
        var sector: [SECTOR_SIZE]u8 = undefined;
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            const lba = volume.clusterLba(cluster) + i;
            if (!readSector(volume.device_index, lba, 1, sector[0..])) return .io;

            var off: usize = 0;
            while (off < SECTOR_SIZE) : (off += 32) {
                stats.dir_entries_scanned +%= 1;
                cooperate(&coop_steps);
                const raw = sector[off .. off + 32];
                if (raw[0] == 0x00) return if (lfn_state.active) .io else .not_found;
                if (raw[0] == 0xE5) {
                    if (lfn_state.active) return .io;
                    continue;
                }
                if (raw[11] == ATTR_LONG_NAME) {
                    if (!lfn_state.consume(raw) or lfn_slot_count >= MAX_LFN_ENTRIES) return .io;
                    lfn_slots[lfn_slot_count] = .{ .lba = lba, .offset = off };
                    lfn_slot_count += 1;
                    continue;
                }
                if ((raw[11] & ATTR_LONG_NAME) == ATTR_LONG_NAME) return .io;
                if (raw[0] == '.') {
                    if (lfn_state.active) return .io;
                    continue;
                }

                const lfn_len = if (lfn_state.active) blk: {
                    if (!lfn_state.completeFor(raw) or lfn_slot_count != @as(usize, lfn_state.total)) return .io;
                    break :blk lfn_state.len;
                } else 0;
                const found = makeEntry(raw, &lfn_state.units, lfn_len);
                if (!entryClusterValid(volume, found)) return .io;
                if (entryNameEquals(found, name) or shortNameEquals(raw[0..11], name)) {
                    var found_lfn_slots: [MAX_LFN_ENTRIES]DirectorySlot = undefined;
                    if (lfn_slot_count > 0) @memcpy(found_lfn_slots[0..lfn_slot_count], lfn_slots[0..lfn_slot_count]);
                    out.* = .{
                        .lba = lba,
                        .offset = off,
                        .entry = found,
                        .lfn_slots = found_lfn_slots,
                        .lfn_slot_count = lfn_slot_count,
                    };
                    return .found;
                }
                lfn_state.reset();
                lfn_slot_count = 0;
            }
        }
    }
    return if (chain.fat_error or lfn_state.active) .io else .not_found;
}

fn listDirectorySectorRange(volume: Volume, cluster: u32, printed: *usize, max_entries: usize) bool {
    stats.dir_scans +%= 1;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var lfn: [NAME_UNITS_MAX]u16 = .{0} ** NAME_UNITS_MAX;
    var lfn_len: usize = 0;
    var i: u8 = 0;
    var coop_steps: u32 = 0;
    while (i < volume.sectors_per_cluster) : (i += 1) {
        const lba = volume.clusterLba(cluster) + i;
        if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;

        var off: usize = 0;
        while (off < SECTOR_SIZE) : (off += 32) {
            stats.dir_entries_scanned +%= 1;
            cooperate(&coop_steps);
            const entry = sector[off .. off + 32];
            if (entry[0] == 0x00) return true;
            if (entry[0] == 0xE5) {
                lfn_len = 0;
                continue;
            }
            if (entry[11] == ATTR_LONG_NAME) {
                readLfnEntry(entry, &lfn, &lfn_len);
                continue;
            }

            const parsed = makeEntry(entry, &lfn, lfn_len);
            lfn_len = 0;
            printEntry(parsed);
            printed.* += 1;
            if (printed.* >= max_entries) return true;
        }
    }
    return true;
}

fn readDirectorySectorRange(volume: Volume, cluster: u32, out: []u8, cursor: *usize, copied: *usize, max_entries: usize) bool {
    stats.dir_scans +%= 1;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var lfn: [NAME_UNITS_MAX]u16 = .{0} ** NAME_UNITS_MAX;
    var lfn_len: usize = 0;
    var i: u8 = 0;
    var coop_steps: u32 = 0;
    while (i < volume.sectors_per_cluster) : (i += 1) {
        const lba = volume.clusterLba(cluster) + i;
        if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;

        var off: usize = 0;
        while (off < SECTOR_SIZE) : (off += 32) {
            stats.dir_entries_scanned +%= 1;
            cooperate(&coop_steps);
            const raw = sector[off .. off + 32];
            if (raw[0] == 0x00) return true;
            if (raw[0] == 0xE5) {
                lfn_len = 0;
                continue;
            }
            if (raw[11] == ATTR_LONG_NAME) {
                readLfnEntry(raw, &lfn, &lfn_len);
                continue;
            }
            if (raw[0] == '.') {
                lfn_len = 0;
                continue;
            }

            const parsed = makeEntry(raw, &lfn, lfn_len);
            lfn_len = 0;
            if (!appendDirectoryEntry(out, cursor, parsed)) return false;
            copied.* += 1;
            if (copied.* >= max_entries) return true;
        }
    }
    return true;
}

fn readDirectoryEntrySectorRangeStatus(
    volume: Volume,
    cluster: u32,
    wanted: usize,
    seen: *usize,
    out: []u8,
    entry_out: *Entry,
) ClusterLookupStatus {
    stats.dir_scans +%= 1;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var lfn: [NAME_UNITS_MAX]u16 = .{0} ** NAME_UNITS_MAX;
    var lfn_len: usize = 0;
    var i: u8 = 0;
    var coop_steps: u32 = 0;
    while (i < volume.sectors_per_cluster) : (i += 1) {
        const lba = volume.clusterLba(cluster) + i;
        if (!readSector(volume.device_index, lba, 1, sector[0..])) return .io;

        var off: usize = 0;
        while (off < SECTOR_SIZE) : (off += 32) {
            stats.dir_entries_scanned +%= 1;
            cooperate(&coop_steps);
            const raw = sector[off .. off + 32];
            if (raw[0] == 0x00) return .end_directory;
            if (raw[0] == 0xE5) {
                lfn_len = 0;
                continue;
            }
            if (raw[11] == ATTR_LONG_NAME) {
                readLfnEntry(raw, &lfn, &lfn_len);
                continue;
            }
            if (raw[0] == '.') {
                lfn_len = 0;
                continue;
            }

            const parsed = makeEntry(raw, &lfn, lfn_len);
            lfn_len = 0;
            if (seen.* == wanted) {
                if (!copyEntryName(out, parsed)) return .io;
                entry_out.* = parsed;
                return .found;
            }
            seen.* += 1;
        }
    }
    return .next_cluster;
}

fn printEntry(entry: Entry) void {
    k.puts("        ");
    k.puts(entry.name[0..entry.name_len]);
    if (entry.isDir()) {
        k.puts(" <DIR>");
    } else {
        k.puts(" ");
        k.putDec(entry.size);
        k.puts(" bytes");
    }
    k.puts(" cluster=");
    k.putDec(entry.first_cluster);
    k.puts("\r\n");
}

fn appendDirectoryEntry(out: []u8, cursor: *usize, entry: Entry) bool {
    if (entry.isDir()) {
        if (!appendBytes(out, cursor, "<DIR> ")) return false;
    } else {
        if (!appendBytes(out, cursor, "      ")) return false;
    }
    if (!appendBytes(out, cursor, entry.name[0..entry.name_len])) return false;
    return appendBytes(out, cursor, "\r\n");
}

fn appendBytes(out: []u8, cursor: *usize, bytes: []const u8) bool {
    if (cursor.* + bytes.len >= out.len) return false;
    @memcpy(out[cursor.* .. cursor.* + bytes.len], bytes);
    cursor.* += bytes.len;
    return true;
}

fn copyEntryName(out: []u8, entry: Entry) bool {
    if (entry.name_len + 1 > out.len) return false;
    @memcpy(out[0..entry.name_len], entry.name[0..entry.name_len]);
    out[entry.name_len] = 0;
    return true;
}

fn makeEntry(raw: []const u8, lfn: *const [NAME_UNITS_MAX]u16, lfn_len: usize) Entry {
    var entry: Entry = .{
        .attr = raw[11],
        .first_cluster = firstCluster(raw),
        .size = readLe32(raw[28..32]),
        .created_time = readLe16(raw[14..16]),
        .created_date = readLe16(raw[16..18]),
        .access_date = readLe16(raw[18..20]),
        .modified_time = readLe16(raw[22..24]),
        .modified_date = readLe16(raw[24..26]),
    };
    // A long name whose UTF-8 form does not fit the name buffer (or that
    // carries surrogate halves) falls back to the always-valid short name.
    if (lfn_len > 0) {
        if (utf16UnitsToUtf8(lfn[0..lfn_len], entry.name[0..])) |utf8_len| {
            entry.name_len = utf8_len;
            return entry;
        }
    }
    entry.name_len = shortNameToBuffer(raw[0..11], &entry.name);
    return entry;
}

fn shortNameEquals(raw: []const u8, name: []const u8) bool {
    var buf: [NAME_MAX]u8 = undefined;
    const len = shortNameToBuffer(raw, &buf);
    if (len != name.len) return false;
    var i: usize = 0;
    while (i < len) : (i += 1) {
        if (upper(buf[i]) != upper(name[i])) return false;
    }
    return true;
}

fn entryNameEquals(entry: Entry, name: []const u8) bool {
    if (entry.name_len != name.len) return false;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        if (upper(entry.name[i]) != upper(name[i])) return false;
    }
    return true;
}

fn shortNameToBuffer(raw: []const u8, out: []u8) usize {
    var len: usize = 0;
    var end: usize = 8;
    while (end > 0 and raw[end - 1] == ' ') : (end -= 1) {}

    var i: usize = 0;
    while (i < end) : (i += 1) {
        out[len] = raw[i];
        len += 1;
    }

    var ext_end: usize = 11;
    while (ext_end > 8 and raw[ext_end - 1] == ' ') : (ext_end -= 1) {}
    if (ext_end > 8) {
        out[len] = '.';
        len += 1;
        i = 8;
        while (i < ext_end) : (i += 1) {
            out[len] = raw[i];
            len += 1;
        }
    }
    return len;
}

fn readLfnEntry(raw: []const u8, out: *[NAME_UNITS_MAX]u16, len: *usize) void {
    const seq = raw[0] & 0x1F;
    if (seq == 0) return;
    const start = @as(usize, seq - 1) * 13;
    const slots = [_]usize{ 1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30 };
    var i: usize = 0;
    while (i < slots.len) : (i += 1) {
        const ch = readLe16(raw[slots[i]..][0..2]);
        if (ch == 0x0000 or ch == 0xFFFF) break;
        const dst = start + i;
        if (dst < out.len) {
            out[dst] = ch;
            if (dst + 1 > len.*) len.* = dst + 1;
        }
    }
}

// ---------------------------------------------------------------------------
// UTF-8 <-> UTF-16 for long names (0.60.18): FAT LFN entries store UTF-16
// units on disk; the VFS/API side speaks UTF-8 (BMP).  Malformed UTF-8,
// overlong forms, surrogates and non-BMP input are visible errors, never
// silently replaced.
// ---------------------------------------------------------------------------

fn utf8ToUtf16Units(name: []const u8, out: []u16) ?usize {
    var i: usize = 0;
    var pos: usize = 0;
    while (i < name.len) {
        const b = name[i];
        var unit: u16 = undefined;
        if (b < 0x80) {
            unit = b;
            i += 1;
        } else if (b & 0xE0 == 0xC0) {
            if (b < 0xC2 or i + 1 >= name.len) return null;
            const b1 = name[i + 1];
            if (b1 & 0xC0 != 0x80) return null;
            unit = (@as(u16, b & 0x1F) << 6) | (b1 & 0x3F);
            i += 2;
        } else if (b & 0xF0 == 0xE0) {
            if (i + 2 >= name.len) return null;
            const b1 = name[i + 1];
            const b2 = name[i + 2];
            if (b1 & 0xC0 != 0x80 or b2 & 0xC0 != 0x80) return null;
            if (b == 0xE0 and b1 < 0xA0) return null; // overlong
            if (b == 0xED and b1 >= 0xA0) return null; // surrogate
            unit = (@as(u16, b & 0x0F) << 12) | (@as(u16, b1 & 0x3F) << 6) | (b2 & 0x3F);
            i += 3;
        } else {
            return null; // stray continuation or non-BMP lead
        }
        if (pos >= out.len) return null;
        out[pos] = unit;
        pos += 1;
    }
    return pos;
}

fn utf16UnitsToUtf8(units: []const u16, out: []u8) ?usize {
    var pos: usize = 0;
    for (units) |unit| {
        if (unit < 0x80) {
            if (pos >= out.len) return null;
            out[pos] = @intCast(unit);
            pos += 1;
        } else if (unit < 0x800) {
            if (pos + 2 > out.len) return null;
            out[pos] = 0xC0 | @as(u8, @intCast(unit >> 6));
            out[pos + 1] = 0x80 | @as(u8, @intCast(unit & 0x3F));
            pos += 2;
        } else {
            if (unit >= 0xD800 and unit < 0xE000) return null; // surrogate half
            if (pos + 3 > out.len) return null;
            out[pos] = 0xE0 | @as(u8, @intCast(unit >> 12));
            out[pos + 1] = 0x80 | @as(u8, @intCast((unit >> 6) & 0x3F));
            out[pos + 2] = 0x80 | @as(u8, @intCast(unit & 0x3F));
            pos += 3;
        }
    }
    return pos;
}

fn printTextBytes(bytes: []const u8) void {
    for (bytes) |c| {
        switch (c) {
            '\n' => k.puts("\r\n"),
            '\r' => {},
            '\t' => k.putc('\t'),
            else => if (c >= 0x20 and c <= 0x7E) k.putc(c) else k.putc('.'),
        }
    }
}

fn min3(a: usize, b: usize, c: usize) usize {
    var m = if (a < b) a else b;
    if (c < m) m = c;
    return m;
}

fn writeDirectoryEntry(volume: Volume, directory_cluster: u32, raw_entry: []const u8) bool {
    stats.dir_scans +%= 1;
    var coop_steps: u32 = 0;
    var chain = DirChainIterator.init(volume, directory_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return false;
        var sector: [SECTOR_SIZE]u8 = undefined;
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            const lba = volume.clusterLba(cluster) + i;
            if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
            var off: usize = 0;
            while (off < SECTOR_SIZE) : (off += 32) {
                stats.dir_entries_scanned +%= 1;
                cooperate(&coop_steps);
                if (sector[off] == 0x00 or sector[off] == 0xE5) {
                    @memcpy(sector[off .. off + 32], raw_entry[0..32]);
                    if (!writeSector(volume, lba, 1, sector[0..])) return false;
                    stats.dir_entry_updates +%= 1;
                    return true;
                }
            }
        }
    }
    return false;
}

const DirectorySlot = struct {
    lba: u32,
    offset: usize,
};

fn writeDirectoryEntries(volume: Volume, directory_cluster: u32, raw_entries: []const u8) bool {
    if (raw_entries.len == 0 or raw_entries.len % 32 != 0) return false;
    const needed = raw_entries.len / 32;
    if (needed > MAX_DIR_ENTRIES_PER_NAME) return false;

    stats.dir_scans +%= 1;
    var slots: [MAX_DIR_ENTRIES_PER_NAME]DirectorySlot = undefined;
    var run_count: usize = 0;
    var coop_steps: u32 = 0;
    var chain = DirChainIterator.init(volume, directory_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return false;
        var sector: [SECTOR_SIZE]u8 = undefined;
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            const lba = volume.clusterLba(cluster) + i;
            if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
            var off: usize = 0;
            while (off < SECTOR_SIZE) : (off += 32) {
                stats.dir_entries_scanned +%= 1;
                cooperate(&coop_steps);
                if (sector[off] == 0x00 or sector[off] == 0xE5) {
                    slots[run_count] = .{ .lba = lba, .offset = off };
                    run_count += 1;
                    if (run_count == needed) return writeDirectorySlots(volume, slots[0..needed], raw_entries);
                } else {
                    run_count = 0;
                }
            }
        }
    }
    return false;
}

fn writeDirectorySlots(volume: Volume, slots: []const DirectorySlot, raw_entries: []const u8) bool {
    var sector: [SECTOR_SIZE]u8 = undefined;
    var index: usize = 0;
    while (index < slots.len) : (index += 1) {
        const slot = slots[index];
        if (!readSector(volume.device_index, slot.lba, 1, sector[0..])) return false;
        @memcpy(sector[slot.offset .. slot.offset + 32], raw_entries[index * 32 ..][0..32]);
        if (!writeSector(volume, slot.lba, 1, sector[0..])) return false;
    }
    stats.dir_entry_updates +%= @intCast(slots.len);
    return true;
}

fn buildNameDirectoryEntries(
    volume: Volume,
    parent_cluster: u32,
    name: []const u8,
    attr: u8,
    first_cluster: u32,
    size: u32,
    out: []u8,
) ?[]const u8 {
    if (out.len < MAX_DIR_ENTRIES_PER_NAME * 32) return null;
    var short: [11]u8 = undefined;
    const short_only = buildShortName(name, &short);
    if (!short_only and !buildUniqueShortAlias(volume, parent_cluster, name, &short)) return null;

    var offset: usize = 0;
    if (!short_only) {
        var units: [NAME_UNITS_MAX]u16 = undefined;
        const unit_count = utf8ToUtf16Units(name, units[0..]) orelse return null;
        const checksum = lfnChecksum(short);
        const lfn_count = (unit_count + 12) / 13;
        if (lfn_count == 0 or lfn_count > MAX_LFN_ENTRIES) return null;
        var seq = lfn_count;
        while (seq >= 1) : (seq -= 1) {
            writeLfnEntry(out[offset .. offset + 32], units[0..unit_count], seq, lfn_count, checksum);
            offset += 32;
            if (seq == 1) break;
        }
    }

    var raw: [32]u8 = .{0} ** 32;
    @memcpy(raw[0..11], &short);
    raw[11] = attr;
    writeTimestamp(&raw);
    writeFirstCluster(&raw, first_cluster);
    writeLe32(raw[28..32], size);
    @memcpy(out[offset .. offset + 32], raw[0..]);
    offset += 32;
    return out[0..offset];
}

fn writeLfnEntry(out: []u8, units: []const u16, seq: usize, total: usize, checksum: u8) void {
    @memset(out, 0);
    const seq_byte: u8 = @intCast(seq);
    out[0] = if (seq == total) seq_byte | 0x40 else seq_byte;
    out[11] = ATTR_LONG_NAME;
    out[12] = 0;
    out[13] = checksum;
    out[26] = 0;
    out[27] = 0;

    const start = (seq - 1) * 13;
    const slots = [_]usize{ 1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30 };
    var ended = false;
    var i: usize = 0;
    while (i < slots.len) : (i += 1) {
        const off = slots[i];
        if (start + i < units.len) {
            writeLe16(out[off .. off + 2], units[start + i]);
        } else if (!ended) {
            out[off] = 0;
            out[off + 1] = 0;
            ended = true;
        } else {
            out[off] = 0xFF;
            out[off + 1] = 0xFF;
        }
    }
}

fn lfnChecksum(short: [11]u8) u8 {
    var sum: u8 = 0;
    for (short) |ch| {
        const lo: u8 = if ((sum & 1) != 0) 0x80 else 0;
        sum = lo +% (sum >> 1) +% ch;
    }
    return sum;
}

fn buildUniqueShortAlias(volume: Volume, parent_cluster: u32, name: []const u8, out: *[11]u8) bool {
    if (!validLongInput(name)) return false;
    var suffix: u32 = 1;
    while (suffix < 1000) : (suffix += 1) {
        if (!buildShortAlias(name, suffix, out)) return false;
        if (!shortNameRawUsed(volume, parent_cluster, out.*)) return true;
    }
    return false;
}

fn buildShortAlias(name: []const u8, suffix: u32, out: *[11]u8) bool {
    @memset(out, ' ');
    if (suffix == 0 or suffix > 999) return false;

    var dot: ?usize = null;
    var i: usize = name.len;
    while (i > 0) : (i -= 1) {
        if (name[i - 1] == '.') {
            dot = i - 1;
            break;
        }
    }

    const base = if (dot) |d| name[0..d] else name;
    const ext = if (dot) |d| name[d + 1 ..] else "";
    if (base.len == 0) return false;

    var digits: [3]u8 = undefined;
    const digit_count = decimalDigits(suffix, digits[0..]);
    const base_limit = 8 - digit_count - 1;
    var out_index: usize = 0;
    i = 0;
    while (i < base.len and out_index < base_limit) : (i += 1) {
        const c = sanitizeShortChar(base[i]);
        out[out_index] = c;
        out_index += 1;
    }
    if (out_index == 0) {
        out[0] = '_';
        out_index = 1;
    }
    out[out_index] = '~';
    @memcpy(out[out_index + 1 .. out_index + 1 + digit_count], digits[0..digit_count]);

    i = 0;
    while (i < ext.len and i < 3) : (i += 1) out[8 + i] = sanitizeShortChar(ext[i]);
    return true;
}

fn decimalDigits(value: u32, out: []u8) usize {
    var temp: [3]u8 = undefined;
    var n = value;
    var count: usize = 0;
    while (n > 0 and count < temp.len) : (n /= 10) {
        temp[temp.len - 1 - count] = @intCast('0' + (n % 10));
        count += 1;
    }
    var i: usize = 0;
    while (i < count) : (i += 1) out[i] = temp[temp.len - count + i];
    return count;
}

fn sanitizeShortChar(ch: u8) u8 {
    const upper_ch = upper(ch);
    if (shortAllowed(upper_ch)) return upper_ch;
    return '_';
}

fn shortNameRawUsed(volume: Volume, start_cluster: u32, short: [11]u8) bool {
    stats.dir_scans +%= 1;
    var coop_steps: u32 = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return true;
        var sector: [SECTOR_SIZE]u8 = undefined;
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            const lba = volume.clusterLba(cluster) + i;
            if (!readSector(volume.device_index, lba, 1, sector[0..])) return true;
            var off: usize = 0;
            while (off < SECTOR_SIZE) : (off += 32) {
                stats.dir_entries_scanned +%= 1;
                cooperate(&coop_steps);
                const raw = sector[off .. off + 32];
                if (raw[0] == 0x00) return false;
                if (raw[0] == 0xE5 or raw[11] == ATTR_LONG_NAME) continue;
                if (rawShortEquals(raw[0..11], short)) return true;
            }
        }
    }
    // FAT-Lesefehler/abgeschnittene Kette konservativ als "belegt"
    // werten (wie das alte Guard-Ende).
    if (chain.fat_error or chain.steps >= DIR_CHAIN_MAX_CLUSTERS) return true;
    return false;
}

fn rawShortEquals(raw: []const u8, short: [11]u8) bool {
    var i: usize = 0;
    while (i < 11) : (i += 1) {
        if (raw[i] != short[i]) return false;
    }
    return true;
}

fn deleteAt(volume: Volume, loc: EntryLocation) bool {
    var slot_index: usize = 0;
    while (slot_index < loc.lfn_slot_count) : (slot_index += 1) {
        if (!markDirectorySlotDeleted(volume, loc.lfn_slots[slot_index])) return false;
    }
    if (!markDirectorySlotDeleted(volume, .{ .lba = loc.lba, .offset = loc.offset })) return false;
    if (loc.entry.first_cluster >= 2) freeChain(volume, loc.entry.first_cluster);
    return true;
}

fn markDirectorySlotDeleted(volume: Volume, slot: DirectorySlot) bool {
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, slot.lba, 1, sector[0..])) return false;
    sector[slot.offset] = 0xE5;
    if (!writeSector(volume, slot.lba, 1, sector[0..])) return false;
    stats.dir_entry_updates +%= 1;
    return true;
}

fn directoryIsEmpty(volume: Volume, start_cluster: u32) bool {
    stats.dir_scans +%= 1;
    var coop_steps: u32 = 0;
    var chain = DirChainIterator.init(volume, start_cluster);
    while (chain.next()) |cluster| {
        if (chain.fat_error) return false;
        var sector: [SECTOR_SIZE]u8 = undefined;
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            if (!readSector(volume.device_index, volume.clusterLba(cluster) + i, 1, sector[0..])) return false;
            var off: usize = 0;
            while (off < SECTOR_SIZE) : (off += 32) {
                stats.dir_entries_scanned +%= 1;
                cooperate(&coop_steps);
                const raw = sector[off .. off + 32];
                if (raw[0] == 0x00) return true;
                if (raw[0] == 0xE5 or raw[11] == ATTR_LONG_NAME or raw[0] == '.') continue;
                return false;
            }
        }
    }
    // Fehler oder abgeschnittene Kette: konservativ NICHT als leer melden.
    if (chain.fat_error or chain.steps >= DIR_CHAIN_MAX_CLUSTERS) return false;
    return true;
}

fn initDirectoryCluster(volume: Volume, cluster: u32, parent_cluster: u32) bool {
    var sector: [SECTOR_SIZE]u8 = .{0} ** SECTOR_SIZE;
    var dot: [11]u8 = "           ".*;
    dot[0] = '.';
    var dotdot: [11]u8 = "           ".*;
    dotdot[0] = '.';
    dotdot[1] = '.';
    @memcpy(sector[0..11], &dot);
    sector[11] = ATTR_DIRECTORY;
    writeFirstCluster(sector[0..32], cluster);
    @memcpy(sector[32..43], &dotdot);
    sector[43] = ATTR_DIRECTORY;
    writeFirstCluster(sector[32..64], parent_cluster);

    var i: u8 = 0;
    var coop_steps: u32 = 0;
    while (i < volume.sectors_per_cluster) : (i += 1) {
        if (!writeSector(volume, volume.clusterLba(cluster) + i, 1, sector[0..])) return false;
        @memset(sector[0..], 0);
        cooperate(&coop_steps);
    }
    return true;
}

fn writeClusterData(volume: Volume, start_cluster: u32, data: []const u8) bool {
    var cluster = start_cluster;
    var written: usize = 0;
    var sector: [SECTOR_SIZE]u8 = .{0} ** SECTOR_SIZE;
    var coop_steps: u32 = 0;
    while (cluster >= 2 and cluster < EOC) {
        var i: u8 = 0;
        while (i < volume.sectors_per_cluster) : (i += 1) {
            @memset(sector[0..], 0);
            if (written < data.len) {
                const count = if (data.len - written < SECTOR_SIZE) data.len - written else SECTOR_SIZE;
                @memcpy(sector[0..count], data[written .. written + count]);
                written += count;
            }
            if (!writeSector(volume, volume.clusterLba(cluster) + i, 1, sector[0..])) return false;
            cooperate(&coop_steps);
        }
        if (written >= data.len) return true;
        const next = readFatEntry(volume, cluster) orelse return false;
        if (next >= EOC) return written >= data.len;
        stats.cluster_walk_steps +%= 1;
        cluster = next;
    }
    return written >= data.len;
}

fn copyClusterData(src_volume: Volume, dst_volume: Volume, src_entry: Entry, dst_start_cluster: u32) bool {
    if (src_entry.size == 0) return true;
    if (src_entry.first_cluster < 2 or dst_start_cluster < 2) return false;

    var src_cluster = src_entry.first_cluster;
    var dst_cluster = dst_start_cluster;
    var src_sector_index: u8 = 0;
    var dst_sector_index: u8 = 0;
    var remaining: usize = @intCast(src_entry.size);
    var sector: [SECTOR_SIZE]u8 = undefined;
    const max_steps = ((remaining + SECTOR_SIZE - 1) / SECTOR_SIZE) + 8;
    var steps: usize = 0;
    var coop_steps: u32 = 0;

    while (remaining > 0 and steps < max_steps) : (steps += 1) {
        if (!readSector(src_volume.device_index, src_volume.clusterLba(src_cluster) + src_sector_index, 1, sector[0..])) return false;
        const count = if (remaining < SECTOR_SIZE) remaining else SECTOR_SIZE;
        if (count < SECTOR_SIZE) @memset(sector[count..], 0);
        if (!writeSector(dst_volume, dst_volume.clusterLba(dst_cluster) + dst_sector_index, 1, sector[0..])) return false;
        remaining -= count;
        cooperate(&coop_steps);
        if (remaining == 0) return true;

        src_sector_index += 1;
        if (src_sector_index >= src_volume.sectors_per_cluster) {
            src_sector_index = 0;
            const next_src = readFatEntry(src_volume, src_cluster) orelse return false;
            if (next_src >= EOC) return false;
            stats.cluster_walk_steps +%= 1;
            src_cluster = next_src;
        }

        dst_sector_index += 1;
        if (dst_sector_index >= dst_volume.sectors_per_cluster) {
            dst_sector_index = 0;
            const next_dst = readFatEntry(dst_volume, dst_cluster) orelse return false;
            if (next_dst >= EOC and remaining > 0) return false;
            stats.cluster_walk_steps +%= 1;
            dst_cluster = next_dst;
        }
    }
    return remaining == 0;
}

fn createEmptyFile(volume: Volume, parent_cluster: u32, name: []const u8) bool {
    var short: [11]u8 = undefined;
    if (!buildShortName(name, &short)) return false;

    var raw: [32]u8 = .{0} ** 32;
    @memcpy(raw[0..11], &short);
    raw[11] = ATTR_ARCHIVE;
    writeTimestamp(&raw);
    writeFirstCluster(&raw, 0);
    writeLe32(raw[28..32], 0);
    return writeDirectoryEntry(volume, parent_cluster, raw[0..]);
}

fn updateFileLocation(volume: Volume, lba: u32, offset: usize, first_cluster: u32, size: u32) bool {
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
    const raw = sector[offset .. offset + 32];
    writeFirstCluster(raw, first_cluster);
    writeLe32(raw[28..32], size);
    const stamp = fatTimestampNow();
    writeLe16(raw[18..20], stamp.date);
    writeLe16(raw[22..24], stamp.time);
    writeLe16(raw[24..26], stamp.date);
    if (!writeSector(volume, lba, 1, sector[0..])) return false;
    stats.dir_entry_updates +%= 1;
    return true;
}

fn cachedAppendLocation(volume: Volume, parent_cluster: u32, name: []const u8, expected_size: ?u32) ?EntryLocation {
    if (!appendCacheMatches(volume, parent_cluster, name)) return null;
    if (expected_size) |wanted_size| {
        if (append_cache.size != wanted_size) return null;
    }
    var entry: Entry = .{
        .name_len = append_cache.name_len,
        .attr = append_cache.attr,
        .first_cluster = append_cache.first_cluster,
        .size = append_cache.size,
    };
    if (entry.name_len > 0) @memcpy(entry.name[0..entry.name_len], append_cache.name[0..entry.name_len]);
    return .{
        .lba = append_cache.lba,
        .offset = append_cache.offset,
        .entry = entry,
        .lfn_slot_count = 0,
    };
}

fn cachedLastCluster(volume: Volume, parent_cluster: u32, name: []const u8, size: u32) ?u32 {
    if (!appendCacheMatches(volume, parent_cluster, name)) return null;
    if (append_cache.size != size or append_cache.last_cluster < 2) return null;
    return append_cache.last_cluster;
}

fn rememberAppendLocation(volume: Volume, parent_cluster: u32, name: []const u8, loc: EntryLocation, first_cluster: u32, size: u32, last_cluster: u32) void {
    append_cache = .{
        .valid = true,
        .device_index = volume.device_index,
        .partition_lba = volume.partition_lba,
        .parent_cluster = parent_cluster,
        .name_len = @min(name.len, NAME_MAX),
        .lba = loc.lba,
        .offset = loc.offset,
        .attr = loc.entry.attr,
        .first_cluster = first_cluster,
        .size = size,
        .last_cluster = last_cluster,
    };
    if (append_cache.name_len > 0) @memcpy(append_cache.name[0..append_cache.name_len], name[0..append_cache.name_len]);
}

fn invalidateAppendCache() void {
    append_cache.valid = false;
    invalidateReadRangeCache();
}

fn invalidateReadRangeCache() void {
    for (&read_range_extents) |*extent| {
        extent.valid = false;
    }
}

fn cachedReadRangeCluster(volume: Volume, entry: Entry, target_cluster_index: usize) ?ReadRangeCursor {
    var best: ?ReadRangeCursor = null;
    var best_index: usize = 0;

    for (&read_range_extents) |*extent| {
        if (!readRangeExtentMatches(extent.*, volume, entry)) continue;
        if (extent.cluster < 2 or extent.cluster >= EOC or extent.count == 0) continue;

        const start = extent.cluster_index;
        const end = start + extent.count;
        if (target_cluster_index >= start and target_cluster_index < end) {
            const offset = target_cluster_index - start;
            touchReadRangeExtent(extent);
            stats.read_extent_cache_hits +%= 1;
            return .{
                .cluster_index = target_cluster_index,
                .cluster = extent.cluster + @as(u32, @intCast(offset)),
                .contiguous_remaining = extent.count - offset,
            };
        }

        if (end <= target_cluster_index and (best == null or end - 1 > best_index)) {
            const offset = extent.count - 1;
            best_index = end - 1;
            best = .{
                .cluster_index = best_index,
                .cluster = extent.cluster + @as(u32, @intCast(offset)),
                .contiguous_remaining = 1,
            };
        }
    }

    if (best) |cursor| {
        stats.read_extent_cache_hits +%= 1;
        return cursor;
    }

    stats.read_extent_cache_misses +%= 1;
    return null;
}

fn cacheReadRangeExtentFrom(volume: Volume, entry: Entry, file_clusters: usize, cluster_index: usize, start_cluster: u32, coop_steps: *u32) usize {
    if (entry.first_cluster < 2 or start_cluster < 2 or start_cluster >= EOC or file_clusters == 0 or cluster_index >= file_clusters) return 1;

    const max_count = @min(file_clusters - cluster_index, READ_RANGE_EXTENT_PREFETCH_CLUSTERS);
    var count: usize = 1;
    var cluster = start_cluster;
    var guard: usize = 0;
    while (count < max_count and cluster >= 2 and cluster < EOC and guard < READ_RANGE_EXTENT_PREFETCH_CLUSTERS) : (guard += 1) {
        const next = readFatEntry(volume, cluster) orelse break;
        stats.cluster_walk_steps +%= 1;
        if (next != cluster + 1 or next >= EOC) break;
        cluster = next;
        count += 1;
        cooperate(coop_steps);
    }

    rememberReadRangeExtent(volume, entry, cluster_index, start_cluster, count);
    return count;
}

fn rememberReadRangeExtent(volume: Volume, entry: Entry, cluster_index: usize, cluster: u32, count: usize) void {
    if (entry.first_cluster < 2 or cluster < 2 or cluster >= EOC or count == 0) return;

    var target: ?*ReadRangeExtent = null;
    for (&read_range_extents) |*extent| {
        if (readRangeExtentMatches(extent.*, volume, entry) and extent.cluster_index == cluster_index) {
            target = extent;
            break;
        }
    }
    if (target == null) {
        for (&read_range_extents) |*extent| {
            if (!extent.valid) {
                target = extent;
                break;
            }
        }
    }
    if (target == null) {
        var lru_index: usize = 0;
        var lru_tick = read_range_extents[0].last_used;
        var i: usize = 1;
        while (i < read_range_extents.len) : (i += 1) {
            if (read_range_extents[i].last_used < lru_tick) {
                lru_tick = read_range_extents[i].last_used;
                lru_index = i;
            }
        }
        target = &read_range_extents[lru_index];
    }

    read_range_cache_clock +%= 1;
    target.?.* = .{
        .valid = true,
        .device_index = volume.device_index,
        .partition_lba = volume.partition_lba,
        .first_cluster = entry.first_cluster,
        .size = entry.size,
        .cluster_index = cluster_index,
        .cluster = cluster,
        .count = count,
        .last_used = read_range_cache_clock,
    };
    stats.read_extent_cache_stores +%= 1;
    stats.read_extent_cache_clusters +%= @as(u64, @intCast(count));
}

fn touchReadRangeExtent(extent: *ReadRangeExtent) void {
    read_range_cache_clock +%= 1;
    extent.last_used = read_range_cache_clock;
}

fn readRangeExtentMatches(extent: ReadRangeExtent, volume: Volume, entry: Entry) bool {
    return extent.valid and
        extent.device_index == volume.device_index and
        extent.partition_lba == volume.partition_lba and
        extent.first_cluster == entry.first_cluster and
        extent.size == entry.size;
}

fn fileClusterCount(entry: Entry, cluster_size: usize) usize {
    if (entry.size == 0 or cluster_size == 0) return 0;
    const size = @as(usize, @intCast(entry.size));
    return (size + cluster_size - 1) / cluster_size;
}

fn appendCacheMatches(volume: Volume, parent_cluster: u32, name: []const u8) bool {
    if (!append_cache.valid) return false;
    if (append_cache.device_index != volume.device_index or append_cache.partition_lba != volume.partition_lba) return false;
    if (append_cache.parent_cluster != parent_cluster or append_cache.name_len != name.len) return false;
    if (name.len > NAME_MAX) return false;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        if (append_cache.name[i] != name[i]) return false;
    }
    return true;
}

fn nthCluster(volume: Volume, start_cluster: u32, index: usize) ?u32 {
    var cluster = start_cluster;
    var i: usize = 0;
    var coop_steps: u32 = 0;
    while (i < index and cluster >= 2 and cluster < EOC and i < 4096) : (i += 1) {
        cluster = readFatEntry(volume, cluster) orelse return null;
        stats.cluster_walk_steps +%= 1;
        cooperate(&coop_steps);
    }
    if (cluster < 2 or cluster >= EOC) return null;
    return cluster;
}

fn writeRangeInChain(volume: Volume, start_cluster: u32, offset: usize, data: []const u8) bool {
    const cluster_size = @as(usize, volume.clusterBytes());
    var cluster = nthCluster(volume, start_cluster, offset / cluster_size) orelse return false;
    var in_cluster = offset % cluster_size;
    var written: usize = 0;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var guard: usize = 0;
    var coop_steps: u32 = 0;

    while (written < data.len and cluster >= 2 and cluster < EOC and guard < 4096) : (guard += 1) {
        var sector_index: u8 = @intCast(in_cluster / SECTOR_SIZE);
        var sector_offset = in_cluster % SECTOR_SIZE;
        while (sector_index < volume.sectors_per_cluster and written < data.len) {
            const lba = volume.clusterLba(cluster) + sector_index;
            if (sector_offset == 0) {
                const full_sectors = @min((data.len - written) / SECTOR_SIZE, @as(usize, volume.sectors_per_cluster - sector_index));
                if (full_sectors > 0) {
                    const byte_count = full_sectors * SECTOR_SIZE;
                    if (!writeSector(volume, lba, @intCast(full_sectors), data[written .. written + byte_count])) return false;
                    written += byte_count;
                    sector_index += @intCast(full_sectors);
                    cooperate(&coop_steps);
                    continue;
                }
            }
            const count = @min(SECTOR_SIZE - sector_offset, data.len - written);
            if (sector_offset == 0 and count == SECTOR_SIZE) {
                if (!writeSector(volume, lba, 1, data[written .. written + SECTOR_SIZE])) return false;
            } else {
                if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
                @memcpy(sector[sector_offset .. sector_offset + count], data[written .. written + count]);
                if (!writeSector(volume, lba, 1, sector[0..])) return false;
            }
            written += count;
            sector_offset = 0;
            sector_index += 1;
            cooperate(&coop_steps);
        }
        if (written >= data.len) return true;
        cluster = readFatEntry(volume, cluster) orelse return false;
        stats.cluster_walk_steps +%= 1;
        in_cluster = 0;
    }

    return written >= data.len;
}

fn writeRangeFromCluster(volume: Volume, start_cluster: u32, in_cluster_offset: usize, data: []const u8) bool {
    const cluster_size = @as(usize, volume.clusterBytes());
    if (in_cluster_offset >= cluster_size) return false;

    var cluster = start_cluster;
    var in_cluster = in_cluster_offset;
    var written: usize = 0;
    var sector: [SECTOR_SIZE]u8 = undefined;
    var guard: usize = 0;
    var coop_steps: u32 = 0;

    while (written < data.len and cluster >= 2 and cluster < EOC and guard < 4096) : (guard += 1) {
        var sector_index: u8 = @intCast(in_cluster / SECTOR_SIZE);
        var sector_offset = in_cluster % SECTOR_SIZE;
        while (sector_index < volume.sectors_per_cluster and written < data.len) {
            const lba = volume.clusterLba(cluster) + sector_index;
            if (sector_offset == 0) {
                const full_sectors = @min((data.len - written) / SECTOR_SIZE, @as(usize, volume.sectors_per_cluster - sector_index));
                if (full_sectors > 0) {
                    const byte_count = full_sectors * SECTOR_SIZE;
                    if (!writeSector(volume, lba, @intCast(full_sectors), data[written .. written + byte_count])) return false;
                    written += byte_count;
                    sector_index += @intCast(full_sectors);
                    cooperate(&coop_steps);
                    continue;
                }
            }
            const count = @min(SECTOR_SIZE - sector_offset, data.len - written);
            if (sector_offset == 0 and count == SECTOR_SIZE) {
                if (!writeSector(volume, lba, 1, data[written .. written + SECTOR_SIZE])) return false;
            } else {
                if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
                @memcpy(sector[sector_offset .. sector_offset + count], data[written .. written + count]);
                if (!writeSector(volume, lba, 1, sector[0..])) return false;
            }
            written += count;
            sector_offset = 0;
            sector_index += 1;
            cooperate(&coop_steps);
        }
        if (written >= data.len) return true;
        cluster = readFatEntry(volume, cluster) orelse return false;
        stats.cluster_walk_steps +%= 1;
        in_cluster = 0;
    }

    return written >= data.len;
}

fn allocateChain(volume: Volume, count: usize) ?u32 {
    return if (allocateChainFrom(volume, count, 3)) |chain| chain.first else null;
}

fn allocateChainPrefer(volume: Volume, count: usize, start_cluster: u32) ?u32 {
    return if (allocateChainPreferDetailed(volume, count, start_cluster)) |chain| chain.first else null;
}

fn allocateChainPreferDetailed(volume: Volume, count: usize, start_cluster: u32) ?ChainAllocation {
    const normalized_start: u32 = if (start_cluster >= 2 and start_cluster < EOC) start_cluster else 3;
    if (allocateChainFrom(volume, count, normalized_start)) |first| return first;
    if (normalized_start != 3) return allocateChainFrom(volume, count, 3);
    return null;
}

fn allocateChainFrom(volume: Volume, count: usize, start_cluster: u32) ?ChainAllocation {
    if (count == 0) return .{};
    stats.alloc_chain_calls +%= 1;
    var first: u32 = 0;
    var prev: u32 = 0;
    var allocated: usize = 0;
    var search_start: u32 = if (start_cluster >= 2) start_cluster else 3;
    var coop_steps: u32 = 0;

    while (allocated < count) {
        const remaining = count - allocated;
        const run = findFreeClusterRun(volume, search_start, remaining) orelse {
            if (first >= 2) freeChain(volume, first);
            return null;
        };
        if (run.first < 2 or run.count == 0) {
            if (first >= 2) freeChain(volume, first);
            return null;
        }
        if (!writeFatChainRunAll(volume, run.first, run.count, EOC_MARK)) {
            if (first >= 2) freeChain(volume, first);
            return null;
        }
        if (prev >= 2 and !writeFatEntryAll(volume, prev, run.first)) {
            freeChain(volume, first);
            freeChain(volume, run.first);
            return null;
        }
        if (first == 0) first = run.first;
        prev = run.first + @as(u32, @intCast(run.count - 1));
        allocated += run.count;
        rememberAllocatedClusterRange(volume, run.first, run.count);
        recordAllocatedRun(run.count);
        search_start = prev + 1;
        cooperate(&coop_steps);
    }
    stats.alloc_clusters +%= @intCast(count);
    if (!writeRuntimeFsInfo(volume)) {
        if (first >= 2) freeChain(volume, first);
        return null;
    }
    return .{ .first = first, .last = prev };
}

fn freeChain(volume: Volume, start_cluster: u32) void {
    var cluster = start_cluster;
    var guard: usize = 0;
    var coop_steps: u32 = 0;
    while (cluster >= 2 and cluster < EOC and guard < 4096) : (guard += 1) {
        const next = readFatEntry(volume, cluster) orelse EOC_MARK;
        _ = writeFatEntryAll(volume, cluster, 0);
        rememberFreeCluster(volume, cluster);
        cooperate(&coop_steps);
        if (next >= EOC) break;
        stats.cluster_walk_steps +%= 1;
        cluster = next;
    }
    _ = writeRuntimeFsInfo(volume);
}

fn findFreeCluster(volume: Volume, start_cluster: u32) ?u32 {
    ensureFatScanned(volume);
    const max_entries = allocClusterEnd(volume);
    const normalized_start = if (start_cluster >= 2 and start_cluster < max_entries) start_cluster else 3;
    var cluster = nextFreeStart(volume, normalized_start);
    var coop_steps: u32 = 0;
    while (cluster < max_entries) : (cluster += 1) {
        stats.alloc_search_steps +%= 1;
        cooperate(&coop_steps);
        if ((readFatEntry(volume, cluster) orelse return null) == 0) {
            rememberAllocatedCluster(volume, cluster);
            return cluster;
        }
    }

    if (cluster != normalized_start) {
        cluster = normalized_start;
        const stop = nextFreeStart(volume, normalized_start);
        while (cluster < stop and cluster < max_entries) : (cluster += 1) {
            stats.alloc_search_steps +%= 1;
            cooperate(&coop_steps);
            if ((readFatEntry(volume, cluster) orelse return null) == 0) {
                rememberAllocatedCluster(volume, cluster);
                return cluster;
            }
        }
    }
    return null;
}

fn findFreeClusterRun(volume: Volume, start_cluster: u32, wanted_count: usize) ?ClusterRun {
    if (wanted_count == 0) return null;
    ensureFatScanned(volume);
    const end = allocClusterEnd(volume);
    if (end <= 2) return null;
    const normalized_start = if (start_cluster >= 2 and start_cluster < end) start_cluster else 3;
    const hinted_raw = nextFreeStart(volume, normalized_start);
    const hinted_start = if (hinted_raw >= 2 and hinted_raw < end) hinted_raw else normalized_start;

    if (scanFreeClusterRunInMap(volume, hinted_start, end, wanted_count, .{})) |first_pass| {
        if (first_pass.count >= wanted_count) return first_pass;
        if (hinted_start != normalized_start) {
            if (scanFreeClusterRunInMap(volume, normalized_start, hinted_start, wanted_count, first_pass)) |second_pass| return second_pass;
        }
        return first_pass;
    }

    if (scanFreeClusterRun(volume, hinted_start, end, wanted_count, .{})) |first_pass| {
        if (first_pass.count >= wanted_count) return first_pass;
        if (hinted_start != normalized_start) {
            if (scanFreeClusterRun(volume, normalized_start, hinted_start, wanted_count, first_pass)) |second_pass| return second_pass;
        }
        return first_pass;
    }

    if (hinted_start != normalized_start) {
        return scanFreeClusterRun(volume, normalized_start, hinted_start, wanted_count, .{});
    }
    return null;
}

fn scanFreeClusterRunInMap(volume: Volume, start_cluster: u32, stop_cluster: u32, wanted_count: usize, initial_best: ClusterRun) ?ClusterRun {
    const state = runtimeStateFor(volume, false) orelse {
        stats.inusemap_alloc_misses +%= 1;
        return null;
    };
    if (!state.inuse_map_ready or wanted_count == 0 or start_cluster >= stop_cluster) {
        stats.inusemap_alloc_misses +%= 1;
        return null;
    }

    var best = initial_best;
    var run_first: u32 = 0;
    var run_count: usize = 0;
    var cluster = start_cluster;
    var coop_steps: u32 = 0;
    while (cluster < stop_cluster) : (cluster += 1) {
        stats.alloc_search_steps +%= 1;
        cooperate(&coop_steps);
        if (stateClusterFree(state, cluster)) {
            if (run_count == 0) run_first = cluster;
            run_count += 1;
            if (run_count >= wanted_count) {
                stats.inusemap_alloc_hits +%= 1;
                return .{ .first = run_first, .count = wanted_count };
            }
        } else {
            if (run_count > best.count) best = .{ .first = run_first, .count = run_count };
            run_count = 0;
        }
    }
    if (run_count > best.count) best = .{ .first = run_first, .count = run_count };
    if (best.count > 0) {
        stats.inusemap_alloc_hits +%= 1;
        return best;
    }
    stats.inusemap_alloc_misses +%= 1;
    return null;
}

fn scanFreeClusterRun(volume: Volume, start_cluster: u32, stop_cluster: u32, wanted_count: usize, initial_best: ClusterRun) ?ClusterRun {
    if (wanted_count == 0 or start_cluster >= stop_cluster) return if (initial_best.count > 0) initial_best else null;

    var best = initial_best;
    var run_first: u32 = 0;
    var run_count: usize = 0;
    var cluster = start_cluster;
    var coop_steps: u32 = 0;
    while (cluster < stop_cluster) {
        var sector: [SECTOR_SIZE]u8 = undefined;
        if (!readSector(volume.device_index, volume.fatLba(cluster), 1, sector[0..])) return null;
        var offset = volume.fatOffset(cluster);
        while (cluster < stop_cluster and offset + 4 <= SECTOR_SIZE) : ({
            cluster += 1;
            offset += 4;
        }) {
            stats.fat_reads +%= 1;
            stats.alloc_search_steps +%= 1;
            cooperate(&coop_steps);
            const free = (readLe32(sector[offset..][0..4]) & 0x0FFF_FFFF) == 0;
            if (free) {
                if (run_count == 0) run_first = cluster;
                run_count += 1;
                if (run_count >= wanted_count) return .{ .first = run_first, .count = wanted_count };
            } else {
                if (run_count > best.count) best = .{ .first = run_first, .count = run_count };
                run_count = 0;
            }
        }
    }
    if (run_count > best.count) best = .{ .first = run_first, .count = run_count };
    return if (best.count > 0) best else null;
}

fn allocClusterEnd(volume: Volume) u32 {
    const fat_entries = (volume.sectors_per_fat * @as(u32, SECTOR_SIZE)) / 4;
    const data_entries = volume.totalClusters() + 2;
    if (data_entries >= 2 and data_entries < fat_entries) return data_entries;
    return fat_entries;
}

fn nextFreeStart(volume: Volume, requested_start: u32) u32 {
    const normalized = if (requested_start >= 2) requested_start else 3;
    if (normalized > 3) return normalized;
    if (runtimeStateFor(volume, false)) |state| {
        if (state.next_free_cluster > normalized and state.next_free_cluster < allocClusterEnd(volume)) return state.next_free_cluster;
    }
    const hint = allocationHintFor(volume);
    if (hint.next_free_cluster > normalized and hint.next_free_cluster < EOC) return hint.next_free_cluster;
    return normalized;
}

fn allocationHintFor(volume: Volume) *VolumeAllocationHint {
    var fallback: ?*VolumeAllocationHint = null;
    var i: usize = 0;
    while (i < allocation_hints.len) : (i += 1) {
        if (allocation_hints[i].valid and allocation_hints[i].device_index == volume.device_index and allocation_hints[i].partition_lba == volume.partition_lba) {
            return &allocation_hints[i];
        }
        if (!allocation_hints[i].valid and fallback == null) fallback = &allocation_hints[i];
    }
    const slot = fallback orelse &allocation_hints[0];
    const state_next = if (runtimeStateFor(volume, false)) |state| state.next_free_cluster else 3;
    slot.* = .{
        .valid = true,
        .device_index = volume.device_index,
        .partition_lba = volume.partition_lba,
        .next_free_cluster = if (state_next >= 2) state_next else 3,
    };
    return slot;
}

fn seedAllocationHint(volume: Volume, next_free_cluster: u32) void {
    const hint = allocationHintFor(volume);
    hint.next_free_cluster = if (next_free_cluster >= 2) next_free_cluster else 3;
}

fn rememberAllocatedCluster(volume: Volume, cluster: u32) void {
    const hint = allocationHintFor(volume);
    if (cluster >= hint.next_free_cluster) hint.next_free_cluster = cluster + 1;
    if (runtimeStateFor(volume, false)) |state| {
        if (state.inuse_map_ready and stateClusterFree(state, cluster)) {
            markInuseBit(state, cluster);
            if (state.free_clusters > 0) state.free_clusters -= 1;
        } else if (!state.inuse_map_ready and state.free_clusters > 0) {
            state.free_clusters -= 1;
        }
        if (cluster >= state.next_free_cluster) state.next_free_cluster = findNextFreeInState(state, cluster + 1);
    }
}

fn rememberAllocatedClusterRange(volume: Volume, first: u32, count: usize) void {
    if (first < 2 or count == 0) return;
    const last = first + @as(u32, @intCast(count - 1));
    const hint = allocationHintFor(volume);
    if (last >= hint.next_free_cluster) hint.next_free_cluster = last + 1;
    if (runtimeStateFor(volume, false)) |state| {
        var newly_used: u32 = 0;
        var i: usize = 0;
        while (i < count) : (i += 1) {
            const cluster = first + @as(u32, @intCast(i));
            if (state.inuse_map_ready) {
                if (stateClusterFree(state, cluster)) {
                    markInuseBit(state, cluster);
                    newly_used += 1;
                }
            } else {
                newly_used += 1;
            }
        }
        if (newly_used >= state.free_clusters) {
            state.free_clusters = 0;
        } else {
            state.free_clusters -= newly_used;
        }
        if (last >= state.next_free_cluster) state.next_free_cluster = findNextFreeInState(state, last + 1);
    }
}

fn rememberFreeCluster(volume: Volume, cluster: u32) void {
    const hint = allocationHintFor(volume);
    if (cluster >= 2 and cluster < hint.next_free_cluster) hint.next_free_cluster = cluster;
    if (runtimeStateFor(volume, false)) |state| {
        if (state.inuse_map_ready) {
            if (!stateClusterFree(state, cluster)) {
                clearInuseBit(state, cluster);
                state.free_clusters +%= 1;
            }
        } else {
            state.free_clusters +%= 1;
        }
        if (cluster >= 2 and cluster < state.next_free_cluster) state.next_free_cluster = cluster;
    }
}

fn findNextFreeInState(state: *const VolumeRuntimeState, requested_start: u32) u32 {
    const end = state.total_clusters + 2;
    var cluster = if (requested_start >= 2 and requested_start < end) requested_start else 2;
    if (state.inuse_map_ready) {
        while (cluster < end) : (cluster += 1) {
            if (stateClusterFree(state, cluster)) return cluster;
        }
        cluster = 2;
        while (cluster < requested_start and cluster < end) : (cluster += 1) {
            if (stateClusterFree(state, cluster)) return cluster;
        }
    }
    return end;
}

fn writeRuntimeFsInfo(volume: Volume) bool {
    const state = runtimeStateFor(volume, false) orelse return true;
    return writeFsInfoState(volume, state);
}

fn recordAllocatedRun(count: usize) void {
    stats.alloc_runs +%= 1;
    stats.alloc_run_clusters +%= @intCast(count);
    if (@as(u64, @intCast(count)) > stats.alloc_run_max_clusters) stats.alloc_run_max_clusters = @intCast(count);
}

fn writeFatEntryAll(volume: Volume, cluster: u32, value: u32) bool {
    return writeFatChainRunAll(volume, cluster, 1, value);
}

fn writeFatChainRunAll(volume: Volume, start_cluster: u32, count: usize, fillwith: u32) bool {
    if (count == 0 or start_cluster < 2) return false;
    const end_cluster = start_cluster + @as(u32, @intCast(count - 1));
    if (end_cluster >= allocClusterEnd(volume)) return false;
    stats.fat_writes +%= @intCast(count);

    var fat: u8 = 0;
    while (fat < volume.fat_count) : (fat += 1) {
        var cluster = start_cluster;
        var remaining = count;
        var coop_steps: u32 = 0;
        while (remaining > 0) {
            const lba = volume.partition_lba + volume.reserved_sectors + @as(u32, fat) * volume.sectors_per_fat + (cluster * 4) / volume.bytes_per_sector;
            var sector: [SECTOR_SIZE]u8 = undefined;
            if (!readSector(volume.device_index, lba, 1, sector[0..])) return false;
            var offset = @as(usize, @intCast((cluster * 4) % volume.bytes_per_sector));
            while (remaining > 0 and offset + 4 <= SECTOR_SIZE) : ({
                cluster += 1;
                remaining -= 1;
                offset += 4;
            }) {
                const next_value = if (remaining == 1) fillwith else cluster + 1;
                const old = readLe32(sector[offset..][0..4]);
                writeLe32(sector[offset..][0..4], (old & 0xF000_0000) | (next_value & 0x0FFF_FFFF));
            }
            if (!writeSector(volume, lba, 1, sector[0..])) return false;
            stats.fat_mirror_writes +%= 1;
            stats.fat_sector_writes +%= 1;
            cooperate(&coop_steps);
        }
    }
    return true;
}

pub fn flushVolume(volume: Volume) bool {
    return flushDevice(volume.device_index);
}

fn clustersForBytes(volume: Volume, bytes: usize) usize {
    const cluster_size = @as(usize, volume.bytes_per_sector) * volume.sectors_per_cluster;
    return (bytes + cluster_size - 1) / cluster_size;
}

fn validShortInput(name: []const u8) bool {
    var short: [11]u8 = undefined;
    return buildShortName(name, &short);
}

fn validInputName(name: []const u8) bool {
    if (validShortInput(name)) return true;
    return validLongInput(name);
}

fn validLongInput(name: []const u8) bool {
    if (name.len == 0 or name.len >= NAME_MAX) return false;
    if (name[0] == '.' or name[name.len - 1] == '.') return false;
    // UTF-8 (BMP) since 0.60.18: the name must convert cleanly to UTF-16
    // units; reserved and control characters stay rejected.
    var units: [NAME_UNITS_MAX]u16 = undefined;
    const count = utf8ToUtf16Units(name, units[0..]) orelse return false;
    var has_text = false;
    for (units[0..count]) |unit| {
        if (unit < 0x20 or unit == 0x7F) return false;
        switch (unit) {
            '\\', '/', ':', '*', '?', '"', '<', '>', '|' => return false,
            ' ' => {},
            else => has_text = true,
        }
    }
    return has_text;
}

fn buildShortName(name: []const u8, out: *[11]u8) bool {
    @memset(out, ' ');
    if (name.len == 0) return false;

    var dot: ?usize = null;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        if (name[i] == '.') {
            if (dot != null) return false;
            dot = i;
        }
    }

    const base = if (dot) |d| name[0..d] else name;
    const ext = if (dot) |d| name[d + 1 ..] else "";
    if (base.len == 0 or base.len > 8 or ext.len > 3) return false;

    i = 0;
    while (i < base.len) : (i += 1) {
        const c = upper(base[i]);
        if (!shortAllowed(c)) return false;
        out[i] = c;
    }
    i = 0;
    while (i < ext.len) : (i += 1) {
        const c = upper(ext[i]);
        if (!shortAllowed(c)) return false;
        out[8 + i] = c;
    }
    return true;
}

fn shortAllowed(c: u8) bool {
    return (c >= 'A' and c <= 'Z') or (c >= '0' and c <= '9') or c == '_' or c == '-' or c == '$' or c == '~';
}

fn writeFirstCluster(raw: []u8, cluster: u32) void {
    writeLe16(raw[20..22], @truncate(cluster >> 16));
    writeLe16(raw[26..28], @truncate(cluster));
}

fn writeTimestamp(raw: *[32]u8) void {
    const stamp = fatTimestampNow();
    writeLe16(raw[14..16], stamp.time);
    writeLe16(raw[16..18], stamp.date);
    writeLe16(raw[18..20], stamp.date);
    writeLe16(raw[22..24], stamp.time);
    writeLe16(raw[24..26], stamp.date);
}

const FatTimestamp = struct {
    date: u16,
    time: u16,
};

fn fatTimestampNow() FatTimestamp {
    // R4OS stores UTC system time in FAT date/time fields; local rendering is userland policy.
    const now = time_core.wallClock();
    if (!validFatTimestampSource(now)) return .{ .date = encodeFatDate(1980, 1, 1), .time = 0 };
    return .{
        .date = encodeFatDate(now.year, now.month, now.day),
        .time = encodeFatTime(now.hour, now.minute, now.second),
    };
}

fn validFatTimestampSource(now: time_core.WallClock) bool {
    if (!now.valid) return false;
    if (now.year < 1980 or now.year > 2107) return false;
    if (now.month < 1 or now.month > 12) return false;
    if (now.day < 1 or now.day > daysInMonth(now.year, now.month)) return false;
    if (now.hour > 23 or now.minute > 59 or now.second > 59) return false;
    return true;
}

fn encodeFatDate(year_raw: u16, month_raw: u8, day_raw: u8) u16 {
    return ((year_raw - 1980) << 9) | (@as(u16, month_raw) << 5) | day_raw;
}

fn encodeFatTime(hour_raw: u8, minute_raw: u8, second_raw: u8) u16 {
    return (@as(u16, hour_raw) << 11) | (@as(u16, minute_raw) << 5) | (second_raw / 2);
}

fn daysInMonth(year: u16, month: u8) u8 {
    return switch (month) {
        1, 3, 5, 7, 8, 10, 12 => 31,
        4, 6, 9, 11 => 30,
        2 => if (isLeapYear(year)) 29 else 28,
        else => 0,
    };
}

fn isLeapYear(year: u16) bool {
    return (year % 4 == 0 and year % 100 != 0) or (year % 400 == 0);
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

/// Backend-exact name comparison (0.60.24).
///
/// FAT32 folds ASCII case only; bytes >= 0x80 are compared as-is.  That is a
/// real difference from NTFS `$UpCase`, which also folds many non-ASCII
/// pairs - so `ae`-umlaut and its capital are DISTINCT here and IDENTICAL on
/// NTFS.  Callers deciding whether two target names would collide must
/// therefore ask the volume rather than fold bytes themselves.
pub fn namesEqualCollated(volume: Volume, a: []const u8, b: []const u8) ?bool {
    _ = volume;
    if (a.len == 0 or b.len == 0) return null;
    if (!validInputName(a) or !validInputName(b)) return null;
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn readFatEntry(volume: Volume, cluster: u32) ?u32 {
    stats.fat_reads +%= 1;
    var sector: [SECTOR_SIZE]u8 = undefined;
    if (!readSector(volume.device_index, volume.fatLba(cluster), 1, sector[0..])) return null;
    return readLe32(sector[volume.fatOffset(cluster)..][0..4]) & 0x0FFF_FFFF;
}

fn firstCluster(entry: []const u8) u32 {
    return (@as(u32, readLe16(entry[20..22])) << 16) | readLe16(entry[26..28]);
}

fn readLe16(bytes: []const u8) u16 {
    return @as(u16, bytes[0]) | (@as(u16, bytes[1]) << 8);
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}

fn writeLe16(bytes: []u8, value: u16) void {
    bytes[0] = @truncate(value);
    bytes[1] = @truncate(value >> 8);
}

fn writeLe32(bytes: []u8, value: u32) void {
    bytes[0] = @truncate(value);
    bytes[1] = @truncate(value >> 8);
    bytes[2] = @truncate(value >> 16);
    bytes[3] = @truncate(value >> 24);
}
