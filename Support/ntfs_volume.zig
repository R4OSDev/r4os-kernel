// Shared NTFS 3.1 volume logic (read since 0.60.4, write phase 1 since
// 0.60.6, write phase 2 since 0.60.7).
//
// Runs identically in the kernel (device = page cache) and in host models
// (device = RAM image) through a small device seam.  The kernel adapter in
// Code/Kernel/fs/ntfs/ntfs.zig owns mount state and the VFS vocabulary; host
// tools drive this module directly for write-order crash testing.
//
// Write phase 1 covers the data path: create/overwrite/append/delete with
// cluster and MFT record allocation, the volume dirty flag with $MFTMirr
// sync and a strict crash-safe write order:
//   dirty -> flush -> bitmap bits -> flush -> payload data -> record (+
//   mirror) -> flush -> index entry -> flush -> dirty clear -> flush.
// Bitmap bits are durable BEFORE any record references the clusters: a crash
// leaks at most allocated-but-unreferenced clusters (chkdsk minor), never a
// cross-link.
//
// Write phase 2 (0.60.7) adds the full $I30 B+ tree engine (leaf/interior
// splits, root push-down, first-time $INDEX_ALLOCATION creation and growth,
// interior deletion via in-subtree predecessor with empty-chain reclaim),
// mkdir/rmdir/rename, in-place append including the resident->non-resident
// conversion, and $MFT growth.  Emptied-but-referenced leaf blocks stay
// allocated (valid NTFS); their space returns when the directory shrinks
// structurally or is removed.

const ntfs = @import("ntfs_format");

pub const SECTOR_SIZE: usize = 512;
/// Windows-parity name limits (0.60.19): NTFS names carry at most 255
/// UTF-16 units; the UTF-8 (BMP) worst case is 765 bytes, buffered as 768.
pub const NAME_UNITS_MAX: usize = 255;
pub const NAME_MAX: usize = 768;
pub const MAX_MFT_RUNS: usize = 64;
pub const MAX_ATTR_RUNS: usize = 256;
pub const METADATA_CACHE_VERSION: u32 = 1;
pub const METADATA_RECORD_CAPACITY: usize = 8;
pub const METADATA_ATTRIBUTE_CAPACITY: usize = 4;
pub const METADATA_INDEX_CAPACITY: usize = 2;
pub const METADATA_PATH_CAPACITY: usize = 8;
pub const METADATA_CACHE_SLOT_CAPACITY: usize = METADATA_RECORD_CAPACITY +
    METADATA_ATTRIBUTE_CAPACITY + METADATA_INDEX_CAPACITY + METADATA_PATH_CAPACITY;
const MAX_INDEX_DEPTH: usize = 8;
const MAX_PATH_DEPTH: usize = 24;
const MAX_DATA_RUNS: usize = 24;
const U32_MAX: u64 = 0xffff_ffff;

pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;

// ---------------------------------------------------------------------------
// Device seam and volume reference
// ---------------------------------------------------------------------------

pub const Device = struct {
    ctx: *anyopaque,
    read_sectors: *const fn (ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool,
    write_sectors: *const fn (ctx: *anyopaque, lba: u64, count: u32, data: []const u8) bool,
    flush: *const fn (ctx: *anyopaque) bool,
};

/// Maximum on-disk index entry size (0x10 header + 0x42 + 255 UTF-16 chars
/// + child VCN, 8-aligned).
const ENTRY_MAX: usize = 0x10 + 0x42 + 510 + 8 + 8;

pub const Scratch = struct {
    attr: AttrScratch = .{},
    record: [4096]u8 = undefined,
    part_record: [4096]u8 = undefined,
    write_record: [4096]u8 = undefined,
    // `block` carries ENTRY_MAX slack beyond the index block size so an
    // insert may temporarily overflow before a split partitions it.
    block: [4096 + ENTRY_MAX]u8 = undefined,
    block2: [4096]u8 = undefined,
    entry_a: [ENTRY_MAX]u8 = undefined,
    entry_b: [ENTRY_MAX]u8 = undefined,
    name_utf16: [NAME_UNITS_MAX * 2]u8 = undefined,
    sector: [SECTOR_SIZE]u8 = undefined,
    // Write-path attribute collections live here instead of on the stack:
    // an AttrScratch is ~7 KB and the write call chains nest four levels
    // deep, which would overflow small kernel task stacks.  Slot discipline
    // by call level (holders must not rely on a slot surviving a call into
    // a deeper level that re-collects it):
    //   attr_op     - public operations (deleteFile/append/rename/rmdir)
    //   attr_index  - index tree layer (insert/remove/dup-update/push-down)
    //   attr_mgmt_a - allocation management ($I30 alloc/bitmap, MFT scan)
    //   attr_mgmt_b - second management collection
    //   attr_cluster- $Bitmap cluster primitives
    //   attr_mirror - $MFTMirr runlist during storeRecord
    attr_op: AttrScratch = .{},
    attr_index: AttrScratch = .{},
    attr_mgmt_a: AttrScratch = .{},
    attr_mgmt_b: AttrScratch = .{},
    attr_cluster: AttrScratch = .{},
    attr_mirror: AttrScratch = .{},
    mirror_buf: [4096]u8 = undefined,
    // LZNT1 compression-unit buffers (64 KB units at 4-KB clusters).
    comp_in: [65536]u8 = undefined,
    comp_out: [65536]u8 = undefined,
};

pub const AttrScratch = struct {
    runs: [MAX_ATTR_RUNS]ntfs.Run = undefined,
    count: usize = 0,
    data_size: u64 = 0,
    initialized_size: u64 = 0,
    alloc_size: u64 = 0,
    flags: u16 = 0,
    compression_unit: u8 = 0,
    resident: bool = false,
    resident_len: usize = 0,
    resident_copy: [1024]u8 = undefined,
};

pub const Volume = struct {
    device: Device,
    partition_lba: u32,
    cluster_bytes: u32,
    record_bytes: u32,
    index_block_bytes: u32,
    total_sectors: u64,
    /// Caller-owned $MFT runlist storage; growMft appends to it in place so
    /// the same operation keeps addressing freshly grown records.
    mft_runs_buf: []ntfs.Run,
    mft_run_count: *usize,
    upcase: []const u8,
    scratch: *Scratch,
    now_filetime: u64 = 0,
    security_id_file: u32 = 265,
    security_id_dir: u32 = 264,
    /// Optional caller-owned decoded-metadata cache. Kernel mounts provide
    /// one stable cache per mount slot; host models may leave it disabled.
    metadata_cache: ?*MetadataCache = null,
    /// Monotonic caller clock used only for bounded negative-entry expiry.
    metadata_cache_now_ticks: u64 = 0,

    pub fn sectorsPerCluster(self: *const Volume) u32 {
        return @intCast(self.cluster_bytes / SECTOR_SIZE);
    }

    pub fn totalClusters(self: *const Volume) u64 {
        if (self.cluster_bytes == 0 or self.cluster_bytes % SECTOR_SIZE != 0) return 0;
        const sectors_per_cluster = self.sectorsPerCluster();
        return self.total_sectors / sectors_per_cluster;
    }

    pub fn mftRuns(self: *const Volume) []const ntfs.Run {
        return self.mft_runs_buf[0..self.mft_run_count.*];
    }
};

pub const Entry = struct {
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
    attr: u8 = 0,
    /// Reparse points (junctions, symlinks) are visible rejections: they
    /// resolve as entries but refuse content access and traversal.
    reparse: bool = false,
    record: u64 = 0,
    /// MFT sequence carried by the directory's FileReference.  Together
    /// with `record` this is the stable on-disk identity; record numbers
    /// alone may be reused after deletion.
    sequence: u16 = 0,
    size: u64 = 0,
    created_time_nt: u64 = 0,
    modified_time_nt: u64 = 0,
    access_time_nt: u64 = 0,

    pub fn isDir(self: Entry) bool {
        return (self.attr & ATTR_DIRECTORY) != 0;
    }
};

pub const WriteStatus = enum(u8) {
    ok,
    invalid,
    not_found,
    exists,
    directory,
    not_directory,
    not_empty,
    read_only_target,
    no_space,
    dir_full,
    record_full,
    unsupported,
    offset_mismatch,
    io,
    /// The primary mutation failed and at least one required rollback or
    /// dirty-clear step failed as well.  Callers must preserve the dirty
    /// volume state and must not reinterpret this as the primary error.
    cleanup_failed,
};

/// Result of a name/path lookup.  `not_found` is reserved for a completely
/// and validly walked index/path.  Any malformed metadata, stale
/// FileReference or device failure is `io`, never an apparent absence.
pub const LookupStatus = enum(u8) {
    found,
    not_found,
    io,
};

pub const MountInfo = struct {
    cluster_bytes: u32,
    record_bytes: u32,
    index_block_bytes: u32,
    total_sectors: u64,
    mft_lcn: u64,
    mftmirr_lcn: u64,
    mft_run_count: usize,
};

// ---------------------------------------------------------------------------
// Raw I/O helpers
// ---------------------------------------------------------------------------

fn checkedAddU64(a: u64, b: u64) ?u64 {
    if (b > ~@as(u64, 0) - a) return null;
    return a + b;
}

fn checkedMulU64(a: u64, b: u64) ?u64 {
    if (a != 0 and b > ~@as(u64, 0) / a) return null;
    return a * b;
}

fn sectorIoRangeValid(v: *const Volume, lba: u64, count: u32, buffer_len: usize) bool {
    if (count == 0) return buffer_len == 0;
    const byte_count = checkedMulU64(@as(u64, count), SECTOR_SIZE) orelse return false;
    const buffer_len_u64: u64 = @intCast(buffer_len);
    if (byte_count != buffer_len_u64) return false;

    const partition_start: u64 = v.partition_lba;
    if (lba < partition_start) return false;
    const relative_lba = lba - partition_start;
    if (relative_lba >= v.total_sectors) return false;
    return @as(u64, count) <= v.total_sectors - relative_lba;
}

fn readSectors(v: *const Volume, lba: u64, count: u32, out: []u8) bool {
    if (!sectorIoRangeValid(v, lba, count, out.len)) return false;
    return v.device.read_sectors(v.device.ctx, lba, count, out);
}

fn writeSectors(v: *const Volume, lba: u64, count: u32, data: []const u8) bool {
    if (!sectorIoRangeValid(v, lba, count, data.len)) return false;
    // Invalidate decoded metadata before any physical write can become
    // visible. This deliberately covers data, namespace, recovery and
    // partial-failure paths with one correctness boundary.
    if (count != 0) if (v.metadata_cache) |cache| cache.invalidateMutation();
    return v.device.write_sectors(v.device.ctx, lba, count, data);
}

fn deviceFlush(v: *const Volume) bool {
    return v.device.flush(v.device.ctx);
}

/// Reads bytes addressed inside one extent starting at `lcn`.
fn readLcnBytes(v: *const Volume, lcn: u64, byte_offset: u64, out: []u8) bool {
    return lcnByteIo(v, lcn, byte_offset, out, null);
}

/// Writes bytes addressed inside one extent starting at `lcn`.
fn writeLcnBytes(v: *const Volume, lcn: u64, byte_offset: u64, data: []const u8) bool {
    return lcnByteIo(v, lcn, byte_offset, @constCast(data), .write);
}

const IoDir = enum { write };

fn lcnByteIo(v: *const Volume, lcn: u64, byte_offset: u64, buffer: []u8, dir: ?IoDir) bool {
    if (v.cluster_bytes == 0 or v.cluster_bytes % SECTOR_SIZE != 0) return false;
    const total_clusters = v.totalClusters();
    if (lcn >= total_clusters) return false;
    const lcn_window = checkedMulU64(
        total_clusters - lcn,
        @as(u64, v.cluster_bytes),
    ) orelse return false;
    if (byte_offset > lcn_window) return false;
    const buffer_len: u64 = @intCast(buffer.len);
    if (buffer_len > lcn_window - byte_offset) return false;

    const volume_bytes = checkedMulU64(v.total_sectors, SECTOR_SIZE) orelse return false;
    const lcn_bytes = checkedMulU64(lcn, @as(u64, v.cluster_bytes)) orelse return false;
    const first_byte = checkedAddU64(lcn_bytes, byte_offset) orelse return false;
    if (first_byte > volume_bytes) return false;
    if (buffer_len > volume_bytes - first_byte) return false;

    var remaining: usize = buffer.len;
    var pos: usize = 0;
    var offset = first_byte;
    while (remaining > 0) {
        const relative_lba = offset / SECTOR_SIZE;
        const lba = checkedAddU64(@as(u64, v.partition_lba), relative_lba) orelse return false;
        const in_sector: usize = @intCast(offset % SECTOR_SIZE);
        if (in_sector == 0 and remaining >= SECTOR_SIZE) {
            const whole = remaining / SECTOR_SIZE;
            const chunk: u32 = @intCast(@min(whole, 64));
            const span = buffer[pos .. pos + @as(usize, chunk) * SECTOR_SIZE];
            const ok = if (dir == null) readSectors(v, lba, chunk, span) else writeSectors(v, lba, chunk, span);
            if (!ok) return false;
            pos += span.len;
            offset += span.len;
            remaining -= span.len;
            continue;
        }
        const sector = v.scratch.sector[0..];
        if (!readSectors(v, lba, 1, sector)) return false;
        const take = @min(SECTOR_SIZE - in_sector, remaining);
        if (dir == null) {
            @memcpy(buffer[pos .. pos + take], sector[in_sector .. in_sector + take]);
        } else {
            @memcpy(sector[in_sector .. in_sector + take], buffer[pos .. pos + take]);
            if (!writeSectors(v, lba, 1, sector)) return false;
        }
        pos += take;
        offset += take;
        remaining -= take;
    }
    return true;
}

/// Reads a byte range in the VCN space of a runlist (sparse reads zeros).
fn readRunBytes(v: *const Volume, runs: []const ntfs.Run, byte_offset: u64, out: []u8) bool {
    @memset(out, 0);
    return runByteIo(v, runs, byte_offset, out, null);
}

/// Writes a byte range in the VCN space of a runlist (sparse runs fail).
fn writeRunBytes(v: *const Volume, runs: []const ntfs.Run, byte_offset: u64, data: []const u8) bool {
    return runByteIo(v, runs, byte_offset, @constCast(data), .write);
}

fn runByteIo(v: *const Volume, runs: []const ntfs.Run, byte_offset: u64, buffer: []u8, dir: ?IoDir) bool {
    const cluster: u64 = v.cluster_bytes;
    if (cluster == 0 or !runlistPhysicalRangeValid(v, runs)) return false;
    var want_start = byte_offset;
    var want_len: u64 = buffer.len;
    var buf_pos: usize = 0;
    var run_start: u64 = 0;
    for (runs) |run| {
        const run_bytes = checkedMulU64(run.length_clusters, cluster) orelse return false;
        const run_end = checkedAddU64(run_start, run_bytes) orelse return false;
        if (want_len == 0) break;
        if (want_start < run_end) {
            if (want_start < run_start) return false;
            const inside = want_start - run_start;
            var take = run_end - want_start;
            if (take > want_len) take = want_len;
            const span = buffer[buf_pos .. buf_pos + @as(usize, @intCast(take))];
            if (run.lcn) |lcn| {
                const ok = if (dir == null) readLcnBytes(v, lcn, inside, span) else writeLcnBytes(v, lcn, inside, span);
                if (!ok) return false;
            } else if (dir != null) {
                return false; // writing into a sparse hole needs allocation
            }
            buf_pos += span.len;
            want_start += take;
            want_len -= take;
        }
        run_start = run_end;
    }
    return want_len == 0;
}

fn runlistPhysicalRangeValid(v: *const Volume, runs: []const ntfs.Run) bool {
    if (v.cluster_bytes == 0 or v.cluster_bytes % SECTOR_SIZE != 0 or
        checkedMulU64(v.total_sectors, SECTOR_SIZE) == null)
    {
        return false;
    }
    var logical_clusters: u64 = 0;
    for (runs) |run| {
        if (run.length_clusters == 0) return false;
        logical_clusters = checkedAddU64(logical_clusters, run.length_clusters) orelse return false;
        if (!mappedRunPhysicalRangeValid(v, run)) return false;
    }
    return true;
}

fn mappedRunPhysicalRangeValid(v: *const Volume, run: ntfs.Run) bool {
    if (run.length_clusters == 0) return false;
    if (v.cluster_bytes == 0 or v.cluster_bytes % SECTOR_SIZE != 0 or
        checkedMulU64(v.total_sectors, SECTOR_SIZE) == null)
    {
        return false;
    }
    const lcn = run.lcn orelse return true;
    const total_clusters = v.totalClusters();
    if (lcn >= total_clusters or run.length_clusters > total_clusters - lcn) return false;
    _ = checkedMulU64(lcn, @as(u64, v.cluster_bytes)) orelse return false;
    _ = checkedMulU64(run.length_clusters, @as(u64, v.cluster_bytes)) orelse return false;
    return true;
}

fn clusterByteOffset(v: *const Volume, vcn: u64) ?u64 {
    return checkedMulU64(vcn, @as(u64, v.cluster_bytes));
}

fn sectorByteOffset(sector_index: u64) ?u64 {
    return checkedMulU64(sector_index, SECTOR_SIZE);
}

// ---------------------------------------------------------------------------
// Mount and record access
// ---------------------------------------------------------------------------

/// Parses the boot sector and bootstraps the MFT runlist into `runs_out`.
pub fn mount(device: Device, partition_lba: u32, scratch: *Scratch, runs_out: []ntfs.Run) ?MountInfo {
    var boot_sector: [SECTOR_SIZE]u8 = undefined;
    if (!device.read_sectors(device.ctx, partition_lba, 1, boot_sector[0..])) return null;
    var boot: ntfs.BootSector = undefined;
    if (ntfs.BootSector.parse(boot_sector[0..], &boot) != .ok) return null;
    if (boot.bytes_per_sector != SECTOR_SIZE) return null;
    if (boot.file_record_bytes > scratch.record.len or boot.index_block_bytes > scratch.block.len) return null;
    if (checkedMulU64(boot.total_sectors, SECTOR_SIZE) == null) return null;
    const sectors_per_cluster = boot.cluster_bytes / SECTOR_SIZE;
    if (sectors_per_cluster == 0) return null;
    const total_clusters = boot.total_sectors / sectors_per_cluster;
    if (boot.mft_lcn >= total_clusters or boot.mftmirr_lcn >= total_clusters) return null;

    var probe_count: usize = 0;
    var probe = Volume{
        .device = device,
        .partition_lba = partition_lba,
        .cluster_bytes = boot.cluster_bytes,
        .record_bytes = boot.file_record_bytes,
        .index_block_bytes = boot.index_block_bytes,
        .total_sectors = boot.total_sectors,
        .mft_runs_buf = runs_out,
        .mft_run_count = &probe_count,
        .upcase = &[_]u8{},
        .scratch = scratch,
    };
    const record = scratch.record[0..boot.file_record_bytes];
    if (!readLcnBytes(&probe, boot.mft_lcn, 0, record)) return null;
    if (ntfs.applyFixups(record) != .ok) return null;
    const header = ntfs.FileRecordHeader.parse(record) orelse return null;
    if (!header.inUse() or header.record_number != ntfs.MFT_RECORD_MFT or
        !attributeStreamValid(record, header))
    {
        return null;
    }
    var geometry = AttributeGeometry{};
    const collected = &scratch.attr;
    collected.* = .{};
    var found_data = false;
    var attr_iter = ntfs.AttributeIterator.init(record, header);
    while (attr_iter.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.data) or
            attribute.name.len != 0)
        {
            continue;
        }
        if (!attribute.non_resident or
            !captureAttribute(&probe, attribute, collected, &geometry))
        {
            return null;
        }
        found_data = true;
    }
    if (!found_data or !geometry.complete() or
        collected.count == 0 or
        collected.count > runs_out.len)
    {
        return null;
    }
    @memcpy(runs_out[0..collected.count], collected.runs[0..collected.count]);
    const run_count = collected.count;
    return .{
        .cluster_bytes = boot.cluster_bytes,
        .record_bytes = boot.file_record_bytes,
        .index_block_bytes = boot.index_block_bytes,
        .total_sectors = boot.total_sectors,
        .mft_lcn = boot.mft_lcn,
        .mftmirr_lcn = boot.mftmirr_lcn,
        .mft_run_count = run_count,
    };
}

fn mftByteIo(v: *const Volume, byte_offset: u64, buffer: []u8, dir: ?IoDir) bool {
    if (v.mft_run_count.* > v.mft_runs_buf.len) return false;
    const runs = v.mft_runs_buf[0..v.mft_run_count.*];
    if (dir == null) return readRunBytes(v, runs, byte_offset, buffer);
    return writeRunBytes(v, runs, byte_offset, buffer);
}

/// Loads MFT record `number` into `buf`, removes fixups, parses the header.
pub fn loadRecord(v: *const Volume, number: u64, buf: []u8) ?ntfs.FileRecordHeader {
    if (v.record_bytes == 0 or v.record_bytes > buf.len) return null;
    const record = buf[0..v.record_bytes];
    const byte_offset = checkedMulU64(number, @as(u64, v.record_bytes)) orelse return null;
    const cached = if (v.metadata_cache) |cache| cache.lookupRecord(number, record) else false;
    if (!cached) {
        if (!mftByteIo(v, byte_offset, record, null)) return null;
        if (ntfs.applyFixups(record) != .ok) return null;
    }
    const header = ntfs.FileRecordHeader.parse(record) orelse return null;
    if (header.record_number != number) return null;
    if (!header.inUse()) return null;
    if (!cached) if (v.metadata_cache) |cache| cache.storeRecord(number, record);
    return header;
}

/// Loads a record without requiring the in-use flag (for allocation).
fn loadRecordRaw(v: *const Volume, number: u64, buf: []u8) bool {
    if (v.record_bytes == 0 or v.record_bytes > buf.len) return false;
    const record = buf[0..v.record_bytes];
    const byte_offset = checkedMulU64(number, @as(u64, v.record_bytes)) orelse return false;
    return mftByteIo(v, byte_offset, record, null);
}

/// Installs fresh fixups and writes record `number`; mirrors records 0-3
/// into $MFTMirr afterwards.
fn storeRecord(v: *const Volume, number: u64, buf: []u8) bool {
    if (v.record_bytes == 0 or v.record_bytes > buf.len) return false;
    const record = buf[0..v.record_bytes];
    const usn = if (record.len >= 0x32) readLe16(record, readLe16(record, 4)) else 0;
    if (ntfs.installFixups(record, usn) != .ok) return false;
    const byte_offset = checkedMulU64(number, @as(u64, v.record_bytes)) orelse return false;
    if (!mftByteIo(v, byte_offset, record, .write)) return false;
    if (number < 4) {
        if (!syncMirror(v)) return false;
    }
    // Leave the buffer fixed-up-removed for continued use.
    _ = ntfs.applyFixups(record);
    if (v.metadata_cache) |cache| cache.storeRecord(number, record);
    return true;
}

/// Copies the first four raw MFT records into $MFTMirr.
fn syncMirror(v: *const Volume) bool {
    const mirr = v.scratch.mirror_buf[0..];
    const four_u64 = checkedMulU64(@as(u64, v.record_bytes), 4) orelse return false;
    if (four_u64 > mirr.len) return false;
    const four: usize = @intCast(four_u64);
    if (!mftByteIo(v, 0, mirr[0..four], null)) return false;

    // $MFTMirr data location comes from record 1.
    const attr = &v.scratch.attr_mirror;
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFTMIRR, .data, &[_]u8{}, attr)) return false;
    if (attr.resident or attr.count == 0) return false;
    return writeRunBytes(v, attr.runs[0..attr.count], 0, mirr[0..four]);
}

fn readLe16(bytes: []const u8, offset: usize) u16 {
    return @as(u16, bytes[offset]) | (@as(u16, bytes[offset + 1]) << 8);
}

fn writeLe16(bytes: []u8, offset: usize, value: u16) void {
    bytes[offset] = @truncate(value);
    bytes[offset + 1] = @truncate(value >> 8);
}

fn readLe32(bytes: []const u8, offset: usize) u32 {
    return @as(u32, readLe16(bytes, offset)) | (@as(u32, readLe16(bytes, offset + 2)) << 16);
}

fn writeLe32(bytes: []u8, offset: usize, value: u32) void {
    writeLe16(bytes, offset, @truncate(value));
    writeLe16(bytes, offset + 2, @truncate(value >> 16));
}

fn readLe64(bytes: []const u8, offset: usize) u64 {
    return @as(u64, readLe32(bytes, offset)) | (@as(u64, readLe32(bytes, offset + 4)) << 32);
}

fn writeLe64(bytes: []u8, offset: usize, value: u64) void {
    writeLe32(bytes, offset, @truncate(value));
    writeLe32(bytes, offset + 4, @truncate(value >> 32));
}

// ---------------------------------------------------------------------------
// Attribute collection (read side)
// ---------------------------------------------------------------------------

const AttributeStorage = enum {
    none,
    resident,
    non_resident,
};

const AttributeGeometry = struct {
    storage: AttributeStorage = .none,
    first_extent_count: u8 = 0,
    next_vcn: u64 = 0,

    fn complete(self: AttributeGeometry) bool {
        return self.storage != .none and self.first_extent_count == 1;
    }
};

fn captureAttribute(
    v: *const Volume,
    attribute: ntfs.Attribute,
    out: *AttrScratch,
    geometry: *AttributeGeometry,
) bool {
    if (!attribute.non_resident) {
        // A resident value is the complete attribute: it may neither be
        // repeated nor mixed with non-resident extents.
        if (geometry.storage != .none) return false;
        if (attribute.value.len > out.resident_copy.len) return false;
        geometry.storage = .resident;
        geometry.first_extent_count = 1;
        out.resident = true;
        @memcpy(out.resident_copy[0..attribute.value.len], attribute.value);
        out.resident_len = attribute.value.len;
        out.data_size = attribute.value.len;
        out.initialized_size = attribute.value.len;
        out.alloc_size = (attribute.value.len + 7) & ~@as(u64, 7);
        out.flags = attribute.flags;
        return true;
    }

    if (geometry.storage == .resident) return false;
    const first_extent = geometry.storage == .none;
    if (first_extent) {
        if (attribute.lowest_vcn != 0) return false;
        geometry.storage = .non_resident;
        geometry.first_extent_count = 1;
    } else if (attribute.lowest_vcn == 0 or attribute.lowest_vcn != geometry.next_vcn) {
        // No duplicate first extent, overlap, reordering or VCN gap.
        return false;
    }
    if (attribute.highest_vcn < attribute.lowest_vcn) return false;
    const extent_clusters = checkedAddU64(
        attribute.highest_vcn - attribute.lowest_vcn,
        1,
    ) orelse return false;

    if (first_extent) {
        if (attribute.initialized_size > attribute.data_size) return false;
        out.data_size = attribute.data_size;
        out.initialized_size = attribute.initialized_size;
        out.alloc_size = attribute.allocated_size;
        out.flags = attribute.flags;
        out.compression_unit = attribute.compression_unit;
    }

    var extent_run_clusters: u64 = 0;
    var iterator = ntfs.RunlistIterator.init(attribute.mapping_pairs);
    while (iterator.next()) |run| {
        extent_run_clusters = checkedAddU64(extent_run_clusters, run.length_clusters) orelse return false;
        if (!mappedRunPhysicalRangeValid(v, run)) return false;
        if (out.count >= out.runs.len) return false;
        out.runs[out.count] = run;
        out.count += 1;
    }
    if (iterator.hadError()) return false;
    if (iterator.offset >= attribute.mapping_pairs.len or
        attribute.mapping_pairs[iterator.offset] != 0 or
        extent_run_clusters != extent_clusters)
    {
        return false;
    }
    geometry.next_vcn = checkedAddU64(attribute.highest_vcn, 1) orelse return false;
    return true;
}

/// Validates the complete raw attribute stream, including the terminating
/// END marker.  The format iterators intentionally stop defensively on
/// malformed input and therefore cannot by themselves distinguish
/// "absent" from "malformed".
fn attributeStreamValid(record: []const u8, header: ntfs.FileRecordHeader) bool {
    const limit: usize = @intCast(header.bytes_in_use);
    var offset: usize = header.attrs_offset;
    while (offset + 4 <= limit) {
        if (readLe32(record, offset) == ntfs.END_MARKER) return true;
        if (offset + 8 > limit) return false;
        const length: usize = @intCast(readLe32(record, offset + 4));
        if (length < 0x18 or length % 8 != 0 or offset + length > limit) return false;

        const non_resident = record[offset + 0x08] != 0;
        const name_units: usize = record[offset + 0x09];
        const name_offset: usize = readLe16(record, offset + 0x0A);
        if (name_units > 0 and
            (name_offset > length or name_units * 2 > length - name_offset))
        {
            return false;
        }

        if (non_resident) {
            if (length < 0x40) return false;
            const mapping_offset: usize = readLe16(record, offset + 0x20);
            if (mapping_offset < 0x40 or mapping_offset > length) return false;
        } else {
            const value_length: usize = @intCast(readLe32(record, offset + 0x10));
            const value_offset: usize = readLe16(record, offset + 0x14);
            if (value_offset < 0x18 or
                value_offset > length or
                value_length > length - value_offset)
            {
                return false;
            }
        }
        offset += length;
    }
    return false;
}

/// Collects all extents of one attribute, following $ATTRIBUTE_LIST.
/// Uses scratch.record and scratch.part_record.
pub fn collectAttributeStatus(v: *const Volume, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, out: *AttrScratch) LookupStatus {
    if (v.metadata_cache) |cache| {
        if (cache.lookupAttribute(record_number, attr_type, name_utf16, out)) return .found;
    }
    out.* = .{};
    const header = loadRecord(v, record_number, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    if (record_number > U32_MAX or !header.inUse() or
        header.record_number != @as(u32, @intCast(record_number)) or
        !attributeStreamValid(record, header))
    {
        return .io;
    }
    var geometry = AttributeGeometry{};

    if (ntfs.findAttribute(record, header, .attribute_list, &[_]u8{})) |list_attr| {
        if (list_attr.non_resident) return .io;
        var found_any = false;
        var listed_match = false;
        var iterator = ntfs.AttributeListIterator.init(list_attr.value);
        while (iterator.next()) |entry| {
            if (entry.attr_type != @intFromEnum(attr_type)) continue;
            if (!eqlBytes(entry.name, name_utf16)) continue;
            listed_match = true;
            const part_header = loadRecord(v, entry.mft_reference.record, v.scratch.part_record[0..]) orelse return .io;
            if (!part_header.inUse() or
                part_header.record_number != entry.mft_reference.record or
                part_header.sequence != entry.mft_reference.sequence)
            {
                return .io;
            }
            if (entry.mft_reference.record != record_number and
                (part_header.base_record.record != record_number or
                    part_header.base_record.sequence != header.sequence))
            {
                return .io;
            }
            const part_record = v.scratch.part_record[0..v.record_bytes];
            if (!attributeStreamValid(part_record, part_header)) return .io;
            var part_iter = ntfs.AttributeIterator.init(part_record, part_header);
            var found_part_count: usize = 0;
            while (part_iter.next()) |attribute| {
                if (attribute.attr_type != @intFromEnum(attr_type)) continue;
                if (!eqlBytes(attribute.name, name_utf16)) continue;
                if (attribute.instance != entry.instance) continue;
                if ((attribute.non_resident and attribute.lowest_vcn != entry.lowest_vcn) or
                    (!attribute.non_resident and entry.lowest_vcn != 0))
                {
                    return .io;
                }
                found_part_count += 1;
                if (found_part_count != 1 or
                    !captureAttribute(v, attribute, out, &geometry))
                {
                    return .io;
                }
                found_any = true;
            }
            if (found_part_count != 1) return .io;
        }
        if (iterator.offset != list_attr.value.len) return .io;
        if (listed_match and !found_any) return .io;
        if (found_any) {
            if (!geometry.complete()) return .io;
            if (v.metadata_cache) |cache| cache.storeAttribute(record_number, attr_type, name_utf16, out);
            return .found;
        }
    }

    // Without a matching $ATTRIBUTE_LIST entry, scan the base record rather
    // than accepting only the first hit. This makes a duplicate first
    // extent or a resident/non-resident mixture a visible metadata error.
    var direct_iter = ntfs.AttributeIterator.init(record, header);
    var found_direct = false;
    while (direct_iter.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(attr_type) or
            !eqlBytes(attribute.name, name_utf16))
        {
            continue;
        }
        if (!captureAttribute(v, attribute, out, &geometry)) return .io;
        found_direct = true;
    }
    if (!found_direct) return .not_found;
    if (!geometry.complete()) return .io;
    if (v.metadata_cache) |cache| cache.storeAttribute(record_number, attr_type, name_utf16, out);
    return .found;
}

/// Compatibility wrapper for callers that only need found/not-found.
pub fn collectAttribute(v: *const Volume, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, out: *AttrScratch) bool {
    return collectAttributeStatus(v, record_number, attr_type, name_utf16, out) == .found;
}

fn eqlBytes(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, 0..) |c, i| {
        if (c != b[i]) return false;
    }
    return true;
}

// ---------------------------------------------------------------------------
// Directory lookup and enumeration (read side)
// ---------------------------------------------------------------------------

pub const LookupResult = struct {
    record: u64,
    sequence: u16,
    entry: Entry,
    // Location of the index entry for in-place duplicate updates:
    in_root: bool = false,
    block_vcn: u64 = 0,
    entry_offset: usize = 0,
};

pub const MetadataCacheSummary = struct {
    version: u32 = METADATA_CACHE_VERSION,
    active_volumes: u32 = 0,
    slot_capacity: u32 = METADATA_CACHE_SLOT_CAPACITY,
    bytes_per_volume: u32 = 0,
    record_capacity: u32 = METADATA_RECORD_CAPACITY,
    attribute_capacity: u32 = METADATA_ATTRIBUTE_CAPACITY,
    index_capacity: u32 = METADATA_INDEX_CAPACITY,
    path_capacity: u32 = METADATA_PATH_CAPACITY,
    mount_generation: u64 = 0,
    content_generation: u64 = 0,
    negative_ttl_ticks: u64 = 0,
    record_entries: u32 = 0,
    attribute_entries: u32 = 0,
    index_entries: u32 = 0,
    path_entries: u32 = 0,
    record_hits: u64 = 0,
    record_misses: u64 = 0,
    record_stores: u64 = 0,
    record_evictions: u64 = 0,
    attribute_hits: u64 = 0,
    attribute_misses: u64 = 0,
    attribute_stores: u64 = 0,
    attribute_evictions: u64 = 0,
    index_hits: u64 = 0,
    index_misses: u64 = 0,
    index_stores: u64 = 0,
    index_evictions: u64 = 0,
    path_queries: u64 = 0,
    path_positive_hits: u64 = 0,
    path_negative_hits: u64 = 0,
    path_misses: u64 = 0,
    path_positive_stores: u64 = 0,
    path_negative_stores: u64 = 0,
    path_expirations: u64 = 0,
    lookup_tree_walks: u64 = 0,
    recovery_cache_bypasses: u64 = 0,
    mount_invalidations: u64 = 0,
    mutation_invalidations: u64 = 0,
    external_invalidations: u64 = 0,
    invalidated_entries: u64 = 0,
    reclaim_requests: u64 = 0,
    reclaim_scans: u64 = 0,
    reclaimed_entries: u64 = 0,
};

