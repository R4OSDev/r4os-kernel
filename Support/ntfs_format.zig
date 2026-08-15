// NTFS 3.1 on-disk structure core.
//
// Single shared truth for every NTFS consumer: the kernel driver, the
// ImageCreator formatter and the host-side verification tools all parse and
// build the same byte layout through this file (same model as
// module_manifest.zig for R4MF).
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

fn le16(bytes: []const u8, offset: usize) u16 {
    return std.mem.readInt(u16, bytes[offset..][0..2], .little);
}

fn le32(bytes: []const u8, offset: usize) u32 {
    return std.mem.readInt(u32, bytes[offset..][0..4], .little);
}

fn le64(bytes: []const u8, offset: usize) u64 {
    return std.mem.readInt(u64, bytes[offset..][0..8], .little);
}

/// 48-bit record number plus 16-bit sequence number.
pub const FileReference = struct {
    record: u64,
    sequence: u16,

    pub fn parse(raw: u64) FileReference {
        return .{ .record = raw & 0x0000_FFFF_FFFF_FFFF, .sequence = @intCast(raw >> 48) };
    }

    pub fn pack(self: FileReference) u64 {
        return (self.record & 0x0000_FFFF_FFFF_FFFF) | (@as(u64, self.sequence) << 48);
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

    /// Decodes the size byte at 0x40/0x44: negative two's complement values
    /// mean 2^|v| bytes, positive values are cluster counts.
    fn decodeSizeByte(raw: u8, cluster_bytes: u32) ?u32 {
        const signed: i8 = @bitCast(raw);
        if (signed < 0) {
            const magnitude: u8 = @intCast(-@as(i16, signed));
            if (magnitude > 30) return null;
            return @as(u32, 1) << @intCast(magnitude);
        }
        if (signed == 0) return null;
        const clusters: u32 = @intCast(signed);
        if (cluster_bytes == 0 or clusters > (1 << 20) / cluster_bytes + 1) {
            if (cluster_bytes == 0) return null;
        }
        return clusters * cluster_bytes;
    }

    pub fn parse(sector: []const u8, out: *BootSector) ParseError {
        if (sector.len < 512) return .too_short;
        if (!std.mem.eql(u8, sector[3..11], "NTFS    ")) return .bad_oem;
        if (sector[510] != 0x55 or sector[511] != 0xAA) return .bad_signature;

        const bytes_per_sector = le16(sector, 0x0B);
        if (bytes_per_sector < 256 or bytes_per_sector > 4096) return .bad_sector_size;
        if (!std.math.isPowerOfTwo(bytes_per_sector)) return .bad_sector_size;

        const spc_raw = sector[0x0D];
        const sectors_per_cluster = decodeSectorsPerCluster(spc_raw) orelse return .bad_cluster_size;
        const cluster_bytes = @as(u32, bytes_per_sector) * sectors_per_cluster;
        if (cluster_bytes == 0 or cluster_bytes > 2 * 1024 * 1024) return .bad_cluster_size;

        const total_sectors = le64(sector, 0x28);
        const mft_lcn = le64(sector, 0x30);
        const mftmirr_lcn = le64(sector, 0x38);
        if (total_sectors == 0 or mft_lcn == 0) return .bad_geometry;

        const file_record_bytes = decodeSizeByte(sector[0x40], cluster_bytes) orelse return .bad_record_size;
        if (file_record_bytes < 256 or file_record_bytes > 4096 or !std.math.isPowerOfTwo(file_record_bytes)) return .bad_record_size;
        const index_block_bytes = decodeSizeByte(sector[0x44], cluster_bytes) orelse return .bad_index_size;
        if (index_block_bytes < 512 or index_block_bytes > 65536 or !std.math.isPowerOfTwo(index_block_bytes)) return .bad_index_size;

        out.* = .{
            .bytes_per_sector = bytes_per_sector,
            .sectors_per_cluster_raw = spc_raw,
            .cluster_bytes = cluster_bytes,
            .total_sectors = total_sectors,
            .mft_lcn = mft_lcn,
            .mftmirr_lcn = mftmirr_lcn,
            .file_record_bytes = file_record_bytes,
            .index_block_bytes = index_block_bytes,
            .serial_number = le64(sector, 0x48),
        };
        return .ok;
    }

    /// Sectors per cluster; values > 0x80 encode 2^(value & 0x0F) clusters
    /// (large-cluster extension since Windows 10 1809).
    fn decodeSectorsPerCluster(raw: u8) ?u32 {
        if (raw == 0) return null;
        if (raw <= 0x80) {
            if (!std.math.isPowerOfTwo(raw)) return null;
            return raw;
        }
        const shift: u5 = @intCast(raw & 0x0F);
        if (shift > 12) return null;
        return @as(u32, 1) << shift;
    }
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
    if (record.len < 0x0A) return .too_short;
    const usa_ofs = le16(record, 0x04);
    const usa_count = le16(record, 0x06);
    if (usa_count < 2) return .bad_offsets;
    const sectors = @as(usize, usa_count) - 1;
    if (record.len < sectors * SECTOR_SIZE) return .too_short;
    if (@as(usize, usa_ofs) + @as(usize, usa_count) * 2 > record.len) return .bad_offsets;

    const usn = le16(record, usa_ofs);
    var sector_index: usize = 0;
    while (sector_index < sectors) : (sector_index += 1) {
        const end_offset = (sector_index + 1) * SECTOR_SIZE - 2;
        if (le16(record, end_offset) != usn) return .usn_mismatch;
        const original = le16(record, usa_ofs + 2 + sector_index * 2);
        std.mem.writeInt(u16, record[end_offset..][0..2], original, .little);
    }
    return .ok;
}

/// Installs fresh fixups before writing a record: saves the last word of
/// every sector into the array and stamps the (incremented) USN.  The caller
/// passes the record with final content and the previous USN value.
pub fn installFixups(record: []u8, previous_usn: u16) FixupError {
    if (record.len < 0x0A) return .too_short;
    const usa_ofs = le16(record, 0x04);
    const usa_count = le16(record, 0x06);
    if (usa_count < 2) return .bad_offsets;
    const sectors = @as(usize, usa_count) - 1;
    if (record.len < sectors * SECTOR_SIZE) return .too_short;
    if (@as(usize, usa_ofs) + @as(usize, usa_count) * 2 > record.len) return .bad_offsets;

    var usn = previous_usn +% 1;
    if (usn == 0 or usn == 0xFFFF) usn = 1;
    std.mem.writeInt(u16, record[usa_ofs..][0..2], usn, .little);

    var sector_index: usize = 0;
    while (sector_index < sectors) : (sector_index += 1) {
        const end_offset = (sector_index + 1) * SECTOR_SIZE - 2;
        const original = le16(record, end_offset);
        std.mem.writeInt(u16, record[usa_ofs + 2 + sector_index * 2 ..][0..2], original, .little);
        std.mem.writeInt(u16, record[end_offset..][0..2], usn, .little);
    }
    return .ok;
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
        if (record.len < 0x30) return null;
        const magic = le32(record, 0x00);
        if (magic != FILE_MAGIC) return null;
        const header = FileRecordHeader{
            .magic = magic,
            .lsn = le64(record, 0x08),
            .sequence = le16(record, 0x10),
            .link_count = le16(record, 0x12),
            .attrs_offset = le16(record, 0x14),
            .flags = le16(record, 0x16),
            .bytes_in_use = le32(record, 0x18),
            .bytes_allocated = le32(record, 0x1C),
            .base_record = FileReference.parse(le64(record, 0x20)),
            .next_attr_instance = le16(record, 0x28),
            .record_number = le32(record, 0x2C),
        };
        if (header.attrs_offset < 0x30 or header.attrs_offset >= record.len) return null;
        if (header.bytes_in_use < header.attrs_offset or header.bytes_in_use > record.len) return null;
        return header;
    }

    pub fn inUse(self: FileRecordHeader) bool {
        return (self.flags & RECORD_IN_USE) != 0;
    }

    pub fn isDirectory(self: FileRecordHeader) bool {
        return (self.flags & RECORD_IS_DIRECTORY) != 0;
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
        return (self.flags & ATTR_FLAG_COMPRESSED) != 0;
    }

    pub fn isSparse(self: Attribute) bool {
        return (self.flags & ATTR_FLAG_SPARSE) != 0;
    }

    pub fn isEncrypted(self: Attribute) bool {
        return (self.flags & ATTR_FLAG_ENCRYPTED) != 0;
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
        const record = self.record;
        if (self.offset + 8 > record.len) return null;
        const attr_type = le32(record, self.offset);
        if (attr_type == END_MARKER) return null;
        const length = le32(record, self.offset + 4);
        if (length < 0x18 or length % 8 != 0) return null;
        if (self.offset + length > record.len) return null;
        const base = self.offset;
        self.offset += length;

        const non_resident = record[base + 0x08] != 0;
        const name_length = record[base + 0x09];
        const name_offset = le16(record, base + 0x0A);
        var name: []const u8 = &[_]u8{};
        if (name_length > 0) {
            const name_end = @as(usize, name_offset) + @as(usize, name_length) * 2;
            if (name_end > length) return null;
            name = record[base + name_offset .. base + name_end];
        }

        var attribute = Attribute{
            .attr_type = attr_type,
            .length = length,
            .non_resident = non_resident,
            .name = name,
            .flags = le16(record, base + 0x0C),
            .instance = le16(record, base + 0x0E),
            .value = &[_]u8{},
            .lowest_vcn = 0,
            .highest_vcn = 0,
            .mapping_pairs = &[_]u8{},
            .compression_unit = 0,
            .allocated_size = 0,
            .data_size = 0,
            .initialized_size = 0,
        };

        if (!non_resident) {
            if (base + 0x18 > record.len) return null;
            const value_length = le32(record, base + 0x10);
            const value_offset = le16(record, base + 0x14);
            const value_end = @as(usize, value_offset) + @as(usize, value_length);
            if (value_end > length) return null;
            attribute.value = record[base + value_offset .. base + value_end];
        } else {
            if (base + 0x40 > record.len) return null;
            attribute.lowest_vcn = le64(record, base + 0x10);
            attribute.highest_vcn = le64(record, base + 0x18);
            const mapping_offset = le16(record, base + 0x20);
            attribute.compression_unit = record[base + 0x22];
            attribute.allocated_size = le64(record, base + 0x28);
            attribute.data_size = le64(record, base + 0x30);
            attribute.initialized_size = le64(record, base + 0x38);
            if (@as(usize, mapping_offset) > length) return null;
            attribute.mapping_pairs = record[base + mapping_offset .. base + length];
        }
        return attribute;
    }
};

