const module_file = @import("module_file.zig");
const k = @import("log.zig");

pub const HEADER_SIZE: usize = 64;
pub const SECTION_SIZE: usize = 32;
pub const ENTRY_SIZE: usize = 16;
pub const IMPORT_SIZE: usize = 16;
pub const EXPORT_SIZE: usize = 16;
pub const RELOCATION_SIZE: usize = 24;
pub const RELOCATION_WINDOW_RECORDS: usize = module_file.metadata_window_size / RELOCATION_SIZE;
pub const VERSION: u16 = 1;
pub const ARCH_X86_64: u16 = 1;

pub const Kind = enum(u16) {
    r4x = 1,
    r4l = 2,
    r4d = 3,
    r4p = 4,
    platform_api_provider_reserved = 5,
    kernel_module_reserved = 6,
};

pub const Header = struct {
    version: u16,
    arch: u16,
    kind_raw: u16,
    header_size: u16,
    flags: u32,
    section_off: u32,
    section_count: u32,
    import_off: u32,
    import_count: u32,
    export_off: u32,
    export_count: u32,
    reloc_off: u32,
    reloc_count: u32,
    entry_off: u32,
    entry_count: u32,
    meta_off: u32,
    meta_size: u32,

    pub fn kind(self: Header) ?Kind {
        return kindFromRaw(self.kind_raw);
    }
};

pub const Limits = struct {
    max_sections: u32 = 16,
    max_imports: u32 = 16,
    max_exports: u32 = 16,
    max_relocations: u32 = 0xFFFF_FFFF,
    require_sections: bool = true,
    require_entries: bool = true,
};

pub const HeaderReadRequest = struct {
    source: module_file.FileSource,
    file_size: usize,
    expected_kind: ?Kind = null,
    limits: Limits = .{},
    name: []const u8 = "r4m-reader",
    verbose: bool = true,
};

pub const MetadataReadRequest = struct {
    source: module_file.FileSource,
    header: Header,
    out: []u8,
    name: []const u8 = "r4m-metadata",
    verbose: bool = true,
};

pub const ExportLookupRequest = struct {
    source: module_file.FileSource,
    file_size: usize,
    header: Header,
    symbol: []const u8,
    min_version: u32 = 1,
    scratch: []u8,
    name: []const u8 = "r4m-export",
    verbose: bool = true,
};

pub const ExportRecord = struct {
    name_offset: u32,
    section: u32,
    offset: u32,
    version: u32,
};

pub const SectionRecord = struct {
    name: [8]u8,
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
};

pub const EntryRecord = struct {
    kind: u32,
    section: u32,
    offset: u32,
    flags: u32,
};

pub const ImportRecord = struct {
    module_offset: u32,
    symbol_offset: u32,
    min_version: u32,
    flags: u32,
};

pub const RelocationRecord = struct {
    kind: u32,
    patch_section: u32,
    patch_offset: u32,
    target_section: u32,
    target_offset: u32,
    addend: i32,
};

