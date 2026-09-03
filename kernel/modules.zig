const vfs = @import("../fs/vfs.zig");
const heap = @import("../memory/heap.zig");
const k = @import("log.zig");
const loader_perf = @import("loader_perf.zig");
const module_file = @import("module_file.zig");
const module_r4m = @import("module_r4m.zig");
const r4x_api = @import("../program/r4x_api.zig");

const PAGE_SIZE: usize = 4096;
const MAX_MODULES: usize = 64;
const MAX_NAME: usize = 32;
const MAX_PATH: usize = 96;
const MAX_SECTIONS: usize = 16;
const MAX_PENDING_LIBS: usize = 32;
const MAX_IMPORTS_PER_MODULE: usize = 16;
const MAX_EXPORTS_PER_MODULE: usize = 16;
const MAX_R4M_METADATA_PROBE: usize = 2048;
const MAX_R4M_NAME_PROBE: usize = 128;
const ACTIVE_BOOT_GENERATION: u32 = 1;
const R4L_INTERFACE_MAGIC: u32 = 0x31493452;
const R4L_INTERFACE_HEADER_VERSION: u16 = 1;
const R4L_INTERFACE_HEADER_SIZE: usize = 32;
const R4L_QUERY_MAGIC: u32 = 0x314C3452;
const R4L_QUERY_VERSION: u32 = 1;
const R4L_QUERY_SIZE: usize = 32;

const R4M_HEADER_SIZE: usize = 64;
const R4M_SECTION_SIZE: usize = 32;
const R4M_ENTRY_SIZE: usize = 16;
const R4M_IMPORT_SIZE: usize = 16;
const R4M_EXPORT_SIZE: usize = 16;
const R4M_RELOCATION_SIZE: usize = 24;
const R4M_VERSION: u16 = 1;
const ARCH_X86_64: u16 = 1;

const R4M_SECTION_FLAG_BSS: u32 = 0x00000008;
const R4M_SECTION_FLAG_EXEC: u32 = 0x00000002;
const R4M_RELOC_ABS64: u32 = 1;
const R4M_RELOC_REL32: u32 = 2;
const R4M_RELOC_BASE_REL64: u32 = 3;
const R4M_RELOC_IMPORT_SLOT64: u32 = 4;

pub const Kind = module_r4m.Kind;

const platform_api_queries: [r4x_api.r4_platform_apis.len]r4x_api.R4LQuery = blk: {
    var queries: [r4x_api.r4_platform_apis.len]r4x_api.R4LQuery = undefined;
    for (r4x_api.r4_platform_apis, 0..) |meta, index| {
        queries[index] = .{
            .magic = r4x_api.r4l_abi_magic,
            .abi_version = r4x_api.r4l_abi_version,
            .size = r4x_api.r4l_query_struct_size,
            .group = @intFromEnum(meta.group),
            .kernel_bridge = 0,
            .reserved = 0,
        };
    }
    break :blk queries;
};

pub const State = enum(u8) {
    empty,
    builtin,
    loaded,
    failed,
};

pub const ResolverStatus = enum(u8) {
    none,
    builtin,
    resolved,
    failed,
};

const PendingState = enum(u8) {
    empty,
    pending,
    loading,
    loaded,
    failed,
};

const Header = module_r4m.Header;

const SectionHeader = struct {
    name: [8]u8,
    name_len: usize,
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
};

const Relocation = struct {
    kind: u32,
    patch_section: u32,
    patch_offset: u32,
    target_section: u32,
    target_offset: u32,
    addend: i32,
};

const ImportRecord = struct {
    module: []const u8,
    symbol: []const u8,
    min_version: u32,
    flags: u32,
};

const ResolvedImport = struct {
    module: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_len: usize = 0,
    symbol: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    symbol_len: usize = 0,
    version: u32 = 0,
    address: u64 = 0,
};

pub const Export = struct {
    used: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    version: u32 = 0,
    address: u64 = 0,
    section_index: u32 = 0,
    section_offset: u32 = 0,
};

const FileImport = struct {
    module: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_len: usize = 0,
    symbol: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    symbol_len: usize = 0,
    min_version: u32 = 0,
    flags: u32 = 0,

    fn view(self: *const FileImport) ImportRecord {
        return .{
            .module = self.module[0..self.module_len],
            .symbol = self.symbol[0..self.symbol_len],
            .min_version = self.min_version,
            .flags = self.flags,
        };
    }
};

const ValidatedFileTables = struct {
    header: Header,
    sections: [MAX_SECTIONS]SectionHeader = undefined,
    imports: [MAX_IMPORTS_PER_MODULE]FileImport = .{FileImport{}} ** MAX_IMPORTS_PER_MODULE,
    exports: [MAX_EXPORTS_PER_MODULE]Export = .{Export{}} ** MAX_EXPORTS_PER_MODULE,
    module_name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_name_len: usize = 0,

    fn name(self: *const ValidatedFileTables, fallback_name: []const u8) []const u8 {
        if (self.module_name_len != 0) return self.module_name[0..self.module_name_len];
        return fallbackModuleName(fallback_name);
    }
};

const PendingLibrary = struct {
    used: bool = false,
    state: PendingState = .empty,
    source: module_file.FileSource = undefined,
    file_size: usize = 0,
    file_name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    file_name_len: usize = 0,
    module_name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_name_len: usize = 0,
    path: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    path_len: usize = 0,
};

const PendingLibraryProbe = struct {
    module_name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_name_len: usize = 0,
};

pub const Section = struct {
    used: bool = false,
    name: [8]u8 = .{0} ** 8,
    name_len: usize = 0,
    flags: u32 = 0,
    file_size: u32 = 0,
    mem_size: u32 = 0,
    alignment: u32 = 0,
    runtime_offset: usize = 0,
    runtime_base: u64 = 0,
};

pub const Entry = struct {
    used: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    path: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    path_len: usize = 0,
    kind: Kind = .r4x,
    state: State = .empty,
    flags: u32 = 0,
    base: u64 = 0,
    size: usize = 0,
    section_count: u32 = 0,
    entry_count: u32 = 0,
    import_count: u32 = 0,
    export_count: u32 = 0,
    reloc_count: u32 = 0,
    reloc_abs64_count: u32 = 0,
    reloc_rel32_count: u32 = 0,
    reloc_base_rel64_count: u32 = 0,
    reloc_import_slot64_count: u32 = 0,
    resolved_import_count: u32 = 0,
    resolver_status: ResolverStatus = .none,
    bss_zeroed: bool = true,
    generation: u32 = 0,
    pinned: bool = false,
    sections: [MAX_SECTIONS]Section = .{Section{}} ** MAX_SECTIONS,
    exports: [MAX_EXPORTS_PER_MODULE]Export = .{Export{}} ** MAX_EXPORTS_PER_MODULE,
};

var entries: [MAX_MODULES]Entry = .{Entry{}} ** MAX_MODULES;
var unknown_relocation_errors: u32 = 0;
var relocation_range_errors: u32 = 0;
var unresolved_import_errors: u32 = 0;
var missing_dependency_errors: u32 = 0;
var symbol_dependency_errors: u32 = 0;
var version_dependency_errors: u32 = 0;
var cycle_dependency_errors: u32 = 0;
var duplicate_provider_errors: u32 = 0;
var libraries_generation_active: bool = false;

pub fn init() void {
    entries = .{Entry{}} ** MAX_MODULES;
    unknown_relocation_errors = 0;
    relocation_range_errors = 0;
    unresolved_import_errors = 0;
    missing_dependency_errors = 0;
    symbol_dependency_errors = 0;
    version_dependency_errors = 0;
    cycle_dependency_errors = 0;
    duplicate_provider_errors = 0;
    libraries_generation_active = false;
    registerPlatformApis();
}

pub fn loadSystemLibraries() void {
    if (libraries_generation_active) {
        k.puts("[MOD] active R4L generation is pinned; reload deferred until reboot\r\n");
        return;
    }
    const volume = vfs.volumeForDrive('C') orelse {
        k.puts("[MOD] C: FAT32 volume not mounted\r\n");
        return;
    };
    const dir_cluster = vfs.resolvePath(volume, "/R4OS/LIBS") orelse {
        k.puts("[MOD] /R4OS/LIBS not found\r\n");
        return;
    };
    libraries_generation_active = true;

    k.puts("[MOD] scanning /R4OS/LIBS\r\n");
    var pending: [MAX_PENDING_LIBS]PendingLibrary = .{PendingLibrary{}} ** MAX_PENDING_LIBS;
    defer freePendingLibraries(pending[0..]);

    var pending_count: usize = 0;
    var index: usize = 0;
    var name_buf: [64]u8 = .{0} ** 64;
    while (true) : (index += 1) {
        const scan_start = loader_perf.now();
        const maybe_entry = vfs.readDirectoryEntry(volume, dir_cluster, index, name_buf[0..]);
        loader_perf.addR4lScanTicks(scan_start);
        const file_entry = maybe_entry orelse break;
        loader_perf.recordR4lScanEntry();
        const file_name = zName(name_buf[0..]);
        if (file_entry.isDir() or !hasR4lExtension(file_name)) continue;
        loader_perf.recordR4lCandidate();
        if (pending_count >= pending.len) {
            k.puts("[MOD] pending library limit reached\r\n");
            break;
        }
        if (collectPendingLibrary(volume, file_entry, file_name, &pending[pending_count])) {
            pending_count += 1;
        }
    }

    markDuplicateProviders(pending[0..pending_count]);

    var loaded: usize = 0;
    var i: usize = 0;
    const resolve_start = loader_perf.now();
    while (i < pending_count) : (i += 1) {
        if (resolveAndLoadPending(pending[0..pending_count], i)) loaded += 1;
    }
    loader_perf.addR4lResolveTicks(resolve_start);
    loader_perf.recordR4lResults(loaded, pending_count);

    k.puts("[MOD] R4L loaded=");
    k.putDec(loaded);
    k.puts(" total=");
    k.putDec(countUsed());
    k.puts("\r\n");

    if (missing_dependency_errors != 0 or symbol_dependency_errors != 0 or version_dependency_errors != 0 or cycle_dependency_errors != 0 or duplicate_provider_errors != 0 or unresolved_import_errors != 0) {
        k.puts("[MOD] resolver errors missing=");
        k.putDec(missing_dependency_errors);
        k.puts(" symbol=");
        k.putDec(symbol_dependency_errors);
        k.puts(" version=");
        k.putDec(version_dependency_errors);
        k.puts(" cycle=");
        k.putDec(cycle_dependency_errors);
        k.puts(" duplicate=");
        k.putDec(duplicate_provider_errors);
        k.puts(" unresolved=");
        k.putDec(unresolved_import_errors);
        k.puts("\r\n");
    }
}

