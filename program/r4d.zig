const vfs = @import("../fs/vfs.zig");
const fs_request = @import("../fs/request.zig");
const driver_api = @import("../kernel/driver_api.zig");
const driver_registry = @import("../driver/registry.zig");
const k = @import("../kernel/log.zig");
const loader_perf = @import("../kernel/loader_perf.zig");
const module_file = @import("../kernel/module_file.zig");
const module_r4m = @import("../kernel/module_r4m.zig");
const modules = @import("../kernel/modules.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("../kernel/timer.zig");

const VERSION: u32 = 1;
const MAX_RUNTIME_DRIVERS: usize = 16;
const MAX_MODULE_DRIVERS: usize = 16;
const MAX_RUNTIME_NAME: usize = 32;
const MAX_PATH: usize = 96;
const R4M_HEADER_SIZE: usize = 64;
const MAX_R4M_METADATA_PROBE: usize = 2048;
const OWNER_HANDOFF_MILLISECONDS: u64 = 250;

const DriverType = enum(u16) {
    audio = 1,
    storage = 2,
    input = 3,
    synth = 4,
    net = 5,
    misc = 255,
    unknown = 0,
};

const InitFn = *const fn (*const driver_api.Table) callconv(.c) i32;
const ShutdownFn = *const fn () callconv(.c) i32;

const LoadMode = enum {
    probe,
    runtime,
};

const LoadSource = enum {
    disk,
    preload,
};

pub const RuntimeLoadResult = enum(u8) {
    loaded,
    already_active,
    name_invalid,
    file_missing,
    invalid_file,
    init_failed,
    load_failed,
};

const ReadFileError = enum(u8) {
    none,
    volume_missing,
    not_found,
    invalid_path,
    allocation_failed,
    read_failed,
};

const RuntimeDriver = struct {
    used: bool = false,
    quarantined: bool = false,
    generation: u64 = 0,
    name: [MAX_RUNTIME_NAME]u8 = .{0} ** MAX_RUNTIME_NAME,
    name_len: usize = 0,
    driver_type: u16 = 0,
    registry_slot: usize = 0,
    owner: u32 = 0,
    shutdown: ?ShutdownFn = null,
};

const ModuleDriver = struct {
    used: bool = false,
    quarantined: bool = false,
    name: [MAX_RUNTIME_NAME]u8 = .{0} ** MAX_RUNTIME_NAME,
    name_len: usize = 0,
    driver_type: u16 = 0,
    module_slot: usize = 0,
    init: ?InitFn = null,
    shutdown: ?ShutdownFn = null,
};

const R4MDriverInfo = struct {
    name: [MAX_RUNTIME_NAME]u8 = .{0} ** MAX_RUNTIME_NAME,
    name_len: usize = 0,
    driver_type: u16,
};

var runtime_drivers: [MAX_RUNTIME_DRIVERS]RuntimeDriver = .{RuntimeDriver{}} ** MAX_RUNTIME_DRIVERS;
var module_drivers: [MAX_MODULE_DRIVERS]ModuleDriver = .{ModuleDriver{}} ** MAX_MODULE_DRIVERS;
var next_runtime_generation: u64 = 1;
var runtime_lifecycle_guard = sync.UnwindGuard.init("r4d-lifecycle");
var runtime_transition_active: bool = false;

pub fn discoverAll() void {
    if (!enterRuntimeLifecycle(sync.WAIT_FOREVER)) return;
    defer leaveRuntimeLifecycle();
    module_drivers = .{ModuleDriver{}} ** MAX_MODULE_DRIVERS;
    const volume = vfs.volumeForDrive('C') orelse {
        k.puts("[R4D] C: FAT32 volume not mounted\r\n");
        loader_perf.recordR4dResults(0);
        return;
    };
    var resolve_req = fs_request.begin(.loader_read, 'C') orelse return;
    var resolve_ok = false;
    const dir_cluster = vfs.resolvePath(volume, "/R4OS/DRIVERS") orelse {
        fs_request.finish(&resolve_req, resolve_ok);
        k.puts("[R4D] /R4OS/DRIVERS not found\r\n");
        loader_perf.recordR4dResults(0);
        return;
    };
    resolve_ok = true;
    fs_request.finish(&resolve_req, resolve_ok);

    k.puts("[R4D] scanning /R4OS/DRIVERS\r\n");
    var index: usize = 0;
    var discovered: usize = 0;
    var name_buf: [64]u8 = .{0} ** 64;
    while (true) : (index += 1) {
        var entry_req = fs_request.begin(.loader_read, 'C') orelse break;
        var entry_ok = false;
        const entry_start = loader_perf.now();
        const maybe_entry = vfs.readDirectoryEntry(volume, dir_cluster, index, name_buf[0..]);
        entry_ok = true;
        fs_request.finish(&entry_req, entry_ok);
        loader_perf.addR4dScanTicks(entry_start);
        const file_entry = maybe_entry orelse break;
        loader_perf.recordR4dScanEntry();
        const file_name = zName(name_buf[0..]);
        if (file_entry.isDir() or !hasR4dExtension(file_name)) continue;
        loader_perf.recordR4dCandidate();
        if (discoverEntry(volume, file_entry, file_name)) discovered += 1;
    }
    loader_perf.recordR4dResults(discovered);
    k.puts("[R4D] modules discovered=");
    k.putDec(discovered);
    k.puts("\r\n");
}

pub fn loadPath(path: []const u8) bool {
    if (!enterRuntimeLifecycle(sync.WAIT_FOREVER)) return false;
    defer leaveRuntimeLifecycle();
    const source = readFile(path) orelse return false;
    return loadR4MDriverFile(source, .probe, fallbackNameFromPath(path), path);
}

pub fn loadRuntimeName(raw_name: []const u8) bool {
    return runtimeLoadSucceeded(loadRuntimeNameResult(raw_name));
}

pub fn loadRuntimeNameResult(raw_name: []const u8) RuntimeLoadResult {
    if (!enterRuntimeLifecycle(sync.WAIT_FOREVER)) return .load_failed;
    defer leaveRuntimeLifecycle();
    if (raw_name.len == 0) {
        k.puts("Usage: LOAD driver\r\n");
        return .name_invalid;
    }

    if (alreadyActive(raw_name)) return .already_active;

    var path_buf: [96]u8 = undefined;
    const path = buildDriverPath(raw_name, &path_buf) orelse {
        k.puts("[R4D] driver name too long\r\n");
        return .name_invalid;
    };
    var read_error: ReadFileError = .none;
    const source = readFileWithError(path, &read_error) orelse return switch (read_error) {
        .not_found => .file_missing,
        .invalid_path => .invalid_file,
        else => .load_failed,
    };
    return loadR4MDriverFileRuntime(source, fallbackNameFromPath(path), path);
}

pub fn loadPreloadModule(name: []const u8, bytes: []const u8) RuntimeLoadResult {
    if (!enterRuntimeLifecycle(sync.WAIT_FOREVER)) return .load_failed;
    defer leaveRuntimeLifecycle();
    if (name.len == 0) return .name_invalid;
    if (alreadyActive(name)) return .already_active;
    var path_buf: [MAX_PATH]u8 = .{0} ** MAX_PATH;
    const path = buildModulePath("PRELOAD:\\", name, &path_buf);
    if (isR4M(bytes)) return loadR4MPreloadDriverBytes(bytes, name, path);

    if (bytes.len >= 4 and memEql(bytes[0..4], "R4D0")) {
        k.puts("[R4D] Legacy preload module format not supported\r\n");
        return .invalid_file;
    }
    if (bytes.len < R4M_HEADER_SIZE) {
        k.puts("[R4D] bad preload file\r\n");
        return .invalid_file;
    }
    k.puts("[R4D] bad preload R4M0 magic\r\n");
    return .invalid_file;
}

pub fn unloadRuntimeName(raw_name: []const u8) bool {
    if (!enterRuntimeLifecycle(sync.WAIT_FOREVER)) return false;
    defer leaveRuntimeLifecycle();
    if (raw_name.len == 0) {
        k.puts("Usage: UNLOAD driver\r\n");
        return false;
    }

    const slot = findRuntime(raw_name) orelse {
        k.puts("[R4D] driver not loaded: ");
        k.puts(raw_name);
        k.puts("\r\n");
        return false;
    };
    const driver = &runtime_drivers[slot];
    if (driver.quarantined) {
        k.puts("[R4D] driver quarantined until reboot: ");
        k.puts(driver.name[0..driver.name_len]);
        k.puts("\r\n");
        return false;
    }
    const expected_generation = driver.generation;
    const expected_owner = driver.owner;
    const expected_registry_slot = driver.registry_slot;
    if (!driver_api.enterOwner(expected_owner)) {
        k.puts("[R4D] owner context busy\r\n");
        return false;
    }
    if (!runtimeIdentityMatches(driver, expected_generation, expected_owner, expected_registry_slot)) {
        _ = driver_api.leaveOwner();
        k.puts("[R4D] unload target changed while waiting for owner\r\n");
        return false;
    }
    if (driver.quarantined) {
        _ = driver_api.leaveOwner();
        k.puts("[R4D] driver quarantined while waiting for owner\r\n");
        return false;
    }
    driver_registry.setState(expected_registry_slot, .shutdown);
    const shutdown = driver.shutdown orelse {
        k.puts("[R4D] missing shutdown entry\r\n");
        quarantineRuntimeDriver(driver);
        _ = driver_api.leaveOwner();
        return false;
    };
    var cleanup_token = driver_api.prepareOwnerCleanup(expected_owner) orelse {
        k.puts("[R4D] unload vetoed: owner resources still busy\r\n");
        driver_registry.setState(expected_registry_slot, .active);
        _ = driver_api.leaveOwner();
        return false;
    };
    driver_api.beginOwnerShutdown(&cleanup_token);
    const result = shutdown();
    if (result != 0) {
        _ = driver_api.quarantineOwnerCleanup(&cleanup_token);
        quarantineRuntimeDriver(driver);
        _ = driver_api.leaveOwner();
        k.puts("[R4D] shutdown failed code=");
        putSignedDec(result);
        k.puts("\r\n");
        return false;
    }
    const cleanup_ok = driver_api.commitOwnerCleanup(&cleanup_token);
    if (!cleanup_ok) {
        quarantineRuntimeDriver(driver);
        _ = driver_api.leaveOwner();
        k.puts("[R4D] unload cleanup failed\r\n");
        return false;
    }
    driver_registry.markUnloaded(expected_registry_slot);
    k.puts("[R4D] unload ");
    k.puts(driver.name[0..driver.name_len]);
    k.puts(" [OK]\r\n");
    driver.* = RuntimeDriver{};
    _ = driver_api.leaveOwner();
    return true;
}

fn readFile(path: []const u8) ?module_file.FileSource {
    return readFileWithError(path, null);
}

fn readFileWithError(path: []const u8, err: ?*ReadFileError) ?module_file.FileSource {
    if (err) |out| out.* = .none;
    const volume = vfs.volumeForDrive('C') orelse {
        k.puts("[R4D] C: FAT32 volume not mounted\r\n");
        if (err) |out| out.* = .volume_missing;
        return null;
    };
    var req = fs_request.begin(.loader_read, 'C') orelse {
        if (err) |out| out.* = .read_failed;
        return null;
    };
    var ok = false;
    const entry = vfs.resolveEntry(volume, path) orelse {
        fs_request.finish(&req, ok);
        k.puts("[R4D] driver not found: ");
        k.puts(path);
        k.puts("\r\n");
        if (err) |out| out.* = .not_found;
        return null;
    };
    if (entry.isDir() or entry.size == 0) {
        fs_request.finish(&req, ok);
        k.puts("[R4D] invalid driver path\r\n");
        if (err) |out| out.* = .invalid_path;
        return null;
    }
    ok = true;
    fs_request.finish(&req, ok);

    return .{
        .volume = volume,
        .entry = entry,
        .drive_letter = 'C',
    };
}

fn discoverEntry(volume: vfs.Volume, entry: vfs.Entry, file_name: []const u8) bool {
    if (entry.size == 0) return false;
    _ = readR4MDriverInfoFromFile(volume, entry, file_name) orelse return false;
    const source: module_file.FileSource = .{
        .volume = volume,
        .entry = entry,
        .drive_letter = 'C',
    };
    var path_buf: [MAX_PATH]u8 = .{0} ** MAX_PATH;
    const path = buildModulePath("C:\\R4OS\\DRIVERS\\", file_name, &path_buf);
    const probe_start = loader_perf.now();
    const result = loadR4MDriverFile(source, .probe, file_name, path);
    loader_perf.addR4dProbeTicks(probe_start);
    return result;
}

fn loadR4MDriverFile(source: module_file.FileSource, mode: LoadMode, fallback_name: []const u8, path: []const u8) bool {
    const info = readR4MDriverInfoFromFile(source.volume, source.entry, fallback_name) orelse return false;
    const info_name = driverInfoName(&info);
    const existing = findModuleDriver(info_name) orelse blk: {
        const module_slot = modules.loadResolvedFile(source, .r4d, fallback_name, path) orelse return false;
        const init_addr = modules.exportAddress(module_slot, "DriverInit", 1) orelse {
            k.puts("[R4D] missing DriverInit export\r\n");
            return false;
        };
        const shutdown_addr = modules.exportAddress(module_slot, "DriverShutdown", 1) orelse {
            k.puts("[R4D] missing DriverShutdown export\r\n");
            return false;
        };
        const descriptor_slot = freeModuleDriverSlot() orelse {
            k.puts("[R4D] module driver table full\r\n");
            return false;
        };
        storeModuleDriver(descriptor_slot, info_name, info.driver_type, module_slot, @ptrFromInt(init_addr), @ptrFromInt(shutdown_addr));
        break :blk descriptor_slot;
    };

    if (mode == .probe) {
        k.puts("[R4D] module ");
        k.puts(module_drivers[existing].name[0..module_drivers[existing].name_len]);
        k.puts(" recognized\r\n");
        return true;
    }
    return runtimeLoadSucceeded(initRuntimeModuleResult(existing));
}

fn loadR4MDriverFileRuntime(source: module_file.FileSource, fallback_name: []const u8, path: []const u8) RuntimeLoadResult {
    const info = readR4MDriverInfoFromFile(source.volume, source.entry, fallback_name) orelse return .invalid_file;
    const info_name = driverInfoName(&info);
    const existing = findModuleDriver(info_name) orelse blk: {
        const module_slot = modules.loadResolvedFile(source, .r4d, fallback_name, path) orelse return .load_failed;
        const init_addr = modules.exportAddress(module_slot, "DriverInit", 1) orelse {
            k.puts("[R4D] missing DriverInit export\r\n");
            return .invalid_file;
        };
        const shutdown_addr = modules.exportAddress(module_slot, "DriverShutdown", 1) orelse {
            k.puts("[R4D] missing DriverShutdown export\r\n");
            return .invalid_file;
        };
        const descriptor_slot = freeModuleDriverSlot() orelse {
            k.puts("[R4D] module driver table full\r\n");
            return .load_failed;
        };
        storeModuleDriver(descriptor_slot, info_name, info.driver_type, module_slot, @ptrFromInt(init_addr), @ptrFromInt(shutdown_addr));
        break :blk descriptor_slot;
    };

    return initRuntimeModuleResult(existing);
}

fn loadR4MPreloadDriverBytes(bytes: []const u8, fallback_name: []const u8, path: []const u8) RuntimeLoadResult {
    const info = parseR4MDriverInfo(bytes, fallback_name) orelse return .invalid_file;
    const info_name = driverInfoName(&info);
    const existing = findModuleDriver(info_name) orelse blk: {
        const module_slot = modules.loadResolvedBytes(bytes, .r4d, fallback_name, path) orelse return .load_failed;
        const init_addr = modules.exportAddress(module_slot, "DriverInit", 1) orelse {
            k.puts("[R4D] missing DriverInit export\r\n");
            return .invalid_file;
        };
        const shutdown_addr = modules.exportAddress(module_slot, "DriverShutdown", 1) orelse {
            k.puts("[R4D] missing DriverShutdown export\r\n");
            return .invalid_file;
        };
        const descriptor_slot = freeModuleDriverSlot() orelse {
            k.puts("[R4D] module driver table full\r\n");
            return .load_failed;
        };
        storeModuleDriver(descriptor_slot, info_name, info.driver_type, module_slot, @ptrFromInt(init_addr), @ptrFromInt(shutdown_addr));
        break :blk descriptor_slot;
    };

    return initRuntimeModuleResultSource(existing, .preload);
}

fn initRuntimeModule(module_slot: usize) bool {
    return runtimeLoadSucceeded(initRuntimeModuleResult(module_slot));
}

fn initRuntimeModuleResult(module_slot: usize) RuntimeLoadResult {
    return initRuntimeModuleResultSource(module_slot, .disk);
}

fn initRuntimeModuleResultSource(module_slot: usize, source: LoadSource) RuntimeLoadResult {
    const descriptor = &module_drivers[module_slot];
    const name = descriptor.name[0..descriptor.name_len];
    if (descriptor.quarantined) {
        k.puts("[R4D] module quarantined until reboot: ");
        k.puts(name);
        k.puts("\r\n");
        return .load_failed;
    }
    if (findRuntime(name) != null) {
        k.puts("[R4D] already loaded: ");
        k.puts(name);
        k.puts("\r\n");
        return .already_active;
    }

    k.puts("[R4D] load ");
    k.puts(name);
    k.puts(" type=");
    k.puts(driverTypeName(descriptor.driver_type));
    k.puts(" format=r4m\r\n");

    const registry_slot = switch (source) {
        .disk => driver_registry.beginLoadR4d(name, descriptor.driver_type, VERSION),
        .preload => driver_registry.beginLoadPreloadR4d(name, descriptor.driver_type, VERSION),
    } orelse {
        k.puts("[R4D] registry full\r\n");
        return .load_failed;
    };

    const owner = ownerForRegistrySlot(registry_slot);
    const init = descriptor.init orelse {
        driver_registry.setState(registry_slot, .failed);
        return .invalid_file;
    };
    const shutdown = descriptor.shutdown orelse {
        k.puts("[R4D] missing DriverShutdown before init\r\n");
        driver_registry.setState(registry_slot, .failed);
        return .invalid_file;
    };
    if (!driver_api.enterOwner(owner)) {
        k.puts("[R4D] owner context busy\r\n");
        driver_registry.setState(registry_slot, .failed);
        return .load_failed;
    }
    const result = owner_init: {
        defer _ = driver_api.leaveOwner();
        break :owner_init init(&driver_api.table);
    };
    if (result != 0) {
        k.puts("[R4D] init failed code=");
        putSignedDec(result);
        k.puts("\r\n");
        const cleanup_ok = shutdownAndCleanupFailedLoad(shutdown, owner, name, descriptor, registry_slot);
        descriptor.quarantined = !cleanup_ok;
        driver_registry.setState(registry_slot, if (cleanup_ok) .failed else .quarantined);
        return .init_failed;
    }
    driver_registry.setState(registry_slot, .initialized);
    driver_registry.setState(registry_slot, .active);

    const runtime_slot = freeRuntimeSlot() orelse {
        k.puts("[R4D] runtime table full\r\n");
        const cleanup_ok = shutdownAndCleanupFailedLoad(shutdown, owner, name, descriptor, registry_slot);
        descriptor.quarantined = !cleanup_ok;
        driver_registry.setState(registry_slot, if (cleanup_ok) .failed else .quarantined);
        return .load_failed;
    };
    storeRuntime(runtime_slot, name, descriptor.driver_type, registry_slot, owner, shutdown);
    k.puts("[R4D] runtime load ");
    k.puts(name);
    k.puts(" [OK]\r\n");
    return .loaded;
}

pub fn runtimeLoadSucceeded(result: RuntimeLoadResult) bool {
    return result == .loaded or result == .already_active;
}

pub fn runtimeLoadResultName(result: RuntimeLoadResult) []const u8 {
    return switch (result) {
        .loaded => "loaded",
        .already_active => "already-active",
        .name_invalid => "name-invalid",
        .file_missing => "file-missing",
        .invalid_file => "invalid-file",
        .init_failed => "init-failed",
        .load_failed => "load-failed",
    };
}

pub fn shutdownNetworkForSystemTransition() bool {
    if (!runtime_lifecycle_guard.enter(ownerHandoffTimeoutTicks())) {
        k.puts("[R4D] system-transition lifecycle timeout\r\n");
        return false;
    }
    defer leaveRuntimeLifecycle();
    if (runtime_transition_active) return false;
    runtime_transition_active = true;
    var success = true;
    var stopped: usize = 0;
    for (module_drivers) |driver| {
        if (!driver.used or !driver.quarantined or driver.driver_type != @intFromEnum(DriverType.net)) continue;
        k.puts("[R4D] system-transition quarantined network module blocks warm reset name=");
        k.puts(driver.name[0..driver.name_len]);
        k.puts("\r\n");
        success = false;
    }
    var index = runtime_drivers.len;
    while (index != 0) {
        index -= 1;
        const driver = &runtime_drivers[index];
        if (!driver.used or driver.driver_type != @intFromEnum(DriverType.net)) continue;
        if (driver.quarantined) {
            success = false;
            continue;
        }
        const expected_generation = driver.generation;
        const expected_owner = driver.owner;
        const expected_registry_slot = driver.registry_slot;
        if (!driver_api.enterOwnerBounded(expected_owner, ownerHandoffTimeoutTicks())) {
            // No runtime pointer may be dereferenced or mutated without the
            // owner guard: the slot could have been cleared/reused meanwhile.
            k.puts("[R4D] system-transition owner timeout owner=");
            k.putDec(expected_owner);
            k.puts(" registry=");
            k.putDec(expected_registry_slot);
            k.puts("\r\n");
            success = false;
            continue;
        }
        if (!runtimeIdentityMatches(driver, expected_generation, expected_owner, expected_registry_slot)) {
            success = false;
            _ = driver_api.leaveOwner();
            continue;
        }
        if (driver.quarantined) {
            success = false;
            _ = driver_api.leaveOwner();
            continue;
        }
        driver_registry.setState(expected_registry_slot, .shutdown);
        k.puts("[R4D] system-transition shutdown name=");
        k.puts(driver.name[0..driver.name_len]);
        k.puts(" owner=");
        k.putDec(driver.owner);
        k.puts("\r\n");
        const result = if (driver.shutdown) |shutdown| shutdown() else -1;
        if (result != 0) {
            quarantineRuntimeDriver(driver);
            k.puts("[R4D] system-transition shutdown failed name=");
            k.puts(driver.name[0..driver.name_len]);
            k.puts(" code=");
            putSignedDec(result);
            k.puts("\r\n");
            success = false;
            _ = driver_api.leaveOwner();
            continue;
        }
        k.puts("[R4D] system-transition shutdown OK name=");
        k.puts(driver.name[0..driver.name_len]);
        k.puts("\r\n");
        stopped += 1;
        _ = driver_api.leaveOwner();
    }
    k.puts("[R4D] system-transition network shutdown stopped=");
    k.putDec(stopped);
    k.puts(if (success) " result=OK\r\n" else " result=FAILED\r\n");
    return success;
}

fn shutdownAndCleanupFailedLoad(
    shutdown: ShutdownFn,
    owner: u32,
    name: []const u8,
    descriptor: *ModuleDriver,
    registry_slot: usize,
) bool {
    // Publish the conservative state before any owner/mutation lock can be
    // released. A successful cleanup clears it at the caller; every failure
    // remains visible to a concurrent system transition.
    descriptor.quarantined = true;
    driver_registry.setState(registry_slot, .quarantined);
    if (!driver_api.enterOwnerBounded(owner, ownerHandoffTimeoutTicks())) {
        logFailedLoadQuarantine(name, owner, "owner-timeout");
        return false;
    }
    var cleanup_token = driver_api.prepareOwnerCleanup(owner) orelse {
        _ = driver_api.leaveOwner();
        logFailedLoadQuarantine(name, owner, "resource-busy");
        return false;
    };
    driver_api.beginOwnerShutdown(&cleanup_token);
    const shutdown_result = shutdown();
    if (shutdown_result != 0) {
        _ = driver_api.quarantineOwnerCleanup(&cleanup_token);
        _ = driver_api.leaveOwner();
        k.puts("[R4D] failed-load shutdown failed name=");
        k.puts(name);
        k.puts(" owner=");
        k.putDec(owner);
        k.puts(" code=");
        putSignedDec(shutdown_result);
        k.puts(" resources=quarantined\r\n");
        return false;
    }
    const cleanup_ok = driver_api.commitOwnerCleanup(&cleanup_token);
    if (!cleanup_ok) {
        _ = driver_api.leaveOwner();
        logFailedLoadQuarantine(name, owner, "cleanup-failed");
        return false;
    }
    _ = driver_api.leaveOwner();
    return true;
}

fn quarantineRuntimeDriver(driver: *RuntimeDriver) void {
    driver.quarantined = true;
    driver_registry.setState(driver.registry_slot, .quarantined);
    const name = driver.name[0..driver.name_len];
    if (findModuleDriver(name)) |module_slot| module_drivers[module_slot].quarantined = true;
    k.puts("[R4D] runtime driver quarantined until reboot name=");
    k.puts(name);
    k.puts(" owner=");
    k.putDec(driver.owner);
    k.puts("\r\n");
}

fn logFailedLoadQuarantine(name: []const u8, owner: u32, reason: []const u8) void {
    k.puts("[R4D] failed-load resources quarantined name=");
    k.puts(name);
    k.puts(" owner=");
    k.putDec(owner);
    k.puts(" reason=");
    k.puts(reason);
    k.puts("\r\n");
}

fn ownerHandoffTimeoutTicks() u64 {
    const hz: u64 = timer.frequency();
    return @max((hz * OWNER_HANDOFF_MILLISECONDS + 999) / 1000, 1);
}

fn enterRuntimeLifecycle(timeout_ticks: u64) bool {
    if (!runtime_lifecycle_guard.enter(timeout_ticks)) return false;
    if (runtime_transition_active) {
        leaveRuntimeLifecycle();
        return false;
    }
    return true;
}

fn leaveRuntimeLifecycle() void {
    _ = runtime_lifecycle_guard.leave();
}

fn storeRuntime(slot: usize, name: []const u8, driver_type: u16, registry_slot: usize, owner: u32, shutdown: ShutdownFn) void {
    const d = &runtime_drivers[slot];
    d.* = .{
        .used = true,
        .generation = allocateRuntimeGeneration(),
        .driver_type = driver_type,
        .registry_slot = registry_slot,
        .owner = owner,
        .shutdown = shutdown,
    };
    d.name_len = if (name.len < MAX_RUNTIME_NAME) name.len else MAX_RUNTIME_NAME - 1;
    if (d.name_len > 0) @memcpy(d.name[0..d.name_len], name[0..d.name_len]);
}

fn allocateRuntimeGeneration() u64 {
    const generation = next_runtime_generation;
    next_runtime_generation +%= 1;
    if (next_runtime_generation == 0) next_runtime_generation = 1;
    return generation;
}

fn runtimeIdentityMatches(driver: *const RuntimeDriver, generation: u64, owner: u32, registry_slot: usize) bool {
    return driver.used and
        driver.generation == generation and
        driver.owner == owner and
        driver.registry_slot == registry_slot;
}

fn storeModuleDriver(slot: usize, name: []const u8, driver_type_value: u16, module_slot: usize, init: InitFn, shutdown: ShutdownFn) void {
    const d = &module_drivers[slot];
    d.* = .{
        .used = true,
        .driver_type = driver_type_value,
        .module_slot = module_slot,
        .init = init,
        .shutdown = shutdown,
    };
    d.name_len = if (name.len < MAX_RUNTIME_NAME) name.len else MAX_RUNTIME_NAME - 1;
    if (d.name_len > 0) @memcpy(d.name[0..d.name_len], name[0..d.name_len]);
}

fn ownerForRegistrySlot(registry_slot: usize) u32 {
    return @intCast(registry_slot + 1);
}

fn copyRuntimeName(src: []const u8, out: *[MAX_RUNTIME_NAME]u8) []const u8 {
    const len = if (src.len < MAX_RUNTIME_NAME) src.len else MAX_RUNTIME_NAME - 1;
    if (len > 0) @memcpy(out[0..len], src[0..len]);
    if (len < out.len) out[len] = 0;
    return out[0..len];
}

fn copyBytes(src: []const u8, dst: []u8) usize {
    const len = @min(src.len, dst.len);
    if (len != 0) @memcpy(dst[0..len], src[0..len]);
    return len;
}

fn freeRuntimeSlot() ?usize {
    var i: usize = 0;
    while (i < runtime_drivers.len) : (i += 1) {
        if (!runtime_drivers[i].used) return i;
    }
    return null;
}

fn freeModuleDriverSlot() ?usize {
    var i: usize = 0;
    while (i < module_drivers.len) : (i += 1) {
        if (!module_drivers[i].used) return i;
    }
    return null;
}

fn findRuntime(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < runtime_drivers.len) : (i += 1) {
        const d = &runtime_drivers[i];
        if (!d.used) continue;
        if (nameEq(d.name[0..d.name_len], name)) return i;
    }
    return null;
}

