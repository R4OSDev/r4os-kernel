// Stateless kernel-private NTFS codec facade; algorithms live in native Rust.
//
// Kernel copy only. SDK/hosttools/Recovery keep their unchanged Zig codecs.
// Original public value types, constants and on-disk bytes remain stable.
//
// Scope: pure layout codecs over caller-provided byte slices.  No I/O, no
// allocation, no OS dependencies; usable from the freestanding kernel and
// from host tools alike.  Volume walking (directory descent, file reads over
// a block device) stays in the consumers.
//
// References: Reference/FileSystem/NTFS/ (ntfsdoc.pdf, ntfs-3g-layout.h,
// linux-ntfs3-ntfs.h, NTFSStrukturUebersicht.txt).

const std = @import("std");

pub const SECTOR_SIZE: usize = 512;
pub const FILE_MAGIC: u32 = 0x454C4946; // "FILE"
pub const INDX_MAGIC: u32 = 0x58444E49; // "INDX"
pub const BAAD_MAGIC: u32 = 0x44414142; // "BAAD"
pub const END_MARKER: u32 = 0xFFFFFFFF;

pub const UPCASE_ENTRIES: usize = 65536;
pub const UPCASE_BYTES: usize = UPCASE_ENTRIES * 2;

/// R4OS volume API policy, including the final file component; not an
/// on-disk NTFS format limit. Builders and verifiers share this boundary.
pub const R4OS_PATH_SEGMENTS_MAX: usize = 24;

// Well-known MFT record numbers.
pub const MFT_RECORD_MFT: u64 = 0;
pub const MFT_RECORD_MFTMIRR: u64 = 1;
pub const MFT_RECORD_LOGFILE: u64 = 2;
pub const MFT_RECORD_VOLUME: u64 = 3;
pub const MFT_RECORD_ATTRDEF: u64 = 4;
pub const MFT_RECORD_ROOT: u64 = 5;
pub const MFT_RECORD_BITMAP: u64 = 6;
pub const MFT_RECORD_BOOT: u64 = 7;
pub const MFT_RECORD_BADCLUS: u64 = 8;
pub const MFT_RECORD_SECURE: u64 = 9;
pub const MFT_RECORD_UPCASE: u64 = 10;
pub const MFT_RECORD_EXTEND: u64 = 11;
pub const MFT_FIRST_NORMAL: u64 = 16;

pub const AttrType = enum(u32) {
    standard_information = 0x10,
    attribute_list = 0x20,
    file_name = 0x30,
    object_id = 0x40,
    security_descriptor = 0x50,
    volume_name = 0x60,
    volume_information = 0x70,
    data = 0x80,
    index_root = 0x90,
    index_allocation = 0xA0,
    bitmap = 0xB0,
    reparse_point = 0xC0,
    ea_information = 0xD0,
    ea = 0xE0,
    logged_utility_stream = 0x100,
    _,
};

// Attribute header flags (0x0C).
pub const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
pub const ATTR_FLAG_ENCRYPTED: u16 = 0x4000;
pub const ATTR_FLAG_SPARSE: u16 = 0x8000;

// FILE record header flags (0x16).
pub const RECORD_IN_USE: u16 = 0x0001;
pub const RECORD_IS_DIRECTORY: u16 = 0x0002;

// $FILE_NAME namespaces.
pub const NAMESPACE_POSIX: u8 = 0;
pub const NAMESPACE_WIN32: u8 = 1;
pub const NAMESPACE_DOS: u8 = 2;
pub const NAMESPACE_WIN32_DOS: u8 = 3;

// $STANDARD_INFORMATION / duplicated $FILE_NAME file attribute flags.
pub const FILE_ATTR_READ_ONLY: u32 = 0x0001;
pub const FILE_ATTR_HIDDEN: u32 = 0x0002;
pub const FILE_ATTR_SYSTEM: u32 = 0x0004;
pub const FILE_ATTR_ARCHIVE: u32 = 0x0020;
pub const FILE_ATTR_SPARSE: u32 = 0x0200;
pub const FILE_ATTR_REPARSE: u32 = 0x0400;
pub const FILE_ATTR_COMPRESSED: u32 = 0x0800;
pub const FILE_ATTR_ENCRYPTED: u32 = 0x4000;
pub const FILE_ATTR_DIRECTORY_DUP: u32 = 0x10000000;

// Index entry flags.
pub const INDEX_ENTRY_NODE: u16 = 0x0001;
pub const INDEX_ENTRY_END: u16 = 0x0002;

// $VOLUME_INFORMATION flags.
pub const VOLUME_FLAG_DIRTY: u16 = 0x0001;

/// 48-bit record number plus 16-bit sequence number.
pub const FileReference = struct {
    record: u64,
    sequence: u16,

    pub fn parse(raw: u64) FileReference {
        var out: CFileReference = undefined;
        _ = run(0, .{ .value = raw, .output = &out });
        return zFileReference(out);
    }

    pub fn pack(self: FileReference) u64 {
        const value = cFileReference(self);
        return run(1, .{ .input = &value }).value;
    }
};

// ---------------------------------------------------------------------------
// Boot sector
// ---------------------------------------------------------------------------

pub const BootSector = struct {
    bytes_per_sector: u16,
    sectors_per_cluster_raw: u8,
    cluster_bytes: u32,
    total_sectors: u64,
    mft_lcn: u64,
    mftmirr_lcn: u64,
    file_record_bytes: u32,
    index_block_bytes: u32,
    serial_number: u64,

    pub const ParseError = enum {
        ok,
        too_short,
        bad_oem,
        bad_signature,
        bad_sector_size,
        bad_cluster_size,
        bad_record_size,
        bad_index_size,
        bad_geometry,
    };

    // Size-byte decoding belongs to the native codec.

    pub fn parse(sector: []const u8, out: *BootSector) ParseError {
        var value: CBootSector = undefined;
        const result = run(2, .{ .bytes = view(sector), .output = &value });
        if (result.present) out.* = zBootSector(value);
        return @enumFromInt(result.code);
    }

    // Native codec also retains the original large-cluster extension.

};

