// Early timer foundation for kernel startup.
//
// This layer starts the initial PIT timer after cpu_boot has prepared GDT, IDT,
// and PIC. Later timer switches to HPET/LAPIC stay in the existing timer core.

const timer = @import("timer.zig");
const monotonic = @import("../platform/monotonic.zig");

var initialized = false;

pub fn init() void {
    if (initialized) return;

    timer.initPit(timer.DEFAULT_HZ);
    monotonic.attachPeriodicClock();

    initialized = true;
}

pub fn isInitialized() bool {
    return initialized;
}
