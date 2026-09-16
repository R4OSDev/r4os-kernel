// Bounded cross-owner allocation metadata. No GPU placement, callback or wait.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
pub const Owner = bo.Owner;
pub const Handle = bo.Handle;
pub const Error = bo.Error;
pub const result_owner = Owner{ .kind = .kernel, .id = 4, .generation = 1 };
pub const Phase = enum(u32) { queued, claimed, terminal };
pub const Entry = struct {
    handle: Handle = .{},
    owner: Owner = .{ .kind = .program, .id = 0, .generation = 0 },
    provider: Handle = .{},
    allocation: abi.GfxNativeAllocation = .{},
    phase: Phase = .queued,
    result: i32 = 0,
    completed_ns: u64 = 0,
    open: bool = false,
    claimed: bool = false,
    reference: Handle = .{},
    waiters: u32 = 0,
    notification_pending: bool = false,
    publishing: bool = false,

    pub fn status(self: *const Entry) abi.GfxNativeStatus {
        return .{ .request = .{ .id = self.handle.id, .generation = self.handle.generation }, .phase = @intFromEnum(self.phase), .result = self.result, .flags = @intFromBool(self.claimed), .deadline_ns = self.allocation.deadline_ns, .completed_ns = self.completed_ns };
    }
    pub fn terminal(self: *Entry, result: i32, instant: u64) void {
        if (self.phase == .terminal) return;
        self.phase = .terminal;
        self.result = result;
        self.completed_ns = instant;
        self.notification_pending = true;
    }
};
pub fn validate(input: abi.GfxNativeAllocation, instant: u64) Error!void {
    if (input.version != 1 or input.size < @sizeOf(abi.GfxNativeAllocation) or input.adapter_id == 0 or
        input.memory_generation == 0 or input.reserved0 != 0 or instant == 0 or
        input.deadline_ns <= instant or input.deadline_ns == std.math.maxInt(u64) or
        input.usage == 0 or input.usage & ~@as(u32, 60) != 0) return error.Invalid;
    if (input.kind > 1 or input.layout > 1) return error.Unsupported;
    if (input.kind == 0) {
        if (input.byte_length == 0 or input.width != 0 or input.height != 0 or input.format != 0 or input.layout != 0) return error.Invalid;
    } else {
        if (input.byte_length != 0 or input.width == 0 or input.height == 0 or input.format == 0) return error.Invalid;
        _ = std.enums.fromInt(bo.layout.Format, input.format) orelse return error.Unsupported;
    }
}
pub fn matches(input: abi.GfxNativeAllocation, descriptor: bo.layout.Descriptor, driver: Owner) bool {
    return descriptor.location == .device_local and descriptor.binding.adapter == input.adapter_id and
        descriptor.binding.device_generation == input.memory_generation and descriptor.binding.driver_owner == driver.id and
        descriptor.usage & input.usage == input.usage and descriptor.usage & 3 == 0 and
        (if (input.layout == 0) descriptor.modifier == 0 else descriptor.modifier != 0) and
        (if (input.kind == 0) descriptor.format == .bytes and descriptor.bytes >= input.byte_length else descriptor.width == input.width and descriptor.height == input.height and @intFromEnum(descriptor.format) == input.format);
}
pub fn Store(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = .{Entry{}} ** capacity,
        serial: u64 = 0,

        pub fn find(self: *Self, handle: Handle) Error!*Entry {
            if (handle.id == 0 or handle.id > capacity or handle.generation == 0) return error.Stale;
            const entry = &self.entries[handle.id - 1];
            if (!entry.handle.eql(handle)) return error.Stale;
            return entry;
        }
        pub fn owned(self: *Self, handle: Handle, owner: Owner) Error!*Entry {
            const entry = try self.find(handle);
            if (!entry.owner.eql(owner)) return error.WrongOwner;
            if (!entry.open) return error.Closed;
            return entry;
        }
        pub fn start(self: *Self, owner: Owner, provider: Handle, input: abi.GfxNativeAllocation, instant: u64) Error!*Entry {
            try validate(input, instant);
            if (!owner.valid() or provider.id == 0 or provider.generation == 0) return error.Invalid;
            if (self.serial == std.math.maxInt(u64)) return error.Exhausted;
            var count: usize = 0;
            for (&self.entries) |entry| if (entry.handle.id != 0 and entry.owner.eql(owner)) {
                count += 1;
            };
            if (count >= 16) return error.Budget;
            for (&self.entries, 0..) |*entry, i| if (entry.handle.id == 0) {
                self.serial += 1;
                entry.* = .{ .handle = .{ .id = @intCast(i + 1), .generation = self.serial }, .owner = owner, .provider = provider, .allocation = input, .open = true };
                entry.allocation.size = @sizeOf(abi.GfxNativeAllocation);
                return entry;
            };
            return error.Capacity;
        }
        pub fn take(self: *Self, provider: Handle, instant: u64) ?*Entry {
            self.expire(instant);
            var selected: ?*Entry = null;
            for (&self.entries) |*entry| if (entry.handle.id != 0 and entry.phase == .queued and entry.provider.eql(provider)) {
                if (selected == null or entry.handle.generation < selected.?.handle.generation) selected = entry;
            };
            if (selected) |entry| {
                entry.phase = .claimed;
                entry.claimed = true;
            }
            return selected;
        }
        pub fn claim(self: *Self, handle: Handle, provider: Handle) Error!*Entry {
            const entry = try self.find(handle);
            if (!entry.provider.eql(provider)) return error.WrongOwner;
            if (!entry.claimed) return error.Closed;
            return entry;
        }
        pub fn finish(self: *Self, handle: Handle, provider: Handle, result: i32, instant: u64) Error!void {
            if (result == 0 or result > 1) return error.Invalid;
            const entry = try self.claim(handle, provider);
            entry.claimed = false;
            entry.terminal(result, instant);
        }
        pub fn close(self: *Self, handle: Handle, owner: Owner, instant: u64) Error!Handle {
            const entry = try self.owned(handle, owner);
            entry.open = false;
            entry.terminal(abi.gfx_queue_error_wait_cancelled, instant);
            const reference = entry.reference;
            entry.reference = .{};
            return reference;
        }
        pub fn expire(self: *Self, instant: u64) void {
            for (&self.entries) |*entry| if (entry.handle.id != 0 and entry.phase != .terminal and instant >= entry.allocation.deadline_ns)
                entry.terminal(abi.gfx_queue_error_wait_timeout, instant);
        }
        pub fn closeProvider(self: *Self, provider: Handle, instant: u64) void {
            for (&self.entries) |*entry| if (entry.handle.id != 0 and entry.provider.eql(provider))
                entry.terminal(abi.gfx_queue_error_device_lost, instant);
        }
        pub fn retainsProvider(self: *const Self, provider: Handle) bool {
            for (&self.entries) |entry| if (entry.handle.id != 0 and entry.provider.eql(provider) and entry.claimed) return true;
            return false;
        }
        pub fn reap(self: *Self) void {
            for (&self.entries) |*entry| if (entry.handle.id != 0 and !entry.open and !entry.claimed and
                entry.reference.id == 0 and entry.waiters == 0 and !entry.notification_pending and !entry.publishing)
            {
                entry.* = .{};
            };
        }
    };
}