// ---------------------------------------------------------------------------
// Update sequence array (fixups)
// ---------------------------------------------------------------------------

pub const FixupError = enum {
    ok,
    too_short,
    bad_offsets,
    usn_mismatch,
};

/// Verifies and removes the update sequence fixups of a FILE/INDX record in
/// place.  The buffer must hold the complete record.  A usn_mismatch means a
/// torn write or corruption; the buffer content is undefined afterwards.
pub fn applyFixups(record: []u8) FixupError {
    return @enumFromInt(run(3, .{ .buffer = buffer(record) }).code);
}

/// Installs fresh fixups before writing a record: saves the last word of
/// every sector into the array and stamps the (incremented) USN.  The caller
/// passes the record with final content and the previous USN value.
pub fn installFixups(record: []u8, previous_usn: u16) FixupError {
    return @enumFromInt(run(4, .{ .buffer = buffer(record), .value = previous_usn }).code);
}

// ---------------------------------------------------------------------------
// FILE records and attributes
// ---------------------------------------------------------------------------

pub const FileRecordHeader = struct {
    magic: u32,
    lsn: u64,
    sequence: u16,
    link_count: u16,
    attrs_offset: u16,
    flags: u16,
    bytes_in_use: u32,
    bytes_allocated: u32,
    base_record: FileReference,
    next_attr_instance: u16,
    record_number: u32,

    pub fn parse(record: []const u8) ?FileRecordHeader {
        var out: CFileRecordHeader = undefined;
        if (!run(5, .{ .bytes = view(record), .output = &out }).present) return null;
        return zFileRecordHeader(out);
    }

    pub fn inUse(self: FileRecordHeader) bool {
        return r4os_rust_ntfs_format_flag(self.flags, RECORD_IN_USE);
    }

    pub fn isDirectory(self: FileRecordHeader) bool {
        return r4os_rust_ntfs_format_flag(self.flags, RECORD_IS_DIRECTORY);
    }
};

pub const Attribute = struct {
    attr_type: u32,
    length: u32,
    non_resident: bool,
    name: []const u8, // UTF-16LE raw bytes, empty for the unnamed attribute
    flags: u16,
    instance: u16,
    // Resident:
    value: []const u8,
    // Non-resident:
    lowest_vcn: u64,
    highest_vcn: u64,
    mapping_pairs: []const u8,
    compression_unit: u8,
    allocated_size: u64,
    data_size: u64,
    initialized_size: u64,

    pub fn typed(self: Attribute) AttrType {
        return @enumFromInt(self.attr_type);
    }

    pub fn isCompressed(self: Attribute) bool {
        return r4os_rust_ntfs_format_flag(self.flags, ATTR_FLAG_COMPRESSED);
    }

    pub fn isSparse(self: Attribute) bool {
        return r4os_rust_ntfs_format_flag(self.flags, ATTR_FLAG_SPARSE);
    }

    pub fn isEncrypted(self: Attribute) bool {
        return r4os_rust_ntfs_format_flag(self.flags, ATTR_FLAG_ENCRYPTED);
    }
};

pub const AttributeIterator = struct {
    record: []const u8,
    offset: usize,

    pub fn init(record: []const u8, header: FileRecordHeader) AttributeIterator {
        return .{ .record = record, .offset = header.attrs_offset };
    }

    /// Yields the next attribute or null at the end marker.  A malformed
    /// attribute terminates the iteration (defensive stop, never a crash).
    pub fn next(self: *AttributeIterator) ?Attribute {
        var state: CIteratorState = .{ .offset = self.offset };
        var out: CAttribute = undefined;
        const result = run(6, .{ .bytes = view(self.record), .state = &state, .output = &out });
        self.offset = state.offset;
        return if (result.present) zAttribute(out) else null;
    }
};

/// Finds the first attribute of a type with an exactly matching name
/// (case-sensitive UTF-16 bytes; unnamed = empty name).
pub fn findAttribute(record: []const u8, header: FileRecordHeader, attr_type: AttrType, name_utf16: []const u8) ?Attribute {
    const input = cFileRecordHeader(header);
    var out: CAttribute = undefined;
    return if (run(7, .{ .bytes = view(record), .input = &input, .value = @intFromEnum(attr_type), .second = view(name_utf16), .output = &out }).present) zAttribute(out) else null;
}

// ---------------------------------------------------------------------------
// Runlists
// ---------------------------------------------------------------------------

pub const Run = struct {
    length_clusters: u64,
    /// Absolute LCN of the run start, or null for a sparse run.
    lcn: ?u64,
};

pub const RunlistIterator = struct {
    mapping: []const u8,
    offset: usize,
    current_lcn: i64,
    failed: bool,

    pub fn init(mapping: []const u8) RunlistIterator {
        return .{ .mapping = mapping, .offset = 0, .current_lcn = 0, .failed = false };
    }

    pub fn next(self: *RunlistIterator) ?Run {
        var state: CIteratorState = .{ .offset = self.offset, .current_lcn = self.current_lcn, .failed = self.failed };
        var out: CRun = undefined;
        const result = run(8, .{ .bytes = view(self.mapping), .state = &state, .output = &out });
        self.offset = state.offset;
        self.current_lcn = state.current_lcn;
        self.failed = state.failed;
        return if (result.present) zRun(out) else null;
    }

    pub fn hadError(self: RunlistIterator) bool {
        return self.failed;
    }
};

/// Encodes one mapping pair.  `lcn_delta` null encodes a sparse run.
/// Returns the encoded length or null when the buffer is too small.
pub fn encodeRun(out: []u8, length_clusters: u64, lcn_delta: ?i64) ?usize {
    return optional(run(9, .{ .buffer = buffer(out), .value = length_clusters, .signed = lcn_delta orelse 0, .flag = lcn_delta != null }));
}

// ---------------------------------------------------------------------------
// $STANDARD_INFORMATION and $FILE_NAME
// ---------------------------------------------------------------------------

pub const StandardInformation = struct {
    creation_time: u64,
    data_change_time: u64,
    mft_change_time: u64,
    access_time: u64,
    file_attributes: u32,

    pub fn parse(value: []const u8) ?StandardInformation {
        var out: CStandardInformation = undefined;
        if (!run(10, .{ .bytes = view(value), .output = &out }).present) return null;
        return zStandardInformation(out);
    }
};