/// Finds the first attribute of a type with an exactly matching name
/// (case-sensitive UTF-16 bytes; unnamed = empty name).
pub fn findAttribute(record: []const u8, header: FileRecordHeader, attr_type: AttrType, name_utf16: []const u8) ?Attribute {
    var iterator = AttributeIterator.init(record, header);
    while (iterator.next()) |attribute| {
        if (attribute.attr_type != @intFromEnum(attr_type)) continue;
        if (!std.mem.eql(u8, attribute.name, name_utf16)) continue;
        return attribute;
    }
    return null;
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
        if (self.failed) return null;
        if (self.offset >= self.mapping.len) return null;
        const header = self.mapping[self.offset];
        if (header == 0) return null;
        const length_size: usize = header & 0x0F;
        const offset_size: usize = (header >> 4) & 0x0F;
        if (length_size == 0 or length_size > 8 or offset_size > 8) {
            self.failed = true;
            return null;
        }
        const encoded_size = 1 + length_size + offset_size;
        if (encoded_size > self.mapping.len - self.offset) {
            self.failed = true;
            return null;
        }

        var length: u64 = 0;
        var i: usize = 0;
        while (i < length_size) : (i += 1) {
            length |= @as(u64, self.mapping[self.offset + 1 + i]) << @intCast(8 * i);
        }
        if (length == 0) {
            self.failed = true;
            return null;
        }

        var run = Run{ .length_clusters = length, .lcn = null };
        if (offset_size > 0) {
            var delta_bits: u64 = 0;
            i = 0;
            while (i < offset_size) : (i += 1) {
                delta_bits |= @as(u64, self.mapping[self.offset + 1 + length_size + i]) << @intCast(8 * i);
            }
            // Sign-extend the top byte.
            if (offset_size < 8) {
                // Keep the 64-bit shift out of this branch: casting 64 to
                // u6 used to trap for a valid eight-byte LCN delta.
                const bits: u6 = @intCast(8 * offset_size);
                const sign_bit = @as(u64, 1) << (bits - 1);
                if ((delta_bits & sign_bit) != 0) {
                    delta_bits |= ~@as(u64, 0) << bits;
                }
            }
            const delta: i64 = @bitCast(delta_bits);
            if ((delta > 0 and self.current_lcn > std.math.maxInt(i64) - delta) or
                (delta < 0 and self.current_lcn < std.math.minInt(i64) - delta))
            {
                self.failed = true;
                return null;
            }
            self.current_lcn += delta;
            if (self.current_lcn < 0) {
                self.failed = true;
                return null;
            }
            run.lcn = @intCast(self.current_lcn);
        }
        self.offset += encoded_size;
        return run;
    }

    pub fn hadError(self: RunlistIterator) bool {
        return self.failed;
    }
};