fn findModuleDriver(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < module_drivers.len) : (i += 1) {
        const d = &module_drivers[i];
        if (!d.used) continue;
        if (nameEq(d.name[0..d.name_len], name)) return i;
    }
    return null;
}

fn alreadyActive(raw_name: []const u8) bool {
    const name = trimDriverExtension(raw_name);
    const slot = driver_registry.findByName(name) orelse return false;
    const entry = driver_registry.get(slot) orelse return false;
    if (entry.state != .loaded and entry.state != .initialized and entry.state != .active) return false;

    k.puts("[R4D] already active: ");
    k.puts(entry.name[0..entry.name_len]);
    k.puts(" (");
    k.puts(driver_registry.sourceName(entry.source));
    k.puts("/");
    k.puts(driver_registry.stateName(entry.state));
    k.puts(")\r\n");
    return true;
}

fn parseR4MDriverInfo(bytes: []const u8, fallback_name: []const u8) ?R4MDriverInfo {
    if (!isR4M(bytes) or bytes.len < R4M_HEADER_SIZE) return null;
    if (readLe16(bytes[8..10]) != @intFromEnum(modules.Kind.r4d)) {
        k.puts("[R4D] R4M0 kind is not r4d\r\n");
        return null;
    }
    const meta_off = readLe32(bytes[56..60]);
    const meta_size = readLe32(bytes[60..64]);
    if (!validRange(meta_off, meta_size, bytes.len)) {
        k.puts("[R4D] invalid R4M0 metadata\r\n");
        return null;
    }
    const meta = bytes[@intCast(meta_off)..][0..@intCast(meta_size)];
    return parseR4MDriverInfoMeta(meta, fallback_name);
}

