// Kernel-private NTFS native value boundary. Algorithms and state live in Rust.

const zstd = @import("std");

const format_provider = @import("ntfs_format");

const ntfs = if (@hasDecl(format_provider, "shared_format")) format_provider.shared_format else format_provider;

const work_counters_enabled = @import("builtin").is_test or (@hasDecl(@import("root"), "ntfs_work_counters") and @import("root").ntfs_work_counters);

pub const SECTOR_SIZE: usize = 512;

pub const NAME_UNITS_MAX: usize = 255;

pub const NAME_MAX: usize = 768;

pub const MAX_MFT_RUNS: usize = 64;

pub const MAX_ATTR_RUNS: usize = 256;

pub const METADATA_CACHE_VERSION: u32 = 2;

pub const METADATA_RECORD_CAPACITY: usize = 8;

pub const METADATA_ATTRIBUTE_CAPACITY: usize = 4;

pub const METADATA_INDEX_CAPACITY: usize = 2;

pub const METADATA_PATH_CAPACITY: usize = 8;

pub const METADATA_CACHE_SLOT_CAPACITY: usize = METADATA_RECORD_CAPACITY +
    METADATA_ATTRIBUTE_CAPACITY + METADATA_INDEX_CAPACITY + METADATA_PATH_CAPACITY;

pub const ATTR_READ_ONLY: u8 = 0x01;

pub const ATTR_HIDDEN: u8 = 0x02;

pub const ATTR_SYSTEM: u8 = 0x04;

pub const ATTR_DIRECTORY: u8 = 0x10;

pub const ATTR_ARCHIVE: u8 = 0x20;

pub const MAX_TRANSFER_SECTORS: u32 = 256;

pub const WorkStats = struct {
    cluster_bitmap_probes: u64 = 0,
    record_bitmap_probes: u64 = 0,
    read_zero_bytes: u64 = 0,
    directory_entries: u64 = 0,
};

pub const Device = struct {
    ctx: *anyopaque,
    read_sectors: *const fn (ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool,
    write_sectors: *const fn (ctx: *anyopaque, lba: u64, count: u32, data: []const u8) bool,
    flush: *const fn (ctx: *anyopaque) bool,
    /// Compatibility default for older seams. Zero accepts the owner's full
    /// bounded chunk; smaller transports may publish their sector limit.
    max_transfer_sectors: u32 = 64,
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

pub const WriteProgress = struct {
    status: WriteStatus,
    completed: usize = 0,
    uncertain: bool = false,
};

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
    used_mft_mirror: bool = false,
};

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
    payload_write_retentions: u64 = 0,
    system_write_retentions: u64 = 0,
    targeted_invalidations: u64 = 0,
    targeted_record_invalidations: u64 = 0,
    targeted_attribute_invalidations: u64 = 0,
    targeted_directory_invalidations: u64 = 0,
    global_mutation_invalidations: u64 = 0,
    recovery_invalidations: u64 = 0,
    mutation_invalidated_record_entries: u64 = 0,
    mutation_invalidated_attribute_entries: u64 = 0,
    mutation_invalidated_index_entries: u64 = 0,
    mutation_invalidated_path_entries: u64 = 0,
    reclaim_requests: u64 = 0,
    reclaim_scans: u64 = 0,
    reclaimed_entries: u64 = 0,
};

pub const MetadataReclaimResult = struct {
    requested_entries: u32 = 0,
    inspected_entries: u32 = 0,
    reclaimed_entries: u32 = 0,
};

pub const RecordName = struct {
    parent: u64,
    parent_sequence: u16,
    name: [NAME_MAX]u8 = .{0} ** NAME_MAX,
    name_len: usize = 0,
};

pub const ResolvedEntry = struct {
    entry: Entry,
    parent_record: u64,
};

pub const DirectoryCursor = extern struct {
    pub const Frame = extern struct { vcn: u64 = 0, offset: u32 = 0, child_done: u32 = 0 };
    state: u32 = 0,
    depth: u32 = 0,
    frames: [MAX_INDEX_DEPTH]Frame = .{Frame{}} ** MAX_INDEX_DEPTH,
};

pub const DirectoryNextStatus = enum { found, not_found, again, io };

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

pub const AllocationResult = struct {
    status: WriteStatus,
    produced: usize = 0,
};

pub const RecordLinkInfo = struct {
    sequence: u16,
    link_count: u16,
};

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

const MetadataMutation = union(enum) {
    /// File bytes changed without changing any decoded metadata.
    payload,
    /// One MFT record and every decoded view derived from it changed.
    record: u64,
    /// One non-resident attribute payload changed; its decoded descriptor is
    /// discarded without touching unrelated records or paths.
    attribute: struct {
        record_number: u64,
        attr_type: ntfs.AttrType,
    },
    /// One non-resident directory index changed; the directory record did not.
    directory: u64,
    /// Cache-independent, fully classified system metadata such as $MFTMirr.
    system,
    /// A mutation whose cache dependencies cannot be proven.
    unknown,
    /// Recovery and ambiguous rollback paths deliberately remain global.
    recovery,
};

const PathCacheLookup = enum {
    miss,
    positive,
    negative,
};

const CView = extern struct {
    data: ?[*]const u8,
    len: usize,
};

const CBuffer = extern struct {
    data: ?[*]u8,
    len: usize,
};

const CRun = extern struct {
    length_clusters: u64,
    has_lcn: bool,
    lcn: u64,
};

const CFileReference = extern struct {
    record: u64,
    sequence: u16,
};

const CFileRecordHeader = extern struct {
    magic: u32,
    lsn: u64,
    sequence: u16,
    link_count: u16,
    attrs_offset: u16,
    flags: u16,
    bytes_in_use: u32,
    bytes_allocated: u32,
    base_record: CFileReference,
    next_attr_instance: u16,
    record_number: u32,
};

const CWorkStats = extern struct {
    cluster_bitmap_probes: u64,
    record_bitmap_probes: u64,
    read_zero_bytes: u64,
    directory_entries: u64,
};

const CEntry = extern struct {
    name: [768]u8,
    name_len: usize,
    attr: u8,
    reparse: bool,
    record: u64,
    sequence: u16,
    size: u64,
    created_time_nt: u64,
    modified_time_nt: u64,
    access_time_nt: u64,
};

const CLookupResult = extern struct {
    record: u64,
    sequence: u16,
    entry: CEntry,
    in_root: bool,
    block_vcn: u64,
    entry_offset: usize,
};

const CAttrScratch = extern struct {
    runs: [256]CRun,
    count: usize,
    data_size: u64,
    initialized_size: u64,
    alloc_size: u64,
    flags: u16,
    compression_unit: u8,
    resident: bool,
    resident_len: usize,
    resident_copy: [1024]u8,
};

const CGeometry = extern struct {
    partition_lba: u32,
    cluster_bytes: u32,
    total_sectors: u64,
};

const CDirectoryFrame = extern struct {
    vcn: u64,
    offset: u32,
    child_done: u32,
};

const CDirectoryCursor = extern struct {
    state: u32,
    depth: u32,
    frames: [8]CDirectoryFrame,
};

const CRecordName = extern struct {
    parent: u64,
    parent_sequence: u16,
    name: [768]u8,
    name_len: usize,
};

const CResolvedEntry = extern struct {
    entry: CEntry,
    parent_record: u64,
};

const CWriteProgress = extern struct {
    status: u8,
    completed: usize,
    uncertain: bool,
};

const CMountInfo = extern struct {
    cluster_bytes: u32,
    record_bytes: u32,
    index_block_bytes: u32,
    total_sectors: u64,
    mft_lcn: u64,
    mftmirr_lcn: u64,
    mft_run_count: usize,
    used_mft_mirror: bool,
};

const CMetadataCacheSummary = extern struct {
    version: u32,
    active_volumes: u32,
    slot_capacity: u32,
    bytes_per_volume: u32,
    record_capacity: u32,
    attribute_capacity: u32,
    index_capacity: u32,
    path_capacity: u32,
    mount_generation: u64,
    content_generation: u64,
    negative_ttl_ticks: u64,
    record_entries: u32,
    attribute_entries: u32,
    index_entries: u32,
    path_entries: u32,
    record_hits: u64,
    record_misses: u64,
    record_stores: u64,
    record_evictions: u64,
    attribute_hits: u64,
    attribute_misses: u64,
    attribute_stores: u64,
    attribute_evictions: u64,
    index_hits: u64,
    index_misses: u64,
    index_stores: u64,
    index_evictions: u64,
    path_queries: u64,
    path_positive_hits: u64,
    path_negative_hits: u64,
    path_misses: u64,
    path_positive_stores: u64,
    path_negative_stores: u64,
    path_expirations: u64,
    lookup_tree_walks: u64,
    recovery_cache_bypasses: u64,
    mount_invalidations: u64,
    mutation_invalidations: u64,
    external_invalidations: u64,
    invalidated_entries: u64,
    payload_write_retentions: u64,
    system_write_retentions: u64,
    targeted_invalidations: u64,
    targeted_record_invalidations: u64,
    targeted_attribute_invalidations: u64,
    targeted_directory_invalidations: u64,
    global_mutation_invalidations: u64,
    recovery_invalidations: u64,
    mutation_invalidated_record_entries: u64,
    mutation_invalidated_attribute_entries: u64,
    mutation_invalidated_index_entries: u64,
    mutation_invalidated_path_entries: u64,
    reclaim_requests: u64,
    reclaim_scans: u64,
    reclaimed_entries: u64,
};

const CMetadataReclaimResult = extern struct {
    requested_entries: u32,
    inspected_entries: u32,
    reclaimed_entries: u32,
};

const CDevice = extern struct {
    context: ?*anyopaque,
    read: *const fn (*anyopaque, u64, u32, [*]u8, usize) callconv(.c) bool,
    write: *const fn (*anyopaque, u64, u32, [*]const u8, usize) callconv(.c) bool,
    flush: *const fn (*anyopaque) callconv(.c) bool,
    max_transfer_sectors: u32,
};

const CIo = extern struct {
    device: CDevice,
    geometry: CGeometry,
    scratch: ?*anyopaque,
    cache: ?*anyopaque,
    work_counters: bool,
};

const CVolume = extern struct {
    io: CIo,
    record_bytes: u32,
    index_block_bytes: u32,
    mft_runs: [*]CRun,
    mft_capacity: usize,
    mft_count: ?*usize,
    upcase: CView,
    now_filetime: u64,
    security_id_file: u32,
    security_id_dir: u32,
    cache_now_ticks: u64,
};