/// Encodes one mapping pair.  `lcn_delta` null encodes a sparse run.
/// Returns the encoded length or null when the buffer is too small.
pub fn encodeRun(out: []u8, length_clusters: u64, lcn_delta: ?i64) ?usize {
    if (length_clusters == 0) return null;
    var length_size: usize = 0;
    var remaining = length_clusters;
    while (remaining != 0) : (remaining >>= 8) length_size += 1;
    // Windows never emits a set top bit in the highest length byte (its
    // decoders treat the field sign-like); pad with one zero byte then.
    if (length_size < 8 and (length_clusters >> @intCast(8 * length_size - 1)) & 1 == 1) length_size += 1;

    var offset_size: usize = 0;
    if (lcn_delta) |delta| {
        // Minimal signed byte length that represents the delta.
        offset_size = 1;
        while (offset_size < 8) : (offset_size += 1) {
            const bits: u6 = @intCast(8 * offset_size);
            const min = -(@as(i64, 1) << (bits - 1));
            const max = (@as(i64, 1) << (bits - 1)) - 1;
            if (delta >= min and delta <= max) break;
        }
    }

    const total = 1 + length_size + offset_size;
    if (out.len < total) return null;
    out[0] = @intCast((offset_size << 4) | length_size);
    var i: usize = 0;
    while (i < length_size) : (i += 1) {
        out[1 + i] = @truncate(length_clusters >> @intCast(8 * i));
    }
    if (lcn_delta) |delta| {
        const raw: u64 = @bitCast(delta);
        i = 0;
        while (i < offset_size) : (i += 1) {
            out[1 + length_size + i] = @truncate(raw >> @intCast(8 * i));
        }
    }
    return total;
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
        if (value.len < 0x24) return null;
        return .{
            .creation_time = le64(value, 0x00),
            .data_change_time = le64(value, 0x08),
            .mft_change_time = le64(value, 0x10),
            .access_time = le64(value, 0x18),
            .file_attributes = le32(value, 0x20),
        };
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
        if (value.len < 0x42) return null;
        const name_length = value[0x40];
        const name_end = 0x42 + @as(usize, name_length) * 2;
        if (value.len < name_end) return null;
        return .{
            .parent = FileReference.parse(le64(value, 0x00)),
            .creation_time = le64(value, 0x08),
            .data_change_time = le64(value, 0x10),
            .mft_change_time = le64(value, 0x18),
            .access_time = le64(value, 0x20),
            .allocated_size = le64(value, 0x28),
            .data_size = le64(value, 0x30),
            .flags = le32(value, 0x38),
            .name_length = name_length,
            .namespace = value[0x41],
            .name = value[0x42..name_end],
        };
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
        const value = self.value;
        if (self.offset + 0x1A > value.len) return null;
        const base = self.offset;
        const entry_length = le16(value, base + 0x04);
        if (entry_length < 0x1A or base + entry_length > value.len) return null;
        const name_length = value[base + 0x06];
        const name_offset = value[base + 0x07];
        var name: []const u8 = &[_]u8{};
        if (name_length > 0) {
            const name_end = @as(usize, name_offset) + @as(usize, name_length) * 2;
            if (name_end > entry_length) return null;
            name = value[base + name_offset .. base + name_end];
        }
        self.offset += entry_length;
        return .{
            .attr_type = le32(value, base + 0x00),
            .entry_length = entry_length,
            .lowest_vcn = le64(value, base + 0x08),
            .mft_reference = FileReference.parse(le64(value, base + 0x10)),
            .instance = le16(value, base + 0x18),
            .name = name,
        };
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
        if (value.len < 0x20) return null;
        const header = IndexHeader.parse(value[0x10..]) orelse return null;
        const entries_start = 0x10 + @as(usize, header.entries_offset);
        const entries_end = 0x10 + @as(usize, header.index_length);
        if (entries_start > value.len or entries_end > value.len or entries_start > entries_end) return null;
        return .{
            .indexed_attr_type = le32(value, 0x00),
            .collation_rule = le32(value, 0x04),
            .index_block_bytes = le32(value, 0x08),
            .header = header,
            .entries = value[entries_start..entries_end],
        };
    }
};