fn readR4MDriverInfoFromFile(volume: vfs.Volume, entry: vfs.Entry, fallback_name: []const u8) ?R4MDriverInfo {
    var meta_buf: [MAX_R4M_METADATA_PROBE]u8 = .{0} ** MAX_R4M_METADATA_PROBE;
    const header = module_r4m.readHeader(.{
        .source = .{
            .volume = volume,
            .entry = entry,
            .drive_letter = 'C',
        },
        .file_size = @intCast(entry.size),
        .expected_kind = .r4d,
        .name = "r4d-metadata-probe",
    }) orelse return null;
    const meta = module_r4m.readMetadata(.{
        .source = .{
            .volume = volume,
            .entry = entry,
            .drive_letter = 'C',
        },
        .header = header,
        .out = meta_buf[0..],
        .name = "r4d-metadata-probe",
    }) orelse return null;
    return parseR4MDriverInfoMeta(meta, fallback_name);
}

fn parseR4MDriverInfoMeta(meta: []const u8, fallback_name: []const u8) ?R4MDriverInfo {
    const name = module_r4m.metadataValue(meta, "r4d.name=") orelse trimDriverExtension(fallback_name);
    const type_name = module_r4m.metadataValue(meta, "r4d.type=") orelse "misc";
    const driver_type_value = parseDriverTypeName(type_name) orelse {
        k.puts("[R4D] invalid R4M0 driver type\r\n");
        return null;
    };
    var info = R4MDriverInfo{ .driver_type = driver_type_value };
    info.name_len = copyBytes(name, info.name[0..]);
    return info;
}