pub fn dumpSummary() void {
    var r4l: usize = 0;
    var r4x: usize = 0;
    var r4d: usize = 0;
    var r4p: usize = 0;
    var builtin: usize = 0;
    var failed: usize = 0;

    for (&entries) |*e| {
        if (!e.used) continue;
        switch (e.kind) {
            .r4l => r4l += 1,
            .r4x => r4x += 1,
            .r4d => r4d += 1,
            .r4p => r4p += 1,
            .platform_api_provider_reserved, .kernel_module_reserved => builtin += 1,
        }
        if (e.state == .failed or e.resolver_status == .failed) failed += 1;
    }

    k.puts("[MOD] summary total=");
    k.putDec(countUsed());
    k.puts(" r4l=");
    k.putDec(@intCast(r4l));
    k.puts(" r4x=");
    k.putDec(@intCast(r4x));
    k.puts(" r4d=");
    k.putDec(@intCast(r4d));
    k.puts(" r4p=");
    k.putDec(@intCast(r4p));
    k.puts(" builtin=");
    k.putDec(@intCast(builtin));
    k.puts(" failed=");
    k.putDec(@intCast(failed));
    k.puts("\r\n");

    if (unknown_relocation_errors != 0 or relocation_range_errors != 0) {
        k.puts("[MOD] relocation errors unknown=");
        k.putDec(unknown_relocation_errors);
        k.puts(" range=");
        k.putDec(relocation_range_errors);
        k.puts("\r\n");
    }
    if (missing_dependency_errors != 0 or symbol_dependency_errors != 0 or version_dependency_errors != 0 or cycle_dependency_errors != 0 or duplicate_provider_errors != 0 or unresolved_import_errors != 0) {
        k.puts("[MOD] resolver errors missing=");
        k.putDec(missing_dependency_errors);
        k.puts(" symbol=");
        k.putDec(symbol_dependency_errors);
        k.puts(" version=");
        k.putDec(version_dependency_errors);
        k.puts(" cycle=");
        k.putDec(cycle_dependency_errors);
        k.puts(" duplicate=");
        k.putDec(duplicate_provider_errors);
        k.puts(" unresolved=");
        k.putDec(unresolved_import_errors);
        k.puts("\r\n");
    }
}

pub fn countUsed() usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (entries[i].used) count += 1;
    }
    return count;
}

pub fn entryAt(index: usize) ?*const Entry {
    if (index >= entries.len or !entries[index].used) return null;
    return &entries[index];
}

pub fn platformApiGroupId(name: []const u8) ?u32 {
    for (r4x_api.r4_platform_apis) |meta| {
        if (nameEq(name, meta.name)) return @intFromEnum(meta.group);
    }
    return null;
}

fn registerPlatformApis() void {
    comptime {
        if (r4x_api.r4_platform_apis.len != 6) @compileError("expected exactly six built-in platform APIs");
    }
    for (r4x_api.r4_platform_apis, 0..) |meta, index| {
        const slot = freeSlot() orelse return;
        const e = &entries[slot];
        e.* = .{
            .used = true,
            .kind = .platform_api_provider_reserved,
            .state = .builtin,
            .resolver_status = .builtin,
            .export_count = 1,
            .generation = ACTIVE_BOOT_GENERATION,
            .pinned = true,
        };
        e.name_len = copyBytes(meta.name, e.name[0..]);
        e.path_len = copyBytes("builtin:", e.path[0..]);
        e.path_len += copyBytes(meta.name, e.path[e.path_len..]);
        e.exports[0] = .{
            .used = true,
            .version = 1,
            .address = @intFromPtr(&platform_api_queries[index]),
            .section_index = 0,
            .section_offset = 0,
        };
        e.exports[0].name_len = copyBytes("Query", e.exports[0].name[0..]);
    }
}

fn collectPendingLibrary(volume: vfs.Volume, file_entry: vfs.Entry, file_name: []const u8, out: *PendingLibrary) bool {
    if (file_entry.size == 0) return false;
    const probe = readPendingLibraryProbe(volume, file_entry, file_name) orelse return false;
    const probe_module_name = probe.module_name[0..probe.module_name_len];
    if (findByName(probe_module_name) != null) {
        duplicate_provider_errors += 1;
        k.puts("[MOD] resolver duplicate provider module=");
        k.puts(probe_module_name);
        k.puts("\r\n");
        return false;
    }

    var path_buf: [MAX_PATH]u8 = .{0} ** MAX_PATH;
    const path = buildLibraryPath(file_name, &path_buf);

    out.* = .{
        .used = true,
        .state = .pending,
        .source = .{
            .volume = volume,
            .entry = file_entry,
            .drive_letter = 'C',
        },
        .file_size = @intCast(file_entry.size),
    };
    out.file_name_len = copyBytes(file_name, out.file_name[0..]);
    out.module_name_len = copyBytes(probe_module_name, out.module_name[0..]);
    out.path_len = copyBytes(path, out.path[0..]);
    return true;
}

fn readPendingLibraryProbe(volume: vfs.Volume, file_entry: vfs.Entry, file_name: []const u8) ?PendingLibraryProbe {
    var meta_buf: [MAX_R4M_METADATA_PROBE]u8 = .{0} ** MAX_R4M_METADATA_PROBE;
    const source = module_file.FileSource{
        .volume = volume,
        .entry = file_entry,
        .drive_letter = 'C',
    };
    var reader = module_r4m.Reader.init(source, @intCast(file_entry.size));
    const header = reader.readHeader(.r4l, .{
        .max_sections = MAX_SECTIONS,
        .max_imports = MAX_IMPORTS_PER_MODULE,
        .max_exports = MAX_EXPORTS_PER_MODULE,
    }, "r4l-metadata-probe", true) orelse return null;
    const meta = reader.readMetadata(header, meta_buf[0..], "r4l-metadata-probe", true) orelse return null;
    const module_name = module_r4m.firstMetadataItem(meta) orelse fallbackModuleName(file_name);
    if (!validModuleName(module_name)) {
        k.puts("[MOD] invalid R4L module name file=");
        k.puts(file_name);
        k.puts("\r\n");
        return null;
    }
    var probe = PendingLibraryProbe{};
    probe.module_name_len = copyBytes(module_name, probe.module_name[0..]);
    return probe;
}

fn freePendingLibraries(pending: []PendingLibrary) void {
    var i: usize = 0;
    while (i < pending.len) : (i += 1) {
        if (pending[i].used) pending[i] = PendingLibrary{};
    }
}

fn markDuplicateProviders(pending: []PendingLibrary) void {
    var i: usize = 0;
    while (i < pending.len) : (i += 1) {
        if (!pending[i].used or pending[i].state == .failed) continue;
        var j = i + 1;
        while (j < pending.len) : (j += 1) {
            if (!pending[j].used or pending[j].state == .failed) continue;
            if (nameEq(pending[i].module_name[0..pending[i].module_name_len], pending[j].module_name[0..pending[j].module_name_len])) {
                duplicate_provider_errors += 1;
                pending[i].state = .failed;
                pending[j].state = .failed;
                k.puts("[MOD] resolver duplicate provider module=");
                k.puts(pending[i].module_name[0..pending[i].module_name_len]);
                k.puts("\r\n");
            }
        }
    }
}

fn resolveAndLoadPending(pending: []PendingLibrary, index: usize) bool {
    if (index >= pending.len or !pending[index].used) return false;
    var p = &pending[index];
    return switch (p.state) {
        .loaded => true,
        .failed, .empty => false,
        .loading => blk: {
            cycle_dependency_errors += 1;
            p.state = .failed;
            k.puts("[MOD] resolver cycle at ");
            k.puts(p.module_name[0..p.module_name_len]);
            k.puts("\r\n");
            break :blk false;
        },
        .pending => blk: {
            p.state = .loading;
            const tables = readValidatedFileTables(p.source, p.file_size, .r4l, p.file_name[0..p.file_name_len]) orelse {
                p.state = .failed;
                break :blk false;
            };
            var resolved_buf: [MAX_IMPORTS_PER_MODULE]ResolvedImport = .{ResolvedImport{}} ** MAX_IMPORTS_PER_MODULE;
            if (tables.header.import_count > resolved_buf.len) {
                p.state = .failed;
                unresolved_import_errors += 1;
                break :blk false;
            }

            var import_index: usize = 0;
            while (import_index < tables.header.import_count) : (import_index += 1) {
                const import = tables.imports[import_index].view();
                const resolved = resolveImport(pending, index, import) orelse {
                    if (p.state == .loading) p.state = .failed;
                    break :blk false;
                };
                resolved_buf[import_index] = resolved;
            }

            if (p.state != .loading) break :blk false;
            const load_start = loader_perf.now();
            if (loadR4MFile(
                p.source,
                p.file_size,
                &tables,
                .r4l,
                p.file_name[0..p.file_name_len],
                p.path[0..p.path_len],
                resolved_buf[0..@intCast(tables.header.import_count)],
            ) != null) {
                loader_perf.addR4lLoadTicks(load_start);
                p.state = .loaded;
                break :blk true;
            }
            loader_perf.addR4lLoadTicks(load_start);
            p.state = .failed;
            break :blk false;
        },
    };
}

