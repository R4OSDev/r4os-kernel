// Early driver registry foundation for kernel startup.
//
// This layer only initializes the central driver status list. The actual
// driver logic stays in the individual driver modules.

const driver_registry = @import("../driver/registry.zig");

var initialized = false;

pub fn init() void {
    if (initialized) return;

    driver_registry.init();

    initialized = true;
}

pub fn isInitialized() bool {
    return initialized;
}