/// One loader-local, allocation-free view of an R4M0 file. All small header,
/// table, name, metadata and relocation reads share the same two bounded
/// windows. Section payloads intentionally continue to use module_file
/// directly so they never evict metadata merely because they are large.
pub const Reader = struct {
    file: module_file.BoundedReader,

    pub fn init(source: module_file.FileSource, file_size: usize) Reader {
        return .{ .file = module_file.BoundedReader.init(source, file_size) };
    }

    pub fn readHeader(self: *Reader, expected_kind: ?Kind, limits: Limits, name: []const u8, verbose: bool) ?Header {
        if (self.file.file_size < HEADER_SIZE) {
            logFailure(verbose, name, "short-header");
            return null;
        }
        var raw: [HEADER_SIZE]u8 = .{0} ** HEADER_SIZE;
        if (!self.file.readExactAt(0, raw[0..], name, verbose)) return null;
        if (memEql(raw[0..4], "R4X0") or memEql(raw[0..4], "R4D0") or memEql(raw[0..4], "R4P0")) {
            logFailure(verbose, name, "legacy-format");
            return null;
        }
        if (!memEql(raw[0..4], "R4M0")) {
            logFailure(verbose, name, "bad-magic");
            return null;
        }
        const header = parseHeader(raw[0..]);
        if (!validateHeader(header, self.file.file_size, expected_kind, limits, name, verbose)) return null;
        return header;
    }

    pub fn readMetadata(self: *Reader, header: Header, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
        if (header.meta_size == 0) return out[0..0];
        const size: usize = @intCast(header.meta_size);
        if (size > out.len) {
            logFailure(verbose, name, "metadata-too-large");
            return null;
        }
        const meta = out[0..size];
        if (!self.file.readExactAt(@intCast(header.meta_off), meta, name, verbose)) return null;
        return meta;
    }

    pub fn hasExport(self: *Reader, header: Header, symbol: []const u8, min_version: u32, scratch: []u8, name: []const u8, verbose: bool) bool {
        if (header.export_count == 0) return false;
        var i: usize = 0;
        while (i < header.export_count) : (i += 1) {
            const record = self.readExportRecord(header, i, name, verbose) orelse return false;
            if (record.version < min_version) continue;
            const export_name = self.readZString(record.name_offset, scratch, name, verbose) orelse return false;
            if (memEql(export_name, symbol)) return true;
        }
        return false;
    }

    pub fn readSectionRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?SectionRecord {
        if (index >= header.section_count) return null;
        var raw: [SECTION_SIZE]u8 = .{0} ** SECTION_SIZE;
        const offset = @as(usize, @intCast(header.section_off)) + index * SECTION_SIZE;
        if (!self.file.readExactAt(offset, raw[0..], name, verbose)) return null;
        var section_name: [8]u8 = .{0} ** 8;
        @memcpy(section_name[0..], raw[0..8]);
        return .{
            .name = section_name,
            .flags = readLe32(raw[8..12]),
            .file_off = readLe32(raw[12..16]),
            .file_size = readLe32(raw[16..20]),
            .mem_size = readLe32(raw[20..24]),
            .alignment = readLe32(raw[24..28]),
        };
    }

    pub fn readEntryRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?EntryRecord {
        if (index >= header.entry_count) return null;
        var raw: [ENTRY_SIZE]u8 = .{0} ** ENTRY_SIZE;
        const offset = @as(usize, @intCast(header.entry_off)) + index * ENTRY_SIZE;
        if (!self.file.readExactAt(offset, raw[0..], name, verbose)) return null;
        return .{
            .kind = readLe32(raw[0..4]),
            .section = readLe32(raw[4..8]),
            .offset = readLe32(raw[8..12]),
            .flags = readLe32(raw[12..16]),
        };
    }

    pub fn readImportRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?ImportRecord {
        if (index >= header.import_count) return null;
        var raw: [IMPORT_SIZE]u8 = .{0} ** IMPORT_SIZE;
        const offset = @as(usize, @intCast(header.import_off)) + index * IMPORT_SIZE;
        if (!self.file.readExactAt(offset, raw[0..], name, verbose)) return null;
        return .{
            .module_offset = readLe32(raw[0..4]),
            .symbol_offset = readLe32(raw[4..8]),
            .min_version = readLe32(raw[8..12]),
            .flags = readLe32(raw[12..16]),
        };
    }

    pub fn readExportRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?ExportRecord {
        if (index >= header.export_count) return null;
        var raw: [EXPORT_SIZE]u8 = .{0} ** EXPORT_SIZE;
        const offset = @as(usize, @intCast(header.export_off)) + index * EXPORT_SIZE;
        if (!self.file.readExactAt(offset, raw[0..], name, verbose)) return null;
        return .{
            .name_offset = readLe32(raw[0..4]),
            .section = readLe32(raw[4..8]),
            .offset = readLe32(raw[8..12]),
            .version = readLe32(raw[12..16]),
        };
    }

    pub fn readRelocationRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?RelocationRecord {
        if (index >= header.reloc_count) return null;
        var raw: [RELOCATION_SIZE]u8 = .{0} ** RELOCATION_SIZE;
        const offset = @as(usize, @intCast(header.reloc_off)) + index * RELOCATION_SIZE;
        if (!self.file.readExactAt(offset, raw[0..], name, verbose)) return null;
        return decodeRelocationRecord(raw[0..]);
    }

    pub fn readZString(self: *Reader, offset_u32: u32, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
        const offset: usize = @intCast(offset_u32);
        if (offset >= self.file.file_size or out.len == 0) return null;
        const max_len = @min(out.len, self.file.file_size - offset);
        const dst = out[0..max_len];
        if (!self.file.readExactAt(offset, dst, name, verbose)) return null;
        var i: usize = 0;
        while (i < dst.len) : (i += 1) {
            if (dst[i] == 0) return dst[0..i];
        }
        logFailure(verbose, name, "zstring-too-large");
        return null;
    }
};