pub const MetadataReclaimResult = struct {
    requested_entries: u32 = 0,
    inspected_entries: u32 = 0,
    reclaimed_entries: u32 = 0,
};

const RecordCacheEntry = struct {
    valid: bool = false,
    generation: u64 = 0,
    last_used: u64 = 0,
    number: u64 = 0,
    byte_len: u16 = 0,
    bytes: [4096]u8 = undefined,
};

const AttributeCacheEntry = struct {
    valid: bool = false,
    generation: u64 = 0,
    last_used: u64 = 0,
    record_number: u64 = 0,
    attr_type: u32 = 0,
    name_len: u16 = 0,
    name: [NAME_UNITS_MAX * 2]u8 = undefined,
    runs: [MAX_ATTR_RUNS]ntfs.Run = undefined,
    count: u16 = 0,
    data_size: u64 = 0,
    initialized_size: u64 = 0,
    alloc_size: u64 = 0,
    flags: u16 = 0,
    compression_unit: u8 = 0,
    resident: bool = false,
    resident_len: u16 = 0,
    resident_copy: [1024]u8 = undefined,
};

const IndexCacheEntry = struct {
    valid: bool = false,
    generation: u64 = 0,
    last_used: u64 = 0,
    directory_record: u64 = 0,
    vcn: u64 = 0,
    byte_len: u16 = 0,
    bytes: [4096]u8 = undefined,
};

const PathCacheEntry = struct {
    valid: bool = false,
    generation: u64 = 0,
    last_used: u64 = 0,
    directory_record: u64 = 0,
    name_len: u16 = 0,
    name_utf16: [NAME_UNITS_MAX * 2]u8 = undefined,
    negative: bool = false,
    expires_at: u64 = 0,
    result: LookupResult = undefined,
};

const PathCacheLookup = enum {
    miss,
    positive,
    negative,
};

/// Fixed, caller-owned cache of decoded NTFS metadata. It never owns sector
/// payloads and never grows: every replacement and reclaim pass is bounded by
/// the capacities above. The filesystem request owner serializes mutations;
/// cache operations themselves neither allocate nor wait.
pub const MetadataCache = struct {
    records: [METADATA_RECORD_CAPACITY]RecordCacheEntry = .{RecordCacheEntry{}} ** METADATA_RECORD_CAPACITY,
    attributes: [METADATA_ATTRIBUTE_CAPACITY]AttributeCacheEntry = .{AttributeCacheEntry{}} ** METADATA_ATTRIBUTE_CAPACITY,
    indices: [METADATA_INDEX_CAPACITY]IndexCacheEntry = .{IndexCacheEntry{}} ** METADATA_INDEX_CAPACITY,
    paths: [METADATA_PATH_CAPACITY]PathCacheEntry = .{PathCacheEntry{}} ** METADATA_PATH_CAPACITY,
    generation: u64 = 1,
    mount_generation: u64 = 0,
    negative_ttl_ticks: u64 = 0,
    access_clock: u64 = 0,
    reclaim_cursor: usize = 0,
    counters: MetadataCacheSummary = .{},

    pub fn beginMount(self: *MetadataCache, negative_ttl_ticks: u64) void {
        const invalidated = self.clearEntries();
        self.mount_generation = nextGeneration(self.mount_generation);
        self.generation = nextGeneration(self.generation);
        self.negative_ttl_ticks = negative_ttl_ticks;
        self.counters.mount_invalidations +%= 1;
        self.counters.invalidated_entries +%= invalidated;
    }

    pub fn invalidateExternal(self: *MetadataCache) void {
        const invalidated = self.clearEntries();
        self.generation = nextGeneration(self.generation);
        self.counters.external_invalidations +%= 1;
        self.counters.invalidated_entries +%= invalidated;
    }

    fn invalidateMutation(self: *MetadataCache) void {
        const invalidated = self.clearEntries();
        if (invalidated == 0) return;
        self.generation = nextGeneration(self.generation);
        self.counters.mutation_invalidations +%= 1;
        self.counters.invalidated_entries +%= invalidated;
    }

    fn touch(self: *MetadataCache) u64 {
        self.access_clock = nextGeneration(self.access_clock);
        return self.access_clock;
    }

    fn lookupRecord(self: *MetadataCache, number: u64, out: []u8) bool {
        for (&self.records) |*entry| {
            if (!entry.valid or entry.generation != self.generation or
                entry.number != number or entry.byte_len != out.len)
            {
                continue;
            }
            @memcpy(out, entry.bytes[0..out.len]);
            entry.last_used = self.touch();
            self.counters.record_hits +%= 1;
            return true;
        }
        self.counters.record_misses +%= 1;
        return false;
    }

    fn storeRecord(self: *MetadataCache, number: u64, bytes: []const u8) void {
        if (bytes.len == 0 or bytes.len > 4096) return;
        var target: usize = 0;
        var oldest = ~@as(u64, 0);
        for (self.records, 0..) |entry, index| {
            if (!entry.valid or entry.generation != self.generation) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.number == number) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.last_used < oldest) {
                oldest = entry.last_used;
                target = index;
            }
        }
        var entry = &self.records[target];
        if (oldest != 0 and entry.valid) self.counters.record_evictions +%= 1;
        entry.valid = true;
        entry.generation = self.generation;
        entry.last_used = self.touch();
        entry.number = number;
        entry.byte_len = @intCast(bytes.len);
        @memcpy(entry.bytes[0..bytes.len], bytes);
        self.counters.record_stores +%= 1;
    }

    fn lookupAttribute(
        self: *MetadataCache,
        record_number: u64,
        attr_type: ntfs.AttrType,
        name_utf16: []const u8,
        out: *AttrScratch,
    ) bool {
        if (name_utf16.len > NAME_UNITS_MAX * 2) return false;
        for (&self.attributes) |*entry| {
            if (!entry.valid or entry.generation != self.generation or
                entry.record_number != record_number or
                entry.attr_type != @intFromEnum(attr_type) or
                entry.name_len != name_utf16.len or
                !eqlBytes(entry.name[0..entry.name_len], name_utf16))
            {
                continue;
            }
            out.* = .{};
            out.count = entry.count;
            if (entry.count != 0) @memcpy(out.runs[0..entry.count], entry.runs[0..entry.count]);
            out.data_size = entry.data_size;
            out.initialized_size = entry.initialized_size;
            out.alloc_size = entry.alloc_size;
            out.flags = entry.flags;
            out.compression_unit = entry.compression_unit;
            out.resident = entry.resident;
            out.resident_len = entry.resident_len;
            if (entry.resident_len != 0) {
                @memcpy(out.resident_copy[0..entry.resident_len], entry.resident_copy[0..entry.resident_len]);
            }
            entry.last_used = self.touch();
            self.counters.attribute_hits +%= 1;
            return true;
        }
        self.counters.attribute_misses +%= 1;
        return false;
    }

    fn storeAttribute(
        self: *MetadataCache,
        record_number: u64,
        attr_type: ntfs.AttrType,
        name_utf16: []const u8,
        value: *const AttrScratch,
    ) void {
        if (name_utf16.len > NAME_UNITS_MAX * 2 or value.count > MAX_ATTR_RUNS or
            value.resident_len > 1024)
        {
            return;
        }
        var target: usize = 0;
        var oldest = ~@as(u64, 0);
        for (self.attributes, 0..) |entry, index| {
            if (!entry.valid or entry.generation != self.generation) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.record_number == record_number and
                entry.attr_type == @intFromEnum(attr_type) and
                entry.name_len == name_utf16.len and
                eqlBytes(entry.name[0..entry.name_len], name_utf16))
            {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.last_used < oldest) {
                oldest = entry.last_used;
                target = index;
            }
        }
        var entry = &self.attributes[target];
        if (oldest != 0 and entry.valid) self.counters.attribute_evictions +%= 1;
        entry.valid = true;
        entry.generation = self.generation;
        entry.last_used = self.touch();
        entry.record_number = record_number;
        entry.attr_type = @intFromEnum(attr_type);
        entry.name_len = @intCast(name_utf16.len);
        if (name_utf16.len != 0) @memcpy(entry.name[0..name_utf16.len], name_utf16);
        entry.count = @intCast(value.count);
        if (value.count != 0) @memcpy(entry.runs[0..value.count], value.runs[0..value.count]);
        entry.data_size = value.data_size;
        entry.initialized_size = value.initialized_size;
        entry.alloc_size = value.alloc_size;
        entry.flags = value.flags;
        entry.compression_unit = value.compression_unit;
        entry.resident = value.resident;
        entry.resident_len = @intCast(value.resident_len);
        if (value.resident_len != 0) {
            @memcpy(entry.resident_copy[0..value.resident_len], value.resident_copy[0..value.resident_len]);
        }
        self.counters.attribute_stores +%= 1;
    }

    fn lookupIndex(self: *MetadataCache, directory_record: u64, vcn: u64, out: []u8) bool {
        for (&self.indices) |*entry| {
            if (!entry.valid or entry.generation != self.generation or
                entry.directory_record != directory_record or entry.vcn != vcn or
                entry.byte_len != out.len)
            {
                continue;
            }
            @memcpy(out, entry.bytes[0..out.len]);
            entry.last_used = self.touch();
            self.counters.index_hits +%= 1;
            return true;
        }
        self.counters.index_misses +%= 1;
        return false;
    }

    fn storeIndex(self: *MetadataCache, directory_record: u64, vcn: u64, bytes: []const u8) void {
        if (bytes.len == 0 or bytes.len > 4096) return;
        var target: usize = 0;
        var oldest = ~@as(u64, 0);
        for (self.indices, 0..) |entry, index| {
            if (!entry.valid or entry.generation != self.generation) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.directory_record == directory_record and entry.vcn == vcn) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.last_used < oldest) {
                oldest = entry.last_used;
                target = index;
            }
        }
        var entry = &self.indices[target];
        if (oldest != 0 and entry.valid) self.counters.index_evictions +%= 1;
        entry.valid = true;
        entry.generation = self.generation;
        entry.last_used = self.touch();
        entry.directory_record = directory_record;
        entry.vcn = vcn;
        entry.byte_len = @intCast(bytes.len);
        @memcpy(entry.bytes[0..bytes.len], bytes);
        self.counters.index_stores +%= 1;
    }

    fn lookupPath(
        self: *MetadataCache,
        upcase: []const u8,
        directory_record: u64,
        name_utf16: []const u8,
        now_ticks: u64,
        out: *LookupResult,
    ) PathCacheLookup {
        self.counters.path_queries +%= 1;
        for (&self.paths) |*entry| {
            if (!entry.valid or entry.generation != self.generation or
                entry.directory_record != directory_record or
                ntfs.compareFileNames(upcase, entry.name_utf16[0..entry.name_len], name_utf16) != .eq)
            {
                continue;
            }
            if (entry.negative and now_ticks >= entry.expires_at) {
                entry.valid = false;
                self.counters.path_expirations +%= 1;
                continue;
            }
            entry.last_used = self.touch();
            if (entry.negative) {
                self.counters.path_negative_hits +%= 1;
                return .negative;
            }
            out.* = entry.result;
            self.counters.path_positive_hits +%= 1;
            return .positive;
        }
        self.counters.path_misses +%= 1;
        return .miss;
    }

    fn storePath(
        self: *MetadataCache,
        directory_record: u64,
        name_utf16: []const u8,
        status: LookupStatus,
        value: *const LookupResult,
        now_ticks: u64,
    ) void {
        if (name_utf16.len == 0 or name_utf16.len > NAME_UNITS_MAX * 2 or status == .io) return;
        if (status == .not_found and self.negative_ttl_ticks == 0) return;
        var target: usize = 0;
        var oldest = ~@as(u64, 0);
        for (self.paths, 0..) |entry, index| {
            if (!entry.valid or entry.generation != self.generation) {
                target = index;
                oldest = 0;
                break;
            }
            if (entry.last_used < oldest) {
                oldest = entry.last_used;
                target = index;
            }
        }
        var entry = &self.paths[target];
        entry.valid = true;
        entry.generation = self.generation;
        entry.last_used = self.touch();
        entry.directory_record = directory_record;
        entry.name_len = @intCast(name_utf16.len);
        @memcpy(entry.name_utf16[0..name_utf16.len], name_utf16);
        entry.negative = status == .not_found;
        entry.expires_at = if (entry.negative)
            saturatingAdd(now_ticks, self.negative_ttl_ticks)
        else
            0;
        if (status == .found) {
            entry.result = value.*;
            self.counters.path_positive_stores +%= 1;
        } else {
            self.counters.path_negative_stores +%= 1;
        }
    }

    fn noteTreeWalk(self: *MetadataCache) void {
        self.counters.lookup_tree_walks +%= 1;
    }

    fn noteRecoveryBypass(self: *MetadataCache) void {
        self.counters.recovery_cache_bypasses +%= 1;
    }

    pub fn reclaim(self: *MetadataCache, requested_entries_raw: u32) MetadataReclaimResult {
        const requested_entries = @min(requested_entries_raw, @as(u32, METADATA_CACHE_SLOT_CAPACITY));
        var result = MetadataReclaimResult{ .requested_entries = requested_entries };
        self.counters.reclaim_requests +%= 1;
        while (result.inspected_entries < METADATA_CACHE_SLOT_CAPACITY and
            result.reclaimed_entries < requested_entries)
        {
            const slot = self.reclaim_cursor;
            self.reclaim_cursor = (self.reclaim_cursor + 1) % METADATA_CACHE_SLOT_CAPACITY;
            result.inspected_entries += 1;
            if (self.clearSlot(slot)) result.reclaimed_entries += 1;
        }
        self.counters.reclaim_scans +%= result.inspected_entries;
        self.counters.reclaimed_entries +%= result.reclaimed_entries;
        return result;
    }

    pub fn summary(self: *const MetadataCache) MetadataCacheSummary {
        var out = self.counters;
        out.version = METADATA_CACHE_VERSION;
        out.active_volumes = 1;
        out.slot_capacity = METADATA_CACHE_SLOT_CAPACITY;
        out.bytes_per_volume = @sizeOf(MetadataCache);
        out.record_capacity = METADATA_RECORD_CAPACITY;
        out.attribute_capacity = METADATA_ATTRIBUTE_CAPACITY;
        out.index_capacity = METADATA_INDEX_CAPACITY;
        out.path_capacity = METADATA_PATH_CAPACITY;
        out.mount_generation = self.mount_generation;
        out.content_generation = self.generation;
        out.negative_ttl_ticks = self.negative_ttl_ticks;
        for (self.records) |entry| if (entry.valid) {
            out.record_entries += 1;
        };
        for (self.attributes) |entry| if (entry.valid) {
            out.attribute_entries += 1;
        };
        for (self.indices) |entry| if (entry.valid) {
            out.index_entries += 1;
        };
        for (self.paths) |entry| if (entry.valid) {
            out.path_entries += 1;
        };
        return out;
    }

    fn clearEntries(self: *MetadataCache) u32 {
        var cleared: u32 = 0;
        for (&self.records) |*entry| if (entry.valid) {
            entry.valid = false;
            cleared += 1;
        };
        for (&self.attributes) |*entry| if (entry.valid) {
            entry.valid = false;
            cleared += 1;
        };
        for (&self.indices) |*entry| if (entry.valid) {
            entry.valid = false;
            cleared += 1;
        };
        for (&self.paths) |*entry| if (entry.valid) {
            entry.valid = false;
            cleared += 1;
        };
        return cleared;
    }

    fn clearSlot(self: *MetadataCache, slot: usize) bool {
        if (slot < METADATA_RECORD_CAPACITY) {
            const entry = &self.records[slot];
            if (!entry.valid) return false;
            entry.valid = false;
            return true;
        }
        var relative = slot - METADATA_RECORD_CAPACITY;
        if (relative < METADATA_ATTRIBUTE_CAPACITY) {
            const entry = &self.attributes[relative];
            if (!entry.valid) return false;
            entry.valid = false;
            return true;
        }
        relative -= METADATA_ATTRIBUTE_CAPACITY;
        if (relative < METADATA_INDEX_CAPACITY) {
            const entry = &self.indices[relative];
            if (!entry.valid) return false;
            entry.valid = false;
            return true;
        }
        relative -= METADATA_INDEX_CAPACITY;
        const entry = &self.paths[relative];
        if (!entry.valid) return false;
        entry.valid = false;
        return true;
    }
};

fn nextGeneration(current: u64) u64 {
    const next = current +% 1;
    return if (next == 0) 1 else next;
}

fn saturatingAdd(a: u64, b: u64) u64 {
    return if (b > ~@as(u64, 0) - a) ~@as(u64, 0) else a + b;
}

fn entryFromFileName(reference: ntfs.FileReference, file_name: ntfs.FileName) ?Entry {
    var entry = Entry{
        .record = reference.record,
        .sequence = reference.sequence,
        .size = file_name.data_size,
    };
    // Real UTF-8 names since 0.60.10; names over NAME_MAX UTF-8 bytes (or
    // outside the BMP) stay visibly unresolvable.
    entry.name_len = ntfs.utf16ToUtf8(file_name.name, entry.name[0..]) orelse return null;

    var attr: u8 = 0;
    if ((file_name.flags & ntfs.FILE_ATTR_READ_ONLY) != 0) attr |= ATTR_READ_ONLY;
    if ((file_name.flags & ntfs.FILE_ATTR_HIDDEN) != 0) attr |= ATTR_HIDDEN;
    if ((file_name.flags & ntfs.FILE_ATTR_SYSTEM) != 0) attr |= ATTR_SYSTEM;
    if ((file_name.flags & ntfs.FILE_ATTR_ARCHIVE) != 0) attr |= ATTR_ARCHIVE;
    if ((file_name.flags & ntfs.FILE_ATTR_DIRECTORY_DUP) != 0) {
        attr |= ATTR_DIRECTORY;
        entry.size = 0;
    }
    entry.reparse = (file_name.flags & ntfs.FILE_ATTR_REPARSE) != 0;
    entry.attr = attr;
    entry.created_time_nt = file_name.creation_time;
    entry.modified_time_nt = file_name.data_change_time;
    entry.access_time_nt = file_name.access_time;
    return entry;
}

/// Loads one INDX block of the directory whose $INDEX_ALLOCATION extents
/// are in `alloc_runs` into scratch.block.
fn loadIndexBlock(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64) bool {
    if (v.cluster_bytes > v.index_block_bytes) return false;
    const block = v.scratch.block[0..v.index_block_bytes];
    const byte_offset = clusterByteOffset(v, vcn) orelse return false;
    if (!readRunBytes(v, alloc_runs, byte_offset, block)) return false;
    return ntfs.applyFixups(block) == .ok;
}

/// Read-side INDX cache. Mutation helpers deliberately keep using the direct
/// loader because their scratch blocks are changed in place and every write
/// invalidates the shared metadata generation before publication.
fn loadIndexBlockCached(v: *const Volume, directory_record: u64, alloc_runs: []const ntfs.Run, vcn: u64) bool {
    const block = v.scratch.block[0..v.index_block_bytes];
    if (v.metadata_cache) |cache| {
        if (cache.lookupIndex(directory_record, vcn, block)) return true;
    }
    if (!loadIndexBlock(v, alloc_runs, vcn)) return false;
    if (v.metadata_cache) |cache| cache.storeIndex(directory_record, vcn, block);
    return true;
}

fn storeIndexBlock(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64) bool {
    const block = v.scratch.block[0..v.index_block_bytes];
    const usn = readLe16(block, readLe16(block, 4));
    if (ntfs.installFixups(block, usn) != .ok) return false;
    const byte_offset = clusterByteOffset(v, vcn) orelse return false;
    if (!writeRunBytes(v, alloc_runs, byte_offset, block)) return false;
    _ = ntfs.applyFixups(block);
    return true;
}

/// Windows hides the filesystem's own metadata records ($MFT .. $Extend,
/// records 0-15, including the root's own "." entry) from every listing
/// and from name-based access; user files always live at record >=
/// MFT_FIRST_NORMAL (16) -- the same bound the record allocator starts
/// at, so no user file can ever fall into the hidden range.  R4OS
/// mirrors that rule (0.60.13): the root enumeration skips these entries
/// and a path lookup on the system names reports not-found, so no
/// generic read/write/delete path can touch a metadata record by name.
fn isHiddenSystemEntry(dir_record: u64, child_record: u64) bool {
    return dir_record == ntfs.MFT_RECORD_ROOT and child_record < ntfs.MFT_FIRST_NORMAL;
}

/// Descends the $I30 tree of `dir_record`.  Only a valid terminating END
/// entry or a valid collation branch without a child proves absence.
pub fn lookupInDirectoryStatus(v: *const Volume, dir_record: u64, name: []const u8, out: *LookupResult) LookupStatus {
    return lookupInDirectoryStatusMode(v, dir_record, name, out, false, 0);
}

/// Recovery-only lookup for alias-first rename/publish operations. During a
/// publish-before-detach sequence an already durable index alias may
/// temporarily disagree with the single canonical $FILE_NAME. Generic VFS
/// lookups must never accept that half-state.
pub fn lookupInDirectoryStatusTransient(v: *const Volume, dir_record: u64, name: []const u8, out: *LookupResult) LookupStatus {
    return lookupInDirectoryStatusMode(v, dir_record, name, out, true, 0);
}

// Hardware diagnosis for transient path disappearances. The low 12 bits
// identify path depth and the exact not-found branch; the high 20 bits are
// a generation so a probe can distinguish a new failure from an older one.
// Successful lookups intentionally do not clear the last failure.
pub var last_lookup_diagnostic_stage: u32 = 0;
var lookup_diagnostic_sequence: u32 = 0;

pub fn lookupDiagnosticStage() u32 {
    return last_lookup_diagnostic_stage;
}

fn recordLookupNotFound(path_depth: u32, phase: u32) LookupStatus {
    const bounded_depth = @min(path_depth, @as(u32, MAX_PATH_DEPTH));
    const code: u32 = @as(u32, bounded_depth) * @as(u32, 100) + phase;
    lookup_diagnostic_sequence = (lookup_diagnostic_sequence +% 1) & 0x000F_FFFF;
    if (lookup_diagnostic_sequence == 0) lookup_diagnostic_sequence = 1;
    last_lookup_diagnostic_stage = (lookup_diagnostic_sequence << 12) | (code & 0x0000_0FFF);
    return .not_found;
}

fn lookupInDirectoryStatusMode(
    v: *const Volume,
    dir_record: u64,
    name: []const u8,
    out: *LookupResult,
    allow_transient_alias: bool,
    diagnostic_depth: u32,
) LookupStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .io;
    const target_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return .io;
    const target = v.scratch.name_utf16[0..target_len];

    if (v.metadata_cache) |cache| {
        if (allow_transient_alias) {
            cache.noteRecoveryBypass();
        } else {
            switch (cache.lookupPath(v.upcase, dir_record, target, v.metadata_cache_now_ticks, out)) {
                .positive => return .found,
                .negative => return recordLookupNotFound(diagnostic_depth, 10),
                .miss => {},
            }
            cache.noteTreeWalk();
        }
    }
    const status = lookupInDirectoryStatusUncached(
        v,
        dir_record,
        target,
        out,
        allow_transient_alias,
        diagnostic_depth,
    );
    if (!allow_transient_alias) if (v.metadata_cache) |cache| {
        cache.storePath(dir_record, target, status, out, v.metadata_cache_now_ticks);
    };
    return status;
}

fn lookupInDirectoryStatusUncached(
    v: *const Volume,
    dir_record: u64,
    target: []const u8,
    out: *LookupResult,
    allow_transient_alias: bool,
    diagnostic_depth: u32,
) LookupStatus {
    const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
    if (dir_record > U32_MAX or !header.inUse() or
        header.record_number != @as(u32, @intCast(dir_record)) or
        !header.isDirectory())
    {
        return .io;
    }
    const record = v.scratch.record[0..v.record_bytes];
    if (!attributeStreamValid(record, header)) return .io;
    const root_attr = ntfs.findAttribute(record, header, .index_root, &ntfs.I30_NAME_UTF16) orelse return .io;
    const index_root = ntfs.IndexRoot.parse(root_attr.value) orelse return .io;
    if (index_root.indexed_attr_type != @intFromEnum(ntfs.AttrType.file_name) or
        index_root.collation_rule != ntfs.COLLATION_FILE_NAME)
    {
        return .io;
    }

    var alloc = AttrScratch{};
    var have_allocation = false;
    if (index_root.header.hasSubNodes()) {
        if (collectAttributeStatus(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, &alloc) != .found) return .io;
        if (alloc.resident or alloc.count == 0) return .io;
        have_allocation = true;
        // collectAttribute reloaded scratch.record with the same record;
        // re-derive the root slice.
        const header2 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
        if (!attributeStreamValid(record, header2)) return .io;
    }
    const root_attr2 = blk: {
        const h = ntfs.FileRecordHeader.parse(record) orelse return .io;
        break :blk ntfs.findAttribute(record, h, .index_root, &ntfs.I30_NAME_UTF16) orelse return .io;
    };
    const root2 = ntfs.IndexRoot.parse(root_attr2.value) orelse return .io;

    var node_entries: []const u8 = root2.entries;
    var in_root = true;
    var current_vcn: u64 = 0;
    var depth: usize = 0;
    while (depth < MAX_INDEX_DEPTH) : (depth += 1) {
        var offset: usize = 0;
        var next_vcn: ?u64 = null;
        var route_decided = false;
        var iterator = ntfs.IndexEntryIterator.init(node_entries);
        while (iterator.next()) |entry| {
            if (entry.isEnd()) {
                if (entry.key_length != 0) return .io;
                route_decided = true;
                if (entry.hasSubNode()) next_vcn = entry.sub_node_vcn;
                if (next_vcn == null) return recordLookupNotFound(diagnostic_depth, 20);
                break;
            }
            const file_name = entry.fileName() orelse return .io;
            if (file_name.parent.record != dir_record or
                file_name.parent.sequence != header.sequence)
            {
                return .io;
            }
            const order = ntfs.compareFileNames(v.upcase, target, file_name.name);
            if (order == .eq) {
                const reference = ntfs.FileReference.parse(entry.file_reference);
                if (isHiddenSystemEntry(dir_record, reference.record)) return recordLookupNotFound(diagnostic_depth, 22);
                const child_header = loadRecord(v, reference.record, v.scratch.part_record[0..]) orelse return .io;
                if (reference.record > U32_MAX or
                    !child_header.inUse() or
                    child_header.record_number != @as(u32, @intCast(reference.record)) or
                    child_header.sequence != reference.sequence or
                    child_header.isDirectory() != ((file_name.flags & ntfs.FILE_ATTR_DIRECTORY_DUP) != 0) or
                    !attributeStreamValid(v.scratch.part_record[0..v.record_bytes], child_header))
                {
                    return .io;
                }
                if (!allow_transient_alias and !recordHasFileNameLink(
                    v,
                    v.scratch.part_record[0..v.record_bytes],
                    child_header,
                    dir_record,
                    header.sequence,
                    target,
                )) return .io;
                const built = entryFromFileName(reference, file_name) orelse return .io;
                out.* = .{
                    .record = reference.record,
                    .sequence = reference.sequence,
                    .entry = built,
                    .in_root = in_root,
                    .block_vcn = current_vcn,
                    .entry_offset = offset,
                };
                return .found;
            }
            if (order == .lt) {
                route_decided = true;
                if (entry.hasSubNode()) next_vcn = entry.sub_node_vcn;
                if (next_vcn == null) return recordLookupNotFound(diagnostic_depth, 21);
                break;
            }
            offset += entry.entry_length;
        }
        // Iterator exhaustion without an END entry is malformed metadata.
        if (!route_decided) return .io;
        const vcn = next_vcn orelse return .io;
        if (!have_allocation) return .io;
        if (!loadIndexBlockCached(v, dir_record, alloc.runs[0..alloc.count], vcn)) return .io;
        const block = ntfs.IndexBlock.parse(v.scratch.block[0..v.index_block_bytes]) orelse return .io;
        if (block.vcn != vcn) return .io;
        node_entries = block.entries;
        in_root = false;
        current_vcn = vcn;
    }
    // A routed tree deeper than the supported bound is corruption/unsupported
    // geometry, not proof that the name is absent.
    return .io;
}

fn recordHasFileNameLink(
    v: *const Volume,
    record: []const u8,
    header: ntfs.FileRecordHeader,
    parent_record: u64,
    parent_sequence: u16,
    wanted_name: []const u8,
) bool {
    var iterator = ntfs.AttributeIterator.init(record, header);
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.file_name)) continue;
        const file_name = ntfs.FileName.parse(attribute.value) orelse return false;
        if (file_name.parent.record == parent_record and
            file_name.parent.sequence == parent_sequence and
            eqlUtf16Upcase(v, file_name.name, wanted_name))
        {
            return true;
        }
    }
    return false;
}

/// Compatibility wrapper retaining the original Optional surface.
pub fn lookupInDirectory(v: *const Volume, dir_record: u64, name: []const u8) ?LookupResult {
    var result: LookupResult = undefined;
    return if (lookupInDirectoryStatus(v, dir_record, name, &result) == .found) result else null;
}

pub fn resolvePathStatus(v: *const Volume, path: []const u8, out: *u64) LookupStatus {
    var record: u64 = ntfs.MFT_RECORD_ROOT;
    var start: usize = 0;
    var depth: usize = 0;
    while (start < path.len) {
        while (start < path.len and (path[start] == '\\' or path[start] == '/')) : (start += 1) {}
        if (start >= path.len) break;
        var end = start;
        while (end < path.len and path[end] != '\\' and path[end] != '/') : (end += 1) {}
        const segment = path[start..end];
        if (segment.len != 0) {
            depth += 1;
            if (depth > MAX_PATH_DEPTH) return .io;
            var found: LookupResult = undefined;
            const status = lookupInDirectoryStatusMode(v, record, segment, &found, false, @intCast(depth));
            if (status != .found) return status;
            if (!found.entry.isDir()) return recordLookupNotFound(@intCast(depth), 90);
            // Junctions/symlinks are visible rejections, never traversed.
            if (found.entry.reparse) return .io;
            record = found.record;
        }
        start = end;
    }
    out.* = record;
    return .found;
}

pub fn resolvePath(v: *const Volume, path: []const u8) ?u64 {
    var record: u64 = undefined;
    return if (resolvePathStatus(v, path, &record) == .found) record else null;
}

pub const ResolvedEntry = struct {
    entry: Entry,
    parent_record: u64,
};

pub fn resolveEntryStatus(v: *const Volume, path: []const u8, out: *ResolvedEntry) LookupStatus {
    var dir_record: u64 = ntfs.MFT_RECORD_ROOT;
    var result: ?Entry = null;
    var parent: u64 = ntfs.MFT_RECORD_ROOT;
    var start: usize = 0;
    var depth: usize = 0;
    while (start < path.len) {
        while (start < path.len and (path[start] == '\\' or path[start] == '/')) : (start += 1) {}
        if (start >= path.len) break;
        var end = start;
        while (end < path.len and path[end] != '\\' and path[end] != '/') : (end += 1) {}
        const segment = path[start..end];
        if (segment.len != 0) {
            depth += 1;
            if (depth > MAX_PATH_DEPTH) return .io;
            if (result) |r| {
                if (!r.isDir()) return recordLookupNotFound(@intCast(depth), 90);
                if (r.reparse) return .io;
                dir_record = r.record;
            }
            var found: LookupResult = undefined;
            const status = lookupInDirectoryStatusMode(v, dir_record, segment, &found, false, @intCast(depth));
            if (status != .found) return status;
            parent = dir_record;
            result = found.entry;
        }
        start = end;
    }
    if (result) |*entry| {
        if (!entry.isDir() and !entry.reparse) {
            var attr = AttrScratch{};
            if (collectAttributeStatus(v, entry.record, .data, &[_]u8{}, &attr) != .found) return .io;
            entry.size = attr.data_size;
        }
        out.* = .{ .entry = entry.*, .parent_record = parent };
        return .found;
    }
    return recordLookupNotFound(0, 91);
}

pub fn resolveEntry(v: *const Volume, path: []const u8) ?ResolvedEntry {
    var result: ResolvedEntry = undefined;
    return if (resolveEntryStatus(v, path, &result) == .found) result else null;
}

pub const RecordName = struct {
    parent: u64,
    parent_sequence: u16,
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
};

/// Reads the parent directory reference and ASCII name from a record's
/// $FILE_NAME attribute (first non-DOS name).  A record with multiple hard
/// links has no unique parent/name and is deliberately rejected.
pub fn recordParentAndName(v: *const Volume, record_number: u64) ?RecordName {
    var link_info: RecordLinkInfo = undefined;
    if (recordLinkInfoStatus(v, record_number, &link_info) != .found or link_info.link_count != 1) return null;
    const header = loadRecord(v, record_number, v.scratch.record[0..]) orelse return null;
    if (header.sequence != link_info.sequence) return null;
    const record = v.scratch.record[0..v.record_bytes];
    var iterator = ntfs.AttributeIterator.init(record, header);
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.file_name)) continue;
        const fn_value = ntfs.FileName.parse(attribute.value) orelse continue;
        if (fn_value.namespace == ntfs.NAMESPACE_DOS) continue;
        var out = RecordName{
            .parent = fn_value.parent.record,
            .parent_sequence = fn_value.parent.sequence,
        };
        out.name_len = ntfs.utf16ToUtf8(fn_value.name, out.name[0..]) orelse return null;
        if (recordIdentityStatus(v, out.parent, out.parent_sequence, true) != .found) return null;
        return out;
    }
    return null;
}

// ---------------------------------------------------------------------------
// Enumeration (read side)
// ---------------------------------------------------------------------------

pub const EnumSink = struct {
    out: ?[]u8 = null,
    cursor: usize = 0,
    listed: usize = 0,
    max_entries: usize = 0,
    wanted: usize = 0,
    seen: usize = 0,
    found: ?Entry = null,
    failed: bool = false,
    /// Alias-count mode (0.60.21): when set to a packed FileReference the
    /// walk only counts matching index entries.  It never lists and never
    /// selects, so the traversal always runs to completion.
    count_ref: ?u64 = null,
    matches: usize = 0,
};

fn appendBytes(out: []u8, cursor: *usize, bytes: []const u8) bool {
    if (cursor.* + bytes.len >= out.len) return false;
    @memcpy(out[cursor.* .. cursor.* + bytes.len], bytes);
    cursor.* += bytes.len;
    return true;
}