pub const FileName = struct {
    parent: FileReference,
    creation_time: u64,
    data_change_time: u64,
    mft_change_time: u64,
    access_time: u64,
    allocated_size: u64,
    data_size: u64,
    flags: u32,
    name_length: u8,
    namespace: u8,
    /// UTF-16LE raw bytes of the name.
    name: []const u8,

    pub fn parse(value: []const u8) ?FileName {
        var out: CFileName = undefined;
        if (!run(11, .{ .bytes = view(value), .output = &out }).present) return null;
        return zFileName(out);
    }
};

// ---------------------------------------------------------------------------
// $ATTRIBUTE_LIST
// ---------------------------------------------------------------------------

pub const AttributeListEntry = struct {
    attr_type: u32,
    entry_length: u16,
    lowest_vcn: u64,
    mft_reference: FileReference,
    instance: u16,
    name: []const u8, // UTF-16LE

    pub fn typed(self: AttributeListEntry) AttrType {
        return @enumFromInt(self.attr_type);
    }
};

pub const AttributeListIterator = struct {
    value: []const u8,
    offset: usize,

    pub fn init(value: []const u8) AttributeListIterator {
        return .{ .value = value, .offset = 0 };
    }

    pub fn next(self: *AttributeListIterator) ?AttributeListEntry {
        var state: CIteratorState = .{ .offset = self.offset };
        var out: CAttributeListEntry = undefined;
        const result = run(12, .{ .bytes = view(self.value), .state = &state, .output = &out });
        self.offset = state.offset;
        return if (result.present) zAttributeListEntry(out) else null;
    }
};

// ---------------------------------------------------------------------------
// Directory indexes ($I30)
// ---------------------------------------------------------------------------

/// "$I30" as UTF-16LE, the index name of filename indexes.
pub const I30_NAME_UTF16: [8]u8 = .{ '$', 0, 'I', 0, '3', 0, '0', 0 };

pub const COLLATION_FILE_NAME: u32 = 1;

pub const IndexRoot = struct {
    indexed_attr_type: u32,
    collation_rule: u32,
    index_block_bytes: u32,
    header: IndexHeader,
    /// Entry bytes of the root node (relative slice).
    entries: []const u8,

    pub fn parse(value: []const u8) ?IndexRoot {
        var out: CIndexRoot = undefined;
        if (!run(14, .{ .bytes = view(value), .output = &out }).present) return null;
        return zIndexRoot(out);
    }
};

pub const IndexHeader = struct {
    entries_offset: u32,
    index_length: u32,
    allocated_size: u32,
    flags: u8,

    pub fn parse(bytes: []const u8) ?IndexHeader {
        var out: CIndexHeader = undefined;
        if (!run(13, .{ .bytes = view(bytes), .output = &out }).present) return null;
        return zIndexHeader(out);
    }

    pub fn hasSubNodes(self: IndexHeader) bool {
        return r4os_rust_ntfs_format_flag(self.flags, 0x01);
    }
};

pub const IndexBlock = struct {
    lsn: u64,
    vcn: u64,
    /// Entry bytes after fixup removal.
    entries: []const u8,

    /// The block buffer must already have fixups applied.
    pub fn parse(block: []const u8) ?IndexBlock {
        var out: CIndexBlock = undefined;
        if (!run(15, .{ .bytes = view(block), .output = &out }).present) return null;
        return zIndexBlock(out);
    }
};

pub const IndexEntry = struct {
    file_reference: u64,
    entry_length: u16,
    key_length: u16,
    flags: u16,
    /// $FILE_NAME value bytes (empty for the END entry).
    key: []const u8,
    /// Sub-node VCN when the NODE flag is set.
    sub_node_vcn: ?u64,

    pub fn isEnd(self: IndexEntry) bool {
        return r4os_rust_ntfs_format_flag(self.flags, INDEX_ENTRY_END);
    }

    pub fn hasSubNode(self: IndexEntry) bool {
        return r4os_rust_ntfs_format_flag(self.flags, INDEX_ENTRY_NODE);
    }

    pub fn fileName(self: IndexEntry) ?FileName {
        return FileName.parse(self.key);
    }
};

pub const IndexEntryIterator = struct {
    entries: []const u8,
    offset: usize,
    done: bool,

    pub fn init(entries: []const u8) IndexEntryIterator {
        return .{ .entries = entries, .offset = 0, .done = false };
    }

    /// Yields every entry including the terminating END entry, then null.
    pub fn next(self: *IndexEntryIterator) ?IndexEntry {
        var state: CIteratorState = .{ .offset = self.offset, .done = self.done };
        var out: CIndexEntry = undefined;
        const result = run(16, .{ .bytes = view(self.entries), .state = &state, .output = &out });
        self.offset = state.offset;
        self.done = state.done;
        return if (result.present) zIndexEntry(out) else null;
    }
};

// ---------------------------------------------------------------------------
// $UpCase collation
// ---------------------------------------------------------------------------

pub fn upcaseUnit(upcase: []const u8, unit: u16) u16 {
    return @intCast(run(17, .{ .bytes = view(upcase), .value = unit }).value);
}

/// COLLATION_FILE_NAME order of two UTF-16LE names using the volume upcase
/// table: unit-wise upcased comparison, shorter prefix first.
pub fn compareFileNames(upcase: []const u8, left_utf16: []const u8, right_utf16: []const u8) std.math.Order {
    return switch (run(18, .{ .bytes = view(upcase), .second = view(left_utf16), .third = view(right_utf16) }).code) {
        -1 => .lt,
        0 => .eq,
        1 => .gt,
        else => unreachable,
    };
}

/// Case-insensitive match of an ASCII name against a UTF-16LE name.
pub fn asciiMatchesUtf16(upcase: []const u8, ascii: []const u8, utf16: []const u8) bool {
    return run(19, .{ .bytes = view(upcase), .second = view(ascii), .third = view(utf16) }).present;
}

