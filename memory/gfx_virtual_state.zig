// Private C projection; all VA lifetime mutation belongs to Rust.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Owner = bo.Owner;
pub const Handle = bo.Handle;
pub const Error = bo.Error;
pub const Claim = enum(u32) { create = 0, retire = 1 };
const Node = extern struct {
    key: u64 = 0,
    priority: usize = 0,
    parent: ?*Node = null,
    children: [2]?*Node = @splat(null),
    pub fn next(self: *Node) ?*Node {
        return r4os_rust_gfx_virtual_node_next(self);
    }
};
const Tree = extern struct {
    root: ?*Node = null,
    xorshift: usize = 0,
    pub fn getMin(self: *const Tree) ?*Node {
        return r4os_rust_gfx_virtual_tree_first(self);
    }
    const Iterator = struct {
        node: ?*Node,
        pub fn next(self: *Iterator) ?*Node {
            const node = self.node orelse return null;
            self.node = node.next();
            return node;
        }
    };
    pub fn inorderIterator(self: *const Tree) Iterator {
        return .{ .node = self.getMin() };
    }
};
pub const Entry = extern struct {
    index: Node = .{},
    handle: Handle = .{},
    owner: Owner = .{ .kind = .program, .id = 0, .generation = 0 },
    provider: Handle = .{},
    request: abi.GfxVirtualRequest = .{},
    parent: ?*Entry = null,
    first_child: ?*Entry = null,
    next_child: ?*Entry = null,
    previous_child: ?*Entry = null,
    live_children: usize = 0,
    executions: usize = 0,
    open: bool = true,
    closing: bool = false,
    retired: bool = false,
    has_claim: bool = false,
    claim: Claim = .create,
    token: abi.GfxVirtualToken = .{},
    address: u64 = 0,
    result: i32 = 0,
    notifications: u8 = 0,
    pub fn claimValue(self: *const Entry) ?Claim {
        return if (self.has_claim) self.claim else null;
    }
    fn call(self: anytype, op: u32, value: u32, reason: i32, out: ?*anyopaque) Error!void {
        return nativeResult(r4os_rust_gfx_virtual_entry(@constCast(self), op, value, reason, out));
    }
    pub fn backend(self: *const Entry) bool {
        var out: bool = undefined;
        self.call(0, 0, 0, &out) catch unreachable;
        return out;
    }
    pub fn status(self: *const Entry) abi.GfxVirtualStatus {
        var out: abi.GfxVirtualStatus = undefined;
        self.call(1, 0, 0, &out) catch unreachable;
        return out;
    }
    pub fn settled(self: *const Entry, until: u32) bool {
        var out: bool = undefined;
        self.call(2, until, 0, &out) catch unreachable;
        return out;
    }
    pub fn close(self: *Entry, release: bool, reason: i32) void {
        self.call(3, @intFromBool(release), reason, null) catch unreachable;
    }
    pub fn pending(self: *const Entry) ?Claim {
        var out: u32 = undefined;
        self.call(4, 0, 0, &out) catch unreachable;
        return if (out == 2) null else @enumFromInt(out);
    }
    pub fn retainExecution(self: *Entry) Error!void {
        return self.call(5, 0, 0, null);
    }
    pub fn releaseExecution(self: *Entry) void {
        self.call(6, 0, 0, null) catch unreachable;
    }
};
const NativeArgs = extern struct {
    owner: Owner = .{ .kind = .program, .id = 0, .generation = 0 },
    provider: Handle = .{},
    handle: Handle = .{},
    input: ?*const abi.GfxVirtualRequest = null,
    completion: ?*const abi.GfxVirtualCompletion = null,
    entry: ?*Entry = null,
    instant: u64 = 0,
};
extern fn r4os_rust_gfx_virtual_state_run(*Store, u32, *const NativeArgs, ?*anyopaque) i32;
extern fn r4os_rust_gfx_virtual_entry(*Entry, u32, u32, i32, ?*anyopaque) i32;
extern fn r4os_rust_gfx_virtual_validate(*const abi.GfxVirtualRequest, u64) i32;
extern fn r4os_rust_gfx_virtual_tree_first(*const Tree) ?*Node;
extern fn r4os_rust_gfx_virtual_node_next(*Node) ?*Node;
extern fn r4os_rust_gfx_virtual_zero_token(*const abi.GfxVirtualToken) bool;
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
pub fn publicHandle(value: Handle) abi.GfxBufferHandle {
    return .{ .id = value.id, .generation = value.generation };
}
pub fn zeroToken(value: abi.GfxVirtualToken) bool {
    return r4os_rust_gfx_virtual_zero_token(&value);
}
pub fn validate(input: abi.GfxVirtualRequest, instant: u64) Error!void {
    return nativeResult(r4os_rust_gfx_virtual_validate(&input, instant));
}
pub const Store = extern struct {
    tree: Tree = .{},
    serial: u64 = 0,
    fn call(self: *Store, op: u32, args: NativeArgs, out: ?*anyopaque) Error!void {
        return nativeResult(r4os_rust_gfx_virtual_state_run(self, op, &args, out));
    }
    fn entry(self: *Store, op: u32, args: NativeArgs) Error!*Entry {
        var out: *Entry = undefined;
        try self.call(op, args, @ptrCast(&out));
        return out;
    }
    pub fn find(self: *Store, handle: Handle) Error!*Entry {
        return self.entry(0, .{ .handle = handle });
    }
    pub fn owned(self: *Store, handle: Handle, owner: Owner) Error!*Entry {
        return self.entry(1, .{ .handle = handle, .owner = owner });
    }
    pub fn parent(self: *Store, owner: Owner, provider: Handle, input: abi.GfxVirtualRequest) Error!?*Entry {
        var out: ?*Entry = undefined;
        try self.call(2, .{ .owner = owner, .provider = provider, .input = &input }, @ptrCast(&out));
        return out;
    }
    pub fn start(self: *Store, value: *Entry, owner: Owner, provider: Handle, input: abi.GfxVirtualRequest, instant: u64) Error!void {
        return self.call(3, .{ .entry = value, .owner = owner, .provider = provider, .input = &input, .instant = instant }, null);
    }
    pub fn expire(self: *Store, instant: u64) void {
        self.call(4, .{ .instant = instant }, null) catch unreachable;
    }
    pub fn take(self: *Store, provider: Handle, instant: u64) ?*Entry {
        var out: ?*Entry = undefined;
        self.call(5, .{ .provider = provider, .instant = instant }, @ptrCast(&out)) catch unreachable;
        return out;
    }
    pub fn complete(self: *Store, provider: Handle, completion: abi.GfxVirtualCompletion) Error!void {
        return self.call(6, .{ .provider = provider, .completion = &completion }, null);
    }
    pub fn closeProvider(self: *Store, provider: Handle) void {
        self.call(7, .{ .provider = provider }, null) catch unreachable;
    }
    pub fn stopped(self: *Store, owner: Owner) void {
        self.call(8, .{ .owner = owner }, null) catch unreachable;
    }
    pub fn removable(value: *const Entry) bool {
        var out: bool = undefined;
        value.call(7, 0, 0, &out) catch unreachable;
        return out;
    }
    pub fn remove(self: *Store, value: *Entry) void {
        self.call(9, .{ .entry = value }, null) catch unreachable;
    }
};

