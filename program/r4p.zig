const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const k = @import("../kernel/log.zig");
const loader_perf = @import("../kernel/loader_perf.zig");
const module_file = @import("../kernel/module_file.zig");
const module_r4m = @import("../kernel/module_r4m.zig");
const protocol_api = @import("../kernel/protocol_api.zig");
const registry = @import("../protocol/registry.zig");
const modsys = @import("../kernel/modules.zig");
const scheduler = @import("../sched/scheduler.zig");

const VERSION: u16 = 1;
const MAX_MODULES: usize = 32;
const MAX_DEPENDENCIES: usize = 16;
const MAX_PATH: usize = 96;
const R4M_HEADER_SIZE: usize = 64;
const MAX_R4M_METADATA_PROBE: usize = 2048;
const MAX_ACTIVATION_DEPTH: usize = MAX_MODULES;

pub const ROLE_STATE_MISSING: u8 = 0;
pub const ROLE_STATE_LOADED: u8 = 1;
pub const ROLE_STATE_ACTIVE: u8 = 2;
pub const ROLE_STATE_BLOCKED: u8 = 3;
pub const ROLE_STATE_ERROR: u8 = 4;
pub const ROLE_STATE_DISABLED: u8 = 5;

const InitFn = *const fn (*const protocol_api.Table) callconv(.c) i32;
const ShutdownFn = *const fn () callconv(.c) i32;
const QueryFn = *const fn (*protocol_api.ProtocolStatus) callconv(.c) i32;
const DispatchFn = *const fn (u32, *const protocol_api.ProtocolBuffer, *protocol_api.ProtocolBuffer) callconv(.c) i32;

const Module = struct {
    used: bool = false,
    initialized: bool = false,
    source: registry.Source = .r4p,
    module_slot: usize = 0,
    registry_slot: usize = 0,
    name: [registry.MAX_NAME]u8 = .{0} ** registry.MAX_NAME,
    name_len: usize = 0,
    role: [registry.MAX_ROLE]u8 = .{0} ** registry.MAX_ROLE,
    role_len: usize = 0,
    dependencies: [MAX_DEPENDENCIES][registry.MAX_ROLE]u8 = .{.{0} ** registry.MAX_ROLE} ** MAX_DEPENDENCIES,
    dependency_lens: [MAX_DEPENDENCIES]usize = .{0} ** MAX_DEPENDENCIES,
    dependency_count: usize = 0,
    init: ?InitFn = null,
    shutdown: ?ShutdownFn = null,
    query: ?QueryFn = null,
    dispatch_fn: ?DispatchFn = null,
};

const R4MProtocolInfo = struct {
    name: [registry.MAX_NAME]u8 = .{0} ** registry.MAX_NAME,
    name_len: usize = 0,
    role: [registry.MAX_ROLE]u8 = .{0} ** registry.MAX_ROLE,
    role_len: usize = 0,
    category: u16 = 0,
    dependencies: [MAX_DEPENDENCIES][registry.MAX_ROLE]u8 = .{.{0} ** registry.MAX_ROLE} ** MAX_DEPENDENCIES,
    dependency_lens: [MAX_DEPENDENCIES]usize = .{0} ** MAX_DEPENDENCIES,
    dependency_count: usize = 0,
};

const CandidateState = enum(u8) {
    cataloged,
    loading,
    finished,
    invalid,
};

const Candidate = struct {
    used: bool = false,
    state: CandidateState = .cataloged,
    file_source: ?module_file.FileSource = null,
    registry_slot: usize = 0,
    module_index: ?usize = null,
    file_name: [64]u8 = .{0} ** 64,
    file_name_len: usize = 0,
    path: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    path_len: usize = 0,
    info: R4MProtocolInfo = .{},
};

pub const RoleRuntimeStatus = struct {
    active_r4p: bool = false,
    builtin_fallback: bool = false,
    state: u8 = ROLE_STATE_MISSING,
};

var modules: [MAX_MODULES]Module = .{Module{}} ** MAX_MODULES;
var candidates: [MAX_MODULES]Candidate = .{Candidate{}} ** MAX_MODULES;
var initialized: bool = false;
var disk_catalog_loaded: bool = false;
var late_duplicate_skipped: usize = 0;
var preload_headers: usize = 0;
var cataloged_roles: usize = 0;
var cataloged_candidates: usize = 0;
var demand_loads: usize = 0;
var activation_gate: u8 = 0;