const CArgs = extern struct {
    volume: ?*const CVolume,
    state: ?*anyopaque,
    out: ?*anyopaque,
    buffer: CBuffer,
    name: CView,
    other: CView,
    third: CView,
    number: u64,
    second: u64,
    third_number: u64,
    kind: u32,
    flag: bool,
};

const CEnumSink = extern struct {
    out: CBuffer,
    has_out: bool,
    cursor: usize,
    listed: usize,
    max_entries: usize,
    wanted: usize,
    seen: usize,
    found: CEntry,
    has_found: bool,
    failed: bool,
    count_ref: u64,
    has_count_ref: bool,
    matches: usize,
};

extern fn r4_ntfs_volume(op: u32, args: *const CArgs) callconv(.c) u64;
extern fn r4_ntfs_metadata(op: u32, args: *const CArgs) callconv(.c) u64;

comptime {
    if (@sizeOf(CView) != 16 or @alignOf(CView) != 8) @compileError("NTFS volume View layout");

    if (@offsetOf(CView, "data") != 0) @compileError("NTFS volume View.data layout");

    if (@offsetOf(CView, "len") != 8) @compileError("NTFS volume View.len layout");

    if (@sizeOf(CBuffer) != 16 or @alignOf(CBuffer) != 8) @compileError("NTFS volume Buffer layout");

    if (@offsetOf(CBuffer, "data") != 0) @compileError("NTFS volume Buffer.data layout");

    if (@offsetOf(CBuffer, "len") != 8) @compileError("NTFS volume Buffer.len layout");

    if (@sizeOf(CRun) != 24 or @alignOf(CRun) != 8) @compileError("NTFS volume Run layout");

    if (@offsetOf(CRun, "length_clusters") != 0) @compileError("NTFS volume Run.length_clusters layout");

    if (@offsetOf(CRun, "has_lcn") != 8) @compileError("NTFS volume Run.has_lcn layout");

    if (@offsetOf(CRun, "lcn") != 16) @compileError("NTFS volume Run.lcn layout");

    if (@sizeOf(CFileReference) != 16 or @alignOf(CFileReference) != 8) @compileError("NTFS volume FileReference layout");

    if (@offsetOf(CFileReference, "record") != 0) @compileError("NTFS volume FileReference.record layout");

    if (@offsetOf(CFileReference, "sequence") != 8) @compileError("NTFS volume FileReference.sequence layout");

    if (@sizeOf(CFileRecordHeader) != 56 or @alignOf(CFileRecordHeader) != 8) @compileError("NTFS volume FileRecordHeader layout");

    if (@offsetOf(CFileRecordHeader, "magic") != 0) @compileError("NTFS volume FileRecordHeader.magic layout");

    if (@offsetOf(CFileRecordHeader, "lsn") != 8) @compileError("NTFS volume FileRecordHeader.lsn layout");

    if (@offsetOf(CFileRecordHeader, "sequence") != 16) @compileError("NTFS volume FileRecordHeader.sequence layout");

    if (@offsetOf(CFileRecordHeader, "link_count") != 18) @compileError("NTFS volume FileRecordHeader.link_count layout");

    if (@offsetOf(CFileRecordHeader, "attrs_offset") != 20) @compileError("NTFS volume FileRecordHeader.attrs_offset layout");

    if (@offsetOf(CFileRecordHeader, "flags") != 22) @compileError("NTFS volume FileRecordHeader.flags layout");

    if (@offsetOf(CFileRecordHeader, "bytes_in_use") != 24) @compileError("NTFS volume FileRecordHeader.bytes_in_use layout");

    if (@offsetOf(CFileRecordHeader, "bytes_allocated") != 28) @compileError("NTFS volume FileRecordHeader.bytes_allocated layout");

    if (@offsetOf(CFileRecordHeader, "base_record") != 32) @compileError("NTFS volume FileRecordHeader.base_record layout");

    if (@offsetOf(CFileRecordHeader, "next_attr_instance") != 48) @compileError("NTFS volume FileRecordHeader.next_attr_instance layout");

    if (@offsetOf(CFileRecordHeader, "record_number") != 52) @compileError("NTFS volume FileRecordHeader.record_number layout");

    if (@sizeOf(CWorkStats) != 32 or @alignOf(CWorkStats) != 8) @compileError("NTFS volume WorkStats layout");

    if (@offsetOf(CWorkStats, "cluster_bitmap_probes") != 0) @compileError("NTFS volume WorkStats.cluster_bitmap_probes layout");

    if (@offsetOf(CWorkStats, "record_bitmap_probes") != 8) @compileError("NTFS volume WorkStats.record_bitmap_probes layout");

    if (@offsetOf(CWorkStats, "read_zero_bytes") != 16) @compileError("NTFS volume WorkStats.read_zero_bytes layout");

    if (@offsetOf(CWorkStats, "directory_entries") != 24) @compileError("NTFS volume WorkStats.directory_entries layout");

    if (@sizeOf(CEntry) != 832 or @alignOf(CEntry) != 8) @compileError("NTFS volume Entry layout");

    if (@offsetOf(CEntry, "name") != 0) @compileError("NTFS volume Entry.name layout");

    if (@offsetOf(CEntry, "name_len") != 768) @compileError("NTFS volume Entry.name_len layout");

    if (@offsetOf(CEntry, "attr") != 776) @compileError("NTFS volume Entry.attr layout");

    if (@offsetOf(CEntry, "reparse") != 777) @compileError("NTFS volume Entry.reparse layout");

    if (@offsetOf(CEntry, "record") != 784) @compileError("NTFS volume Entry.record layout");

    if (@offsetOf(CEntry, "sequence") != 792) @compileError("NTFS volume Entry.sequence layout");

    if (@offsetOf(CEntry, "size") != 800) @compileError("NTFS volume Entry.size layout");

    if (@offsetOf(CEntry, "created_time_nt") != 808) @compileError("NTFS volume Entry.created_time_nt layout");

    if (@offsetOf(CEntry, "modified_time_nt") != 816) @compileError("NTFS volume Entry.modified_time_nt layout");

    if (@offsetOf(CEntry, "access_time_nt") != 824) @compileError("NTFS volume Entry.access_time_nt layout");

    if (@sizeOf(CLookupResult) != 872 or @alignOf(CLookupResult) != 8) @compileError("NTFS volume LookupResult layout");

    if (@offsetOf(CLookupResult, "record") != 0) @compileError("NTFS volume LookupResult.record layout");

    if (@offsetOf(CLookupResult, "sequence") != 8) @compileError("NTFS volume LookupResult.sequence layout");

    if (@offsetOf(CLookupResult, "entry") != 16) @compileError("NTFS volume LookupResult.entry layout");

    if (@offsetOf(CLookupResult, "in_root") != 848) @compileError("NTFS volume LookupResult.in_root layout");

    if (@offsetOf(CLookupResult, "block_vcn") != 856) @compileError("NTFS volume LookupResult.block_vcn layout");

    if (@offsetOf(CLookupResult, "entry_offset") != 864) @compileError("NTFS volume LookupResult.entry_offset layout");

    if (@sizeOf(CAttrScratch) != 7216 or @alignOf(CAttrScratch) != 8) @compileError("NTFS volume AttrScratch layout");

    if (@offsetOf(CAttrScratch, "runs") != 0) @compileError("NTFS volume AttrScratch.runs layout");

    if (@offsetOf(CAttrScratch, "count") != 6144) @compileError("NTFS volume AttrScratch.count layout");

    if (@offsetOf(CAttrScratch, "data_size") != 6152) @compileError("NTFS volume AttrScratch.data_size layout");

    if (@offsetOf(CAttrScratch, "initialized_size") != 6160) @compileError("NTFS volume AttrScratch.initialized_size layout");

    if (@offsetOf(CAttrScratch, "alloc_size") != 6168) @compileError("NTFS volume AttrScratch.alloc_size layout");

    if (@offsetOf(CAttrScratch, "flags") != 6176) @compileError("NTFS volume AttrScratch.flags layout");

    if (@offsetOf(CAttrScratch, "compression_unit") != 6178) @compileError("NTFS volume AttrScratch.compression_unit layout");

    if (@offsetOf(CAttrScratch, "resident") != 6179) @compileError("NTFS volume AttrScratch.resident layout");

    if (@offsetOf(CAttrScratch, "resident_len") != 6184) @compileError("NTFS volume AttrScratch.resident_len layout");

    if (@offsetOf(CAttrScratch, "resident_copy") != 6192) @compileError("NTFS volume AttrScratch.resident_copy layout");

    if (@sizeOf(CGeometry) != 16 or @alignOf(CGeometry) != 8) @compileError("NTFS volume Geometry layout");

    if (@offsetOf(CGeometry, "partition_lba") != 0) @compileError("NTFS volume Geometry.partition_lba layout");

    if (@offsetOf(CGeometry, "cluster_bytes") != 4) @compileError("NTFS volume Geometry.cluster_bytes layout");

    if (@offsetOf(CGeometry, "total_sectors") != 8) @compileError("NTFS volume Geometry.total_sectors layout");

    if (@sizeOf(CDirectoryFrame) != 16 or @alignOf(CDirectoryFrame) != 8) @compileError("NTFS volume DirectoryFrame layout");

    if (@offsetOf(CDirectoryFrame, "vcn") != 0) @compileError("NTFS volume DirectoryFrame.vcn layout");

    if (@offsetOf(CDirectoryFrame, "offset") != 8) @compileError("NTFS volume DirectoryFrame.offset layout");

    if (@offsetOf(CDirectoryFrame, "child_done") != 12) @compileError("NTFS volume DirectoryFrame.child_done layout");

    if (@sizeOf(CDirectoryCursor) != 136 or @alignOf(CDirectoryCursor) != 8) @compileError("NTFS volume DirectoryCursor layout");

    if (@offsetOf(CDirectoryCursor, "state") != 0) @compileError("NTFS volume DirectoryCursor.state layout");

    if (@offsetOf(CDirectoryCursor, "depth") != 4) @compileError("NTFS volume DirectoryCursor.depth layout");

    if (@offsetOf(CDirectoryCursor, "frames") != 8) @compileError("NTFS volume DirectoryCursor.frames layout");

    if (@sizeOf(CRecordName) != 792 or @alignOf(CRecordName) != 8) @compileError("NTFS volume RecordName layout");

    if (@offsetOf(CRecordName, "parent") != 0) @compileError("NTFS volume RecordName.parent layout");

    if (@offsetOf(CRecordName, "parent_sequence") != 8) @compileError("NTFS volume RecordName.parent_sequence layout");

    if (@offsetOf(CRecordName, "name") != 10) @compileError("NTFS volume RecordName.name layout");

    if (@offsetOf(CRecordName, "name_len") != 784) @compileError("NTFS volume RecordName.name_len layout");

    if (@sizeOf(CResolvedEntry) != 840 or @alignOf(CResolvedEntry) != 8) @compileError("NTFS volume ResolvedEntry layout");

    if (@offsetOf(CResolvedEntry, "entry") != 0) @compileError("NTFS volume ResolvedEntry.entry layout");

    if (@offsetOf(CResolvedEntry, "parent_record") != 832) @compileError("NTFS volume ResolvedEntry.parent_record layout");

    if (@sizeOf(CWriteProgress) != 24 or @alignOf(CWriteProgress) != 8) @compileError("NTFS volume WriteProgress layout");

    if (@offsetOf(CWriteProgress, "status") != 0) @compileError("NTFS volume WriteProgress.status layout");

    if (@offsetOf(CWriteProgress, "completed") != 8) @compileError("NTFS volume WriteProgress.completed layout");

    if (@offsetOf(CWriteProgress, "uncertain") != 16) @compileError("NTFS volume WriteProgress.uncertain layout");

    if (@sizeOf(CMountInfo) != 56 or @alignOf(CMountInfo) != 8) @compileError("NTFS volume MountInfo layout");

    if (@offsetOf(CMountInfo, "cluster_bytes") != 0) @compileError("NTFS volume MountInfo.cluster_bytes layout");

    if (@offsetOf(CMountInfo, "record_bytes") != 4) @compileError("NTFS volume MountInfo.record_bytes layout");

    if (@offsetOf(CMountInfo, "index_block_bytes") != 8) @compileError("NTFS volume MountInfo.index_block_bytes layout");

    if (@offsetOf(CMountInfo, "total_sectors") != 16) @compileError("NTFS volume MountInfo.total_sectors layout");

    if (@offsetOf(CMountInfo, "mft_lcn") != 24) @compileError("NTFS volume MountInfo.mft_lcn layout");

    if (@offsetOf(CMountInfo, "mftmirr_lcn") != 32) @compileError("NTFS volume MountInfo.mftmirr_lcn layout");

    if (@offsetOf(CMountInfo, "mft_run_count") != 40) @compileError("NTFS volume MountInfo.mft_run_count layout");

    if (@offsetOf(CMountInfo, "used_mft_mirror") != 48) @compileError("NTFS volume MountInfo.used_mft_mirror layout");

    if (@sizeOf(CMetadataCacheSummary) != 392 or @alignOf(CMetadataCacheSummary) != 8) @compileError("NTFS volume MetadataCacheSummary layout");

    if (@offsetOf(CMetadataCacheSummary, "version") != 0) @compileError("NTFS volume MetadataCacheSummary.version layout");

    if (@offsetOf(CMetadataCacheSummary, "active_volumes") != 4) @compileError("NTFS volume MetadataCacheSummary.active_volumes layout");

    if (@offsetOf(CMetadataCacheSummary, "slot_capacity") != 8) @compileError("NTFS volume MetadataCacheSummary.slot_capacity layout");

    if (@offsetOf(CMetadataCacheSummary, "bytes_per_volume") != 12) @compileError("NTFS volume MetadataCacheSummary.bytes_per_volume layout");

    if (@offsetOf(CMetadataCacheSummary, "record_capacity") != 16) @compileError("NTFS volume MetadataCacheSummary.record_capacity layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_capacity") != 20) @compileError("NTFS volume MetadataCacheSummary.attribute_capacity layout");

    if (@offsetOf(CMetadataCacheSummary, "index_capacity") != 24) @compileError("NTFS volume MetadataCacheSummary.index_capacity layout");

    if (@offsetOf(CMetadataCacheSummary, "path_capacity") != 28) @compileError("NTFS volume MetadataCacheSummary.path_capacity layout");

    if (@offsetOf(CMetadataCacheSummary, "mount_generation") != 32) @compileError("NTFS volume MetadataCacheSummary.mount_generation layout");

    if (@offsetOf(CMetadataCacheSummary, "content_generation") != 40) @compileError("NTFS volume MetadataCacheSummary.content_generation layout");

    if (@offsetOf(CMetadataCacheSummary, "negative_ttl_ticks") != 48) @compileError("NTFS volume MetadataCacheSummary.negative_ttl_ticks layout");

    if (@offsetOf(CMetadataCacheSummary, "record_entries") != 56) @compileError("NTFS volume MetadataCacheSummary.record_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_entries") != 60) @compileError("NTFS volume MetadataCacheSummary.attribute_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "index_entries") != 64) @compileError("NTFS volume MetadataCacheSummary.index_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "path_entries") != 68) @compileError("NTFS volume MetadataCacheSummary.path_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "record_hits") != 72) @compileError("NTFS volume MetadataCacheSummary.record_hits layout");

    if (@offsetOf(CMetadataCacheSummary, "record_misses") != 80) @compileError("NTFS volume MetadataCacheSummary.record_misses layout");

    if (@offsetOf(CMetadataCacheSummary, "record_stores") != 88) @compileError("NTFS volume MetadataCacheSummary.record_stores layout");

    if (@offsetOf(CMetadataCacheSummary, "record_evictions") != 96) @compileError("NTFS volume MetadataCacheSummary.record_evictions layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_hits") != 104) @compileError("NTFS volume MetadataCacheSummary.attribute_hits layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_misses") != 112) @compileError("NTFS volume MetadataCacheSummary.attribute_misses layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_stores") != 120) @compileError("NTFS volume MetadataCacheSummary.attribute_stores layout");

    if (@offsetOf(CMetadataCacheSummary, "attribute_evictions") != 128) @compileError("NTFS volume MetadataCacheSummary.attribute_evictions layout");

    if (@offsetOf(CMetadataCacheSummary, "index_hits") != 136) @compileError("NTFS volume MetadataCacheSummary.index_hits layout");

    if (@offsetOf(CMetadataCacheSummary, "index_misses") != 144) @compileError("NTFS volume MetadataCacheSummary.index_misses layout");

    if (@offsetOf(CMetadataCacheSummary, "index_stores") != 152) @compileError("NTFS volume MetadataCacheSummary.index_stores layout");

    if (@offsetOf(CMetadataCacheSummary, "index_evictions") != 160) @compileError("NTFS volume MetadataCacheSummary.index_evictions layout");

    if (@offsetOf(CMetadataCacheSummary, "path_queries") != 168) @compileError("NTFS volume MetadataCacheSummary.path_queries layout");

    if (@offsetOf(CMetadataCacheSummary, "path_positive_hits") != 176) @compileError("NTFS volume MetadataCacheSummary.path_positive_hits layout");

    if (@offsetOf(CMetadataCacheSummary, "path_negative_hits") != 184) @compileError("NTFS volume MetadataCacheSummary.path_negative_hits layout");

    if (@offsetOf(CMetadataCacheSummary, "path_misses") != 192) @compileError("NTFS volume MetadataCacheSummary.path_misses layout");

    if (@offsetOf(CMetadataCacheSummary, "path_positive_stores") != 200) @compileError("NTFS volume MetadataCacheSummary.path_positive_stores layout");

    if (@offsetOf(CMetadataCacheSummary, "path_negative_stores") != 208) @compileError("NTFS volume MetadataCacheSummary.path_negative_stores layout");

    if (@offsetOf(CMetadataCacheSummary, "path_expirations") != 216) @compileError("NTFS volume MetadataCacheSummary.path_expirations layout");

    if (@offsetOf(CMetadataCacheSummary, "lookup_tree_walks") != 224) @compileError("NTFS volume MetadataCacheSummary.lookup_tree_walks layout");

    if (@offsetOf(CMetadataCacheSummary, "recovery_cache_bypasses") != 232) @compileError("NTFS volume MetadataCacheSummary.recovery_cache_bypasses layout");

    if (@offsetOf(CMetadataCacheSummary, "mount_invalidations") != 240) @compileError("NTFS volume MetadataCacheSummary.mount_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "mutation_invalidations") != 248) @compileError("NTFS volume MetadataCacheSummary.mutation_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "external_invalidations") != 256) @compileError("NTFS volume MetadataCacheSummary.external_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "invalidated_entries") != 264) @compileError("NTFS volume MetadataCacheSummary.invalidated_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "payload_write_retentions") != 272) @compileError("NTFS volume MetadataCacheSummary.payload_write_retentions layout");

    if (@offsetOf(CMetadataCacheSummary, "system_write_retentions") != 280) @compileError("NTFS volume MetadataCacheSummary.system_write_retentions layout");

    if (@offsetOf(CMetadataCacheSummary, "targeted_invalidations") != 288) @compileError("NTFS volume MetadataCacheSummary.targeted_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "targeted_record_invalidations") != 296) @compileError("NTFS volume MetadataCacheSummary.targeted_record_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "targeted_attribute_invalidations") != 304) @compileError("NTFS volume MetadataCacheSummary.targeted_attribute_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "targeted_directory_invalidations") != 312) @compileError("NTFS volume MetadataCacheSummary.targeted_directory_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "global_mutation_invalidations") != 320) @compileError("NTFS volume MetadataCacheSummary.global_mutation_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "recovery_invalidations") != 328) @compileError("NTFS volume MetadataCacheSummary.recovery_invalidations layout");

    if (@offsetOf(CMetadataCacheSummary, "mutation_invalidated_record_entries") != 336) @compileError("NTFS volume MetadataCacheSummary.mutation_invalidated_record_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "mutation_invalidated_attribute_entries") != 344) @compileError("NTFS volume MetadataCacheSummary.mutation_invalidated_attribute_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "mutation_invalidated_index_entries") != 352) @compileError("NTFS volume MetadataCacheSummary.mutation_invalidated_index_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "mutation_invalidated_path_entries") != 360) @compileError("NTFS volume MetadataCacheSummary.mutation_invalidated_path_entries layout");

    if (@offsetOf(CMetadataCacheSummary, "reclaim_requests") != 368) @compileError("NTFS volume MetadataCacheSummary.reclaim_requests layout");

    if (@offsetOf(CMetadataCacheSummary, "reclaim_scans") != 376) @compileError("NTFS volume MetadataCacheSummary.reclaim_scans layout");

    if (@offsetOf(CMetadataCacheSummary, "reclaimed_entries") != 384) @compileError("NTFS volume MetadataCacheSummary.reclaimed_entries layout");

    if (@sizeOf(CMetadataReclaimResult) != 12 or @alignOf(CMetadataReclaimResult) != 4) @compileError("NTFS volume MetadataReclaimResult layout");

    if (@offsetOf(CMetadataReclaimResult, "requested_entries") != 0) @compileError("NTFS volume MetadataReclaimResult.requested_entries layout");

    if (@offsetOf(CMetadataReclaimResult, "inspected_entries") != 4) @compileError("NTFS volume MetadataReclaimResult.inspected_entries layout");

    if (@offsetOf(CMetadataReclaimResult, "reclaimed_entries") != 8) @compileError("NTFS volume MetadataReclaimResult.reclaimed_entries layout");

    if (@sizeOf(CDevice) != 40 or @alignOf(CDevice) != 8) @compileError("NTFS volume Device layout");

    if (@offsetOf(CDevice, "context") != 0) @compileError("NTFS volume Device.context layout");

    if (@offsetOf(CDevice, "read") != 8) @compileError("NTFS volume Device.read layout");

    if (@offsetOf(CDevice, "write") != 16) @compileError("NTFS volume Device.write layout");

    if (@offsetOf(CDevice, "flush") != 24) @compileError("NTFS volume Device.flush layout");

    if (@offsetOf(CDevice, "max_transfer_sectors") != 32) @compileError("NTFS volume Device.max_transfer_sectors layout");

    if (@sizeOf(CIo) != 80 or @alignOf(CIo) != 8) @compileError("NTFS volume Io layout");

    if (@offsetOf(CIo, "device") != 0) @compileError("NTFS volume Io.device layout");

    if (@offsetOf(CIo, "geometry") != 40) @compileError("NTFS volume Io.geometry layout");

    if (@offsetOf(CIo, "scratch") != 56) @compileError("NTFS volume Io.scratch layout");

    if (@offsetOf(CIo, "cache") != 64) @compileError("NTFS volume Io.cache layout");

    if (@offsetOf(CIo, "work_counters") != 72) @compileError("NTFS volume Io.work_counters layout");

    if (@sizeOf(CVolume) != 152 or @alignOf(CVolume) != 8) @compileError("NTFS volume Volume layout");

    if (@offsetOf(CVolume, "io") != 0) @compileError("NTFS volume Volume.io layout");

    if (@offsetOf(CVolume, "record_bytes") != 80) @compileError("NTFS volume Volume.record_bytes layout");

    if (@offsetOf(CVolume, "index_block_bytes") != 84) @compileError("NTFS volume Volume.index_block_bytes layout");

    if (@offsetOf(CVolume, "mft_runs") != 88) @compileError("NTFS volume Volume.mft_runs layout");

    if (@offsetOf(CVolume, "mft_capacity") != 96) @compileError("NTFS volume Volume.mft_capacity layout");

    if (@offsetOf(CVolume, "mft_count") != 104) @compileError("NTFS volume Volume.mft_count layout");

    if (@offsetOf(CVolume, "upcase") != 112) @compileError("NTFS volume Volume.upcase layout");

    if (@offsetOf(CVolume, "now_filetime") != 128) @compileError("NTFS volume Volume.now_filetime layout");

    if (@offsetOf(CVolume, "security_id_file") != 136) @compileError("NTFS volume Volume.security_id_file layout");

    if (@offsetOf(CVolume, "security_id_dir") != 140) @compileError("NTFS volume Volume.security_id_dir layout");

    if (@offsetOf(CVolume, "cache_now_ticks") != 144) @compileError("NTFS volume Volume.cache_now_ticks layout");

    if (@sizeOf(CArgs) != 120 or @alignOf(CArgs) != 8) @compileError("NTFS volume Args layout");

    if (@offsetOf(CArgs, "volume") != 0) @compileError("NTFS volume Args.volume layout");

    if (@offsetOf(CArgs, "state") != 8) @compileError("NTFS volume Args.state layout");

    if (@offsetOf(CArgs, "out") != 16) @compileError("NTFS volume Args.out layout");

    if (@offsetOf(CArgs, "buffer") != 24) @compileError("NTFS volume Args.buffer layout");

    if (@offsetOf(CArgs, "name") != 40) @compileError("NTFS volume Args.name layout");

    if (@offsetOf(CArgs, "other") != 56) @compileError("NTFS volume Args.other layout");

    if (@offsetOf(CArgs, "third") != 72) @compileError("NTFS volume Args.third layout");

    if (@offsetOf(CArgs, "number") != 88) @compileError("NTFS volume Args.number layout");

    if (@offsetOf(CArgs, "second") != 96) @compileError("NTFS volume Args.second layout");

    if (@offsetOf(CArgs, "third_number") != 104) @compileError("NTFS volume Args.third_number layout");

    if (@offsetOf(CArgs, "kind") != 112) @compileError("NTFS volume Args.kind layout");

    if (@offsetOf(CArgs, "flag") != 116) @compileError("NTFS volume Args.flag layout");

    if (@sizeOf(CEnumSink) != 928 or @alignOf(CEnumSink) != 8) @compileError("NTFS volume EnumSink layout");

    if (@offsetOf(CEnumSink, "out") != 0) @compileError("NTFS volume EnumSink.out layout");

    if (@offsetOf(CEnumSink, "has_out") != 16) @compileError("NTFS volume EnumSink.has_out layout");

    if (@offsetOf(CEnumSink, "cursor") != 24) @compileError("NTFS volume EnumSink.cursor layout");

    if (@offsetOf(CEnumSink, "listed") != 32) @compileError("NTFS volume EnumSink.listed layout");

    if (@offsetOf(CEnumSink, "max_entries") != 40) @compileError("NTFS volume EnumSink.max_entries layout");

    if (@offsetOf(CEnumSink, "wanted") != 48) @compileError("NTFS volume EnumSink.wanted layout");

    if (@offsetOf(CEnumSink, "seen") != 56) @compileError("NTFS volume EnumSink.seen layout");

    if (@offsetOf(CEnumSink, "found") != 64) @compileError("NTFS volume EnumSink.found layout");

    if (@offsetOf(CEnumSink, "has_found") != 896) @compileError("NTFS volume EnumSink.has_found layout");

    if (@offsetOf(CEnumSink, "failed") != 897) @compileError("NTFS volume EnumSink.failed layout");

    if (@offsetOf(CEnumSink, "count_ref") != 904) @compileError("NTFS volume EnumSink.count_ref layout");

    if (@offsetOf(CEnumSink, "has_count_ref") != 912) @compileError("NTFS volume EnumSink.has_count_ref layout");

    if (@offsetOf(CEnumSink, "matches") != 920) @compileError("NTFS volume EnumSink.matches layout");
}