/// Copies a UTF-16LE name into an ASCII buffer.  Returns the length or null
/// when a unit is outside 0x20..0x7E (caller decides how to surface that).
pub fn utf16ToAscii(utf16: []const u8, out: []u8) ?usize {
    return optional(run(20, .{ .bytes = view(utf16), .buffer = buffer(out) }));
}

pub fn asciiToUtf16(ascii: []const u8, out: []u8) ?usize {
    return optional(run(21, .{ .bytes = view(ascii), .buffer = buffer(out) }));
}

/// UTF-8 -> UTF-16LE for file names (BMP only; supplementary planes and
/// malformed sequences are visible failures).  Returns the byte length of
/// the UTF-16 output.
pub fn utf8ToUtf16(name: []const u8, out: []u8) ?usize {
    return optional(run(22, .{ .bytes = view(name), .buffer = buffer(out) }));
}

/// UTF-16LE -> UTF-8 for file names (BMP only).  Returns the byte length of
/// the UTF-8 output or null when the buffer is too small or a surrogate
/// appears.
pub fn utf16ToUtf8(name_utf16: []const u8, out: []u8) ?usize {
    return optional(run(23, .{ .bytes = view(name_utf16), .buffer = buffer(out) }));
}

// ---------------------------------------------------------------------------
// LZNT1 decompression (read side)
// ---------------------------------------------------------------------------

/// Decompresses one LZNT1 compression unit (a sequence of 4-KB chunks) from
/// `src` into `dst`.  Returns the number of bytes produced; a header of 0
/// ends the unit early (the remainder reads as zeros at the caller).  Bad
/// chunk signatures and out-of-range back references are visible failures.
pub fn lznt1Decompress(src: []const u8, dst: []u8) ?usize {
    return optional(run(24, .{ .bytes = view(src), .buffer = buffer(dst) }));
}

// ---------------------------------------------------------------------------
// $VOLUME_INFORMATION
// ---------------------------------------------------------------------------

pub const VolumeInformation = struct {
    major: u8,
    minor: u8,
    flags: u16,

    pub fn parse(value: []const u8) ?VolumeInformation {
        var out: CVolumeInformation = undefined;
        if (!run(25, .{ .bytes = view(value), .output = &out }).present) return null;
        return zVolumeInformation(out);
    }
};