pub fn loadAll() void {
    ensureInitialized();
    if (disk_catalog_loaded) {
        refreshPerformanceResults();
        return;
    }
    disk_catalog_loaded = true;
    late_duplicate_skipped = 0;
    cataloged_roles = 0;
    cataloged_candidates = 0;
    demand_loads = 0;

    const volume = vfs.volumeForDrive('C') orelse {
        k.puts("[R4P] C: FAT32 volume not mounted\r\n");
        refreshPerformanceResults();
        return;
    };
    var resolve_req = fs_request.begin(.loader_read, 'C') orelse return;
    var resolve_ok = false;
    const dir_cluster = vfs.resolvePath(volume, "/R4OS/PROTOCOLS") orelse {
        fs_request.finish(&resolve_req, resolve_ok);
        k.puts("[R4P] /R4OS/PROTOCOLS not found\r\n");
        refreshPerformanceResults();
        return;
    };
    resolve_ok = true;
    fs_request.finish(&resolve_req, resolve_ok);

    k.puts("[R4P] scanning /R4OS/PROTOCOLS\r\n");
    var index: usize = 0;
    while (true) : (index += 1) {
        var name_buf: [64]u8 = .{0} ** 64;
        var entry_req = fs_request.begin(.loader_read, 'C') orelse break;
        var entry_ok = false;
        const entry_start = loader_perf.now();
        const maybe_entry = vfs.readDirectoryEntry(volume, dir_cluster, index, name_buf[0..]);
        entry_ok = true;
        fs_request.finish(&entry_req, entry_ok);
        loader_perf.addR4pScanTicks(entry_start);
        const entry = maybe_entry orelse break;
        loader_perf.recordR4pScanEntry();
        const name = zName(name_buf[0..]);
        if (entry.isDir() or !hasR4pExtension(name)) continue;
        loader_perf.recordR4pCandidate();
        _ = catalogModuleForEntry(volume, entry, name);
    }

    refreshPerformanceResults();
    k.puts("[R4P] catalog roles=");
    k.putDec(cataloged_roles);
    k.puts(" candidates=");
    k.putDec(cataloged_candidates);
    k.puts(" active=");
    k.putDec(activeR4pCount());
    k.puts(" demand-full=");
    k.putDec(demand_loads);
    if (late_duplicate_skipped != 0) {
        k.puts(" alternates=");
        k.putDec(late_duplicate_skipped);
    }
    k.puts("\r\n");
}

pub fn loadPreloadModule(name: []const u8, bytes: []const u8) bool {
    ensureInitialized();
    var path_buf: [MAX_PATH]u8 = .{0} ** MAX_PATH;
    const path = buildModulePath("PRELOAD:\\", name, &path_buf);
    if (loadModuleBytes(bytes, name, path, .preload)) {
        preload_headers += 1;
        return true;
    }
    return false;
}

pub fn finishPreload() void {
    ensureInitialized();
    resolveAndInit();
    k.puts("[R4P] preload discovered=");
    k.putDec(preload_headers);
    k.puts(" active=");
    k.putDec(activeR4pCount());
    k.puts("\r\n");
}

pub fn dumpStatus() void {
    registry.dumpStatus();
}

pub fn dispatch(role: []const u8, op: u32, in_buffer: *const protocol_api.ProtocolBuffer, out_buffer: *protocol_api.ProtocolBuffer) i32 {
    if (!ensureRoleActive(role)) return -5;
    const module = activeModuleForRole(role) orelse return -5;
    const dispatch_fn = module.dispatch_fn orelse return -4;
    protocol_api.enter(module.registry_slot);
    const result = dispatch_fn(op, in_buffer, out_buffer);
    protocol_api.leave();
    return result;
}

pub fn hasActiveR4p(role: []const u8) bool {
    return ensureRoleActive(role);
}

pub fn activeSourceName(role: []const u8) []const u8 {
    const entry = registry.moduleEntryForRole(role) orelse return "fallback";
    if (entry.state != .active) return "fallback";
    return registry.sourceName(entry.source);
}

pub fn requiredSourceName(role: []const u8) []const u8 {
    const entry = registry.moduleEntryForRole(role) orelse return "none";
    if (entry.state != .active) return "none";
    return registry.sourceName(entry.source);
}

