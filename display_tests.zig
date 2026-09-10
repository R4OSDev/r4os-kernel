test {
    _ = @import("display/display.zig");
    _ = @import("display/backend_state.zig");
    _ = @import("display/queue_state.zig");
    _ = @import("display/queue_resources.zig");
    _ = @import("display/queue_ingress.zig");
    _ = @import("display/firmware_access.zig");
    _ = @import("display/framebuffer.zig");
    _ = @import("display/console_scroll_buffer.zig");
    _ = @import("kernel/graphics_boot_policy.zig");
    _ = @import("memory/gfx_buffer_layout.zig");
    _ = @import("memory/gfx_buffer_owner.zig");
    _ = @import("program/gfx_buffer_api.zig");
    _ = @import("kernel/mmio_windows.zig");
}