// Scratch and cache bytes have one resident native owner. The logical values
// below are bounded projections at the private call boundary, not second
// algorithm implementations or rollback snapshots. The kernel adapter uses
// native storage directly; this facade also retains the original models.
const MAX_INDEX_DEPTH: usize = 8;
pub const Scratch = struct {
    native: [213168]u8 align(8) = undefined,
    initialized: bool = false,
    work: WorkStats = .{},
    bridge_mft: [MAX_MFT_RUNS]CRun = undefined,
    bridge_runs: [MAX_ATTR_RUNS]CRun = undefined,
    bridge_attr: CAttrScratch = undefined,
    logical_attr: AttrScratch = .{},
    // Original foreign-volume models inspect/edit this exact loaded record.
    // It aliases native resident bytes; no second record or copyback exists.
    record: []u8 = &.{},

    fn owner(self: *Scratch) *anyopaque {
        if (!self.initialized) {
            var a = args();
            a.state = &self.native;
            _ = r4_ntfs_volume(0, &a);
            @memset(zstd.mem.asBytes(&self.bridge_attr), 0);
            self.initialized = true;
        }
        // Rust asserts this private field offset in ntfs_volume_layout.rs.
        self.record = self.native[11376 .. 11376 + 4096];
        return &self.native;
    }
};

