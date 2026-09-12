// Driver memory metadata. The runtime bridge uses the shared BO owner for
// every access; page walks, MMIO and backing allocation run outside it.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
pub const Owner = lifetime.Owner;
pub const Error = lifetime.Error;

pub fn State(comptime owner_capacity: usize, comptime device_capacity: usize) type {
    return struct {
        const Self = @This();
        const Epoch = struct {
            identity: Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
            closing: bool = false,
            mmio_busy: bool = false,
            mmio_retained: bool = false,
            mmio_pending: bool = false,
        };
        pub const Device = struct {
            owner: Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
            descriptor: abi.GfxDeviceLease = .{},
            busy: bool = false,
        };
        epochs: [owner_capacity]Epoch = .{Epoch{}} ** owner_capacity,
        devices: [device_capacity]Device = .{Device{}} ** device_capacity,

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
            if (epoch.mmio_busy or epoch.mmio_retained or epoch.mmio_pending) return true;
            for (&self.devices) |*record| {
                if (record.descriptor.lease.id != 0 and record.owner.eql(identity)) return true;
            }
            return false;
        }
        pub fn reserve(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease) Error!*Device {
            const epoch = try self.matchEpoch(identity);
            if (epoch.closing) return error.Closed;
            for (&self.devices) |*record| if (record.descriptor.lease.id == 0) {
                record.* = .{ .owner = identity, .descriptor = descriptor, .busy = true };
                return record;
            };
            return error.Capacity;
        }
        pub fn matching(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease) Error!*Device {
            _ = try self.matchEpoch(identity);
            for (&self.devices) |*record| {
                if (record.descriptor.lease.id != 0 and record.owner.eql(identity) and std.meta.eql(record.descriptor, descriptor)) {
                    if (record.busy) return error.Busy;
                    return record;
                }
            }
            return error.Stale;
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
    var state: State(2, 2) = .{};
    try t.expect(state.bind(7, 41));
    const old = try state.owner(7, true);
    const first = try state.reserve(old, .{ .lease = .{ .id = 1, .generation = 3 } });
    const second = try state.reserve(old, .{ .lease = .{ .id = 2, .generation = 4 } });
    try t.expect(first != second);
    try t.expectError(error.Busy, state.matching(old, first.descriptor));
    // The first DMA page walk fails while the second is still preparing.
    first.* = .{};
    state.close(7);
    try t.expectError(error.Closed, state.owner(7, true));
    try t.expectError(error.Closed, state.reserve(old, .{}));
    try t.expect(!state.retire(old));
    second.busy = false;
    try t.expect((try state.matching(try state.owner(7, false), second.descriptor)) == second);
    const stale = second.descriptor;
    second.* = .{};
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
}

test "worker collection distinguishes live MMIO from incomplete or busy retirement" {
    const t = std.testing;
    var state: State(1, 1) = .{};
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