pub fn roleRuntimeStatus(role: []const u8) RoleRuntimeStatus {
    const entry = registry.r4pEntryForRole(role);
    return .{
        .active_r4p = activeModuleForRole(role) != null,
        .builtin_fallback = registry.builtinEntryForRole(role) != null,
        .state = if (entry) |e| roleStateValue(e.state) else ROLE_STATE_MISSING,
    };
}

fn activeModuleForRole(role: []const u8) ?*const Module {
    var i: usize = 0;
    while (i < modules.len) : (i += 1) {
        if (!modules[i].used or !modules[i].initialized) continue;
        if (!nameEq(moduleRole(&modules[i]), role)) continue;
        const entry = registry.get(modules[i].registry_slot) orelse continue;
        if (isModuleSource(entry.source) and entry.state == .active) return &modules[i];
    }
    return null;
}

fn catalogModuleForEntry(volume: vfs.Volume, entry: vfs.Entry, file_name: []const u8) bool {
    if (entry.size == 0) return false;
    const read_start = loader_perf.now();
    const info = readR4MProtocolInfoFromFile(volume, entry, file_name) orelse {
        loader_perf.addR4pReadTicks(read_start);
        k.puts("[R4P] invalid R4M0 module: ");
        k.puts(file_name);
        k.puts("\r\n");
        return false;
    };
    loader_perf.addR4pReadTicks(read_start);
    if (!protocolInfoFits(info, file_name)) return false;
    const info_role = protocolInfoRole(&info);

    var registry_slot: usize = undefined;
    if (registry.moduleEntryForRole(info_role)) |existing| {
        if (existing.source == .preload or existing.state != .loaded) {
            late_duplicate_skipped += 1;
            k.puts("[R4P] skip duplicate role ");
            k.puts(info_role);
            k.puts(" from ");
            k.puts(file_name);
            k.puts("\r\n");
            return false;
        }
        registry_slot = registrySlotForCatalogRole(info_role) orelse {
            late_duplicate_skipped += 1;
            return false;
        };
        late_duplicate_skipped += 1;
    } else {
        registry_slot = registry.catalogR4p(protocolInfoName(&info), info_role, info.category, VERSION, protocol_api.VERSION) orelse {
            k.puts("[R4P] registry full for role ");
            k.puts(info_role);
            k.puts("\r\n");
            return false;
        };
        cataloged_roles += 1;
    }

    const candidate_slot = freeCandidateSlot() orelse {
        registry.setError(registry_slot, -3, "lazy candidate table full");
        k.puts("[R4P] candidate table full\r\n");
        return false;
    };
    var candidate = Candidate{
        .used = true,
        .file_source = module_file.FileSource{
            .volume = volume,
            .entry = entry,
            .drive_letter = 'C',
        },
        .registry_slot = registry_slot,
        .info = info,
    };
    candidate.file_name_len = copyBytes(file_name, candidate.file_name[0..]);
    candidate.path_len = copyBytes("C:\\R4OS\\PROTOCOLS\\", candidate.path[0..]);
    candidate.path_len += copyBytes(file_name, candidate.path[candidate.path_len..]);
    candidates[candidate_slot] = candidate;
    cataloged_candidates += 1;
    return true;
}

fn loadModuleBytes(bytes: []const u8, file_name: []const u8, path: []const u8, source: registry.Source) bool {
    const info = parseR4MProtocolInfo(bytes, file_name) orelse {
        k.puts("[R4P] invalid R4M0 module: ");
        k.puts(file_name);
        k.puts("\r\n");
        return false;
    };
    return loadModuleBytesWithInfo(bytes, info, file_name, path, source);
}

