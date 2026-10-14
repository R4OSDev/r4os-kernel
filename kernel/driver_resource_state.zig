// Serialized by DriverApi's wait-spanning owner guard. No resource handle
// owns a buffer, a filesystem lease or a path which could be reopened.
const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
const owner_capacity = @import("../driver/registry.zig").MAX_DRIVERS;
pub const Error = error{ Owner, Busy, Stale, Capacity };
pub const Binding = extern struct {
    epoch: u64 = 0,
    slot: usize = 0,
    module_generation: u32 = 0,
    closing: bool = false,
};
extern fn r4os_rust_resource_state_mutate(state: *State, operation: u8, owner: u32, slot: usize, generation: u32) u8;
extern fn r4os_rust_resource_current(state: *const State, owner: u32, output: *?*const Binding) u8;
extern fn r4os_rust_resource_handle(state: *const State, operation: u8, owner: u32, value: u64, output: *u64) u8;
extern fn r4os_rust_resource_publish(output: *@import("r4os_kernel_contract").DriverResourceApi, input: *const @import("r4os_kernel_contract").DriverResourceApi) bool;
comptime {
    if (@sizeOf(Binding) != 24 or @offsetOf(Binding, "module_generation") != 16 or
        @sizeOf(State) != 776 or @offsetOf(State, "bindings") != 8)
        @compileError("Native resource state C layout drift");
}
fn checked(code: u8) Error!void {
    return switch (code) {
        0 => {},
        1 => error.Owner,
        2 => error.Busy,
        3 => error.Stale,
        4 => error.Capacity,
        else => @panic("Invalid native resource status"),
    };
}
pub const State = extern struct {
    next_epoch: u64 = 1,
    bindings: [owner_capacity]Binding = .{Binding{}} ** owner_capacity,
    pub fn bind(self: *State, owner: u32, slot: usize, generation: u32) Error!void {
        try checked(r4os_rust_resource_state_mutate(self, 0, owner, slot, generation));
    }
    pub fn current(self: *const State, owner: u32) Error!*const Binding {
        var value: ?*const Binding = null;
        try checked(r4os_rust_resource_current(self, owner, &value));
        return value.?;
    }
    pub fn handle(self: *const State, owner: u32, index: usize) Error!u64 {
        var value: u64 = undefined;
        try checked(r4os_rust_resource_handle(self, 0, owner, index, &value));
        return value;
    }
    pub fn resolve(self: *const State, owner: u32, id: u64) Error!usize {
        var value: u64 = undefined;
        try checked(r4os_rust_resource_handle(self, 1, owner, id, &value));
        return @intCast(value);
    }
    pub fn close(self: *State, owner: u32) void {
        _ = r4os_rust_resource_state_mutate(self, 1, owner, 0, 0);
    }
    pub fn finish(self: *State, owner: u32) void {
        _ = r4os_rust_resource_state_mutate(self, 2, owner, 0, 0);
    }
};
pub fn publishApi(output: *@import("r4os_kernel_contract").DriverResourceApi, input: @import("r4os_kernel_contract").DriverResourceApi) bool {
    return r4os_rust_resource_publish(output, &input);
}

test "resource handles reject foreign owners, restart, shutdown and exhaustion" {
    const Api = @import("r4os_kernel_contract").DriverResourceApi;
    const t = std.testing;
    for ([_]u32{ 31, 32, 39, 40, 47, 48, 55, 56, 64 }) |capacity| {
        var storage: [80]u8 align(8) = @splat(0x79);
        std.mem.writeInt(u32, storage[0..4], 1, .little);
        std.mem.writeInt(u32, storage[4..8], capacity, .little);
        const before = storage;
        const output: *Api = @ptrCast(&storage);
        if (capacity < 32) {
            try t.expect(!publishApi(output, .{}));
            try t.expectEqualSlices(u8, &before, &storage);
        } else {
            try t.expect(publishApi(output, .{ .stat = 11, .read_at = 13, .now_ns = 17, .acpi_stat = 19, .acpi_read_at = 23 }));
            const written = @min(capacity & ~@as(u32, 7), @sizeOf(Api));
            try t.expectEqual(written, output.size);
            try t.expectEqual(@as(u64, 11), output.stat);
            try t.expectEqualSlices(u8, before[written..], storage[written..]);
        }
    }
    var state: State = .{};
    try state.bind(owner_capacity, 9, 102);
    const last = try state.handle(owner_capacity, 0);
    try t.expectEqual(@as(usize, 0), try state.resolve(owner_capacity, last));
    try t.expectError(error.Owner, state.bind(owner_capacity + 1, 10, 103));
    try t.expectError(error.Owner, state.resolve(owner_capacity + 1, last));
    state.close(owner_capacity);
    try t.expectError(error.Stale, state.resolve(owner_capacity, last));
    state.finish(owner_capacity);
    try state.bind(1, 7, 100);
    try state.bind(2, 8, 101);
    const first = try state.handle(1, 63);
    try std.testing.expectEqual(@as(usize, 63), try state.resolve(1, first));
    try std.testing.expectError(error.Stale, state.resolve(2, first));
    try std.testing.expectError(error.Owner, state.resolve(0, first));
    try std.testing.expectError(error.Stale, state.resolve(1, first + 1));
    try std.testing.expectError(error.Busy, state.bind(1, 7, 100));
    state.close(1);
    try std.testing.expectError(error.Stale, state.resolve(1, first));
    try std.testing.expectError(error.Busy, state.bind(1, 7, 100));
    state.finish(1);
    try state.bind(1, 7, 100);
    try std.testing.expectError(error.Stale, state.resolve(1, first));
    state.finish(1);
    state.next_epoch = std.math.maxInt(u56);
    try state.bind(1, 7, 101);
    try std.testing.expectEqual(@as(usize, 0), try state.resolve(1, try state.handle(1, 0)));
    state.finish(1);
    try std.testing.expectError(error.Capacity, state.bind(1, 7, 102));
}