pub const MetadataCache = struct {
    native: [84328]u8 align(8) = undefined,
    initialized: bool = false,
    bridge_attr: CAttrScratch = undefined,
    fn owner(self: *MetadataCache) *anyopaque {
        if (!self.initialized) {
            var a = args();
            a.state = &self.native;
            _ = r4_ntfs_metadata(0, &a);
            @memset(zstd.mem.asBytes(&self.bridge_attr), 0);
            self.initialized = true;
        }
        return &self.native;
    }
    fn call(self: *MetadataCache, op: u32, input: CArgs) u64 {
        var a = input;
        a.state = self.owner();
        return r4_ntfs_metadata(op, &a);
    }
    pub fn beginMount(self: *MetadataCache, negative_ttl_ticks: u64) void {
        var a = args();
        a.number = negative_ttl_ticks;
        _ = self.call(1, a);
    }
    pub fn invalidateExternal(self: *MetadataCache) void {
        _ = self.call(2, args());
    }
    fn invalidateMutation(self: *MetadataCache, mutation: MetadataMutation) void {
        var a = args();
        mutationArgs(&a, mutation);
        _ = self.call(3, a);
    }
    fn freeClustersFor(self: *const MetadataCache, total: u64) ?u64 {
        var out: u64 = 0;
        var a = args();
        a.number = total;
        a.out = &out;
        return if (@constCast(self).call(16, a) != 0) out else null;
    }
    fn storeFreeClusters(self: *MetadataCache, total: u64, free: u64) void {
        var a = args();
        a.number = total;
        a.second = free;
        _ = self.call(17, a);
    }
    fn lookupRecord(self: *MetadataCache, number: u64, out: []u8) bool {
        var a = args();
        a.number = number;
        a.buffer = buffer(out);
        return self.call(4, a) != 0;
    }
    fn storeRecord(self: *MetadataCache, number: u64, bytes: []const u8) void {
        var a = args();
        a.number = number;
        a.name = view(bytes);
        _ = self.call(5, a);
    }
    fn lookupAttribute(self: *MetadataCache, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, out: *AttrScratch) bool {
        _ = self.owner();
        var a = args();
        a.number = record_number;
        a.kind = @intFromEnum(attr_type);
        a.name = view(name_utf16);
        a.out = &self.bridge_attr;
        if (self.call(6, a) == 0) return false;
        fromAttr(out, &self.bridge_attr);
        return true;
    }
    fn storeAttribute(self: *MetadataCache, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, value: *const AttrScratch) void {
        _ = self.owner();
        toAttr(&self.bridge_attr, value);
        var a = args();
        a.number = record_number;
        a.kind = @intFromEnum(attr_type);
        a.name = view(name_utf16);
        a.out = &self.bridge_attr;
        _ = self.call(7, a);
    }
    fn lookupIndex(self: *MetadataCache, directory_record: u64, vcn: u64, out: []u8) bool {
        var a = args();
        a.number = directory_record;
        a.second = vcn;
        a.buffer = buffer(out);
        return self.call(8, a) != 0;
    }
    fn storeIndex(self: *MetadataCache, directory_record: u64, vcn: u64, bytes: []const u8) void {
        var a = args();
        a.number = directory_record;
        a.second = vcn;
        a.name = view(bytes);
        _ = self.call(9, a);
    }
    fn lookupPath(self: *MetadataCache, upcase: []const u8, directory_record: u64, name_utf16: []const u8, now: u64, out: *LookupResult) PathCacheLookup {
        var result: CLookupResult = zstd.mem.zeroes(CLookupResult);
        var a = args();
        a.number = directory_record;
        a.name = view(name_utf16);
        a.other = view(upcase);
        a.second = now;
        a.out = &result;
        const tag = self.call(10, a);
        if (tag == 1) out.* = project(LookupResult, result);
        return @enumFromInt(tag);
    }
    fn storePath(self: *MetadataCache, directory_record: u64, name_utf16: []const u8, status: LookupStatus, value: *const LookupResult, now: u64) void {
        var result = zstd.mem.zeroes(CLookupResult);
        if (status == .found) result = project(CLookupResult, value.*);
        var a = args();
        a.number = directory_record;
        a.name = view(name_utf16);
        a.second = now;
        a.flag = status == .found;
        a.out = &result;
        _ = self.call(11, a);
    }
    fn noteTreeWalk(self: *MetadataCache) void {
        _ = self.call(12, args());
    }
    fn noteRecoveryBypass(self: *MetadataCache) void {
        _ = self.call(13, args());
    }
    pub fn reclaim(self: *MetadataCache, requested_entries_raw: u32) MetadataReclaimResult {
        var result: CMetadataReclaimResult = undefined;
        var a = args();
        a.kind = requested_entries_raw;
        a.out = &result;
        _ = self.call(14, a);
        return project(MetadataReclaimResult, result);
    }
    pub fn summary(self: *const MetadataCache) MetadataCacheSummary {
        var result: CMetadataCacheSummary = undefined;
        var a = args();
        a.out = &result;
        _ = @constCast(self).call(15, a);
        return project(MetadataCacheSummary, result);
    }
};

