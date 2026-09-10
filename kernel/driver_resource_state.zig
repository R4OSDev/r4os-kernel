// Serialized by DriverApi's wait-spanning owner guard. No resource handle
// owns a buffer, a filesystem lease or a path which could be reopened.
const std = @import("std");
pub const Error = error{ Owner, Busy, Stale, Capacity };
pub const Binding = struct {
    epoch: u64 = 0,
    slot: usize = 0,
    module_generation: u32 = 0,
    closing: bool = false,
};
pub const State = struct {
    next_epoch: u64 = 1,
    bindings: [16]Binding = .{Binding{}} ** 16,

    pub fn bind(self: *State, owner: u32, slot: usize, generation: u32) Error!void {
        if (owner == 0 or owner > self.bindings.len or generation == 0) return error.Owner;
        if (self.bindings[owner - 1].epoch != 0) return error.Busy;
        // Exhaustion fails closed; old handles can never become current.
        if (self.next_epoch > std.math.maxInt(u56)) return error.Capacity;
        self.bindings[owner - 1] = .{ .epoch = self.next_epoch, .slot = slot, .module_generation = generation };
        self.next_epoch += 1;
    }
    pub fn current(self: *const State, owner: u32) Error!*const Binding {
        if (owner == 0 or owner > self.bindings.len) return error.Owner;
        const binding = &self.bindings[owner - 1];
        if (binding.epoch == 0 or binding.closing) return error.Stale;
        return binding;
    }
    pub fn handle(self: *const State, owner: u32, index: usize) Error!u64 {
        if (index >= 64) return error.Capacity;
        return ((try self.current(owner)).epoch << 8) | (index + 1);
    }
    pub fn resolve(self: *const State, owner: u32, id: u64) Error!usize {
        const binding = try self.current(owner);
        const index = id & 255;
        if (id >> 8 != binding.epoch or index == 0 or index > 64) return error.Stale;
        return @intCast(index - 1);
    }
    pub fn close(self: *State, owner: u32) void {
        if (owner != 0 and owner <= self.bindings.len) self.bindings[owner - 1].closing = true;
    }
    pub fn finish(self: *State, owner: u32) void {
        if (owner != 0 and owner <= self.bindings.len) self.bindings[owner - 1] = .{};
    }
};

test "resource handles reject foreign owners, restart, shutdown and exhaustion" {
    var state: State = .{};
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