comptime {
    if (@sizeOf(Node) != 40 or @alignOf(Node) != 8) @compileError("native VA Node");
    if (@offsetOf(Node, "key") != 0) @compileError("native VA Node.key");
    if (@offsetOf(Node, "priority") != 8) @compileError("native VA Node.priority");
    if (@offsetOf(Node, "parent") != 16) @compileError("native VA Node.parent");
    if (@offsetOf(Node, "children") != 24) @compileError("native VA Node.children");
    if (@sizeOf(Tree) != 16 or @alignOf(Tree) != 8) @compileError("native VA Tree");
    if (@offsetOf(Tree, "root") != 0) @compileError("native VA Tree.root");
    if (@offsetOf(Tree, "xorshift") != 8) @compileError("native VA Tree.xorshift");
    if (@sizeOf(Entry) != 312 or @alignOf(Entry) != 8) @compileError("native VA Entry");
    if (@offsetOf(Entry, "index") != 0) @compileError("native VA Entry.index");
    if (@offsetOf(Entry, "handle") != 40) @compileError("native VA Entry.handle");
    if (@offsetOf(Entry, "owner") != 56) @compileError("native VA Entry.owner");
    if (@offsetOf(Entry, "provider") != 80) @compileError("native VA Entry.provider");
    if (@offsetOf(Entry, "request") != 96) @compileError("native VA Entry.request");
    if (@offsetOf(Entry, "parent") != 216) @compileError("native VA Entry.parent");
    if (@offsetOf(Entry, "first_child") != 224) @compileError("native VA Entry.first_child");
    if (@offsetOf(Entry, "next_child") != 232) @compileError("native VA Entry.next_child");
    if (@offsetOf(Entry, "previous_child") != 240) @compileError("native VA Entry.previous_child");
    if (@offsetOf(Entry, "live_children") != 248) @compileError("native VA Entry.live_children");
    if (@offsetOf(Entry, "executions") != 256) @compileError("native VA Entry.executions");
    if (@offsetOf(Entry, "open") != 264) @compileError("native VA Entry.open");
    if (@offsetOf(Entry, "closing") != 265) @compileError("native VA Entry.closing");
    if (@offsetOf(Entry, "retired") != 266) @compileError("native VA Entry.retired");
    if (@offsetOf(Entry, "has_claim") != 267) @compileError("native VA Entry.has_claim");
    if (@offsetOf(Entry, "claim") != 268) @compileError("native VA Entry.claim");
    if (@offsetOf(Entry, "token") != 272) @compileError("native VA Entry.token");
    if (@offsetOf(Entry, "address") != 296) @compileError("native VA Entry.address");
    if (@offsetOf(Entry, "result") != 304) @compileError("native VA Entry.result");
    if (@offsetOf(Entry, "notifications") != 308) @compileError("native VA Entry.notifications");
    if (@sizeOf(Store) != 24 or @alignOf(Store) != 8) @compileError("native VA Store");
    if (@offsetOf(Store, "tree") != 0) @compileError("native VA Store.tree");
    if (@offsetOf(Store, "serial") != 16) @compileError("native VA Store.serial");
    if (@sizeOf(NativeArgs) != 88 or @alignOf(NativeArgs) != 8) @compileError("native VA Args");
    if (@offsetOf(NativeArgs, "owner") != 0) @compileError("native VA Args.owner");
    if (@offsetOf(NativeArgs, "provider") != 24) @compileError("native VA Args.provider");
    if (@offsetOf(NativeArgs, "handle") != 40) @compileError("native VA Args.handle");
    if (@offsetOf(NativeArgs, "input") != 56) @compileError("native VA Args.input");
    if (@offsetOf(NativeArgs, "completion") != 64) @compileError("native VA Args.completion");
    if (@offsetOf(NativeArgs, "entry") != 72) @compileError("native VA Args.entry");
    if (@offsetOf(NativeArgs, "instant") != 80) @compileError("native VA Args.instant");
}

