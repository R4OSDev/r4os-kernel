// Loader and config boot layer after the production storage path.
//
// This layer only reads module, driver, and boot config data reachable from C:.
// Driver policy, R4P startup, and service startup remain separate later steps.

const boot_config = @import("boot_config.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const font_catalog = @import("font_catalog.zig");
const k = @import("log.zig");
const loader_perf = @import("loader_perf.zig");
const modules = @import("modules.zig");
const r4d = @import("../program/r4d.zig");
const storage_boot = @import("storage_boot.zig");

var initialized = false;
var cached_config: ?*const boot_config.Config = null;

pub fn initFilesystemLoader() bool {
    if (initialized) return true;

    if (!storage_boot.isControllersInitialized()) {
        return fail("Loader boot before storage controllers");
    }

    loader_perf.beginFilesystemLoader();
    modules.loadSystemLibraries();
    r4d.discoverAll();
    modules.dumpSummary();
    cached_config = boot_config.load();
    const fonts = font_catalog.reloadInstalled();
    k.puts("[FONT] catalog loaded=");
    k.putDec(fonts.registered);
    k.puts(" rejected=");
    k.putDec(fonts.rejected);
    k.puts("\r\n");
    loader_perf.finishFilesystemLoader();

    initialized = true;
    boot_status.statusLine("  Loader config [OK]\r\n");
    return true;
}

pub fn isInitialized() bool {
    return initialized;
}

pub fn config() ?*const boot_config.Config {
    if (!initialized) return null;
    return cached_config;
}

fn fail(message: []const u8) bool {
    return fatal.fail(.loader, message);
}