fn emitEntry(sink: *EnumSink, entry: Entry) void {
    if (sink.count_ref) |wanted| {
        if (ntfs.FileReference.pack(.{ .record = entry.record, .sequence = entry.sequence }) == wanted) {
            sink.matches += 1;
        }
        return;
    }
    if (sink.out) |out| {
        if (entry.isDir()) {
            if (!appendBytes(out, &sink.cursor, "<DIR> ")) {
                sink.failed = true;
                return;
            }
        } else {
            if (!appendBytes(out, &sink.cursor, "      ")) {
                sink.failed = true;
                return;
            }
        }
        if (!appendBytes(out, &sink.cursor, entry.name[0..entry.name_len]) or
            !appendBytes(out, &sink.cursor, "\r\n"))
        {
            sink.failed = true;
            return;
        }
        sink.listed += 1;
        return;
    }
    if (sink.seen == sink.wanted) {
        sink.found = entry;
        return;
    }
    sink.seen += 1;
}

fn walkNode(v: *const Volume, dir_record: u64, alloc_runs: []const ntfs.Run, sink: *EnumSink, node_vcn: ?u64, root_entries: []const u8, depth: usize, hide_system: bool) void {
    if (sink.failed or sink.found != null) return;
    if (depth >= MAX_INDEX_DEPTH) {
        sink.failed = true;
        return;
    }
    var offset: usize = 0;
    while (true) {
        var entries: []const u8 = undefined;
        if (node_vcn) |vcn| {
            if (!loadIndexBlockCached(v, dir_record, alloc_runs, vcn)) {
                sink.failed = true;
                return;
            }
            const block = ntfs.IndexBlock.parse(v.scratch.block[0..v.index_block_bytes]) orelse {
                sink.failed = true;
                return;
            };
            entries = block.entries;
        } else {
            entries = root_entries;
        }
        if (offset >= entries.len) return;

        var iterator = ntfs.IndexEntryIterator.init(entries[offset..]);
        const entry = iterator.next() orelse return;
        offset += entry.entry_length;

        var pending: ?Entry = null;
        if (!entry.isEnd()) {
            const file_name = entry.fileName() orelse {
                sink.failed = true;
                return;
            };
            if (file_name.namespace != ntfs.NAMESPACE_DOS) {
                const reference = ntfs.FileReference.parse(entry.file_reference);
                if (!(hide_system and reference.record < ntfs.MFT_FIRST_NORMAL)) {
                    pending = entryFromFileName(reference, file_name);
                }
            }
        }
        const is_end = entry.isEnd();
        if (entry.hasSubNode()) {
            walkNode(v, dir_record, alloc_runs, sink, entry.sub_node_vcn.?, root_entries, depth + 1, hide_system);
            if (sink.failed or sink.found != null) return;
        }
        if (is_end) return;
        const built = pending orelse continue;
        emitEntry(sink, built);
        if (sink.failed or sink.found != null) return;
        if (sink.out != null and sink.listed >= sink.max_entries) return;
    }
}

pub fn enumerateDirectory(v: *const Volume, dir_record: u64, sink: *EnumSink) bool {
    const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return false;
    const record = v.scratch.record[0..v.record_bytes];
    const root_attr = ntfs.findAttribute(record, header, .index_root, &ntfs.I30_NAME_UTF16) orelse return false;
    const index_root = ntfs.IndexRoot.parse(root_attr.value) orelse return false;

    const hide_system = dir_record == ntfs.MFT_RECORD_ROOT;
    if (index_root.header.hasSubNodes()) {
        var alloc = AttrScratch{};
        if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, &alloc)) return false;
        const header2 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return false;
        const root_attr2 = ntfs.findAttribute(record, header2, .index_root, &ntfs.I30_NAME_UTF16) orelse return false;
        const index_root2 = ntfs.IndexRoot.parse(root_attr2.value) orelse return false;
        walkNode(v, dir_record, alloc.runs[0..alloc.count], sink, null, index_root2.entries, 0, hide_system);
    } else {
        walkNode(v, dir_record, &[_]ntfs.Run{}, sink, null, index_root.entries, 0, hide_system);
    }
    return !sink.failed;
}

// ---------------------------------------------------------------------------
// File content read
// ---------------------------------------------------------------------------

pub fn readFileRange(v: *const Volume, record_number: u64, offset: usize, out: []u8) ?usize {
    var attr = AttrScratch{};
    if (!collectAttribute(v, record_number, .data, &[_]u8{}, &attr)) return null;
    return readAttrRange(v, &attr, offset, out);
}

/// Reads a named $DATA stream (alternate data stream) of a record.
pub fn readNamedStreamRange(v: *const Volume, record_number: u64, stream_name_utf8: []const u8, offset: usize, out: []u8) ?usize {
    var name_utf16: [NAME_UNITS_MAX * 2]u8 = undefined;
    const name_len = ntfs.utf8ToUtf16(stream_name_utf8, name_utf16[0..]) orelse return null;
    var attr = AttrScratch{};
    if (!collectAttribute(v, record_number, .data, name_utf16[0..name_len], &attr)) return null;
    return readAttrRange(v, &attr, offset, out);
}

fn readAttrRange(v: *const Volume, attr: *AttrScratch, offset: usize, out: []u8) ?usize {
    if ((attr.flags & ntfs.ATTR_FLAG_ENCRYPTED) != 0) return null;
    const offset_u64: u64 = @intCast(offset);
    if (offset_u64 >= attr.data_size) return 0;
    var want: u64 = out.len;
    if (want > attr.data_size - offset_u64) want = attr.data_size - offset_u64;
    const want_len: usize = @intCast(want);

    if (attr.resident) {
        @memcpy(out[0..want_len], attr.resident_copy[offset .. offset + want_len]);
        return want_len;
    }
    if ((attr.flags & ntfs.ATTR_FLAG_COMPRESSED) != 0) {
        if (!readCompressedRange(v, attr, offset_u64, out[0..want_len])) return null;
        return want_len;
    }
    if (!readRunBytes(v, attr.runs[0..attr.count], offset_u64, out[0..want_len])) return null;
    if (want > attr.initialized_size -| offset_u64) {
        const first_zero = if (attr.initialized_size > offset_u64)
            @as(usize, @intCast(attr.initialized_size - offset_u64))
        else
            0;
        @memset(out[first_zero..want_len], 0);
    }
    return want_len;
}

/// LZNT1 read path: the runlist is organized in compression units of
/// 2^compression_unit clusters.  A fully mapped unit is stored raw, a fully
/// sparse unit reads as zeros, and a partially mapped unit holds an LZNT1
/// stream in its leading mapped clusters.
fn readCompressedRange(v: *const Volume, attr: *AttrScratch, offset: u64, out: []u8) bool {
    const shift_value = if (attr.compression_unit == 0) @as(u8, 4) else attr.compression_unit;
    if (shift_value >= 64) return false;
    const shift: u6 = @intCast(shift_value);
    const unit_clusters: u64 = @as(u64, 1) << shift;
    const unit_bytes = checkedMulU64(unit_clusters, @as(u64, v.cluster_bytes)) orelse return false;
    if (unit_bytes == 0) return false;
    if (unit_bytes > v.scratch.comp_out.len) return false;

    var pos = offset;
    var out_pos: usize = 0;
    while (out_pos < out.len) {
        const unit_index = pos / unit_bytes;
        const in_unit: usize = @intCast(pos % unit_bytes);
        const unit_start_vcn = checkedMulU64(unit_index, unit_clusters) orelse return false;
        const mapped = mappedClustersInUnit(attr, unit_start_vcn, unit_clusters) orelse return false;
        const take = @min(out.len - out_pos, @as(usize, @intCast(unit_bytes)) - in_unit);
        const unit_byte_offset = checkedMulU64(unit_index, unit_bytes) orelse return false;

        if (mapped == unit_clusters) {
            // Stored unit: raw clusters.
            const read_offset = checkedAddU64(unit_byte_offset, in_unit) orelse return false;
            if (!readRunBytes(v, attr.runs[0..attr.count], read_offset, out[out_pos .. out_pos + take])) return false;
        } else if (mapped == 0) {
            @memset(out[out_pos .. out_pos + take], 0);
        } else {
            const comp_bytes_u64 = checkedMulU64(mapped, @as(u64, v.cluster_bytes)) orelse return false;
            if (comp_bytes_u64 > v.scratch.comp_in.len) return false;
            const comp_bytes: usize = @intCast(comp_bytes_u64);
            if (!readRunBytes(v, attr.runs[0..attr.count], unit_byte_offset, v.scratch.comp_in[0..comp_bytes])) return false;
            const produced = ntfs.lznt1Decompress(v.scratch.comp_in[0..comp_bytes], v.scratch.comp_out[0..@intCast(unit_bytes)]) orelse return false;
            if (produced < unit_bytes) @memset(v.scratch.comp_out[produced..@intCast(unit_bytes)], 0);
            @memcpy(out[out_pos .. out_pos + take], v.scratch.comp_out[in_unit .. in_unit + take]);
        }
        out_pos += take;
        pos += take;
    }
    return true;
}

fn mappedClustersInUnit(attr: *const AttrScratch, unit_start_vcn: u64, unit_clusters: u64) ?u64 {
    const unit_end_vcn = checkedAddU64(unit_start_vcn, unit_clusters) orelse return null;
    var mapped: u64 = 0;
    var vcn: u64 = 0;
    for (attr.runs[0..attr.count]) |run| {
        const run_end = checkedAddU64(vcn, run.length_clusters) orelse return null;
        const lo = @max(vcn, unit_start_vcn);
        const hi = @min(run_end, unit_end_vcn);
        if (hi > lo and run.lcn != null) {
            mapped = checkedAddU64(mapped, hi - lo) orelse return null;
        }
        vcn = run_end;
        if (vcn >= unit_end_vcn) break;
    }
    return mapped;
}

// ---------------------------------------------------------------------------
// Write phase 1: allocation
// ---------------------------------------------------------------------------

pub const AllocationResult = struct {
    status: WriteStatus,
    produced: usize = 0,
};

const FindFreeRunResult = struct {
    status: WriteStatus,
    run: ntfs.Run = .{ .length_clusters = 0, .lcn = null },
};

/// Finds `count` free clusters first, without mutating $Bitmap, and only then
/// marks the complete prepared run set.  Capacity and search failures cannot
/// therefore strand an already-marked prefix.  If a bitmap RMW fails, the
/// current (possibly partially written) run and every earlier run are cleared
/// before the failure becomes visible.
fn allocateClusters(v: *const Volume, count: u64, runs_out: []ntfs.Run) AllocationResult {
    const bitmap_attr = &v.scratch.attr_cluster;
    if (!collectAttribute(v, ntfs.MFT_RECORD_BITMAP, .data, &[_]u8{}, bitmap_attr)) return .{ .status = .io };
    if (bitmap_attr.resident) return .{ .status = .io };
    if (count == 0) return .{ .status = .ok };
    const total = v.totalClusters();

    var produced: usize = 0;
    var needed = count;
    var search_start: u64 = 0;
    while (needed > 0) {
        if (produced >= runs_out.len) return .{ .status = .record_full };
        const found = findFreeRun(v, bitmap_attr, total, search_start, needed);
        if (found.status != .ok) return .{ .status = found.status };
        const run = found.run;
        runs_out[produced] = run;
        produced += 1;
        needed -= run.length_clusters;
        search_start = checkedAddU64(run.lcn.?, run.length_clusters) orelse return .{ .status = .io };
    }

    var marked: usize = 0;
    while (marked < produced) : (marked += 1) {
        const run = runs_out[marked];
        if (!setBitmapRange(v, bitmap_attr, run.lcn.?, run.length_clusters, true)) {
            // Include the current range: setBitmapRange may already have
            // committed one or more of its bitmap sectors before failing.
            const rollback_ok = rollbackClusterRanges(v, bitmap_attr, runs_out[0 .. marked + 1]);
            return .{ .status = if (rollback_ok) .io else .cleanup_failed };
        }
    }
    return .{ .status = .ok, .produced = produced };
}

fn rollbackClusterRanges(v: *const Volume, bitmap_attr: *const AttrScratch, runs: []const ntfs.Run) bool {
    var ok = true;
    for (runs) |run| {
        const lcn = run.lcn orelse continue;
        // Continue after a failed clear so every independent range gets its
        // best chance to return to the pre-allocation state.
        if (!setBitmapRange(v, bitmap_attr, lcn, run.length_clusters, false)) ok = false;
    }
    return ok;
}

fn freeClusters(v: *const Volume, runs: []const ntfs.Run) bool {
    if (!runlistPhysicalRangeValid(v, runs)) return false;
    const bitmap_attr = &v.scratch.attr_cluster;
    if (!collectAttribute(v, ntfs.MFT_RECORD_BITMAP, .data, &[_]u8{}, bitmap_attr)) return false;
    for (runs) |run| {
        const lcn = run.lcn orelse continue;
        if (!setBitmapRange(v, bitmap_attr, lcn, run.length_clusters, false)) return false;
    }
    return true;
}

/// Longest free run starting at or after `from`, capped at `max_len`.
fn findFreeRun(v: *const Volume, bitmap_attr: *const AttrScratch, total: u64, from: u64, max_len: u64) FindFreeRunResult {
    var sector_buf: [SECTOR_SIZE]u8 = undefined;
    var run_start: ?u64 = null;
    var run_len: u64 = 0;
    var cluster: u64 = from;
    var loaded_sector: u64 = ~@as(u64, 0);
    while (cluster < total) : (cluster += 1) {
        const byte_index = cluster / 8;
        const sector_index = byte_index / SECTOR_SIZE;
        if (sector_index != loaded_sector) {
            const bitmap_offset = sectorByteOffset(sector_index) orelse return .{ .status = .io };
            if (!readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return .{ .status = .io };
            loaded_sector = sector_index;
        }
        const bit = (sector_buf[@intCast(byte_index % SECTOR_SIZE)] >> @intCast(cluster % 8)) & 1;
        if (bit == 0) {
            if (run_start == null) run_start = cluster;
            run_len += 1;
            if (run_len == max_len) break;
        } else if (run_start != null) {
            break;
        }
    }
    const start = run_start orelse return .{ .status = .no_space };
    if (run_len == 0) return .{ .status = .no_space };
    return .{ .status = .ok, .run = .{ .length_clusters = run_len, .lcn = start } };
}

/// Sets or clears a cluster range in $Bitmap and writes the touched sectors.
fn setBitmapRange(v: *const Volume, bitmap_attr: *const AttrScratch, lcn: u64, count: u64, set: bool) bool {
    if (count == 0) return true;
    const total_clusters = v.totalClusters();
    if (lcn >= total_clusters or count > total_clusters - lcn) return false;
    var sector_buf: [SECTOR_SIZE]u8 = undefined;
    var cluster = lcn;
    const end = checkedAddU64(lcn, count) orelse return false;
    while (cluster < end) {
        const sector_index = (cluster / 8) / SECTOR_SIZE;
        const bitmap_offset = sectorByteOffset(sector_index) orelse return false;
        if (!readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return false;
        while (cluster < end and (cluster / 8) / SECTOR_SIZE == sector_index) : (cluster += 1) {
            const byte_index: usize = @intCast((cluster / 8) % SECTOR_SIZE);
            const mask = @as(u8, 1) << @intCast(cluster % 8);
            if (set) {
                sector_buf[byte_index] |= mask;
            } else {
                sector_buf[byte_index] &= ~mask;
            }
        }
        if (!writeRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return false;
    }
    return true;
}

/// Allocates a free MFT record: scans $MFT/$BITMAP, marks the bit, returns
/// the record number with its next sequence number.
fn allocateRecord(v: *const Volume) ?struct { number: u64, sequence: u16 } {
    const bitmap_attr = &v.scratch.attr_mgmt_a;
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .bitmap, &[_]u8{}, bitmap_attr)) return null;
    const data_attr = &v.scratch.attr_mgmt_b;
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .data, &[_]u8{}, data_attr)) return null;
    const record_count = data_attr.data_size / v.record_bytes;

    var sector_buf: [SECTOR_SIZE]u8 = undefined;
    var number: u64 = ntfs.MFT_FIRST_NORMAL;
    var loaded_sector: u64 = ~@as(u64, 0);
    while (number < record_count) : (number += 1) {
        const byte_index = number / 8;
        const sector_index = byte_index / SECTOR_SIZE;
        if (sector_index != loaded_sector) {
            const bitmap_offset = sectorByteOffset(sector_index) orelse return null;
            const io_ok = if (bitmap_attr.resident)
                copyResident(bitmap_attr, bitmap_offset, sector_buf[0..])
            else
                readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..]);
            if (!io_ok) return null;
            loaded_sector = sector_index;
        }
        const bit = (sector_buf[@intCast(byte_index % SECTOR_SIZE)] >> @intCast(number % 8)) & 1;
        if (bit == 0) break;
    }
    if (number >= record_count) {
        // Grow the MFT and take the first freshly added record.  growMft
        // reuses the management scratch slots, so re-collect the bitmap.
        if (!growMft(v)) return null;
        number = record_count;
        if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .bitmap, &[_]u8{}, bitmap_attr)) return null;
        const sector_index = (number / 8) / SECTOR_SIZE;
        const bitmap_offset = sectorByteOffset(sector_index) orelse return null;
        const io_ok = if (bitmap_attr.resident)
            copyResident(bitmap_attr, bitmap_offset, sector_buf[0..])
        else
            readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..]);
        if (!io_ok) return null;
        loaded_sector = sector_index;
    }

    // Old sequence from the (possibly zeroed) record.
    var sequence: u16 = 1;
    if (loadRecordRaw(v, number, v.scratch.write_record[0..])) {
        const raw = v.scratch.write_record[0..v.record_bytes];
        if (readLe32(raw, 0) == ntfs.FILE_MAGIC) {
            const old_seq = readLe16(raw, 0x10);
            sequence = if (old_seq == 0xFFFF) 1 else old_seq + 1;
        }
    }

    // Mark the bit durable before the record is initialized.
    if (bitmap_attr.resident) return null;
    sector_buf[@intCast((number / 8) % SECTOR_SIZE)] |= @as(u8, 1) << @intCast(number % 8);
    const bitmap_offset = sectorByteOffset(number / 8 / SECTOR_SIZE) orelse return null;
    if (!writeRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return null;
    return .{ .number = number, .sequence = sequence };
}

/// Extends $MFT by 64 records: allocates clusters (bits durable first),
/// zeroes the new area, then updates record 0 (auto-synced into $MFTMirr)
/// and the in-memory runlist so the same operation can use the new records.
fn growMft(v: *const Volume) bool {
    const grow_records: u64 = 64;
    const data_attr = &v.scratch.attr_mgmt_b;
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .data, &[_]u8{}, data_attr)) return false;
    if (data_attr.resident) return false;
    const bitmap_attr = &v.scratch.attr_mgmt_a;
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .bitmap, &[_]u8{}, bitmap_attr)) return false;
    const cur_records = data_attr.data_size / v.record_bytes;
    // The MFT bitmap keeps its formatted size; growth must stay within it.
    const grown_records = checkedAddU64(cur_records, grow_records) orelse return false;
    const rounded_records = checkedAddU64(grown_records, 7) orelse return false;
    if (rounded_records / 8 > bitmap_attr.data_size) return false;

    const add_bytes = checkedMulU64(grow_records, @as(u64, v.record_bytes)) orelse return false;
    if (v.cluster_bytes == 0 or add_bytes % v.cluster_bytes != 0) return false;
    const add_clusters = add_bytes / v.cluster_bytes;
    if (add_clusters == 0) return false;
    if (data_attr.count + 8 > data_attr.runs.len) return false;

    var new_runs: [8]ntfs.Run = undefined;
    const allocation = allocateClusters(v, add_clusters, new_runs[0..]);
    if (allocation.status != .ok) return false;
    const produced = allocation.produced;

    // Zero the fresh area before record 0 references it (free records are
    // zeroed, matching the formatter's layout that chkdsk accepts).
    var zeros: [SECTOR_SIZE]u8 = .{0} ** SECTOR_SIZE;
    for (new_runs[0..produced]) |run| {
        const lcn = run.lcn orelse return false;
        var written: u64 = 0;
        const run_bytes = checkedMulU64(run.length_clusters, @as(u64, v.cluster_bytes)) orelse return false;
        while (written < run_bytes) : (written += SECTOR_SIZE) {
            if (!writeLcnBytes(v, lcn, written, zeros[0..])) return false;
        }
    }

    // Record 0: extend the collected runlist in place (merging adjacency).
    var total_runs = data_attr.count;
    for (new_runs[0..produced]) |run| {
        const adjacent = if (total_runs > 0 and
            data_attr.runs[total_runs - 1].lcn != null and run.lcn != null)
            (checkedAddU64(
                data_attr.runs[total_runs - 1].lcn.?,
                data_attr.runs[total_runs - 1].length_clusters,
            ) orelse return false) == run.lcn.?
        else
            false;
        if (adjacent) {
            data_attr.runs[total_runs - 1].length_clusters = checkedAddU64(
                data_attr.runs[total_runs - 1].length_clusters,
                run.length_clusters,
            ) orelse return false;
        } else {
            if (total_runs >= data_attr.runs.len) return false;
            data_attr.runs[total_runs] = run;
            total_runs += 1;
        }
    }
    if (total_runs > v.mft_runs_buf.len) return false;
    if (!runlistPhysicalRangeValid(v, data_attr.runs[0..total_runs])) return false;
    const new_bytes = checkedAddU64(data_attr.data_size, add_bytes) orelse return false;
    var header = loadRecord(v, ntfs.MFT_RECORD_MFT, v.scratch.write_record[0..]) orelse return false;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (!updateNonResident(record, &header, .data, &[_]u8{}, data_attr.runs[0..total_runs], new_bytes, new_bytes, new_bytes)) {
        if (!freeClusters(v, new_runs[0..produced])) return false;
        return false;
    }
    if (!updateFileNameDup(record, header, new_bytes, new_bytes, 0)) return false;
    if (!storeRecord(v, ntfs.MFT_RECORD_MFT, record)) return false;

    // Refresh the in-memory runlist for the remainder of this operation.
    @memcpy(v.mft_runs_buf[0..total_runs], data_attr.runs[0..total_runs]);
    v.mft_run_count.* = total_runs;
    return true;
}

fn copyResident(attr: *const AttrScratch, offset: u64, out: []u8) bool {
    @memset(out, 0);
    if (offset >= attr.resident_len) return true;
    const avail = attr.resident_len - @as(usize, @intCast(offset));
    const take = @min(avail, out.len);
    @memcpy(out[0..take], attr.resident_copy[@intCast(offset) .. @as(usize, @intCast(offset)) + take]);
    return true;
}

fn releaseRecord(v: *const Volume, number: u64) bool {
    var bitmap_attr = AttrScratch{};
    if (!collectAttribute(v, ntfs.MFT_RECORD_MFT, .bitmap, &[_]u8{}, &bitmap_attr)) return false;
    if (bitmap_attr.resident) return false;
    var sector_buf: [SECTOR_SIZE]u8 = undefined;
    const sector_index = (number / 8) / SECTOR_SIZE;
    const bitmap_offset = sectorByteOffset(sector_index) orelse return false;
    if (!readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return false;
    sector_buf[@intCast((number / 8) % SECTOR_SIZE)] &= ~(@as(u8, 1) << @intCast(number % 8));
    return writeRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..]);
}

// ---------------------------------------------------------------------------
// Write phase 1: dirty flag
// ---------------------------------------------------------------------------

pub fn setDirty(v: *const Volume, dirty: bool) bool {
    const header = loadRecord(v, ntfs.MFT_RECORD_VOLUME, v.scratch.write_record[0..]) orelse return false;
    const record = v.scratch.write_record[0..v.record_bytes];
    const attr = ntfs.findAttribute(record, header, .volume_information, &[_]u8{}) orelse return false;
    // Locate the flags inside the raw record buffer.
    const value_start = @intFromPtr(attr.value.ptr) - @intFromPtr(record.ptr);
    if (attr.value.len < 0x0C) return false;
    var flags = readLe16(record, value_start + 0x0A);
    const was_dirty = (flags & ntfs.VOLUME_FLAG_DIRTY) != 0;
    if (was_dirty == dirty) return true;
    if (dirty) {
        flags |= ntfs.VOLUME_FLAG_DIRTY;
    } else {
        flags &= ~ntfs.VOLUME_FLAG_DIRTY;
    }
    writeLe16(record, value_start + 0x0A, flags);
    if (!storeRecord(v, ntfs.MFT_RECORD_VOLUME, record)) return false;
    return deviceFlush(v);
}

/// Durable dirty-entry barrier for retryable metadata transactions. Unlike
/// deferred stream appends, a retry that observes an already-dirty cached
/// $Volume record must flush again: the previous device completion may have
/// been lost after the write reached media.
fn ensureDirtyDurable(v: *const Volume) bool {
    const already_dirty = isDirty(v) orelse return false;
    if (already_dirty) return deviceFlush(v);
    return setDirty(v, true);
}

/// Reads the on-disk dirty flag from $Volume (crash/hardening diagnostics).
pub fn isDirty(v: *const Volume) ?bool {
    const header = loadRecord(v, ntfs.MFT_RECORD_VOLUME, v.scratch.write_record[0..]) orelse return null;
    const record = v.scratch.write_record[0..v.record_bytes];
    const attr = ntfs.findAttribute(record, header, .volume_information, &[_]u8{}) orelse return null;
    const value_start = @intFromPtr(attr.value.ptr) - @intFromPtr(record.ptr);
    if (attr.value.len < 0x0C) return null;
    return (readLe16(record, value_start + 0x0A) & ntfs.VOLUME_FLAG_DIRTY) != 0;
}

// ---------------------------------------------------------------------------
// Write phase 1: record attribute editing
// ---------------------------------------------------------------------------

const AttrSpan = struct {
    offset: usize,
    length: usize,
};

fn findAttrSpan(record: []const u8, header: ntfs.FileRecordHeader, attr_type: ntfs.AttrType, name_utf16: []const u8) ?AttrSpan {
    var offset: usize = header.attrs_offset;
    while (offset + 8 <= record.len) {
        const t = readLe32(record, offset);
        if (t == ntfs.END_MARKER) return null;
        const length = readLe32(record, offset + 4);
        if (length < 0x18 or offset + length > record.len) return null;
        if (t == @intFromEnum(attr_type)) {
            const name_length = record[offset + 9];
            const name_offset = readLe16(record, offset + 10);
            const name = if (name_length > 0) record[offset + name_offset .. offset + name_offset + @as(usize, name_length) * 2] else record[0..0];
            if (eqlBytes(name, name_utf16)) return .{ .offset = offset, .length = length };
        }
        offset += length;
    }
    return null;
}

/// Grows or shrinks one attribute in place by moving the tail of the record.
/// Returns false when the record has no space.
fn resizeAttr(record: []u8, header: *ntfs.FileRecordHeader, span: *AttrSpan, new_length: usize) bool {
    const aligned = (new_length + 7) & ~@as(usize, 7);
    if (aligned == span.length) return true;
    const tail_start = span.offset + span.length;
    const tail_len = header.bytes_in_use - tail_start;
    const new_end = span.offset + aligned + tail_len;
    if (new_end > record.len) return false;
    // Move the record tail (including the end marker).
    const src = record[tail_start .. tail_start + tail_len];
    const dst = record[span.offset + aligned .. span.offset + aligned + tail_len];
    if (aligned > span.length) {
        var i: usize = tail_len;
        while (i > 0) : (i -= 1) dst[i - 1] = src[i - 1];
    } else {
        for (src, 0..) |b, i| dst[i] = b;
    }
    writeLe32(record, span.offset + 4, @intCast(aligned));
    header.bytes_in_use = @intCast(new_end);
    writeLe32(record, 0x18, header.bytes_in_use);
    span.length = aligned;
    return true;
}

/// Rewrites a non-resident attribute's runlist and size fields.
fn updateNonResident(record: []u8, header: *ntfs.FileRecordHeader, attr_type: ntfs.AttrType, name_utf16: []const u8, runs: []const ntfs.Run, data_size: u64, init_size: u64, alloc_size: u64) bool {
    var span = findAttrSpan(record, header.*, attr_type, name_utf16) orelse return false;
    if (record[span.offset + 8] != 1) return false; // must be non-resident

    // Record-sized mapping buffer: a fragmented runlist can approach the
    // full record in encoded size before it must spill to an extension
    // record.  A 256-byte buffer (0.60.15) capped this at ~64 fragments
    // regardless of record capacity (0.60.16 fix).
    var mapping: [4096]u8 = undefined;
    var mapping_len: usize = 0;
    var previous: i64 = 0;
    var vcn_total: u64 = 0;
    for (runs) |run| {
        if (mapping_len >= mapping.len) return false;
        const delta: ?i64 = if (run.lcn) |lcn| blk: {
            if (lcn > 0x7FFF_FFFF_FFFF_FFFF) return false;
            const current: i64 = @intCast(lcn);
            break :blk current - previous;
        } else null;
        const written = ntfs.encodeRun(mapping[mapping_len..], run.length_clusters, delta) orelse return false;
        mapping_len += written;
        if (run.lcn) |lcn| previous = @intCast(lcn);
        vcn_total = checkedAddU64(vcn_total, run.length_clusters) orelse return false;
    }
    if (mapping_len >= mapping.len) return false;
    mapping[mapping_len] = 0;
    mapping_len += 1;

    const mapping_offset = readLe16(record, span.offset + 0x20);
    if (mapping_offset > span.length) return false;
    const needed = mapping_offset + mapping_len;
    if (!resizeAttr(record, header, &span, needed)) return false;
    writeLe64(record, span.offset + 0x18, if (vcn_total == 0) 0 else vcn_total - 1);
    writeLe64(record, span.offset + 0x28, alloc_size);
    writeLe64(record, span.offset + 0x30, data_size);
    writeLe64(record, span.offset + 0x38, init_size);
    // Sparse/compressed attributes carry the real on-disk allocation
    // (total_allocated) at 0x40; keep it in sync when holes get mapped
    // (0.60.17).  alloc_size spans the whole VCN space, so the cluster size
    // falls out of alloc_size / vcn_total.
    const attr_flags = readLe16(record, span.offset + 0x0C);
    if ((attr_flags & (ntfs.ATTR_FLAG_SPARSE | ntfs.ATTR_FLAG_COMPRESSED)) != 0 and mapping_offset >= 0x48 and vcn_total > 0 and alloc_size % vcn_total == 0) {
        const cluster_bytes = alloc_size / vcn_total;
        var mapped_clusters: u64 = 0;
        for (runs) |run| {
            if (run.lcn != null) {
                mapped_clusters = checkedAddU64(mapped_clusters, run.length_clusters) orelse return false;
            }
        }
        const total_allocated = checkedMulU64(mapped_clusters, cluster_bytes) orelse return false;
        writeLe64(record, span.offset + 0x40, total_allocated);
    }
    @memcpy(record[span.offset + mapping_offset .. span.offset + mapping_offset + mapping_len], mapping[0..mapping_len]);
    return true;
}

/// Replaces a resident attribute's value (same or different length).
fn updateResident(record: []u8, header: *ntfs.FileRecordHeader, attr_type: ntfs.AttrType, name_utf16: []const u8, value: []const u8) bool {
    var span = findAttrSpan(record, header.*, attr_type, name_utf16) orelse return false;
    if (record[span.offset + 8] != 0) return false;
    const value_offset = readLe16(record, span.offset + 0x14);
    if (!resizeAttr(record, header, &span, value_offset + value.len)) return false;
    writeLe32(record, span.offset + 0x10, @intCast(value.len));
    @memcpy(record[span.offset + value_offset .. span.offset + value_offset + value.len], value);
    return true;
}

/// Updates the duplicated sizes/time in the record's $FILE_NAME attribute.
fn updateFileNameDup(record: []u8, header: ntfs.FileRecordHeader, alloc_size: u64, data_size: u64, mtime: u64) bool {
    const span = findAttrSpan(record, header, .file_name, &[_]u8{}) orelse return false;
    const value_offset = readLe16(record, span.offset + 0x14);
    const v = span.offset + value_offset;
    if (mtime != 0) {
        writeLe64(record, v + 0x10, mtime);
        writeLe64(record, v + 0x18, mtime);
    }
    writeLe64(record, v + 0x28, alloc_size);
    writeLe64(record, v + 0x30, data_size);
    return true;
}

/// Inserts a fully prepared attribute (its instance field is assigned here)
/// into the record at the type-sorted position.  `attr_bytes` length must be
/// 8-aligned with the length field set.
fn insertAttrRaw(record: []u8, attr_bytes: []const u8) bool {
    var header = ntfs.FileRecordHeader.parse(record) orelse return false;
    const attr_type = readLe32(attr_bytes, 0);
    // Find the first attribute with a larger type (or the end marker).
    var offset: usize = header.attrs_offset;
    while (offset + 8 <= record.len) {
        const t = readLe32(record, offset);
        if (t == ntfs.END_MARKER or t > attr_type) break;
        const length = readLe32(record, offset + 4);
        if (length < 0x18 or offset + length > record.len) return false;
        offset += length;
    }
    const add = attr_bytes.len;
    const tail_len = header.bytes_in_use - offset;
    if (header.bytes_in_use + add > record.len) return false;
    var i: usize = tail_len;
    const tail = record[offset..];
    while (i > 0) : (i -= 1) tail[i - 1 + add] = tail[i - 1];
    @memcpy(record[offset .. offset + add], attr_bytes);
    // Assign the next instance number.
    const instance = readLe16(record, 0x28);
    writeLe16(record, offset + 14, instance);
    writeLe16(record, 0x28, instance + 1);
    header.bytes_in_use += @intCast(add);
    writeLe32(record, 0x18, header.bytes_in_use);
    return true;
}

/// Removes one attribute from the record.
fn removeAttrRaw(record: []u8, attr_type: ntfs.AttrType, name_utf16: []const u8) bool {
    var header = ntfs.FileRecordHeader.parse(record) orelse return false;
    const span = findAttrSpan(record, header, attr_type, name_utf16) orelse return false;
    const tail_start = span.offset + span.length;
    const tail_len = header.bytes_in_use - tail_start;
    for (record[tail_start .. tail_start + tail_len], 0..) |b, i| record[span.offset + i] = b;
    header.bytes_in_use -= @intCast(span.length);
    writeLe32(record, 0x18, header.bytes_in_use);
    return true;
}

// ---------------------------------------------------------------------------
// Write phase 1: crash-fault injection (host models)
// ---------------------------------------------------------------------------

/// Optional abort budget: when set, every deviceFlush decrements it and the
/// operation is aborted at zero.  Host crash tests set this to reproduce a
/// power loss after each durable step; the kernel leaves it null.
pub var flush_budget: ?u32 = null;

/// Optional publish cut (0.60.21): when true `publishFileCreateOnly` stops
/// right after the canonical $FILE_NAME rewrite and before the target index
/// insert.  That window is durable on a real device but contains no flush of
/// its own, so `flush_budget` alone cannot reproduce it.  It is exactly the
/// state in which an index name and the canonical name disagree, which the
/// recovery compare-and-delete has to be able to reverse.  The kernel leaves
/// this false.
pub var cut_after_canonical_rewrite: bool = false;

fn budgetedFlush(v: *const Volume) bool {
    if (!deviceFlush(v)) return false;
    if (flush_budget) |*budget| {
        if (budget.* == 0) return false;
        budget.* -= 1;
    }
    return true;
}

