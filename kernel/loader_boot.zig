// Loader and config boot layer after the production storage path.
//
// This layer only reads module, driver, and boot config data reachable from C:.
// Driver policy, R4P startup, and service startup remain separate later steps.

const boot_config = @import("boot_config.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const font_catalog = @import("font_catalog.zig");
const k = @import("log.zig");
const modules = @import("modules.zig");
const r4d = @import("../program/r4d.zig");
const storage_boot = @import("storage_boot.zig");

// Stateless peer projections: native Rust alone owns ordering/readiness/config.
const NativeFonts = extern struct { registered: u32, rejected: u32 };
const NativePeers = extern struct {
    controllers: *const fn () callconv(.c) bool,
    fail: *const fn ([*]const u8, usize) callconv(.c) bool,
    libraries: *const fn () callconv(.c) void,
    drivers: *const fn () callconv(.c) void,
    summary: *const fn () callconv(.c) void,
    config: *const fn () callconv(.c) ?*const anyopaque,
    fonts: *const fn () callconv(.c) NativeFonts,
    log: *const fn ([*]const u8, usize) callconv(.c) void,
    decimal: *const fn (u64) callconv(.c) void,
    status: *const fn ([*]const u8, usize) callconv(.c) void,
};
extern fn r4os_rust_loader_boot_init(*const NativePeers) bool;
extern fn r4os_rust_loader_boot_ready() bool;
extern fn r4os_rust_loader_boot_config() ?*const boot_config.Config;

pub fn initFilesystemLoader() bool {
    return r4os_rust_loader_boot_init(&peers);
}
pub fn isInitialized() bool {
    return r4os_rust_loader_boot_ready();
}
pub fn config() ?*const boot_config.Config {
    return r4os_rust_loader_boot_config();
}

fn controllers() callconv(.c) bool {
    return storage_boot.isControllersInitialized();
}
fn fail(data: [*]const u8, len: usize) callconv(.c) bool {
    return fatal.fail(.loader, data[0..len]);
}

fn libraries() callconv(.c) void {
    modules.loadSystemLibraries();
}
fn drivers() callconv(.c) void {
    r4d.discoverAll();
}
fn summary() callconv(.c) void {
    modules.dumpSummary();
}
fn loadConfig() callconv(.c) ?*const anyopaque {
    return boot_config.load();
}
fn fonts() callconv(.c) NativeFonts {
    const result = font_catalog.reloadInstalled();
    return .{ .registered = result.registered, .rejected = result.rejected };
}
fn puts(data: [*]const u8, len: usize) callconv(.c) void {
    k.puts(data[0..len]);
}
fn decimal(value: u64) callconv(.c) void {
    k.putDec(value);
}

fn status(data: [*]const u8, len: usize) callconv(.c) void {
    boot_status.statusLine(data[0..len]);
}
const peers = NativePeers{ .controllers = controllers, .fail = fail, .libraries = libraries, .drivers = drivers, .summary = summary, .config = loadConfig, .fonts = fonts, .log = puts, .decimal = decimal, .status = status };

comptime {
    if (@sizeOf(NativeFonts) != 8 or @offsetOf(NativeFonts, "registered") != 0 or @offsetOf(NativeFonts, "rejected") != 4) @compileError("loader boot font C layout drift");
    if (@sizeOf(NativePeers) != 80) @compileError("loader boot peer C layout drift");
    const names = [_][]const u8{ "controllers", "fail", "libraries", "drivers", "summary", "config", "fonts", "log", "decimal", "status" };
    for (names, 0..) |name, index| {
        if (@offsetOf(NativePeers, name) != index * 8) @compileError("loader boot peer C offset drift");
    }
}