fn loadModuleBytesWithInfo(bytes: []const u8, info: R4MProtocolInfo, file_name: []const u8, path: []const u8, source: registry.Source) bool {
    const module_slot = freeModuleSlot() orelse {
        k.puts("[R4P] module table full\r\n");
        return false;
    };
    if (!protocolInfoFits(info, file_name)) return false;
    const info_name = protocolInfoName(&info);
    const info_role = protocolInfoRole(&info);
    if (registry.moduleEntryForRole(info_role) != null) {
        if (source == .r4p) late_duplicate_skipped += 1;
        k.puts("[R4P] skip duplicate role ");
        k.puts(info_role);
        k.puts(" from ");
        k.puts(file_name);
        k.puts("\r\n");
        return false;
    }
    const loaded_module_slot = modsys.loadResolvedBytes(bytes, .r4p, file_name, path) orelse return false;
    const init_addr = modsys.exportAddress(loaded_module_slot, "ProtocolInit", 1) orelse return missingExport("ProtocolInit");
    const shutdown_addr = modsys.exportAddress(loaded_module_slot, "ProtocolShutdown", 1) orelse return missingExport("ProtocolShutdown");
    const query_addr = modsys.exportAddress(loaded_module_slot, "ProtocolQuery", 1) orelse return missingExport("ProtocolQuery");
    const dispatch_addr = modsys.exportAddress(loaded_module_slot, "ProtocolDispatch", 1) orelse return missingExport("ProtocolDispatch");
    const registry_slot = switch (source) {
        .preload => registry.beginLoadPreload(info_name, info_role, info.category, VERSION, protocol_api.VERSION),
        else => registry.beginLoadR4p(info_name, info_role, info.category, VERSION, protocol_api.VERSION),
    } orelse {
        k.puts("[R4P] duplicate role or registry full: ");
        k.puts(info_role);
        k.puts("\r\n");
        return false;
    };

    modules[module_slot] = .{
        .used = true,
        .source = source,
        .module_slot = loaded_module_slot,
        .registry_slot = registry_slot,
        .dependency_count = info.dependency_count,
        .init = @ptrFromInt(init_addr),
        .shutdown = @ptrFromInt(shutdown_addr),
        .query = @ptrFromInt(query_addr),
        .dispatch_fn = @ptrFromInt(dispatch_addr),
    };
    storeProtocolInfo(&modules[module_slot], info);
    return true;
}

fn loadCatalogCandidate(candidate_index: usize) ?usize {
    if (candidate_index >= candidates.len or !candidates[candidate_index].used) return null;
    const candidate = &candidates[candidate_index];
    const file_source = candidate.file_source orelse return null;
    const info = candidate.info;
    const file_name = candidateFileName(candidate);
    const path = candidatePath(candidate);
    const module_slot = freeModuleSlot() orelse {
        k.puts("[R4P] module table full\r\n");
        return null;
    };
    if (!protocolInfoFits(info, file_name)) return null;
    const info_name = protocolInfoName(&info);
    const info_role = protocolInfoRole(&info);
    if (!registry.selectCatalogR4p(candidate.registry_slot, info_name, info_role, info.category, VERSION, protocol_api.VERSION)) return null;

    demand_loads += 1;
    k.puts("[R4P] demand role=");
    k.puts(info_role);
    k.puts(" file=");
    k.puts(file_name);
    k.puts("\r\n");
    const read_start = loader_perf.now();
    const loaded_module_slot = modsys.loadResolvedFile(file_source, .r4p, file_name, path) orelse {
        loader_perf.addR4pReadTicks(read_start);
        return null;
    };
    loader_perf.addR4pReadTicks(read_start);
    const init_addr = modsys.exportAddress(loaded_module_slot, "ProtocolInit", 1) orelse {
        _ = missingExport("ProtocolInit");
        return null;
    };
    const shutdown_addr = modsys.exportAddress(loaded_module_slot, "ProtocolShutdown", 1) orelse {
        _ = missingExport("ProtocolShutdown");
        return null;
    };
    const query_addr = modsys.exportAddress(loaded_module_slot, "ProtocolQuery", 1) orelse {
        _ = missingExport("ProtocolQuery");
        return null;
    };
    const dispatch_addr = modsys.exportAddress(loaded_module_slot, "ProtocolDispatch", 1) orelse {
        _ = missingExport("ProtocolDispatch");
        return null;
    };

    modules[module_slot] = .{
        .used = true,
        .source = .r4p,
        .module_slot = loaded_module_slot,
        .registry_slot = candidate.registry_slot,
        .dependency_count = info.dependency_count,
        .init = @ptrFromInt(init_addr),
        .shutdown = @ptrFromInt(shutdown_addr),
        .query = @ptrFromInt(query_addr),
        .dispatch_fn = @ptrFromInt(dispatch_addr),
    };
    storeProtocolInfo(&modules[module_slot], info);
    candidate.module_index = module_slot;
    return module_slot;
}

const ActivationResult = enum {
    active,
    invalid_candidate,
    inactive,
};

fn ensureRoleActive(role: []const u8) bool {
    if (activeModuleForRole(role) != null) return true;
    acquireActivationGate();
    defer releaseActivationGate();
    return ensureRoleActiveSerial(role, 0);
}