// ---------------------------------------------------------------------------
// Write phase 2: $I30 B+ tree engine
// ---------------------------------------------------------------------------
//
// Node vocabulary: the resident $INDEX_ROOT is the root node; INDX blocks in
// $INDEX_ALLOCATION are interior/leaf nodes.  Every entry of an interior
// node carries a sub-node VCN whose subtree holds the smaller keys; the END
// entry's sub-node holds the largest.  Insert splits bottom-up (median
// promoted with the new left block); a full root pushes all its entries into
// a fresh block and keeps a single END child.  Interior deletion replaces
// the entry with the predecessor from its subtree; chains of empty blocks
// discovered on the predecessor path are reclaimed in the $I30 bitmap.

/// Builds a $FILE_NAME value (0x42 + UTF-16 name) from a UTF-8 name.
/// `creation_time` of 0 means "now".  Returns the value length.
fn buildFileNameValue(v: *const Volume, out: []u8, parent_record: u64, parent_sequence: u16, name: []const u8, is_dir: bool, alloc_size: u64, data_size: u64, creation_time: u64) ?usize {
    const namespace: u8 = if (is83Safe(name)) 3 else 0;
    const flags: u32 = if (is_dir) 0x10000000 else 0x20;
    if (0x42 > out.len) return null;
    const name16_len = ntfs.utf8ToUtf16(name, out[0x42..]) orelse return null;
    const fn_len = 0x42 + name16_len;
    @memset(out[0..0x42], 0);
    writeLe64(out, 0, ntfs.FileReference.pack(.{ .record = parent_record, .sequence = parent_sequence }));
    const created = if (creation_time != 0) creation_time else v.now_filetime;
    writeLe64(out, 0x08, created);
    writeLe64(out, 0x10, v.now_filetime);
    writeLe64(out, 0x18, v.now_filetime);
    writeLe64(out, 0x20, v.now_filetime);
    writeLe64(out, 0x28, alloc_size);
    writeLe64(out, 0x30, data_size);
    writeLe32(out, 0x38, flags);
    out[0x40] = @intCast(name16_len / 2);
    out[0x41] = namespace;
    return fn_len;
}

fn buildFileNameKey(v: *const Volume, out: []u8, parent_record: u64, parent_sequence: u16, name: []const u8, is_dir: bool, alloc_size: u64, data_size: u64) ?usize {
    if (out.len < 0x10) return null;
    const fn_len = buildFileNameValue(v, out[0x10..], parent_record, parent_sequence, name, is_dir, alloc_size, data_size, 0) orelse return null;
    const entry_len = (0x10 + fn_len + 7) & ~@as(usize, 7);
    if (entry_len > out.len) return null;
    @memset(out[0..0x10], 0);
    @memset(out[0x10 + fn_len .. entry_len], 0);
    writeLe16(out, 8, @intCast(entry_len));
    writeLe16(out, 10, @intCast(fn_len));
    return entry_len;
}

fn is83Safe(name: []const u8) bool {
    var dot: ?usize = null;
    for (name, 0..) |c, i| {
        if (c == '.') {
            if (dot != null) return false;
            dot = i;
            continue;
        }
        const ok = (c >= 'A' and c <= 'Z') or (c >= '0' and c <= '9') or
            c == '_' or c == '-' or c == '~' or c == '$' or c == '@' or c == '!' or
            c == '#' or c == '%' or c == '&' or c == '\'' or c == '(' or c == ')' or
            c == '{' or c == '}' or c == '^' or c == '`';
        if (!ok) return false;
    }
    const base = dot orelse name.len;
    if (base == 0 or base > 8) return false;
    if (dot) |d| {
        const ext = name.len - d - 1;
        if (ext == 0 or ext > 3) return false;
    }
    return true;
}

fn fileNameOfKey(key_entry: []const u8) ?ntfs.FileName {
    if (key_entry.len < 0x10 + 0x42) return null;
    return ntfs.FileName.parse(key_entry[0x10..]);
}

fn writeKeyReference(entry: []u8, key_ref: u64) void {
    writeLe64(entry, 0, key_ref);
}

fn eqlUtf16Upcase(v: *const Volume, a: []const u8, b: []const u8) bool {
    return ntfs.compareFileNames(v.upcase, a, b) == .eq;
}

// ---- raw entry accessors ---------------------------------------------------

fn entryLenAt(buf: []const u8, pos: usize) usize {
    return readLe16(buf, pos + 8);
}

fn entryIsEndAt(buf: []const u8, pos: usize) bool {
    return (readLe16(buf, pos + 12) & ntfs.INDEX_ENTRY_END) != 0;
}

fn entryChildAt(buf: []const u8, pos: usize) ?u64 {
    if ((readLe16(buf, pos + 12) & ntfs.INDEX_ENTRY_NODE) == 0) return null;
    const len = entryLenAt(buf, pos);
    return readLe64(buf, pos + len - 8);
}

fn setEntryChildAt(buf: []u8, pos: usize, vcn: u64) void {
    const len = entryLenAt(buf, pos);
    writeLe64(buf, pos + len - 8, vcn);
}

fn entryNameAt(buf: []const u8, pos: usize) []const u8 {
    const chars: usize = buf[pos + 0x10 + 0x40];
    return buf[pos + 0x10 + 0x42 .. pos + 0x10 + 0x42 + chars * 2];
}

fn nameOfPrepared(entry: []const u8) []const u8 {
    return entryNameAt(entry, 0);
}

pub const RecordLinkInfo = struct {
    sequence: u16,
    link_count: u16,
};

/// Reads the stable sequence/link-count pair of a live, structurally valid
/// base record.  Callers must not infer a single-name object from a record
/// number alone: Windows hard links share the same record and data stream.
pub fn recordLinkInfoStatus(v: *const Volume, record_number: u64, out: *RecordLinkInfo) LookupStatus {
    const header = loadRecord(v, record_number, v.scratch.part_record[0..]) orelse return .io;
    if (record_number > U32_MAX or !header.inUse() or
        header.record_number != @as(u32, @intCast(record_number)) or
        !attributeStreamValid(v.scratch.part_record[0..v.record_bytes], header))
    {
        return .io;
    }
    out.* = .{
        .sequence = header.sequence,
        .link_count = header.link_count,
    };
    return .found;
}

/// Statusful mutation preflight for operations which rewrite a canonical
/// $FILE_NAME or replace the whole file object.  A stale identity is I/O
/// corruption; a valid hard-linked object is a visible unsupported case.
pub fn requireSingleLinkStatus(v: *const Volume, record_number: u64, expected_sequence: u16) WriteStatus {
    var info: RecordLinkInfo = undefined;
    if (recordLinkInfoStatus(v, record_number, &info) != .found) return .io;
    if (info.sequence != expected_sequence) return .io;
    return if (info.link_count == 1) .ok else .unsupported;
}

/// The sequence number of a live record (loads it into scratch.part_record).
/// There is deliberately no fabricated fallback: mutation code must stop
/// when the parent/base identity cannot be read.
fn seqOf(v: *const Volume, record_number: u64) ?u16 {
    var info: RecordLinkInfo = undefined;
    if (recordLinkInfoStatus(v, record_number, &info) != .found) return null;
    return info.sequence;
}

fn lookupResultIsLive(v: *const Volume, found: LookupResult) bool {
    const header = loadRecord(v, found.record, v.scratch.part_record[0..]) orelse return false;
    return found.record <= U32_MAX and
        header.inUse() and
        header.record_number == @as(u32, @intCast(found.record)) and
        header.sequence == found.sequence and
        header.isDirectory() == found.entry.isDir() and
        attributeStreamValid(v.scratch.part_record[0..v.record_bytes], header);
}

pub fn recordIdentityStatus(v: *const Volume, record_number: u64, sequence: u16, expect_directory: bool) LookupStatus {
    const header = loadRecord(v, record_number, v.scratch.part_record[0..]) orelse return .io;
    if (record_number > U32_MAX or !header.inUse() or
        header.record_number != @as(u32, @intCast(record_number)) or
        header.sequence != sequence or
        header.isDirectory() != expect_directory or
        !attributeStreamValid(v.scratch.part_record[0..v.record_bytes], header))
    {
        return .io;
    }
    return .found;
}

// ---- node views ------------------------------------------------------------

/// Root node byte geometry inside the directory record (scratch.record).
const RootView = struct {
    span: AttrSpan,
    value_offset: usize,
    header_at: usize, // absolute offset of the INDEX_HEADER in the record

    fn locate(record: []const u8, header: ntfs.FileRecordHeader) ?RootView {
        const span = findAttrSpan(record, header, .index_root, &ntfs.I30_NAME_UTF16) orelse return null;
        const value_offset = readLe16(record, span.offset + 0x14);
        return .{ .span = span, .value_offset = value_offset, .header_at = span.offset + value_offset + 0x10 };
    }
};

fn blockHasSubNodes(block: []const u8) bool {
    return (block[0x18 + 0x0C] & 0x01) != 0;
}

fn blockEndChild(block: []const u8) ?u64 {
    const header_at: usize = 0x18;
    var pos = header_at + readLe32(block, header_at + 0x00);
    const end = header_at + readLe32(block, header_at + 0x04);
    while (pos < end) {
        if (entryIsEndAt(block, pos)) return entryChildAt(block, pos);
        pos += entryLenAt(block, pos);
    }
    return null;
}

fn blockHasRealEntries(block: []const u8) bool {
    const header_at: usize = 0x18;
    const pos = header_at + readLe32(block, header_at + 0x00);
    return !entryIsEndAt(block, pos);
}

fn loadIndexBlockInto(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64, buf: []u8) bool {
    const block = buf[0..v.index_block_bytes];
    const byte_offset = clusterByteOffset(v, vcn) orelse return false;
    if (!readRunBytes(v, alloc_runs, byte_offset, block)) return false;
    return ntfs.applyFixups(block) == .ok;
}

fn storeIndexBlockFrom(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64, buf: []u8) bool {
    const block = buf[0..v.index_block_bytes];
    const usn = readLe16(block, readLe16(block, 4));
    if (ntfs.installFixups(block, usn) != .ok) return false;
    const byte_offset = clusterByteOffset(v, vcn) orelse return false;
    if (!writeRunBytes(v, alloc_runs, byte_offset, block)) return false;
    _ = ntfs.applyFixups(block);
    return true;
}

fn writeIndxHeaderLocal(v: *const Volume, block: []u8, vcn: u64, used_end: usize, has_children: bool) void {
    writeLe32(block, 0, ntfs.INDX_MAGIC);
    writeLe16(block, 4, 0x28);
    writeLe16(block, 6, @intCast(v.index_block_bytes / SECTOR_SIZE + 1));
    writeLe64(block, 8, 0);
    writeLe64(block, 16, vcn);
    writeLe32(block, 0x18, 0x28); // entries_offset (relative to 0x18)
    writeLe32(block, 0x1C, @intCast(used_end - 0x18));
    writeLe32(block, 0x20, v.index_block_bytes - 0x18);
    block[0x24] = if (has_children) 1 else 0;
    block[0x25] = 0;
    writeLe16(block, 0x26, 0);
}

// ---- $INDEX_ALLOCATION management ------------------------------------------

/// Guard: the write path assumes one cluster per index block.
fn indexGeometryOk(v: *const Volume) bool {
    return v.cluster_bytes == v.index_block_bytes;
}

/// Returns a fresh usable block VCN, creating $INDEX_ALLOCATION/$BITMAP on
/// first use or growing the allocation by one cluster when all VCNs are used.
fn allocateIndexBlockVcn(v: *const Volume, dir_record: u64) ?u64 {
    if (!indexGeometryOk(v)) return null;

    const alloc = &v.scratch.attr_mgmt_a;
    const alloc_status = collectAttributeStatus(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc);
    if (alloc_status == .io) return null;
    const have_alloc = alloc_status == .found;
    if (!have_alloc) {
        // First-time creation with one cluster.
        var run: [1]ntfs.Run = undefined;
        const allocation = allocateClusters(v, 1, run[0..]);
        if (allocation.status != .ok) return null;
        const produced = allocation.produced;
        if (produced != 1) return null;
        const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return null;
        _ = header;
        const record = v.scratch.record[0..v.record_bytes];

        // $INDEX_ALLOCATION ($I30), non-resident, one cluster.
        var attr_buf: [0x60]u8 = .{0} ** 0x60;
        writeLe32(attr_buf[0..], 0, @intFromEnum(ntfs.AttrType.index_allocation));
        attr_buf[8] = 1; // non-resident
        attr_buf[9] = 4; // name length in chars
        writeLe16(attr_buf[0..], 10, 0x40);
        writeLe64(attr_buf[0..], 0x10, 0); // lowest_vcn
        writeLe64(attr_buf[0..], 0x18, 0); // highest_vcn (one cluster -> 0)
        writeLe16(attr_buf[0..], 0x20, 0x48);
        writeLe64(attr_buf[0..], 0x28, v.index_block_bytes);
        writeLe64(attr_buf[0..], 0x30, v.index_block_bytes);
        writeLe64(attr_buf[0..], 0x38, v.index_block_bytes);
        @memcpy(attr_buf[0x40..0x48], &ntfs.I30_NAME_UTF16);
        var mapping_len = ntfs.encodeRun(attr_buf[0x48..], 1, @intCast(run[0].lcn.?)) orelse return null;
        attr_buf[0x48 + mapping_len] = 0;
        mapping_len += 1;
        const total = (0x48 + mapping_len + 7) & ~@as(usize, 7);
        writeLe32(attr_buf[0..], 4, @intCast(total));
        if (!insertAttrRaw(record, attr_buf[0..total])) {
            if (!freeClusters(v, run[0..1])) return null;
            return null;
        }

        // $BITMAP ($I30), resident, 8 bytes, bit 0 set.
        var bmp_buf: [0x28]u8 = .{0} ** 0x28;
        writeLe32(bmp_buf[0..], 0, @intFromEnum(ntfs.AttrType.bitmap));
        writeLe32(bmp_buf[0..], 4, 0x28);
        bmp_buf[8] = 0;
        bmp_buf[9] = 4;
        writeLe16(bmp_buf[0..], 10, 0x18);
        writeLe32(bmp_buf[0..], 0x10, 8); // value length
        writeLe16(bmp_buf[0..], 0x14, 0x20);
        @memcpy(bmp_buf[0x18..0x20], &ntfs.I30_NAME_UTF16);
        bmp_buf[0x20] = 0x01;
        if (!insertAttrRaw(record, bmp_buf[0..0x28])) {
            if (!freeClusters(v, run[0..1])) return null;
            return null;
        }
        if (!storeRecord(v, dir_record, record)) return null;
        return 0;
    }

    const bmp = &v.scratch.attr_mgmt_b;
    if (!collectAttribute(v, dir_record, .bitmap, &ntfs.I30_NAME_UTF16, bmp)) return null;
    if (!bmp.resident) return null;
    const capacity: u64 = alloc.alloc_size / v.index_block_bytes;

    var vcn: u64 = 0;
    while (vcn < capacity) : (vcn += 1) {
        const byte: usize = @intCast(vcn / 8);
        if (byte >= bmp.resident_len) break;
        if ((bmp.resident_copy[byte] >> @intCast(vcn % 8)) & 1 == 0) break;
    }

    // Allocate the growth cluster BEFORE loading the directory record:
    // allocateClusters reloads scratch.record with the $Bitmap record.
    var grow_run: [1]ntfs.Run = undefined;
    var grew = false;
    if (vcn == capacity) {
        if (alloc.count >= MAX_DATA_RUNS) return null;
        const allocation = allocateClusters(v, 1, grow_run[0..]);
        if (allocation.status != .ok) return null;
        const produced = allocation.produced;
        if (produced != 1) return null;
        // Merge with the last run when physically adjacent to keep the
        // runlist compact.
        const adjacent = if (alloc.count > 0 and
            alloc.runs[alloc.count - 1].lcn != null and grow_run[0].lcn != null)
            (checkedAddU64(
                alloc.runs[alloc.count - 1].lcn.?,
                alloc.runs[alloc.count - 1].length_clusters,
            ) orelse return null) == grow_run[0].lcn.?
        else
            false;
        if (adjacent) {
            alloc.runs[alloc.count - 1].length_clusters = checkedAddU64(
                alloc.runs[alloc.count - 1].length_clusters,
                1,
            ) orelse return null;
        } else {
            alloc.runs[alloc.count] = grow_run[0];
            alloc.count += 1;
        }
        grew = true;
    }

    const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return null;
    var header = header0;
    const record = v.scratch.record[0..v.record_bytes];

    if (grew) {
        if (!runlistPhysicalRangeValid(v, alloc.runs[0..alloc.count])) return null;
        const new_bytes = checkedAddU64(alloc.alloc_size, @as(u64, v.index_block_bytes)) orelse return null;
        if (!updateNonResident(record, &header, .index_allocation, &ntfs.I30_NAME_UTF16, alloc.runs[0..alloc.count], new_bytes, new_bytes, new_bytes)) {
            if (!freeClusters(v, grow_run[0..1])) return null;
            return null;
        }
    }

    // Set the bit (extending the resident bitmap value in 8-byte steps).
    var value: [256]u8 = .{0} ** 256;
    var value_len = bmp.resident_len;
    if (value_len > value.len) return null;
    @memcpy(value[0..value_len], bmp.resident_copy[0..value_len]);
    const need_bytes = (((@as(usize, @intCast(vcn)) / 8) + 1 + 7) / 8) * 8;
    if (need_bytes > value.len) return null;
    if (need_bytes > value_len) value_len = need_bytes;
    value[@intCast(vcn / 8)] |= @as(u8, 1) << @intCast(vcn % 8);
    if (!updateResident(record, &header, .bitmap, &ntfs.I30_NAME_UTF16, value[0..value_len])) return null;
    if (!storeRecord(v, dir_record, record)) return null;
    return vcn;
}

/// Clears $I30 bitmap bits for freed blocks (they stay allocated clusters of
/// $INDEX_ALLOCATION and can be reused for new blocks).
fn freeIndexBlocks(v: *const Volume, dir_record: u64, vcns: []const u64) bool {
    if (vcns.len == 0) return true;
    const bmp = &v.scratch.attr_mgmt_a;
    if (!collectAttribute(v, dir_record, .bitmap, &ntfs.I30_NAME_UTF16, bmp)) return false;
    if (!bmp.resident) return false;
    var value: [256]u8 = undefined;
    if (bmp.resident_len > value.len) return false;
    @memcpy(value[0..bmp.resident_len], bmp.resident_copy[0..bmp.resident_len]);
    for (vcns) |vcn| {
        const byte: usize = @intCast(vcn / 8);
        if (byte >= bmp.resident_len) return false;
        value[byte] &= ~(@as(u8, 1) << @intCast(vcn % 8));
    }
    var header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return false;
    const record = v.scratch.record[0..v.record_bytes];
    if (!updateResident(record, &header, .bitmap, &ntfs.I30_NAME_UTF16, value[0..bmp.resident_len])) return false;
    return storeRecord(v, dir_record, record);
}

// ---- root node editing -----------------------------------------------------

/// The resident root is kept deliberately small (like Windows) so the
/// record always retains space for $INDEX_ALLOCATION runlist and $BITMAP
/// growth; a root exceeding this budget pushes down instead.
const ROOT_VALUE_BUDGET: usize = 0x180;

/// Inserts a fully prepared entry (reference set, optional child) into the
/// resident root in collation order.  Returns dir_full when the root budget
/// or the record space is exhausted.
fn insertEntryIntoRoot(v: *const Volume, dir_record: u64, entry: []const u8) WriteStatus {
    const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header0) orelse return .invalid;
    const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
    const index_length = readLe32(record, view.header_at + 0x04);
    const entries_end = view.header_at + index_length;
    if (readLe32(record, view.span.offset + 0x10) + entry.len > ROOT_VALUE_BUDGET) return .dir_full;
    const key_name = nameOfPrepared(entry);

    var pos = entries_at;
    while (pos < entries_end) {
        if (entryIsEndAt(record, pos)) break;
        if (ntfs.compareFileNames(v.upcase, key_name, entryNameAt(record, pos)) == .lt) break;
        pos += entryLenAt(record, pos);
    }

    var header = header0;
    var span = view.span;
    const add = entry.len;
    if (!resizeAttr(record, &header, &span, span.length + add)) return .dir_full;

    // Open the gap (positions inside the attribute are unchanged by resize).
    const tail = view.header_at + readLe32(record, view.header_at + 0x04);
    var i: usize = tail;
    while (i > pos) : (i -= 1) record[i - 1 + add] = record[i - 1];
    @memcpy(record[pos .. pos + add], entry);
    writeLe32(record, view.header_at + 0x04, index_length + @as(u32, @intCast(add)));
    writeLe32(record, view.header_at + 0x08, readLe32(record, view.header_at + 0x08) + @as(u32, @intCast(add)));
    // The attribute value grew as well.
    writeLe32(record, view.span.offset + 0x10, readLe32(record, view.span.offset + 0x10) + @as(u32, @intCast(add)));

    if (!storeRecord(v, dir_record, record)) return .io;
    return .ok;
}

/// Moves every root entry into a fresh INDX block; the root keeps a single
/// END entry pointing at it.  Creates the allocation attributes on first use.
fn pushRootDown(v: *const Volume, dir_record: u64) WriteStatus {
    const new_vcn = allocateIndexBlockVcn(v, dir_record) orelse return .no_space;
    const alloc = &v.scratch.attr_index;
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;

    const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header0) orelse return .invalid;
    const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
    const index_length = readLe32(record, view.header_at + 0x04);
    const entries_end = view.header_at + index_length;

    // Copy all entries (including END with its child) into the new block.
    const block2 = v.scratch.block2[0..v.index_block_bytes];
    @memset(block2, 0);
    const copy_len = entries_end - entries_at;
    if (0x40 + copy_len > block2.len) return .invalid;
    @memcpy(block2[0x40 .. 0x40 + copy_len], record[entries_at..entries_end]);
    var has_children = false;
    var pos: usize = 0x40;
    while (pos < 0x40 + copy_len) {
        if (entryChildAt(block2, pos) != null) has_children = true;
        if (entryIsEndAt(block2, pos)) break;
        pos += entryLenAt(block2, pos);
    }
    writeIndxHeaderLocal(v, block2, new_vcn, 0x40 + copy_len, has_children);
    if (!storeIndexBlockFrom(v, alloc.runs[0..alloc.count], new_vcn, v.scratch.block2[0..])) return .io;

    // Rebuild the root value: IndexRoot head + header + END(child=new_vcn).
    var root_value: [0x38]u8 = .{0} ** 0x38;
    @memcpy(root_value[0..0x10], record[view.span.offset + view.value_offset .. view.span.offset + view.value_offset + 0x10]);
    writeLe32(root_value[0..], 0x10, 0x10); // entries_offset
    writeLe32(root_value[0..], 0x14, 0x10 + 0x18); // index_length
    writeLe32(root_value[0..], 0x18, 0x10 + 0x18); // allocated
    root_value[0x1C] = 1; // has sub-nodes
    // END entry with child.
    writeLe16(root_value[0..], 0x20 + 8, 0x18);
    writeLe16(root_value[0..], 0x20 + 12, ntfs.INDEX_ENTRY_END | ntfs.INDEX_ENTRY_NODE);
    writeLe64(root_value[0..], 0x20 + 0x10, new_vcn);

    var header = header0;
    if (!updateResident(record, &header, .index_root, &ntfs.I30_NAME_UTF16, root_value[0..])) return .io;
    if (!storeRecord(v, dir_record, record)) return .io;
    return .ok;
}

fn rootEndChild(v: *const Volume, dir_record: u64) ?u64 {
    const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return null;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header) orelse return null;
    var pos = view.header_at + readLe32(record, view.header_at + 0x00);
    const end = view.header_at + readLe32(record, view.header_at + 0x04);
    while (pos < end) {
        if (entryIsEndAt(record, pos)) return entryChildAt(record, pos);
        pos += entryLenAt(record, pos);
    }
    return null;
}

// ---- block editing ---------------------------------------------------------

/// Inserts a prepared entry into scratch.block at the collation position.
/// The buffer carries ENTRY_MAX slack, so the insert itself always succeeds;
/// the caller checks fitsIndexBlock() and splits when over.
fn insertEntryIntoBlockSlack(v: *const Volume, entry: []const u8) void {
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;
    const key_name = nameOfPrepared(entry);

    var pos = entries_at;
    while (pos < entries_end) {
        if (entryIsEndAt(block, pos)) break;
        if (ntfs.compareFileNames(v.upcase, key_name, entryNameAt(block, pos)) == .lt) break;
        pos += entryLenAt(block, pos);
    }
    const add = entry.len;
    var i: usize = entries_end;
    while (i > pos) : (i -= 1) block[i - 1 + add] = block[i - 1];
    @memcpy(block[pos .. pos + add], entry);
    writeLe32(block, header_at + 0x04, index_length + @as(u32, @intCast(add)));
}

fn fitsIndexBlock(v: *const Volume) bool {
    const block = v.scratch.block[0..];
    return readLe32(block, 0x18 + 0x04) <= readLe32(block, 0x18 + 0x08);
}

/// Splits the overflowed scratch.block (at `vcn`): smaller half moves into a
/// new block at `new_vcn`, the median is left in scratch.entry_a as an
/// interior entry pointing at the new block.  Both blocks are stored.
fn splitOverflowedBlock(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64, new_vcn: u64) bool {
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;

    // Locate END and the real-entry span.
    var end_pos = entries_at;
    var real_count: usize = 0;
    while (end_pos < entries_end and !entryIsEndAt(block, end_pos)) {
        real_count += 1;
        end_pos += entryLenAt(block, end_pos);
    }
    if (real_count < 3) return false;

    // Median by cumulative bytes, keeping >=1 entry on each side.
    const payload = end_pos - entries_at;
    var median_pos = entries_at;
    var median_index: usize = 0;
    var cum: usize = 0;
    while (median_index + 1 < real_count) {
        const len = entryLenAt(block, median_pos);
        if (cum + len >= payload / 2 and median_index > 0) break;
        cum += len;
        median_pos += len;
        median_index += 1;
    }
    const median_len = entryLenAt(block, median_pos);
    const median_child = entryChildAt(block, median_pos);

    // New block: entries before the median + END(child = median's child).
    const block2 = v.scratch.block2[0..v.index_block_bytes];
    @memset(block2, 0);
    const left_len = median_pos - entries_at;
    @memcpy(block2[0x40 .. 0x40 + left_len], block[entries_at..median_pos]);
    var has_children = median_child != null;
    var scan: usize = 0x40;
    while (scan < 0x40 + left_len) {
        if (entryChildAt(block2, scan) != null) has_children = true;
        scan += entryLenAt(block2, scan);
    }
    var off = 0x40 + left_len;
    const end2_len: usize = if (median_child != null) 0x18 else 0x10;
    writeLe16(block2, off + 8, @intCast(end2_len));
    writeLe16(block2, off + 12, if (median_child != null) ntfs.INDEX_ENTRY_END | ntfs.INDEX_ENTRY_NODE else ntfs.INDEX_ENTRY_END);
    if (median_child) |c| writeLe64(block2, off + end2_len - 8, c);
    off += end2_len;
    writeIndxHeaderLocal(v, block2, new_vcn, off, has_children);
    if (!storeIndexBlockFrom(v, alloc_runs, new_vcn, v.scratch.block2[0..])) return false;

    // Promote the median as an interior entry pointing at the new block.
    const fn_len: usize = readLe16(block, median_pos + 10);
    const plain_len = (0x10 + fn_len + 7) & ~@as(usize, 7);
    const promoted_len = plain_len + 8;
    if (promoted_len > v.scratch.entry_a.len) return false;
    const promoted = v.scratch.entry_a[0..promoted_len];
    @memset(promoted, 0);
    @memcpy(promoted[0..0x10], block[median_pos .. median_pos + 0x10]);
    @memcpy(promoted[0x10 .. 0x10 + fn_len], block[median_pos + 0x10 .. median_pos + 0x10 + fn_len]);
    writeLe16(promoted, 8, @intCast(promoted_len));
    writeLe16(promoted, 12, ntfs.INDEX_ENTRY_NODE);
    writeLe64(promoted, promoted_len - 8, new_vcn);

    // Compact the old block: keep entries after the median (incl. END).
    const rest_start = median_pos + median_len;
    const rest_len = entries_end - rest_start;
    for (block[rest_start..entries_end], 0..) |b, i| block[entries_at + i] = b;
    writeLe32(block, header_at + 0x04, @intCast(entries_at - header_at + rest_len));
    return storeIndexBlock(v, alloc_runs, vcn);
}

// ---- insert ----------------------------------------------------------------

/// Records the descent path (block VCNs from below the root to the leaf).
fn descentPathFor(v: *const Volume, dir_record: u64, key_name: []const u8, alloc_runs: []const ntfs.Run, path: *[MAX_INDEX_DEPTH]u64) ?usize {
    const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return null;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header) orelse return null;
    var pos = view.header_at + readLe32(record, view.header_at + 0x00);
    const end = view.header_at + readLe32(record, view.header_at + 0x04);
    var next: ?u64 = null;
    while (pos < end) {
        if (entryIsEndAt(record, pos)) {
            next = entryChildAt(record, pos);
            break;
        }
        if (ntfs.compareFileNames(v.upcase, key_name, entryNameAt(record, pos)) == .lt) {
            next = entryChildAt(record, pos);
            break;
        }
        pos += entryLenAt(record, pos);
    }

    var depth: usize = 0;
    while (depth < MAX_INDEX_DEPTH) {
        const vcn = next orelse return depth;
        path[depth] = vcn;
        depth += 1;
        if (!loadIndexBlock(v, alloc_runs, vcn)) return null;
        const block = v.scratch.block[0..v.index_block_bytes];
        if (!blockHasSubNodes(block)) return depth;
        const header_at: usize = 0x18;
        var bpos = header_at + readLe32(block, header_at + 0x00);
        const bend = header_at + readLe32(block, header_at + 0x04);
        next = null;
        while (bpos < bend) {
            if (entryIsEndAt(block, bpos)) {
                next = entryChildAt(block, bpos);
                break;
            }
            if (ntfs.compareFileNames(v.upcase, key_name, entryNameAt(block, bpos)) == .lt) {
                next = entryChildAt(block, bpos);
                break;
            }
            bpos += entryLenAt(block, bpos);
        }
        if (next == null) return null; // interior node without a route
    }
    return null;
}

/// Inserts a FILE_NAME index entry into the directory with full B+ tree
/// handling (leaf/interior splits, root push-down, allocation growth).
fn indexInsert(v: *const Volume, dir_record: u64, key_ref: u64, key_entry: []const u8) WriteStatus {
    if (key_entry.len > v.scratch.entry_a.len) return .invalid;
    const prepared = v.scratch.entry_a[0..key_entry.len];
    @memcpy(prepared, key_entry);
    writeKeyReference(prepared, key_ref);
    return insertPreparedEntry(v, dir_record, key_entry.len);
}

/// Inserts scratch.entry_a[0..entry_len].  The buffer is reused for promoted
/// medians as splits walk up the tree.
fn insertPreparedEntry(v: *const Volume, dir_record: u64, entry_len: usize) WriteStatus {
    {
        const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
        const record = v.scratch.record[0..v.record_bytes];
        const view = RootView.locate(record, header) orelse return .invalid;
        const has_children = (record[view.header_at + 0x0C] & 0x01) != 0;
        if (!has_children) {
            const st = insertEntryIntoRoot(v, dir_record, v.scratch.entry_a[0..entry_len]);
            if (st != .dir_full) return st;
            const pd = pushRootDown(v, dir_record);
            if (pd != .ok) return pd;
        }
    }

    const alloc = &v.scratch.attr_index;
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
    var path: [MAX_INDEX_DEPTH]u64 = undefined;
    const key_name_probe = nameOfPrepared(v.scratch.entry_a[0..entry_len]);
    const depth = descentPathFor(v, dir_record, key_name_probe, alloc.runs[0..alloc.count], &path) orelse return .io;
    if (depth == 0) return .io;

    var level = depth - 1;
    var current_len = entry_len;
    var guard: usize = 0;
    while (guard < MAX_INDEX_DEPTH + 2) : (guard += 1) {
        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], path[level])) return .io;
        insertEntryIntoBlockSlack(v, v.scratch.entry_a[0..current_len]);
        if (fitsIndexBlock(v)) {
            if (!storeIndexBlock(v, alloc.runs[0..alloc.count], path[level])) return .io;
            return .ok;
        }
        // Split; scratch.block holds the overflowed node, so the fresh VCN
        // must be obtained without touching scratch.block (allocation code
        // only uses record/part_record scratch).
        const new_vcn = allocateIndexBlockVcn(v, dir_record) orelse return .no_space;
        if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
        if (!splitOverflowedBlock(v, alloc.runs[0..alloc.count], path[level], new_vcn)) return .io;
        current_len = promotedLen(v);

        if (level == 0) {
            const st = insertEntryIntoRoot(v, dir_record, v.scratch.entry_a[0..current_len]);
            if (st != .dir_full) return st;
            const pd = pushRootDown(v, dir_record);
            if (pd != .ok) return pd;
            if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
            const child = rootEndChild(v, dir_record) orelse return .io;
            if (!loadIndexBlock(v, alloc.runs[0..alloc.count], child)) return .io;
            insertEntryIntoBlockSlack(v, v.scratch.entry_a[0..current_len]);
            if (!fitsIndexBlock(v)) return .io; // cannot happen: root was small
            if (!storeIndexBlock(v, alloc.runs[0..alloc.count], child)) return .io;
            return .ok;
        }
        level -= 1;
    }
    return .io;
}

fn promotedLen(v: *const Volume) usize {
    return entryLenAt(v.scratch.entry_a[0..], 0);
}

// ---- remove ----------------------------------------------------------------

/// Removes the index entry for `name`, handling entries in the root, in
/// leaves and in interior nodes (predecessor replacement).
fn indexRemove(v: *const Volume, dir_record: u64, name: []const u8) WriteStatus {
    const target_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return .invalid;
    return indexRemoveUtf16Expected(v, dir_record, v.scratch.name_utf16[0..target_len], null);
}

/// Removes a name only when the index entry still carries the caller's exact
/// FileReference.  This is the detach primitive used after a rename has
/// durably published its second alias; a stale/reused name must never remove
/// a foreign object.
fn indexRemoveIdentity(v: *const Volume, dir_record: u64, name: []const u8, expected_ref: u64) WriteStatus {
    const target_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return .invalid;
    return indexRemoveUtf16Expected(v, dir_record, v.scratch.name_utf16[0..target_len], expected_ref);
}

