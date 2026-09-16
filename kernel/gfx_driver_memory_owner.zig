// Driver memory metadata. The runtime bridge uses the shared BO owner for
// every access; page walks, MMIO and backing allocation run outside it.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
pub const Owner = lifetime.Owner;
pub const Error = lifetime.Error;

pub fn State(comptime owner_capacity: usize) type {
    return struct {
        const Self = @This();
        const Tree = std.Treap(u32, std.math.order);
        const Epoch = struct {
            identity: Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
            closing: bool = false,
            mmio_busy: bool = false,
            mmio_retained: bool = false,
            mmio_pending: bool = false,
            devices: usize = 0,
        };
        pub const Device = struct {
            index: Tree.Node = undefined,
            self_address: usize = 0,
            state_address: usize = 0,
            owner: Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
            descriptor: abi.GfxDeviceLease = .{},
            busy: bool = false,
        };
        epochs: [owner_capacity]Epoch = .{Epoch{}} ** owner_capacity,
        devices: Tree = .{},

        // Bind before DriverInit/Work can run. Lazy creation after an owner
        // snapshot could otherwise miss a concurrent close of its first BO.
        pub fn bind(self: *Self, id: u32, generation: u64) bool {
            if (id == 0 or generation == 0 or self.find(id) != null) return false;
            for (&self.epochs) |*epoch| if (epoch.identity.id == 0) {
                epoch.* = .{ .identity = .{ .kind = .driver, .id = id, .generation = generation } };
                return true;
            };
            return false;
        }
        pub fn owner(self: *Self, id: u32, admission: bool) Error!Owner {
            const epoch = self.find(id) orelse return error.Stale;
            if (admission and epoch.closing) return error.Closed;
            return epoch.identity;
        }
        pub fn close(self: *Self, id: u32) void {
            if (self.find(id)) |epoch| epoch.closing = true;
        }
        pub fn retire(self: *Self, identity: Owner) bool {
            const epoch = self.matchEpoch(identity) catch return false;
            if (self.retains(identity)) return false;
            epoch.* = .{};
            return true;
        }
        pub fn retains(self: *Self, identity: Owner) bool {
            const epoch = self.matchEpoch(identity) catch return true;
            return epoch.mmio_busy or epoch.mmio_retained or epoch.mmio_pending or epoch.devices != 0;
        }
        // Caller allocates stable storage before acquiring the BO owner and
        // keeps it until detach. No allocator or page walk runs in this state.
        pub fn reserve(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease, record: *Device) Error!void {
            const epoch = try self.matchEpoch(identity);
            if (epoch.closing) return error.Closed;
            if (descriptor.lease.id == 0 or descriptor.lease.generation == 0 or record.self_address != 0 or record.state_address != 0) return error.Stale;
            var place = self.devices.getEntryFor(descriptor.lease.id);
            if (place.node != null) return error.Stale;
            if (epoch.devices == std.math.maxInt(usize)) return error.Exhausted;
            record.* = .{ .self_address = @intFromPtr(record), .state_address = @intFromPtr(self), .owner = identity, .descriptor = descriptor, .busy = true };
            place.set(&record.index);
            epoch.devices += 1;
        }
        pub fn matching(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease) Error!*Device {
            _ = try self.matchEpoch(identity);
            const node = self.devices.getEntryFor(descriptor.lease.id).node orelse return error.Stale;
            const record: *Device = @fieldParentPtr("index", node);
            if (record.self_address != @intFromPtr(record) or record.state_address != @intFromPtr(self) or
                !record.owner.eql(identity) or !std.meta.eql(record.descriptor, descriptor)) return error.Stale;
            if (record.busy) return error.Busy;
            return record;
        }
        // Also permits the exact busy record after a failed DMA page walk.
        // End its common BO use before detaching; free storage after unlock.
        pub fn detach(self: *Self, record: *Device) Error!void {
            if (record.self_address != @intFromPtr(record) or record.state_address != @intFromPtr(self)) return error.Stale;
            const epoch = try self.matchEpoch(record.owner);
            var place = self.devices.getEntryFor(record.descriptor.lease.id);
            if (place.node != &record.index or epoch.devices == 0) return error.Stale;
            place.set(null);
            epoch.devices -= 1;
            record.* = .{};
        }
        pub fn beginMmio(self: *Self, identity: Owner) void {
            const epoch = self.matchEpoch(identity) catch unreachable;
            std.debug.assert(!epoch.mmio_busy);
            epoch.mmio_busy = true;
        }
        pub fn endMmio(self: *Self, identity: Owner, retained: bool, pending: bool) void {
            const epoch = self.matchEpoch(identity) catch unreachable;
            std.debug.assert(epoch.mmio_busy);
            epoch.mmio_busy = false;
            epoch.mmio_retained = retained;
            epoch.mmio_pending = pending;
        }
        pub fn pendingMmio(self: *Self, identity: Owner) bool {
            const epoch = self.matchEpoch(identity) catch return true;
            return epoch.mmio_busy or epoch.mmio_pending;
        }
        fn find(self: *Self, id: u64) ?*Epoch {
            if (id == 0) return null;
            for (&self.epochs) |*epoch| if (epoch.identity.id == id) return epoch;
            return null;
        }
        fn matchEpoch(self: *Self, identity: Owner) Error!*Epoch {
            const epoch = self.find(identity.id) orelse return error.Stale;
            if (!epoch.identity.eql(identity)) return error.Stale;
            return epoch;
        }
    };
}