fn ensureRoleActiveSerial(role: []const u8, depth: usize) bool {
    if (activeModuleForRole(role) != null) return true;
    if (depth >= MAX_ACTIVATION_DEPTH) return false;

    var saw_candidate = false;
    var saw_loading = false;
    var i: usize = 0;
    while (i < candidates.len) : (i += 1) {
        if (!candidates[i].used or !nameEq(protocolInfoRole(&candidates[i].info), role)) continue;
        saw_candidate = true;
        switch (candidates[i].state) {
            .loading => saw_loading = true,
            .finished => return activeModuleForRole(role) != null,
            .invalid => continue,
            .cataloged => switch (activateCandidate(i, depth)) {
                .active => return true,
                .inactive => return false,
                .invalid_candidate => continue,
            },
        }
    }
    if (saw_loading) return false;
    if (saw_candidate) {
        if (registrySlotForCatalogRole(role)) |slot| registry.setError(slot, -4, "all lazy candidates invalid");
        refreshPerformanceResults();
    }
    return false;
}

fn activateCandidate(candidate_index: usize, depth: usize) ActivationResult {
    const candidate = &candidates[candidate_index];
    candidate.state = .loading;
    const module_index = loadCatalogCandidate(candidate_index) orelse {
        candidate.state = .invalid;
        return .invalid_candidate;
    };
    const module = &modules[module_index];

    var dependency_index: usize = 0;
    while (dependency_index < module.dependency_count) : (dependency_index += 1) {
        const dependency = moduleDependency(module, dependency_index);
        if (dependency.len == 0 or !ensureRoleActiveSerial(dependency, depth + 1)) {
            const dep_entry = registry.r4pEntryForRole(dependency);
            const note = if (dep_entry == null)
                "dependency missing"
            else if (dep_entry.?.state == .loaded)
                "dependency cycle or unresolved dependency"
            else
                "dependency blocked";
            registry.setBlocked(module.registry_slot, -5, note);
            module.initialized = true;
            candidate.state = .finished;
            refreshPerformanceResults();
            return .inactive;
        }
    }

    initModule(module);
    module.initialized = true;
    candidate.state = .finished;
    refreshPerformanceResults();
    return if (activeModuleForRole(moduleRole(module)) != null) .active else .inactive;
}

fn acquireActivationGate() void {
    while (@cmpxchgStrong(u8, &activation_gate, 0, 1, .acquire, .monotonic) != null) scheduler.yield();
}

fn releaseActivationGate() void {
    @atomicStore(u8, &activation_gate, 0, .release);
}

fn resolveAndInit() void {
    const resolve_start = loader_perf.now();
    var progress = true;
    while (progress) {
        progress = false;
        var i: usize = 0;
        while (i < modules.len) : (i += 1) {
            if (!modules[i].used or modules[i].initialized) continue;
            const dep = dependencyState(&modules[i]);
            if (dep == .missing) {
                registry.setBlocked(modules[i].registry_slot, -5, "dependency missing");
                modules[i].initialized = true;
                progress = true;
            } else if (dep == .blocked) {
                registry.setBlocked(modules[i].registry_slot, -5, "dependency blocked");
                modules[i].initialized = true;
                progress = true;
            } else if (dep == .ready) {
                initModule(&modules[i]);
                modules[i].initialized = true;
                progress = true;
            }
        }
    }

    var i: usize = 0;
    while (i < modules.len) : (i += 1) {
        if (!modules[i].used or modules[i].initialized) continue;
        registry.setBlocked(modules[i].registry_slot, -5, "dependency cycle or unresolved dependency");
        modules[i].initialized = true;
    }
    loader_perf.addR4pResolveTicks(resolve_start);
}

const DepState = enum {
    ready,
    wait,
    missing,
    blocked,
};

fn dependencyState(module: *const Module) DepState {
    if (module.dependency_count == 0) return .ready;
    if (module.dependency_count > MAX_DEPENDENCIES) return .missing;
    var i: usize = 0;
    while (i < module.dependency_count) : (i += 1) {
        const dep = moduleDependency(module, i);
        if (dep.len == 0) return .missing;
        const entry = registry.r4pEntryForRole(dep) orelse return .missing;
        switch (entry.state) {
            .active => continue,
            .blocked, .err, .disabled => return .blocked,
            .loaded => return .wait,
            else => return .blocked,
        }
    }
    return .ready;
}

