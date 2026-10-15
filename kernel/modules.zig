//! Typed compatibility facade. Native Rust owns module discovery, dependency
//! admission, image construction, publication and the sole registry state.

const heap = @import("../memory/heap.zig");
const k = @import("log.zig");
const loader_perf = @import("loader_perf.zig");
const module_file = @import("module_file.zig");
const module_r4m = @import("module_r4m.zig");
const resources = @import("module_resources.zig");
const r4x_api = @import("../program/r4x_api.zig");
const task_context = @import("../sched/task_context.zig");

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
const R4M_SECTION_FLAG_ALLOC: u32 = 0x00000001;
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


const Header = module_r4m.Header;

const SectionHeader = extern struct {
    name: [8]u8,
    name_len: usize,
    flags: u32,
    file_off: u32,
    file_size: u32,
    mem_size: u32,
    alignment: u32,
};

const Relocation = module_r4m.RelocationRecord;

const ImportRecord = struct {
    module: []const u8,
    symbol: []const u8,
    min_version: u32,
    flags: u32,
};

const ResolvedImport = extern struct {
    module: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    module_len: usize = 0,
    symbol: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    symbol_len: usize = 0,
    version: u32 = 0,
    address: u64 = 0,
};

pub const Export = extern struct {
    used: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    version: u32 = 0,
    address: u64 = 0,
    section_index: u32 = 0,
    section_offset: u32 = 0,
};


