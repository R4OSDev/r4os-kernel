test {
    _ = @import("kernel/modules.zig");
    _ = @import("kernel/module_resources.zig");
    _ = @import("kernel/driver_resource_state.zig");
    _ = @import("kernel/driver_memory_owner.zig");
    _ = @import("kernel/driver_thread_owner.zig");
    _ = @import("kernel/driver_semaphore_owner.zig");
    _ = @import("arch/x86_64/callback_abort.zig");
}