fn initModule(module: *const Module) void {
    k.puts("[R4P] load ");
    k.puts(moduleName(module));
    k.puts(" role=");
    k.puts(moduleRole(module));
    k.puts(" source=");
    k.puts(registry.sourceName(module.source));
    k.puts("\r\n");

    const init = module.init orelse {
        registry.setError(module.registry_slot, -4, "missing init export");
        return;
    };
    const init_start = loader_perf.now();
    protocol_api.enter(module.registry_slot);
    const result = init(&protocol_api.table);
    protocol_api.leave();
    loader_perf.addR4pInitTicks(init_start);
    if (result != 0) {
        registry.setError(module.registry_slot, result, "init failed");
        k.puts("[R4P] init failed ");
        k.puts(moduleName(module));
        k.puts(" code=");
        putSignedDec(result);
        k.puts("\r\n");
        return;
    }

    var status: protocol_api.ProtocolStatus = .{};
    const query = module.query orelse {
        registry.setState(module.registry_slot, .active);
        registry.setNote(module.registry_slot, "active; query export missing");
        return;
    };
    protocol_api.enter(module.registry_slot);
    const q = query(&status);
    protocol_api.leave();
    if (q == 0) {
        registry.setStatus(module.registry_slot, status.state, zName(status.note[0..]));
    } else {
        registry.setState(module.registry_slot, .active);
        registry.setNote(module.registry_slot, "active; query failed");
    }
}

fn activeR4pCount() usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < registry.MAX_PROTOCOLS) : (i += 1) {
        const e = registry.entryAt(i) orelse continue;
        if (isModuleSource(e.source) and e.state == .active) count += 1;
    }
    return count;
}

fn blockedR4pCount() usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < registry.MAX_PROTOCOLS) : (i += 1) {
        const e = registry.entryAt(i) orelse continue;
        if (isModuleSource(e.source) and e.state == .blocked) count += 1;
    }
    return count;
}

fn failedR4pCount() usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < registry.MAX_PROTOCOLS) : (i += 1) {
        const e = registry.entryAt(i) orelse continue;
        if (isModuleSource(e.source) and e.state == .err) count += 1;
    }
    return count;
}

fn refreshPerformanceResults() void {
    loader_perf.recordR4pResults(cataloged_roles, activeR4pCount(), blockedR4pCount(), failedR4pCount());
}

fn ensureInitialized() void {
    if (initialized) return;
    registry.init();
    modules = .{Module{}} ** MAX_MODULES;
    candidates = .{Candidate{}} ** MAX_MODULES;
    preload_headers = 0;
    disk_catalog_loaded = false;
    late_duplicate_skipped = 0;
    cataloged_roles = 0;
    cataloged_candidates = 0;
    demand_loads = 0;
    activation_gate = 0;
    initialized = true;
}

fn isModuleSource(source: registry.Source) bool {
    return source == .r4p or source == .preload;
}

fn roleStateValue(state: registry.State) u8 {
    return switch (state) {
        .loaded => ROLE_STATE_LOADED,
        .active, .fallback => ROLE_STATE_ACTIVE,
        .blocked => ROLE_STATE_BLOCKED,
        .err => ROLE_STATE_ERROR,
        .disabled => ROLE_STATE_DISABLED,
        else => ROLE_STATE_MISSING,
    };
}

fn freeModuleSlot() ?usize {
    var i: usize = 0;
    while (i < modules.len) : (i += 1) {
        if (!modules[i].used) return i;
    }
    return null;
}

fn freeCandidateSlot() ?usize {
    var i: usize = 0;
    while (i < candidates.len) : (i += 1) {
        if (!candidates[i].used) return i;
    }
    return null;
}

fn registrySlotForCatalogRole(role: []const u8) ?usize {
    var i: usize = 0;
    while (i < candidates.len) : (i += 1) {
        if (!candidates[i].used) continue;
        if (nameEq(protocolInfoRole(&candidates[i].info), role)) return candidates[i].registry_slot;
    }
    return null;
}

fn missingExport(name: []const u8) bool {
    k.puts("[R4P] missing export ");
    k.puts(name);
    k.puts("\r\n");
    return false;
}

fn moduleName(module: *const Module) []const u8 {
    return module.name[0..module.name_len];
}

fn moduleRole(module: *const Module) []const u8 {
    return module.role[0..module.role_len];
}