/// Streams the unchanged relocation table in record-aligned windows.  The
/// caller still observes and applies records in their original order, while
/// the filesystem sees one request per window instead of one 24-byte request
/// per relocation.
pub const RelocationWindowReader = struct {
    reader: *Reader,
    header: Header,
    name: []const u8,
    verbose: bool,
    next_index: usize = 0,

    pub fn init(reader: *Reader, header: Header, name: []const u8, verbose: bool) RelocationWindowReader {
        return .{
            .reader = reader,
            .header = header,
            .name = name,
            .verbose = verbose,
        };
    }

    pub fn next(self: *RelocationWindowReader) ?RelocationRecord {
        if (self.next_index >= self.header.reloc_count) return null;
        const record = self.reader.readRelocationRecord(self.header, self.next_index, self.name, self.verbose) orelse return null;
        self.next_index += 1;
        return record;
    }
};

pub fn relocationWindowCount(record_count: usize) usize {
    if (record_count == 0) return 0;
    return 1 + (record_count - 1) / RELOCATION_WINDOW_RECORDS;
}

pub const MetadataIterator = struct {
    meta: []const u8,
    offset: usize = 0,

    pub fn next(self: *MetadataIterator) ?[]const u8 {
        while (self.offset < self.meta.len) {
            const start = self.offset;
            while (self.offset < self.meta.len and self.meta[self.offset] != 0) : (self.offset += 1) {}
            const item = self.meta[start..self.offset];
            if (self.offset < self.meta.len) self.offset += 1;
            if (item.len != 0) return item;
        }
        return null;
    }
};

pub fn readHeader(req: HeaderReadRequest) ?Header {
    var reader = Reader.init(req.source, req.file_size);
    return reader.readHeader(req.expected_kind, req.limits, req.name, req.verbose);
}

pub fn readMetadata(req: MetadataReadRequest) ?[]const u8 {
    var reader = Reader.init(req.source, @intCast(req.source.entry.size));
    return reader.readMetadata(req.header, req.out, req.name, req.verbose);
}

pub fn metadataIterator(meta: []const u8) MetadataIterator {
    return .{ .meta = meta };
}

pub fn metadataValue(meta: []const u8, prefix: []const u8) ?[]const u8 {
    var it = metadataIterator(meta);
    var found: ?[]const u8 = null;
    while (it.next()) |item| {
        if (startsWith(item, prefix)) found = item[prefix.len..];
    }
    return found;
}

pub fn metadataContains(meta: []const u8, needle: []const u8) bool {
    var it = metadataIterator(meta);
    while (it.next()) |item| {
        if (memEql(item, needle)) return true;
    }
    return false;
}

pub fn firstMetadataItem(meta: []const u8) ?[]const u8 {
    var it = metadataIterator(meta);
    return it.next();
}

pub fn hasExport(req: ExportLookupRequest) bool {
    var reader = Reader.init(req.source, req.file_size);
    return reader.hasExport(req.header, req.symbol, req.min_version, req.scratch, req.name, req.verbose);
}

