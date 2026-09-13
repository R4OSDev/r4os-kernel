test {
    const std = @import("std");
    const queue = @import("display/queue.zig");
    const abi = @import("r4os_kernel_contract");
    try std.testing.expectEqualDeep(abi.GfxBackendProfile{}, try queue.validatedProfile(.{}));
    var profile: abi.GfxBackendProfile = .{ .size = 104, .interface_id_hi = 0x100000017, .revision = 1, .data_bytes = 64, .data = @splat(0xa5) };
    const accepted = try queue.validatedProfile(profile);
    try std.testing.expect(accepted.size == 96 and accepted.interface_id_hi == 0x100000017 and accepted.data[63] == 0xa5);
    profile.data_bytes = 65; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.data_bytes = 63; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.data_bytes = 64; profile.revision = 0; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.revision = 1; profile.interface_id_hi = 0; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    _ = @import("display/display.zig");
    _ = @import("display/backend_state.zig");
    _ = @import("display/output_state.zig");
    _ = @import("display/mode_state.zig");
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
    _ = @import("kernel/gfx_driver_memory_owner.zig");
}
