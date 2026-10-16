// Private resident C records; native driver epochs/device nodes own all mutation.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
pub const Owner = lifetime.Owner;
pub const Error = lifetime.Error;
const Node = extern struct { key: u64 = 0, priority: usize = 0, parent: ?*Node = null, children: [2]?*Node = @splat(null) };
const Tree = extern struct { root: ?*Node = null, xorshift: usize = 0 };
const Epoch = extern struct { identity: Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, closing: bool = false, mmio_busy: bool = false, mmio_retained: bool = false, mmio_pending: bool = false, devices: usize = 0 };
const DeviceRecord = extern struct { index: Node = .{}, self_address: usize = 0, state_address: usize = 0, owner: Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, descriptor: abi.GfxDeviceLease = .{}, busy: bool = false };
const View = extern struct { epochs: [*]Epoch, capacity: usize, devices: *Tree, identity: usize };
extern fn r4os_rust_gfx_mapping_range(u64, u64, u64, u64) bool;
extern fn r4os_rust_gfx_driver_model_bind(*const View, u32, u64) bool;
extern fn r4os_rust_gfx_driver_model_owner(*const View, u32, bool, *Owner) i32;
extern fn r4os_rust_gfx_driver_model_close(*const View, u32) void;
extern fn r4os_rust_gfx_driver_model_retire(*const View, *const Owner) bool;
extern fn r4os_rust_gfx_driver_model_retained(*const View, *const Owner, bool) bool;
extern fn r4os_rust_gfx_driver_model_reserve(*const View, *const Owner, *const abi.GfxDeviceLease, *DeviceRecord) i32;
extern fn r4os_rust_gfx_driver_model_matching(*const View, *const Owner, *const abi.GfxDeviceLease, **DeviceRecord) i32;
extern fn r4os_rust_gfx_driver_model_detach(*const View, *DeviceRecord) i32;
extern fn r4os_rust_gfx_driver_model_mmio(*const View, *const Owner, bool, bool, bool) void;
fn status(code: i32) Error!void {
    return switch (code) {
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
pub fn mappingRange(offset: u64, bytes: u64, address: u64, object_bytes: u64) bool {
    return r4os_rust_gfx_mapping_range(offset, bytes, address, object_bytes);
}
pub fn State(comptime owner_capacity: usize) type {
    return struct {
        const Self = @This();
        pub const Device = DeviceRecord;
        epochs: [owner_capacity]Epoch = @splat(.{}),
        devices: Tree = .{},
        fn view(self: *Self) View {
            return .{ .epochs = self.epochs[0..].ptr, .capacity = owner_capacity, .devices = &self.devices, .identity = @intFromPtr(self) };
        }
        pub fn bind(self: *Self, id: u32, generation: u64) bool {
            const v = self.view();
            return r4os_rust_gfx_driver_model_bind(&v, id, generation);
        }
        pub fn owner(self: *Self, id: u32, admission: bool) Error!Owner {
            const v = self.view();
            var out: Owner = undefined;
            try status(r4os_rust_gfx_driver_model_owner(&v, id, admission, &out));
            return out;
        }
        pub fn close(self: *Self, id: u32) void {
            const v = self.view();
            r4os_rust_gfx_driver_model_close(&v, id);
        }
        pub fn retire(self: *Self, identity: Owner) bool {
            const v = self.view();
            return r4os_rust_gfx_driver_model_retire(&v, &identity);
        }
        pub fn retains(self: *Self, identity: Owner) bool {
            const v = self.view();
            return r4os_rust_gfx_driver_model_retained(&v, &identity, false);
        }
        pub fn reserve(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease, record: *Device) Error!void {
            const v = self.view();
            try status(r4os_rust_gfx_driver_model_reserve(&v, &identity, &descriptor, record));
        }
        pub fn matching(self: *Self, identity: Owner, descriptor: abi.GfxDeviceLease) Error!*Device {
            const v = self.view();
            var out: *Device = undefined;
            try status(r4os_rust_gfx_driver_model_matching(&v, &identity, &descriptor, &out));
            return out;
        }
        pub fn detach(self: *Self, record: *Device) Error!void {
            const v = self.view();
            try status(r4os_rust_gfx_driver_model_detach(&v, record));
        }
        pub fn beginMmio(self: *Self, identity: Owner) void {
            const v = self.view();
            r4os_rust_gfx_driver_model_mmio(&v, &identity, true, false, false);
        }
        pub fn endMmio(self: *Self, identity: Owner, retained: bool, pending: bool) void {
            const v = self.view();
            r4os_rust_gfx_driver_model_mmio(&v, &identity, false, retained, pending);
        }
        pub fn pendingMmio(self: *Self, identity: Owner) bool {
            const v = self.view();
            return r4os_rust_gfx_driver_model_retained(&v, &identity, true);
        }
    };
}

comptime {
    if (@sizeOf(Node) != 40 or @alignOf(Node) != 8) @compileError("Native driver memory Node C layout");
    if (@offsetOf(Node, "key") != 0) @compileError("Native driver memory Node.key C offset");
    if (@offsetOf(Node, "priority") != 8) @compileError("Native driver memory Node.priority C offset");
    if (@offsetOf(Node, "parent") != 16) @compileError("Native driver memory Node.parent C offset");
    if (@offsetOf(Node, "children") != 24) @compileError("Native driver memory Node.children C offset");
    if (@sizeOf(Tree) != 16 or @alignOf(Tree) != 8) @compileError("Native driver memory Tree C layout");
    if (@offsetOf(Tree, "root") != 0) @compileError("Native driver memory Tree.root C offset");
    if (@offsetOf(Tree, "xorshift") != 8) @compileError("Native driver memory Tree.xorshift C offset");
    if (@sizeOf(Epoch) != 40 or @alignOf(Epoch) != 8) @compileError("Native driver memory Epoch C layout");
    if (@offsetOf(Epoch, "identity") != 0) @compileError("Native driver memory Epoch.identity C offset");
    if (@offsetOf(Epoch, "closing") != 24) @compileError("Native driver memory Epoch.closing C offset");
    if (@offsetOf(Epoch, "mmio_busy") != 25) @compileError("Native driver memory Epoch.mmio_busy C offset");
    if (@offsetOf(Epoch, "mmio_retained") != 26) @compileError("Native driver memory Epoch.mmio_retained C offset");
    if (@offsetOf(Epoch, "mmio_pending") != 27) @compileError("Native driver memory Epoch.mmio_pending C offset");
    if (@offsetOf(Epoch, "devices") != 32) @compileError("Native driver memory Epoch.devices C offset");
    if (@sizeOf(DeviceRecord) != 168 or @alignOf(DeviceRecord) != 8) @compileError("Native driver memory DeviceRecord C layout");
    if (@offsetOf(DeviceRecord, "index") != 0) @compileError("Native driver memory DeviceRecord.index C offset");
    if (@offsetOf(DeviceRecord, "self_address") != 40) @compileError("Native driver memory DeviceRecord.self_address C offset");
    if (@offsetOf(DeviceRecord, "state_address") != 48) @compileError("Native driver memory DeviceRecord.state_address C offset");
    if (@offsetOf(DeviceRecord, "owner") != 56) @compileError("Native driver memory DeviceRecord.owner C offset");
    if (@offsetOf(DeviceRecord, "descriptor") != 80) @compileError("Native driver memory DeviceRecord.descriptor C offset");
    if (@offsetOf(DeviceRecord, "busy") != 160) @compileError("Native driver memory DeviceRecord.busy C offset");
    if (@sizeOf(View) != 32 or @alignOf(View) != 8) @compileError("Native driver memory View C layout");
    if (@offsetOf(View, "epochs") != 0) @compileError("Native driver memory View.epochs C offset");
    if (@offsetOf(View, "capacity") != 8) @compileError("Native driver memory View.capacity C offset");
    if (@offsetOf(View, "devices") != 16) @compileError("Native driver memory View.devices C offset");
    if (@offsetOf(View, "identity") != 24) @compileError("Native driver memory View.identity C offset");
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
        var wrong = descriptor;
        wrong.lease.generation += 1;
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

test "GPU residency admits only the final logical partial page without enlarging the lease" {
    const t = std.testing;
    try t.expect(mappingRange(0, 5001, 0x100000, 5001));
    try t.expect(mappingRange(4096, 905, 0x100000, 5001));
    try t.expect(mappingRange(0, 4096, 0x100000, 5001));
    try t.expect(!mappingRange(0, 4097, 0x100000, 5001));
    try t.expect(!mappingRange(0, 8192, 0x100000, 5001));
    try t.expect(!mappingRange(1, 5000, 0x100000, 5001));
    try t.expect(!mappingRange(0, 5001, 0x100001, 5001));
    try t.expect(!mappingRange(0, 8192, 0xfffffffffffff000, 8192));
    try t.expect(!mappingRange(0, 0, 0x1000, 1));
}