pub fn loadResolvedBytes(bytes: []const u8, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?usize {
    const header = parseHeader(bytes) orelse {
        k.puts("[MOD] bad R4M0 header: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    const kind = kindFromRaw(header.kind_raw) orelse {
        k.puts("[MOD] unknown ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    if (!module_r4m.isLoadableContainerKind(kind) or !module_r4m.isLoadableContainerKind(expected_kind)) {
        k.puts("[MOD] reserved ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    if (kind != expected_kind) {
        k.puts("[MOD] unexpected ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    if (!validateTables(header, bytes)) {
        k.puts("[MOD] invalid R4M0 tables: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }

    const importer_name = moduleName(bytes, header) orelse fallbackModuleName(fallback_name);

    var resolved_buf: [MAX_IMPORTS_PER_MODULE]ResolvedImport = .{ResolvedImport{}} ** MAX_IMPORTS_PER_MODULE;
    if (header.import_count > resolved_buf.len) {
        unresolved_import_errors += 1;
        k.puts("[MOD] too many imports importer=");
        k.puts(importer_name);
        k.puts("\r\n");
        return null;
    }

    var import_index: usize = 0;
    while (import_index < header.import_count) : (import_index += 1) {
        const import = readImport(bytes, header, import_index) orelse {
            unresolved_import_errors += 1;
            k.puts("[MOD] bad import importer=");
            k.puts(importer_name);
            k.puts("\r\n");
            return null;
        };
        const provider_slot = findByName(import.module) orelse {
            missing_dependency_errors += 1;
            k.puts("[MOD] resolver missing module=");
            k.puts(import.module);
            k.puts(" importer=");
            k.puts(importer_name);
            k.puts("\r\n");
            return null;
        };
        const resolved = resolveImportFromEntry(&entries[provider_slot], import, importer_name) orelse return null;
        resolved_buf[import_index] = resolved;
    }

    return loadR4M(bytes, expected_kind, fallback_name, path, resolved_buf[0..@intCast(header.import_count)]);
}

pub fn loadResolvedFile(source: module_file.FileSource, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?usize {
    if (!module_r4m.isLoadableContainerKind(expected_kind)) {
        k.puts("[MOD] reserved ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    const file_size: usize = @intCast(source.entry.size);
    const tables = readValidatedFileTables(source, file_size, expected_kind, fallback_name) orelse {
        k.puts("[MOD] bad R4M0 header: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    const header = tables.header;
    const kind = header.kind() orelse {
        k.puts("[MOD] unknown ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    if (kind != expected_kind) {
        k.puts("[MOD] unexpected ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    const importer_name = tables.name(fallback_name);

    var resolved_buf: [MAX_IMPORTS_PER_MODULE]ResolvedImport = .{ResolvedImport{}} ** MAX_IMPORTS_PER_MODULE;
    if (header.import_count > resolved_buf.len) {
        unresolved_import_errors += 1;
        k.puts("[MOD] too many imports importer=");
        k.puts(importer_name);
        k.puts("\r\n");
        return null;
    }

    var import_index: usize = 0;
    while (import_index < header.import_count) : (import_index += 1) {
        const import = tables.imports[import_index].view();
        const provider_slot = findByName(import.module) orelse {
            missing_dependency_errors += 1;
            k.puts("[MOD] resolver missing module=");
            k.puts(import.module);
            k.puts(" importer=");
            k.puts(importer_name);
            k.puts("\r\n");
            return null;
        };
        const resolved = resolveImportFromEntry(&entries[provider_slot], import, importer_name) orelse return null;
        resolved_buf[import_index] = resolved;
    }

    return loadR4MFile(source, file_size, &tables, expected_kind, fallback_name, path, resolved_buf[0..@intCast(header.import_count)]);
}

pub fn exportAddress(slot: usize, symbol: []const u8, min_version: u32) ?u64 {
    const info = exportInfo(slot, symbol, min_version) orelse return null;
    return info.address;
}

pub const ExportInfo = struct {
    address: u64,
    version: u32,
    available_size: u32,
    generation: u32,
    module_slot: u8,
    kind: Kind,
};

pub fn exportInfo(slot: usize, symbol: []const u8, min_version: u32) ?ExportInfo {
    if (slot >= entries.len or !entries[slot].used) return null;
    const exp = findExport(&entries[slot], symbol) orelse return null;
    if (exp.version < min_version) return null;
    if (entries[slot].kind == .platform_api_provider_reserved) return .{
        .address = exp.address,
        .version = exp.version,
        .available_size = r4x_api.r4l_query_struct_size,
        .generation = entries[slot].generation,
        .module_slot = @intCast(slot),
        .kind = entries[slot].kind,
    };
    if (exp.section_index >= entries[slot].section_count or exp.section_index >= entries[slot].sections.len) return null;
    const section = entries[slot].sections[@intCast(exp.section_index)];
    if (!section.used or exp.section_offset >= section.mem_size) return null;
    return .{
        .address = exp.address,
        .version = exp.version,
        .available_size = section.mem_size - exp.section_offset,
        .generation = entries[slot].generation,
        .module_slot = @intCast(slot),
        .kind = entries[slot].kind,
    };
}

// IRQ-time executable ownership lookup. Runtime R4Ls are boot-generation
// pinned today, but callers retain the exact slot+generation so a later
// unload/reload implementation cannot accidentally make a stale program
// binding preemptible. Only explicitly executable sections qualify; data,
// BSS, other module kinds and overflowed ranges fail closed.
pub fn isExecutableAddress(module_slot: u8, generation: u32, address: u64) bool {
    const slot: usize = module_slot;
    if (slot >= entries.len or generation == 0) return false;
    const entry = &entries[slot];
    if (!entry.used or entry.state != .loaded or entry.kind != .r4l or entry.generation != generation) return false;

    const section_count: usize = @min(@as(usize, @intCast(entry.section_count)), entry.sections.len);
    for (entry.sections[0..section_count]) |section| {
        if (!section.used or (section.flags & R4M_SECTION_FLAG_EXEC) == 0 or section.mem_size == 0) continue;
        const end = section.runtime_base +% @as(u64, section.mem_size);
        if (end < section.runtime_base) continue;
        if (address >= section.runtime_base and address < end) return true;
    }
    return false;
}

pub fn resolveExportAddress(module_name: []const u8, symbol: []const u8, min_version: u32) ?u64 {
    const info = resolveExportInfo(module_name, symbol, min_version) orelse return null;
    return info.address;
}

pub fn resolveExportInfo(module_name: []const u8, symbol: []const u8, min_version: u32) ?ExportInfo {
    const slot = findByName(module_name) orelse return null;
    return exportInfo(slot, symbol, min_version);
}

fn resolveImport(pending: []PendingLibrary, importer_index: usize, import: ImportRecord) ?ResolvedImport {
    const importer_name = pending[importer_index].module_name[0..pending[importer_index].module_name_len];
    if (findByName(import.module)) |slot| {
        return resolveImportFromEntry(&entries[slot], import, importer_name);
    }

    const provider_index = findPendingByName(pending, import.module) orelse {
        missing_dependency_errors += 1;
        k.puts("[MOD] resolver missing module=");
        k.puts(import.module);
        k.puts(" importer=");
        k.puts(importer_name);
        k.puts("\r\n");
        return null;
    };

    if (pending[provider_index].state == .loading) {
        cycle_dependency_errors += 1;
        pending[provider_index].state = .failed;
        k.puts("[MOD] resolver cycle importer=");
        k.puts(importer_name);
        k.puts(" provider=");
        k.puts(import.module);
        k.puts("\r\n");
        return null;
    }
    if (!resolveAndLoadPending(pending, provider_index)) return null;
    if (findByName(import.module)) |slot| {
        return resolveImportFromEntry(&entries[slot], import, importer_name);
    }
    unresolved_import_errors += 1;
    return null;
}

fn resolveImportFromEntry(provider: *const Entry, import: ImportRecord, importer_name: []const u8) ?ResolvedImport {
    const exp = findExport(provider, import.symbol) orelse {
        symbol_dependency_errors += 1;
        k.puts("[MOD] resolver missing export module=");
        k.puts(provider.name[0..provider.name_len]);
        k.puts(" symbol=");
        k.puts(import.symbol);
        k.puts(" importer=");
        k.puts(importer_name);
        k.puts("\r\n");
        return null;
    };
    if (exp.version < import.min_version) {
        version_dependency_errors += 1;
        k.puts("[MOD] resolver version reject module=");
        k.puts(provider.name[0..provider.name_len]);
        k.puts(" symbol=");
        k.puts(import.symbol);
        k.puts(" have=");
        k.putDec(exp.version);
        k.puts(" need=");
        k.putDec(import.min_version);
        k.puts(" importer=");
        k.puts(importer_name);
        k.puts("\r\n");
        return null;
    }

    var resolved: ResolvedImport = .{};
    resolved.module_len = copyBytes(provider.name[0..provider.name_len], resolved.module[0..]);
    resolved.symbol_len = copyBytes(exp.name[0..exp.name_len], resolved.symbol[0..]);
    resolved.version = exp.version;
    resolved.address = exp.address;
    return resolved;
}

fn loadR4MFile(source: module_file.FileSource, file_size: usize, tables: *const ValidatedFileTables, expected_kind: Kind, fallback_name: []const u8, path: []const u8, resolved_imports: []const ResolvedImport) ?usize {
    const header = tables.header;
    const kind = header.kind() orelse {
        k.puts("[MOD] unknown ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    if (kind != expected_kind) {
        k.puts("[MOD] unexpected ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    const section_headers = tables.sections[0..@intCast(header.section_count)];

    var section_offsets: [MAX_SECTIONS]usize = .{0} ** MAX_SECTIONS;
    const image_size = layoutSections(section_headers, section_offsets[0..]) orelse {
        k.puts("[MOD] R4M0 image layout failed: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    const image = heap.alloc(image_size, PAGE_SIZE) orelse {
        k.puts("[MOD] image allocation failed: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    @memset(image, 0);

    const slot = freeSlot() orelse {
        k.puts("[MOD] registry full\r\n");
        _ = heap.free(image);
        return null;
    };

    const module_name = tables.name(fallback_name);
    var bss_zeroed = true;
    var sections: [MAX_SECTIONS]Section = .{Section{}} ** MAX_SECTIONS;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const sh = section_headers[i];
        const dst_off = section_offsets[i];
        const mem_size: usize = @intCast(sh.mem_size);
        const file_bytes: usize = @intCast(sh.file_size);
        if (file_bytes != 0 and !module_file.readExact(.{
            .source = source,
            .offset = @intCast(sh.file_off),
            .out = image[dst_off .. dst_off + file_bytes],
            .name = "r4m-section-image",
        })) {
            _ = heap.free(image);
            return null;
        }
        if ((sh.flags & R4M_SECTION_FLAG_BSS) != 0 and !rangeIsZero(image[dst_off .. dst_off + mem_size])) {
            bss_zeroed = false;
        }
        sections[i] = .{
            .used = true,
            .name = sh.name,
            .name_len = sh.name_len,
            .flags = sh.flags,
            .file_size = sh.file_size,
            .mem_size = sh.mem_size,
            .alignment = sh.alignment,
            .runtime_offset = dst_off,
            .runtime_base = @intFromPtr(image.ptr) + @as(u64, @intCast(dst_off)),
        };
    }

    var reader = module_r4m.Reader.init(source, file_size);
    const reloc_stats = applyRelocationsFromFile(&reader, header, section_headers, section_offsets[0..], image, fallback_name, resolved_imports) orelse {
        _ = heap.free(image);
        return null;
    };
    if (kind == .r4l and !validateLoadedExportsFromTables(tables, section_headers, section_offsets[0..], image, fallback_name)) {
        _ = heap.free(image);
        return null;
    }

    const e = &entries[slot];
    e.* = .{
        .used = true,
        .kind = kind,
        .state = .loaded,
        .flags = header.flags,
        .base = @intFromPtr(image.ptr),
        .size = image.len,
        .section_count = header.section_count,
        .entry_count = header.entry_count,
        .import_count = header.import_count,
        .export_count = header.export_count,
        .reloc_count = header.reloc_count,
        .reloc_abs64_count = reloc_stats.abs64,
        .reloc_rel32_count = reloc_stats.rel32,
        .reloc_base_rel64_count = reloc_stats.base_rel64,
        .reloc_import_slot64_count = reloc_stats.import_slot64,
        .resolved_import_count = @intCast(resolved_imports.len),
        .resolver_status = .resolved,
        .bss_zeroed = bss_zeroed,
        .generation = ACTIVE_BOOT_GENERATION,
        .pinned = kind == .r4l,
        .sections = sections,
    };
    e.name_len = copyBytes(module_name, e.name[0..]);
    e.path_len = copyBytes(path, e.path[0..]);
    fillExportsFromTables(e, tables);

    k.puts("[MOD] load ");
    k.puts(e.name[0..e.name_len]);
    k.puts(" kind=");
    k.puts(kindName(kind));
    k.puts(" sections=");
    k.putDec(e.section_count);
    k.puts(" image=0x");
    k.putHex(e.base, 16);
    k.puts(" size=");
    k.putDec(@intCast(e.size));
    k.puts(" bss=");
    k.puts(if (bss_zeroed) "zero" else "bad");
    if (e.pinned) {
        k.puts(" generation=");
        k.putDec(e.generation);
        k.puts(" pinned");
    }
    if (header.import_count != 0) {
        k.puts(" imports_resolved=");
        k.putDec(e.resolved_import_count);
    }
    if (header.reloc_count != 0) {
        k.puts(" relocs=");
        k.putDec(header.reloc_count);
    }
    k.puts("\r\n");
    if (!bss_zeroed) return null;
    return slot;
}

fn loadR4M(bytes: []const u8, expected_kind: Kind, fallback_name: []const u8, path: []const u8, resolved_imports: []const ResolvedImport) ?usize {
    const header = parseHeader(bytes) orelse {
        k.puts("[MOD] bad R4M0 header: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    const kind = kindFromRaw(header.kind_raw) orelse {
        k.puts("[MOD] unknown ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    if (kind != expected_kind) {
        k.puts("[MOD] unexpected ModuleKind: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }
    if (!validateTables(header, bytes)) {
        k.puts("[MOD] invalid R4M0 tables: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }

    var section_headers: [MAX_SECTIONS]SectionHeader = undefined;
    if (!readSections(header, bytes, section_headers[0..])) {
        k.puts("[MOD] invalid R4M0 sections: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    }

    var section_offsets: [MAX_SECTIONS]usize = .{0} ** MAX_SECTIONS;
    const image_size = layoutSections(section_headers[0..@intCast(header.section_count)], section_offsets[0..]) orelse {
        k.puts("[MOD] R4M0 image layout failed: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    const image = heap.alloc(image_size, PAGE_SIZE) orelse {
        k.puts("[MOD] image allocation failed: ");
        k.puts(fallback_name);
        k.puts("\r\n");
        return null;
    };
    @memset(image, 0);

    const slot = freeSlot() orelse {
        k.puts("[MOD] registry full\r\n");
        _ = heap.free(image);
        return null;
    };

    const module_name = moduleName(bytes, header) orelse fallbackModuleName(fallback_name);
    var bss_zeroed = true;
    var sections: [MAX_SECTIONS]Section = .{Section{}} ** MAX_SECTIONS;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const sh = section_headers[i];
        const dst_off = section_offsets[i];
        const mem_size: usize = @intCast(sh.mem_size);
        const file_size: usize = @intCast(sh.file_size);
        if (file_size != 0) {
            const src_off: usize = @intCast(sh.file_off);
            @memcpy(image[dst_off .. dst_off + file_size], bytes[src_off .. src_off + file_size]);
        }
        if ((sh.flags & R4M_SECTION_FLAG_BSS) != 0 and !rangeIsZero(image[dst_off .. dst_off + mem_size])) {
            bss_zeroed = false;
        }
        sections[i] = .{
            .used = true,
            .name = sh.name,
            .name_len = sh.name_len,
            .flags = sh.flags,
            .file_size = sh.file_size,
            .mem_size = sh.mem_size,
            .alignment = sh.alignment,
            .runtime_offset = dst_off,
            .runtime_base = @intFromPtr(image.ptr) + @as(u64, @intCast(dst_off)),
        };
    }

    const reloc_stats = applyRelocations(bytes, header, section_headers[0..@intCast(header.section_count)], section_offsets[0..], image, fallback_name, resolved_imports) orelse {
        _ = heap.free(image);
        return null;
    };
    if (kind == .r4l and !validateLoadedExports(bytes, header, section_headers[0..@intCast(header.section_count)], section_offsets[0..], image, fallback_name)) {
        _ = heap.free(image);
        return null;
    }

    const e = &entries[slot];
    e.* = .{
        .used = true,
        .kind = kind,
        .state = .loaded,
        .flags = header.flags,
        .base = @intFromPtr(image.ptr),
        .size = image.len,
        .section_count = header.section_count,
        .entry_count = header.entry_count,
        .import_count = header.import_count,
        .export_count = header.export_count,
        .reloc_count = header.reloc_count,
        .reloc_abs64_count = reloc_stats.abs64,
        .reloc_rel32_count = reloc_stats.rel32,
        .reloc_base_rel64_count = reloc_stats.base_rel64,
        .reloc_import_slot64_count = reloc_stats.import_slot64,
        .resolved_import_count = @intCast(resolved_imports.len),
        .resolver_status = .resolved,
        .bss_zeroed = bss_zeroed,
        .generation = ACTIVE_BOOT_GENERATION,
        .pinned = kind == .r4l,
        .sections = sections,
    };
    e.name_len = copyBytes(module_name, e.name[0..]);
    e.path_len = copyBytes(path, e.path[0..]);
    fillExports(e, bytes, header);

    k.puts("[MOD] load ");
    k.puts(e.name[0..e.name_len]);
    k.puts(" kind=");
    k.puts(kindName(kind));
    k.puts(" sections=");
    k.putDec(e.section_count);
    k.puts(" image=0x");
    k.putHex(e.base, 16);
    k.puts(" size=");
    k.putDec(@intCast(e.size));
    k.puts(" bss=");
    k.puts(if (bss_zeroed) "zero" else "bad");
    if (e.pinned) {
        k.puts(" generation=");
        k.putDec(e.generation);
        k.puts(" pinned");
    }
    if (header.import_count != 0) {
        k.puts(" imports_resolved=");
        k.putDec(e.resolved_import_count);
    }
    if (header.reloc_count != 0) {
        k.puts(" relocs=");
        k.putDec(header.reloc_count);
    }
    k.puts("\r\n");
    if (!bss_zeroed) return null;
    return slot;
}

fn parseHeader(bytes: []const u8) ?Header {
    if (bytes.len < R4M_HEADER_SIZE) return null;
    if (!memEql(bytes[0..4], "R4M0")) return null;
    return .{
        .version = readLe16(bytes[4..6]),
        .arch = readLe16(bytes[6..8]),
        .kind_raw = readLe16(bytes[8..10]),
        .header_size = readLe16(bytes[10..12]),
        .flags = readLe32(bytes[12..16]),
        .section_off = readLe32(bytes[16..20]),
        .section_count = readLe32(bytes[20..24]),
        .import_off = readLe32(bytes[24..28]),
        .import_count = readLe32(bytes[28..32]),
        .export_off = readLe32(bytes[32..36]),
        .export_count = readLe32(bytes[36..40]),
        .reloc_off = readLe32(bytes[40..44]),
        .reloc_count = readLe32(bytes[44..48]),
        .entry_off = readLe32(bytes[48..52]),
        .entry_count = readLe32(bytes[52..56]),
        .meta_off = readLe32(bytes[56..60]),
        .meta_size = readLe32(bytes[60..64]),
    };
}

fn validateTables(header: Header, bytes: []const u8) bool {
    if (header.version != R4M_VERSION or header.arch != ARCH_X86_64) return false;
    if (header.header_size != R4M_HEADER_SIZE) return false;
    if (header.section_count == 0 or header.section_count > MAX_SECTIONS) return false;
    if (header.import_count > MAX_IMPORTS_PER_MODULE or header.export_count > MAX_EXPORTS_PER_MODULE) return false;
    if (!checkTable(bytes.len, header.section_off, header.section_count, R4M_SECTION_SIZE, true)) return false;
    if (!checkTable(bytes.len, header.entry_off, header.entry_count, R4M_ENTRY_SIZE, true)) return false;
    if (!checkTable(bytes.len, header.import_off, header.import_count, R4M_IMPORT_SIZE, false)) return false;
    if (!checkTable(bytes.len, header.export_off, header.export_count, R4M_EXPORT_SIZE, false)) return false;
    if (!checkTable(bytes.len, header.reloc_off, header.reloc_count, R4M_RELOCATION_SIZE, false)) return false;
    if (header.meta_size != 0 and !checkRange(bytes.len, header.meta_off, header.meta_size)) return false;
    if (!validateEntries(header, bytes)) return false;
    if (!validateImports(header, bytes)) return false;
    if (!validateExports(header, bytes)) return false;
    return true;
}

fn readSections(header: Header, bytes: []const u8, out: []SectionHeader) bool {
    if (header.section_count > out.len) return false;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const off = @as(usize, @intCast(header.section_off)) + i * R4M_SECTION_SIZE;
        var name: [8]u8 = .{0} ** 8;
        @memcpy(name[0..], bytes[off .. off + 8]);
        const name_len = fixedNameLen(name[0..]);
        const file_off = readLe32(bytes[off + 12 .. off + 16]);
        const file_size = readLe32(bytes[off + 16 .. off + 20]);
        const mem_size = readLe32(bytes[off + 20 .. off + 24]);
        const alignment = readLe32(bytes[off + 24 .. off + 28]);
        if (name_len == 0 or mem_size < file_size) return false;
        if (alignment == 0 or !isPowerOfTwo(alignment)) return false;
        if (file_size != 0 and !checkRange(bytes.len, file_off, file_size)) return false;
        out[i] = .{
            .name = name,
            .name_len = name_len,
            .flags = readLe32(bytes[off + 8 .. off + 12]),
            .file_off = file_off,
            .file_size = file_size,
            .mem_size = mem_size,
            .alignment = alignment,
        };
    }
    return true;
}

fn readValidatedFileTables(source: module_file.FileSource, file_size: usize, expected_kind: Kind, fallback_name: []const u8) ?ValidatedFileTables {
    var reader = module_r4m.Reader.init(source, file_size);
    const header = reader.readHeader(expected_kind, .{
        .max_sections = MAX_SECTIONS,
        .max_imports = MAX_IMPORTS_PER_MODULE,
        .max_exports = MAX_EXPORTS_PER_MODULE,
    }, "r4m-module-header", true) orelse return null;
    var tables = ValidatedFileTables{ .header = header };
    if (!readSectionsFromReader(&reader, file_size, header, tables.sections[0..])) return null;
    const sections = tables.sections[0..@intCast(header.section_count)];

    var entry_index: usize = 0;
    while (entry_index < header.entry_count) : (entry_index += 1) {
        const entry = reader.readEntryRecord(header, entry_index, "r4m-entry-table", true) orelse return null;
        if (entry.section >= sections.len) return null;
        if (entry.offset >= sections[@intCast(entry.section)].mem_size) return null;
    }

    var import_index: usize = 0;
    while (import_index < header.import_count) : (import_index += 1) {
        const record = reader.readImportRecord(header, import_index, "r4m-import-table", true) orelse return null;
        var module_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        var symbol_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        const module_name = reader.readZString(record.module_offset, module_buf[0..], "r4m-import-module", true) orelse return null;
        const symbol_name = reader.readZString(record.symbol_offset, symbol_buf[0..], "r4m-import-symbol", true) orelse return null;
        if (!validImportModuleName(module_name) or !validSymbolName(symbol_name) or record.min_version == 0) {
            k.puts("[MOD] malformed import module=");
            k.puts(module_name);
            k.puts(" symbol=");
            k.puts(symbol_name);
            k.puts(" importer=");
            k.puts(fallback_name);
            k.puts("\r\n");
            return null;
        }
        var file_import = &tables.imports[import_index];
        file_import.module_len = copyBytes(module_name, file_import.module[0..]);
        file_import.symbol_len = copyBytes(symbol_name, file_import.symbol[0..]);
        file_import.min_version = record.min_version;
        file_import.flags = record.flags;
    }

    var export_index: usize = 0;
    while (export_index < header.export_count) : (export_index += 1) {
        const record = reader.readExportRecord(header, export_index, "r4m-export-table", true) orelse return null;
        var name_buf: [MAX_R4M_NAME_PROBE]u8 = .{0} ** MAX_R4M_NAME_PROBE;
        const export_name = reader.readZString(record.name_offset, name_buf[0..], "r4m-export-name", true) orelse return null;
        if (!validSymbolName(export_name) or record.version == 0) {
            k.puts("[MOD] malformed export module=");
            k.puts(fallback_name);
            k.puts(" symbol=");
            k.puts(export_name);
            k.puts("\r\n");
            return null;
        }
        if (record.section >= sections.len or record.offset >= sections[@intCast(record.section)].mem_size) return null;
        var prior_index: usize = 0;
        while (prior_index < export_index) : (prior_index += 1) {
            const prior = &tables.exports[prior_index];
            if (nameEq(export_name, prior.name[0..prior.name_len])) {
                k.puts("[MOD] duplicate export module=");
                k.puts(fallback_name);
                k.puts(" symbol=");
                k.puts(export_name);
                k.puts("\r\n");
                return null;
            }
        }
        var planned_export = &tables.exports[export_index];
        planned_export.used = true;
        planned_export.name_len = copyBytes(export_name, planned_export.name[0..]);
        planned_export.version = record.version;
        planned_export.section_index = record.section;
        planned_export.section_offset = record.offset;
    }

    if (header.meta_size <= MAX_R4M_METADATA_PROBE) {
        var meta_buf: [MAX_R4M_METADATA_PROBE]u8 = .{0} ** MAX_R4M_METADATA_PROBE;
        if (reader.readMetadata(header, meta_buf[0..], "r4m-module-name", true)) |meta| {
            if (module_r4m.firstMetadataItem(meta)) |module_name| {
                tables.module_name_len = copyBytes(module_name, tables.module_name[0..]);
            }
        }
    }
    return tables;
}

fn readSectionsFromReader(reader: *module_r4m.Reader, file_size: usize, header: Header, out: []SectionHeader) bool {
    if (header.section_count > out.len) return false;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const record = reader.readSectionRecord(header, i, "r4m-section-table", true) orelse return false;
        const name_len = fixedNameLen(record.name[0..]);
        if (name_len == 0 or record.mem_size < record.file_size) return false;
        if (record.alignment == 0 or !isPowerOfTwo(record.alignment)) return false;
        if (record.file_size != 0 and !checkRange(file_size, record.file_off, record.file_size)) return false;
        out[i] = .{
            .name = record.name,
            .name_len = name_len,
            .flags = record.flags,
            .file_off = record.file_off,
            .file_size = record.file_size,
            .mem_size = record.mem_size,
            .alignment = record.alignment,
        };
    }
    return true;
}

fn validateLoadedExportsFromTables(
    tables: *const ValidatedFileTables,
    sections: []const SectionHeader,
    section_offsets: []const usize,
    image: []const u8,
    module_name: []const u8,
) bool {
    var i: usize = 0;
    while (i < tables.header.export_count) : (i += 1) {
        const exp = &tables.exports[i];
        if (!validateLoadedExport(exp.name[0..exp.name_len], exp.version, exp.section_index, exp.section_offset, sections, section_offsets, image, module_name)) return false;
    }
    return true;
}

fn validateLoadedExports(
    bytes: []const u8,
    header: Header,
    sections: []const SectionHeader,
    section_offsets: []const usize,
    image: []const u8,
    module_name: []const u8,
) bool {
    var i: usize = 0;
    while (i < header.export_count) : (i += 1) {
        const off = @as(usize, @intCast(header.export_off)) + i * R4M_EXPORT_SIZE;
        const name = zString(bytes, readLe32(bytes[off + 0 .. off + 4])) orelse return false;
        if (!validateLoadedExport(
            name,
            readLe32(bytes[off + 12 .. off + 16]),
            readLe32(bytes[off + 4 .. off + 8]),
            readLe32(bytes[off + 8 .. off + 12]),
            sections,
            section_offsets,
            image,
            module_name,
        )) return false;
    }
    return true;
}

fn validateLoadedExport(
    name: []const u8,
    export_version: u32,
    section_index: u32,
    export_offset: u32,
    sections: []const SectionHeader,
    section_offsets: []const usize,
    image: []const u8,
    module_name: []const u8,
) bool {
    const is_query = nameEq(name, "Query");
    const has_interface_prefix = interfaceExportPrefix(name);
    if (!is_query and !has_interface_prefix) return true;
    if (section_index >= sections.len or section_index >= section_offsets.len) {
        return rejectLoadedExport(module_name, name, "section");
    }
    const section = sections[@intCast(section_index)];
    if (export_offset >= section.mem_size) return rejectLoadedExport(module_name, name, "offset");
    const runtime_offset = section_offsets[@intCast(section_index)] + @as(usize, @intCast(export_offset));
    const available: usize = @intCast(section.mem_size - export_offset);
    if (runtime_offset > image.len or available > image.len - runtime_offset) {
        return rejectLoadedExport(module_name, name, "range");
    }
    const table = image[runtime_offset .. runtime_offset + available];
    if ((@intFromPtr(table.ptr) & 7) != 0) return rejectLoadedExport(module_name, name, "alignment");

    if (is_query) return validateLoadedQuery(table, export_version, module_name, name);
    const abi_major = interfaceMajorFromName(name) orelse return rejectLoadedExport(module_name, name, "major-name");
    return validateLoadedInterface(table, export_version, abi_major, module_name, name);
}

fn validateLoadedQuery(table: []const u8, export_version: u32, module_name: []const u8, symbol_name: []const u8) bool {
    if (table.len < R4L_QUERY_SIZE) return rejectLoadedExport(module_name, symbol_name, "query-range");
    const declared_size = readLe32(table[8..12]);
    if (export_version != 1 or
        readLe32(table[0..4]) != R4L_QUERY_MAGIC or
        readLe32(table[4..8]) != R4L_QUERY_VERSION or
        declared_size < R4L_QUERY_SIZE or
        (declared_size & 7) != 0 or
        @as(usize, @intCast(declared_size)) > table.len or
        readLe64(table[24..32]) != 0)
    {
        return rejectLoadedExport(module_name, symbol_name, "query-header");
    }
    return true;
}

fn validateLoadedInterface(table: []const u8, export_version: u32, expected_major: u16, module_name: []const u8, symbol_name: []const u8) bool {
    if (table.len < R4L_INTERFACE_HEADER_SIZE) return rejectLoadedExport(module_name, symbol_name, "interface-range");
    const flags = readLe16(table[6..8]);
    const declared_size = readLe32(table[8..12]);
    const abi_major = readLe16(table[12..14]);
    const abi_minor = readLe16(table[14..16]);
    const interface_id_lo = readLe64(table[16..24]);
    const interface_id_hi = readLe64(table[24..32]);
    if (export_version == 0 or export_version > 0xffff or
        readLe32(table[0..4]) != R4L_INTERFACE_MAGIC or
        readLe16(table[4..6]) != R4L_INTERFACE_HEADER_VERSION or
        (flags & 0x00ff) != 0 or
        declared_size < R4L_INTERFACE_HEADER_SIZE or
        (declared_size & 7) != 0 or
        @as(usize, @intCast(declared_size)) > table.len or
        abi_major != expected_major or
        abi_minor == 0 or
        @as(u32, abi_minor) != export_version or
        (interface_id_lo == 0 and interface_id_hi == 0))
    {
        return rejectLoadedExport(module_name, symbol_name, "interface-header");
    }
    return true;
}

fn rejectLoadedExport(module_name: []const u8, symbol_name: []const u8, reason: []const u8) bool {
    k.puts("[MOD] interface reject module=");
    k.puts(module_name);
    k.puts(" symbol=");
    k.puts(symbol_name);
    k.puts(" reason=");
    k.puts(reason);
    k.puts("\r\n");
    return false;
}

fn interfaceExportPrefix(name: []const u8) bool {
    return name.len >= 5 and
        upper(name[0]) == 'A' and
        upper(name[1]) == 'P' and
        upper(name[2]) == 'I' and
        name[3] == '_' and
        upper(name[4]) == 'V';
}

fn interfaceMajorFromName(name: []const u8) ?u16 {
    if (!interfaceExportPrefix(name) or name.len == 5 or name[5] == '0') return null;
    var value: u32 = 0;
    for (name[5..]) |byte| {
        if (byte < '0' or byte > '9') return null;
        const digit: u32 = byte - '0';
        if (value > (0xffff - digit) / 10) return null;
        value = value * 10 + digit;
    }
    return @intCast(value);
}

fn layoutSections(sections: []const SectionHeader, section_offsets: []usize) ?usize {
    var cursor: usize = 0;
    for (sections, 0..) |section, index| {
        const section_align = if (section.alignment > PAGE_SIZE) @as(usize, @intCast(section.alignment)) else PAGE_SIZE;
        cursor = alignForward(cursor, section_align);
        section_offsets[index] = cursor;
        cursor += @as(usize, @intCast(section.mem_size));
    }
    const image_size = alignForward(cursor, PAGE_SIZE);
    if (image_size == 0) return null;
    return image_size;
}

const RelocStats = struct {
    abs64: u32 = 0,
    rel32: u32 = 0,
    base_rel64: u32 = 0,
    import_slot64: u32 = 0,
};

fn applyRelocations(bytes: []const u8, header: Header, sections: []const SectionHeader, section_offsets: []const usize, image: []u8, module_name: []const u8, resolved_imports: []const ResolvedImport) ?RelocStats {
    var stats: RelocStats = .{};
    var i: usize = 0;
    while (i < header.reloc_count) : (i += 1) {
        const reloc = readRelocation(bytes, header, i) orelse return null;
        switch (applyRelocation(reloc, sections, section_offsets, image, resolved_imports)) {
            .ok => {},
            .unknown_type => {
                unknown_relocation_errors += 1;
                k.puts("[MOD] relocation error unknown type=");
                k.putDec(reloc.kind);
                k.puts(" module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
            .bad_range => {
                relocation_range_errors += 1;
                k.puts("[MOD] relocation range error module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
            .unresolved_import => {
                unresolved_import_errors += 1;
                k.puts("[MOD] relocation import unresolved module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
        }
        switch (reloc.kind) {
            R4M_RELOC_ABS64 => stats.abs64 += 1,
            R4M_RELOC_REL32 => stats.rel32 += 1,
            R4M_RELOC_BASE_REL64 => stats.base_rel64 += 1,
            R4M_RELOC_IMPORT_SLOT64 => stats.import_slot64 += 1,
            else => {},
        }
    }
    return stats;
}

fn applyRelocationsFromFile(reader: *module_r4m.Reader, header: Header, sections: []const SectionHeader, section_offsets: []const usize, image: []u8, module_name: []const u8, resolved_imports: []const ResolvedImport) ?RelocStats {
    var stats: RelocStats = .{};
    var reloc_reader = module_r4m.RelocationWindowReader.init(reader, header, "r4m-relocation-table", true);
    var i: usize = 0;
    while (i < header.reloc_count) : (i += 1) {
        const record = reloc_reader.next() orelse return null;
        const reloc = Relocation{
            .kind = record.kind,
            .patch_section = record.patch_section,
            .patch_offset = record.patch_offset,
            .target_section = record.target_section,
            .target_offset = record.target_offset,
            .addend = record.addend,
        };
        switch (applyRelocation(reloc, sections, section_offsets, image, resolved_imports)) {
            .ok => {},
            .unknown_type => {
                unknown_relocation_errors += 1;
                k.puts("[MOD] relocation error unknown type=");
                k.putDec(reloc.kind);
                k.puts(" module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
            .bad_range => {
                relocation_range_errors += 1;
                k.puts("[MOD] relocation range error module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
            .unresolved_import => {
                unresolved_import_errors += 1;
                k.puts("[MOD] relocation import unresolved module=");
                k.puts(module_name);
                k.puts("\r\n");
                return null;
            },
        }
        switch (reloc.kind) {
            R4M_RELOC_ABS64 => stats.abs64 += 1,
            R4M_RELOC_REL32 => stats.rel32 += 1,
            R4M_RELOC_BASE_REL64 => stats.base_rel64 += 1,
            R4M_RELOC_IMPORT_SLOT64 => stats.import_slot64 += 1,
            else => {},
        }
    }
    return stats;
}

const RelocApplyResult = enum {
    ok,
    unknown_type,
    bad_range,
    unresolved_import,
};

fn applyRelocation(reloc: Relocation, sections: []const SectionHeader, section_offsets: []const usize, image: []u8, resolved_imports: []const ResolvedImport) RelocApplyResult {
    const patch = patchSlice(reloc, sections, section_offsets, image) orelse return .bad_range;
    const image_base: u64 = @intFromPtr(image.ptr);
    switch (reloc.kind) {
        R4M_RELOC_ABS64 => {
            const target = targetAddress(reloc, sections, section_offsets, image_base) orelse return .bad_range;
            writeLe64(patch, addSignedU64(target, reloc.addend) orelse return .bad_range);
            return .ok;
        },
        R4M_RELOC_REL32 => {
            const target = targetAddress(reloc, sections, section_offsets, image_base) orelse return .bad_range;
            const patched_target = addSignedU64(target, reloc.addend) orelse return .bad_range;
            const patch_addr = image_base + @as(u64, @intCast(section_offsets[@intCast(reloc.patch_section)])) + reloc.patch_offset;
            const delta = @as(i128, patched_target) - @as(i128, patch_addr + 4);
            if (delta < -2147483648 or delta > 2147483647) return .bad_range;
            writeLe32(patch, @bitCast(@as(i32, @intCast(delta))));
            return .ok;
        },
        R4M_RELOC_BASE_REL64 => {
            const target = targetAddress(reloc, sections, section_offsets, image_base) orelse return .bad_range;
            writeLe64(patch, addSignedU64(target, reloc.addend) orelse return .bad_range);
            return .ok;
        },
        R4M_RELOC_IMPORT_SLOT64 => {
            const import_index: usize = @intCast(reloc.target_section);
            if (import_index >= resolved_imports.len) return .unresolved_import;
            writeLe64(patch, addSignedU64(resolved_imports[import_index].address, reloc.addend) orelse return .bad_range);
            return .ok;
        },
        else => return .unknown_type,
    }
}

fn patchSlice(reloc: Relocation, sections: []const SectionHeader, section_offsets: []const usize, image: []u8) ?[]u8 {
    if (reloc.patch_section >= sections.len) return null;
    const size = relocationPatchSize(reloc.kind);
    const section = sections[@intCast(reloc.patch_section)];
    if (reloc.patch_offset > section.mem_size or size > section.mem_size - reloc.patch_offset) return null;
    const off = section_offsets[@intCast(reloc.patch_section)] + @as(usize, @intCast(reloc.patch_offset));
    if (off > image.len or size > image.len - off) return null;
    return image[off .. off + size];
}

fn targetAddress(reloc: Relocation, sections: []const SectionHeader, section_offsets: []const usize, image_base: u64) ?u64 {
    if (reloc.target_section >= sections.len) return null;
    const section = sections[@intCast(reloc.target_section)];
    if (reloc.target_offset >= section.mem_size) return null;
    return image_base + @as(u64, @intCast(section_offsets[@intCast(reloc.target_section)])) + reloc.target_offset;
}

fn readRelocation(bytes: []const u8, header: Header, index: usize) ?Relocation {
    if (index >= header.reloc_count) return null;
    const off = @as(usize, @intCast(header.reloc_off)) + index * R4M_RELOCATION_SIZE;
    return .{
        .kind = readLe32(bytes[off + 0 .. off + 4]),
        .patch_section = readLe32(bytes[off + 4 .. off + 8]),
        .patch_offset = readLe32(bytes[off + 8 .. off + 12]),
        .target_section = readLe32(bytes[off + 12 .. off + 16]),
        .target_offset = readLe32(bytes[off + 16 .. off + 20]),
        .addend = @bitCast(readLe32(bytes[off + 20 .. off + 24])),
    };
}

fn readImport(bytes: []const u8, header: Header, index: usize) ?ImportRecord {
    if (index >= header.import_count) return null;
    const off = @as(usize, @intCast(header.import_off)) + index * R4M_IMPORT_SIZE;
    return .{
        .module = zString(bytes, readLe32(bytes[off + 0 .. off + 4])) orelse return null,
        .symbol = zString(bytes, readLe32(bytes[off + 4 .. off + 8])) orelse return null,
        .min_version = readLe32(bytes[off + 8 .. off + 12]),
        .flags = readLe32(bytes[off + 12 .. off + 16]),
    };
}

fn relocationPatchSize(kind: u32) u32 {
    return switch (kind) {
        R4M_RELOC_REL32 => 4,
        R4M_RELOC_ABS64, R4M_RELOC_BASE_REL64, R4M_RELOC_IMPORT_SLOT64 => 8,
        else => 1,
    };
}

fn addSignedU64(base: u64, addend: i32) ?u64 {
    if (addend >= 0) {
        const plus: u64 = @intCast(addend);
        if (0xffffffffffffffff - base < plus) return null;
        return base + plus;
    }
    const sub: u64 = @intCast(-@as(i64, addend));
    if (sub > base) return null;
    return base - sub;
}

fn validateEntries(header: Header, bytes: []const u8) bool {
    if (header.entry_count == 0) return false;
    var section_sizes: [MAX_SECTIONS]u32 = .{0} ** MAX_SECTIONS;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const off = @as(usize, @intCast(header.section_off)) + i * R4M_SECTION_SIZE;
        section_sizes[i] = readLe32(bytes[off + 20 .. off + 24]);
    }
    i = 0;
    while (i < header.entry_count) : (i += 1) {
        const off = @as(usize, @intCast(header.entry_off)) + i * R4M_ENTRY_SIZE;
        const section_index = readLe32(bytes[off + 4 .. off + 8]);
        const section_offset = readLe32(bytes[off + 8 .. off + 12]);
        if (section_index >= header.section_count) return false;
        if (section_offset >= section_sizes[@intCast(section_index)]) return false;
    }
    return true;
}

fn validateImports(header: Header, bytes: []const u8) bool {
    var i: usize = 0;
    while (i < header.import_count) : (i += 1) {
        const off = @as(usize, @intCast(header.import_off)) + i * R4M_IMPORT_SIZE;
        if (!checkZ(bytes, readLe32(bytes[off + 0 .. off + 4]))) return false;
        if (!checkZ(bytes, readLe32(bytes[off + 4 .. off + 8]))) return false;
        const module_name = zString(bytes, readLe32(bytes[off + 0 .. off + 4])) orelse return false;
        const symbol_name = zString(bytes, readLe32(bytes[off + 4 .. off + 8])) orelse return false;
        if (!validImportModuleName(module_name) or !validSymbolName(symbol_name)) return false;
        if (readLe32(bytes[off + 8 .. off + 12]) == 0) return false;
    }
    return true;
}

fn validateExports(header: Header, bytes: []const u8) bool {
    var section_sizes: [MAX_SECTIONS]u32 = .{0} ** MAX_SECTIONS;
    var i: usize = 0;
    while (i < header.section_count) : (i += 1) {
        const off = @as(usize, @intCast(header.section_off)) + i * R4M_SECTION_SIZE;
        section_sizes[i] = readLe32(bytes[off + 20 .. off + 24]);
    }
    i = 0;
    while (i < header.export_count) : (i += 1) {
        const off = @as(usize, @intCast(header.export_off)) + i * R4M_EXPORT_SIZE;
        const section_index = readLe32(bytes[off + 4 .. off + 8]);
        const section_offset = readLe32(bytes[off + 8 .. off + 12]);
        if (!checkZ(bytes, readLe32(bytes[off + 0 .. off + 4]))) return false;
        const name = zString(bytes, readLe32(bytes[off + 0 .. off + 4])) orelse return false;
        if (!validSymbolName(name) or readLe32(bytes[off + 12 .. off + 16]) == 0) return false;
        if (section_index >= header.section_count) return false;
        if (section_offset >= section_sizes[@intCast(section_index)]) return false;
        var prior_index: usize = 0;
        while (prior_index < i) : (prior_index += 1) {
            const prior_off = @as(usize, @intCast(header.export_off)) + prior_index * R4M_EXPORT_SIZE;
            const prior_name = zString(bytes, readLe32(bytes[prior_off + 0 .. prior_off + 4])) orelse return false;
            if (nameEq(name, prior_name)) return false;
        }
    }
    return true;
}

fn fillExports(e: *Entry, bytes: []const u8, header: Header) void {
    var i: usize = 0;
    while (i < header.export_count and i < e.exports.len) : (i += 1) {
        const off = @as(usize, @intCast(header.export_off)) + i * R4M_EXPORT_SIZE;
        const name = zString(bytes, readLe32(bytes[off + 0 .. off + 4])) orelse continue;
        const section_index = readLe32(bytes[off + 4 .. off + 8]);
        const section_offset = readLe32(bytes[off + 8 .. off + 12]);
        const version = readLe32(bytes[off + 12 .. off + 16]);
        if (section_index >= e.section_count or section_index >= e.sections.len) continue;
        const section = &e.sections[@intCast(section_index)];
        if (!section.used or section_offset >= section.mem_size) continue;

        e.exports[i] = .{
            .used = true,
            .version = version,
            .address = section.runtime_base + section_offset,
            .section_index = section_index,
            .section_offset = section_offset,
        };
        e.exports[i].name_len = copyBytes(name, e.exports[i].name[0..]);
    }
}

fn fillExportsFromTables(e: *Entry, tables: *const ValidatedFileTables) void {
    var i: usize = 0;
    while (i < tables.header.export_count and i < e.exports.len) : (i += 1) {
        const planned = &tables.exports[i];
        if (planned.section_index >= e.section_count or planned.section_index >= e.sections.len) continue;
        const section = &e.sections[@intCast(planned.section_index)];
        if (!section.used or planned.section_offset >= section.mem_size) continue;

        e.exports[i] = .{
            .used = true,
            .version = planned.version,
            .address = section.runtime_base + planned.section_offset,
            .section_index = planned.section_index,
            .section_offset = planned.section_offset,
        };
        e.exports[i].name_len = copyBytes(planned.name[0..planned.name_len], e.exports[i].name[0..]);
    }
}

fn findExport(e: *const Entry, symbol: []const u8) ?*const Export {
    var i: usize = 0;
    while (i < e.export_count and i < e.exports.len) : (i += 1) {
        const exp = &e.exports[i];
        if (exp.used and nameEq(exp.name[0..exp.name_len], symbol)) return exp;
    }
    return null;
}

fn findByName(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
        if (nameEq(e.name[0..e.name_len], name)) return i;
    }
    return null;
}

fn findPendingByName(pending: []PendingLibrary, name: []const u8) ?usize {
    var i: usize = 0;
    while (i < pending.len) : (i += 1) {
        const p = &pending[i];
        if (!p.used) continue;
        if (nameEq(p.module_name[0..p.module_name_len], name)) return i;
    }
    return null;
}

fn moduleName(bytes: []const u8, header: Header) ?[]const u8 {
    if (header.meta_size == 0) return null;
    return zString(bytes, header.meta_off);
}

fn fallbackModuleName(file_name: []const u8) []const u8 {
    if (file_name.len > 4 and file_name[file_name.len - 4] == '.') return file_name[0 .. file_name.len - 4];
    return file_name;
}

fn buildLibraryPath(file_name: []const u8, out: *[MAX_PATH]u8) []const u8 {
    const prefix = "C:\\R4OS\\LIBS\\";
    var len: usize = 0;
    const prefix_len = @min(prefix.len, out.len);
    @memcpy(out[0..prefix_len], prefix[0..prefix_len]);
    len = prefix_len;
    const remaining = out.len - len;
    const copy_len = @min(file_name.len, remaining);
    if (copy_len > 0) @memcpy(out[len .. len + copy_len], file_name[0..copy_len]);
    len += copy_len;
    return out[0..len];
}

fn checkTable(image_len: usize, off: u32, count: u32, item_size: usize, required: bool) bool {
    if (count == 0) return !required;
    if (off == 0) return false;
    const bytes = @as(u64, count) * @as(u64, item_size);
    if (bytes > 0xffffffff) return false;
    return checkRange(image_len, off, @intCast(bytes));
}

fn checkRange(image_len: usize, off_u32: u32, size_u32: u32) bool {
    const off: usize = @intCast(off_u32);
    const size: usize = @intCast(size_u32);
    return off <= image_len and size <= image_len - off;
}

fn checkZ(image: []const u8, off_u32: u32) bool {
    const off: usize = @intCast(off_u32);
    if (off >= image.len) return false;
    var i = off;
    while (i < image.len) : (i += 1) {
        if (image[i] == 0) return true;
    }
    return false;
}

fn zString(image: []const u8, off_u32: u32) ?[]const u8 {
    const off: usize = @intCast(off_u32);
    if (off >= image.len) return null;
    var i = off;
    while (i < image.len) : (i += 1) {
        if (image[i] == 0) return image[off..i];
    }
    return null;
}

fn zName(buf: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buf.len and buf[len] != 0) : (len += 1) {}
    return buf[0..len];
}

fn rangeIsZero(bytes: []const u8) bool {
    for (bytes) |b| {
        if (b != 0) return false;
    }
    return true;
}

fn fixedNameLen(name: []const u8) usize {
    var len: usize = 0;
    while (len < name.len and name[len] != 0) : (len += 1) {}
    return len;
}

fn freeSlot() ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (!entries[i].used) return i;
    }
    return null;
}

fn kindFromRaw(raw: u16) ?Kind {
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

fn kindName(kind: Kind) []const u8 {
    return switch (kind) {
        .r4x => "r4x",
        .r4l => "r4l",
        .r4d => "r4d",
        .r4p => "r4p",
        .platform_api_provider_reserved => "platform_api_provider_reserved",
        .kernel_module_reserved => "kernel_module_reserved",
    };
}

fn hasR4lExtension(name: []const u8) bool {
    if (name.len < 4) return false;
    const ext = name[name.len - 4 ..];
    return upper(ext[0]) == '.' and upper(ext[1]) == 'R' and upper(ext[2]) == '4' and upper(ext[3]) == 'L';
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn validModuleName(name: []const u8) bool {
    if (name.len == 0 or name.len >= MAX_NAME) return false;
    for (name) |byte| {
        if (!asciiAlphaNumeric(byte) and byte != '_' and byte != '-') return false;
    }
    return true;
}

fn validImportModuleName(name: []const u8) bool {
    if (name.len == 0 or name.len >= MAX_NAME) return false;
    for (name) |byte| {
        if (!asciiAlphaNumeric(byte) and byte != '_' and byte != '-' and byte != '.') return false;
    }
    return true;
}

fn validSymbolName(name: []const u8) bool {
    if (name.len == 0 or name.len >= MAX_NAME) return false;
    if (!asciiAlphabetic(name[0]) and name[0] != '_') return false;
    for (name[1..]) |byte| {
        if (!asciiAlphaNumeric(byte) and byte != '_') return false;
    }
    return true;
}

fn asciiAlphabetic(value: u8) bool {
    const folded = upper(value);
    return folded >= 'A' and folded <= 'Z';
}

fn asciiAlphaNumeric(value: u8) bool {
    return asciiAlphabetic(value) or (value >= '0' and value <= '9');
}

fn copyBytes(src: []const u8, dst: []u8) usize {
    const len = @min(src.len, dst.len);
    if (len > 0) @memcpy(dst[0..len], src[0..len]);
    return len;
}

fn alignForward(value: usize, alignment: usize) usize {
    return (value + alignment - 1) & ~(alignment - 1);
}

fn isPowerOfTwo(value: u32) bool {
    return value != 0 and (value & (value - 1)) == 0;
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

fn readLe64(bytes: []const u8) u64 {
    return @as(u64, readLe32(bytes[0..4])) |
        (@as(u64, readLe32(bytes[4..8])) << 32);
}

fn writeLe32(bytes: []u8, value: u32) void {
    bytes[0] = @truncate(value);
    bytes[1] = @truncate(value >> 8);
    bytes[2] = @truncate(value >> 16);
    bytes[3] = @truncate(value >> 24);
}

fn writeLe64(bytes: []u8, value: u64) void {
    writeLe32(bytes[0..4], @truncate(value));
    writeLe32(bytes[4..8], @truncate(value >> 32));
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

test "Runtime-R4L identity parser fails closed" {
    const testing = @import("std").testing;

    try testing.expect(hasR4lExtension("EXAMPLE.R4L"));
    try testing.expect(hasR4lExtension("example.r4l"));
    try testing.expect(!hasR4lExtension("EXAMPLE.R4X"));
    try testing.expect(!hasR4lExtension("R4L"));

    try testing.expect(validModuleName("EXAMPLE-CODEC"));
    try testing.expect(!validModuleName(""));
    try testing.expect(!validModuleName("BAD:NAME"));
    try testing.expect(validSymbolName("API_V1"));
    try testing.expect(!validSymbolName("1API"));
    try testing.expect(!validSymbolName("API:V1"));

    try testing.expectEqual(@as(?u16, 1), interfaceMajorFromName("API_V1"));
    try testing.expectEqual(@as(?u16, 65535), interfaceMajorFromName("api_v65535"));
    try testing.expectEqual(@as(?u16, null), interfaceMajorFromName("API_V"));
    try testing.expectEqual(@as(?u16, null), interfaceMajorFromName("API_V0"));
    try testing.expectEqual(@as(?u16, null), interfaceMajorFromName("API_V01"));
    try testing.expectEqual(@as(?u16, null), interfaceMajorFromName("API_V65536"));
    try testing.expectEqual(@as(?u16, null), interfaceMajorFromName("API_V1X"));
    try testing.expectEqual(@as(?Kind, null), kindFromRaw(0));
    try testing.expectEqual(@as(?Kind, null), kindFromRaw(7));
}

test "built-in platform APIs own the six reserved Query providers" {
    const testing = @import("std").testing;

    try testing.expect(module_r4m.isLoadableContainerKind(.r4x));
    try testing.expect(module_r4m.isLoadableContainerKind(.r4l));
    try testing.expect(module_r4m.isLoadableContainerKind(.r4d));
    try testing.expect(module_r4m.isLoadableContainerKind(.r4p));
    try testing.expect(!module_r4m.isLoadableContainerKind(.platform_api_provider_reserved));
    try testing.expect(!module_r4m.isLoadableContainerKind(.kernel_module_reserved));

    init();
    try testing.expectEqual(r4x_api.r4_platform_apis.len, countUsed());
    for (r4x_api.r4_platform_apis) |meta| {
        const slot = findByName(meta.name) orelse return error.MissingPlatformApi;
        const entry = entries[slot];
        try testing.expectEqual(Kind.platform_api_provider_reserved, entry.kind);
        try testing.expectEqual(State.builtin, entry.state);
        const info = exportInfo(slot, "Query", 1) orelse return error.MissingPlatformQuery;
        try testing.expectEqual(r4x_api.r4l_query_struct_size, info.available_size);
        const query: *const r4x_api.R4LQuery = @ptrFromInt(info.address);
        try testing.expectEqual(r4x_api.r4l_abi_magic, query.magic);
        try testing.expectEqual(r4x_api.r4l_abi_version, query.abi_version);
        try testing.expectEqual(@intFromEnum(meta.group), query.group);
        try testing.expectEqual(@as(u64, 0), query.kernel_bridge);
        try testing.expectEqual(@as(u64, 0), query.reserved);
        try testing.expectEqual(@as(?u32, @intFromEnum(meta.group)), platformApiGroupId(meta.name));
    }
    try testing.expectEqual(@as(?u32, 6), platformApiGroupId("r4dev"));
    try testing.expectEqual(@as(?u32, null), platformApiGroupId("R4STD"));
}

test "Runtime-R4L executable ownership is exact and generation safe" {
    const std = @import("std");
    const testing = std.testing;

    init();
    const slot = r4x_api.r4_platform_apis.len;
    entries[slot] = .{
        .used = true,
        .kind = .r4l,
        .state = .loaded,
        .generation = 7,
        .pinned = true,
        .section_count = 3,
    };
    entries[slot].sections[0] = .{
        .used = true,
        .flags = R4M_SECTION_FLAG_EXEC,
        .mem_size = 0x40,
        .runtime_base = 0x1000,
    };
    entries[slot].sections[1] = .{
        .used = true,
        .flags = 0,
        .mem_size = 0x40,
        .runtime_base = 0x2000,
    };
    entries[slot].sections[2] = .{
        .used = true,
        .flags = R4M_SECTION_FLAG_EXEC,
        .mem_size = 0x40,
        .runtime_base = std.math.maxInt(u64) - 0x1f,
    };

    try testing.expect(isExecutableAddress(@intCast(slot), 7, 0x1000));
    try testing.expect(isExecutableAddress(@intCast(slot), 7, 0x103f));
    try testing.expect(!isExecutableAddress(@intCast(slot), 7, 0x1040));
    try testing.expect(!isExecutableAddress(@intCast(slot), 7, 0x2000));
    try testing.expect(!isExecutableAddress(@intCast(slot), 6, 0x1000));
    try testing.expect(!isExecutableAddress(@intCast(slot), 7, std.math.maxInt(u64)));

    entries[slot].kind = .r4d;
    try testing.expect(!isExecutableAddress(@intCast(slot), 7, 0x1000));
    entries[slot].kind = .r4l;
    entries[slot].state = .failed;
    try testing.expect(!isExecutableAddress(@intCast(slot), 7, 0x1000));
}
