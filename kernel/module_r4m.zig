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
pub const IMPORT_FLAG_OPTIONAL: u32 = 1;

pub const Kind = enum(u16) {
    r4x = 1,
    r4l = 2,
    r4d = 3,
    r4p = 4,
    platform_api_provider_reserved = 5,
    kernel_module_reserved = 6,
};

pub const Header = extern struct {
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

pub const Limits = extern struct {
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

pub const ExportRecord = extern struct {
    name_offset: u32,
    section: u32,
    offset: u32,
    version: u32,
};

pub const SectionRecord = extern struct {
    name: [8]u8,
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
};

pub const EntryRecord = extern struct {
    kind: u32,
    section: u32,
    offset: u32,
    flags: u32,
};

pub const ImportRecord = extern struct {
    module_offset: u32,
    symbol_offset: u32,
    min_version: u32,
    flags: u32,
};

pub const RelocationRecord = extern struct {
    kind: u32,
    patch_section: u32,
    patch_offset: u32,
    target_section: u32,
    target_offset: u32,
    addend: i32,
};

// Private C values; native Rust owns every parser/iterator decision.
const NativeBytes = extern struct { data: [*]const u8, len: usize };
const NativeSpan = extern struct { offset: usize, len: usize, present: bool };
const NativeIo = extern struct { context: ?*anyopaque, read: *const fn (?*anyopaque, usize, [*]u8, usize) callconv(.c) bool, log: *const fn (?*anyopaque, [*]const u8, usize) callconv(.c) void, file_size: usize };
extern fn r4os_rust_r4m_kind(u16) u16;
extern fn r4os_rust_r4m_loadable(u16) bool;
extern fn r4os_rust_r4m_import(NativeBytes, u32) bool;
extern fn r4os_rust_r4m_header(*const NativeIo, u16, *const Limits, *Header) bool;
extern fn r4os_rust_r4m_record(*const NativeIo, *const Header, usize, u8, *anyopaque) bool;
extern fn r4os_rust_r4m_decode([*]const u8, usize, *RelocationRecord) void;
extern fn r4os_rust_r4m_zstring(*const NativeIo, u32, [*]u8, usize) NativeSpan;
extern fn r4os_rust_r4m_metadata(*const NativeIo, *const Header, [*]u8, usize) NativeSpan;
extern fn r4os_rust_r4m_export(*const NativeIo, *const Header, NativeBytes, u32, [*]u8, usize) bool;
extern fn r4os_rust_r4m_windows(usize) usize;
extern fn r4os_rust_r4m_next(*const NativeIo, *const Header, *usize, *RelocationRecord) bool;
extern fn r4os_rust_r4m_meta(u8, NativeBytes, NativeBytes, ?*usize) NativeSpan;
extern fn r4os_rust_r4m_labels(NativeBytes, *VersionLabels) void;
fn nativeBytes(value: []const u8) NativeBytes {
    return .{ .data = value.ptr, .len = value.len };
}
fn nativeSpan(data: []const u8, value: NativeSpan) ?[]const u8 {
    return if (value.present) data[value.offset..][0..value.len] else null;
}
const NativeContext = struct {
    file: *module_file.BoundedReader,
    name: []const u8,
    verbose: bool,
    fn read(pointer: ?*anyopaque, offset: usize, out: [*]u8, len: usize) callconv(.c) bool {
        const context: *@This() = @ptrCast(@alignCast(pointer.?));
        return context.file.readExactAt(offset, out[0..len], context.name, context.verbose);
    }
    fn log(pointer: ?*anyopaque, reason: [*]const u8, len: usize) callconv(.c) void {
        const context: *@This() = @ptrCast(@alignCast(pointer.?));
        if (!context.verbose) return;
        k.puts("[R4M] ");
        k.puts(reason[0..len]);
        k.puts(" failed ");
        k.puts(context.name);
        k.puts("\r\n");
    }
    fn io(self: *@This()) NativeIo {
        return .{ .context = self, .read = read, .log = log, .file_size = self.file.file_size };
    }
};
pub const Reader = struct {
    file: module_file.BoundedReader,
    pub fn init(source: module_file.FileSource, file_size: usize) Reader {
        return .{ .file = module_file.BoundedReader.init(source, file_size) };
    }
    pub fn reset(self: *Reader, source: module_file.FileSource, file_size: usize) void {
        self.file.reset(source, file_size);
    }
    pub fn readHeader(self: *Reader, expected_kind: ?Kind, limits: Limits, name: []const u8, verbose: bool) ?Header {
        var context: NativeContext = .{ .file = &self.file, .name = name, .verbose = verbose };
        const io = context.io();
        var out: Header = undefined;
        return if (r4os_rust_r4m_header(&io, if (expected_kind) |kind| @intFromEnum(kind) else 0, &limits, &out)) out else null;
    }
    pub fn readMetadata(self: *Reader, header: Header, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
        var context: NativeContext = .{ .file = &self.file, .name = name, .verbose = verbose };
        const io = context.io();
        return nativeSpan(out, r4os_rust_r4m_metadata(&io, &header, out.ptr, out.len));
    }
    pub fn hasExport(self: *Reader, header: Header, symbol: []const u8, min_version: u32, scratch: []u8, name: []const u8, verbose: bool) bool {
        var context: NativeContext = .{ .file = &self.file, .name = name, .verbose = verbose };
        const io = context.io();
        return r4os_rust_r4m_export(&io, &header, nativeBytes(symbol), min_version, scratch.ptr, scratch.len);
    }
    fn record(self: *Reader, comptime T: type, header: Header, index: usize, operation: u8, name: []const u8, verbose: bool) ?T {
        var context: NativeContext = .{ .file = &self.file, .name = name, .verbose = verbose };
        const io = context.io();
        var out: T = undefined;
        return if (r4os_rust_r4m_record(&io, &header, index, operation, &out)) out else null;
    }
    pub fn readSectionRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?SectionRecord {
        return self.record(SectionRecord, header, index, 0, name, verbose);
    }
    pub fn readEntryRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?EntryRecord {
        return self.record(EntryRecord, header, index, 1, name, verbose);
    }
    pub fn readImportRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?ImportRecord {
        return self.record(ImportRecord, header, index, 2, name, verbose);
    }
    pub fn readExportRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?ExportRecord {
        return self.record(ExportRecord, header, index, 3, name, verbose);
    }
    pub fn readRelocationRecord(self: *Reader, header: Header, index: usize, name: []const u8, verbose: bool) ?RelocationRecord {
        return self.record(RelocationRecord, header, index, 4, name, verbose);
    }
    pub fn readZString(self: *Reader, offset_u32: u32, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
        var context: NativeContext = .{ .file = &self.file, .name = name, .verbose = verbose };
        const io = context.io();
        return nativeSpan(out, r4os_rust_r4m_zstring(&io, offset_u32, out.ptr, out.len));
    }
};
pub const RelocationWindowReader = struct {
    reader: *Reader,
    header: Header,
    name: []const u8,
    verbose: bool,
    next_index: usize = 0,
    pub fn init(reader: *Reader, header: Header, name: []const u8, verbose: bool) RelocationWindowReader {
        return .{ .reader = reader, .header = header, .name = name, .verbose = verbose };
    }
    pub fn next(self: *RelocationWindowReader) ?RelocationRecord {
        var context: NativeContext = .{ .file = &self.reader.file, .name = self.name, .verbose = self.verbose };
        const io = context.io();
        var out: RelocationRecord = undefined;
        return if (r4os_rust_r4m_next(&io, &self.header, &self.next_index, &out)) out else null;
    }
};
pub fn relocationWindowCount(record_count: usize) usize {
    return r4os_rust_r4m_windows(record_count);
}
pub const MetadataIterator = struct {
    meta: []const u8,
    offset: usize = 0,
    pub fn next(self: *MetadataIterator) ?[]const u8 {
        return nativeSpan(self.meta, r4os_rust_r4m_meta(0, nativeBytes(self.meta), nativeBytes(""), &self.offset));
    }
};
pub fn metadataIterator(meta: []const u8) MetadataIterator {
    return .{ .meta = meta };
}
pub fn metadataValue(meta: []const u8, prefix: []const u8) ?[]const u8 {
    return nativeSpan(meta, r4os_rust_r4m_meta(1, nativeBytes(meta), nativeBytes(prefix), null));
}
pub fn metadataContains(meta: []const u8, needle: []const u8) bool {
    return r4os_rust_r4m_meta(2, nativeBytes(meta), nativeBytes(needle), null).present;
}
pub fn firstMetadataItem(meta: []const u8) ?[]const u8 {
    var it = metadataIterator(meta);
    return it.next();
}
pub const VersionLabels = extern struct {
    module: [32]u8 = .{0} ** 32,
    firmware: [32]u8 = .{0} ** 32,
    pub fn parse(meta: []const u8) VersionLabels {
        var out: VersionLabels = undefined;
        r4os_rust_r4m_labels(nativeBytes(meta), &out);
        return out;
    }
};
pub fn validImportFlags(module: []const u8, flags: u32) bool {
    return r4os_rust_r4m_import(nativeBytes(module), flags);
}
pub fn kindFromRaw(raw: u16) ?Kind {
    const kind = r4os_rust_r4m_kind(raw);
    return if (kind == 0) null else @enumFromInt(kind);
}
pub fn isLoadableContainerKind(kind: Kind) bool {
    return r4os_rust_r4m_loadable(@intFromEnum(kind));
}
pub fn readHeader(req: HeaderReadRequest) ?Header {
    var reader = Reader.init(req.source, req.file_size);
    return reader.readHeader(req.expected_kind, req.limits, req.name, req.verbose);
}
pub fn readMetadata(req: MetadataReadRequest) ?[]const u8 {
    var reader = Reader.init(req.source, @intCast(req.source.entry.size));
    return reader.readMetadata(req.header, req.out, req.name, req.verbose);
}
pub fn hasExport(req: ExportLookupRequest) bool {
    var reader = Reader.init(req.source, req.file_size);
    return reader.hasExport(req.header, req.symbol, req.min_version, req.scratch, req.name, req.verbose);
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
pub fn readZString(source: module_file.FileSource, file_size: usize, offset_u32: u32, out: []u8, name: []const u8, verbose: bool) ?[]const u8 {
    var reader = Reader.init(source, file_size);
    return reader.readZString(offset_u32, out, name, verbose);
}
fn decodeRelocationRecord(raw: []const u8) RelocationRecord {
    var out: RelocationRecord = undefined;
    r4os_rust_r4m_decode(raw.ptr, raw.len, &out);
    return out;
}
comptime {
    if (@sizeOf(Header) != 60 or @alignOf(Header) != 4 or @sizeOf(Limits) != 20 or @sizeOf(SectionRecord) != 28 or @sizeOf(EntryRecord) != 16 or @sizeOf(ImportRecord) != 16 or @sizeOf(ExportRecord) != 16 or @sizeOf(RelocationRecord) != 24 or @sizeOf(NativeSpan) != 24 or @sizeOf(NativeIo) != 32 or @sizeOf(VersionLabels) != 64 or RELOCATION_WINDOW_RECORDS != 170) @compileError("private R4M0 C layout");
    if (@offsetOf(Header, "version") != 0) @compileError("private Header.version offset");
    if (@offsetOf(Header, "arch") != 2) @compileError("private Header.arch offset");
    if (@offsetOf(Header, "kind_raw") != 4) @compileError("private Header.kind_raw offset");
    if (@offsetOf(Header, "header_size") != 6) @compileError("private Header.header_size offset");
    if (@offsetOf(Header, "flags") != 8) @compileError("private Header.flags offset");
    if (@offsetOf(Header, "section_off") != 12) @compileError("private Header.section_off offset");
    if (@offsetOf(Header, "section_count") != 16) @compileError("private Header.section_count offset");
    if (@offsetOf(Header, "import_off") != 20) @compileError("private Header.import_off offset");
    if (@offsetOf(Header, "import_count") != 24) @compileError("private Header.import_count offset");
    if (@offsetOf(Header, "export_off") != 28) @compileError("private Header.export_off offset");
    if (@offsetOf(Header, "export_count") != 32) @compileError("private Header.export_count offset");
    if (@offsetOf(Header, "reloc_off") != 36) @compileError("private Header.reloc_off offset");
    if (@offsetOf(Header, "reloc_count") != 40) @compileError("private Header.reloc_count offset");
    if (@offsetOf(Header, "entry_off") != 44) @compileError("private Header.entry_off offset");
    if (@offsetOf(Header, "entry_count") != 48) @compileError("private Header.entry_count offset");
    if (@offsetOf(Header, "meta_off") != 52) @compileError("private Header.meta_off offset");
    if (@offsetOf(Header, "meta_size") != 56) @compileError("private Header.meta_size offset");
    if (@offsetOf(Limits, "max_sections") != 0) @compileError("private Limits.max_sections offset");
    if (@offsetOf(Limits, "max_imports") != 4) @compileError("private Limits.max_imports offset");
    if (@offsetOf(Limits, "max_exports") != 8) @compileError("private Limits.max_exports offset");
    if (@offsetOf(Limits, "max_relocations") != 12) @compileError("private Limits.max_relocations offset");
    if (@offsetOf(Limits, "require_sections") != 16) @compileError("private Limits.require_sections offset");
    if (@offsetOf(Limits, "require_entries") != 17) @compileError("private Limits.require_entries offset");
    if (@offsetOf(ExportRecord, "name_offset") != 0) @compileError("private ExportRecord.name_offset offset");
    if (@offsetOf(ExportRecord, "section") != 4) @compileError("private ExportRecord.section offset");
    if (@offsetOf(ExportRecord, "offset") != 8) @compileError("private ExportRecord.offset offset");
    if (@offsetOf(ExportRecord, "version") != 12) @compileError("private ExportRecord.version offset");
    if (@offsetOf(SectionRecord, "name") != 0) @compileError("private SectionRecord.name offset");
    if (@offsetOf(SectionRecord, "flags") != 8) @compileError("private SectionRecord.flags offset");
    if (@offsetOf(SectionRecord, "file_off") != 12) @compileError("private SectionRecord.file_off offset");
    if (@offsetOf(SectionRecord, "file_size") != 16) @compileError("private SectionRecord.file_size offset");
    if (@offsetOf(SectionRecord, "mem_size") != 20) @compileError("private SectionRecord.mem_size offset");
    if (@offsetOf(SectionRecord, "alignment") != 24) @compileError("private SectionRecord.alignment offset");
    if (@offsetOf(EntryRecord, "kind") != 0) @compileError("private EntryRecord.kind offset");
    if (@offsetOf(EntryRecord, "section") != 4) @compileError("private EntryRecord.section offset");
    if (@offsetOf(EntryRecord, "offset") != 8) @compileError("private EntryRecord.offset offset");
    if (@offsetOf(EntryRecord, "flags") != 12) @compileError("private EntryRecord.flags offset");
    if (@offsetOf(ImportRecord, "module_offset") != 0) @compileError("private ImportRecord.module_offset offset");
    if (@offsetOf(ImportRecord, "symbol_offset") != 4) @compileError("private ImportRecord.symbol_offset offset");
    if (@offsetOf(ImportRecord, "min_version") != 8) @compileError("private ImportRecord.min_version offset");
    if (@offsetOf(ImportRecord, "flags") != 12) @compileError("private ImportRecord.flags offset");
    if (@offsetOf(RelocationRecord, "kind") != 0) @compileError("private RelocationRecord.kind offset");
    if (@offsetOf(RelocationRecord, "patch_section") != 4) @compileError("private RelocationRecord.patch_section offset");
    if (@offsetOf(RelocationRecord, "patch_offset") != 8) @compileError("private RelocationRecord.patch_offset offset");
    if (@offsetOf(RelocationRecord, "target_section") != 12) @compileError("private RelocationRecord.target_section offset");
    if (@offsetOf(RelocationRecord, "target_offset") != 16) @compileError("private RelocationRecord.target_offset offset");
    if (@offsetOf(RelocationRecord, "addend") != 20) @compileError("private RelocationRecord.addend offset");
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
