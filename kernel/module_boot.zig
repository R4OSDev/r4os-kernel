const log = @import("log.zig");
const modules = @import("modules.zig");

var initialized = false;

pub fn init() void {
    if (initialized) return;

    modules.init();
    initialized = true;

    log.puts("  Module registry ");
    log.puts("[OK]\r\n");
}

pub fn isInitialized() bool {
    return initialized;
}