fn indexRemoveUtf16Expected(v: *const Volume, dir_record: u64, target: []const u8, expected_ref: ?u64) WriteStatus {
    const alloc = &v.scratch.attr_index;

    // Search the root first.
    {
        const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
        const record = v.scratch.record[0..v.record_bytes];
        const view = RootView.locate(record, header) orelse return .invalid;
        var pos = view.header_at + readLe32(record, view.header_at + 0x00);
        const end = view.header_at + readLe32(record, view.header_at + 0x04);
        var next: ?u64 = null;
        while (pos < end) {
            if (entryIsEndAt(record, pos)) {
                next = entryChildAt(record, pos);
                break;
            }
            const order = ntfs.compareFileNames(v.upcase, target, entryNameAt(record, pos));
            if (order == .eq) {
                if (expected_ref) |wanted| {
                    if (readLe64(record, pos) != wanted) return .io;
                }
                const child = entryChildAt(record, pos);
                if (child == null) return removeEntryFromRoot(v, dir_record, target);
                return interiorRemove(v, dir_record, true, 0, target, child.?);
            }
            if (order == .lt) {
                next = entryChildAt(record, pos);
                break;
            }
            pos += entryLenAt(record, pos);
        }
        const vcn0 = next orelse return .not_found;
        if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;

        var cur = vcn0;
        var depth: usize = 0;
        while (depth < MAX_INDEX_DEPTH) : (depth += 1) {
            if (!loadIndexBlock(v, alloc.runs[0..alloc.count], cur)) return .io;
            const block = v.scratch.block[0..v.index_block_bytes];
            const header_at: usize = 0x18;
            var bpos = header_at + readLe32(block, header_at + 0x00);
            const bend = header_at + readLe32(block, header_at + 0x04);
            var bnext: ?u64 = null;
            while (bpos < bend) {
                if (entryIsEndAt(block, bpos)) {
                    bnext = entryChildAt(block, bpos);
                    break;
                }
                const order = ntfs.compareFileNames(v.upcase, target, entryNameAt(block, bpos));
                if (order == .eq) {
                    if (expected_ref) |wanted| {
                        if (readLe64(block, bpos) != wanted) return .io;
                    }
                    const child = entryChildAt(block, bpos);
                    if (child == null) return removeEntryFromBlock(v, dir_record, alloc.runs[0..alloc.count], cur, target);
                    return interiorRemove(v, dir_record, false, cur, target, child.?);
                }
                if (order == .lt) {
                    bnext = entryChildAt(block, bpos);
                    break;
                }
                bpos += entryLenAt(block, bpos);
            }
            cur = bnext orelse return .not_found;
        }
        // A routed descent that exhausts MAX_INDEX_DEPTH did not prove
        // absence; classify the unsupported/corrupt geometry as I/O.
        return .io;
    }
}

/// Plain removal of a childless entry from the resident root.
fn removeEntryFromRoot(v: *const Volume, dir_record: u64, target: []const u8) WriteStatus {
    const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header0) orelse return .invalid;
    const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
    const index_length = readLe32(record, view.header_at + 0x04);
    const entries_end = view.header_at + index_length;

    var pos = entries_at;
    var found_len: usize = 0;
    while (pos < entries_end) {
        if (entryIsEndAt(record, pos)) break;
        if (eqlUtf16Upcase(v, entryNameAt(record, pos), target)) {
            found_len = entryLenAt(record, pos);
            break;
        }
        pos += entryLenAt(record, pos);
    }
    if (found_len == 0) return .not_found;

    const tail_start = pos + found_len;
    const tail_len = entries_end - tail_start;
    for (record[tail_start .. tail_start + tail_len], 0..) |b, i| record[pos + i] = b;
    var header = header0;
    writeLe32(record, view.header_at + 0x04, index_length - @as(u32, @intCast(found_len)));
    writeLe32(record, view.header_at + 0x08, readLe32(record, view.header_at + 0x08) - @as(u32, @intCast(found_len)));
    writeLe32(record, view.span.offset + 0x10, readLe32(record, view.span.offset + 0x10) - @as(u32, @intCast(found_len)));
    var span = view.span;
    _ = resizeAttr(record, &header, &span, span.length - found_len);
    if (!storeRecord(v, dir_record, record)) return .io;
    return .ok;
}

/// Plain removal of a childless entry from a loaded leaf block.  A block
/// that loses its last real entry is eliminated from the tree afterwards
/// (Windows/chkdsk do not tolerate referenced empty index blocks).
fn removeEntryFromBlock(v: *const Volume, dir_record: u64, alloc_runs: []const ntfs.Run, vcn: u64, target: []const u8) WriteStatus {
    if (!loadIndexBlock(v, alloc_runs, vcn)) return .io;
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;
    var pos = entries_at;
    while (pos < entries_end) {
        if (entryIsEndAt(block, pos)) return .not_found;
        if (eqlUtf16Upcase(v, entryNameAt(block, pos), target)) {
            const len = entryLenAt(block, pos);
            const tail_start = pos + len;
            for (block[tail_start..entries_end], 0..) |b, i| block[pos + i] = b;
            writeLe32(block, header_at + 0x04, index_length - @as(u32, @intCast(len)));
            const now_empty = !blockHasRealEntries(block[0..v.index_block_bytes]);
            if (!storeIndexBlock(v, alloc_runs, vcn)) return .io;
            if (now_empty) return eliminateEmptyBlock(v, dir_record, vcn);
            return .ok;
        }
        pos += entryLenAt(block, pos);
    }
    return .not_found;
}

// ---- empty-block elimination -----------------------------------------------

const Referrer = struct {
    in_root: bool,
    node_vcn: u64,
    entry_pos: usize,
    is_end: bool,
};

/// Finds the node whose entry (or END) references block `vcn`.
fn findReferrer(v: *const Volume, dir_record: u64, alloc_runs: []const ntfs.Run, vcn: u64) ?Referrer {
    // Root first.
    {
        const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return null;
        const record = v.scratch.record[0..v.record_bytes];
        const view = RootView.locate(record, header) orelse return null;
        var pos = view.header_at + readLe32(record, view.header_at + 0x00);
        const end = view.header_at + readLe32(record, view.header_at + 0x04);
        while (pos < end) {
            if (entryChildAt(record, pos)) |child| {
                if (child == vcn) return .{ .in_root = true, .node_vcn = 0, .entry_pos = pos, .is_end = entryIsEndAt(record, pos) };
            }
            if (entryIsEndAt(record, pos)) break;
            pos += entryLenAt(record, pos);
        }
    }
    // Then every used block.
    const total = blk: {
        var clusters: u64 = 0;
        for (alloc_runs) |run| {
            clusters = checkedAddU64(clusters, run.length_clusters) orelse return null;
        }
        const bytes = checkedMulU64(clusters, @as(u64, v.cluster_bytes)) orelse return null;
        if (v.index_block_bytes == 0) return null;
        break :blk bytes / v.index_block_bytes;
    };
    var scan: u64 = 0;
    while (scan < total) : (scan += 1) {
        if (scan == vcn) continue;
        if (!loadIndexBlock(v, alloc_runs, scan)) continue;
        const block = v.scratch.block[0..];
        const header_at: usize = 0x18;
        var pos = header_at + readLe32(block, header_at + 0x00);
        const end = header_at + readLe32(block, header_at + 0x04);
        while (pos < end) {
            if (entryChildAt(block, pos)) |child| {
                if (child == vcn) return .{ .in_root = false, .node_vcn = scan, .entry_pos = pos, .is_end = entryIsEndAt(block, pos) };
            }
            if (entryIsEndAt(block, pos)) break;
            pos += entryLenAt(block, pos);
        }
    }
    return null;
}

/// Removes an empty block (no real entries) from the tree.  Interior-only-END
/// blocks are bypassed (referrer points at their child); empty leaves rewire
/// their referrer: an entry is re-inserted plainly, an END rotates the last
/// entry of the parent, and a fully drained tree collapses back to a small
/// resident root.
fn eliminateEmptyBlock(v: *const Volume, dir_record: u64, first_vcn: u64) WriteStatus {
    var vcn = first_vcn;
    var guard: usize = 0;
    while (guard < MAX_INDEX_DEPTH + 2) : (guard += 1) {
        const alloc = &v.scratch.attr_index;
        if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;

        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], vcn)) return .io;
        const empty_block = v.scratch.block[0..v.index_block_bytes];
        if (blockHasRealEntries(empty_block)) return .ok; // raced/nothing to do
        const bypass_child = blockEndChild(empty_block);

        const referrer = findReferrer(v, dir_record, alloc.runs[0..alloc.count], vcn) orelse return .io;

        if (bypass_child) |child| {
            // Interior with only an END: referrer points directly at child.
            if (referrer.in_root) {
                const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
                const record = v.scratch.record[0..v.record_bytes];
                _ = RootView.locate(record, header) orelse return .invalid;
                setEntryChildAt(record, referrer.entry_pos, child);
                if (!storeRecord(v, dir_record, record)) return .io;
            } else {
                if (!loadIndexBlock(v, alloc.runs[0..alloc.count], referrer.node_vcn)) return .io;
                setEntryChildAt(v.scratch.block[0..], referrer.entry_pos, child);
                if (!storeIndexBlock(v, alloc.runs[0..alloc.count], referrer.node_vcn)) return .io;
            }
            if (!freeIndexBlocks(v, dir_record, &[_]u64{vcn})) return .io;
            return .ok;
        }

        // Empty leaf.
        if (!referrer.is_end) {
            // Referrer is a real entry E: remove it, free the leaf, re-insert
            // E as a plain entry (it descends into a neighbouring subtree).
            if (referrer.in_root) {
                const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
                const record = v.scratch.record[0..v.record_bytes];
                _ = header;
                if (!copyEntryPlain(v, record, referrer.entry_pos)) return .io;
            } else {
                if (!loadIndexBlock(v, alloc.runs[0..alloc.count], referrer.node_vcn)) return .io;
                if (!copyEntryPlain(v, v.scratch.block[0..], referrer.entry_pos)) return .io;
            }
            var name_copy: [512]u8 = undefined;
            const plain = v.scratch.entry_a[0..entryLenAt(v.scratch.entry_a[0..], 0)];
            const nm = entryNameAt(plain, 0);
            if (nm.len > name_copy.len) return .io;
            @memcpy(name_copy[0..nm.len], nm);
            const st = if (referrer.in_root)
                removeEntryFromRoot(v, dir_record, name_copy[0..nm.len])
            else
                removeEntryWithChildFromBlockByName(v, alloc.runs[0..alloc.count], referrer.node_vcn, name_copy[0..nm.len]);
            if (st != .ok) return st;
            if (!freeIndexBlocks(v, dir_record, &[_]u64{vcn})) return .io;
            const plain_len = entryLenAt(v.scratch.entry_a[0..], 0);
            return insertPreparedEntry(v, dir_record, plain_len);
        }

        // Referrer is an END entry.
        if (referrer.in_root) {
            const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
            const record = v.scratch.record[0..v.record_bytes];
            const view = RootView.locate(record, header) orelse return .invalid;
            const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
            if (!entryIsEndAt(record, entries_at)) {
                // Root still has entries: rotate its last entry into END.
                return rotateEndFromRoot(v, dir_record, vcn);
            }
            // Tree fully drained: collapse to an empty resident root.
            return collapseRootToResident(v, dir_record);
        }
        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], referrer.node_vcn)) return .io;
        const pblock = v.scratch.block[0..v.index_block_bytes];
        if (blockHasRealEntries(pblock)) {
            return rotateEndFromBlock(v, dir_record, referrer.node_vcn, vcn);
        }
        // Parent has only END -> child(=vcn): drop its child; the parent
        // becomes an empty leaf and is eliminated on the next iteration.
        const header_at: usize = 0x18;
        const end_pos = header_at + readLe32(pblock, header_at + 0x00);
        const end_len = entryLenAt(pblock, end_pos);
        if (end_len < 0x18) return .io;
        writeLe16(v.scratch.block[0..], end_pos + 8, 0x10);
        writeLe16(v.scratch.block[0..], end_pos + 12, ntfs.INDEX_ENTRY_END);
        writeLe32(v.scratch.block[0..], header_at + 0x04, readLe32(pblock, header_at + 0x04) - 8);
        v.scratch.block[0x24] = 0; // no longer has children
        if (!storeIndexBlock(v, alloc.runs[0..alloc.count], referrer.node_vcn)) return .io;
        if (!freeIndexBlocks(v, dir_record, &[_]u64{vcn})) return .io;
        vcn = referrer.node_vcn;
    }
    return .io;
}

/// Copies the entry at `pos` in `buf` into scratch.entry_a as a plain entry
/// (no child, no flags).
fn copyEntryPlain(v: *const Volume, buf: []const u8, pos: usize) bool {
    const fn_len: usize = readLe16(buf, pos + 10);
    const plain_len = (0x10 + fn_len + 7) & ~@as(usize, 7);
    if (plain_len > v.scratch.entry_a.len) return false;
    const out = v.scratch.entry_a[0..plain_len];
    @memset(out, 0);
    @memcpy(out[0..0x10], buf[pos .. pos + 0x10]);
    @memcpy(out[0x10 .. 0x10 + fn_len], buf[pos + 0x10 .. pos + 0x10 + fn_len]);
    writeLe16(out, 8, @intCast(plain_len));
    writeLe16(out, 12, 0);
    return true;
}

/// Generic by-name removal from a block that also handles entries WITH a
/// child (the caller has already rescued the child linkage).
fn removeEntryWithChildFromBlockByName(v: *const Volume, alloc_runs: []const ntfs.Run, vcn: u64, target: []const u8) WriteStatus {
    if (!loadIndexBlock(v, alloc_runs, vcn)) return .io;
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;
    var pos = entries_at;
    while (pos < entries_end) {
        if (entryIsEndAt(block, pos)) return .not_found;
        if (eqlUtf16Upcase(v, entryNameAt(block, pos), target)) {
            const len = entryLenAt(block, pos);
            const tail_start = pos + len;
            for (block[tail_start..entries_end], 0..) |b, i| block[pos + i] = b;
            writeLe32(block, header_at + 0x04, index_length - @as(u32, @intCast(len)));
            if (!storeIndexBlock(v, alloc_runs, vcn)) return .io;
            return .ok;
        }
        pos += entryLenAt(block, pos);
    }
    return .not_found;
}

/// The last real entry of the root moves into its END position: END adopts
/// the entry's child, the freed leaf `leaf_vcn` is released and the entry is
/// re-inserted plainly.
fn rotateEndFromRoot(v: *const Volume, dir_record: u64, leaf_vcn: u64) WriteStatus {
    const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    const view = RootView.locate(record, header0) orelse return .invalid;
    const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
    const index_length = readLe32(record, view.header_at + 0x04);
    const entries_end = view.header_at + index_length;
    var pos = entries_at;
    var last: ?usize = null;
    while (pos < entries_end and !entryIsEndAt(record, pos)) {
        last = pos;
        pos += entryLenAt(record, pos);
    }
    const q_at = last orelse return .io;
    const q_child = entryChildAt(record, q_at) orelse return .io;
    if (!copyEntryPlain(v, record, q_at)) return .io;
    // Remove Q and point END at Q's child.
    const q_len = entryLenAt(record, q_at);
    const tail_start = q_at + q_len;
    const tail_len = entries_end - tail_start;
    for (record[tail_start .. tail_start + tail_len], 0..) |b, i| record[q_at + i] = b;
    var header = header0;
    const nl = index_length - @as(u32, @intCast(q_len));
    writeLe32(record, view.header_at + 0x04, nl);
    writeLe32(record, view.header_at + 0x08, nl);
    writeLe32(record, view.span.offset + 0x10, readLe32(record, view.span.offset + 0x10) - @as(u32, @intCast(q_len)));
    var span = view.span;
    _ = resizeAttr(record, &header, &span, span.length - q_len);
    // END is now at q_at; give it Q's child.
    setEntryChildAt(record, q_at, q_child);
    if (!storeRecord(v, dir_record, record)) return .io;
    if (!freeIndexBlocks(v, dir_record, &[_]u64{leaf_vcn})) return .io;
    const plain_len = entryLenAt(v.scratch.entry_a[0..], 0);
    return insertPreparedEntry(v, dir_record, plain_len);
}

/// Same rotation inside an interior block.
fn rotateEndFromBlock(v: *const Volume, dir_record: u64, node_vcn: u64, leaf_vcn: u64) WriteStatus {
    const alloc = &v.scratch.attr_index;
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
    if (!loadIndexBlock(v, alloc.runs[0..alloc.count], node_vcn)) return .io;
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;
    var pos = entries_at;
    var last: ?usize = null;
    while (pos < entries_end and !entryIsEndAt(block, pos)) {
        last = pos;
        pos += entryLenAt(block, pos);
    }
    const q_at = last orelse return .io;
    const q_child = entryChildAt(block, q_at) orelse return .io;
    if (!copyEntryPlain(v, block, q_at)) return .io;
    const q_len = entryLenAt(block, q_at);
    const tail_start = q_at + q_len;
    for (block[tail_start..entries_end], 0..) |b, i| block[q_at + i] = b;
    writeLe32(block, header_at + 0x04, index_length - @as(u32, @intCast(q_len)));
    setEntryChildAt(v.scratch.block[0..], q_at, q_child);
    if (!storeIndexBlock(v, alloc.runs[0..alloc.count], node_vcn)) return .io;
    if (!freeIndexBlocks(v, dir_record, &[_]u64{leaf_vcn})) return .io;
    const plain_len = entryLenAt(v.scratch.entry_a[0..], 0);
    return insertPreparedEntry(v, dir_record, plain_len);
}

/// Collapses a fully drained large index back to an empty resident root and
/// removes $INDEX_ALLOCATION/$BITMAP, freeing their clusters.
fn collapseRootToResident(v: *const Volume, dir_record: u64) WriteStatus {
    // attr_op belongs to the public operation (deleteFile keeps its data
    // runs there across the index removal) — use a management slot.
    const alloc = &v.scratch.attr_mgmt_b;
    const alloc_status = collectAttributeStatus(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc);
    if (alloc_status == .io) return .io;
    const has_alloc = alloc_status == .found;

    var header = loadRecord(v, dir_record, v.scratch.write_record[0..]) orelse return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    var root_value: [0x30]u8 = .{0} ** 0x30;
    writeLe32(root_value[0..], 0, @intFromEnum(ntfs.AttrType.file_name));
    writeLe32(root_value[0..], 4, ntfs.COLLATION_FILE_NAME);
    writeLe32(root_value[0..], 8, v.index_block_bytes);
    root_value[12] = 1;
    writeLe32(root_value[0..], 0x10, 0x10);
    writeLe32(root_value[0..], 0x14, 0x20);
    writeLe32(root_value[0..], 0x18, 0x20);
    writeLe16(root_value[0..], 0x20 + 8, 0x10);
    writeLe16(root_value[0..], 0x20 + 12, ntfs.INDEX_ENTRY_END);
    if (!updateResident(record, &header, .index_root, &ntfs.I30_NAME_UTF16, root_value[0..])) return .io;
    _ = removeAttrRaw(record, .index_allocation, &ntfs.I30_NAME_UTF16);
    _ = removeAttrRaw(record, .bitmap, &ntfs.I30_NAME_UTF16);
    if (!storeRecord(v, dir_record, record)) return .io;
    if (has_alloc and !alloc.resident) {
        if (!freeClusters(v, alloc.runs[0..alloc.count])) return .io;
    }
    return .ok;
}

/// Removes an interior entry: replaces it with the predecessor of its
/// subtree, or drops it when the whole subtree is empty (reclaiming the
/// chain of empty blocks).
fn interiorRemove(v: *const Volume, dir_record: u64, e_in_root: bool, e_vcn: u64, target: []const u8, sub: u64) WriteStatus {
    const alloc = &v.scratch.attr_index;
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;

    // Predecessor descent: follow END children; remember the deepest node
    // with real entries (K) and the empty chain below it.
    var chain: [MAX_INDEX_DEPTH]u64 = undefined;
    var chain_len: usize = 0;
    var k_vcn: ?u64 = null;
    var cur = sub;
    var depth: usize = 0;
    while (depth < MAX_INDEX_DEPTH) : (depth += 1) {
        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], cur)) return .io;
        const block = v.scratch.block[0..v.index_block_bytes];
        if (blockHasRealEntries(block)) {
            k_vcn = cur;
            chain_len = 0;
        } else {
            if (chain_len >= chain.len) return .io;
            chain[chain_len] = cur;
            chain_len += 1;
        }
        const end_child = blockEndChild(block) orelse break;
        cur = end_child;
    }

    if (k_vcn == null) {
        // Whole subtree empty: drop the entry, reclaim the chain.
        const st = if (e_in_root)
            removeInteriorEntryFromRoot(v, dir_record, target)
        else
            removeInteriorEntryFromBlock(v, dir_record, alloc.runs[0..alloc.count], e_vcn, target);
        if (st != .ok) return st;
        if (!freeIndexBlocks(v, dir_record, chain[0..chain_len])) return .io;
        return .ok;
    }

    // Copy the predecessor Q (last real entry of K) into entry_b, plain.
    if (!loadIndexBlock(v, alloc.runs[0..alloc.count], k_vcn.?)) return .io;
    const kblock = v.scratch.block[0..];
    const header_at: usize = 0x18;
    var qpos = header_at + readLe32(kblock, header_at + 0x00);
    var last_real: ?usize = null;
    while (!entryIsEndAt(kblock, qpos)) {
        last_real = qpos;
        qpos += entryLenAt(kblock, qpos);
    }
    const q_at = last_real orelse return .io;
    const q_len = entryLenAt(kblock, q_at);
    const q_child = entryChildAt(kblock, q_at);
    const q_fn_len: usize = readLe16(kblock, q_at + 10);
    const q_plain_len = (0x10 + q_fn_len + 7) & ~@as(usize, 7);
    if (q_plain_len > v.scratch.entry_b.len) return .io;
    const qb = v.scratch.entry_b[0..q_plain_len];
    @memset(qb, 0);
    @memcpy(qb[0..0x10], kblock[q_at .. q_at + 0x10]);
    @memcpy(qb[0x10 .. 0x10 + q_fn_len], kblock[q_at + 0x10 .. q_at + 0x10 + q_fn_len]);
    writeLe16(qb, 8, @intCast(q_plain_len));
    writeLe16(qb, 12, 0);

    // Remove Q from K (block still loaded in scratch.block).
    const k_is_leaf = blockEndChild(kblock[0..v.index_block_bytes]) == null;
    const k_index_length = readLe32(kblock, header_at + 0x04);
    const k_entries_end = header_at + k_index_length;
    const tail_start = q_at + q_len;
    for (kblock[tail_start..k_entries_end], 0..) |b, i| kblock[q_at + i] = b;
    writeLe32(kblock, header_at + 0x04, k_index_length - @as(u32, @intCast(q_len)));
    if (!k_is_leaf) {
        // Interior K: END inherits Q's subtree; the empty chain is freed.
        const end_at = q_at; // END moved into Q's old position
        if (entryChildAt(kblock, end_at) == null) return .io;
        setEntryChildAt(kblock[0..], end_at, q_child orelse return .io);
    }
    const k_now_empty = !blockHasRealEntries(kblock[0..v.index_block_bytes]);
    if (!storeIndexBlock(v, alloc.runs[0..alloc.count], k_vcn.?)) return .io;
    if (!k_is_leaf) {
        if (!freeIndexBlocks(v, dir_record, chain[0..chain_len])) return .io;
    }

    // Replace E with Q (Q inherits E's child).
    const st = replaceInteriorEntry(v, dir_record, e_in_root, e_vcn, target, q_plain_len);
    if (st != .ok) return st;
    // K may have drained completely; eliminate it so no empty block stays
    // referenced (chkdsk parity).
    if (k_now_empty) return eliminateEmptyBlock(v, dir_record, k_vcn.?);
    return .ok;
}

/// Removes an entry WITH child from a node; the following entries close the
/// gap (used only when the entry's subtree is empty).
fn removeInteriorEntryFromRoot(v: *const Volume, dir_record: u64, target: []const u8) WriteStatus {
    return removeEntryFromRoot(v, dir_record, target);
}

fn removeInteriorEntryFromBlock(v: *const Volume, dir_record: u64, alloc_runs: []const ntfs.Run, vcn: u64, target: []const u8) WriteStatus {
    return removeEntryFromBlock(v, dir_record, alloc_runs, vcn, target);
}

/// Replaces interior entry `target` with scratch.entry_b (plain) + the old
/// entry's child.  Handles both root and block nodes; a block overflow is
/// resolved with the normal split machinery via re-descent.
fn replaceInteriorEntry(v: *const Volume, dir_record: u64, in_root: bool, vcn: u64, target: []const u8, q_plain_len: usize) WriteStatus {
    if (in_root) {
        const header0 = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return .io;
        const record = v.scratch.record[0..v.record_bytes];
        const view = RootView.locate(record, header0) orelse return .invalid;
        const entries_at = view.header_at + readLe32(record, view.header_at + 0x00);
        const index_length = readLe32(record, view.header_at + 0x04);
        const entries_end = view.header_at + index_length;
        var pos = entries_at;
        while (pos < entries_end and !entryIsEndAt(record, pos)) {
            if (eqlUtf16Upcase(v, entryNameAt(record, pos), target)) {
                const old_len = entryLenAt(record, pos);
                const child = entryChildAt(record, pos) orelse return .io;
                const new_len = q_plain_len + 8;
                var header = header0;
                var span = view.span;
                if (new_len > old_len) {
                    if (!resizeAttr(record, &header, &span, span.length + (new_len - old_len))) {
                        // Root cannot grow: push it down, then replace in the block.
                        const pd = pushRootDown(v, dir_record);
                        if (pd != .ok) return pd;
                        const child_vcn = rootEndChild(v, dir_record) orelse return .io;
                        return replaceInteriorEntry(v, dir_record, false, child_vcn, target, q_plain_len);
                    }
                }
                // Shift the tail to the new entry size.
                const tail_src = pos + old_len;
                const tail_dst = pos + new_len;
                const tail_len = entries_end - tail_src;
                if (new_len > old_len) {
                    var i: usize = tail_len;
                    while (i > 0) : (i -= 1) record[tail_dst + i - 1] = record[tail_src + i - 1];
                } else {
                    for (record[tail_src .. tail_src + tail_len], 0..) |b, i| record[tail_dst + i] = b;
                }
                @memcpy(record[pos .. pos + q_plain_len], v.scratch.entry_b[0..q_plain_len]);
                writeLe16(record, pos + 8, @intCast(new_len));
                writeLe16(record, pos + 12, ntfs.INDEX_ENTRY_NODE);
                @memset(record[pos + q_plain_len .. pos + new_len], 0);
                writeLe64(record, pos + new_len - 8, child);
                const delta_new = @as(i64, @intCast(new_len)) - @as(i64, @intCast(old_len));
                const nl: u32 = @intCast(@as(i64, @intCast(index_length)) + delta_new);
                writeLe32(record, view.header_at + 0x04, nl);
                writeLe32(record, view.header_at + 0x08, nl);
                const vl: u32 = @intCast(@as(i64, @intCast(readLe32(record, view.span.offset + 0x10))) + delta_new);
                writeLe32(record, view.span.offset + 0x10, vl);
                if (new_len < old_len) {
                    _ = resizeAttr(record, &header, &span, span.length - (old_len - new_len));
                }
                if (!storeRecord(v, dir_record, record)) return .io;
                return .ok;
            }
            pos += entryLenAt(record, pos);
        }
        return .not_found;
    }

    const alloc = &v.scratch.attr_index;
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
    if (!loadIndexBlock(v, alloc.runs[0..alloc.count], vcn)) return .io;
    const block = v.scratch.block[0..];
    const header_at: usize = 0x18;
    const entries_at = header_at + readLe32(block, header_at + 0x00);
    const index_length = readLe32(block, header_at + 0x04);
    const entries_end = header_at + index_length;
    var pos = entries_at;
    while (pos < entries_end and !entryIsEndAt(block, pos)) {
        if (eqlUtf16Upcase(v, entryNameAt(block, pos), target)) {
            const old_len = entryLenAt(block, pos);
            const child = entryChildAt(block, pos) orelse return .io;
            const new_len = q_plain_len + 8;
            const tail_src = pos + old_len;
            const tail_dst = pos + new_len;
            const tail_len = entries_end - tail_src;
            if (new_len > old_len) {
                var i: usize = tail_len;
                while (i > 0) : (i -= 1) block[tail_dst + i - 1] = block[tail_src + i - 1];
            } else {
                for (block[tail_src .. tail_src + tail_len], 0..) |b, i| block[tail_dst + i] = b;
            }
            @memcpy(block[pos .. pos + q_plain_len], v.scratch.entry_b[0..q_plain_len]);
            writeLe16(block, pos + 8, @intCast(new_len));
            writeLe16(block, pos + 12, ntfs.INDEX_ENTRY_NODE);
            @memset(block[pos + q_plain_len .. pos + new_len], 0);
            writeLe64(block, pos + new_len - 8, child);
            const delta_new = @as(i64, @intCast(new_len)) - @as(i64, @intCast(old_len));
            writeLe32(block, header_at + 0x04, @intCast(@as(i64, @intCast(index_length)) + delta_new));
            if (fitsIndexBlock(v)) {
                if (!storeIndexBlock(v, alloc.runs[0..alloc.count], vcn)) return .io;
                return .ok;
            }
            // Overflow: split this node and promote via the insert machinery.
            const new_vcn = allocateIndexBlockVcn(v, dir_record) orelse return .no_space;
            if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return .io;
            if (!splitOverflowedBlock(v, alloc.runs[0..alloc.count], vcn, new_vcn)) return .io;
            return promoteIntoParentOf(v, dir_record, vcn);
        }
        pos += entryLenAt(block, pos);
    }
    return .not_found;
}

/// Inserts the promoted entry in scratch.entry_a into the parent of `vcn`
/// (found by re-descending toward the promoted key).
fn promoteIntoParentOf(v: *const Volume, dir_record: u64, below_vcn: u64) WriteStatus {
    _ = below_vcn;
    const current_len = promotedLen(v);
    return insertPreparedEntry(v, dir_record, current_len);
}

// ---- duplicate-info update -------------------------------------------------

/// Patches the duplicated alloc/data sizes and mtime of the index entry for
/// `name`, wherever it lives (root, interior or leaf).  Entry sizes do not
/// change, so this never restructures the tree.
fn updateIndexEntrySizes(v: *const Volume, dir_record: u64, name: []const u8, alloc_size: u64, data_size: u64, mtime: u64) bool {
    last_append_diagnostic_stage = 171; // index name conversion
    const target_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return false;
    const target = v.scratch.name_utf16[0..target_len];

    // Root scan + descent.
    last_append_diagnostic_stage = 172; // index root record
    const header = loadRecord(v, dir_record, v.scratch.record[0..]) orelse return false;
    const record = v.scratch.record[0..v.record_bytes];
    last_append_diagnostic_stage = 173; // index root view
    const view = RootView.locate(record, header) orelse return false;
    var pos = view.header_at + readLe32(record, view.header_at + 0x00);
    const end = view.header_at + readLe32(record, view.header_at + 0x04);
    var next: ?u64 = null;
    while (pos < end) {
        if (entryIsEndAt(record, pos)) {
            next = entryChildAt(record, pos);
            break;
        }
        const order = ntfs.compareFileNames(v.upcase, target, entryNameAt(record, pos));
        if (order == .eq) {
            patchDupAt(record, pos, alloc_size, data_size, mtime);
            last_append_diagnostic_stage = 174; // index root store
            return storeRecord(v, dir_record, record);
        }
        if (order == .lt) {
            next = entryChildAt(record, pos);
            break;
        }
        pos += entryLenAt(record, pos);
    }
    last_append_diagnostic_stage = 175; // index descent target
    var cur = next orelse return false;
    const alloc = &v.scratch.attr_index;
    last_append_diagnostic_stage = 176; // index allocation attribute
    if (!collectAttribute(v, dir_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc)) return false;
    var depth: usize = 0;
    while (depth < MAX_INDEX_DEPTH) : (depth += 1) {
        last_append_diagnostic_stage = 177; // index block load
        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], cur)) return false;
        const block = v.scratch.block[0..];
        const header_at: usize = 0x18;
        var bpos = header_at + readLe32(block, header_at + 0x00);
        const bend = header_at + readLe32(block, header_at + 0x04);
        var bnext: ?u64 = null;
        while (bpos < bend) {
            if (entryIsEndAt(block, bpos)) {
                bnext = entryChildAt(block, bpos);
                break;
            }
            const order = ntfs.compareFileNames(v.upcase, target, entryNameAt(block, bpos));
            if (order == .eq) {
                patchDupAt(block, bpos, alloc_size, data_size, mtime);
                last_append_diagnostic_stage = 178; // index block store
                return storeIndexBlock(v, alloc.runs[0..alloc.count], cur);
            }
            if (order == .lt) {
                bnext = entryChildAt(block, bpos);
                break;
            }
            bpos += entryLenAt(block, bpos);
        }
        last_append_diagnostic_stage = 179; // next index branch
        cur = bnext orelse return false;
    }
    last_append_diagnostic_stage = 180; // index depth exhausted
    return false;
}

fn patchDupAt(buf: []u8, pos: usize, alloc_size: u64, data_size: u64, mtime: u64) void {
    const val = pos + 0x10;
    if (mtime != 0) {
        writeLe64(buf, val + 0x10, mtime);
        writeLe64(buf, val + 0x18, mtime);
    }
    writeLe64(buf, val + 0x28, alloc_size);
    writeLe64(buf, val + 0x30, data_size);
}

// ---------------------------------------------------------------------------
// Write phase 1: public file operations
// ---------------------------------------------------------------------------

const RESIDENT_DATA_MAX: usize = 0x2A0;

/// Builds a fresh FILE record for a new file into scratch.write_record.
/// Data is resident (<= RESIDENT_DATA_MAX) or a non-resident placeholder
/// whose runs are filled by the caller.
fn buildFileRecord(v: *const Volume, number: u64, sequence: u16, parent_record: u64, parent_sequence: u16, name: []const u8, resident_data: ?[]const u8, non_resident_size: u64, alloc_size: u64) usize {
    const record = v.scratch.write_record[0..v.record_bytes];
    @memset(record, 0);
    writeLe32(record, 0, ntfs.FILE_MAGIC);
    writeLe16(record, 4, 0x30); // usa_ofs
    writeLe16(record, 6, @intCast(v.record_bytes / SECTOR_SIZE + 1));
    writeLe16(record, 0x10, sequence);
    writeLe16(record, 0x12, 1); // link_count
    writeLe16(record, 0x14, 0x38); // attrs_offset
    writeLe16(record, 0x16, 0x01); // in_use
    writeLe32(record, 0x1C, v.record_bytes);
    writeLe32(record, 0x2C, @intCast(number));
    var offset: usize = 0x38;
    var instance: u16 = 0;

    // $STANDARD_INFORMATION (72 bytes).
    offset += emitAttr(record, offset, .standard_information, &[_]u8{}, false, blk: {
        var si: [0x48]u8 = .{0} ** 0x48;
        var t: usize = 0;
        while (t < 0x20) : (t += 8) writeLe64(si[0..], t, v.now_filetime);
        writeLe32(si[0..], 0x20, 0x20); // archive
        writeLe32(si[0..], 0x34, v.security_id_file);
        break :blk si[0..];
    }, 0, &instance);

    // $FILE_NAME.
    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const data_size = if (resident_data) |d| @as(u64, d.len) else non_resident_size;
    const fn_len = buildFileNameValue(v, fn_buf[0..], parent_record, parent_sequence, name, false, alloc_size, data_size, 0) orelse return 0;
    offset += emitAttr(record, offset, .file_name, &[_]u8{}, true, fn_buf[0..fn_len], 0, &instance);

    // $DATA.
    if (resident_data) |d| {
        offset += emitAttr(record, offset, .data, &[_]u8{}, false, d, 0, &instance);
    } else {
        offset += emitNonResidentDataStub(record, offset, non_resident_size, alloc_size, &instance);
    }

    writeLe32(record, offset, ntfs.END_MARKER);
    writeLe32(record, offset + 4, 0);
    offset += 8;
    writeLe16(record, 0x28, instance);
    writeLe32(record, 0x18, @intCast(offset));
    return offset;
}