test "native allocation close, expiry, ownership and publication retain exact claims" {
    try @import("gfx_virtual_state.zig").checkLifetime();
    const t = std.testing;
    const app: Owner = .{ .kind = .program, .id = 7, .generation = 2 };
    const other: Owner = .{ .kind = .program, .id = 7, .generation = 3 };
    const provider: Handle = .{ .id = 1, .generation = 8 };
    const input: abi.GfxNativeAllocation = .{ .adapter_id = 3, .memory_generation = 9, .byte_length = 4096, .usage = 12, .deadline_ns = 100 };
    var store: Store(1) = .{};
    const entry = try store.start(app, provider, input, 10);
    const handle = entry.handle;
    try t.expectError(error.WrongOwner, store.owned(handle, other));
    _ = store.take(provider, 11).?;
    try t.expect(store.retainsProvider(provider));
    entry.waiters = 1;
    _ = try store.close(handle, app, 12);
    try t.expectEqual(abi.gfx_queue_error_wait_cancelled, entry.result);
    store.reap();
    try t.expectError(error.Capacity, store.start(other, provider, input, 13));
    try store.finish(handle, provider, 1, 14);
    try t.expectEqual(abi.gfx_queue_error_wait_cancelled, entry.result);
    try t.expect(!store.retainsProvider(provider));
    entry.notification_pending = false;
    entry.publishing = true;
    entry.waiters = 0;
    store.reap();
    try t.expect((try store.find(handle)).handle.eql(handle));
    entry.publishing = false;
    store.reap();
    try t.expectError(error.Stale, store.find(handle));
    const next = try store.start(other, provider, input, 15);
    _ = store.take(provider, 100);
    try t.expectEqual(Phase.terminal, next.phase);
    try t.expectEqual(abi.gfx_queue_error_wait_timeout, next.result);
    try t.expectError(error.Stale, store.finish(handle, provider, 1, 101));
    var invalid = input;
    invalid.deadline_ns = std.math.maxInt(u64);
    try t.expectError(error.Invalid, validate(invalid, 1));
    invalid = input;
    invalid.usage |= 1;
    try t.expectError(error.Invalid, validate(invalid, 1));
}