// Private native codec projections; no compiler-specific optional/slice layout crosses C.
const CView = extern struct {
    data: ?[*]const u8 = null,
    len: usize = 0,
};
const CBuffer = extern struct {
    data: ?[*]u8 = null,
    len: usize = 0,
};
const CIteratorState = extern struct {
    offset: usize = 0,
    current_lcn: i64 = 0,
    failed: bool = false,
    done: bool = false,
};
const CArgs = extern struct {
    bytes: CView = .{},
    second: CView = .{},
    third: CView = .{},
    buffer: CBuffer = .{},
    input: ?*const anyopaque = null,
    output: ?*anyopaque = null,
    state: ?*CIteratorState = null,
    value: u64 = 0,
    signed: i64 = 0,
    flag: bool = false,
};
const CResult = extern struct { value: u64, code: i32, present: bool };
extern fn r4os_rust_ntfs_format_run(op: u8, args: *const CArgs) CResult;
extern fn r4os_rust_ntfs_format_flag(value: u64, mask: u64) bool;
fn view(value: []const u8) CView {
    return .{ .data = value.ptr, .len = value.len };
}
fn buffer(value: []u8) CBuffer {
    return .{ .data = value.ptr, .len = value.len };
}
fn zview(value: CView) []const u8 {
    return if (value.len == 0) &.{} else value.data.?[0..value.len];
}
fn run(op: u8, args: CArgs) CResult {
    return r4os_rust_ntfs_format_run(op, &args);
}
fn optional(result: CResult) ?usize {
    return if (result.present) @intCast(result.value) else null;
}
const CFileReference = extern struct {
    record: u64,
    sequence: u16,
};
fn cFileReference(v: FileReference) CFileReference {
    return .{
        .record = v.record,
        .sequence = v.sequence,
    };
}
fn zFileReference(v: CFileReference) FileReference {
    return .{
        .record = v.record,
        .sequence = v.sequence,
    };
}
const CBootSector = extern struct {
    bytes_per_sector: u16,
    sectors_per_cluster_raw: u8,
    cluster_bytes: u32,
    total_sectors: u64,
    mft_lcn: u64,
    mftmirr_lcn: u64,
    file_record_bytes: u32,
    index_block_bytes: u32,
    serial_number: u64,
};
fn cBootSector(v: BootSector) CBootSector {
    return .{
        .bytes_per_sector = v.bytes_per_sector,
        .sectors_per_cluster_raw = v.sectors_per_cluster_raw,
        .cluster_bytes = v.cluster_bytes,
        .total_sectors = v.total_sectors,
        .mft_lcn = v.mft_lcn,
        .mftmirr_lcn = v.mftmirr_lcn,
        .file_record_bytes = v.file_record_bytes,
        .index_block_bytes = v.index_block_bytes,
        .serial_number = v.serial_number,
    };
}
fn zBootSector(v: CBootSector) BootSector {
    return .{
        .bytes_per_sector = v.bytes_per_sector,
        .sectors_per_cluster_raw = v.sectors_per_cluster_raw,
        .cluster_bytes = v.cluster_bytes,
        .total_sectors = v.total_sectors,
        .mft_lcn = v.mft_lcn,
        .mftmirr_lcn = v.mftmirr_lcn,
        .file_record_bytes = v.file_record_bytes,
        .index_block_bytes = v.index_block_bytes,
        .serial_number = v.serial_number,
    };
}
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
fn cFileRecordHeader(v: FileRecordHeader) CFileRecordHeader {
    return .{
        .magic = v.magic,
        .lsn = v.lsn,
        .sequence = v.sequence,
        .link_count = v.link_count,
        .attrs_offset = v.attrs_offset,
        .flags = v.flags,
        .bytes_in_use = v.bytes_in_use,
        .bytes_allocated = v.bytes_allocated,
        .base_record = cFileReference(v.base_record),
        .next_attr_instance = v.next_attr_instance,
        .record_number = v.record_number,
    };
}
fn zFileRecordHeader(v: CFileRecordHeader) FileRecordHeader {
    return .{
        .magic = v.magic,
        .lsn = v.lsn,
        .sequence = v.sequence,
        .link_count = v.link_count,
        .attrs_offset = v.attrs_offset,
        .flags = v.flags,
        .bytes_in_use = v.bytes_in_use,
        .bytes_allocated = v.bytes_allocated,
        .base_record = zFileReference(v.base_record),
        .next_attr_instance = v.next_attr_instance,
        .record_number = v.record_number,
    };
}
const CAttribute = extern struct {
    attr_type: u32,
    length: u32,
    non_resident: bool,
    name: CView,
    flags: u16,
    instance: u16,
    value: CView,
    lowest_vcn: u64,
    highest_vcn: u64,
    mapping_pairs: CView,
    compression_unit: u8,
    allocated_size: u64,
    data_size: u64,
    initialized_size: u64,
};
fn cAttribute(v: Attribute) CAttribute {
    return .{
        .attr_type = v.attr_type,
        .length = v.length,
        .non_resident = v.non_resident,
        .name = view(v.name),
        .flags = v.flags,
        .instance = v.instance,
        .value = view(v.value),
        .lowest_vcn = v.lowest_vcn,
        .highest_vcn = v.highest_vcn,
        .mapping_pairs = view(v.mapping_pairs),
        .compression_unit = v.compression_unit,
        .allocated_size = v.allocated_size,
        .data_size = v.data_size,
        .initialized_size = v.initialized_size,
    };
}
fn zAttribute(v: CAttribute) Attribute {
    return .{
        .attr_type = v.attr_type,
        .length = v.length,
        .non_resident = v.non_resident,
        .name = zview(v.name),
        .flags = v.flags,
        .instance = v.instance,
        .value = zview(v.value),
        .lowest_vcn = v.lowest_vcn,
        .highest_vcn = v.highest_vcn,
        .mapping_pairs = zview(v.mapping_pairs),
        .compression_unit = v.compression_unit,
        .allocated_size = v.allocated_size,
        .data_size = v.data_size,
        .initialized_size = v.initialized_size,
    };
}
const CRun = extern struct {
    length_clusters: u64,
    has_lcn: bool,
    lcn: u64,
};
fn cRun(v: Run) CRun {
    return .{
        .length_clusters = v.length_clusters,
        .has_lcn = v.lcn != null,
        .lcn = v.lcn orelse 0,
    };
}
fn zRun(v: CRun) Run {
    return .{
        .length_clusters = v.length_clusters,
        .lcn = if (v.has_lcn) v.lcn else null,
    };
}
const CStandardInformation = extern struct {
    creation_time: u64,
    data_change_time: u64,
    mft_change_time: u64,
    access_time: u64,
    file_attributes: u32,
};
fn cStandardInformation(v: StandardInformation) CStandardInformation {
    return .{
        .creation_time = v.creation_time,
        .data_change_time = v.data_change_time,
        .mft_change_time = v.mft_change_time,
        .access_time = v.access_time,
        .file_attributes = v.file_attributes,
    };
}
fn zStandardInformation(v: CStandardInformation) StandardInformation {
    return .{
        .creation_time = v.creation_time,
        .data_change_time = v.data_change_time,
        .mft_change_time = v.mft_change_time,
        .access_time = v.access_time,
        .file_attributes = v.file_attributes,
    };
}
const CFileName = extern struct {
    parent: CFileReference,
    creation_time: u64,
    data_change_time: u64,
    mft_change_time: u64,
    access_time: u64,
    allocated_size: u64,
    data_size: u64,
    flags: u32,
    name_length: u8,
    namespace: u8,
    name: CView,
};
fn cFileName(v: FileName) CFileName {
    return .{
        .parent = cFileReference(v.parent),
        .creation_time = v.creation_time,
        .data_change_time = v.data_change_time,
        .mft_change_time = v.mft_change_time,
        .access_time = v.access_time,
        .allocated_size = v.allocated_size,
        .data_size = v.data_size,
        .flags = v.flags,
        .name_length = v.name_length,
        .namespace = v.namespace,
        .name = view(v.name),
    };
}
fn zFileName(v: CFileName) FileName {
    return .{
        .parent = zFileReference(v.parent),
        .creation_time = v.creation_time,
        .data_change_time = v.data_change_time,
        .mft_change_time = v.mft_change_time,
        .access_time = v.access_time,
        .allocated_size = v.allocated_size,
        .data_size = v.data_size,
        .flags = v.flags,
        .name_length = v.name_length,
        .namespace = v.namespace,
        .name = zview(v.name),
    };
}
const CAttributeListEntry = extern struct {
    attr_type: u32,
    entry_length: u16,
    lowest_vcn: u64,
    mft_reference: CFileReference,
    instance: u16,
    name: CView,
};
fn cAttributeListEntry(v: AttributeListEntry) CAttributeListEntry {
    return .{
        .attr_type = v.attr_type,
        .entry_length = v.entry_length,
        .lowest_vcn = v.lowest_vcn,
        .mft_reference = cFileReference(v.mft_reference),
        .instance = v.instance,
        .name = view(v.name),
    };
}
fn zAttributeListEntry(v: CAttributeListEntry) AttributeListEntry {
    return .{
        .attr_type = v.attr_type,
        .entry_length = v.entry_length,
        .lowest_vcn = v.lowest_vcn,
        .mft_reference = zFileReference(v.mft_reference),
        .instance = v.instance,
        .name = zview(v.name),
    };
}
const CIndexRoot = extern struct {
    indexed_attr_type: u32,
    collation_rule: u32,
    index_block_bytes: u32,
    header: CIndexHeader,
    entries: CView,
};
fn cIndexRoot(v: IndexRoot) CIndexRoot {
    return .{
        .indexed_attr_type = v.indexed_attr_type,
        .collation_rule = v.collation_rule,
        .index_block_bytes = v.index_block_bytes,
        .header = cIndexHeader(v.header),
        .entries = view(v.entries),
    };
}
fn zIndexRoot(v: CIndexRoot) IndexRoot {
    return .{
        .indexed_attr_type = v.indexed_attr_type,
        .collation_rule = v.collation_rule,
        .index_block_bytes = v.index_block_bytes,
        .header = zIndexHeader(v.header),
        .entries = zview(v.entries),
    };
}
const CIndexHeader = extern struct {
    entries_offset: u32,
    index_length: u32,
    allocated_size: u32,
    flags: u8,
};
fn cIndexHeader(v: IndexHeader) CIndexHeader {
    return .{
        .entries_offset = v.entries_offset,
        .index_length = v.index_length,
        .allocated_size = v.allocated_size,
        .flags = v.flags,
    };
}
fn zIndexHeader(v: CIndexHeader) IndexHeader {
    return .{
        .entries_offset = v.entries_offset,
        .index_length = v.index_length,
        .allocated_size = v.allocated_size,
        .flags = v.flags,
    };
}
const CIndexBlock = extern struct {
    lsn: u64,
    vcn: u64,
    entries: CView,
};
fn cIndexBlock(v: IndexBlock) CIndexBlock {
    return .{
        .lsn = v.lsn,
        .vcn = v.vcn,
        .entries = view(v.entries),
    };
}
fn zIndexBlock(v: CIndexBlock) IndexBlock {
    return .{
        .lsn = v.lsn,
        .vcn = v.vcn,
        .entries = zview(v.entries),
    };
}
const CIndexEntry = extern struct {
    file_reference: u64,
    entry_length: u16,
    key_length: u16,
    flags: u16,
    key: CView,
    has_sub_node_vcn: bool,
    sub_node_vcn: u64,
};
fn cIndexEntry(v: IndexEntry) CIndexEntry {
    return .{
        .file_reference = v.file_reference,
        .entry_length = v.entry_length,
        .key_length = v.key_length,
        .flags = v.flags,
        .key = view(v.key),
        .has_sub_node_vcn = v.sub_node_vcn != null,
        .sub_node_vcn = v.sub_node_vcn orelse 0,
    };
}
fn zIndexEntry(v: CIndexEntry) IndexEntry {
    return .{
        .file_reference = v.file_reference,
        .entry_length = v.entry_length,
        .key_length = v.key_length,
        .flags = v.flags,
        .key = zview(v.key),
        .sub_node_vcn = if (v.has_sub_node_vcn) v.sub_node_vcn else null,
    };
}
const CVolumeInformation = extern struct {
    major: u8,
    minor: u8,
    flags: u16,
};
fn cVolumeInformation(v: VolumeInformation) CVolumeInformation {
    return .{
        .major = v.major,
        .minor = v.minor,
        .flags = v.flags,
    };
}
fn zVolumeInformation(v: CVolumeInformation) VolumeInformation {
    return .{
        .major = v.major,
        .minor = v.minor,
        .flags = v.flags,
    };
}