fn project(comptime T: type, value: anytype) T {
    return switch (@typeInfo(T)) {
        .@"struct" => result: {
            var out: T = undefined;
            inline for (zstd.meta.fields(T)) |f| @field(out, f.name) = project(f.type, @field(value, f.name));
            break :result out;
        },
        .@"enum" => @enumFromInt(value),
        else => @as(T, value),
    };
}
fn view(bytes: []const u8) CView {
    return .{ .data = bytes.ptr, .len = bytes.len };
}
fn buffer(bytes: []u8) CBuffer {
    return .{ .data = bytes.ptr, .len = bytes.len };
}
fn args() CArgs {
    return zstd.mem.zeroes(CArgs);
}
fn cRun(run: ntfs.Run) CRun {
    return .{ .length_clusters = run.length_clusters, .has_lcn = run.lcn != null, .lcn = run.lcn orelse 0 };
}
fn zRun(run: CRun) ntfs.Run {
    return .{ .length_clusters = run.length_clusters, .lcn = if (run.has_lcn) run.lcn else null };
}
fn toAttr(out: *CAttrScratch, value: *const AttrScratch) void {
    zstd.debug.assert(value.count <= MAX_ATTR_RUNS and value.resident_len <= 1024);
    out.count = value.count;
    out.data_size = value.data_size;
    out.initialized_size = value.initialized_size;
    out.alloc_size = value.alloc_size;
    out.flags = value.flags;
    out.compression_unit = value.compression_unit;
    out.resident = value.resident;
    out.resident_len = value.resident_len;
    for (value.runs[0..value.count], out.runs[0..value.count]) |run, *target| target.* = cRun(run);
    if (value.resident) @memcpy(out.resident_copy[0..value.resident_len], value.resident_copy[0..value.resident_len]);
}
fn fromAttr(out: *AttrScratch, value: *const CAttrScratch) void {
    zstd.debug.assert(value.count <= MAX_ATTR_RUNS and value.resident_len <= 1024);
    out.count = value.count;
    out.data_size = value.data_size;
    out.initialized_size = value.initialized_size;
    out.alloc_size = value.alloc_size;
    out.flags = value.flags;
    out.compression_unit = value.compression_unit;
    out.resident = value.resident;
    out.resident_len = value.resident_len;
    for (value.runs[0..value.count], out.runs[0..value.count]) |run, *target| target.* = zRun(run);
    if (value.resident) @memcpy(out.resident_copy[0..value.resident_len], value.resident_copy[0..value.resident_len]);
}
fn mutationArgs(a: *CArgs, mutation: MetadataMutation) void {
    switch (mutation) {
        .payload => a.kind = 0,
        .record => |n| {
            a.kind = 1;
            a.number = n;
        },
        .attribute => |x| {
            a.kind = 2;
            a.number = x.record_number;
            a.second = @intFromEnum(x.attr_type);
        },
        .directory => |n| {
            a.kind = 3;
            a.number = n;
        },
        .system => a.kind = 4,
        .unknown => a.kind = 5,
        .recovery => a.kind = 6,
    }
}
fn readBridge(ctx: *anyopaque, lba: u64, count: u32, out: [*]u8, len: usize) callconv(.c) bool {
    const device: *const Device = @ptrCast(@alignCast(ctx));
    return device.read_sectors(device.ctx, lba, count, out[0..len]);
}
fn writeBridge(ctx: *anyopaque, lba: u64, count: u32, data: [*]const u8, len: usize) callconv(.c) bool {
    const device: *const Device = @ptrCast(@alignCast(ctx));
    return device.write_sectors(device.ctx, lba, count, data[0..len]);
}
fn flushBridge(ctx: *anyopaque) callconv(.c) bool {
    const device: *const Device = @ptrCast(@alignCast(ctx));
    return device.flush(device.ctx);
}
fn nativeVolume(v: *const Volume) CVolume {
    const owner = v.scratch.owner();
    zstd.debug.assert(v.mft_runs_buf.len <= MAX_MFT_RUNS and v.mft_run_count.* <= v.mft_runs_buf.len);
    for (v.mft_runs_buf[0..v.mft_run_count.*], v.scratch.bridge_mft[0..v.mft_run_count.*]) |run, *out| out.* = cRun(run);
    const work: *CWorkStats = @ptrCast(@alignCast(owner));
    work.* = project(CWorkStats, v.scratch.work);
    return .{ .io = .{ .device = .{ .context = @constCast(&v.device), .read = readBridge, .write = writeBridge, .flush = flushBridge, .max_transfer_sectors = v.device.max_transfer_sectors }, .geometry = .{ .partition_lba = v.partition_lba, .cluster_bytes = v.cluster_bytes, .total_sectors = v.total_sectors }, .scratch = owner, .cache = if (v.metadata_cache) |c| c.owner() else null, .work_counters = work_counters_enabled }, .record_bytes = v.record_bytes, .index_block_bytes = v.index_block_bytes, .mft_runs = &v.scratch.bridge_mft, .mft_capacity = v.mft_runs_buf.len, .mft_count = v.mft_run_count, .upcase = view(v.upcase), .now_filetime = v.now_filetime, .security_id_file = v.security_id_file, .security_id_dir = v.security_id_dir, .cache_now_ticks = v.metadata_cache_now_ticks };
}
fn callVolume(v: *const Volume, op: u32, input: CArgs) u64 {
    var native = nativeVolume(v);
    var a = input;
    a.volume = &native;
    var seams = args();
    seams.flag = flush_budget != null;
    seams.number = flush_budget orelse 0;
    seams.kind = @intFromBool(cut_after_canonical_rewrite);
    _ = r4_ntfs_volume(42, &seams);
    const result = r4_ntfs_volume(op, &a);
    const remaining = r4_ntfs_volume(43, &seams);
    flush_budget = if (remaining & (1 << 32) != 0) @truncate(remaining) else null;
    zstd.debug.assert(v.mft_run_count.* <= v.mft_runs_buf.len);
    for (v.scratch.bridge_mft[0..v.mft_run_count.*], v.mft_runs_buf[0..v.mft_run_count.*]) |run, *out| out.* = zRun(run);
    const work: *const CWorkStats = @ptrCast(@alignCast(&v.scratch.native));
    v.scratch.work = project(WorkStats, work.*);
    return result;
}