test "driver BO reservations survive interleaved page walks and close before epoch reuse" {
    const t = std.testing;
    var state: State(2) = .{};
    try t.expect(state.bind(7, 41));
    const old = try state.owner(7, true);
    var first: State(2).Device = .{};
    var second: State(2).Device = .{};
    try state.reserve(old, .{ .lease = .{ .id = 1, .generation = 3 } }, &first);
    try state.reserve(old, .{ .lease = .{ .id = 2, .generation = 4 } }, &second);
    try t.expectError(error.Busy, state.matching(old, first.descriptor));
    // The first DMA page walk fails while the second is still preparing.
    try state.detach(&first);
    state.close(7);
    try t.expectError(error.Closed, state.owner(7, true));
    try t.expectError(error.Closed, state.reserve(old, .{}, &first));
    try t.expect(!state.retire(old));
    second.busy = false;
    try t.expect((try state.matching(try state.owner(7, false), second.descriptor)) == &second);
    const stale = second.descriptor;
    try state.detach(&second);
    try t.expect(state.retire(old));
    try t.expect(state.bind(7, 42));
    const fresh = try state.owner(7, true);
    try t.expect(!fresh.eql(old));
    try t.expectError(error.Stale, state.matching(old, stale));
    try t.expectError(error.Stale, state.matching(fresh, stale));
    // A driver which has never queried the BO API still observes close.
    try t.expect(state.bind(8, 43));
    state.close(8);
    try t.expectError(error.Closed, state.owner(8, true));
    try checkDynamicDevices();
}

fn checkDynamicDevices() !void {
    const t = std.testing;
    var state: State(2) = .{};
    try t.expect(state.bind(7, 61) and state.bind(8, 62));
    const owners = [_]Owner{ try state.owner(7, true), try state.owner(8, true) };
    // Probe the removed 1024-entry limit with interleaved, stable page-walk
    // records belonging to two independent driver epochs.
    const records = try t.allocator.alloc(State(2).Device, 2050);
    defer t.allocator.free(records);
    @memset(records, .{});
    for (records, 0..) |*record, i| {
        try state.reserve(owners[i % 2], .{ .lease = .{ .id = @intCast(i + 1), .generation = 100 + i }, .byte_length = 4096 }, record);
        try t.expectError(error.Busy, state.matching(owners[i % 2], record.descriptor));
    }
    var duplicate: State(2).Device = .{};
    try t.expectError(error.Stale, state.reserve(owners[0], records[1024].descriptor, &duplicate));
    var moved = records[1024];
    try t.expectError(error.Stale, state.detach(&moved));
    var copied = state;
    try t.expectError(error.Stale, copied.matching(owners[0], records[1024].descriptor));
    state.close(7);
    try t.expectError(error.Closed, state.reserve(owners[0], .{ .lease = .{ .id = 9001, .generation = 9 } }, &duplicate));
    try t.expect(!state.retire(owners[0]) and !state.retire(owners[1]));
    for (records, 0..) |*record, i| {
        const descriptor = record.descriptor;
        record.busy = false;
        try t.expect((try state.matching(owners[i % 2], descriptor)) == record);
        var wrong = descriptor; wrong.lease.generation += 1;
        try t.expectError(error.Stale, state.matching(owners[i % 2], wrong));
        try t.expectError(error.Stale, state.matching(owners[(i + 1) % 2], descriptor));
        try state.detach(record);
        try t.expectError(error.Stale, state.matching(owners[i % 2], descriptor));
        try t.expectError(error.Stale, state.detach(record));
    }
    try t.expect(state.devices.root == null and state.retire(owners[0]) and state.retire(owners[1]));
}

test "worker collection distinguishes live MMIO from incomplete or busy retirement" {
    const t = std.testing;
    var state: State(1) = .{};
    try t.expect(state.bind(7, 41));
    const identity = try state.owner(7, true);
    state.beginMmio(identity);
    try t.expect(state.pendingMmio(identity));
    try t.expect(!state.retire(identity));
    state.endMmio(identity, true, false);
    try t.expect(!state.pendingMmio(identity));
    try t.expect(state.retains(identity));
    state.beginMmio(identity);
    state.endMmio(identity, true, true);
    try t.expect(state.pendingMmio(identity));
    state.close(7);
    state.beginMmio(identity);
    state.endMmio(identity, false, false);
    try t.expect(!state.pendingMmio(identity));
    try t.expect(state.retire(identity));
}