pub const IndexHeader = struct {
    entries_offset: u32,
    index_length: u32,
    allocated_size: u32,
    flags: u8,

    pub fn parse(bytes: []const u8) ?IndexHeader {
        if (bytes.len < 0x10) return null;
        const header = IndexHeader{
            .entries_offset = le32(bytes, 0x00),
            .index_length = le32(bytes, 0x04),
            .allocated_size = le32(bytes, 0x08),
            .flags = bytes[0x0C],
        };
        if (header.entries_offset < 0x10 or header.index_length < header.entries_offset) return null;
        return header;
    }

    pub fn hasSubNodes(self: IndexHeader) bool {
        return (self.flags & 0x01) != 0;
    }
};

pub const IndexBlock = struct {
    lsn: u64,
    vcn: u64,
    /// Entry bytes after fixup removal.
    entries: []const u8,

    /// The block buffer must already have fixups applied.
    pub fn parse(block: []const u8) ?IndexBlock {
        if (block.len < 0x28) return null;
        if (le32(block, 0x00) != INDX_MAGIC) return null;
        const header = IndexHeader.parse(block[0x18..]) orelse return null;
        const entries_start = 0x18 + @as(usize, header.entries_offset);
        const entries_end = 0x18 + @as(usize, header.index_length);
        if (entries_start > block.len or entries_end > block.len or entries_start > entries_end) return null;
        return .{
            .lsn = le64(block, 0x08),
            .vcn = le64(block, 0x10),
            .entries = block[entries_start..entries_end],
        };
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
        return (self.flags & INDEX_ENTRY_END) != 0;
    }

    pub fn hasSubNode(self: IndexEntry) bool {
        return (self.flags & INDEX_ENTRY_NODE) != 0;
    }

    pub fn fileName(self: IndexEntry) ?FileName {
        if (self.key.len == 0) return null;
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
        if (self.done) return null;
        const entries = self.entries;
        if (self.offset + 0x10 > entries.len) {
            self.done = true;
            return null;
        }
        const base = self.offset;
        const entry_length = le16(entries, base + 0x08);
        if (entry_length < 0x10 or base + entry_length > entries.len) {
            self.done = true;
            return null;
        }
        const key_length = le16(entries, base + 0x0A);
        const flags = le16(entries, base + 0x0C);
        var key: []const u8 = &[_]u8{};
        if (key_length > 0) {
            if (0x10 + @as(usize, key_length) > entry_length) {
                self.done = true;
                return null;
            }
            key = entries[base + 0x10 .. base + 0x10 + key_length];
        }
        var sub_node: ?u64 = null;
        if ((flags & INDEX_ENTRY_NODE) != 0) {
            if (entry_length < 0x10 + 8) {
                self.done = true;
                return null;
            }
            sub_node = le64(entries, base + entry_length - 8);
        }
        self.offset += entry_length;
        if ((flags & INDEX_ENTRY_END) != 0) self.done = true;
        return .{
            .file_reference = le64(entries, base + 0x00),
            .entry_length = entry_length,
            .key_length = key_length,
            .flags = flags,
            .key = key,
            .sub_node_vcn = sub_node,
        };
    }
};