pub fn kindFromRaw(raw: u16) ?Kind {
    return switch (raw) {
        1 => .r4x,
        2 => .r4l,
        3 => .r4d,
        4 => .r4p,
        5 => .platform_api_provider_reserved,
        6 => .kernel_module_reserved,
        else => null,
    };
}

pub fn isLoadableContainerKind(kind: Kind) bool {
    return switch (kind) {
        .r4x, .r4l, .r4d, .r4p => true,
        .platform_api_provider_reserved, .kernel_module_reserved => false,
    };
}

pub fn readSectionRecord(source: module_file.FileSource, header: Header, index: usize, name: []const u8, verbose: bool) ?SectionRecord {
    var reader = Reader.init(source, @intCast(source.entry.size));
    return reader.readSectionRecord(header, index, name, verbose);
}

pub fn readEntryRecord(source: module_file.FileSource, header: Header, index: usize, name: []const u8, verbose: bool) ?EntryRecord {
    var reader = Reader.init(source, @intCast(source.entry.size));
    return reader.readEntryRecord(header, index, name, verbose);
}

pub fn readImportRecord(source: module_file.FileSource, header: Header, index: usize, name: []const u8, verbose: bool) ?ImportRecord {
    var reader = Reader.init(source, @intCast(source.entry.size));
    return reader.readImportRecord(header, index, name, verbose);
}

pub fn readExportRecord(source: module_file.FileSource, header: Header, index: usize, name: []const u8, verbose: bool) ?ExportRecord {
    var reader = Reader.init(source, @intCast(source.entry.size));
    return reader.readExportRecord(header, index, name, verbose);
}

pub fn readRelocationRecord(source: module_file.FileSource, header: Header, index: usize, name: []const u8, verbose: bool) ?RelocationRecord {
    var reader = Reader.init(source, @intCast(source.entry.size));
    return reader.readRelocationRecord(header, index, name, verbose);
}

fn decodeRelocationRecord(raw: []const u8) RelocationRecord {
    return .{
        .kind = readLe32(raw[0..4]),
        .patch_section = readLe32(raw[4..8]),
        .patch_offset = readLe32(raw[8..12]),
        .target_section = readLe32(raw[12..16]),
        .target_offset = readLe32(raw[16..20]),
        .addend = @bitCast(readLe32(raw[20..24])),
    };
}

pub fn readZString(source: module_file.FileSource, file_size: usize, offset_u32: u32, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
    var reader = Reader.init(source, file_size);
    return reader.readZString(offset_u32, out, name, verbose);
}

fn parseHeader(raw: []const u8) Header {
    return .{
        .version = readLe16(raw[4..6]),
        .arch = readLe16(raw[6..8]),
        .kind_raw = readLe16(raw[8..10]),
        .header_size = readLe16(raw[10..12]),
        .flags = readLe32(raw[12..16]),
        .section_off = readLe32(raw[16..20]),
        .section_count = readLe32(raw[20..24]),
        .import_off = readLe32(raw[24..28]),
        .import_count = readLe32(raw[28..32]),
        .export_off = readLe32(raw[32..36]),
        .export_count = readLe32(raw[36..40]),
        .reloc_off = readLe32(raw[40..44]),
        .reloc_count = readLe32(raw[44..48]),
        .entry_off = readLe32(raw[48..52]),
        .entry_count = readLe32(raw[52..56]),
        .meta_off = readLe32(raw[56..60]),
        .meta_size = readLe32(raw[60..64]),
    };
}