comptime {
    if (@sizeOf(CView) != 16 or @alignOf(CView) != 8) @compileError("NTFS private View shape");
    if (@offsetOf(CView, "data") != 0) @compileError("NTFS private View.data offset");
    if (@offsetOf(CView, "len") != 8) @compileError("NTFS private View.len offset");
    if (@sizeOf(CBuffer) != 16 or @alignOf(CBuffer) != 8) @compileError("NTFS private Buffer shape");
    if (@offsetOf(CBuffer, "data") != 0) @compileError("NTFS private Buffer.data offset");
    if (@offsetOf(CBuffer, "len") != 8) @compileError("NTFS private Buffer.len offset");
    if (@sizeOf(CFileReference) != 16 or @alignOf(CFileReference) != 8) @compileError("NTFS private FileReference shape");
    if (@offsetOf(CFileReference, "record") != 0) @compileError("NTFS private FileReference.record offset");
    if (@offsetOf(CFileReference, "sequence") != 8) @compileError("NTFS private FileReference.sequence offset");
    if (@sizeOf(CBootSector) != 48 or @alignOf(CBootSector) != 8) @compileError("NTFS private BootSector shape");
    if (@offsetOf(CBootSector, "bytes_per_sector") != 0) @compileError("NTFS private BootSector.bytes_per_sector offset");
    if (@offsetOf(CBootSector, "sectors_per_cluster_raw") != 2) @compileError("NTFS private BootSector.sectors_per_cluster_raw offset");
    if (@offsetOf(CBootSector, "cluster_bytes") != 4) @compileError("NTFS private BootSector.cluster_bytes offset");
    if (@offsetOf(CBootSector, "total_sectors") != 8) @compileError("NTFS private BootSector.total_sectors offset");
    if (@offsetOf(CBootSector, "mft_lcn") != 16) @compileError("NTFS private BootSector.mft_lcn offset");
    if (@offsetOf(CBootSector, "mftmirr_lcn") != 24) @compileError("NTFS private BootSector.mftmirr_lcn offset");
    if (@offsetOf(CBootSector, "file_record_bytes") != 32) @compileError("NTFS private BootSector.file_record_bytes offset");
    if (@offsetOf(CBootSector, "index_block_bytes") != 36) @compileError("NTFS private BootSector.index_block_bytes offset");
    if (@offsetOf(CBootSector, "serial_number") != 40) @compileError("NTFS private BootSector.serial_number offset");
    if (@sizeOf(CFileRecordHeader) != 56 or @alignOf(CFileRecordHeader) != 8) @compileError("NTFS private FileRecordHeader shape");
    if (@offsetOf(CFileRecordHeader, "magic") != 0) @compileError("NTFS private FileRecordHeader.magic offset");
    if (@offsetOf(CFileRecordHeader, "lsn") != 8) @compileError("NTFS private FileRecordHeader.lsn offset");
    if (@offsetOf(CFileRecordHeader, "sequence") != 16) @compileError("NTFS private FileRecordHeader.sequence offset");
    if (@offsetOf(CFileRecordHeader, "link_count") != 18) @compileError("NTFS private FileRecordHeader.link_count offset");
    if (@offsetOf(CFileRecordHeader, "attrs_offset") != 20) @compileError("NTFS private FileRecordHeader.attrs_offset offset");
    if (@offsetOf(CFileRecordHeader, "flags") != 22) @compileError("NTFS private FileRecordHeader.flags offset");
    if (@offsetOf(CFileRecordHeader, "bytes_in_use") != 24) @compileError("NTFS private FileRecordHeader.bytes_in_use offset");
    if (@offsetOf(CFileRecordHeader, "bytes_allocated") != 28) @compileError("NTFS private FileRecordHeader.bytes_allocated offset");
    if (@offsetOf(CFileRecordHeader, "base_record") != 32) @compileError("NTFS private FileRecordHeader.base_record offset");
    if (@offsetOf(CFileRecordHeader, "next_attr_instance") != 48) @compileError("NTFS private FileRecordHeader.next_attr_instance offset");
    if (@offsetOf(CFileRecordHeader, "record_number") != 52) @compileError("NTFS private FileRecordHeader.record_number offset");
    if (@sizeOf(CAttribute) != 120 or @alignOf(CAttribute) != 8) @compileError("NTFS private Attribute shape");
    if (@offsetOf(CAttribute, "attr_type") != 0) @compileError("NTFS private Attribute.attr_type offset");
    if (@offsetOf(CAttribute, "length") != 4) @compileError("NTFS private Attribute.length offset");
    if (@offsetOf(CAttribute, "non_resident") != 8) @compileError("NTFS private Attribute.non_resident offset");
    if (@offsetOf(CAttribute, "name") != 16) @compileError("NTFS private Attribute.name offset");
    if (@offsetOf(CAttribute, "flags") != 32) @compileError("NTFS private Attribute.flags offset");
    if (@offsetOf(CAttribute, "instance") != 34) @compileError("NTFS private Attribute.instance offset");
    if (@offsetOf(CAttribute, "value") != 40) @compileError("NTFS private Attribute.value offset");
    if (@offsetOf(CAttribute, "lowest_vcn") != 56) @compileError("NTFS private Attribute.lowest_vcn offset");
    if (@offsetOf(CAttribute, "highest_vcn") != 64) @compileError("NTFS private Attribute.highest_vcn offset");
    if (@offsetOf(CAttribute, "mapping_pairs") != 72) @compileError("NTFS private Attribute.mapping_pairs offset");
    if (@offsetOf(CAttribute, "compression_unit") != 88) @compileError("NTFS private Attribute.compression_unit offset");
    if (@offsetOf(CAttribute, "allocated_size") != 96) @compileError("NTFS private Attribute.allocated_size offset");
    if (@offsetOf(CAttribute, "data_size") != 104) @compileError("NTFS private Attribute.data_size offset");
    if (@offsetOf(CAttribute, "initialized_size") != 112) @compileError("NTFS private Attribute.initialized_size offset");
    if (@sizeOf(CRun) != 24 or @alignOf(CRun) != 8) @compileError("NTFS private Run shape");
    if (@offsetOf(CRun, "length_clusters") != 0) @compileError("NTFS private Run.length_clusters offset");
    if (@offsetOf(CRun, "has_lcn") != 8) @compileError("NTFS private Run.has_lcn offset");
    if (@offsetOf(CRun, "lcn") != 16) @compileError("NTFS private Run.lcn offset");
    if (@sizeOf(CStandardInformation) != 40 or @alignOf(CStandardInformation) != 8) @compileError("NTFS private StandardInformation shape");
    if (@offsetOf(CStandardInformation, "creation_time") != 0) @compileError("NTFS private StandardInformation.creation_time offset");
    if (@offsetOf(CStandardInformation, "data_change_time") != 8) @compileError("NTFS private StandardInformation.data_change_time offset");
    if (@offsetOf(CStandardInformation, "mft_change_time") != 16) @compileError("NTFS private StandardInformation.mft_change_time offset");
    if (@offsetOf(CStandardInformation, "access_time") != 24) @compileError("NTFS private StandardInformation.access_time offset");
    if (@offsetOf(CStandardInformation, "file_attributes") != 32) @compileError("NTFS private StandardInformation.file_attributes offset");
    if (@sizeOf(CFileName) != 88 or @alignOf(CFileName) != 8) @compileError("NTFS private FileName shape");
    if (@offsetOf(CFileName, "parent") != 0) @compileError("NTFS private FileName.parent offset");
    if (@offsetOf(CFileName, "creation_time") != 16) @compileError("NTFS private FileName.creation_time offset");
    if (@offsetOf(CFileName, "data_change_time") != 24) @compileError("NTFS private FileName.data_change_time offset");
    if (@offsetOf(CFileName, "mft_change_time") != 32) @compileError("NTFS private FileName.mft_change_time offset");
    if (@offsetOf(CFileName, "access_time") != 40) @compileError("NTFS private FileName.access_time offset");
    if (@offsetOf(CFileName, "allocated_size") != 48) @compileError("NTFS private FileName.allocated_size offset");
    if (@offsetOf(CFileName, "data_size") != 56) @compileError("NTFS private FileName.data_size offset");
    if (@offsetOf(CFileName, "flags") != 64) @compileError("NTFS private FileName.flags offset");
    if (@offsetOf(CFileName, "name_length") != 68) @compileError("NTFS private FileName.name_length offset");
    if (@offsetOf(CFileName, "namespace") != 69) @compileError("NTFS private FileName.namespace offset");
    if (@offsetOf(CFileName, "name") != 72) @compileError("NTFS private FileName.name offset");
    if (@sizeOf(CAttributeListEntry) != 56 or @alignOf(CAttributeListEntry) != 8) @compileError("NTFS private AttributeListEntry shape");
    if (@offsetOf(CAttributeListEntry, "attr_type") != 0) @compileError("NTFS private AttributeListEntry.attr_type offset");
    if (@offsetOf(CAttributeListEntry, "entry_length") != 4) @compileError("NTFS private AttributeListEntry.entry_length offset");
    if (@offsetOf(CAttributeListEntry, "lowest_vcn") != 8) @compileError("NTFS private AttributeListEntry.lowest_vcn offset");
    if (@offsetOf(CAttributeListEntry, "mft_reference") != 16) @compileError("NTFS private AttributeListEntry.mft_reference offset");
    if (@offsetOf(CAttributeListEntry, "instance") != 32) @compileError("NTFS private AttributeListEntry.instance offset");
    if (@offsetOf(CAttributeListEntry, "name") != 40) @compileError("NTFS private AttributeListEntry.name offset");
    if (@sizeOf(CIndexRoot) != 48 or @alignOf(CIndexRoot) != 8) @compileError("NTFS private IndexRoot shape");
    if (@offsetOf(CIndexRoot, "indexed_attr_type") != 0) @compileError("NTFS private IndexRoot.indexed_attr_type offset");
    if (@offsetOf(CIndexRoot, "collation_rule") != 4) @compileError("NTFS private IndexRoot.collation_rule offset");
    if (@offsetOf(CIndexRoot, "index_block_bytes") != 8) @compileError("NTFS private IndexRoot.index_block_bytes offset");
    if (@offsetOf(CIndexRoot, "header") != 12) @compileError("NTFS private IndexRoot.header offset");
    if (@offsetOf(CIndexRoot, "entries") != 32) @compileError("NTFS private IndexRoot.entries offset");
    if (@sizeOf(CIndexHeader) != 16 or @alignOf(CIndexHeader) != 4) @compileError("NTFS private IndexHeader shape");
    if (@offsetOf(CIndexHeader, "entries_offset") != 0) @compileError("NTFS private IndexHeader.entries_offset offset");
    if (@offsetOf(CIndexHeader, "index_length") != 4) @compileError("NTFS private IndexHeader.index_length offset");
    if (@offsetOf(CIndexHeader, "allocated_size") != 8) @compileError("NTFS private IndexHeader.allocated_size offset");
    if (@offsetOf(CIndexHeader, "flags") != 12) @compileError("NTFS private IndexHeader.flags offset");
    if (@sizeOf(CIndexBlock) != 32 or @alignOf(CIndexBlock) != 8) @compileError("NTFS private IndexBlock shape");
    if (@offsetOf(CIndexBlock, "lsn") != 0) @compileError("NTFS private IndexBlock.lsn offset");
    if (@offsetOf(CIndexBlock, "vcn") != 8) @compileError("NTFS private IndexBlock.vcn offset");
    if (@offsetOf(CIndexBlock, "entries") != 16) @compileError("NTFS private IndexBlock.entries offset");
    if (@sizeOf(CIndexEntry) != 48 or @alignOf(CIndexEntry) != 8) @compileError("NTFS private IndexEntry shape");
    if (@offsetOf(CIndexEntry, "file_reference") != 0) @compileError("NTFS private IndexEntry.file_reference offset");
    if (@offsetOf(CIndexEntry, "entry_length") != 8) @compileError("NTFS private IndexEntry.entry_length offset");
    if (@offsetOf(CIndexEntry, "key_length") != 10) @compileError("NTFS private IndexEntry.key_length offset");
    if (@offsetOf(CIndexEntry, "flags") != 12) @compileError("NTFS private IndexEntry.flags offset");
    if (@offsetOf(CIndexEntry, "key") != 16) @compileError("NTFS private IndexEntry.key offset");
    if (@offsetOf(CIndexEntry, "has_sub_node_vcn") != 32) @compileError("NTFS private IndexEntry.has_sub_node_vcn offset");
    if (@offsetOf(CIndexEntry, "sub_node_vcn") != 40) @compileError("NTFS private IndexEntry.sub_node_vcn offset");
    if (@sizeOf(CVolumeInformation) != 4 or @alignOf(CVolumeInformation) != 2) @compileError("NTFS private VolumeInformation shape");
    if (@offsetOf(CVolumeInformation, "major") != 0) @compileError("NTFS private VolumeInformation.major offset");
    if (@offsetOf(CVolumeInformation, "minor") != 1) @compileError("NTFS private VolumeInformation.minor offset");
    if (@offsetOf(CVolumeInformation, "flags") != 2) @compileError("NTFS private VolumeInformation.flags offset");
    if (@sizeOf(CIteratorState) != 24 or @alignOf(CIteratorState) != 8) @compileError("NTFS private IteratorState shape");
    if (@offsetOf(CIteratorState, "offset") != 0) @compileError("NTFS private IteratorState.offset offset");
    if (@offsetOf(CIteratorState, "current_lcn") != 8) @compileError("NTFS private IteratorState.current_lcn offset");
    if (@offsetOf(CIteratorState, "failed") != 16) @compileError("NTFS private IteratorState.failed offset");
    if (@offsetOf(CIteratorState, "done") != 17) @compileError("NTFS private IteratorState.done offset");
    if (@sizeOf(CArgs) != 112 or @alignOf(CArgs) != 8) @compileError("NTFS private Args shape");
    if (@offsetOf(CArgs, "bytes") != 0) @compileError("NTFS private Args.bytes offset");
    if (@offsetOf(CArgs, "second") != 16) @compileError("NTFS private Args.second offset");
    if (@offsetOf(CArgs, "third") != 32) @compileError("NTFS private Args.third offset");
    if (@offsetOf(CArgs, "buffer") != 48) @compileError("NTFS private Args.buffer offset");
    if (@offsetOf(CArgs, "input") != 64) @compileError("NTFS private Args.input offset");
    if (@offsetOf(CArgs, "output") != 72) @compileError("NTFS private Args.output offset");
    if (@offsetOf(CArgs, "state") != 80) @compileError("NTFS private Args.state offset");
    if (@offsetOf(CArgs, "value") != 88) @compileError("NTFS private Args.value offset");
    if (@offsetOf(CArgs, "signed") != 96) @compileError("NTFS private Args.signed offset");
    if (@offsetOf(CArgs, "flag") != 104) @compileError("NTFS private Args.flag offset");
    if (@sizeOf(CResult) != 16 or @alignOf(CResult) != 8) @compileError("NTFS private Result shape");
    if (@offsetOf(CResult, "value") != 0) @compileError("NTFS private Result.value offset");
    if (@offsetOf(CResult, "code") != 8) @compileError("NTFS private Result.code offset");
    if (@offsetOf(CResult, "present") != 12) @compileError("NTFS private Result.present offset");
}