fn emitAttr(record: []u8, offset: usize, attr_type: ntfs.AttrType, name_utf16: []const u8, indexed: bool, value: []const u8, res_flags: u8, instance: *u16) usize {
    const name_offset: usize = 0x18;
    const value_offset = (name_offset + name_utf16.len + 7) & ~@as(usize, 7);
    const total = (value_offset + value.len + 7) & ~@as(usize, 7);
    const a = record[offset..];
    @memset(a[0..total], 0);
    writeLe32(a, 0, @intFromEnum(attr_type));
    writeLe32(a, 4, @intCast(total));
    a[8] = 0; // resident
    a[9] = @intCast(name_utf16.len / 2);
    writeLe16(a, 10, @intCast(name_offset));
    writeLe16(a, 14, instance.*);
    instance.* += 1;
    writeLe32(a, 0x10, @intCast(value.len));
    writeLe16(a, 0x14, @intCast(value_offset));
    a[0x16] = if (indexed or res_flags != 0) 1 else 0;
    if (name_utf16.len > 0) @memcpy(a[name_offset .. name_offset + name_utf16.len], name_utf16);
    @memcpy(a[value_offset .. value_offset + value.len], value);
    return total;
}

fn emitNonResidentDataStub(record: []u8, offset: usize, data_size: u64, alloc_size: u64, instance: *u16) usize {
    // A single 0x00-terminated empty mapping; the runlist is patched in by
    // updateNonResident once clusters are allocated.
    const mapping_offset: usize = 0x40;
    const total = (mapping_offset + 8 + 7) & ~@as(usize, 7);
    const a = record[offset..];
    @memset(a[0..total], 0);
    writeLe32(a, 0, @intFromEnum(ntfs.AttrType.data));
    writeLe32(a, 4, @intCast(total));
    a[8] = 1; // non-resident
    writeLe16(a, 10, @intCast(mapping_offset));
    writeLe16(a, 14, instance.*);
    instance.* += 1;
    writeLe64(a, 0x10, 0); // lowest_vcn
    writeLe64(a, 0x18, 0); // highest_vcn (0 until runs added)
    writeLe16(a, 0x20, @intCast(mapping_offset));
    writeLe64(a, 0x28, alloc_size);
    writeLe64(a, 0x30, data_size);
    writeLe64(a, 0x38, 0); // initialized_size
    return total;
}

/// Creates a new file `name` in `parent_record` with `data`.  Crash-safe
/// order: dirty -> bitmap -> data -> record(+mirror) -> index -> clear dirty.
pub fn createFile(v: *const Volume, parent_record: u64, name: []const u8, data: []const u8) WriteStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    const data_len: u64 = @intCast(data.len);
    var existing: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &existing)) {
        .found => return .exists,
        .not_found => {},
        .io => return .io,
    }
    const parent_sequence = seqOf(v, parent_record) orelse return .io;

    if (!setDirty(v, true)) return .io;

    const resident = data.len <= RESIDENT_DATA_MAX;
    var runs: [MAX_DATA_RUNS]ntfs.Run = undefined;
    var run_count: usize = 0;
    var alloc_size: u64 = (data_len + 7) & ~@as(u64, 7);
    if (!resident and data.len > 0) {
        if (v.cluster_bytes == 0) return abortWrite(v, .io);
        const rounded_size = checkedAddU64(data_len, @as(u64, v.cluster_bytes) - 1) orelse return abortWrite(v, .invalid);
        const clusters = rounded_size / v.cluster_bytes;
        const allocation = allocateClusters(v, clusters, runs[0..]);
        if (allocation.status != .ok) return abortWrite(v, allocation.status);
        run_count = allocation.produced;
        alloc_size = checkedMulU64(clusters, @as(u64, v.cluster_bytes)) orelse {
            return abortWriteFreeing(v, runs[0..run_count], .io);
        };
        if (!budgetedFlush(v)) return abortWriteFreeing(v, runs[0..run_count], .io);
    }

    const slot = allocateRecord(v) orelse return abortWriteFreeing(v, runs[0..run_count], .no_space);
    if (!budgetedFlush(v)) return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .io);

    // Write payload before the record references it.
    if (!resident and data.len > 0) {
        if (!writeRunBytes(v, runs[0..run_count], 0, data)) return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .io);
        // Zero the slack in the last cluster.
        const tail = alloc_size - data.len;
        if (tail > 0) {
            var zeros: [SECTOR_SIZE]u8 = .{0} ** SECTOR_SIZE;
            var written: u64 = 0;
            while (written < tail) {
                const chunk = @min(tail - written, zeros.len);
                if (!writeRunBytes(v, runs[0..run_count], data.len + written, zeros[0..@intCast(chunk)])) return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .io);
                written += chunk;
            }
        }
        if (!budgetedFlush(v)) return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .io);
    }

    // Build and store the record.
    if (buildFileRecord(v, slot.number, slot.sequence, parent_record, parent_sequence, name, if (resident) data else null, data.len, alloc_size) == 0) {
        return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .invalid);
    }
    if (!resident and data.len > 0) {
        var header = ntfs.FileRecordHeader.parse(v.scratch.write_record[0..v.record_bytes]) orelse return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .io);
        if (!updateNonResident(v.scratch.write_record[0..v.record_bytes], &header, .data, &[_]u8{}, runs[0..run_count], data.len, data.len, alloc_size)) return abortWriteReleasingRecord(v, slot.number, false, runs[0..run_count], .record_full);
    }
    if (!storeRecord(v, slot.number, v.scratch.write_record[0..v.record_bytes])) return .io;
    if (!budgetedFlush(v)) return .io;

    // Insert the index entry.  A logical failure (dir_full needing a B+
    // split, deferred to 0.60.7) rolls the record and clusters back and
    // clears the dirty flag, leaving the directory exactly as before.
    var key: [0x10 + 0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const key_len = buildFileNameKey(v, key[0..], parent_record, parent_sequence, name, false, alloc_size, data.len) orelse {
        return abortWriteReleasingRecord(v, slot.number, true, runs[0..run_count], .invalid);
    };
    const key_ref = ntfs.FileReference.pack(.{ .record = slot.number, .sequence = slot.sequence });
    const insert = indexInsert(v, parent_record, key_ref, key[0..key_len]);
    if (insert != .ok) {
        // An I/O result can mean that the index write reached media but its
        // completion was lost.  Never free a record or clusters that may
        // already be visible through that index; leave the volume dirty.
        if (insert == .io or insert == .cleanup_failed) return insert;
        return abortWriteReleasingRecord(v, slot.number, true, runs[0..run_count], insert);
    }
    if (!budgetedFlush(v)) return .io;

    if (!setDirty(v, false)) return .io;
    return .ok;
}

/// Clears the in-use flag of a record in place (rollback helper).
fn markRecordFree(v: *const Volume, number: u64) bool {
    if (!loadRecordRaw(v, number, v.scratch.write_record[0..])) return false;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (readLe32(record, 0) != ntfs.FILE_MAGIC) return false;
    var flags = readLe16(record, 0x16);
    flags &= ~@as(u16, 0x01);
    writeLe16(record, 0x16, flags);
    return storeRecord(v, number, record);
}

/// Overwrites an existing file, or creates it when absent.
pub fn writeFile(v: *const Volume, parent_record: u64, name: []const u8, data: []const u8) WriteStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    var existing: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &existing)) {
        .found => {},
        .not_found => return createFile(v, parent_record, name, data),
        .io => return .io,
    }
    if (existing.entry.isDir()) return .directory;
    if (!lookupResultIsLive(v, existing)) return .io;
    const link_status = requireSingleLinkStatus(v, existing.record, existing.sequence);
    if (link_status != .ok) return link_status;
    // Simplest correct overwrite: delete then create (frees old clusters,
    // allocates new).  Not atomic, but crash-safe (dirty flag brackets it).
    const del = deleteFile(v, parent_record, name);
    if (del != .ok) return del;
    return createFile(v, parent_record, name, data);
}

/// Deletes a file: dirty -> index remove -> record free -> cluster free ->
/// clear dirty.  A record with more than one hard link only loses this name
/// (index entry + matching $FILE_NAME, link_count decremented); record and
/// clusters survive under the remaining names.
pub fn deleteFile(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    var found: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &found)) {
        .found => {},
        .not_found => return .not_found,
        .io => return .io,
    }
    if (found.entry.isDir()) return .directory;
    if (!lookupResultIsLive(v, found)) return .io;

    // Hard-link case first: it never releases record or clusters.
    {
        const header = loadRecord(v, found.record, v.scratch.part_record[0..]) orelse return .io;
        if (header.sequence != found.sequence) return .io;
        if (header.link_count > 1) return deleteHardLinkName(v, parent_record, name, found.record, found.sequence);
    }

    return releaseFileObjectLocked(v, parent_record, name, found);
}

/// Releases a resolved single-link file object: identity-bound index
/// removal, record (plus any 0.60.16 $DATA extension record) free and
/// cluster release, inside one dirty bracket with the established flush
/// order.  `deleteFile` and the recovery compare-and-delete share this body
/// so both inherit exactly the same crash ordering.
fn releaseFileObjectLocked(v: *const Volume, parent_record: u64, name: []const u8, found: LookupResult) WriteStatus {
    if (!setDirty(v, true)) return .io;

    // Collect the data runs before the record is invalidated.  A spilled
    // file (0.60.16) also owns an extension record that must be freed.
    const data_attr = &v.scratch.attr_op;
    if (collectAttributeStatus(v, found.record, .data, &[_]u8{}, data_attr) != .found) return .io;
    var ext_identity: RecordIdentity = undefined;
    const ext_status = dataExtensionRecordStatus(v, found.record, &ext_identity);
    if (ext_status == .io) return .io;
    const ext_record: ?RecordIdentity = if (ext_status == .found) ext_identity else null;

    // Identity-bound removal: the resolved FileReference must still be the
    // one sitting under this name, so a concurrently republished name can
    // never take a foreign object down with it.
    const remove = indexRemoveIdentity(v, parent_record, name, ntfs.FileReference.pack(.{
        .record = found.record,
        .sequence = found.sequence,
    }));
    if (remove != .ok) return remove;
    if (!budgetedFlush(v)) return .io;

    // Mark the record free and bump its sequence in place.
    if (!loadRecordRaw(v, found.record, v.scratch.write_record[0..])) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (readLe32(record, 0) != ntfs.FILE_MAGIC or
        readLe16(record, 0x10) != found.sequence or
        (readLe16(record, 0x16) & 0x01) == 0)
    {
        return .io;
    }
    var flags = readLe16(record, 0x16);
    flags &= ~@as(u16, 0x01); // clear in_use
    writeLe16(record, 0x16, flags);
    const seq = readLe16(record, 0x10);
    writeLe16(record, 0x10, if (seq == 0xFFFF) 1 else seq + 1);
    if (!storeRecord(v, found.record, record)) return .io;
    if (!releaseRecord(v, found.record)) return .io;
    if (!budgetedFlush(v)) return .io;

    if (ext_record) |ext| {
        if (!loadRecordRaw(v, ext.record, v.scratch.write_record[0..])) return .io;
        const ext_raw = v.scratch.write_record[0..v.record_bytes];
        if (readLe32(ext_raw, 0) != ntfs.FILE_MAGIC or
            readLe16(ext_raw, 0x10) != ext.sequence or
            (readLe16(ext_raw, 0x16) & 0x01) == 0)
        {
            return .io;
        }
        var ext_flags = readLe16(ext_raw, 0x16);
        ext_flags &= ~@as(u16, 0x01);
        writeLe16(ext_raw, 0x16, ext_flags);
        writeLe16(ext_raw, 0x10, if (ext.sequence == 0xFFFF) 1 else ext.sequence + 1);
        if (!storeRecord(v, ext.record, ext_raw)) return .io;
        if (!releaseRecord(v, ext.record)) return .io;
        if (!budgetedFlush(v)) return .io;
    }

    if (!data_attr.resident) {
        if (!freeClusters(v, data_attr.runs[0..data_attr.count])) return .io;
        if (!budgetedFlush(v)) return .io;
    }

    if (!setDirty(v, false)) return .io;
    return .ok;
}

// ---------------------------------------------------------------------------
// Recovery-only compare-and-delete (0.60.21)
// ---------------------------------------------------------------------------

/// Outcome of the recovery-only compare-and-delete.
pub const RecoveryDeleteResult = enum(u8) {
    /// The name is gone and the object itself was released.
    ok,
    /// Only the surplus index alias was detached; the object stays reachable
    /// under its canonical name.
    unlinked,
    /// No such name in this directory.
    not_found,
    /// The name resolves to a different {record, sequence}: a foreign object
    /// or one that has since been replaced.  Never touched.
    mismatch,
    /// The name is a directory; this primitive only reverses file publishes.
    directory,
    /// A half-state this primitive deliberately refuses to guess about
    /// (hard links, or a canonical $FILE_NAME parented outside this
    /// directory).
    unsupported,
    io,
};

/// The directory holding a record's single canonical (non-DOS) $FILE_NAME.
/// Recovery needs it to establish that every name of the object lives in the
/// directory it is about to reason about.
fn canonicalParentOfStatus(
    v: *const Volume,
    record_number: u64,
    expected_sequence: u16,
    out: *ntfs.FileReference,
) LookupStatus {
    const header = loadRecord(v, record_number, v.scratch.part_record[0..]) orelse return .io;
    const record = v.scratch.part_record[0..v.record_bytes];
    if (header.sequence != expected_sequence or !attributeStreamValid(record, header)) return .io;
    if (header.link_count != 1) return .not_found;

    var iterator = ntfs.AttributeIterator.init(record, header);
    var file_name_count: usize = 0;
    var canonical_count: usize = 0;
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.file_name)) continue;
        file_name_count += 1;
        const file_name = ntfs.FileName.parse(attribute.value) orelse return .io;
        if (file_name.namespace == ntfs.NAMESPACE_DOS) continue;
        canonical_count += 1;
        out.* = file_name.parent;
    }
    return if (file_name_count == 1 and canonical_count == 1) .found else .not_found;
}

/// Counts the index entries of `dir_record` that still reference exactly
/// this {record, sequence}.  Removing a name may only release the object
/// when this proves the name was its last one, so a surviving durable alias
/// can never be left pointing at a freed record.
fn countIndexAliases(v: *const Volume, dir_record: u64, record: u64, sequence: u16, out: *usize) bool {
    var sink = EnumSink{
        .count_ref = ntfs.FileReference.pack(.{ .record = record, .sequence = sequence }),
    };
    if (!enumerateDirectory(v, dir_record, &sink) or sink.failed) return false;
    out.* = sink.matches;
    return true;
}

/// Recovery-only compare-and-delete for the alias-first publish windows.
///
/// A crash between the canonical $FILE_NAME rewrite and the target index
/// insert leaves an index entry whose name no longer matches the record's
/// canonical name.  Generic lookups reject that half-state on purpose, so
/// the ordinary identity-bound delete cannot reverse it and the entry would
/// be stuck forever.  This primitive resolves the name through the transient
/// view but only ever acts on an exact {record, sequence} match:
///
///   * the alias is the object's sole index presence -> the object is
///     released (record, any extension record, clusters);
///   * the canonical name is separately indexed to the same object -> only
///     the surplus alias is detached and the object survives.
///
/// A merely equal name, a recycled record and a replaced object all fail as
/// `mismatch` without any write.
pub fn deleteRecoveryEntryIfIdentity(
    v: *const Volume,
    parent_record: u64,
    name: []const u8,
    expected_record: u64,
    expected_sequence: u16,
) RecoveryDeleteResult {
    if (name.len == 0 or name.len > NAME_MAX) return .io;

    var found: LookupResult = undefined;
    switch (lookupInDirectoryStatusTransient(v, parent_record, name, &found)) {
        .found => {},
        .not_found => return .not_found,
        .io => return .io,
    }
    if (found.record != expected_record or found.sequence != expected_sequence) return .mismatch;
    if (found.entry.isDir()) return .directory;
    if (!lookupResultIsLive(v, found)) return .io;

    // Hard links are outside the create-only publish contract that produces
    // these half-states; refuse rather than guess at the right link to drop.
    switch (requireSingleLinkStatus(v, found.record, found.sequence)) {
        .ok => {},
        .unsupported => return .unsupported,
        else => return .io,
    }

    var canonical_parent: ntfs.FileReference = undefined;
    switch (canonicalParentOfStatus(v, found.record, found.sequence, &canonical_parent)) {
        .found => {},
        .not_found => return .unsupported,
        .io => return .io,
    }
    // With a single link and its canonical $FILE_NAME parented here, every
    // name of this object lives in this directory, so the alias count below
    // is complete.  A canonical name parented elsewhere would be a
    // cross-directory half-state the create-only publisher never produces.
    if (canonical_parent.record != parent_record) return .unsupported;

    var alias_count: usize = 0;
    if (!countIndexAliases(v, parent_record, found.record, found.sequence, &alias_count)) return .io;
    // The name was just resolved, so a zero count is corrupt metadata rather
    // than an absence.
    if (alias_count == 0) return .io;

    if (alias_count > 1) {
        if (!setDirty(v, true)) return .io;
        const expected_ref = ntfs.FileReference.pack(.{
            .record = found.record,
            .sequence = found.sequence,
        });
        if (indexRemoveIdentity(v, parent_record, name, expected_ref) != .ok) return .io;
        if (!budgetedFlush(v)) return .io;
        if (!setDirty(v, false)) return .io;
        return .unlinked;
    }

    return switch (releaseFileObjectLocked(v, parent_record, name, found)) {
        .ok => .ok,
        .not_found => .not_found,
        .directory => .directory,
        else => .io,
    };
}

/// Removes one hard-link name: index entry out, the matching $FILE_NAME
/// attribute (this name + parent) out of the record, link_count decremented.
const LinkNameAttrSpan = struct {
    offset: usize,
    length: usize,
};

fn findLinkNameAttrStatus(
    v: *const Volume,
    record: []const u8,
    header: ntfs.FileRecordHeader,
    parent_record: u64,
    parent_sequence: u16,
    target: []const u8,
    out: *LinkNameAttrSpan,
) LookupStatus {
    if (!attributeStreamValid(record, header)) return .io;
    const limit: usize = @intCast(header.bytes_in_use);
    var offset: usize = header.attrs_offset;
    var found = false;
    while (offset + 8 <= limit) {
        const attr_type = readLe32(record, offset);
        if (attr_type == ntfs.END_MARKER) break;
        const length: usize = @intCast(readLe32(record, offset + 4));
        // attributeStreamValid already proved these bounds; keep this helper
        // independently fail-closed if its contract changes later.
        if (length < 0x18 or offset + length > limit) return .io;
        if (attr_type == @intFromEnum(ntfs.AttrType.file_name)) {
            if (record[offset + 8] != 0) return .io;
            const value_length: usize = @intCast(readLe32(record, offset + 0x10));
            const value_offset: usize = readLe16(record, offset + 0x14);
            if (value_offset < 0x18 or value_offset > length or value_length > length - value_offset) return .io;
            const file_name = ntfs.FileName.parse(record[offset + value_offset .. offset + value_offset + value_length]) orelse return .io;
            if (file_name.parent.record == parent_record and
                file_name.parent.sequence == parent_sequence and
                eqlUtf16Upcase(v, file_name.name, target))
            {
                if (found) return .io;
                out.* = .{ .offset = offset, .length = length };
                found = true;
            }
        }
        offset += length;
    }
    return if (found) .found else .not_found;
}

fn deleteHardLinkName(v: *const Volume, parent_record: u64, name: []const u8, record_number: u64, expected_sequence: u16) WriteStatus {
    var target16: [NAME_UNITS_MAX * 2]u8 = undefined;
    const target_len = ntfs.utf8ToUtf16(name, target16[0..]) orelse return .invalid;
    const target = target16[0..target_len];

    var parent_info: RecordLinkInfo = undefined;
    if (recordLinkInfoStatus(v, parent_record, &parent_info) != .found) return .io;
    var link_info: RecordLinkInfo = undefined;
    if (recordLinkInfoStatus(v, record_number, &link_info) != .found or
        link_info.sequence != expected_sequence or
        link_info.link_count <= 1)
    {
        return .io;
    }

    // Prove the exact {parent record, parent sequence, name} attribute before
    // removing the directory index alias.  Record-only comparison could
    // delete a link whose parent record number had already been reused.
    var preflight_span: LinkNameAttrSpan = undefined;
    const preflight_header = loadRecord(v, record_number, v.scratch.write_record[0..]) orelse return .io;
    if (preflight_header.sequence != link_info.sequence or
        findLinkNameAttrStatus(
            v,
            v.scratch.write_record[0..v.record_bytes],
            preflight_header,
            parent_record,
            parent_info.sequence,
            target,
            &preflight_span,
        ) != .found)
    {
        return .io;
    }

    if (!setDirty(v, true)) return .io;

    const remove = indexRemove(v, parent_record, name);
    if (remove != .ok) return remove;
    if (!budgetedFlush(v)) return .io;

    // Drop the $FILE_NAME whose name and parent match this link.
    var header = loadRecord(v, record_number, v.scratch.write_record[0..]) orelse return .io;
    if (header.sequence != link_info.sequence or header.link_count != link_info.link_count) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    var span: LinkNameAttrSpan = undefined;
    if (findLinkNameAttrStatus(
        v,
        record,
        header,
        parent_record,
        parent_info.sequence,
        target,
        &span,
    ) != .found) return .io;
    const tail_start = span.offset + span.length;
    const bytes_in_use: usize = @intCast(header.bytes_in_use);
    const tail_len = bytes_in_use - tail_start;
    for (record[tail_start .. tail_start + tail_len], 0..) |b, i| record[span.offset + i] = b;
    header.bytes_in_use -= @intCast(span.length);
    writeLe32(record, 0x18, header.bytes_in_use);
    writeLe16(record, 0x12, header.link_count - 1);
    if (!storeRecord(v, record_number, record)) return .io;
    if (!budgetedFlush(v)) return .io;

    if (!setDirty(v, false)) return .io;
    return .ok;
}

// ---------------------------------------------------------------------------
// Write phase (0.60.16): $ATTRIBUTE_LIST spill for over-large $DATA runlists
// ---------------------------------------------------------------------------
//
// A FILE record is fixed-size (1 KB here).  A heavily fragmented non-resident
// $DATA whose runlist no longer fits into the record is moved, Windows-style,
// into a dedicated extension record; the base record keeps
// $STANDARD_INFORMATION and $FILE_NAME and gains an $ATTRIBUTE_LIST that maps
// every attribute to its owning record.  The read side (collectAttribute)
// already follows $ATTRIBUTE_LIST into extension records; the write side adds
// the spill and keeps growing the runlist inside the extension record.
//
// Scope: exactly one extension record holds the whole $DATA (lowest_vcn 0).
// A runlist so long it fills even a full extension record (many dozens of
// fragments at 1 KB records) would need a VCN-split across several records;
// that stays a visible record_full and is a documented follow-up.

/// If `base_number` carries an $ATTRIBUTE_LIST that places the unnamed $DATA
/// in a different (extension) record, returns its status and identity-safe
/// record number.
const RecordIdentity = struct {
    record: u64,
    sequence: u16,
};

fn dataExtensionRecordStatus(v: *const Volume, base_number: u64, out: *RecordIdentity) LookupStatus {
    const header = loadRecord(v, base_number, v.scratch.record[0..]) orelse return .io;
    const record = v.scratch.record[0..v.record_bytes];
    if (!attributeStreamValid(record, header)) return .io;
    const list_attr = ntfs.findAttribute(record, header, .attribute_list, &[_]u8{}) orelse return .not_found;
    if (list_attr.non_resident) return .io;
    var iterator = ntfs.AttributeListIterator.init(list_attr.value);
    var extension: ?RecordIdentity = null;
    var saw_unnamed_data = false;
    while (iterator.next()) |entry| {
        if (entry.attr_type != @intFromEnum(ntfs.AttrType.data)) continue;
        if (entry.name.len != 0) continue;
        // The write side deliberately supports one complete unnamed $DATA
        // extent only. Silently ignoring a continuation VCN, or accepting a
        // duplicate list entry for the same record, would let
        // commitDataRunlist overwrite only one part of a foreign split
        // stream.
        if (saw_unnamed_data or entry.lowest_vcn != 0) return .io;
        saw_unnamed_data = true;
        if (entry.mft_reference.record == base_number) {
            if (entry.mft_reference.sequence != header.sequence) return .io;
            const data_attr = ntfs.findAttribute(record, header, .data, &[_]u8{}) orelse return .io;
            if (!data_attr.non_resident or data_attr.lowest_vcn != 0 or
                data_attr.instance != entry.instance)
            {
                return .io;
            }
            continue;
        }
        const ext_header = loadRecord(v, entry.mft_reference.record, v.scratch.part_record[0..]) orelse return .io;
        const ext_record = v.scratch.part_record[0..v.record_bytes];
        if (ext_header.sequence != entry.mft_reference.sequence or
            ext_header.base_record.record != base_number or
            ext_header.base_record.sequence != header.sequence or
            !attributeStreamValid(ext_record, ext_header))
        {
            return .io;
        }
        const data_attr = ntfs.findAttribute(ext_record, ext_header, .data, &[_]u8{}) orelse return .io;
        if (!data_attr.non_resident or data_attr.lowest_vcn != 0 or
            data_attr.instance != entry.instance)
        {
            return .io;
        }
        if (extension) |known| {
            if (known.record != entry.mft_reference.record or
                known.sequence != entry.mft_reference.sequence)
            {
                return .io;
            }
        }
        extension = .{
            .record = entry.mft_reference.record,
            .sequence = entry.mft_reference.sequence,
        };
    }
    if (iterator.offset != list_attr.value.len) return .io;
    if (extension) |identity| {
        out.* = identity;
        return .found;
    }
    return .not_found;
}

fn dataExtensionRecord(v: *const Volume, base_number: u64) ?u64 {
    var identity: RecordIdentity = undefined;
    return if (dataExtensionRecordStatus(v, base_number, &identity) == .found) identity.record else null;
}

/// Test/diagnostic helper: does the named file's $DATA live in an extension
/// record (i.e. has it spilled)?
pub fn dataResidesInExtension(v: *const Volume, parent_record: u64, name: []const u8) bool {
    const found = lookupInDirectory(v, parent_record, name) orelse return false;
    return dataExtensionRecord(v, found.record) != null;
}

/// Builds the $ATTRIBUTE_LIST value for a base record whose $DATA moved to
/// `ext_number`: lists $STANDARD_INFORMATION and $FILE_NAME in the base and
/// the unnamed $DATA in the extension.  Returns the value length.
/// The attribute-list entry instances MUST equal the referenced attribute's
/// real instance id (chkdsk cross-checks {type, name, VCN, instance});
/// mismatched instances are reported as a corrupt attribute-list entry.
fn buildDataAttributeListValue(out: []u8, base_number: u64, base_seq: u16, si_instance: u16, fn_instance: u16, ext_number: u64, ext_seq: u16, data_instance: u16) usize {
    var len: usize = 0;
    const base_ref = ntfs.FileReference.pack(.{ .record = base_number, .sequence = base_seq });
    const ext_ref = ntfs.FileReference.pack(.{ .record = ext_number, .sequence = ext_seq });
    const ListEntry = struct { attr_type: ntfs.AttrType, ref: u64, instance: u16 };
    const entries = [_]ListEntry{
        .{ .attr_type = .standard_information, .ref = base_ref, .instance = si_instance },
        .{ .attr_type = .file_name, .ref = base_ref, .instance = fn_instance },
        .{ .attr_type = .data, .ref = ext_ref, .instance = data_instance },
    };
    for (entries) |e| {
        const entry_len: usize = 0x20; // 0x1A rounded up to 8
        @memset(out[len .. len + entry_len], 0);
        writeLe32(out, len + 0x00, @intFromEnum(e.attr_type));
        writeLe16(out, len + 0x04, @intCast(entry_len));
        out[len + 0x06] = 0; // name length
        out[len + 0x07] = 0x1A; // name offset (no name)
        writeLe64(out, len + 0x08, 0); // lowest_vcn
        writeLe64(out, len + 0x10, e.ref);
        writeLe16(out, len + 0x18, e.instance);
        len += entry_len;
    }
    return len;
}

/// Reads the instance id of the first attribute of `attr_type` (unnamed) in
/// `record`, or null if absent.
fn attributeInstance(record: []const u8, header: ntfs.FileRecordHeader, attr_type: ntfs.AttrType) ?u16 {
    var offset: usize = header.attrs_offset;
    while (offset + 8 <= record.len) {
        const t = readLe32(record, offset);
        if (t == ntfs.END_MARKER) return null;
        const length = readLe32(record, offset + 4);
        if (length < 0x18 or offset + length > record.len) return null;
        if (t == @intFromEnum(attr_type) and record[offset + 9] == 0) return readLe16(record, offset + 14);
        offset += length;
    }
    return null;
}

/// Moves the base record's non-resident $DATA into a fresh extension record
/// and installs an $ATTRIBUTE_LIST in the base.  Runs inside the caller's
/// active dirty bracket; every write goes through storeRecord so fixups and
/// the $MFTMirr stay consistent.
fn spillDataToExtension(v: *const Volume, base_number: u64, runs: []const ntfs.Run, data_size: u64, init_size: u64, alloc_size: u64) WriteStatus {
    const base_sequence = seqOf(v, base_number) orelse return .io;
    const alloc = allocateRecord(v) orelse return .no_space;
    const ext_number = alloc.number;
    const ext_seq = alloc.sequence;

    // Build the extension record: base reference + non-resident $DATA stub,
    // then patch the runlist in.  Uses part_record so write_record stays free
    // for the base.
    const ext = v.scratch.part_record[0..v.record_bytes];
    @memset(ext, 0);
    writeLe32(ext, 0, ntfs.FILE_MAGIC);
    writeLe16(ext, 4, 0x30);
    writeLe16(ext, 6, @intCast(v.record_bytes / SECTOR_SIZE + 1));
    writeLe16(ext, 0x10, ext_seq);
    writeLe16(ext, 0x12, 0); // extension records carry link_count 0
    writeLe16(ext, 0x14, 0x38);
    writeLe16(ext, 0x16, 0x01); // in_use, not directory
    writeLe32(ext, 0x1C, v.record_bytes);
    writeLe64(ext, 0x20, ntfs.FileReference.pack(.{ .record = base_number, .sequence = base_sequence }));
    writeLe32(ext, 0x2C, @intCast(ext_number));
    var instance: u16 = 0;
    var offset: usize = 0x38;
    offset += emitNonResidentDataStub(ext, offset, data_size, alloc_size, &instance);
    writeLe32(ext, offset, ntfs.END_MARKER);
    writeLe32(ext, offset + 4, 0);
    offset += 8;
    writeLe16(ext, 0x28, instance);
    writeLe32(ext, 0x18, @intCast(offset));
    var ext_header = ntfs.FileRecordHeader.parse(ext) orelse return .io;
    if (!updateNonResident(ext, &ext_header, .data, &[_]u8{}, runs, data_size, init_size, alloc_size)) {
        // Even a fresh full extension record cannot hold the runlist: the
        // multi-record VCN split is out of 0.60.16 scope.  Roll back the
        // just-allocated record bit before reporting.
        return rollbackUnpublishedRecord(v, ext_number, false, .record_full);
    }
    if (!storeRecord(v, ext_number, ext)) return .io;
    if (!budgetedFlush(v)) return rollbackUnpublishedRecord(v, ext_number, true, .io);

    // Base record: drop $DATA, add the $ATTRIBUTE_LIST.  Read SI/FN instances
    // first; the extension $DATA has instance 0 (emitNonResidentDataStub).
    const header = loadRecord(v, base_number, v.scratch.write_record[0..]) orelse return rollbackUnpublishedRecord(v, ext_number, true, .io);
    const base = v.scratch.write_record[0..v.record_bytes];
    const si_instance = attributeInstance(base, header, .standard_information) orelse return rollbackUnpublishedRecord(v, ext_number, true, .io);
    const fn_instance = attributeInstance(base, header, .file_name) orelse return rollbackUnpublishedRecord(v, ext_number, true, .io);
    if (!removeAttrRaw(base, .data, &[_]u8{})) return rollbackUnpublishedRecord(v, ext_number, true, .io);
    var list_value: [3 * 0x20]u8 = undefined;
    const list_len = buildDataAttributeListValue(list_value[0..], base_number, header.sequence, si_instance, fn_instance, ext_number, ext_seq, 0);
    var list_attr: [0x18 + 3 * 0x20]u8 = undefined;
    const value_offset: usize = 0x18;
    const attr_total = (value_offset + list_len + 7) & ~@as(usize, 7);
    @memset(list_attr[0..attr_total], 0);
    writeLe32(list_attr[0..], 0, @intFromEnum(ntfs.AttrType.attribute_list));
    writeLe32(list_attr[0..], 4, @intCast(attr_total));
    list_attr[8] = 0; // resident
    writeLe16(list_attr[0..], 10, value_offset);
    writeLe32(list_attr[0..], 0x10, @intCast(list_len));
    writeLe16(list_attr[0..], 0x14, value_offset);
    @memcpy(list_attr[value_offset .. value_offset + list_len], list_value[0..list_len]);
    if (!insertAttrRaw(base, list_attr[0..attr_total])) return rollbackUnpublishedRecord(v, ext_number, true, .record_full);
    if (!storeRecord(v, base_number, base)) return .io;
    return .ok;
}

