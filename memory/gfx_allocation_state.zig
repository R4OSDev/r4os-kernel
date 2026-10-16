// Fixed C projection of the original native allocation owner.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Owner = bo.Owner;
pub const Handle = bo.Handle;
pub const Error = bo.Error;
pub const result_owner = Owner{ .kind = .kernel, .id = 4, .generation = 1 };
pub const Phase = enum(u32) { queued, claimed, terminal };
pub const Entry = extern struct {
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
        var out: abi.GfxNativeStatus = undefined;
        r4os_rust_gfx_allocation_entry(@constCast(self), 0, 0, 0, &out);
        return out;
    }
    pub fn terminal(self: *Entry, result: i32, instant: u64) void {
        r4os_rust_gfx_allocation_entry(self, 1, result, instant, null);
    }
};
const NativeView = extern struct { entries: [*]Entry, capacity: usize, serial: *u64 };
const NativeArgs = extern struct {
    owner: Owner = .{ .kind = .program, .id = 0, .generation = 0 },
    provider: Handle = .{},
    handle: Handle = .{},
    input: ?*const abi.GfxNativeAllocation = null,
    instant: u64 = 0,
    result: i32 = 0,
};
extern fn r4os_rust_gfx_allocation_state_run(*const NativeView, u32, *const NativeArgs, ?*anyopaque) i32;
extern fn r4os_rust_gfx_allocation_entry(*Entry, u32, i32, u64, ?*abi.GfxNativeStatus) void;
extern fn r4os_rust_gfx_allocation_validate(*const abi.GfxNativeAllocation, u64) i32;
extern fn r4os_rust_gfx_allocation_matches(*const abi.GfxNativeAllocation, *const bo.layout.Descriptor, *const Owner) bool;
fn nativeResult(value: i32) Error!void {
    return switch (value) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        else => unreachable,
    };
}
pub fn validate(input: abi.GfxNativeAllocation, instant: u64) Error!void {
    return nativeResult(r4os_rust_gfx_allocation_validate(&input, instant));
}
pub fn matches(input: abi.GfxNativeAllocation, descriptor: bo.layout.Descriptor, driver: Owner) bool {
    return r4os_rust_gfx_allocation_matches(&input, &descriptor, &driver);
}
pub fn Store(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = @splat(Entry{}),
        serial: u64 = 0,
        fn call(self: anytype, comptime op: u32, args: NativeArgs, out: ?*anyopaque) Error!void {
            const view = NativeView{ .entries = @constCast(&self.entries), .capacity = capacity, .serial = @constCast(&self.serial) };
            return nativeResult(r4os_rust_gfx_allocation_state_run(&view, op, &args, out));
        }
        fn entry(self: *Self, comptime op: u32, args: NativeArgs) Error!*Entry {
            var out: *Entry = undefined;
            try self.call(op, args, @ptrCast(&out));
            return out;
        }
        pub fn find(self: *Self, handle: Handle) Error!*Entry {
            return self.entry(0, .{ .handle = handle });
        }
        pub fn owned(self: *Self, handle: Handle, owner: Owner) Error!*Entry {
            return self.entry(1, .{ .handle = handle, .owner = owner });
        }
        pub fn start(self: *Self, owner: Owner, provider: Handle, input: abi.GfxNativeAllocation, instant: u64) Error!*Entry {
            return self.entry(2, .{ .owner = owner, .provider = provider, .input = &input, .instant = instant });
        }
        pub fn take(self: *Self, provider: Handle, instant: u64) ?*Entry {
            var out: ?*Entry = undefined;
            self.call(3, .{ .provider = provider, .instant = instant }, @ptrCast(&out)) catch unreachable;
            return out;
        }
        pub fn claim(self: *Self, handle: Handle, provider: Handle) Error!*Entry {
            return self.entry(4, .{ .handle = handle, .provider = provider });
        }
        pub fn finish(self: *Self, handle: Handle, provider: Handle, result: i32, instant: u64) Error!void {
            return self.call(5, .{ .handle = handle, .provider = provider, .result = result, .instant = instant }, null);
        }
        pub fn close(self: *Self, handle: Handle, owner: Owner, instant: u64) Error!Handle {
            var out: Handle = undefined;
            try self.call(6, .{ .handle = handle, .owner = owner, .instant = instant }, &out);
            return out;
        }
        pub fn expire(self: *Self, instant: u64) void {
            self.call(7, .{ .instant = instant }, null) catch unreachable;
        }
        pub fn closeProvider(self: *Self, provider: Handle, instant: u64) void {
            self.call(8, .{ .provider = provider, .instant = instant }, null) catch unreachable;
        }
        pub fn retainsProvider(self: *const Self, provider: Handle) bool {
            var out: bool = undefined;
            self.call(9, .{ .provider = provider }, &out) catch unreachable;
            return out;
        }
        pub fn reap(self: *Self) void {
            self.call(10, .{}, null) catch unreachable;
        }
    };
}

comptime {
    if (@sizeOf(Entry) != 168 or @alignOf(Entry) != 8) @compileError("native allocation Entry");
    if (@offsetOf(Entry, "handle") != 0) @compileError("native allocation Entry.handle");
    if (@offsetOf(Entry, "owner") != 16) @compileError("native allocation Entry.owner");
    if (@offsetOf(Entry, "provider") != 40) @compileError("native allocation Entry.provider");
    if (@offsetOf(Entry, "allocation") != 56) @compileError("native allocation Entry.allocation");
    if (@offsetOf(Entry, "phase") != 120) @compileError("native allocation Entry.phase");
    if (@offsetOf(Entry, "result") != 124) @compileError("native allocation Entry.result");
    if (@offsetOf(Entry, "completed_ns") != 128) @compileError("native allocation Entry.completed_ns");
    if (@offsetOf(Entry, "open") != 136) @compileError("native allocation Entry.open");
    if (@offsetOf(Entry, "claimed") != 137) @compileError("native allocation Entry.claimed");
    if (@offsetOf(Entry, "reference") != 144) @compileError("native allocation Entry.reference");
    if (@offsetOf(Entry, "waiters") != 160) @compileError("native allocation Entry.waiters");
    if (@offsetOf(Entry, "notification_pending") != 164) @compileError("native allocation Entry.notification_pending");
    if (@offsetOf(Entry, "publishing") != 165) @compileError("native allocation Entry.publishing");
    if (@sizeOf(NativeView) != 24 or @alignOf(NativeView) != 8) @compileError("native allocation View");
    if (@offsetOf(NativeView, "entries") != 0) @compileError("native allocation View.entries");
    if (@offsetOf(NativeView, "capacity") != 8) @compileError("native allocation View.capacity");
    if (@offsetOf(NativeView, "serial") != 16) @compileError("native allocation View.serial");
    if (@sizeOf(NativeArgs) != 80 or @alignOf(NativeArgs) != 8) @compileError("native allocation Args");
    if (@offsetOf(NativeArgs, "owner") != 0) @compileError("native allocation Args.owner");
    if (@offsetOf(NativeArgs, "provider") != 24) @compileError("native allocation Args.provider");
    if (@offsetOf(NativeArgs, "handle") != 40) @compileError("native allocation Args.handle");
    if (@offsetOf(NativeArgs, "input") != 56) @compileError("native allocation Args.input");
    if (@offsetOf(NativeArgs, "instant") != 64) @compileError("native allocation Args.instant");
    if (@offsetOf(NativeArgs, "result") != 72) @compileError("native allocation Args.result");
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