fn validateHeader(header: Header, file_size: usize, expected_kind: ?Kind, limits: Limits, name: []const u8, verbose: bool) bool {
    if (header.version != VERSION) return reject(verbose, name, "version");
    if (header.arch != ARCH_X86_64) return reject(verbose, name, "arch");
    if (header.header_size != HEADER_SIZE) return reject(verbose, name, "header-size");
    const kind = header.kind() orelse return reject(verbose, name, "kind");
    if (!isLoadableContainerKind(kind)) return reject(verbose, name, "reserved-kind");
    if (expected_kind) |expected| {
        if (kind != expected) return reject(verbose, name, "unexpected-kind");
    }
    if (header.section_count > limits.max_sections) return reject(verbose, name, "section-count");
    if (header.import_count > limits.max_imports) return reject(verbose, name, "import-count");
    if (header.export_count > limits.max_exports) return reject(verbose, name, "export-count");
    if (header.reloc_count > limits.max_relocations) return reject(verbose, name, "reloc-count");
    if (!checkTable(file_size, header.section_off, header.section_count, SECTION_SIZE, limits.require_sections)) return reject(verbose, name, "section-table");
    if (!checkTable(file_size, header.entry_off, header.entry_count, ENTRY_SIZE, limits.require_entries)) return reject(verbose, name, "entry-table");
    if (!checkTable(file_size, header.import_off, header.import_count, IMPORT_SIZE, false)) return reject(verbose, name, "import-table");
    if (!checkTable(file_size, header.export_off, header.export_count, EXPORT_SIZE, false)) return reject(verbose, name, "export-table");
    if (!checkTable(file_size, header.reloc_off, header.reloc_count, RELOCATION_SIZE, false)) return reject(verbose, name, "reloc-table");
    if (header.meta_size != 0 and !checkRange(file_size, @intCast(header.meta_off), @intCast(header.meta_size))) return reject(verbose, name, "metadata");
    return true;
}

fn checkTable(file_size: usize, off: u32, count: u32, entry_size: usize, required: bool) bool {
    if (count == 0) return !required;
    if (off == 0) return false;
    const table_off: usize = @intCast(off);
    const table_count: usize = @intCast(count);
    if (table_count > file_size / entry_size) return false;
    return checkRange(file_size, table_off, table_count * entry_size);
}

fn checkRange(file_size: usize, off: usize, len: usize) bool {
    return off <= file_size and len <= file_size - off;
}

fn reject(verbose: bool, name: []const u8, reason: []const u8) bool {
    logFailure(verbose, name, reason);
    return false;
}

fn logFailure(verbose: bool, name: []const u8, reason: []const u8) void {
    if (!verbose) return;
    k.puts("[R4M] ");
    k.puts(reason);
    k.puts(" failed ");
    k.puts(name);
    k.puts("\r\n");
}

fn startsWith(value: []const u8, prefix: []const u8) bool {
    if (value.len < prefix.len) return false;
    var i: usize = 0;
    while (i < prefix.len) : (i += 1) {
        if (value[i] != prefix[i]) return false;
    }
    return true;
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
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

test "relocation windows scale with contiguous table ranges" {
    const testing = @import("std").testing;

    try testing.expectEqual(@as(usize, 0), relocationWindowCount(0));
    try testing.expectEqual(@as(usize, 1), relocationWindowCount(1));
    try testing.expectEqual(@as(usize, 1), relocationWindowCount(RELOCATION_WINDOW_RECORDS));
    try testing.expectEqual(@as(usize, 2), relocationWindowCount(RELOCATION_WINDOW_RECORDS + 1));
    try testing.expectEqual(@as(usize, 139), relocationWindowCount(23_510));

    var raw: [RELOCATION_SIZE]u8 = .{0} ** RELOCATION_SIZE;
    raw[0] = 4;
    raw[4] = 2;
    raw[8] = 0x78;
    raw[9] = 0x56;
    raw[10] = 0x34;
    raw[11] = 0x12;
    raw[12] = 3;
    raw[16] = 9;
    raw[20] = 0xFE;
    raw[21] = 0xFF;
    raw[22] = 0xFF;
    raw[23] = 0xFF;
    const record = decodeRelocationRecord(raw[0..]);
    try testing.expectEqual(@as(u32, 4), record.kind);
    try testing.expectEqual(@as(u32, 2), record.patch_section);
    try testing.expectEqual(@as(u32, 0x12345678), record.patch_offset);
    try testing.expectEqual(@as(u32, 3), record.target_section);
    try testing.expectEqual(@as(u32, 9), record.target_offset);
    try testing.expectEqual(@as(i32, -2), record.addend);
}