fn moduleDependency(module: *const Module, index: usize) []const u8 {
    if (index >= module.dependency_count or index >= MAX_DEPENDENCIES) return "";
    return module.dependencies[index][0..module.dependency_lens[index]];
}

fn candidateFileName(candidate: *const Candidate) []const u8 {
    return candidate.file_name[0..candidate.file_name_len];
}

fn candidatePath(candidate: *const Candidate) []const u8 {
    return candidate.path[0..candidate.path_len];
}

fn protocolInfoFits(info: R4MProtocolInfo, file_name: []const u8) bool {
    if (info.name_len > registry.MAX_NAME) return metadataTooLong(file_name, "name");
    if (info.role_len > registry.MAX_ROLE) return metadataTooLong(file_name, "role");
    if (info.dependency_count > MAX_DEPENDENCIES) return metadataTooLong(file_name, "dependency-list");
    var i: usize = 0;
    while (i < info.dependency_count) : (i += 1) {
        if (info.dependency_lens[i] > registry.MAX_ROLE) return metadataTooLong(file_name, "dependency");
    }
    return true;
}

fn metadataTooLong(file_name: []const u8, field: []const u8) bool {
    k.puts("[R4P] metadata ");
    k.puts(field);
    k.puts(" too long: ");
    k.puts(file_name);
    k.puts("\r\n");
    return false;
}

fn storeProtocolInfo(module: *Module, info: R4MProtocolInfo) void {
    module.name_len = copyBytes(protocolInfoName(&info), module.name[0..]);
    module.role_len = copyBytes(protocolInfoRole(&info), module.role[0..]);
    module.dependency_count = info.dependency_count;
    var i: usize = 0;
    while (i < info.dependency_count and i < MAX_DEPENDENCIES) : (i += 1) {
        module.dependency_lens[i] = copyBytes(protocolInfoDependency(&info, i), module.dependencies[i][0..]);
    }
}

fn protocolInfoName(info: *const R4MProtocolInfo) []const u8 {
    return info.name[0..@min(info.name_len, info.name.len)];
}

fn protocolInfoRole(info: *const R4MProtocolInfo) []const u8 {
    return info.role[0..@min(info.role_len, info.role.len)];
}

fn protocolInfoDependency(info: *const R4MProtocolInfo, index: usize) []const u8 {
    if (index >= info.dependency_count or index >= MAX_DEPENDENCIES) return "";
    return info.dependencies[index][0..@min(info.dependency_lens[index], info.dependencies[index].len)];
}

fn copyInfoBytes(src: []const u8, dst: []u8) usize {
    const len = @min(src.len, dst.len);
    if (len != 0) @memcpy(dst[0..len], src[0..len]);
    return src.len;
}

fn parseR4MProtocolInfo(bytes: []const u8, fallback_name: []const u8) ?R4MProtocolInfo {
    if (bytes.len >= 4 and memEql(bytes[0..4], "R4P0")) {
        k.puts("[R4P] Legacy module format not supported\r\n");
        return null;
    }
    if (bytes.len < R4M_HEADER_SIZE or !memEql(bytes[0..4], "R4M0")) return null;
    if (readLe16(bytes[8..10]) != @intFromEnum(modsys.Kind.r4p)) return null;
    const meta_off = readLe32(bytes[56..60]);
    const meta_size = readLe32(bytes[60..64]);
    if (!validRange(meta_off, meta_size, bytes.len)) return null;
    const meta = bytes[@intCast(meta_off)..][0..@intCast(meta_size)];
    return parseR4MProtocolInfoMeta(meta, fallback_name);
}

fn readR4MProtocolInfoFromFile(volume: vfs.Volume, entry: vfs.Entry, fallback_name: []const u8) ?R4MProtocolInfo {
    var meta_buf: [MAX_R4M_METADATA_PROBE]u8 = .{0} ** MAX_R4M_METADATA_PROBE;
    const header = module_r4m.readHeader(.{
        .source = .{
            .volume = volume,
            .entry = entry,
            .drive_letter = 'C',
        },
        .file_size = @intCast(entry.size),
        .expected_kind = .r4p,
        .name = "r4p-metadata-probe",
    }) orelse return null;
    const meta = module_r4m.readMetadata(.{
        .source = .{
            .volume = volume,
            .entry = entry,
            .drive_letter = 'C',
        },
        .header = header,
        .out = meta_buf[0..],
        .name = "r4p-metadata-probe",
    }) orelse return null;
    return parseR4MProtocolInfoMeta(meta, fallback_name);
}