// ---------------------------------------------------------------------------
// $UpCase collation
// ---------------------------------------------------------------------------

pub fn upcaseUnit(upcase: []const u8, unit: u16) u16 {
    const index = @as(usize, unit) * 2;
    if (index + 2 > upcase.len) return unit;
    return le16(upcase, index);
}

/// COLLATION_FILE_NAME order of two UTF-16LE names using the volume upcase
/// table: unit-wise upcased comparison, shorter prefix first.
pub fn compareFileNames(upcase: []const u8, left_utf16: []const u8, right_utf16: []const u8) std.math.Order {
    const left_units = left_utf16.len / 2;
    const right_units = right_utf16.len / 2;
    const common = @min(left_units, right_units);
    var i: usize = 0;
    while (i < common) : (i += 1) {
        const l = upcaseUnit(upcase, le16(left_utf16, i * 2));
        const r = upcaseUnit(upcase, le16(right_utf16, i * 2));
        if (l < r) return .lt;
        if (l > r) return .gt;
    }
    if (left_units < right_units) return .lt;
    if (left_units > right_units) return .gt;
    return .eq;
}

/// Case-insensitive match of an ASCII name against a UTF-16LE name.
pub fn asciiMatchesUtf16(upcase: []const u8, ascii: []const u8, utf16: []const u8) bool {
    if (utf16.len != ascii.len * 2) return false;
    var i: usize = 0;
    while (i < ascii.len) : (i += 1) {
        const a = upcaseUnit(upcase, @as(u16, ascii[i]));
        const u = upcaseUnit(upcase, le16(utf16, i * 2));
        if (a != u) return false;
    }
    return true;
}