/// Central $DATA runlist commit for the non-resident append paths: routes to
/// the extension record when the file is already spilled, updates the base
/// otherwise, and spills on record overflow.  The base record's $FILE_NAME
/// duplicate sizes are the caller's responsibility (they always stay in the
/// base record).
fn commitDataRunlist(v: *const Volume, base_number: u64, runs: []const ntfs.Run, data_size: u64, init_size: u64, alloc_size: u64) WriteStatus {
    var ext_identity: RecordIdentity = undefined;
    last_append_diagnostic_stage = 151; // locate DATA extension
    switch (dataExtensionRecordStatus(v, base_number, &ext_identity)) {
        .found => {},
        .not_found => {
            last_append_diagnostic_stage = 152; // load base DATA record
            var header = loadRecord(v, base_number, v.scratch.write_record[0..]) orelse return .io;
            const base = v.scratch.write_record[0..v.record_bytes];
            last_append_diagnostic_stage = 153; // encode base runlist
            if (updateNonResident(base, &header, .data, &[_]u8{}, runs, data_size, init_size, alloc_size)) {
                last_append_diagnostic_stage = 154; // store base runlist
                if (!storeRecord(v, base_number, base)) return .io;
                return .ok;
            }
            last_append_diagnostic_stage = 155; // spill DATA extension
            return spillDataToExtension(v, base_number, runs, data_size, init_size, alloc_size);
        },
        .io => return .io,
    }
    {
        last_append_diagnostic_stage = 156; // load extension DATA record
        var header = loadRecord(v, ext_identity.record, v.scratch.part_record[0..]) orelse return .io;
        last_append_diagnostic_stage = 157; // validate extension identity
        if (header.sequence != ext_identity.sequence) return .io;
        const ext = v.scratch.part_record[0..v.record_bytes];
        last_append_diagnostic_stage = 158; // encode extension runlist
        if (!updateNonResident(ext, &header, .data, &[_]u8{}, runs, data_size, init_size, alloc_size)) return .record_full;
        last_append_diagnostic_stage = 159; // store extension runlist
        if (!storeRecord(v, ext_identity.record, ext)) return .io;
        return .ok;
    }
}

/// Appends `data` at `expected_offset` (must equal the current size), in
/// place: resident growth, resident -> non-resident conversion, writes into
/// existing slack, or cluster extension.  Order per case: dirty -> (bitmap)
/// -> payload -> record sizes/runlist (+FN dup) -> index dup -> clear dirty.
pub fn appendFileAtOffset(v: *const Volume, parent_record: u64, name: []const u8, expected_offset: u64, data: []const u8) WriteStatus {
    return appendFileAtOffsetImpl(v, parent_record, name, expected_offset, data, true);
}

/// Stream-batching variant (0.60.14): same mutation order, but no durable
/// flush per phase and the dirty flag stays SET across the whole stream.
/// setDirty(true) is idempotent-cheap once the volume is dirty, so only
/// the first chunk pays a device flush; finishDeferred() is the single
/// durability point (drain, then clear dirty).  A crash mid-stream leaves
/// a dirty volume with a torn stream file -- exactly the chkdsk-repairable
/// dirty case; consistency is restored by deleting the torn file (SYSUPD
/// recovery does that for stage files).  The durable per-call contract of
/// appendFileAtOffset is unchanged.
pub fn appendFileAtOffsetDeferred(v: *const Volume, parent_record: u64, name: []const u8, expected_offset: u64, data: []const u8) WriteStatus {
    return appendFileAtOffsetImpl(v, parent_record, name, expected_offset, data, false);
}

// Hardware diagnosis for the deferred stream path. The filesystem-request
// gate serializes NTFS operations, so one process-wide stage is sufficient
// to identify the last failed append without changing the public WriteStatus.
// A successful append clears the stage again.
pub var last_append_diagnostic_stage: u32 = 0;

pub fn appendDiagnosticStage() u32 {
    return last_append_diagnostic_stage;
}

/// Durability point for deferred appends: drain every buffered write
/// first, then clear the dirty flag with its own flush so the clear can
/// never overtake the data.
pub fn finishDeferred(v: *const Volume) bool {
    if (!deviceFlush(v)) return false;
    return setDirty(v, false);
}

fn appendFileAtOffsetImpl(v: *const Volume, parent_record: u64, name: []const u8, expected_offset: u64, data: []const u8, durable: bool) WriteStatus {
    last_append_diagnostic_stage = 1; // directory lookup
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    var found: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &found)) {
        .found => {},
        .not_found => return .not_found,
        .io => return .io,
    }
    last_append_diagnostic_stage = 2; // child identity
    if (found.entry.isDir()) return .directory;
    if (!lookupResultIsLive(v, found)) return .io;
    last_append_diagnostic_stage = 3; // data attribute
    const attr = &v.scratch.attr_op;
    if (collectAttributeStatus(v, found.record, .data, &[_]u8{}, attr) != .found) return .io;
    if (attr.data_size != expected_offset) return .offset_mismatch;
    if ((attr.flags & (ntfs.ATTR_FLAG_COMPRESSED | ntfs.ATTR_FLAG_ENCRYPTED)) != 0) return .unsupported;
    if (data.len == 0) {
        last_append_diagnostic_stage = 0;
        return .ok;
    }
    last_append_diagnostic_stage = 4; // single-link preflight
    const link_status = requireSingleLinkStatus(v, found.record, found.sequence);
    if (link_status != .ok) return link_status;
    const data_len: u64 = @intCast(data.len);
    const total = checkedAddU64(attr.data_size, data_len) orelse return .invalid;

    last_append_diagnostic_stage = 5; // dirty bracket
    if (!setDirty(v, true)) return .io;

    if (attr.resident) {
        if (total <= RESIDENT_DATA_MAX) {
            last_append_diagnostic_stage = 6; // resident record update
            // Grow the resident value inside the record.
            var buf: [RESIDENT_DATA_MAX]u8 = undefined;
            const old_len = attr.resident_len;
            @memcpy(buf[0..old_len], attr.resident_copy[0..old_len]);
            @memcpy(buf[old_len..@intCast(total)], data);
            var header = loadRecord(v, found.record, v.scratch.write_record[0..]) orelse return .io;
            const record = v.scratch.write_record[0..v.record_bytes];
            if (updateResident(record, &header, .data, &[_]u8{}, buf[0..@intCast(total)])) {
                const alloc8 = (total + 7) & ~@as(u64, 7);
                _ = updateFileNameDup(record, header, alloc8, total, v.now_filetime);
                if (!storeRecord(v, found.record, record)) return .io;
                if (durable and !budgetedFlush(v)) return .io;
                if (!updateIndexEntrySizes(v, parent_record, name, alloc8, total, v.now_filetime)) return .io;
                if (durable and !budgetedFlush(v)) return .io;
                if (durable and !setDirty(v, false)) return .io;
                last_append_diagnostic_stage = 0;
                return .ok;
            }
            // Record has no room for the larger value: convert below.
        }
        last_append_diagnostic_stage = 7; // resident conversion
        const converted = convertResidentAndAppend(v, parent_record, name, found.record, attr, data, total, durable);
        if (converted == .ok) last_append_diagnostic_stage = 0;
        return converted;
    }

    // Non-resident.  A sparse file first zeroes any mapped gap between
    // initialized_size and data_size (append claims init == new size) and
    // maps the append window if it lands in a hole (0.60.17).
    if ((attr.flags & ntfs.ATTR_FLAG_SPARSE) != 0) {
        last_append_diagnostic_stage = 8; // sparse preparation
        if (attr.initialized_size < attr.data_size) {
            if (!zeroMappedRange(v, attr.runs[0..attr.count], attr.initialized_size, attr.data_size)) return .io;
        }
        // The append window inside the existing VCN space may be a hole;
        // beyond alloc_size the normal extension path allocates anyway.
        const fill_end = if (total < attr.alloc_size) total else attr.alloc_size;
        if (fill_end > attr.data_size) {
            const fill = fillHolesInRuns(v, attr, attr.data_size, fill_end - attr.data_size);
            if (fill != .ok) return abortWrite(v, fill);
        }
    }
    if (total <= attr.alloc_size) {
        // Fits into existing allocation slack.
        last_append_diagnostic_stage = 9; // slack payload
        if (!writeRunBytes(v, attr.runs[0..attr.count], attr.data_size, data)) return .io;
        if (durable and !budgetedFlush(v)) return .io;
        last_append_diagnostic_stage = 10; // slack runlist
        const commit = commitDataRunlist(v, found.record, attr.runs[0..attr.count], total, total, attr.alloc_size);
        if (commit != .ok) return abortWriteAfterCommitFailure(v, commit);
        if (durable and !budgetedFlush(v)) return .io;
        last_append_diagnostic_stage = 11; // slack FILE_NAME
        if (!updateFileNameDupOnBase(v, found.record, attr.alloc_size, total)) return .io;
        if (durable and !budgetedFlush(v)) return .io;
        last_append_diagnostic_stage = 12; // slack directory index
        if (!updateIndexEntrySizes(v, parent_record, name, attr.alloc_size, total, v.now_filetime)) return .io;
        if (durable and !budgetedFlush(v)) return .io;
        if (durable and !setDirty(v, false)) return .io;
        last_append_diagnostic_stage = 0;
        return .ok;
    }

    // Extend the allocation.
    last_append_diagnostic_stage = 13; // allocation geometry
    if (v.cluster_bytes == 0) return abortWrite(v, .io);
    const rounded_total = checkedAddU64(total, @as(u64, v.cluster_bytes) - 1) orelse return abortWrite(v, .invalid);
    const required_clusters = rounded_total / v.cluster_bytes;
    const allocated_clusters = attr.alloc_size / v.cluster_bytes;
    if (required_clusters <= allocated_clusters) return abortWrite(v, .io);
    const need_clusters = required_clusters - allocated_clusters;
    if (attr.count + 8 > attr.runs.len) return abortWrite(v, .record_full);
    var new_runs: [8]ntfs.Run = undefined;
    const allocation = allocateClusters(v, need_clusters, new_runs[0..]);
    if (allocation.status != .ok) return abortWrite(v, allocation.status);
    const produced = allocation.produced;
    if (durable and !budgetedFlush(v)) return abortWriteFreeing(v, new_runs[0..produced], .io);
    var total_runs = attr.count;
    for (new_runs[0..produced]) |run| {
        const adjacent = if (total_runs > 0 and
            attr.runs[total_runs - 1].lcn != null and run.lcn != null)
            (checkedAddU64(
                attr.runs[total_runs - 1].lcn.?,
                attr.runs[total_runs - 1].length_clusters,
            ) orelse {
                return abortWriteFreeing(v, new_runs[0..produced], .io);
            }) == run.lcn.?
        else
            false;
        if (adjacent) {
            attr.runs[total_runs - 1].length_clusters = checkedAddU64(
                attr.runs[total_runs - 1].length_clusters,
                run.length_clusters,
            ) orelse {
                return abortWriteFreeing(v, new_runs[0..produced], .io);
            };
        } else {
            if (total_runs >= attr.runs.len) {
                return abortWriteFreeing(v, new_runs[0..produced], .record_full);
            }
            attr.runs[total_runs] = run;
            total_runs += 1;
        }
    }
    if (!runlistPhysicalRangeValid(v, attr.runs[0..total_runs])) {
        return abortWriteFreeing(v, new_runs[0..produced], .io);
    }
    const extension_bytes = checkedMulU64(need_clusters, @as(u64, v.cluster_bytes)) orelse {
        return abortWriteFreeing(v, new_runs[0..produced], .io);
    };
    const new_alloc = checkedAddU64(attr.alloc_size, extension_bytes) orelse {
        return abortWriteFreeing(v, new_runs[0..produced], .io);
    };
    last_append_diagnostic_stage = 14; // extended payload
    if (!writeRunBytes(v, attr.runs[0..total_runs], attr.data_size, data)) return abortWriteFreeing(v, new_runs[0..produced], .io);
    if (durable and !budgetedFlush(v)) return abortWriteFreeing(v, new_runs[0..produced], .io);
    last_append_diagnostic_stage = 15; // extended runlist
    const commit = commitDataRunlist(v, found.record, attr.runs[0..total_runs], total, total, new_alloc);
    if (commit != .ok) {
        // record_full/no_space are pre-publication failures.  An I/O result
        // may mean the new runlist is already visible and must never be
        // followed by a best-effort free of its clusters.
        if (commit == .record_full or commit == .no_space) {
            return abortWriteFreeing(v, new_runs[0..produced], commit);
        }
        return commit;
    }
    if (durable and !budgetedFlush(v)) return .io;
    last_append_diagnostic_stage = 16; // extended FILE_NAME
    if (!updateFileNameDupOnBase(v, found.record, new_alloc, total)) return .io;
    if (durable and !budgetedFlush(v)) return .io;
    last_append_diagnostic_stage = 17; // extended directory index
    if (!updateIndexEntrySizes(v, parent_record, name, new_alloc, total, v.now_filetime)) return .io;
    if (durable and !budgetedFlush(v)) return .io;
    if (durable and !setDirty(v, false)) return .io;
    last_append_diagnostic_stage = 0;
    return .ok;
}

/// Updates the base record's $FILE_NAME duplicate sizes ($FILE_NAME never
/// moves to an extension record).  Loads, patches and stores the base.
fn updateFileNameDupOnBase(v: *const Volume, base_number: u64, alloc_size: u64, data_size: u64) bool {
    last_append_diagnostic_stage = 161; // load base FILE_NAME
    const header = loadRecord(v, base_number, v.scratch.write_record[0..]) orelse return false;
    const base = v.scratch.write_record[0..v.record_bytes];
    _ = updateFileNameDup(base, header, alloc_size, data_size, v.now_filetime);
    last_append_diagnostic_stage = 162; // store base FILE_NAME
    return storeRecord(v, base_number, base);
}

/// Converts a resident $DATA to non-resident while appending `data`.
fn convertResidentAndAppend(v: *const Volume, parent_record: u64, name: []const u8, record_number: u64, attr: *AttrScratch, data: []const u8, total: u64, durable: bool) WriteStatus {
    if (v.cluster_bytes == 0) return abortWrite(v, .io);
    const rounded_total = checkedAddU64(total, @as(u64, v.cluster_bytes) - 1) orelse return abortWrite(v, .invalid);
    const clusters = rounded_total / v.cluster_bytes;
    var runs: [MAX_DATA_RUNS]ntfs.Run = undefined;
    const allocation = allocateClusters(v, clusters, runs[0..]);
    if (allocation.status != .ok) return abortWrite(v, allocation.status);
    const produced = allocation.produced;
    const alloc_bytes = checkedMulU64(clusters, @as(u64, v.cluster_bytes)) orelse {
        return abortWriteFreeing(v, runs[0..produced], .io);
    };
    if (durable and !budgetedFlush(v)) return abortWriteFreeing(v, runs[0..produced], .io);

    // Old resident payload + new data into the fresh clusters.
    if (attr.resident_len > 0) {
        if (!writeRunBytes(v, runs[0..produced], 0, attr.resident_copy[0..attr.resident_len])) return abortWriteFreeing(v, runs[0..produced], .io);
    }
    if (!writeRunBytes(v, runs[0..produced], attr.resident_len, data)) return abortWriteFreeing(v, runs[0..produced], .io);
    if (durable and !budgetedFlush(v)) return abortWriteFreeing(v, runs[0..produced], .io);

    // Swap the record's $DATA to a non-resident stub with the new runlist.
    var header = loadRecord(v, record_number, v.scratch.write_record[0..]) orelse return abortWriteFreeing(v, runs[0..produced], .io);
    const record = v.scratch.write_record[0..v.record_bytes];
    if (!removeAttrRaw(record, .data, &[_]u8{})) return abortWriteFreeing(v, runs[0..produced], .io);
    var stub: [0x48]u8 = .{0} ** 0x48;
    writeLe32(stub[0..], 0, @intFromEnum(ntfs.AttrType.data));
    writeLe32(stub[0..], 4, 0x48);
    stub[8] = 1; // non-resident
    writeLe16(stub[0..], 0x20, 0x40);
    writeLe64(stub[0..], 0x28, alloc_bytes);
    writeLe64(stub[0..], 0x30, total);
    writeLe64(stub[0..], 0x38, total);
    if (!insertAttrRaw(record, stub[0..])) return abortWriteFreeing(v, runs[0..produced], .io);
    header = ntfs.FileRecordHeader.parse(record) orelse return abortWriteFreeing(v, runs[0..produced], .io);
    if (!updateNonResident(record, &header, .data, &[_]u8{}, runs[0..produced], total, total, alloc_bytes)) {
        return abortWriteFreeing(v, runs[0..produced], .record_full);
    }
    _ = updateFileNameDup(record, header, alloc_bytes, total, v.now_filetime);
    if (!storeRecord(v, record_number, record)) return .io;
    if (durable and !budgetedFlush(v)) return .io;
    if (!updateIndexEntrySizes(v, parent_record, name, alloc_bytes, total, v.now_filetime)) return .io;
    if (durable and !budgetedFlush(v)) return .io;
    if (durable and !setDirty(v, false)) return .io;
    return .ok;
}

// ---------------------------------------------------------------------------
// Write phase 2: directories and rename
// ---------------------------------------------------------------------------

/// Builds a fresh directory record (empty resident $I30 root) into
/// scratch.write_record.
fn buildDirRecord(v: *const Volume, number: u64, sequence: u16, parent_record: u64, parent_sequence: u16, name: []const u8) usize {
    const record = v.scratch.write_record[0..v.record_bytes];
    @memset(record, 0);
    writeLe32(record, 0, ntfs.FILE_MAGIC);
    writeLe16(record, 4, 0x30);
    writeLe16(record, 6, @intCast(v.record_bytes / SECTOR_SIZE + 1));
    writeLe16(record, 0x10, sequence);
    writeLe16(record, 0x12, 1);
    writeLe16(record, 0x14, 0x38);
    writeLe16(record, 0x16, 0x03); // in_use | directory
    writeLe32(record, 0x1C, v.record_bytes);
    writeLe32(record, 0x2C, @intCast(number));
    var offset: usize = 0x38;
    var instance: u16 = 0;

    // $STANDARD_INFORMATION: directories carry attrs 0x00 + dir security id.
    offset += emitAttr(record, offset, .standard_information, &[_]u8{}, false, blk: {
        var si: [0x48]u8 = .{0} ** 0x48;
        var t: usize = 0;
        while (t < 0x20) : (t += 8) writeLe64(si[0..], t, v.now_filetime);
        writeLe32(si[0..], 0x20, 0x00);
        writeLe32(si[0..], 0x34, v.security_id_dir);
        break :blk si[0..];
    }, 0, &instance);

    // $FILE_NAME (directory flag).
    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = buildFileNameValue(v, fn_buf[0..], parent_record, parent_sequence, name, true, 0, 0, 0) orelse return 0;
    offset += emitAttr(record, offset, .file_name, &[_]u8{}, true, fn_buf[0..fn_len], 0, &instance);

    // Empty $INDEX_ROOT ($I30): head + header + END.
    var root_value: [0x30]u8 = .{0} ** 0x30;
    writeLe32(root_value[0..], 0, @intFromEnum(ntfs.AttrType.file_name));
    writeLe32(root_value[0..], 4, ntfs.COLLATION_FILE_NAME);
    writeLe32(root_value[0..], 8, v.index_block_bytes);
    root_value[12] = 1; // clusters per index block
    writeLe32(root_value[0..], 0x10, 0x10); // entries_offset
    writeLe32(root_value[0..], 0x14, 0x20); // index_length
    writeLe32(root_value[0..], 0x18, 0x20); // allocated
    writeLe16(root_value[0..], 0x20 + 8, 0x10);
    writeLe16(root_value[0..], 0x20 + 12, ntfs.INDEX_ENTRY_END);
    offset += emitAttr(record, offset, .index_root, &ntfs.I30_NAME_UTF16, false, root_value[0..], 0, &instance);

    writeLe32(record, offset, ntfs.END_MARKER);
    writeLe32(record, offset + 4, 0);
    offset += 8;
    writeLe16(record, 0x28, instance);
    writeLe32(record, 0x18, @intCast(offset));
    return offset;
}

/// Creates an empty directory `name` in `parent_record`.
pub fn createDirectory(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    if (!indexGeometryOk(v)) return .unsupported;
    var existing: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &existing)) {
        .found => return .exists,
        .not_found => {},
        .io => return .io,
    }
    const parent_sequence = seqOf(v, parent_record) orelse return .io;

    if (!setDirty(v, true)) return .io;
    const slot = allocateRecord(v) orelse return abortWrite(v, .no_space);
    if (!budgetedFlush(v)) return abortWriteReleasingRecord(v, slot.number, false, &.{}, .io);

    if (buildDirRecord(v, slot.number, slot.sequence, parent_record, parent_sequence, name) == 0) {
        return abortWriteReleasingRecord(v, slot.number, false, &.{}, .invalid);
    }
    if (!storeRecord(v, slot.number, v.scratch.write_record[0..v.record_bytes])) return .io;
    if (!budgetedFlush(v)) return .io;

    var key: [0x10 + 0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const key_len = buildFileNameKey(v, key[0..], parent_record, parent_sequence, name, true, 0, 0) orelse {
        return abortWriteReleasingRecord(v, slot.number, true, &.{}, .invalid);
    };
    const key_ref = ntfs.FileReference.pack(.{ .record = slot.number, .sequence = slot.sequence });
    const insert = indexInsert(v, parent_record, key_ref, key[0..key_len]);
    if (insert != .ok) {
        if (insert == .io or insert == .cleanup_failed) return insert;
        return abortWriteReleasingRecord(v, slot.number, true, &.{}, insert);
    }
    if (!budgetedFlush(v)) return .io;
    if (!setDirty(v, false)) return .io;
    return .ok;
}

/// Removes an EMPTY directory.
pub fn deleteDirectory(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    if (name.len == 0 or name.len > NAME_MAX) return .invalid;
    var found: LookupResult = undefined;
    switch (lookupInDirectoryStatus(v, parent_record, name, &found)) {
        .found => {},
        .not_found => return .not_found,
        .io => return .io,
    }
    if (!found.entry.isDir()) return .not_directory;
    if (!lookupResultIsLive(v, found)) return .io;

    var sink = EnumSink{ .wanted = 0 };
    if (!enumerateDirectory(v, found.record, &sink)) return .io;
    if (sink.found != null) return .not_empty;

    if (!setDirty(v, true)) return .io;

    // Collect the $INDEX_ALLOCATION clusters before the record goes away.
    const alloc = &v.scratch.attr_op;
    const alloc_status = collectAttributeStatus(v, found.record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc);
    if (alloc_status == .io) return .io;
    const has_alloc = alloc_status == .found;

    const remove = indexRemove(v, parent_record, name);
    if (remove != .ok) return remove;
    if (!budgetedFlush(v)) return .io;

    if (!loadRecordRaw(v, found.record, v.scratch.write_record[0..])) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (readLe32(record, 0) != ntfs.FILE_MAGIC or
        readLe16(record, 0x10) != found.sequence or
        (readLe16(record, 0x16) & 0x01) == 0)
    {
        return .io;
    }
    var flags = readLe16(record, 0x16);
    flags &= ~@as(u16, 0x01);
    writeLe16(record, 0x16, flags);
    const seq = readLe16(record, 0x10);
    writeLe16(record, 0x10, if (seq == 0xFFFF) 1 else seq + 1);
    if (!storeRecord(v, found.record, record)) return .io;
    if (!releaseRecord(v, found.record)) return .io;
    if (!budgetedFlush(v)) return .io;

    if (has_alloc and !alloc.resident) {
        if (!freeClusters(v, alloc.runs[0..alloc.count])) return .io;
        if (!budgetedFlush(v)) return .io;
    }

    if (!setDirty(v, false)) return .io;
    return .ok;
}

const RenamePhase = enum(u8) {
    initial,
    alias_published,
    canonical_rewritten,
    complete,
    missing,
    conflict,
    unsupported,
    io,
};

/// Classifies every durable rename cut without trusting a name alone.  The
/// transient lookup is intentional: between publish and detach one of the
/// two index aliases cannot match the single canonical $FILE_NAME yet.
fn inspectRenamePhase(
    v: *const Volume,
    old_parent: u64,
    old_name: []const u8,
    new_parent: u64,
    new_name: []const u8,
    subject: *LookupResult,
) RenamePhase {
    const old_parent_sequence = seqOf(v, old_parent) orelse return .io;
    const new_parent_sequence = seqOf(v, new_parent) orelse return .io;

    var old_value: LookupResult = undefined;
    const old_status = lookupInDirectoryStatusTransient(v, old_parent, old_name, &old_value);
    if (old_status == .io) return .io;
    var new_value: LookupResult = undefined;
    const new_status = lookupInDirectoryStatusTransient(v, new_parent, new_name, &new_value);
    if (new_status == .io) return .io;
    if (old_status == .not_found and new_status == .not_found) return .missing;

    if (old_status == .found and !lookupResultIsLive(v, old_value)) return .io;
    if (new_status == .found and !lookupResultIsLive(v, new_value)) return .io;
    if (old_status == .found and new_status == .found) {
        if (old_value.record != new_value.record or old_value.sequence != new_value.sequence) return .conflict;
        if (old_value.entry.isDir() != new_value.entry.isDir()) return .io;
    }

    subject.* = if (old_status == .found) old_value else new_value;
    const link_status = requireSingleLinkStatus(v, subject.record, subject.sequence);
    if (link_status == .unsupported) return .unsupported;
    if (link_status != .ok) return .io;

    const canonical_old = canonicalFileNameStatus(
        v,
        subject.record,
        subject.sequence,
        old_parent,
        old_parent_sequence,
        old_name,
    );
    if (canonical_old == .io) return .io;
    const canonical_new = canonicalFileNameStatus(
        v,
        subject.record,
        subject.sequence,
        new_parent,
        new_parent_sequence,
        new_name,
    );
    if (canonical_new == .io) return .io;
    if (canonical_old == .found and canonical_new == .found) return .conflict;
    if (canonical_old == .not_found and canonical_new == .not_found) return .io;

    if (old_status == .found and new_status == .not_found) {
        return if (canonical_old == .found) .initial else .io;
    }
    if (old_status == .found and new_status == .found) {
        return if (canonical_old == .found) .alias_published else .canonical_rewritten;
    }
    return if (canonical_new == .found) .complete else .io;
}

/// Copies the exact $FILE_NAME key carried by a previously resolved index
/// alias.  A retry uses these bytes for the canonical record rewrite, so
/// timestamps and duplicate fields cannot diverge across a crash.
fn copyRenameAliasFileName(
    v: *const Volume,
    parent_record: u64,
    name: []const u8,
    location: LookupResult,
    out: []u8,
) ?usize {
    const parent_sequence = seqOf(v, parent_record) orelse return null;
    const wanted_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return null;
    const wanted = v.scratch.name_utf16[0..wanted_len];
    var entries: []const u8 = undefined;

    if (location.in_root) {
        const header = loadRecord(v, parent_record, v.scratch.record[0..]) orelse return null;
        if (header.sequence != parent_sequence or !header.isDirectory()) return null;
        const record = v.scratch.record[0..v.record_bytes];
        if (!attributeStreamValid(record, header)) return null;
        const attr = ntfs.findAttribute(record, header, .index_root, &ntfs.I30_NAME_UTF16) orelse return null;
        const root = ntfs.IndexRoot.parse(attr.value) orelse return null;
        entries = root.entries;
    } else {
        const alloc = &v.scratch.attr_index;
        if (collectAttributeStatus(v, parent_record, .index_allocation, &ntfs.I30_NAME_UTF16, alloc) != .found or
            alloc.resident or alloc.count == 0)
        {
            return null;
        }
        if (!loadIndexBlock(v, alloc.runs[0..alloc.count], location.block_vcn)) return null;
        const block = ntfs.IndexBlock.parse(v.scratch.block[0..v.index_block_bytes]) orelse return null;
        if (block.vcn != location.block_vcn) return null;
        entries = block.entries;
    }

    if (location.entry_offset >= entries.len) return null;
    var iterator = ntfs.IndexEntryIterator.init(entries[location.entry_offset..]);
    const index_entry = iterator.next() orelse return null;
    if (index_entry.isEnd()) return null;
    const expected_ref = ntfs.FileReference.pack(.{
        .record = location.record,
        .sequence = location.sequence,
    });
    if (index_entry.file_reference != expected_ref) return null;
    const file_name = index_entry.fileName() orelse return null;
    if (file_name.parent.record != parent_record or
        file_name.parent.sequence != parent_sequence or
        ntfs.compareFileNames(v.upcase, wanted, file_name.name) != .eq or
        index_entry.key.len > out.len)
    {
        return null;
    }
    @memcpy(out[0..index_entry.key.len], index_entry.key);
    return index_entry.key.len;
}

fn canonicalRenameValueStatus(
    v: *const Volume,
    record_number: u64,
    expected_sequence: u16,
    expected_value: []const u8,
) LookupStatus {
    _ = ntfs.FileName.parse(expected_value) orelse return .io;
    const header = loadRecord(v, record_number, v.scratch.part_record[0..]) orelse return .io;
    const record = v.scratch.part_record[0..v.record_bytes];
    if (header.sequence != expected_sequence or header.link_count != 1 or
        !attributeStreamValid(record, header))
    {
        return .io;
    }
    var iterator = ntfs.AttributeIterator.init(record, header);
    var file_name_count: usize = 0;
    var exact_count: usize = 0;
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.file_name)) continue;
        file_name_count += 1;
        if (eqlBytes(attribute.value, expected_value)) exact_count += 1;
    }
    return if (file_name_count == 1 and exact_count == 1) .found else .not_found;
}

fn publishRenameAlias(
    v: *const Volume,
    source: LookupResult,
    new_parent: u64,
    new_name: []const u8,
) WriteStatus {
    const new_parent_sequence = seqOf(v, new_parent) orelse return .io;
    const is_dir = source.entry.isDir();
    var alloc_size: u64 = 0;
    var data_size: u64 = 0;
    if (!is_dir) {
        const attr = &v.scratch.attr_op;
        if (collectAttributeStatus(v, source.record, .data, &[_]u8{}, attr) != .found) return .io;
        alloc_size = attr.alloc_size;
        data_size = attr.data_size;
    }

    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = buildFileNameValue(
        v,
        fn_buf[0..],
        new_parent,
        new_parent_sequence,
        new_name,
        is_dir,
        alloc_size,
        data_size,
        source.entry.created_time_nt,
    ) orelse return abortWrite(v, .invalid);

    // Prove that the canonical value can grow before the destination alias
    // becomes durable.  A deterministic record_full therefore leaves only
    // the original name and may safely clear the dirty bracket.
    var header = loadRecord(v, source.record, v.scratch.write_record[0..]) orelse return .io;
    if (header.sequence != source.sequence or header.link_count != 1) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (!attributeStreamValid(record, header)) return .io;
    if (!updateResident(record, &header, .file_name, &[_]u8{}, fn_buf[0..fn_len])) {
        return abortWrite(v, .record_full);
    }

    var key: [0x10 + 0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const entry_len = (0x10 + fn_len + 7) & ~@as(usize, 7);
    @memset(key[0..entry_len], 0);
    writeLe16(key[0..], 8, @intCast(entry_len));
    writeLe16(key[0..], 10, @intCast(fn_len));
    @memcpy(key[0x10 .. 0x10 + fn_len], fn_buf[0..fn_len]);
    const key_ref = ntfs.FileReference.pack(.{
        .record = source.record,
        .sequence = source.sequence,
    });
    return indexInsert(v, new_parent, key_ref, key[0..entry_len]);
}

fn rewriteRenameCanonical(
    v: *const Volume,
    subject: LookupResult,
    new_parent: u64,
    new_name: []const u8,
) WriteStatus {
    var target: LookupResult = undefined;
    if (lookupInDirectoryStatusTransient(v, new_parent, new_name, &target) != .found or
        target.record != subject.record or target.sequence != subject.sequence)
    {
        return .io;
    }
    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = copyRenameAliasFileName(v, new_parent, new_name, target, fn_buf[0..]) orelse return .io;
    const file_name = ntfs.FileName.parse(fn_buf[0..fn_len]) orelse return .io;
    if (((file_name.flags & ntfs.FILE_ATTR_DIRECTORY_DUP) != 0) != subject.entry.isDir()) return .io;

    var header = loadRecord(v, subject.record, v.scratch.write_record[0..]) orelse return .io;
    if (header.sequence != subject.sequence or header.link_count != 1) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (!attributeStreamValid(record, header)) return .io;
    if (!updateResident(record, &header, .file_name, &[_]u8{}, fn_buf[0..fn_len])) return .io;
    return if (storeRecord(v, subject.record, record)) .ok else .io;
}

fn detachRenameSource(
    v: *const Volume,
    subject: LookupResult,
    old_parent: u64,
    old_name: []const u8,
    new_parent: u64,
    new_name: []const u8,
) WriteStatus {
    var target: LookupResult = undefined;
    if (lookupInDirectoryStatusTransient(v, new_parent, new_name, &target) != .found or
        target.record != subject.record or target.sequence != subject.sequence)
    {
        return .io;
    }
    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = copyRenameAliasFileName(v, new_parent, new_name, target, fn_buf[0..]) orelse return .io;
    if (canonicalRenameValueStatus(v, subject.record, subject.sequence, fn_buf[0..fn_len]) != .found) return .io;
    const expected_ref = ntfs.FileReference.pack(.{
        .record = subject.record,
        .sequence = subject.sequence,
    });
    return indexRemoveIdentity(v, old_parent, old_name, expected_ref);
}

/// Renames (and optionally moves) a file or directory.  The destination
/// FileReference alias is made durable first, the canonical $FILE_NAME is
/// then rewritten from that exact index key, and only then is the source
/// alias detached by identity.
pub fn renameEntry(v: *const Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) WriteStatus {
    if (old_name.len == 0 or old_name.len > NAME_MAX or new_name.len == 0 or new_name.len > NAME_MAX) return .invalid;
    var subject: LookupResult = undefined;
    const phase = inspectRenamePhase(v, old_parent, old_name, new_parent, new_name, &subject);
    switch (phase) {
        .missing => return .not_found,
        .conflict => return .exists,
        .unsupported => return .unsupported,
        .io => return .io,
        .complete => {
            // A target-only clean state cannot prove that this caller
            // performed the rename; it may be an unrelated pre-existing
            // object next to a missing source.  Only a still-dirty volume
            // can be the durable final cut of the alias-first sequence.
            const dirty = isDirty(v) orelse return .io;
            if (!dirty) return .not_found;
        },
        .alias_published, .canonical_rewritten => {
            if (!(isDirty(v) orelse return .io)) return .io;
        },
        .initial => {},
    }
    if (!setDirty(v, true)) return .io;
    const st = renameLocked(v, old_parent, old_name, new_parent, new_name, true);
    if (st != .ok) return st;
    if (!setDirty(v, false)) return .io;
    return .ok;
}

/// Idempotent rename core inside an active dirty bracket.  `flush_steps` is
/// true for the public operation and makes each phase independently
/// replayable after a lost completion or reset.
fn renameLocked(v: *const Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8, flush_steps: bool) WriteStatus {
    var transitions: usize = 0;
    while (transitions < 4) : (transitions += 1) {
        var subject: LookupResult = undefined;
        const phase = inspectRenamePhase(v, old_parent, old_name, new_parent, new_name, &subject);
        const step = switch (phase) {
            .initial => publishRenameAlias(v, subject, new_parent, new_name),
            .alias_published => rewriteRenameCanonical(v, subject, new_parent, new_name),
            .canonical_rewritten => detachRenameSource(v, subject, old_parent, old_name, new_parent, new_name),
            .complete => return .ok,
            .missing => return .not_found,
            .conflict => return .exists,
            .unsupported => return .unsupported,
            .io => return .io,
        };
        if (step != .ok) return step;
        if (flush_steps and !budgetedFlush(v)) return .io;
    }
    return .io;
}

// ---------------------------------------------------------------------------
// Write phase 3 (0.60.8): atomic ownership transfer
// ---------------------------------------------------------------------------

pub const ReplaceResult = enum(u8) {
    ok,
    invalid,
    not_found,
    alias,
    conflict,
    read_only,
    io,
    not_atomic,
};

fn canonicalFileNameStatus(
    v: *const Volume,
    record_number: u64,
    expected_sequence: u16,
    parent_record: u64,
    parent_sequence: u16,
    name: []const u8,
) LookupStatus {
    const wanted_len = ntfs.utf8ToUtf16(name, v.scratch.name_utf16[0..]) orelse return .io;
    const wanted = v.scratch.name_utf16[0..wanted_len];
    const header = loadRecord(v, record_number, v.scratch.part_record[0..]) orelse return .io;
    if (header.sequence != expected_sequence or !attributeStreamValid(v.scratch.part_record[0..v.record_bytes], header)) return .io;
    if (header.link_count != 1) return .not_found;
    const record = v.scratch.part_record[0..v.record_bytes];
    var iterator = ntfs.AttributeIterator.init(record, header);
    var file_name_count: usize = 0;
    var canonical_count: usize = 0;
    var matches = false;
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(ntfs.AttrType.file_name)) continue;
        file_name_count += 1;
        const file_name = ntfs.FileName.parse(attribute.value) orelse return .io;
        if (file_name.namespace == ntfs.NAMESPACE_DOS) continue;
        canonical_count += 1;
        if (file_name.parent.record == parent_record and
            file_name.parent.sequence == parent_sequence and
            eqlUtf16Upcase(v, file_name.name, wanted))
        {
            matches = true;
        }
    }
    // The create-only publisher deliberately accepts only a single-link
    // canonical record.  Hard-linked/foreign records are a conflict, not an
    // object that this operation may retarget.
    return if (file_name_count == 1 and canonical_count == 1 and matches) .found else .not_found;
}

