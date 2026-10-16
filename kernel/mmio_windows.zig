// Owner-bound MMIO windows. Rust owns admission, records, accounting and
// rollback; the existing wait-spanning driver execution guard serializes it.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Owner = lifetime.Owner;
pub const Handle = lifetime.Handle;
pub const page_size: u64 = 4096;
pub const Policy = enum(u8) { other, write_combining, uncached };
pub const Request = extern struct { resource_base: u64, resource_bytes: u64, offset: u64, bytes: u64, policy: Policy, prefetchable: bool = false };
pub const Extent = extern struct { physical: u64, bytes: u64 };
pub const Error = error{ Invalid, Overflow, Unsupported, Busy, Capacity, Stale, Exhausted, MapFailed };
pub const Window = struct { handle: Handle, cpu: u64, physical: u64, bytes: u64, policy: Policy, borrowed: bool };
const NativeOwner = extern struct {
    id: u64,
    generation: u64,
    kind: u8,
    fn from(value: Owner) NativeOwner {
        return .{ .id = value.id, .generation = value.generation, .kind = @intCast(@intFromEnum(value.kind)) };
    }
};
const NativeHandle = extern struct { generation: u64 = 0, id: u32 = 0 };
const NativeWindow = extern struct {
    handle: NativeHandle = .{},
    cpu: u64 = 0,
    physical: u64 = 0,
    bytes: u64 = 0,
    policy: Policy = .other,
    borrowed: bool = false,
    fn public(self: NativeWindow) Window {
        return .{ .handle = .{ .id = self.handle.id, .generation = self.handle.generation }, .cpu = self.cpu, .physical = self.physical, .bytes = self.bytes, .policy = self.policy, .borrowed = self.borrowed };
    }
};
const Record = extern struct {
    owner: NativeOwner = .{ .id = 0, .generation = 0, .kind = 2 },
    window: NativeWindow = .{},
    mapped: u64 = 0,
    used: bool = false,
    retiring: bool = false,
};
const NativeBackend = extern struct {
    context: *anyopaque,
    call: *const fn (*anyopaque, u8, u64, u64, u8, *u64) callconv(.c) bool,
};
extern fn r4os_rust_mmio_cache_selector(pat: u64, memory_type: u8) u8;
extern fn r4os_rust_mmio_validate(request: *const Request, output: *Extent) u8;
extern fn r4os_rust_mmio_create(state: *anyopaque, backend: *const NativeBackend, owner: *const NativeOwner, request: *const Request, output: *NativeWindow) u8;
extern fn r4os_rust_mmio_release(state: *anyopaque, backend: *const NativeBackend, owner: *const NativeOwner, handle: *const NativeHandle, quiesced: bool) u8;
extern fn r4os_rust_mmio_query(state: *anyopaque, backend: ?*const NativeBackend, owner: *const NativeOwner, operation: u8) bool;
comptime {
    if (@sizeOf(NativeOwner) != 24 or @sizeOf(NativeWindow) != 48 or @sizeOf(Record) != 88 or @sizeOf(Request) != 40) @compileError("Native MMIO C layout drift");
}
fn checked(code: u8) Error!void {
    return switch (code) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Busy,
        5 => error.Capacity,
        6 => error.Stale,
        7 => error.Exhausted,
        8 => error.MapFailed,
        else => @panic("Invalid native MMIO result"),
    };
}
pub fn cacheSelector(pat: u64, memory_type: u8) ?u3 {
    const index = r4os_rust_mmio_cache_selector(pat, memory_type);
    return if (index == 255) null else @intCast(index);
}
pub fn validate(request: Request) Error!Extent {
    var output: Extent = undefined;
    try checked(r4os_rust_mmio_validate(&request, &output));
    return output;
}
pub fn Manager(comptime Backend: type) type {
    return extern struct {
        const Self = @This();
        records: [64]Record = .{Record{}} ** 64,
        serial: u64 = 0,
        charged: u64 = 0,
        budget: u64 = 1024 * 1024 * 1024,
        comptime {
            if (@sizeOf(Self) != 5656 or @offsetOf(Self, "serial") != 5632) @compileError("Native MMIO state layout drift");
        }
        fn call(context: *anyopaque, operation: u8, address: u64, bytes_or_physical: u64, policy: u8, output: *u64) callconv(.c) bool {
            const backend: *Backend = @ptrCast(@alignCast(context));
            return switch (operation) {
                0 => backend.deviceSpan(address, bytes_or_physical),
                1 => blk: {
                    output.* = backend.cpuAddress(address, bytes_or_physical) orelse break :blk false;
                    break :blk true;
                },
                2 => blk: {
                    output.* = @intFromEnum(backend.policy(address) orelse break :blk false);
                    break :blk true;
                },
                3 => backend.map(address, bytes_or_physical, @enumFromInt(policy)),
                4 => backend.unmap(address),
                else => @panic("Invalid native MMIO backend operation"),
            };
        }
        fn adapter(backend: *Backend) NativeBackend {
            return .{ .context = backend, .call = call };
        }
        pub fn create(self: *Self, backend: *Backend, owner: Owner, request: Request) Error!Window {
            var output: NativeWindow = undefined;
            try checked(r4os_rust_mmio_create(self, &adapter(backend), &NativeOwner.from(owner), &request, &output));
            return output.public();
        }
        pub fn release(self: *Self, backend: *Backend, owner: Owner, handle: Handle, quiesced: bool) Error!void {
            try checked(r4os_rust_mmio_release(self, &adapter(backend), &NativeOwner.from(owner), &.{ .generation = handle.generation, .id = handle.id }, quiesced));
        }
        pub fn collect(self: *Self, backend: *Backend, owner: Owner) bool {
            return r4os_rust_mmio_query(self, &adapter(backend), &NativeOwner.from(owner), 0);
        }
        pub fn retains(self: *const Self, owner: Owner) bool {
            return r4os_rust_mmio_query(@constCast(self), null, &NativeOwner.from(owner), 1);
        }
        pub fn pending(self: *const Self, owner: Owner) bool {
            return r4os_rust_mmio_query(@constCast(self), null, &NativeOwner.from(owner), 2);
        }
    };
}