/// Copies a UTF-16LE name into an ASCII buffer.  Returns the length or null
/// when a unit is outside 0x20..0x7E (caller decides how to surface that).
pub fn utf16ToAscii(utf16: []const u8, out: []u8) ?usize {
    const units = utf16.len / 2;
    if (units > out.len) return null;
    var i: usize = 0;
    while (i < units) : (i += 1) {
        const unit = le16(utf16, i * 2);
        if (unit < 0x20 or unit > 0x7E) return null;
        out[i] = @intCast(unit);
    }
    return units;
}

pub fn asciiToUtf16(ascii: []const u8, out: []u8) ?usize {
    if (out.len < ascii.len * 2) return null;
    var i: usize = 0;
    while (i < ascii.len) : (i += 1) {
        out[i * 2] = ascii[i];
        out[i * 2 + 1] = 0;
    }
    return ascii.len * 2;
}

/// UTF-8 -> UTF-16LE for file names (BMP only; supplementary planes and
/// malformed sequences are visible failures).  Returns the byte length of
/// the UTF-16 output.
pub fn utf8ToUtf16(name: []const u8, out: []u8) ?usize {
    var i: usize = 0;
    var o: usize = 0;
    while (i < name.len) {
        const b0 = name[i];
        var code: u32 = 0;
        var extra: usize = 0;
        if (b0 < 0x80) {
            code = b0;
        } else if (b0 & 0xE0 == 0xC0) {
            code = b0 & 0x1F;
            extra = 1;
        } else if (b0 & 0xF0 == 0xE0) {
            code = b0 & 0x0F;
            extra = 2;
        } else {
            return null; // 4-byte sequences (> BMP) and stray continuations
        }
        if (i + extra >= name.len) return null;
        var k: usize = 0;
        while (k < extra) : (k += 1) {
            const cont = name[i + 1 + k];
            if (cont & 0xC0 != 0x80) return null;
            code = (code << 6) | (cont & 0x3F);
        }
        i += 1 + extra;
        if (code == 0 or (code >= 0xD800 and code <= 0xDFFF) or code > 0xFFFF) return null;
        if (o + 2 > out.len) return null;
        out[o] = @truncate(code);
        out[o + 1] = @truncate(code >> 8);
        o += 2;
    }
    return o;
}

