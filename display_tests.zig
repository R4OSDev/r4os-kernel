test {
    const std = @import("std");
    const queue = @import("display/queue.zig");
    const abi = @import("r4os_kernel_contract");
    try @import("display/output_runtime_check.zig").run();
    try std.testing.expectEqualDeep(abi.GfxBackendProfile{}, try queue.validatedProfile(.{}));
    var profile: abi.GfxBackendProfile = .{ .size = 104, .interface_id_hi = 0x100000017, .revision = 1, .data_bytes = 64, .data = @splat(0xa5) };
    const accepted = try queue.validatedProfile(profile);
    try std.testing.expect(accepted.size == 96 and accepted.interface_id_hi == 0x100000017 and accepted.data[63] == 0xa5);
    profile.data_bytes = 65; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.data_bytes = 63; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.data_bytes = 64; profile.revision = 0; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    profile.revision = 1; profile.interface_id_hi = 0; try std.testing.expectError(error.Invalid, queue.validatedProfile(profile));
    const wire = @import("program/gfx_queue_wire.zig");
    inline for (.{ abi.GfxBackendRegistration, abi.GfxBackendInfo, abi.GfxSubmission, abi.GfxDriverJob }) |T| {
        // Caller capacity and physical guard bytes are independent. Exercise
        // every incomplete tail as well as both released prefix revisions.
        var guarded: [@sizeOf(T) + 16]u8 align(@alignOf(T)) = undefined;
        for (8..@sizeOf(T) + 9) |capacity| {
            @memset(&guarded, 0xa5);
            const ptr: *T = @ptrCast(&guarded);
            ptr.version = 1; ptr.size = @intCast(capacity);
            const size = wire.capacity(T, ptr) orelse continue;
            try std.testing.expect(size <= capacity and size <= @sizeOf(T));
            wire.write(T, ptr, size, .{});
            try std.testing.expect(std.mem.allEqual(u8, guarded[size..], 0xa5));
            const decoded = wire.read(T, ptr).?;
            var expected: T = .{}; expected.size = size;
            try std.testing.expectEqualDeep(expected, decoded);
        }
    }
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
    _ = @import("memory/gfx_allocation_state.zig");
    _ = @import("program/gfx_buffer_api.zig");
    _ = @import("kernel/mmio_windows.zig");
    _ = @import("kernel/gfx_driver_memory_owner.zig");
}