// Called by the existing native-allocation owner test, not a new test gate.
pub fn checkLifetime() !void {
    const t = std.testing;
    const app: Owner = .{ .kind = .program, .id = 7, .generation = 2 };
    const other: Owner = .{ .kind = .program, .id = 7, .generation = 3 };
    const provider: Handle = .{ .id = 3, .generation = 8 };
    var store: Store = .{};
    var range: Entry = .{};
    var binding: Entry = .{};
    var extra: Entry = .{};
    const input: abi.GfxVirtualRequest = .{ .kind = 1, .adapter_id = 3, .memory_generation = 9, .byte_length = 16384, .alignment = 4096, .deadline_ns = 100, .flags = abi.gfx_virtual_flag_blocklinear | (255 << abi.gfx_virtual_layout_shift) };
    // The broker preserves a driver-specific byte, including IDs unknown to
    // NVIDIA, without interpreting it. Reserved bits and a pitch-only layout
    // ID are invalid before any provider work or resource retention.
    var malformed = input;
    malformed.flags &= ~abi.gfx_virtual_flag_blocklinear;
    try t.expectError(error.Invalid, validate(malformed, 1));
    malformed.flags = input.flags | (1 << 16);
    try t.expectError(error.Unsupported, validate(malformed, 1));
    try store.start(&range, app, provider, input, 1);
    try t.expectEqual(input.flags, range.request.flags);
    const old = range.handle;
    try t.expectError(error.WrongOwner, store.owned(old, other));
    try t.expect(store.take(provider, 2).? == &range);
    const token: abi.GfxVirtualToken = .{ .opaque0 = 9, .opaque1 = 8 };
    var completion: abi.GfxVirtualCompletion = .{ .resource = publicHandle(old), .result = 1, .token = token, .address = 0x90000000 };
    try store.complete(provider, completion);
    var request: abi.GfxVirtualRequest = .{ .kind = 2, .adapter_id = 3, .memory_generation = 9, .parent = publicHandle(old), .reference = .{ .id = 2, .generation = 6 }, .byte_offset = 4096, .virtual_offset = 8192, .byte_length = 4096, .deadline_ns = 100 };
    try store.start(&binding, app, provider, request, 3);
    try t.expectError(error.Busy, store.start(&extra, app, provider, request, 3));
    try t.expect(store.take(provider, 4).? == &binding);
    // Close/release while claimed cannot retire either resource. The actual
    // late successful bind remains represented and must unmap before its VA.
    store.stopped(app);
    try t.expect(!range.open and !binding.open and !binding.retired and range.live_children == 1 and store.take(provider, 5) == null);
    completion = .{ .resource = publicHandle(binding.handle), .result = 1, .token = .{ .opaque0 = 9, .opaque1 = 8, .opaque2 = 1 }, .address = 0x90002000 };
    try store.complete(provider, completion);
    try t.expect(binding.result == abi.gfx_queue_error_wait_cancelled and binding.status().address == 0);
    try t.expect(store.take(provider, 6).? == &binding);
    completion.operation = 1;
    completion.address = 0;
    completion.token.opaque2 += 1;
    try t.expectError(error.Invalid, store.complete(provider, completion));
    try t.expect(binding.claimValue().? == .retire and !binding.retired);
    completion.token.opaque2 -= 1;
    try store.complete(provider, completion);
    try t.expect(binding.retired and range.live_children == 0);
    try t.expect(store.take(provider, 7).? == &range);
    completion = .{ .resource = publicHandle(old), .operation = 1, .result = 1, .token = token };
    try store.complete(provider, completion);
    try t.expect(range.retired and !Store.removable(&range));
    binding.notifications = 0;
    store.remove(&binding);
    range.notifications = 0;
    store.remove(&range);
    try t.expectError(error.Stale, store.find(old));
    // Timeout before claim performs no driver operation; a timed-out claimed
    // range still needs retirement after the late allocation succeeds.
    try store.start(&range, app, provider, input, 9);
    try t.expect(range.handle.generation > old.generation);
    try t.expect(store.take(provider, 10).? == &range);
    store.expire(100);
    try t.expect(range.result == abi.gfx_queue_error_wait_timeout and !range.retired);
    completion = .{ .resource = publicHandle(range.handle), .result = 1, .token = token, .address = 0x90000000 };
    try store.complete(provider, completion);
    try t.expect(range.result == abi.gfx_queue_error_wait_timeout and store.take(provider, 101).? == &range);
    completion.operation = 1;
    completion.address = 0;
    try store.complete(provider, completion);
    range.close(true, abi.gfx_queue_error_wait_cancelled);
    range.notifications = 0;
    store.remove(&range);
    try store.start(&range, other, provider, input, 11);
    store.closeProvider(provider);
    try t.expect(range.retired and range.result == abi.gfx_queue_error_device_lost and store.take(provider, 12) == null);
    range.close(true, abi.gfx_queue_error_wait_cancelled);
    range.notifications = 0;
    store.remove(&range);
    request.byte_offset = std.math.maxInt(u64) - 4095;
    try t.expectError(error.Invalid, validate(request, 1));
    try t.expect(store.tree.getMin() == null);
}
