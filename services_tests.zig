const services = @import("kernel/services.zig");

// The native service owner reaches lower sync/filesystem value boundaries.
// Link their actual stateless adapters; these serialized metadata cases never
// initialize the scheduler or execute their hardware operations.
comptime {
    _ = @import("storage/block.zig");
    _ = @import("fs/page_cache.zig");
    _ = @import("fs/fat/fat32.zig");
    _ = @import("platform/clocksource.zig");
    _ = @import("arch/x86_64/ioapic.zig");
    _ = @import("arch/x86_64/hpet.zig");
    _ = @import("arch/x86_64/lapic.zig");
    _ = @import("kernel/timer.zig");
}

test "service module reset tests are reachable" {
    _ = services.API_VERSION;
}
