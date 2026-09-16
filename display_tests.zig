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
    var properties: abi.GfxBackendProperties = .{ .size = 304, .interface_id_hi = 0x100000035, .revision = 1, .data_bytes = 256, .data = @splat(0x35) };
    const facts = try queue.validatedProperties(properties);
    try std.testing.expect(facts.size == 288 and facts.data[255] == 0x35);
    properties.data_bytes = 257; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.data_bytes = 255; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.data_bytes = 0; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.data_bytes = 256; properties.size = 287; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.size = 288; properties.version = 2; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.version = 1; properties.revision = 0; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
    properties.revision = 1; properties.interface_id_hi = 0; try std.testing.expectError(error.Invalid, queue.validatedProperties(properties));
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
    {
        const color_wire = @import("program/gfx_output_wire.zig");
        const Color = abi.GfxOutputColorState;
        var guarded: [@sizeOf(Color) + 16]u8 align(@alignOf(Color)) = undefined;
        for (8..@sizeOf(Color) + 9) |capacity| {
            @memset(&guarded, 0xa5);
            const ptr: *Color = @ptrCast(&guarded);
            ptr.version = 1; ptr.size = @intCast(capacity);
            const count = color_wire.capacity(ptr) orelse { try std.testing.expect(capacity < 128); continue; };
            try std.testing.expect(count <= capacity and count == (if (capacity < 192) @as(u32, 128) else 192));
            color_wire.write(ptr, count, .{ .formats = 3, .dsc_depths = 3, .max_frl_rate = 6 });
            try std.testing.expect(std.mem.allEqual(u8, guarded[count..], 0xa5));
            const decoded = color_wire.read(ptr).?;
            try std.testing.expect(decoded.size == 192 and decoded.formats == 3 and decoded.dsc_depths == (if (count == 192) @as(u32, 3) else 0));
            try std.testing.expect(decoded.max_frl_rate == (if (count == 192) @as(u32, 6) else 0));
        }
    }
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
    {
        const wire_stats = @import("program/gfx_buffer_stats_wire.zig");
        const Stats = abi.GfxBufferStats;
        var guarded: [@sizeOf(Stats) + 16]u8 align(@alignOf(Stats)) = undefined;
        for (8..@sizeOf(Stats) + 9) |capacity| {
            @memset(&guarded, 0xa5);
            const ptr: *Stats = @ptrCast(&guarded);
            ptr.version = 1; ptr.size = @intCast(capacity);
            const count = wire_stats.capacity(ptr) orelse { try std.testing.expect(capacity < 56); continue; };
            try std.testing.expect(count == (if (capacity < 136) @as(u32, 56) else 136));
            wire_stats.write(ptr, count, .{ .committed_bytes = 0x100000007, .device_bytes = 0x100000007 });
            try std.testing.expect(ptr.size == count and ptr.committed_bytes == 0x100000007);
            if (count == 136) try std.testing.expect(ptr.device_bytes == 0x100000007);
            try std.testing.expect(std.mem.allEqual(u8, guarded[count..], 0xa5));
        }
    }
    _ = @import("memory/gfx_allocation_state.zig");
    _ = @import("memory/gfx_telemetry_state.zig");
    _ = @import("program/gfx_buffer_api.zig");
    _ = @import("kernel/mmio_windows.zig");
    _ = @import("kernel/gfx_driver_memory_owner.zig");
}