pub const Section = extern struct {
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

pub const NativeOptionalSource = extern struct { present: bool = false, value: module_file.NativeSource = undefined };

pub const Entry = extern struct {
    version_labels: module_r4m.VersionLabels = .{},
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
    resource_source: NativeOptionalSource = .{},
    resource_file_offset: usize = 0,
    sections: [MAX_SECTIONS]Section = .{Section{}} ** MAX_SECTIONS,
    exports: [MAX_EXPORTS_PER_MODULE]Export = .{Export{}} ** MAX_EXPORTS_PER_MODULE,
};


const ClaimState = enum(u32) { free, reserved, published, retiring, retained };
extern var r4os_rust_module_claims: [MAX_MODULES]u64;
const slot_claims = &r4os_rust_module_claims;

pub const ResourceView = struct {
    source: module_file.FileSource,
    file_offset: usize,
    catalog: *const resources.Catalog,
};

pub fn driverResourceView(slot: usize, generation: u32) ?ResourceView {
    var result: NativeResourceView = undefined;
    if (!r4os_rust_mod_resources(slot, generation, &result)) return null;
    return .{ .source = module_file.sourceFromNative(result.source), .file_offset = result.file_offset, .catalog = result.catalog };
}


fn claimValue(generation: u32, state: ClaimState) u64 {
    return (@as(u64, generation) << 32) | @intFromEnum(state);
}

pub const PreparedLoad = extern struct {
    slot: usize,
    generation: u32,
    scratch: ?*anyopaque = null,
    unwind: task_context.UnwindToken,
    pub fn exportAddress(self: *const PreparedLoad, symbol: []const u8, min_version: u32) ?u64 {
        var value: u64 = undefined;
        return if (r4os_rust_mod_prepared_export(self, nativeBytes(symbol), min_version, &value)) value else null;
    }
    pub fn commit(self: *PreparedLoad) ?usize {
        const peers = loadPeers();
        const index = r4os_rust_mod_commit(self, &peers);
        return if (index == @import("std").math.maxInt(usize)) null else index;
    }
    pub fn abort(self: *PreparedLoad) void {
        const peers = loadPeers();
        r4os_rust_mod_abort(self, &peers);
    }
};

const NativeLoadPeers = extern struct {
    scratch_size: usize,
    scratch_align: usize,
    alloc: *const fn (usize, usize) callconv(.c) ?[*]u8,
    free: *const fn ([*]u8, usize) callconv(.c) u8,
    publication: *const fn (*const Entry) callconv(.c) void,
    log: *const fn ([*]const u8, usize) callconv(.c) void,
};

extern fn r4os_rust_mod_commit(*PreparedLoad, *const NativeLoadPeers) usize;
extern fn r4os_rust_mod_abort(*PreparedLoad, *const NativeLoadPeers) void;
extern fn r4os_rust_mod_scratch_layout(bool) usize;
extern fn r4os_rust_mod_prepare(NativeBytes, ?*const module_file.NativeSource, u16, NativeBytes, NativeBytes, *const NativeLoadPeers, *PreparedLoad) bool;
extern fn r4os_rust_mod_load(NativeBytes, ?*const module_file.NativeSource, u16, NativeBytes, NativeBytes, *const NativeLoadPeers) usize;
extern fn r4os_rust_mod_prepared_export(*const PreparedLoad, NativeBytes, u32, *u64) bool;
extern fn r4os_rust_mod_load_libraries(*const NativeLoadPeers) void;

fn nativeLoadAlloc(size: usize, alignment: usize) callconv(.c) ?[*]u8 {
    return (heap.alloc(size, alignment) orelse return null).ptr;
}

fn nativeLoadFree(data: [*]u8, size: usize) callconv(.c) u8 {
    return if (heap.free(data[0..size]) == .ok) 0 else 1;
}

fn nativeLoadLog(data: [*]const u8, len: usize) callconv(.c) void {
    k.puts(data[0..len]);
}

fn loadPeers() NativeLoadPeers {
    return .{ .scratch_size = r4os_rust_mod_scratch_layout(false), .scratch_align = r4os_rust_mod_scratch_layout(true), .alloc = nativeLoadAlloc, .free = nativeLoadFree, .publication = r4os_rust_mod_publication, .log = nativeLoadLog };
}

comptime {
    if (@sizeOf(NativeOptionalSource) != 944 or @offsetOf(NativeOptionalSource, "present") != 0 or @offsetOf(NativeOptionalSource, "value") != 8 or @sizeOf(Entry) != 3296) @compileError("native module Entry/source C layout drift");
    for (.{ .{ "version_labels", 0 }, .{ "used", 64 }, .{ "name", 65 }, .{ "name_len", 104 }, .{ "path", 112 }, .{ "path_len", 208 }, .{ "kind", 216 }, .{ "state", 218 }, .{ "flags", 220 }, .{ "base", 224 }, .{ "size", 232 }, .{ "section_count", 240 }, .{ "entry_count", 244 }, .{ "import_count", 248 }, .{ "export_count", 252 }, .{ "reloc_count", 256 }, .{ "reloc_abs64_count", 260 }, .{ "reloc_rel32_count", 264 }, .{ "reloc_base_rel64_count", 268 }, .{ "reloc_import_slot64_count", 272 }, .{ "resolved_import_count", 276 }, .{ "resolver_status", 280 }, .{ "bss_zeroed", 281 }, .{ "generation", 284 }, .{ "pinned", 288 }, .{ "resource_source", 296 }, .{ "resource_file_offset", 1240 }, .{ "sections", 1248 }, .{ "exports", 2144 } }) |field| {
        if (@offsetOf(Entry, field[0]) != field[1]) @compileError("native module Entry C offset drift");
    }
    if (@sizeOf(PreparedLoad) != 40 or @offsetOf(PreparedLoad, "slot") != 0 or @offsetOf(PreparedLoad, "generation") != 8 or @offsetOf(PreparedLoad, "scratch") != 16 or @offsetOf(PreparedLoad, "unwind") != 24) @compileError("native PreparedLoad C layout drift");
    if (@sizeOf(NativeLoadPeers) != 48) @compileError("native load peer C size drift");
    const names = [_][]const u8{ "scratch_size", "scratch_align", "alloc", "free", "publication", "log" };
    for (names, 0..) |name, i| {
        if (@offsetOf(NativeLoadPeers, name) != i * 8) @compileError("native load peer C offset drift");
    }
}


extern var r4os_rust_module_entries: [MAX_MODULES]Entry;
const entries = &r4os_rust_module_entries;

// Stateless projections of the canonical Contract metadata and logger.
// Native Rust owns all registration, lookup, counter and rendering decisions.
const NativePlatform = extern struct { name: NativeBytes, group: u32, query: u64 };
const NativeResourceView = extern struct { source: module_file.NativeSource, file_offset: usize, catalog: *const resources.Catalog };
extern fn r4os_rust_mod_init(*const NativeLoadPeers) void;
extern fn r4os_rust_mod_counter(u8, bool) u32;
extern fn r4os_rust_mod_libraries_active(bool) bool;
extern fn r4os_rust_mod_count() usize;
extern fn r4os_rust_mod_entry(usize) ?*const Entry;
extern fn r4os_rust_mod_find_name(NativeBytes) usize;
extern fn r4os_rust_mod_platform_group(NativeBytes) u64;
extern fn r4os_rust_mod_summary() void;
extern fn r4os_rust_mod_library_results(usize) void;
extern fn r4os_rust_mod_resolve_logged(*const Entry, NativeBytes, NativeBytes, u32, u32, NativeBytes, *ResolvedImport) bool;
extern fn r4os_rust_mod_required_logged(*const Entry) bool;
extern fn r4os_rust_mod_reject(NativeBytes, NativeBytes, NativeBytes) bool;
extern fn r4os_rust_mod_export_info(usize, NativeBytes, u32, *ExportInfo) bool;
extern fn r4os_rust_mod_resolve_info(NativeBytes, NativeBytes, u32, *ExportInfo) bool;
extern fn r4os_rust_mod_code_owner(u8, u32, u64) bool;
extern fn r4os_rust_mod_resources(usize, u32, *NativeResourceView) bool;
extern fn r4os_rust_mod_publication(*const Entry) void;
export fn r4os_zig_mod_log(op: u8, value: u64, data: ?[*]const u8, len: usize) callconv(.c) void {
    switch (op) {
        0 => k.puts(data.?[0..len]),
        1 => k.putDec(value),
        2 => k.putHex(value, 16),
        else => unreachable,
    }
}
export fn r4os_zig_mod_platform(index: usize, out: *NativePlatform) callconv(.c) bool {
    if (index >= r4x_api.r4_platform_apis.len) return false;
    const meta = r4x_api.r4_platform_apis[index];
    out.* = .{ .name = nativeBytes(meta.name), .group = @intFromEnum(meta.group), .query = @intFromPtr(&platform_api_queries[index]) };
    return true;
}
comptime {
    if (r4x_api.r4_platform_apis.len != 6) @compileError("expected exactly six built-in platform APIs");
    if (@sizeOf(NativePlatform) != 32 or @offsetOf(NativePlatform, "name") != 0 or @offsetOf(NativePlatform, "group") != 16 or @offsetOf(NativePlatform, "query") != 24) @compileError("native platform metadata C layout drift");
    if (@sizeOf(ExportInfo) != 24 or @offsetOf(ExportInfo, "address") != 0 or @offsetOf(ExportInfo, "version") != 8 or @offsetOf(ExportInfo, "available_size") != 12 or @offsetOf(ExportInfo, "generation") != 16 or @offsetOf(ExportInfo, "module_slot") != 20 or @offsetOf(ExportInfo, "kind") != 22) @compileError("native ExportInfo C layout drift");
    if (@sizeOf(NativeResourceView) != 952 or @offsetOf(NativeResourceView, "source") != 0 or @offsetOf(NativeResourceView, "file_offset") != 936 or @offsetOf(NativeResourceView, "catalog") != 944) @compileError("native ResourceView C layout drift");
}


export fn r4os_zig_mod_perf(op: u8, a: u64, b: u64) callconv(.c) u64 {
    switch (op) {
        0 => return loader_perf.now(),
        1 => loader_perf.addR4lScanTicks(a),
        2 => loader_perf.recordR4lScanEntry(),
        3 => loader_perf.recordR4lCandidate(),
        4 => loader_perf.addR4lResolveTicks(a),
        5 => loader_perf.addR4lLoadTicks(a),
        6 => loader_perf.recordR4lResults(@intCast(a), @intCast(b)),
        else => unreachable,
    }
    return 0;
}
pub fn init() void {
    const peers = loadPeers();
    r4os_rust_mod_init(&peers);
}
pub fn loadSystemLibraries() void {
    const peers = loadPeers();
    r4os_rust_mod_load_libraries(&peers);
}

pub fn dumpSummary() void {
    r4os_rust_mod_summary();
}

pub fn countUsed() usize {
    return r4os_rust_mod_count();
}

pub fn entryAt(index: usize) ?*const Entry {
    return r4os_rust_mod_entry(index);
}

pub fn platformApiGroupId(name: []const u8) ?u32 {
    const result = r4os_rust_mod_platform_group(nativeBytes(name));
    return if (result == @import("std").math.maxInt(u64)) null else @intCast(result);
}

pub fn loadResolvedBytes(bytes: []const u8, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?usize {
    const peers = loadPeers();
    const index = r4os_rust_mod_load(nativeBytes(bytes), null, @intFromEnum(expected_kind), nativeBytes(fallback_name), nativeBytes(path), &peers);
    return if (index == @import("std").math.maxInt(usize)) null else index;
}
pub fn loadResolvedFile(source: module_file.FileSource, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?usize {
    const peers = loadPeers();
    const native_source = module_file.nativeSource(source);
    const index = r4os_rust_mod_load(nativeBytes(""), &native_source, @intFromEnum(expected_kind), nativeBytes(fallback_name), nativeBytes(path), &peers);
    return if (index == @import("std").math.maxInt(usize)) null else index;
}
pub fn prepareResolvedBytes(bytes: []const u8, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?PreparedLoad {
    const peers = loadPeers();
    var result: PreparedLoad = undefined;
    return if (r4os_rust_mod_prepare(nativeBytes(bytes), null, @intFromEnum(expected_kind), nativeBytes(fallback_name), nativeBytes(path), &peers, &result)) result else null;
}
pub fn prepareResolvedFile(source: module_file.FileSource, expected_kind: Kind, fallback_name: []const u8, path: []const u8) ?PreparedLoad {
    const peers = loadPeers();
    const native_source = module_file.nativeSource(source);
    var result: PreparedLoad = undefined;
    return if (r4os_rust_mod_prepare(nativeBytes(""), &native_source, @intFromEnum(expected_kind), nativeBytes(fallback_name), nativeBytes(path), &peers, &result)) result else null;
}

pub fn exportAddress(slot: usize, symbol: []const u8, min_version: u32) ?u64 {
    const info = exportInfo(slot, symbol, min_version) orelse return null;
    return info.address;
}

pub const ExportInfo = extern struct {
    address: u64,
    version: u32,
    available_size: u32,
    generation: u32,
    module_slot: u8,
    kind: Kind,
};

pub fn exportInfo(slot: usize, symbol: []const u8, min_version: u32) ?ExportInfo {
    var result: ExportInfo = undefined;
    return if (r4os_rust_mod_export_info(slot, nativeBytes(symbol), min_version, &result)) result else null;
}

// IRQ-time executable ownership lookup. Runtime R4Ls are boot-generation
// pinned today, but callers retain the exact slot+generation so a later
// unload/reload implementation cannot accidentally make a stale program
// binding preemptible. Only explicitly executable sections qualify; data,
// BSS, other module kinds and overflowed ranges fail closed.
pub fn isExecutableAddress(module_slot: u8, generation: u32, address: u64) bool {
    return r4os_rust_mod_code_owner(module_slot, generation, address);
}

pub fn resolveExportAddress(module_name: []const u8, symbol: []const u8, min_version: u32) ?u64 {
    const info = resolveExportInfo(module_name, symbol, min_version) orelse return null;
    return info.address;
}

pub fn resolveExportInfo(module_name: []const u8, symbol: []const u8, min_version: u32) ?ExportInfo {
    var result: ExportInfo = undefined;
    return if (r4os_rust_mod_resolve_info(nativeBytes(module_name), nativeBytes(symbol), min_version, &result)) result else null;
}


const NativeEntryView = extern struct { kind: u16, state: u8, generation: u32, section_count: u32, export_count: u32, name: NativeBytes, sections: [*]const Section, exports: [*]const Export };
const NativeRequired = extern struct { symbol: NativeBytes, reason: NativeBytes, accepted: bool };
extern fn r4os_rust_mod_find_export(*const NativeEntryView, NativeBytes) ?*const Export;
extern fn r4os_rust_mod_resolve(*const NativeEntryView, NativeBytes, NativeBytes, u32, u32, *ResolvedImport, *u32) u8;
extern fn r4os_rust_mod_optional(NativeBytes, NativeBytes, *ResolvedImport) void;
extern fn r4os_rust_mod_resolved_count([*]const ResolvedImport, usize) u32;
extern fn r4os_rust_mod_executable(*const NativeEntryView, u32, u64) bool;
extern fn r4os_rust_mod_required(*const NativeEntryView) NativeRequired;
fn nativeEntryView(e: *const Entry) NativeEntryView {
    return .{ .kind = @intFromEnum(e.kind), .state = @intFromEnum(e.state), .generation = e.generation, .section_count = e.section_count, .export_count = e.export_count, .name = .{ .data = &e.name, .len = e.name_len }, .sections = &e.sections, .exports = &e.exports };
}
comptime {
    if (@sizeOf(Export) != 72 or @sizeOf(Section) != 56 or @sizeOf(NativeEntryView) != 48 or @sizeOf(NativeRequired) != 40) @compileError("module query private C size drift");
    for (.{ .{ "used", 0 }, .{ "name", 1 }, .{ "name_len", 40 }, .{ "version", 48 }, .{ "address", 56 }, .{ "section_index", 64 }, .{ "section_offset", 68 } }) |field| {
        if (@offsetOf(Export, field[0]) != field[1]) @compileError("module export C offset drift");
    }
    for (.{ .{ "used", 0 }, .{ "name", 1 }, .{ "name_len", 16 }, .{ "flags", 24 }, .{ "file_size", 28 }, .{ "mem_size", 32 }, .{ "alignment", 36 }, .{ "runtime_offset", 40 }, .{ "runtime_base", 48 } }) |field| {
        if (@offsetOf(Section, field[0]) != field[1]) @compileError("module runtime section C offset drift");
    }
    for (.{ .{ "kind", 0 }, .{ "state", 2 }, .{ "generation", 4 }, .{ "section_count", 8 }, .{ "export_count", 12 }, .{ "name", 16 }, .{ "sections", 32 }, .{ "exports", 40 } }) |field| {
        if (@offsetOf(NativeEntryView, field[0]) != field[1]) @compileError("module query view C offset drift");
    }
    if (@offsetOf(NativeRequired, "symbol") != 0 or @offsetOf(NativeRequired, "reason") != 16 or @offsetOf(NativeRequired, "accepted") != 32) @compileError("module required-export C offset drift");
}

// Stateless format projections. Native Rust alone validates byte admission,
// section policy, image layout, individual relocations and R4L table headers.
const NativeBytes = extern struct { data: [*]const u8, len: usize };
const NativeImport = extern struct { module: NativeBytes, symbol: NativeBytes, min_version: u32, flags: u32 };
const NativeRelocationImage = extern struct { sections: [*]const SectionHeader, section_count: usize, offsets: [*]const usize, offset_count: usize, image: [*]u8, image_len: usize, imports: [*]const ResolvedImport, import_count: usize };
const NativeLoadedExport = extern struct { name: NativeBytes, version: u32, section_index: u32, export_offset: u32, sections: [*]const SectionHeader, section_count: usize, offsets: [*]const usize, offset_count: usize, image: NativeBytes };
extern fn r4os_rust_mod_header(NativeBytes, *Header) bool;
extern fn r4os_rust_mod_validate(NativeBytes, *const Header, u8) bool;
extern fn r4os_rust_mod_sections(NativeBytes, *const Header, [*]SectionHeader, usize) bool;
extern fn r4os_rust_mod_section_kinds([*]const SectionHeader, usize, u16) bool;
extern fn r4os_rust_mod_layout([*]const SectionHeader, usize, [*]usize, *usize) bool;
extern fn r4os_rust_mod_reloc(Relocation, *const NativeRelocationImage) u8;
extern fn r4os_rust_mod_import(*const Header, NativeBytes, usize, *NativeImport) bool;
extern fn r4os_rust_mod_reloc_record(*const Header, NativeBytes, usize, *Relocation) bool;
extern fn r4os_rust_mod_loaded_export(*const NativeLoadedExport) NativeBytes;
extern fn r4os_rust_mod_name(NativeBytes, u8) u16;
fn nativeBytes(value: []const u8) NativeBytes {
    return .{ .data = value.ptr, .len = value.len };
}
comptime {
    if (@sizeOf(SectionHeader) != 40 or @sizeOf(NativeImport) != 40 or @sizeOf(ResolvedImport) != 96 or @sizeOf(NativeRelocationImage) != 64 or @sizeOf(NativeLoadedExport) != 80) @compileError("module format private C size drift");
    for (.{ .{ "name", 0 }, .{ "name_len", 8 }, .{ "flags", 16 }, .{ "file_off", 20 }, .{ "file_size", 24 }, .{ "mem_size", 28 }, .{ "alignment", 32 } }) |field| {
        if (@offsetOf(SectionHeader, field[0]) != field[1]) @compileError("module section C offset drift");
    }
    for (.{ .{ "module", 0 }, .{ "symbol", 16 }, .{ "min_version", 32 }, .{ "flags", 36 } }) |field| {
        if (@offsetOf(NativeImport, field[0]) != field[1]) @compileError("module import C offset drift");
    }
    for (.{ .{ "module", 0 }, .{ "module_len", 32 }, .{ "symbol", 40 }, .{ "symbol_len", 72 }, .{ "version", 80 }, .{ "address", 88 } }) |field| {
        if (@offsetOf(ResolvedImport, field[0]) != field[1]) @compileError("module resolved import C offset drift");
    }
    const names = [_][]const u8{ "sections", "section_count", "offsets", "offset_count", "image", "image_len", "imports", "import_count" };
    for (names, 0..) |name, i| {
        if (@offsetOf(NativeRelocationImage, name) != i * 8) @compileError("module relocation image C offset drift");
    }
    for (.{ .{ "name", 0 }, .{ "version", 16 }, .{ "section_index", 20 }, .{ "export_offset", 24 }, .{ "sections", 32 }, .{ "section_count", 40 }, .{ "offsets", 48 }, .{ "offset_count", 56 }, .{ "image", 64 } }) |field| {
        if (@offsetOf(NativeLoadedExport, field[0]) != field[1]) @compileError("module loaded export C offset drift");
    }
}


fn resolveImportFromEntry(provider: *const Entry, import: ImportRecord, importer_name: []const u8) ?ResolvedImport {
    var result: ResolvedImport = undefined;
    return if (r4os_rust_mod_resolve_logged(provider, nativeBytes(import.module), nativeBytes(import.symbol), import.min_version, import.flags, nativeBytes(importer_name), &result)) result else null;
}

fn countResolvedImports(imports: []const ResolvedImport) u32 {
    return r4os_rust_mod_resolved_count(imports.ptr, imports.len);
}

fn readSections(header: Header, bytes: []const u8, out: []SectionHeader) bool {
    return r4os_rust_mod_sections(nativeBytes(bytes), &header, out.ptr, out.len);
}

fn validateSectionKinds(sections: []const SectionHeader, kind: Kind) bool {
    return r4os_rust_mod_section_kinds(sections.ptr, sections.len, @intFromEnum(kind));
}

fn interfaceMajorFromName(name: []const u8) ?u16 {
    const value = r4os_rust_mod_name(nativeBytes(name), 5);
    return if (value == 0) null else value;
}

fn layoutSections(sections: []const SectionHeader, section_offsets: []usize) ?usize {
    var size: usize = undefined;
    const offsets = section_offsets[0..sections.len];
    return if (r4os_rust_mod_layout(sections.ptr, sections.len, offsets.ptr, &size)) size else null;
}

fn applyRelocation(reloc: Relocation, sections: []const SectionHeader, section_offsets: []const usize, image: []u8, resolved_imports: []const ResolvedImport) RelocApplyResult {
    const io = NativeRelocationImage{ .sections = sections.ptr, .section_count = sections.len, .offsets = section_offsets.ptr, .offset_count = section_offsets.len, .image = image.ptr, .image_len = image.len, .imports = resolved_imports.ptr, .import_count = resolved_imports.len };
    return @enumFromInt(r4os_rust_mod_reloc(reloc, &io));
}

fn validateEntries(header: Header, bytes: []const u8) bool {
    return r4os_rust_mod_validate(nativeBytes(bytes), &header, 1);
}

fn validateExports(header: Header, bytes: []const u8) bool {
    return r4os_rust_mod_validate(nativeBytes(bytes), &header, 3);
}

fn findByName(name: []const u8) ?usize {
    const result = r4os_rust_mod_find_name(nativeBytes(name));
    return if (result == @import("std").math.maxInt(usize)) null else result;
}

fn hasR4lExtension(name: []const u8) bool {
    return r4os_rust_mod_name(nativeBytes(name), 3) != 0;
}

fn validModuleName(name: []const u8) bool {
    return r4os_rust_mod_name(nativeBytes(name), 0) != 0;
}

fn validSymbolName(name: []const u8) bool {
    return r4os_rust_mod_name(nativeBytes(name), 2) != 0;
}

fn copyBytes(src: []const u8, dst: []u8) usize {
    const len = @min(src.len, dst.len);
    if (len > 0) @memcpy(dst[0..len], src[0..len]);
    return len;
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

const RelocApplyResult = enum(u8) { ok, unknown_type, bad_range, unresolved_import };

fn kindFromRaw(raw: u16) ?Kind { return module_r4m.kindFromRaw(raw); }

test "R4D resources do not consume executable image memory or relocation addresses" {
    const std = @import("std");
    const testing = std.testing;
    const metadata = "NVIDIA\x00module.version=0.1.144\x00firmware.version=570.144\x00";
    const labels = module_r4m.VersionLabels.parse(metadata);
    try testing.expectEqualStrings("0.1.144", std.mem.sliceTo(&labels.module, 0));
    try testing.expectEqualStrings("570.144", std.mem.sliceTo(&labels.firmware, 0));
    const bad_labels = module_r4m.VersionLabels.parse("module.version=12345678901234567890123456789012\x00firmware.version=x\ny\x00");
    try testing.expectEqual(@as(u8, 0), bad_labels.module[0]);
    try testing.expectEqual(@as(u8, 0), bad_labels.firmware[0]);
    try testing.expectEqual(@as(u8, 0), module_r4m.VersionLabels.parse("NVIDIA\x00").firmware[0]);
    const code = SectionHeader{ .name = ".text\x00\x00\x00".*, .name_len = 5, .flags = 3, .file_off = 256, .file_size = 16, .mem_size = 16, .alignment = 16 };
    const resource = SectionHeader{ .name = ".rsrc\x00\x00\x00".*, .name_len = 5, .flags = 0, .file_off = 512, .file_size = 64 * 1024 * 1024, .mem_size = 64 * 1024 * 1024, .alignment = 16 };
    const data = SectionHeader{ .name = ".data\x00\x00\x00".*, .name_len = 5, .flags = 5, .file_off = 272, .file_size = 16, .mem_size = 16, .alignment = 16 };
    var sections = [_]SectionHeader{ code, resource, data };
    var offsets: [3]usize = undefined;
    try testing.expect(validateSectionKinds(&sections, .r4d));
    try testing.expect(validateSectionKinds(&sections, .r4p));
    try testing.expect(!validateSectionKinds(&sections, .r4l));
    try testing.expectEqual(@as(?usize, 8192), layoutSections(&sections, &offsets));
    try testing.expectEqualSlices(usize, &.{ 0, 0, 4096 }, &offsets);
    var image: [8192]u8 = .{0xa5} ** 8192;
    var relocation = Relocation{ .kind = R4M_RELOC_ABS64, .patch_section = 0, .patch_offset = 0, .target_section = 1, .target_offset = 0, .addend = 0 };
    try testing.expectEqual(RelocApplyResult.bad_range, applyRelocation(relocation, &sections, &offsets, &image, &.{}));
    relocation.patch_section = 1;
    relocation.target_section = 0;
    try testing.expectEqual(RelocApplyResult.bad_range, applyRelocation(relocation, &sections, &offsets, &image, &.{}));
    for (image) |byte| try testing.expectEqual(@as(u8, 0xa5), byte);
    relocation.patch_section = 2;
    try testing.expectEqual(RelocApplyResult.ok, applyRelocation(relocation, &sections, &offsets, &image, &.{}));
    try testing.expectEqual(@intFromPtr(&image), readLe64(image[4096..4104]));
    sections[2] = resource;
    try testing.expect(!validateSectionKinds(&sections, .r4d));
    sections[2] = data;
    inline for (.{ @as(u32, 1), @as(u32, 2), @as(u32, 8) }) |flags| {
        sections[1].flags = flags;
        try testing.expect(!validateSectionKinds(&sections, .r4d));
    }
    sections[1] = resource;
    sections[1].alignment = 8;
    try testing.expect(!validateSectionKinds(&sections, .r4d));
    sections[1] = resource;
    sections[1].file_off += 1;
    try testing.expect(!validateSectionKinds(&sections, .r4d));
    sections[1] = resource;
    sections[1].name = ".other\x00\x00".*;
    sections[1].name_len = 6;
    try testing.expect(!validateSectionKinds(&sections, .r4d));
}

test "R4D byte admission rejects entries and exports into file resources" {
    const std = @import("std");
    const testing = std.testing;
    var bytes = [_]u8{0} ** 1024;
    var header = std.mem.zeroes(Header);
    header.kind_raw = @intFromEnum(Kind.r4d);
    header.section_off = 64;
    header.section_count = 2;
    header.entry_off = 128;
    header.entry_count = 1;
    header.export_off = 144;
    header.export_count = 1;
    @memcpy(bytes[64..69], ".text");
    writeLe32(bytes[72..76], 3);
    writeLe32(bytes[76..80], 256);
    writeLe32(bytes[80..84], 16);
    writeLe32(bytes[84..88], 16);
    writeLe32(bytes[88..92], 16);
    @memcpy(bytes[96..101], ".rsrc");
    writeLe32(bytes[108..112], 512);
    writeLe32(bytes[112..116], 32);
    writeLe32(bytes[116..120], 32);
    writeLe32(bytes[120..124], 16);
    writeLe32(bytes[144..148], 192);
    writeLe32(bytes[156..160], 1);
    @memcpy(bytes[192..202], "DriverInit");
    var sections: [2]SectionHeader = undefined;
    try testing.expect(readSections(header, &bytes, &sections));
    try testing.expect(validateEntries(header, &bytes));
    try testing.expect(validateExports(header, &bytes));
    writeLe32(bytes[132..136], 1);
    writeLe32(bytes[148..152], 1);
    try testing.expect(!validateEntries(header, &bytes));
    try testing.expect(!validateExports(header, &bytes));
    header.kind_raw = @intFromEnum(Kind.r4l);
    try testing.expect(!readSections(header, &bytes, &sections));
}

test "Runtime-R4L identity parser fails closed" {
    const t = @import("std").testing;
    try t.expect(module_r4m.validImportFlags("R4NV", module_r4m.IMPORT_FLAG_OPTIONAL));
    try t.expect(!module_r4m.validImportFlags("r4sys", module_r4m.IMPORT_FLAG_OPTIONAL));
    try t.expect(!module_r4m.validImportFlags("R4NV", 2));
    var provider: Entry = .{ .kind = .r4l };
    provider.name_len = copyBytes("R4NV", &provider.name);
    provider.exports[0] = .{ .used = true, .version = 2, .address = 0x123456789abc };
    provider.exports[0].name_len = copyBytes("API_V1", &provider.exports[0].name);
    provider.export_count = 1;
    const optional = ImportRecord{ .module = "R4NV", .symbol = "API_V1", .min_version = 2, .flags = module_r4m.IMPORT_FLAG_OPTIONAL };
    const present = resolveImportFromEntry(&provider, optional, "consumer").?;
    try t.expectEqual(@as(u64, 0x123456789abc), present.address);
    var request = optional;
    request.min_version = 3;
    const absent = resolveImportFromEntry(&provider, request, "consumer").?;
    try t.expect(absent.address == 0 and absent.version == 0);
    try t.expectEqualStrings("R4NV", absent.module[0..absent.module_len]);
    request.symbol = "NOT_FOUND";
    try t.expectEqual(@as(u64, 0), resolveImportFromEntry(&provider, request, "consumer").?.address);
    request.flags = 0;
    try t.expect(resolveImportFromEntry(&provider, request, "consumer") == null);
    try t.expectEqual(@as(u32, 1), countResolvedImports(&.{present, absent}));
    provider.kind = .r4d;
    try t.expect(resolveImportFromEntry(&provider, optional, "consumer") == null);
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
        .flags = R4M_SECTION_FLAG_ALLOC | R4M_SECTION_FLAG_EXEC,
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
        .flags = R4M_SECTION_FLAG_ALLOC | R4M_SECTION_FLAG_EXEC,
        .mem_size = 0x40,
        .runtime_base = std.math.maxInt(u64) - 0x1f,
    };

    slot_claims[slot] = claimValue(7, .published);
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