test "MMIO windows keep 64-bit offsets, reject cache aliases and retain failed TLB release" {
    try std.testing.expectEqual(@as(?u3, 3), cacheSelector(0x0007_0406_0007_0406, 0));
    try std.testing.expectEqual(@as(?u3, 2), cacheSelector(0x0606_0606_0600_0606, 0));
    try std.testing.expectEqual(@as(?u3, null), cacheSelector(0x0707_0707_0707_0707, 0));
    const Fake = struct {
        entries: [8]?Policy = .{null} ** 8,
        fail_map_after: usize = 8,
        fail_unmap: bool = false,
        pub fn deviceSpan(_: *@This(), _: u64, _: u64) bool {
            return true;
        }
        pub fn cpuAddress(_: *@This(), _: u64, bytes: u64) ?u64 {
            return if (bytes <= 8 * page_size) 0x1000 else null;
        }
        pub fn policy(self: *@This(), address: u64) ?Policy {
            return self.entries[(address - 0x1000) / page_size];
        }
        pub fn map(self: *@This(), address: u64, _: u64, cache: Policy) bool {
            const index = (address - 0x1000) / page_size;
            if (index >= self.fail_map_after) return false;
            self.entries[index] = cache;
            return true;
        }
        pub fn unmap(self: *@This(), address: u64) bool {
            if (self.fail_unmap) return false;
            self.entries[(address - 0x1000) / page_size] = null;
            return true;
        }
    };
    const t = std.testing;
    const owner = Owner{ .kind = .driver, .id = 7, .generation = 11 };
    const request = Request{ .resource_base = 0x1_0000_0000, .resource_bytes = 0x2_0000_0000, .offset = 0x1_0000_1000, .bytes = 4 * page_size, .policy = .uncached };
    var backend = Fake{};
    var manager = Manager(Fake){};
    const window = try manager.create(&backend, owner, request);
    try t.expect(!manager.pending(owner));
    try t.expectEqual(@as(u64, 0x2_0000_1000), window.physical);
    try t.expect(window.cpu != window.physical);
    try t.expectError(error.Busy, manager.release(&backend, owner, window.handle, false));
    backend.fail_unmap = true;
    try t.expectError(error.Busy, manager.release(&backend, owner, window.handle, true));
    try t.expect(manager.pending(owner));
    try t.expect(manager.retains(owner));
    try t.expectEqual(request.bytes, manager.charged);
    backend.fail_unmap = false;
    try t.expect(manager.collect(&backend, owner));
    try t.expect(!manager.pending(owner));
    try t.expectEqual(@as(u64, 0), manager.charged);
    try t.expectError(error.Stale, manager.release(&backend, owner, window.handle, true));
    backend.entries = .{Policy.write_combining} ** 8;
    try t.expectError(error.Unsupported, manager.create(&backend, owner, request));
    backend.entries = .{null} ** 8;
    backend.fail_map_after = 2;
    backend.fail_unmap = true;
    try t.expectError(error.MapFailed, manager.create(&backend, owner, request));
    try t.expect(manager.retains(owner));
    backend.fail_unmap = false;
    try t.expect(manager.collect(&backend, owner));
    try t.expectEqual(@as(u64, 0), manager.charged);
}