fn rewriteSingleCanonicalFileName(
    v: *const Volume,
    entry: LookupResult,
    parent_record: u64,
    parent_sequence: u16,
    target_name: []const u8,
) WriteStatus {
    const data_attr = &v.scratch.attr_op;
    if (collectAttributeStatus(v, entry.record, .data, &[_]u8{}, data_attr) != .found) return .io;
    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = buildFileNameValue(
        v,
        fn_buf[0..],
        parent_record,
        parent_sequence,
        target_name,
        false,
        data_attr.alloc_size,
        data_attr.data_size,
        entry.entry.created_time_nt,
    ) orelse return .invalid;

    var header = loadRecord(v, entry.record, v.scratch.write_record[0..]) orelse return .io;
    if (header.sequence != entry.sequence or header.link_count != 1) return .io;
    const record = v.scratch.write_record[0..v.record_bytes];
    if (!attributeStreamValid(record, header)) return .io;
    if (!updateResident(record, &header, .file_name, &[_]u8{}, fn_buf[0..fn_len])) return .record_full;
    if (!storeRecord(v, entry.record, record)) return .io;
    return .ok;
}

/// Create-only ownership transfer for inbox-style publishing.
///
/// The canonical $FILE_NAME is rewritten first, then a target index alias
/// carrying the exact stage FileReference is inserted and durably flushed.
/// Only after that publish point is the stale stage index alias removed.
/// If a prior attempt stopped after publishing, a retry finalizes only when
/// target and stage are the same live {record, sequence}; no by-name delete
/// is used and a foreign target is never consumed.
pub fn publishFileCreateOnly(
    v: *const Volume,
    parent_record: u64,
    target_name: []const u8,
    staged_name: []const u8,
) ReplaceResult {
    if (target_name.len == 0 or target_name.len > NAME_MAX or
        staged_name.len == 0 or staged_name.len > NAME_MAX)
    {
        return .invalid;
    }
    const target_stage_alias = namesEqualUpcase(v, target_name, staged_name) orelse return .invalid;
    if (target_stage_alias) return .alias;

    const parent_sequence = seqOf(v, parent_record) orelse return .io;

    var target_value: LookupResult = undefined;
    const target_status = lookupInDirectoryStatusTransient(v, parent_record, target_name, &target_value);
    if (target_status == .io) return .io;
    var staged_value: LookupResult = undefined;
    const staged_status = lookupInDirectoryStatusTransient(v, parent_record, staged_name, &staged_value);
    if (staged_status == .io) return .io;

    if (target_status == .found) {
        if (staged_status != .found) {
            // Target-only cannot prove that this caller published it.
            return .conflict;
        }
        if (target_value.record != staged_value.record or
            target_value.sequence != staged_value.sequence)
        {
            return .conflict;
        }
        if (!lookupResultIsLive(v, target_value) or !lookupResultIsLive(v, staged_value)) return .io;
        if (target_value.entry.isDir() or staged_value.entry.isDir() or
            (target_value.entry.attr & ATTR_READ_ONLY) != 0 or
            (staged_value.entry.attr & ATTR_READ_ONLY) != 0)
        {
            return .read_only;
        }
        const canonical_target = canonicalFileNameStatus(
            v,
            target_value.record,
            target_value.sequence,
            parent_record,
            parent_sequence,
            target_name,
        );
        if (canonical_target == .io) return .io;
        const canonical_stage = canonicalFileNameStatus(
            v,
            target_value.record,
            target_value.sequence,
            parent_record,
            parent_sequence,
            staged_name,
        );
        if (canonical_stage == .io) return .io;
        if (canonical_target != .found and canonical_stage != .found) return .conflict;

        if (!ensureDirtyDurable(v)) return .io;
        if (canonical_target != .found) {
            // The target index alias reached disk before the canonical
            // $FILE_NAME rewrite. Complete that half-state first and make it
            // durable before the stage index can disappear.
            if (rewriteSingleCanonicalFileName(
                v,
                staged_value,
                parent_record,
                parent_sequence,
                target_name,
            ) != .ok) return .io;
            if (!budgetedFlush(v)) return .io;
        }
        if (indexRemove(v, parent_record, staged_name) != .ok) return .io;
        if (!budgetedFlush(v)) return .io;
        if (!setDirty(v, false)) return .io;
        return .ok;
    }

    if (staged_status != .found) return .not_found;
    if (!lookupResultIsLive(v, staged_value)) return .io;
    if (staged_value.entry.isDir() or (staged_value.entry.attr & ATTR_READ_ONLY) != 0) return .read_only;

    const canonical_stage = canonicalFileNameStatus(
        v,
        staged_value.record,
        staged_value.sequence,
        parent_record,
        parent_sequence,
        staged_name,
    );
    if (canonical_stage == .io) return .io;
    const canonical_target = canonicalFileNameStatus(
        v,
        staged_value.record,
        staged_value.sequence,
        parent_record,
        parent_sequence,
        target_name,
    );
    if (canonical_target == .io) return .io;
    if (canonical_stage != .found and canonical_target != .found) return .conflict;

    const data_attr = &v.scratch.attr_op;
    if (collectAttributeStatus(v, staged_value.record, .data, &[_]u8{}, data_attr) != .found) return .io;
    const alloc_size = data_attr.alloc_size;
    const data_size = data_attr.data_size;

    var fn_buf: [0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const fn_len = buildFileNameValue(
        v,
        fn_buf[0..],
        parent_record,
        parent_sequence,
        target_name,
        false,
        alloc_size,
        data_size,
        staged_value.entry.created_time_nt,
    ) orelse return .invalid;

    if (!ensureDirtyDurable(v)) return .io;

    if (canonical_target != .found) {
        var header = loadRecord(v, staged_value.record, v.scratch.write_record[0..]) orelse return .io;
        if (header.sequence != staged_value.sequence or header.link_count != 1) return .conflict;
        const record = v.scratch.write_record[0..v.record_bytes];
        if (!attributeStreamValid(record, header)) return .io;
        if (!updateResident(record, &header, .file_name, &[_]u8{}, fn_buf[0..fn_len])) return .io;
        if (!storeRecord(v, staged_value.record, record)) return .io;
        if (cut_after_canonical_rewrite) return .io;
    }

    var key: [0x10 + 0x42 + 2 * NAME_UNITS_MAX]u8 = undefined;
    const entry_len = (0x10 + fn_len + 7) & ~@as(usize, 7);
    @memset(key[0..entry_len], 0);
    writeLe16(key[0..], 8, @intCast(entry_len));
    writeLe16(key[0..], 10, @intCast(fn_len));
    @memcpy(key[0x10 .. 0x10 + fn_len], fn_buf[0..fn_len]);
    const key_ref = ntfs.FileReference.pack(.{
        .record = staged_value.record,
        .sequence = staged_value.sequence,
    });
    if (indexInsert(v, parent_record, key_ref, key[0..entry_len]) != .ok) return .io;
    if (!budgetedFlush(v)) return .io;

    // Re-resolve the just-published alias before detaching anything.  Both
    // index names must still identify the exact stage object.
    var published: LookupResult = undefined;
    if (lookupInDirectoryStatusTransient(v, parent_record, target_name, &published) != .found or
        published.record != staged_value.record or
        published.sequence != staged_value.sequence)
    {
        return .io;
    }
    var still_staged: LookupResult = undefined;
    if (lookupInDirectoryStatusTransient(v, parent_record, staged_name, &still_staged) != .found or
        still_staged.record != staged_value.record or
        still_staged.sequence != staged_value.sequence)
    {
        return .io;
    }

    if (indexRemove(v, parent_record, staged_name) != .ok) return .io;
    if (!budgetedFlush(v)) return .io;
    if (!setDirty(v, false)) return .io;
    return .ok;
}

/// Atomic replace with the SYSUPD ownership-transfer semantics: the staged
/// file takes over the target name, the previous target survives under the
/// backup name.
///
/// Each leg uses the create-only alias publisher: publish the destination
/// alias durably before detaching the source alias. This avoids the old
/// record-first rename window in which an I/O failure after index removal
/// could leave the object nameless. The two independently replayable legs
/// are target->backup and staged->target. A crash between them leaves the
/// last-good object under backup and the new object under stage, with the
/// volume dirty for recovery before normal runtime.
pub fn replaceFileAtomic(v: *const Volume, parent_record: u64, target_name: []const u8, staged_name: []const u8, backup_name: []const u8, consume_stage: bool) ReplaceResult {
    if (!consume_stage) return .not_atomic;
    if (target_name.len == 0 or target_name.len > NAME_MAX) return .invalid;
    if (!is83Safe(staged_name) or !is83Safe(backup_name)) return .invalid;
    const target_stage_alias = namesEqualUpcase(v, target_name, staged_name) orelse return .invalid;
    const target_backup_alias = namesEqualUpcase(v, target_name, backup_name) orelse return .invalid;
    const stage_backup_alias = namesEqualUpcase(v, staged_name, backup_name) orelse return .invalid;
    if (target_stage_alias or target_backup_alias or stage_backup_alias) return .alias;

    var target_value: LookupResult = undefined;
    const target_status = lookupInDirectoryStatusTransient(v, parent_record, target_name, &target_value);
    if (target_status == .io) return .io;
    const target: ?LookupResult = if (target_status == .found) target_value else null;
    if (target) |t| {
        if (!lookupResultIsLive(v, t)) return .io;
        if (t.entry.isDir() or (t.entry.attr & ATTR_READ_ONLY) != 0) return .read_only;
    }
    var backup_value: LookupResult = undefined;
    const backup_status = lookupInDirectoryStatusTransient(v, parent_record, backup_name, &backup_value);
    if (backup_status == .io) return .io;
    const backup: ?LookupResult = if (backup_status == .found) backup_value else null;
    if (backup) |b| {
        if (!lookupResultIsLive(v, b)) return .io;
        if (b.entry.isDir() or (b.entry.attr & ATTR_READ_ONLY) != 0) return .conflict;
    }
    var staged_value: LookupResult = undefined;
    const staged_status = lookupInDirectoryStatusTransient(v, parent_record, staged_name, &staged_value);
    if (staged_status == .io) return .io;
    const staged: ?LookupResult = if (staged_status == .found) staged_value else null;
    if (staged == null) {
        // Idempotent completion: a consumed stage next to a present target.
        // Drain pending writes and durably clear a Dirty bit whose final
        // completion may have been lost on the previous call.
        if (target == null) return .not_found;
        return if (finishDeferred(v)) .ok else .io;
    }
    if (!lookupResultIsLive(v, staged.?)) return .io;
    if (staged.?.entry.isDir() or (staged.?.entry.attr & ATTR_READ_ONLY) != 0) return .read_only;

    if (target) |t| {
        if (sameLookupIdentity(t, staged.?)) {
            // Second leg published the target alias but did not yet detach
            // stage. The create-only publisher verifies the exact identity
            // before finalizing.
            return publishFileCreateOnly(v, parent_record, target_name, staged_name);
        }
        if (backup) |b| {
            if (sameLookupIdentity(t, b)) {
                // First leg published backup but did not yet detach target.
                const first = publishFileCreateOnly(v, parent_record, backup_name, target_name);
                if (first != .ok) return first;
                return publishFileCreateOnly(v, parent_record, target_name, staged_name);
            }
            // Three distinct live objects cannot be produced by either leg.
            return .conflict;
        }
        const first = publishFileCreateOnly(v, parent_record, backup_name, target_name);
        if (first != .ok) return first;
        return publishFileCreateOnly(v, parent_record, target_name, staged_name);
    }

    // Either the original target never existed, or target->backup completed.
    // A pre-existing backup is accepted only in this replay shape; callers
    // additionally bind its expected content.
    return publishFileCreateOnly(v, parent_record, target_name, staged_name);
}

fn sameLookupIdentity(a: LookupResult, b: LookupResult) bool {
    return a.record == b.record and a.sequence == b.sequence;
}

/// Backend-exact name comparison (0.60.24).
///
/// NTFS decides name identity through `$UpCase`, which folds far more than
/// ASCII.  Callers that need to know whether two names would resolve to the
/// same object on THIS volume must ask the backend instead of folding bytes
/// themselves; a byte-wise ASCII fold is only a proof for ASCII.
pub fn namesEqualCollated(v: *const Volume, a: []const u8, b: []const u8) ?bool {
    if (a.len == 0 or a.len > NAME_MAX or b.len == 0 or b.len > NAME_MAX) return null;
    return namesEqualUpcase(v, a, b);
}

fn namesEqualUpcase(v: *const Volume, a: []const u8, b: []const u8) ?bool {
    var a_utf16: [NAME_UNITS_MAX * 2]u8 = undefined;
    var b_utf16: [NAME_UNITS_MAX * 2]u8 = undefined;
    const a_len = ntfs.utf8ToUtf16(a, a_utf16[0..]) orelse return null;
    const b_len = ntfs.utf8ToUtf16(b, b_utf16[0..]) orelse return null;
    return ntfs.compareFileNames(v.upcase, a_utf16[0..a_len], b_utf16[0..b_len]) == .eq;
}

// ---------------------------------------------------------------------------
// Write phase (0.60.17): sparse holes
// ---------------------------------------------------------------------------
//
// Writes into holes of a sparse-flagged $DATA map the hole on demand:
// clusters are allocated, the unmapped run is split into
// [hole][mapped][hole], the fresh clusters are zero-filled (the cluster
// slop around the payload must keep reading as zeros) and the runlist is
// committed through commitDataRunlist (base or extension record, 0.60.16).
// Untouched holes stay holes; creating new holes (hole punching) stays
// visibly unsupported.  initialized_size follows Windows semantics: bytes
// in [initialized_size, data_size) read as zeros, and a write beyond
// initialized_size first zeroes any MAPPED clusters in the gap before
// raising it.

const ZERO_CHUNK = [_]u8{0} ** 4096;

/// Appends a run, coalescing with the previous one when both are holes or
/// physically contiguous mapped extents.
fn pushRun(out: []ntfs.Run, count: *usize, run: ntfs.Run) bool {
    if (run.length_clusters == 0) return true;
    if (count.* > 0) {
        const prev = &out[count.* - 1];
        if (prev.lcn == null and run.lcn == null) {
            prev.length_clusters = checkedAddU64(prev.length_clusters, run.length_clusters) orelse return false;
            return true;
        }
        if (prev.lcn != null and run.lcn != null) {
            const previous_end = checkedAddU64(prev.lcn.?, prev.length_clusters) orelse return false;
            if (previous_end == run.lcn.?) {
                prev.length_clusters = checkedAddU64(prev.length_clusters, run.length_clusters) orelse return false;
                return true;
            }
        }
    }
    if (count.* >= out.len) return false;
    out[count.*] = run;
    count.* += 1;
    return true;
}

/// Zero-fills every cluster of one mapped run.
fn zeroRunClusters(v: *const Volume, run: ntfs.Run) bool {
    const lcn = run.lcn orelse return true;
    if (!runlistPhysicalRangeValid(v, &[_]ntfs.Run{run})) return false;
    const total = checkedMulU64(run.length_clusters, @as(u64, v.cluster_bytes)) orelse return false;
    var pos: u64 = 0;
    while (pos < total) {
        const take = @min(total - pos, ZERO_CHUNK.len);
        if (!writeLcnBytes(v, lcn, pos, ZERO_CHUNK[0..@intCast(take)])) return false;
        pos += take;
    }
    return true;
}

/// Zeroes the MAPPED parts of the byte range [from, to) in the runlist's
/// VCN space; holes are skipped (they read as zeros anyway).
fn zeroMappedRange(v: *const Volume, runs: []const ntfs.Run, from: u64, to: u64) bool {
    if (to <= from) return true;
    const cluster: u64 = v.cluster_bytes;
    if (cluster == 0 or !runlistPhysicalRangeValid(v, runs)) return false;
    var run_start: u64 = 0;
    for (runs) |run| {
        const run_bytes = checkedMulU64(run.length_clusters, cluster) orelse return false;
        const run_end = checkedAddU64(run_start, run_bytes) orelse return false;
        if (run_start >= to) break;
        if (run.lcn) |lcn| {
            if (run_end > from) {
                var pos = if (run_start > from) run_start else from;
                const stop = if (run_end < to) run_end else to;
                while (pos < stop) {
                    const take = @min(stop - pos, ZERO_CHUNK.len);
                    if (!writeLcnBytes(v, lcn, pos - run_start, ZERO_CHUNK[0..@intCast(take)])) return false;
                    pos += take;
                }
            }
        }
        run_start = run_end;
    }
    return true;
}

/// Does the byte range [offset, offset+len) overlap any hole in the runlist?
fn rangeOverlapsHole(v: *const Volume, runs: []const ntfs.Run, offset: u64, len: u64) ?bool {
    if (len == 0) return false;
    const range_end = checkedAddU64(offset, len) orelse return null;
    const cluster: u64 = v.cluster_bytes;
    if (cluster == 0 or !runlistPhysicalRangeValid(v, runs)) return null;
    var run_start: u64 = 0;
    for (runs) |run| {
        const run_bytes = checkedMulU64(run.length_clusters, cluster) orelse return null;
        const run_end = checkedAddU64(run_start, run_bytes) orelse return null;
        if (run_start >= range_end) break;
        if (run.lcn == null and run_end > offset) return true;
        run_start = run_end;
    }
    return false;
}

/// Maps every hole overlapping [offset, offset+len): allocates clusters,
/// splits the unmapped runs and zero-fills the fresh clusters.  Mutates
/// `attr.runs`/`attr.count` in place; the caller commits the runlist and
/// runs inside an active dirty bracket.  On failure every cluster already
/// allocated here is returned to the bitmap.
fn fillHolesInRuns(v: *const Volume, attr: *AttrScratch, offset: u64, len: u64) WriteStatus {
    if (len == 0) return .ok;
    const cluster: u64 = v.cluster_bytes;
    if (cluster == 0 or !runlistPhysicalRangeValid(v, attr.runs[0..attr.count])) return .io;
    const range_end = checkedAddU64(offset, len) orelse return .io;
    const first_vcn = offset / cluster;
    const last_vcn = (range_end - 1) / cluster;

    var out: [MAX_ATTR_RUNS]ntfs.Run = undefined;
    var out_count: usize = 0;
    var fresh: [32]ntfs.Run = undefined;
    var fresh_count: usize = 0;
    var status: WriteStatus = .ok;
    var changed = false;
    var vcn: u64 = 0;

    build: for (attr.runs[0..attr.count]) |run| {
        const run_end = checkedAddU64(vcn, run.length_clusters) orelse {
            status = .io;
            break :build;
        };
        if (run.lcn != null or run_end <= first_vcn or vcn > last_vcn) {
            if (!pushRun(out[0..], &out_count, run)) {
                status = .record_full;
                break :build;
            }
            vcn = run_end;
            continue;
        }
        // Hole overlapping the target range: split into up to three pieces.
        const fill_from = if (vcn > first_vcn) vcn else first_vcn;
        const fill_to = if (run_end - 1 < last_vcn) run_end - 1 else last_vcn;
        if (!pushRun(out[0..], &out_count, .{ .lcn = null, .length_clusters = fill_from - vcn })) {
            status = .record_full;
            break :build;
        }
        var need = fill_to - fill_from + 1;
        while (need > 0) {
            if (fresh_count + 8 > fresh.len) {
                status = .record_full;
                break :build;
            }
            var new_runs: [8]ntfs.Run = undefined;
            const allocation = allocateClusters(v, need, new_runs[0..]);
            if (allocation.status != .ok) {
                status = allocation.status;
                break :build;
            }
            const produced = allocation.produced;
            for (new_runs[0..produced]) |nr| {
                fresh[fresh_count] = nr;
                fresh_count += 1;
                need -= nr.length_clusters;
                if (!zeroRunClusters(v, nr)) {
                    status = .io;
                    break :build;
                }
                if (!pushRun(out[0..], &out_count, nr)) {
                    status = .record_full;
                    break :build;
                }
            }
        }
        if (!pushRun(out[0..], &out_count, .{ .lcn = null, .length_clusters = (run_end - 1) - fill_to })) {
            status = .record_full;
            break :build;
        }
        changed = true;
        vcn = run_end;
    }

    if (status != .ok) {
        if (fresh_count != 0 and !freeClusters(v, fresh[0..fresh_count])) return .cleanup_failed;
        return status;
    }
    if (!changed) return .ok;
    @memcpy(attr.runs[0..out_count], out[0..out_count]);
    attr.count = out_count;
    return .ok;
}

/// In-place range write inside the existing file content (no size change):
/// the pager and random-access writers patch bytes without touching any
/// metadata, so no dirty bracket is needed.  Writes beyond the initialized
/// content are refused (extension goes through appendFileAtOffset); on a
/// sparse file they are allowed up to data_size and raise initialized_size
/// (0.60.17), and writes into holes allocate + split inside a dirty bracket.
pub fn writeFileAt(v: *const Volume, record_number: u64, offset: u64, data: []const u8) WriteStatus {
    if (data.len == 0) return .ok;
    const data_len: u64 = @intCast(data.len);
    const write_end = checkedAddU64(offset, data_len) orelse return .offset_mismatch;
    const attr = &v.scratch.attr_op;
    if (!collectAttribute(v, record_number, .data, &[_]u8{}, attr)) return .io;
    if ((attr.flags & (ntfs.ATTR_FLAG_COMPRESSED | ntfs.ATTR_FLAG_ENCRYPTED)) != 0) return .unsupported;
    if (write_end > attr.data_size) return .offset_mismatch;

    if (attr.resident) {
        // Patch the resident value inside the record.
        var header = loadRecord(v, record_number, v.scratch.write_record[0..]) orelse return .io;
        const record = v.scratch.write_record[0..v.record_bytes];
        var value: [RESIDENT_DATA_MAX]u8 = undefined;
        if (attr.resident_len > value.len) return .io;
        @memcpy(value[0..attr.resident_len], attr.resident_copy[0..attr.resident_len]);
        @memcpy(value[@intCast(offset)..@intCast(write_end)], data);
        if (!updateResident(record, &header, .data, &[_]u8{}, value[0..attr.resident_len])) return .io;
        if (!storeRecord(v, record_number, record)) return .io;
        return .ok;
    }

    const is_sparse = (attr.flags & ntfs.ATTR_FLAG_SPARSE) != 0;
    if (write_end > attr.initialized_size and !is_sparse) return .offset_mismatch;

    const overlaps_hole = rangeOverlapsHole(v, attr.runs[0..attr.count], offset, data_len) orelse return .io;
    const needs_metadata = is_sparse and
        (overlaps_hole or write_end > attr.initialized_size);
    if (!needs_metadata) {
        if (!writeRunBytes(v, attr.runs[0..attr.count], offset, data)) return .io;
        // Pure data writes stay lazy: no metadata changed, the page-cache
        // writeback worker drains the dirty pages.  A device flush per random
        // write made the pager/tooling paths measurably too slow.
        return .ok;
    }

    // Sparse metadata path: dirty -> bitmap+zero (fill) -> init gap zero ->
    // payload -> record (runlist + initialized_size) -> clear dirty.  A crash
    // before the record commit leaves the old view (holes read as zeros).
    if (!setDirty(v, true)) return .io;
    const fill = fillHolesInRuns(v, attr, offset, data.len);
    if (fill != .ok) return abortWrite(v, fill);
    if (!budgetedFlush(v)) return .io;
    if (offset > attr.initialized_size) {
        if (!zeroMappedRange(v, attr.runs[0..attr.count], attr.initialized_size, offset)) return .io;
    }
    if (!writeRunBytes(v, attr.runs[0..attr.count], offset, data)) return .io;
    if (!budgetedFlush(v)) return .io;
    const new_init = if (write_end > attr.initialized_size) write_end else attr.initialized_size;
    const commit = commitDataRunlist(v, record_number, attr.runs[0..attr.count], attr.data_size, new_init, attr.alloc_size);
    if (commit != .ok) return abortWriteAfterCommitFailure(v, commit);
    if (!budgetedFlush(v)) return .io;
    if (!setDirty(v, false)) return .io;
    return .ok;
}

/// Counts free clusters by scanning $Bitmap (used for free-space display).
pub fn freeClusterCount(v: *const Volume) ?u64 {
    const bitmap_attr = &v.scratch.attr_cluster;
    if (!collectAttribute(v, ntfs.MFT_RECORD_BITMAP, .data, &[_]u8{}, bitmap_attr)) return null;
    if (bitmap_attr.resident) return null;
    const total = v.totalClusters();
    var free: u64 = 0;
    var sector_buf: [SECTOR_SIZE]u8 = undefined;
    var cluster: u64 = 0;
    while (cluster < total) {
        const sector_index = (cluster / 8) / SECTOR_SIZE;
        const bitmap_offset = sectorByteOffset(sector_index) orelse return null;
        if (!readRunBytes(v, bitmap_attr.runs[0..bitmap_attr.count], bitmap_offset, sector_buf[0..])) return null;
        while (cluster < total and (cluster / 8) / SECTOR_SIZE == sector_index) : (cluster += 1) {
            const bit = (sector_buf[@intCast((cluster / 8) % SECTOR_SIZE)] >> @intCast(cluster % 8)) & 1;
            if (bit == 0) free += 1;
        }
    }
    return free;
}

/// Test seams for host models that need raw record access (synthetic
/// reparse/EFS flag patches).  Not used by the kernel adapter.
pub fn loadRecordForTest(v: *const Volume, number: u64) ?ntfs.FileRecordHeader {
    return loadRecord(v, number, v.scratch.record[0..]);
}

/// Test seam: collects the unnamed $DATA of `record_number` into the shared
/// scratch (runs, sizes, flags) and returns it.  Valid until the next
/// volume operation reuses the scratch.
pub fn collectAttributeForTest(v: *const Volume, record_number: u64) ?*AttrScratch {
    const attr = &v.scratch.attr_op;
    if (!collectAttribute(v, record_number, .data, &[_]u8{}, attr)) return null;
    return attr;
}

pub fn storeRecordForTest(v: *const Volume, number: u64) bool {
    return storeRecord(v, number, v.scratch.record[0..v.record_bytes]);
}

fn abortWrite(v: *const Volume, status: WriteStatus) WriteStatus {
    if (status == .cleanup_failed) return status;
    if (!setDirty(v, false)) return .cleanup_failed;
    return status;
}

fn abortWriteAfterCommitFailure(v: *const Volume, status: WriteStatus) WriteStatus {
    // A store returning I/O may have reached media before its completion was
    // lost.  Keep the dirty bracket in that ambiguous state; clearing it
    // would falsely certify a runlist whose visibility is unknown.
    if (status == .io or status == .cleanup_failed) return status;
    return abortWrite(v, status);
}

fn abortWriteFreeing(v: *const Volume, runs: []const ntfs.Run, status: WriteStatus) WriteStatus {
    if (status == .cleanup_failed) return status;
    // If cluster release fails, the dirty bit is deliberately retained.  A
    // successful dirty clear must never hide incomplete allocation cleanup.
    if (runs.len != 0 and !freeClusters(v, runs)) return .cleanup_failed;
    return abortWrite(v, status);
}

fn abortWriteReleasingRecord(
    v: *const Volume,
    record_number: u64,
    record_stored: bool,
    runs: []const ntfs.Run,
    status: WriteStatus,
) WriteStatus {
    if (status == .cleanup_failed) return status;
    // A stored record is made non-live before its MFT bitmap bit and owned
    // clusters are released.  Stopping on the first failure avoids freeing
    // storage that an on-disk record may still reference.
    if (record_stored and !markRecordFree(v, record_number)) return .cleanup_failed;
    if (!releaseRecord(v, record_number)) return .cleanup_failed;
    return abortWriteFreeing(v, runs, status);
}

fn rollbackUnpublishedRecord(v: *const Volume, record_number: u64, record_stored: bool, status: WriteStatus) WriteStatus {
    if (record_stored and !markRecordFree(v, record_number)) return .cleanup_failed;
    if (!releaseRecord(v, record_number)) return .cleanup_failed;
    return status;
}

/// Host-model seams for allocation and cleanup fault injection.  Production
/// filesystem adapters do not use these entry points.
pub fn allocateClustersForTest(v: *const Volume, count: u64, runs_out: []ntfs.Run) AllocationResult {
    return allocateClusters(v, count, runs_out);
}

pub fn freeClustersForTest(v: *const Volume, runs: []const ntfs.Run) bool {
    return freeClusters(v, runs);
}

pub fn abortWriteForTest(v: *const Volume, status: WriteStatus) WriteStatus {
    return abortWrite(v, status);
}

pub fn abortWriteFreeingForTest(v: *const Volume, runs: []const ntfs.Run, status: WriteStatus) WriteStatus {
    return abortWriteFreeing(v, runs, status);
}

test "metadata cache generation invalidates every decoded cache kind" {
    const testing = @import("std").testing;
    var cache = MetadataCache{};
    cache.beginMount(100);

    const record = [_]u8{ 1, 2, 3, 4 };
    cache.storeRecord(17, record[0..]);
    var record_out: [4]u8 = undefined;
    try testing.expect(cache.lookupRecord(17, record_out[0..]));
    try testing.expectEqualSlices(u8, record[0..], record_out[0..]);

    var attribute = AttrScratch{};
    attribute.runs[0] = .{ .length_clusters = 3, .lcn = 91 };
    attribute.count = 1;
    attribute.data_size = 12288;
    cache.storeAttribute(17, .data, &[_]u8{}, &attribute);
    var attribute_out = AttrScratch{};
    try testing.expect(cache.lookupAttribute(17, .data, &[_]u8{}, &attribute_out));
    try testing.expectEqual(@as(usize, 1), attribute_out.count);
    try testing.expectEqual(@as(u64, 91), attribute_out.runs[0].lcn.?);

    const block = [_]u8{0x49} ** 32;
    cache.storeIndex(5, 2, block[0..]);
    var block_out: [32]u8 = undefined;
    try testing.expect(cache.lookupIndex(5, 2, block_out[0..]));

    var name: [32]u8 = undefined;
    const name_len = ntfs.asciiToUtf16("CACHE.TXT", name[0..]).?;
    const found = LookupResult{
        .record = 17,
        .sequence = 4,
        .entry = .{ .record = 17, .sequence = 4 },
    };
    cache.storePath(5, name[0..name_len], .found, &found, 10);
    var found_out: LookupResult = undefined;
    try testing.expectEqual(PathCacheLookup.positive, cache.lookupPath(&[_]u8{}, 5, name[0..name_len], 10, &found_out));
    try testing.expectEqual(@as(u64, 17), found_out.record);

    const before = cache.summary();
    try testing.expectEqual(@as(u32, 1), before.record_entries);
    try testing.expectEqual(@as(u32, 1), before.attribute_entries);
    try testing.expectEqual(@as(u32, 1), before.index_entries);
    try testing.expectEqual(@as(u32, 1), before.path_entries);

    cache.invalidateMutation();
    const after = cache.summary();
    try testing.expectEqual(@as(u32, 0), after.record_entries + after.attribute_entries + after.index_entries + after.path_entries);
    try testing.expectEqual(@as(u64, 1), after.mutation_invalidations);
    try testing.expectEqual(@as(u64, 4), after.invalidated_entries);
    try testing.expect(!cache.lookupRecord(17, record_out[0..]));
}

test "negative path cache expires and external generation rejects stale results" {
    const testing = @import("std").testing;
    var cache = MetadataCache{};
    cache.beginMount(10);
    var name: [32]u8 = undefined;
    const name_len = ntfs.asciiToUtf16("MISSING.TXT", name[0..]).?;
    var unused = LookupResult{ .record = 0, .sequence = 0, .entry = .{} };
    cache.storePath(5, name[0..name_len], .not_found, &unused, 100);
    try testing.expectEqual(PathCacheLookup.negative, cache.lookupPath(&[_]u8{}, 5, name[0..name_len], 109, &unused));
    try testing.expectEqual(PathCacheLookup.miss, cache.lookupPath(&[_]u8{}, 5, name[0..name_len], 110, &unused));
    try testing.expectEqual(@as(u64, 1), cache.summary().path_expirations);

    cache.storePath(5, name[0..name_len], .not_found, &unused, 200);
    cache.invalidateExternal();
    try testing.expectEqual(PathCacheLookup.miss, cache.lookupPath(&[_]u8{}, 5, name[0..name_len], 201, &unused));
    try testing.expectEqual(@as(u64, 1), cache.summary().external_invalidations);
}

test "metadata cache capacity and reclaim work stay bounded" {
    const testing = @import("std").testing;
    var cache = MetadataCache{};
    cache.beginMount(100);
    var payload: [8]u8 = undefined;
    var number: u64 = 0;
    while (number < METADATA_RECORD_CAPACITY + 1) : (number += 1) {
        @memset(payload[0..], @truncate(number));
        cache.storeRecord(number, payload[0..]);
    }
    var summary = cache.summary();
    try testing.expectEqual(@as(u32, METADATA_RECORD_CAPACITY), summary.record_entries);
    try testing.expectEqual(@as(u64, 1), summary.record_evictions);

    const reclaimed = cache.reclaim(3);
    try testing.expectEqual(@as(u32, 3), reclaimed.reclaimed_entries);
    try testing.expect(reclaimed.inspected_entries <= METADATA_CACHE_SLOT_CAPACITY);
    summary = cache.summary();
    try testing.expectEqual(@as(u32, METADATA_RECORD_CAPACITY - 3), summary.record_entries);
    try testing.expectEqual(@as(u64, 3), summary.reclaimed_entries);
}
