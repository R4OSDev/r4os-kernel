const log = @import("log.zig");
const service_core = @import("service_core.zig");

var initialized = false;

pub fn init() void {
    if (initialized) return;

    service_core.init();
    initialized = true;

    log.puts("  Service registry ");
    log.puts("[OK]\r\n");
}

pub fn isInitialized() bool {
    return initialized;
}