pub fn mount(device: Device, partition_lba: u32, scratch: *Scratch, runs_out: []ntfs.Run) ?MountInfo {
    var count: usize = 0;
    const v = Volume{ .device = device, .partition_lba = partition_lba, .cluster_bytes = 0, .record_bytes = 0, .index_block_bytes = 0, .total_sectors = 0, .mft_runs_buf = runs_out, .mft_run_count = &count, .upcase = &.{}, .scratch = scratch };
    var out: CMountInfo = undefined;
    var a = args();
    a.out = &out;
    if (callVolume(&v, 2, a) == 0) return null;
    // mount returns the validated run count separately; its local bootstrap
    // view never mutates the caller's runtime run-count pointer.
    for (scratch.bridge_mft[0..out.mft_run_count], runs_out[0..out.mft_run_count]) |run, *target| target.* = zRun(run);
    return project(MountInfo, out);
}
pub fn loadRecord(v: *const Volume, number: u64, buf: []u8) ?ntfs.FileRecordHeader {
    var out: CFileRecordHeader = undefined;
    var a = args();
    a.number = number;
    a.buffer = buffer(buf);
    a.out = &out;
    return if (callVolume(v, 3, a) != 0) project(ntfs.FileRecordHeader, out) else null;
}
pub fn collectAttributeStatus(v: *const Volume, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, out: *AttrScratch) LookupStatus {
    _ = v.scratch.owner();
    var a = args();
    a.number = record_number;
    a.kind = @intFromEnum(attr_type);
    a.name = view(name_utf16);
    a.out = &v.scratch.bridge_attr;
    const tag = callVolume(v, 4, a);
    fromAttr(out, &v.scratch.bridge_attr);
    return @enumFromInt(tag);
}
pub fn collectAttribute(v: *const Volume, record_number: u64, attr_type: ntfs.AttrType, name_utf16: []const u8, out: *AttrScratch) bool {
    return collectAttributeStatus(v, record_number, attr_type, name_utf16, out) == .found;
}
pub fn lookupInDirectoryStatus(v: *const Volume, dir_record: u64, name: []const u8, out: *LookupResult) LookupStatus {
    return lookupMode(v, dir_record, name, out, false);
}
pub fn lookupInDirectoryStatusTransient(v: *const Volume, dir_record: u64, name: []const u8, out: *LookupResult) LookupStatus {
    return lookupMode(v, dir_record, name, out, true);
}
fn lookupMode(v: *const Volume, dir_record: u64, name: []const u8, out: *LookupResult, transient: bool) LookupStatus {
    var result: CLookupResult = undefined;
    var a = args();
    a.number = dir_record;
    a.name = view(name);
    a.out = &result;
    a.flag = transient;
    const tag = callVolume(v, 5, a);
    if (tag == 0) out.* = project(LookupResult, result);
    return @enumFromInt(tag);
}
pub fn lookupInDirectory(v: *const Volume, dir_record: u64, name: []const u8) ?LookupResult {
    var out: LookupResult = undefined;
    return if (lookupInDirectoryStatus(v, dir_record, name, &out) == .found) out else null;
}
pub fn lookupDiagnosticStage() u32 {
    var a = args();
    return @intCast(r4_ntfs_volume(40, &a));
}
pub fn appendDiagnosticStage() u32 {
    var a = args();
    return @intCast(r4_ntfs_volume(41, &a));
}
pub fn resolvePathStatus(v: *const Volume, path: []const u8, out: *u64) LookupStatus {
    var a = args();
    a.name = view(path);
    a.out = out;
    return @enumFromInt(callVolume(v, 6, a));
}
pub fn resolvePath(v: *const Volume, path: []const u8) ?u64 {
    var out: u64 = undefined;
    return if (resolvePathStatus(v, path, &out) == .found) out else null;
}
pub fn resolveEntryStatus(v: *const Volume, path: []const u8, out: *ResolvedEntry) LookupStatus {
    var result: CResolvedEntry = undefined;
    var a = args();
    a.name = view(path);
    a.out = &result;
    const tag = callVolume(v, 7, a);
    if (tag == 0) out.* = project(ResolvedEntry, result);
    return @enumFromInt(tag);
}
pub fn resolveEntry(v: *const Volume, path: []const u8) ?ResolvedEntry {
    var out: ResolvedEntry = undefined;
    return if (resolveEntryStatus(v, path, &out) == .found) out else null;
}
pub fn recordParentAndName(v: *const Volume, record_number: u64) ?RecordName {
    var out: CRecordName = undefined;
    var a = args();
    a.number = record_number;
    a.out = &out;
    return if (callVolume(v, 8, a) != 0) project(RecordName, out) else null;
}
pub fn nextDirectoryEntry(v: *const Volume, dir: u64, cursor: *DirectoryCursor, out: *Entry) DirectoryNextStatus {
    var result: CEntry = undefined;
    var a = args();
    a.number = dir;
    a.state = cursor;
    a.out = &result;
    const tag = callVolume(v, 9, a);
    if (tag == 0) out.* = project(Entry, result);
    return @enumFromInt(tag);
}
pub fn enumerateDirectory(v: *const Volume, dir_record: u64, sink: *EnumSink) bool {
    var native = zstd.mem.zeroes(CEnumSink);
    native.out = if (sink.out) |b| buffer(b) else zstd.mem.zeroes(CBuffer);
    native.has_out = sink.out != null;
    native.cursor = sink.cursor;
    native.listed = sink.listed;
    native.max_entries = sink.max_entries;
    native.wanted = sink.wanted;
    native.seen = sink.seen;
    native.has_found = sink.found != null;
    if (sink.found) |entry| native.found = project(CEntry, entry);
    native.failed = sink.failed;
    native.has_count_ref = sink.count_ref != null;
    native.count_ref = sink.count_ref orelse 0;
    native.matches = sink.matches;
    var a = args();
    a.number = dir_record;
    a.state = &native;
    const result = callVolume(v, 10, a) != 0;
    sink.cursor = native.cursor;
    sink.listed = native.listed;
    sink.seen = native.seen;
    sink.found = if (native.has_found) project(Entry, native.found) else null;
    sink.failed = native.failed;
    sink.matches = native.matches;
    return result;
}
fn readRange(v: *const Volume, op: u32, number: u64, name: []const u8, offset: usize, out: []u8) ?usize {
    var n: usize = 0;
    var a = args();
    a.number = number;
    a.name = view(name);
    a.second = offset;
    a.buffer = buffer(out);
    a.out = &n;
    return if (callVolume(v, op, a) != 0) n else null;
}
pub fn readFileRange(v: *const Volume, record_number: u64, offset: usize, out: []u8) ?usize {
    return readRange(v, 11, record_number, &.{}, offset, out);
}
pub fn readNamedStreamRange(v: *const Volume, record_number: u64, stream_name_utf8: []const u8, offset: usize, out: []u8) ?usize {
    return readRange(v, 12, record_number, stream_name_utf8, offset, out);
}
fn readAttrRange(v: *const Volume, attr: *AttrScratch, offset: usize, out: []u8) ?usize {
    _ = v.scratch.owner();
    toAttr(&v.scratch.bridge_attr, attr);
    var n: usize = 0;
    var a = args();
    a.state = &v.scratch.bridge_attr;
    a.second = offset;
    a.buffer = buffer(out);
    a.out = &n;
    return if (callVolume(v, 13, a) != 0) n else null;
}
fn writeName(v: *const Volume, op: u32, parent: u64, name: []const u8, data: []const u8) WriteStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    a.other = view(data);
    return @enumFromInt(callVolume(v, op, a));
}
pub fn createFile(v: *const Volume, parent_record: u64, name: []const u8, data: []const u8) WriteStatus {
    return writeName(v, 14, parent_record, name, data);
}
pub fn writeFile(v: *const Volume, parent_record: u64, name: []const u8, data: []const u8) WriteStatus {
    return writeName(v, 15, parent_record, name, data);
}
pub fn deleteFile(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    return writeName(v, 16, parent_record, name, &.{});
}
pub fn createDirectory(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    return writeName(v, 19, parent_record, name, &.{});
}
pub fn deleteDirectory(v: *const Volume, parent_record: u64, name: []const u8) WriteStatus {
    return writeName(v, 20, parent_record, name, &.{});
}
fn appendMode(v: *const Volume, parent: u64, name: []const u8, offset: u64, data: []const u8, durable: bool) WriteStatus {
    var a = args();
    a.number = parent;
    a.name = view(name);
    a.second = offset;
    a.other = view(data);
    a.flag = durable;
    return @enumFromInt(callVolume(v, 17, a));
}
pub fn appendFileAtOffset(v: *const Volume, parent_record: u64, name: []const u8, expected_offset: u64, data: []const u8) WriteStatus {
    return appendMode(v, parent_record, name, expected_offset, data, true);
}
pub fn appendFileAtOffsetDeferred(v: *const Volume, parent_record: u64, name: []const u8, expected_offset: u64, data: []const u8) WriteStatus {
    return appendMode(v, parent_record, name, expected_offset, data, false);
}
pub fn finishDeferred(v: *const Volume) bool {
    return callVolume(v, 18, args()) != 0;
}
pub fn renameEntry(v: *const Volume, old_parent: u64, old_name: []const u8, new_parent: u64, new_name: []const u8) WriteStatus {
    var a = args();
    a.number = old_parent;
    a.name = view(old_name);
    a.second = new_parent;
    a.other = view(new_name);
    return @enumFromInt(callVolume(v, 21, a));
}
pub fn replaceFileAtomic(v: *const Volume, parent_record: u64, target_name: []const u8, staged_name: []const u8, backup_name: []const u8, consume_stage: bool) ReplaceResult {
    var a = args();
    a.number = parent_record;
    a.name = view(target_name);
    a.other = view(staged_name);
    a.third = view(backup_name);
    a.flag = consume_stage;
    return @enumFromInt(callVolume(v, 22, a));
}
pub fn publishFileCreateOnly(v: *const Volume, parent_record: u64, target_name: []const u8, staged_name: []const u8) ReplaceResult {
    var a = args();
    a.number = parent_record;
    a.name = view(target_name);
    a.other = view(staged_name);
    return @enumFromInt(callVolume(v, 23, a));
}
pub fn namesEqualCollated(v: *const Volume, a: []const u8, b: []const u8) ?bool {
    var x = args();
    x.name = view(a);
    x.other = view(b);
    const tag = callVolume(v, 24, x);
    return if (tag == 0) null else tag == 2;
}
pub fn writeFileAtProgress(v: *const Volume, record_number: u64, offset: u64, data: []const u8) WriteProgress {
    var out: CWriteProgress = undefined;
    var a = args();
    a.number = record_number;
    a.second = offset;
    a.name = view(data);
    a.out = &out;
    _ = callVolume(v, 25, a);
    return project(WriteProgress, out);
}
pub fn writeFileAt(v: *const Volume, record_number: u64, offset: u64, data: []const u8) WriteStatus {
    return writeFileAtProgress(v, record_number, offset, data).status;
}
pub fn freeClusterCount(v: *const Volume) ?u64 {
    var out: u64 = 0;
    var a = args();
    a.out = &out;
    return if (callVolume(v, 26, a) != 0) out else null;
}
pub fn setDirty(v: *const Volume, dirty: bool) bool {
    var a = args();
    a.flag = dirty;
    return callVolume(v, 27, a) != 0;
}
pub fn isDirty(v: *const Volume) ?bool {
    const tag = callVolume(v, 28, args());
    return if (tag == 0) null else tag == 2;
}
pub fn dataResidesInExtension(v: *const Volume, parent_record: u64, name: []const u8) bool {
    var a = args();
    a.number = parent_record;
    a.name = view(name);
    return callVolume(v, 29, a) != 0;
}
pub fn allocateClustersForTest(v: *const Volume, count: u64, runs_out: []ntfs.Run) AllocationResult {
    _ = v.scratch.owner();
    zstd.debug.assert(runs_out.len <= MAX_ATTR_RUNS);
    var n: usize = 0;
    var a = args();
    a.number = count;
    a.buffer = .{ .data = @ptrCast(&v.scratch.bridge_runs), .len = runs_out.len };
    a.out = &n;
    const tag = callVolume(v, 30, a);
    for (v.scratch.bridge_runs[0..n], runs_out[0..n]) |run, *out| out.* = zRun(run);
    return .{ .status = @enumFromInt(tag), .produced = n };
}
fn runsArgs(v: *const Volume, runs: []const ntfs.Run) CArgs {
    _ = v.scratch.owner();
    zstd.debug.assert(runs.len <= MAX_ATTR_RUNS);
    for (runs, v.scratch.bridge_runs[0..runs.len]) |run, *out| out.* = cRun(run);
    var a = args();
    a.buffer = .{ .data = @ptrCast(&v.scratch.bridge_runs), .len = runs.len };
    return a;
}
pub fn freeClustersForTest(v: *const Volume, runs: []const ntfs.Run) bool {
    return callVolume(v, 31, runsArgs(v, runs)) != 0;
}
pub fn abortWriteForTest(v: *const Volume, status: WriteStatus) WriteStatus {
    var a = args();
    a.kind = @intFromEnum(status);
    return @enumFromInt(callVolume(v, 32, a));
}
pub fn abortWriteFreeingForTest(v: *const Volume, runs: []const ntfs.Run, status: WriteStatus) WriteStatus {
    var a = runsArgs(v, runs);
    a.kind = @intFromEnum(status);
    return @enumFromInt(callVolume(v, 33, a));
}
pub fn storeRecordForTest(v: *const Volume, number: u64) bool {
    var a = args();
    a.number = number;
    return callVolume(v, 34, a) != 0;
}
pub fn loadRecordForTest(v: *const Volume, number: u64) ?ntfs.FileRecordHeader {
    var out: CFileRecordHeader = undefined;
    var a = args();
    a.number = number;
    a.out = &out;
    return if (callVolume(v, 35, a) != 0) project(ntfs.FileRecordHeader, out) else null;
}
pub fn collectAttributeForTest(v: *const Volume, record_number: u64) ?*AttrScratch {
    return if (collectAttribute(v, record_number, .data, &.{}, &v.scratch.logical_attr)) &v.scratch.logical_attr else null;
}
fn readLcnBytes(v: *const Volume, lcn: u64, byte_offset: u64, out: []u8) bool {
    var a = args();
    a.number = lcn;
    a.second = byte_offset;
    a.buffer = buffer(out);
    return callVolume(v, 36, a) != 0;
}
fn writeLcnBytes(v: *const Volume, lcn: u64, byte_offset: u64, data: []const u8, mutation: MetadataMutation) bool {
    var a = args();
    mutationArgs(&a, mutation);
    a.third_number = a.number;
    a.number = lcn;
    a.second = byte_offset;
    a.buffer = buffer(@constCast(data));
    return callVolume(v, 37, a) != 0;
}
fn setBitmapRange(v: *const Volume, bitmap_attr: *const AttrScratch, lcn: u64, count: u64, set: bool) bool {
    _ = v.scratch.owner();
    toAttr(&v.scratch.bridge_attr, bitmap_attr);
    var a = args();
    a.state = &v.scratch.bridge_attr;
    a.number = lcn;
    a.second = count;
    a.flag = set;
    return callVolume(v, 38, a) != 0;
}
pub fn deleteRecoveryEntryIfIdentity(v: *const Volume, parent_record: u64, name: []const u8, record_number: u64, sequence: u16) RecoveryDeleteResult {
    var a = args();
    a.number = parent_record;
    a.name = view(name);
    a.second = record_number;
    a.third_number = sequence;
    return @enumFromInt(callVolume(v, 39, a));
}
pub fn requireSingleLinkStatus(v: *const Volume, record_number: u64, expected_sequence: u16) WriteStatus {
    var a = args();
    a.number = record_number;
    a.second = expected_sequence;
    return @enumFromInt(callVolume(v, 44, a));
}
pub fn recordIdentityStatus(v: *const Volume, record_number: u64, sequence: u16, expect_directory: bool) LookupStatus {
    var a = args();
    a.number = record_number;
    a.second = sequence;
    a.flag = expect_directory;
    return @enumFromInt(callVolume(v, 45, a));
}
pub fn recordLinkInfoStatus(v: *const Volume, record_number: u64, out: *RecordLinkInfo) LookupStatus {
    var n: u32 = 0;
    var a = args();
    a.number = record_number;
    a.out = &n;
    const tag = callVolume(v, 46, a);
    if (tag == 0) out.* = .{ .sequence = @truncate(n), .link_count = @intCast(n >> 16) };
    return @enumFromInt(tag);
}
pub var flush_budget: ?u32 = null;
pub var cut_after_canonical_rewrite: bool = false;