fn driverInfoName(info: *const R4MDriverInfo) []const u8 {
    return info.name[0..info.name_len];
}

fn parseDriverTypeName(value: []const u8) ?u16 {
    if (nameEq(value, "audio")) return 1;
    if (nameEq(value, "storage")) return 2;
    if (nameEq(value, "input")) return 3;
    if (nameEq(value, "synth")) return 4;
    if (nameEq(value, "net")) return 5;
    if (nameEq(value, "misc")) return 255;
    return null;
}

fn buildDriverPath(raw_name: []const u8, out: *[96]u8) ?[]const u8 {
    const prefix = "/R4OS/DRIVERS/";
    const suffix = ".r4d";
    const name = trimDriverExtension(raw_name);
    if (name.len == 0 or prefix.len + name.len + suffix.len > out.len) return null;

    @memcpy(out[0..prefix.len], prefix);
    var len = prefix.len;
    var i: usize = 0;
    while (i < name.len) : (i += 1) {
        out[len] = lower(name[i]);
        len += 1;
    }
    @memcpy(out[len .. len + suffix.len], suffix);
    return out[0 .. len + suffix.len];
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

fn trimDriverExtension(name: []const u8) []const u8 {
    if (name.len >= 4 and name[name.len - 4] == '.' and upper(name[name.len - 3]) == 'R' and upper(name[name.len - 2]) == '4' and upper(name[name.len - 1]) == 'D') {
        return name[0 .. name.len - 4];
    }
    return name;
}

fn fallbackNameFromPath(path: []const u8) []const u8 {
    var start: usize = 0;
    var i: usize = 0;
    while (i < path.len) : (i += 1) {
        if (path[i] == '/' or path[i] == '\\') start = i + 1;
    }
    return trimDriverExtension(path[start..]);
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

fn hasR4dExtension(name: []const u8) bool {
    return name.len >= 4 and name[name.len - 4] == '.' and upper(name[name.len - 3]) == 'R' and upper(name[name.len - 2]) == '4' and upper(name[name.len - 1]) == 'D';
}

fn isR4M(bytes: []const u8) bool {
    return bytes.len >= 4 and bytes[0] == 'R' and bytes[1] == '4' and bytes[2] == 'M' and bytes[3] == '0';
}

fn driverType(value: u16) ?DriverType {
    return switch (value) {
        1 => .audio,
        2 => .storage,
        3 => .input,
        4 => .synth,
        5 => .net,
        255 => .misc,
        else => null,
    };
}

fn driverTypeName(value: u16) []const u8 {
    return switch (driverType(value) orelse .unknown) {
        .audio => "audio",
        .storage => "storage",
        .input => "input",
        .synth => "synth",
        .net => "net",
        .misc => "misc",
        .unknown => "unknown",
    };
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn nameEq(a: []const u8, b: []const u8) bool {
    const bb = trimDriverExtension(b);
    if (a.len != bb.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(bb[i])) return false;
    }
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

fn lower(c: u8) u8 {
    if (c >= 'A' and c <= 'Z') return c + ('a' - 'A');
    return c;
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

fn putSignedDec(value: i32) void {
    const wide: i64 = value;
    if (wide < 0) {
        k.puts("-");
        k.putDec(@intCast(-wide));
    } else {
        k.putDec(@intCast(wide));
    }
}