/// UTF-16LE -> UTF-8 for file names (BMP only).  Returns the byte length of
/// the UTF-8 output or null when the buffer is too small or a surrogate
/// appears.
pub fn utf16ToUtf8(name_utf16: []const u8, out: []u8) ?usize {
    const units = name_utf16.len / 2;
    var i: usize = 0;
    var o: usize = 0;
    while (i < units) : (i += 1) {
        const unit = le16(name_utf16, i * 2);
        if (unit == 0 or (unit >= 0xD800 and unit <= 0xDFFF)) return null;
        if (unit < 0x80) {
            if (o + 1 > out.len) return null;
            out[o] = @intCast(unit);
            o += 1;
        } else if (unit < 0x800) {
            if (o + 2 > out.len) return null;
            out[o] = @intCast(0xC0 | (unit >> 6));
            out[o + 1] = @intCast(0x80 | (unit & 0x3F));
            o += 2;
        } else {
            if (o + 3 > out.len) return null;
            out[o] = @intCast(0xE0 | (unit >> 12));
            out[o + 1] = @intCast(0x80 | ((unit >> 6) & 0x3F));
            out[o + 2] = @intCast(0x80 | (unit & 0x3F));
            o += 3;
        }
    }
    return o;
}

// ---------------------------------------------------------------------------
// LZNT1 decompression (read side)
// ---------------------------------------------------------------------------

/// Decompresses one LZNT1 compression unit (a sequence of 4-KB chunks) from
/// `src` into `dst`.  Returns the number of bytes produced; a header of 0
/// ends the unit early (the remainder reads as zeros at the caller).  Bad
/// chunk signatures and out-of-range back references are visible failures.
pub fn lznt1Decompress(src: []const u8, dst: []u8) ?usize {
    var in: usize = 0;
    var out: usize = 0;
    while (in + 2 <= src.len and out < dst.len) {
        const header = le16(src, in);
        in += 2;
        if (header == 0) break;
        if ((header & 0x7000) != 0x3000) return null;
        const payload_len: usize = (header & 0x0FFF) + 1;
        const chunk_end_in = in + payload_len;
        if (chunk_end_in > src.len) return null;
        const chunk_out_start = out;

        if ((header & 0x8000) == 0) {
            // Stored chunk: literal payload (normally 4096 bytes).
            if (out + payload_len > dst.len) return null;
            @memcpy(dst[out .. out + payload_len], src[in..chunk_end_in]);
            out += payload_len;
            in = chunk_end_in;
            continue;
        }

        while (in < chunk_end_in and out < dst.len) {
            var flags = src[in];
            in += 1;
            var t: usize = 0;
            while (t < 8 and in < chunk_end_in) : (t += 1) {
                if ((flags & 1) == 0) {
                    if (out + 1 > dst.len) return null;
                    dst[out] = src[in];
                    out += 1;
                    in += 1;
                } else {
                    if (in + 2 > chunk_end_in) return null;
                    const tuple = le16(src, in);
                    in += 2;
                    const written = out - chunk_out_start;
                    if (written == 0) return null; // back reference at chunk start
                    // The offset/length split depends on how much of the
                    // chunk has been decoded already.
                    var l: u4 = 0;
                    var u: usize = written - 1;
                    while (u >= 0x10) : (u >>= 1) l += 1;
                    const length: usize = (tuple & (@as(u16, 0x0FFF) >> l)) + 3;
                    const offset: usize = (tuple >> @intCast(12 - @as(u16, l))) + 1;
                    if (offset > written) return null;
                    if (out + length > dst.len) return null;
                    var k: usize = 0;
                    while (k < length) : (k += 1) {
                        dst[out] = dst[out - offset];
                        out += 1;
                    }
                }
                flags >>= 1;
                if (out >= dst.len) break;
            }
        }
        in = chunk_end_in;
    }
    return out;
}

// ---------------------------------------------------------------------------
// $VOLUME_INFORMATION
// ---------------------------------------------------------------------------

pub const VolumeInformation = struct {
    major: u8,
    minor: u8,
    flags: u16,

    pub fn parse(value: []const u8) ?VolumeInformation {
        if (value.len < 0x0C) return null;
        return .{ .major = value[0x08], .minor = value[0x09], .flags = le16(value, 0x0A) };
    }
};