test "free-space count caches exact bitmap deltas and discards uncertain state" {
    const std = @import("std");
    const testing = std.testing;
    const Memory = struct {
        bytes: [1024]u8 = .{0} ** 1024,
        reads: u32 = 0,
        fail_read: bool = false,
        fail_write: bool = false,
        fn read(ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.reads += 1;
            if (self.fail_read or lba < 1 or lba - 1 + count > 2) return false;
            @memcpy(out, self.bytes[@intCast((lba - 1) * 512)..][0..out.len]);
            return true;
        }
        fn write(ctx: *anyopaque, lba: u64, count: u32, data: []const u8) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            if (lba < 1 or lba - 1 + count > 2) return false;
            @memcpy(self.bytes[@intCast((lba - 1) * 512)..][0..data.len], data);
            return !self.fail_write; // failure may follow an actual media write
        }
        fn flush(_: *anyopaque) bool {
            return true;
        }
    };
    var memory = Memory{};
    memory.bytes[0] = 0x85; // three allocated clusters
    memory.bytes[512] = 0xfd; // one free of the final three valid bits
    const scratch = try testing.allocator.create(Scratch);
    defer testing.allocator.destroy(scratch);
    scratch.* = .{};
    const cache = try testing.allocator.create(MetadataCache);
    defer testing.allocator.destroy(cache);
    cache.* = .{};
    var run_count: usize = 0;
    var v = Volume{
        .device = .{ .ctx = &memory, .read_sectors = Memory.read, .write_sectors = Memory.write, .flush = Memory.flush },
        .partition_lba = 0,
        .cluster_bytes = 512,
        .record_bytes = 1024,
        .index_block_bytes = 4096,
        .total_sectors = 4099,
        .mft_runs_buf = &.{},
        .mft_run_count = &run_count,
        .upcase = &.{},
        .scratch = scratch,
        .metadata_cache = cache,
    };
    var attr = AttrScratch{ .count = 1, .data_size = 513, .initialized_size = 513, .alloc_size = 1024 };
    attr.runs[0] = .{ .lcn = 1, .length_clusters = 2 };
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    try testing.expectEqual(@as(?u64, 4094), freeClusterCount(&v));
    const reads = memory.reads;
    try testing.expectEqual(@as(?u64, 4094), freeClusterCount(&v));
    try testing.expectEqual(reads, memory.reads);

    try testing.expectEqual(WriteStatus.ok, writeFileAt(&v, ntfs.MFT_RECORD_BITMAP, 0, &.{0}));
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4099));
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    try testing.expectEqual(@as(?u64, 4097), freeClusterCount(&v));
    try testing.expectEqual(WriteStatus.ok, writeFileAt(&v, ntfs.MFT_RECORD_BITMAP, 0, &.{0x85}));
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    try testing.expectEqual(@as(?u64, 4094), freeClusterCount(&v));

    // Includes already-set bits: count actual transitions, not range length.
    try testing.expect(setBitmapRange(&v, &attr, 0, 10, true));
    try testing.expectEqual(@as(?u64, 4087), freeClusterCount(&v));
    try testing.expect(setBitmapRange(&v, &attr, 0, 10, true));
    try testing.expectEqual(@as(?u64, 4087), freeClusterCount(&v));
    try testing.expect(setBitmapRange(&v, &attr, 0, 10, false));
    try testing.expectEqual(@as(?u64, 4097), freeClusterCount(&v));
    cache.invalidateMutation(.payload);
    cache.invalidateMutation(.{ .record = 42 });
    try testing.expectEqual(@as(?u64, 4097), freeClusterCount(&v));

    memory.fail_write = true;
    try testing.expect(!setBitmapRange(&v, &attr, 4094, 5, true));
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4099));
    memory.fail_write = false;
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    memory.fail_read = true;
    try testing.expectEqual(@as(?u64, null), freeClusterCount(&v));
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4099));
    memory.fail_read = false;
    try testing.expectEqual(@as(?u64, 4095), freeClusterCount(&v));

    cache.invalidateMutation(.{ .record = ntfs.MFT_RECORD_BITMAP });
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4099));
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    attr.initialized_size = 512;
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    try testing.expectEqual(@as(?u64, null), freeClusterCount(&v));
    attr.initialized_size = 513;
    cache.storeAttribute(ntfs.MFT_RECORD_BITMAP, .data, &.{}, &attr);
    try testing.expectEqual(@as(?u64, 4095), freeClusterCount(&v));
    v.total_sectors = 4096; // a different geometry cannot reuse the old count
    try testing.expectEqual(@as(?u64, 4094), freeClusterCount(&v));
    cache.beginMount(100);
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4096));
    cache.storeFreeClusters(4096, 100);
    cache.invalidateExternal();
    try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4096));
    for ([_]MetadataMutation{ .unknown, .recovery }) |mutation| {
        cache.storeFreeClusters(4096, 100);
        cache.invalidateMutation(mutation);
        try testing.expectEqual(@as(?u64, null), cache.freeClustersFor(4096));
    }
}