fn parseR4MProtocolInfoMeta(meta: []const u8, fallback_name: []const u8) ?R4MProtocolInfo {
    const name = module_r4m.metadataValue(meta, "r4p.name=") orelse trimProtocolExtension(fallback_name);
    const role = module_r4m.metadataValue(meta, "r4p.role=") orelse return null;
    var info = R4MProtocolInfo{
        .category = parseCategoryName(module_r4m.metadataValue(meta, "r4p.category=") orelse "misc") orelse return null,
    };
    info.name_len = copyInfoBytes(name, info.name[0..]);
    info.role_len = copyInfoBytes(role, info.role[0..]);

    var it = module_r4m.metadataIterator(meta);
    while (it.next()) |item| {
        if (startsWith(item, "r4p.dep=")) {
            if (info.dependency_count >= MAX_DEPENDENCIES) return null;
            info.dependency_lens[info.dependency_count] = copyInfoBytes(item["r4p.dep=".len..], info.dependencies[info.dependency_count][0..]);
            info.dependency_count += 1;
        }
    }
    return info;
}

fn startsWith(value: []const u8, prefix: []const u8) bool {
    if (value.len < prefix.len) return false;
    var i: usize = 0;
    while (i < prefix.len) : (i += 1) {
        if (value[i] != prefix[i]) return false;
    }
    return true;
}

fn parseCategoryName(value: []const u8) ?u16 {
    if (nameEq(value, "net")) return 1;
    if (nameEq(value, "usb")) return 2;
    if (nameEq(value, "audio") or nameEq(value, "synth")) return 3;
    if (nameEq(value, "data")) return 4;
    if (nameEq(value, "misc") or nameEq(value, "test")) return 255;
    return null;
}

fn buildModulePath(prefix: []const u8, file_name: []const u8, out: *[MAX_PATH]u8) []const u8 {
    const prefix_len: usize = @min(prefix.len, out.len);
    @memcpy(out[0..prefix_len], prefix[0..prefix_len]);
    var len: usize = prefix_len;
    const copy_len: usize = @min(file_name.len, out.len - len);
    if (copy_len > 0) @memcpy(out[len .. len + copy_len], file_name[0..copy_len]);
    len += copy_len;
    return out[0..len];
}

fn trimProtocolExtension(name: []const u8) []const u8 {
    if (hasR4pExtension(name)) return name[0 .. name.len - 4];
    return name;
}

fn validRange(offset: u32, size: u32, file_len: usize) bool {
    const start: usize = @intCast(offset);
    const len: usize = @intCast(size);
    if (start > file_len) return false;
    if (len > file_len - start) return false;
    return true;
}

fn zString(bytes: []const u8, offset: u32) ?[]const u8 {
    const start: usize = @intCast(offset);
    if (start >= bytes.len) return null;
    var end = start;
    while (end < bytes.len and bytes[end] != 0) : (end += 1) {}
    if (end >= bytes.len or end == start) return null;
    return bytes[start..end];
}

fn zName(buf: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buf.len and buf[len] != 0) : (len += 1) {}
    return buf[0..len];
}

fn hasR4pExtension(name: []const u8) bool {
    return name.len >= 4 and name[name.len - 4] == '.' and upper(name[name.len - 3]) == 'R' and upper(name[name.len - 2]) == '4' and upper(name[name.len - 1]) == 'P';
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn copyBytes(src: []const u8, dst: []u8) usize {
    const len = @min(src.len, dst.len);
    if (len != 0) @memcpy(dst[0..len], src[0..len]);
    return len;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
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

fn writeLe16(bytes: []u8, value: u16) void {
    bytes[0] = @intCast(value & 0x00FF);
    bytes[1] = @intCast(value >> 8);
}

fn writeLe32(bytes: []u8, value: u32) void {
    bytes[0] = @intCast(value & 0x000000FF);
    bytes[1] = @intCast((value >> 8) & 0x000000FF);
    bytes[2] = @intCast((value >> 16) & 0x000000FF);
    bytes[3] = @intCast((value >> 24) & 0x000000FF);
}

fn putSignedDec(value: i32) void {
    if (value < 0) {
        k.puts("-");
        k.putDec(@intCast(-value));
    } else {
        k.putDec(@intCast(value));
    }
}