test "range reads zero only holes and uninitialized tails, preserve short results and errors" {
    const std = @import("std");
    const Memory = struct {
        bytes: [2048]u8 = .{0xa7} ** 2048,
        calls: u32 = 0,
        fail: bool = false,
        fn read(ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.calls += 1;
            if (self.fail or lba + count > 4) return false;
            @memcpy(out, self.bytes[@intCast(lba * 512)..][0..out.len]);
            return true;
        }
        fn write(_: *anyopaque, _: u64, _: u32, _: []const u8) bool {
            return false;
        }
        fn flush(_: *anyopaque) bool {
            return true;
        }
    };
    var memory = Memory{};
    const scratch = try std.testing.allocator.create(Scratch);
    defer std.testing.allocator.destroy(scratch);
    scratch.* = .{};
    var run_count: usize = 0;
    const v = Volume{
        .device = .{ .ctx = &memory, .read_sectors = Memory.read, .write_sectors = Memory.write, .flush = Memory.flush },
        .partition_lba = 0,
        .cluster_bytes = 512,
        .record_bytes = 1024,
        .index_block_bytes = 4096,
        .total_sectors = 4,
        .mft_runs_buf = &.{},
        .mft_run_count = &run_count,
        .upcase = &.{},
        .scratch = scratch,
    };
    var attr = AttrScratch{ .count = 3, .data_size = 1536, .initialized_size = 1280 };
    attr.runs[0] = .{ .lcn = 1, .length_clusters = 1 };
    attr.runs[1] = .{ .lcn = null, .length_clusters = 1 };
    attr.runs[2] = .{ .lcn = 3, .length_clusters = 1 };
    var out: [1600]u8 = .{0xcc} ** 1600;
    try std.testing.expectEqual(@as(?usize, 1536), readAttrRange(&v, &attr, 0, &out));
    try std.testing.expectEqualSlices(u8, memory.bytes[0..512], out[0..512]);
    for (out[512..1024]) |byte| try std.testing.expectEqual(@as(u8, 0), byte);
    for (out[1024..1280]) |byte| try std.testing.expectEqual(@as(u8, 0xa7), byte);
    for (out[1280..1536]) |byte| try std.testing.expectEqual(@as(u8, 0), byte);
    for (out[1536..]) |byte| try std.testing.expectEqual(@as(u8, 0xcc), byte);
    try std.testing.expectEqual(@as(u64, 768), scratch.work.read_zero_bytes);
    const before = memory.calls;
    try std.testing.expectEqual(@as(?usize, 256), readAttrRange(&v, &attr, 1280, &out));
    try std.testing.expectEqual(before, memory.calls); // uninitialized tail never reaches the device
    try std.testing.expectEqual(@as(?usize, 0), readAttrRange(&v, &attr, 1536, &out));
    memory.fail = true;
    @memset(&out, 0xcc);
    try std.testing.expectEqual(@as(?usize, null), readAttrRange(&v, &attr, 0, &out));
    for (out) |byte| try std.testing.expectEqual(@as(u8, 0xcc), byte);
    memory.fail = false;
    attr.count = 1; // malformed missing run: partial bytes do not become a successful read
    try std.testing.expectEqual(@as(?usize, null), readAttrRange(&v, &attr, 0, &out));
}

test "extent transfers honor transport caps and preserve partial-sector neighbors" {
    const std = @import("std");
    const Memory = struct {
        bytes: [300 * 512]u8 = .{0xa7} ** (300 * 512),
        limit: u32 = 256,
        maximum: u32 = 0,
        calls: u32 = 0,
        fail_at: u32 = 0,
        fn read(ctx: *anyopaque, lba: u64, count: u32, out: []u8) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.calls += 1;
            self.maximum = @max(self.maximum, count);
            if (count > self.limit or self.calls == self.fail_at) return false;
            @memcpy(out, self.bytes[@intCast(lba * 512)..][0..out.len]);
            return true;
        }
        fn write(ctx: *anyopaque, lba: u64, count: u32, bytes: []const u8) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.calls += 1;
            self.maximum = @max(self.maximum, count);
            if (count > self.limit or self.calls == self.fail_at) return false;
            @memcpy(self.bytes[@intCast(lba * 512)..][0..bytes.len], bytes);
            return true;
        }
        fn flush(_: *anyopaque) bool {
            return true;
        }
    };
    const memory = try std.testing.allocator.create(Memory);
    defer std.testing.allocator.destroy(memory);
    memory.* = .{};
    const scratch = try std.testing.allocator.create(Scratch);
    defer std.testing.allocator.destroy(scratch);
    scratch.* = .{};
    var run_count: usize = 0;
    var v = Volume{
        .device = .{ .ctx = memory, .read_sectors = Memory.read, .write_sectors = Memory.write, .flush = Memory.flush, .max_transfer_sectors = 0 },
        .partition_lba = 0,
        .cluster_bytes = 512,
        .record_bytes = 1024,
        .index_block_bytes = 4096,
        .total_sectors = 300,
        .mft_runs_buf = &.{},
        .mft_run_count = &run_count,
        .upcase = &.{},
        .scratch = scratch,
    };
    const out = try std.testing.allocator.alloc(u8, 280 * 512);
    defer std.testing.allocator.free(out);
    for ([_]u32{ 0, 7, 1 }) |limit| {
        v.device.max_transfer_sectors = limit;
        memory.limit = if (limit == 0) 256 else limit;
        memory.maximum = 0;
        @memset(out, 0x79);
        try std.testing.expect(writeLcnBytes(&v, 0, 13, out, .payload));
        try std.testing.expectEqual(@as(u8, 0xa7), memory.bytes[12]);
        try std.testing.expectEqual(@as(u8, 0xa7), memory.bytes[13 + out.len]);
        @memset(out, 0xcc);
        try std.testing.expect(readLcnBytes(&v, 0, 13, out));
        for (out) |byte| try std.testing.expectEqual(@as(u8, 0x79), byte);
        try std.testing.expectEqual(memory.limit, memory.maximum);
    }
    memory.fail_at = memory.calls + 2;
    try std.testing.expect(!readLcnBytes(&v, 0, 0, out));
    memory.fail_at = memory.calls + 2;
    try std.testing.expect(!writeLcnBytes(&v, 0, 0, out, .payload));
}

test "metadata mutations retain payload reads and invalidate exact dependencies" {
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

    cache.invalidateMutation(.payload);
    var after = cache.summary();
    try testing.expectEqual(@as(u32, 4), after.record_entries + after.attribute_entries + after.index_entries + after.path_entries);
    try testing.expectEqual(@as(u64, 1), after.payload_write_retentions);

    cache.invalidateMutation(.system);
    after = cache.summary();
    try testing.expectEqual(@as(u32, 4), after.record_entries + after.attribute_entries + after.index_entries + after.path_entries);
    try testing.expectEqual(@as(u64, 1), after.system_write_retentions);

    cache.invalidateMutation(.{ .attribute = .{ .record_number = 17, .attr_type = .data } });
    after = cache.summary();
    try testing.expectEqual(@as(u32, 0), after.attribute_entries);
    try testing.expectEqual(@as(u32, 1), after.record_entries);
    try testing.expectEqual(@as(u32, 1), after.index_entries);
    try testing.expectEqual(@as(u32, 1), after.path_entries);
    cache.storeAttribute(17, .data, &[_]u8{}, &attribute);

    cache.invalidateMutation(.{ .record = 17 });
    after = cache.summary();
    try testing.expectEqual(@as(u32, 0), after.record_entries);
    try testing.expectEqual(@as(u32, 0), after.attribute_entries);
    try testing.expectEqual(@as(u32, 1), after.index_entries);
    try testing.expectEqual(@as(u32, 0), after.path_entries);
    try testing.expect(!cache.lookupRecord(17, record_out[0..]));
    try testing.expect(cache.lookupIndex(5, 2, block_out[0..]));

    cache.storeRecord(17, record[0..]);
    cache.storeAttribute(17, .data, &[_]u8{}, &attribute);
    cache.storePath(5, name[0..name_len], .found, &found, 20);
    cache.invalidateMutation(.{ .directory = 5 });
    after = cache.summary();
    try testing.expectEqual(@as(u32, 1), after.record_entries);
    try testing.expectEqual(@as(u32, 1), after.attribute_entries);
    try testing.expectEqual(@as(u32, 0), after.index_entries);
    try testing.expectEqual(@as(u32, 0), after.path_entries);

    cache.invalidateMutation(.recovery);
    after = cache.summary();
    try testing.expectEqual(@as(u32, 0), after.record_entries + after.attribute_entries + after.index_entries + after.path_entries);
    try testing.expectEqual(@as(u64, 3), after.targeted_invalidations);
    try testing.expectEqual(@as(u64, 1), after.targeted_record_invalidations);
    try testing.expectEqual(@as(u64, 1), after.targeted_attribute_invalidations);
    try testing.expectEqual(@as(u64, 1), after.targeted_directory_invalidations);
    try testing.expectEqual(@as(u64, 1), after.global_mutation_invalidations);
    try testing.expectEqual(@as(u64, 1), after.recovery_invalidations);
    try testing.expectEqual(@as(u64, 4), after.mutation_invalidations);
    try testing.expectEqual(@as(u64, 8), after.invalidated_entries);
    try testing.expectEqual(@as(u64, 2), after.mutation_invalidated_record_entries);
    try testing.expectEqual(@as(u64, 3), after.mutation_invalidated_attribute_entries);
    try testing.expectEqual(@as(u64, 1), after.mutation_invalidated_index_entries);
    try testing.expectEqual(@as(u64, 2), after.mutation_invalidated_path_entries);
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

comptime {
    if (@sizeOf(DirectoryCursor) != @sizeOf(CDirectoryCursor) or @alignOf(DirectoryCursor) != @alignOf(CDirectoryCursor)) @compileError("NTFS cursor boundary shape");
    if (@offsetOf(DirectoryCursor, "state") != 0 or @offsetOf(DirectoryCursor, "depth") != 4 or @offsetOf(DirectoryCursor, "frames") != 8) @compileError("NTFS cursor offsets");
    if (@sizeOf(DirectoryCursor.Frame) != 16 or @offsetOf(DirectoryCursor.Frame, "vcn") != 0 or @offsetOf(DirectoryCursor.Frame, "offset") != 8 or @offsetOf(DirectoryCursor.Frame, "child_done") != 12) @compileError("NTFS cursor frame layout");
}
