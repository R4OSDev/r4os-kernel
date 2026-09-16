// Cross-owner GPU virtual-resource lifetime. Resident caller-allocated nodes;
// hardware placement, execution, allocation and notification stay outside.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const bo = @import("gfx_buffer_owner.zig");
pub const Owner = bo.Owner;
pub const Handle = bo.Handle;
pub const Error = bo.Error;
const Tree = std.Treap(u64, std.math.order);
pub const Claim = enum(u32) { create = 0, retire = 1 };
pub const Entry = struct {
    index: Tree.Node = undefined,
    handle: Handle = .{},
    owner: Owner = .{ .kind = .program, .id = 0, .generation = 0 },
    provider: Handle = .{},
    request: abi.GfxVirtualRequest = .{},
    parent: ?*Entry = null,
    first_child: ?*Entry = null,
    next_child: ?*Entry = null,
    previous_child: ?*Entry = null,
    live_children: usize = 0,
    open: bool = true,
    closing: bool = false,
    retired: bool = false,
    claim: ?Claim = null,
    token: abi.GfxVirtualToken = .{},
    address: u64 = 0,
    result: i32 = 0,
    notifications: u2 = 0,

    pub fn backend(self: *const Entry) bool { return !zeroToken(self.token); }
    pub fn status(self: *const Entry) abi.GfxVirtualStatus {
        return .{ .resource = publicHandle(self.handle), .parent = self.request.parent, .kind = self.request.kind,
            .flags = @as(u32, @intFromBool(self.result != 0)) | (@as(u32, @intFromBool(self.retired)) << 1) |
                (@as(u32, @intFromBool(self.closing)) << 2) | (@as(u32, @intFromBool(self.claim != null)) << 3),
            .result = self.result, .address = if (self.result == 1 and !self.closing) self.address else 0,
            .byte_length = self.request.byte_length, .deadline_ns = self.request.deadline_ns };
    }
    pub fn settled(self: *const Entry, until: u32) bool {
        return if (until == 0) self.result != 0 else self.retired;
    }
    fn finishResult(self: *Entry, result: i32) void {
        if (self.result != 0) return;
        self.result = result;
        self.notifications |= 1;
    }
    // Parent metadata remains linked until the child handle is released, but
    // physical parent retirement only waits for physical child retirement.
    fn settleRetirement(self: *Entry) void {
        if (self.retired or !self.closing or self.claim != null or self.backend() or self.live_children != 0) return;
        self.retired = true;
        self.notifications |= 2;
        if (self.parent) |parent| {
            std.debug.assert(parent.live_children != 0);
            parent.live_children -= 1;
            parent.settleRetirement();
        }
    }
    pub fn close(self: *Entry, release: bool, reason: i32) void {
        if (release) self.open = false;
        self.closing = true;
        self.finishResult(reason);
        var child = self.first_child;
        while (child) |entry| : (child = entry.next_child) entry.close(release, reason);
        self.settleRetirement();
    }
    pub fn pending(self: *const Entry) ?Claim {
        if (self.claim != null or self.retired) return null;
        if (self.closing) return if (self.backend() and self.live_children == 0) .retire else null;
        return if (self.result == 0) .create else null;
    }
};
pub fn publicHandle(value: Handle) abi.GfxBufferHandle { return .{ .id = value.id, .generation = value.generation }; }
fn emptyHandle(value: abi.GfxBufferHandle) bool { return value.id == 0 and value.generation == 0 and value.reserved0 == 0; }
pub fn zeroToken(value: abi.GfxVirtualToken) bool { return value.opaque0 == 0 and value.opaque1 == 0 and value.opaque2 == 0; }
fn validHandle(value: abi.GfxBufferHandle) bool { return value.id != 0 and value.generation != 0 and value.reserved0 == 0; }
pub fn validate(input: abi.GfxVirtualRequest, instant: u64) Error!void {
    if (input.version != 1 or input.size < @sizeOf(abi.GfxVirtualRequest) or input.adapter_id == 0 or input.memory_generation == 0 or
        input.reserved0 != 0 or input.reserved1 != 0 or instant == 0 or input.deadline_ns <= instant or input.deadline_ns == std.math.maxInt(u64) or
        input.byte_length == 0 or input.byte_length & 4095 != 0) return error.Invalid;
    switch (input.kind) {
        1 => {
            if (input.flags & ~@as(u32, 1) != 0 or input.location > 1) return error.Unsupported;
            if (!emptyHandle(input.parent) or !emptyHandle(input.reference) or input.byte_offset != 0 or input.virtual_offset != 0 or
                input.alignment < 4096 or !std.math.isPowerOfTwo(input.alignment) or input.fixed_address % input.alignment != 0 or
                input.fixed_address > std.math.maxInt(u64) - input.byte_length) return error.Invalid;
        },
        2 => {
            if (input.flags != 0 or input.location != 0) return error.Unsupported;
            if (!validHandle(input.parent) or !validHandle(input.reference) or input.alignment != 0 or input.fixed_address != 0 or
                (input.byte_offset | input.virtual_offset) & 4095 != 0 or input.byte_offset > std.math.maxInt(u64) - input.byte_length or
                input.virtual_offset > std.math.maxInt(u64) - input.byte_length) return error.Invalid;
        },
        else => return error.Unsupported,
    }
}
pub const Store = struct {
    tree: Tree = .{},
    serial: u64 = 0,

    pub fn find(self: *Store, handle: Handle) Error!*Entry {
        // A tree generation is never reused, including after all nodes free.
        if (handle.id != 1 or handle.generation == 0) return error.Stale;
        const node = self.tree.getEntryFor(handle.generation).node orelse return error.Stale;
        return @fieldParentPtr("index", node);
    }
    pub fn owned(self: *Store, handle: Handle, owner: Owner) Error!*Entry {
        const entry = try self.find(handle);
        if (!entry.owner.eql(owner)) return error.WrongOwner;
        if (!entry.open) return error.Closed;
        return entry;
    }
    pub fn parent(self: *Store, owner: Owner, provider: Handle, input: abi.GfxVirtualRequest) Error!?*Entry {
        if (input.kind == 1) return null;
        const entry = try self.owned(.{ .id = input.parent.id, .generation = input.parent.generation }, owner);
        if (entry.request.kind != 1 or !entry.provider.eql(provider) or entry.request.adapter_id != input.adapter_id or
            entry.request.memory_generation != input.memory_generation) return error.Stale;
        if (entry.closing or entry.result < 0) return error.Closed;
        if (entry.result != 1 or !entry.backend() or entry.claim != null) return error.Busy;
        if (input.byte_length > entry.request.byte_length or input.virtual_offset > entry.request.byte_length - input.byte_length) return error.Invalid;
        var child = entry.first_child;
        while (child) |value| : (child = value.next_child) {
            if (!value.retired and input.virtual_offset < value.request.virtual_offset + value.request.byte_length and
                value.request.virtual_offset < input.virtual_offset + input.byte_length) return error.Busy;
        }
        return entry;
    }
    pub fn start(self: *Store, entry: *Entry, owner: Owner, provider: Handle, input: abi.GfxVirtualRequest, instant: u64) Error!void {
        try validate(input, instant);
        if (entry.handle.id != 0 or !owner.valid() or provider.id == 0 or provider.generation == 0) return error.Invalid;
        const enclosing = try self.parent(owner, provider, input);
        if (self.serial == std.math.maxInt(u64)) return error.Exhausted;
        if (enclosing) |value| if (value.live_children == std.math.maxInt(usize)) return error.Exhausted;
        self.serial += 1;
        entry.* = .{ .handle = .{ .id = 1, .generation = self.serial }, .owner = owner, .provider = provider, .request = input, .parent = enclosing };
        entry.request.size = @sizeOf(abi.GfxVirtualRequest);
        var place = self.tree.getEntryFor(self.serial);
        place.set(&entry.index);
        if (enclosing) |value| {
            entry.next_child = value.first_child;
            if (value.first_child) |first| first.previous_child = entry;
            value.first_child = entry;
            value.live_children += 1;
        }
    }
    pub fn expire(self: *Store, instant: u64) void {
        var iter = self.tree.inorderIterator();
        while (iter.next()) |node| {
            const entry: *Entry = @fieldParentPtr("index", node);
            if (entry.result == 0 and instant >= entry.request.deadline_ns) entry.close(false, abi.gfx_queue_error_wait_timeout);
        }
    }
    pub fn take(self: *Store, provider: Handle, instant: u64) ?*Entry {
        self.expire(instant);
        var iter = self.tree.inorderIterator();
        while (iter.next()) |node| {
            const entry: *Entry = @fieldParentPtr("index", node);
            if (!entry.provider.eql(provider)) continue;
            if (entry.pending()) |operation| { entry.claim = operation; return entry; }
        }
        return null;
    }
    pub fn complete(self: *Store, provider: Handle, completion: abi.GfxVirtualCompletion) Error!void {
        if (completion.version != 1 or completion.size < @sizeOf(abi.GfxVirtualCompletion) or !validHandle(completion.resource) or
            completion.operation > 1 or completion.result == 0 or completion.result > 1) return error.Invalid;
        const entry = try self.find(.{ .id = completion.resource.id, .generation = completion.resource.generation });
        if (!entry.provider.eql(provider)) return error.WrongOwner;
        const operation = entry.claim orelse return error.Closed;
        if (@intFromEnum(operation) != completion.operation) return error.Stale;
        switch (operation) {
            .create => {
                if (completion.result == 1) {
                    if (zeroToken(completion.token) or completion.address == 0 or completion.address & 4095 != 0 or
                        completion.address > std.math.maxInt(u64) - entry.request.byte_length) return error.Invalid;
                    if (entry.parent) |enclosing| {
                        if (completion.address != enclosing.address + entry.request.virtual_offset) return error.Invalid;
                    } else if (completion.address % entry.request.alignment != 0 or
                        (entry.request.fixed_address != 0 and completion.address != entry.request.fixed_address)) return error.Invalid;
                    entry.token = completion.token;
                    entry.address = completion.address;
                } else {
                    if (!zeroToken(completion.token) or completion.address != 0) return error.Invalid;
                    entry.closing = true;
                }
                entry.finishResult(completion.result);
            },
            .retire => {
                if (completion.result != 1 or completion.address != 0 or !std.meta.eql(completion.token, entry.token)) return error.Invalid;
                entry.token = .{};
            },
        }
        entry.claim = null;
        entry.settleRetirement();
    }
    pub fn closeProvider(self: *Store, provider: Handle) void {
        var iter = self.tree.inorderIterator();
        while (iter.next()) |node| {
            const entry: *Entry = @fieldParentPtr("index", node);
            if (entry.provider.eql(provider)) entry.close(false, abi.gfx_queue_error_device_lost);
        }
    }
    pub fn stopped(self: *Store, owner: Owner) void {
        var iter = self.tree.inorderIterator();
        while (iter.next()) |node| {
            const entry: *Entry = @fieldParentPtr("index", node);
            if (entry.owner.eql(owner)) entry.close(true, abi.gfx_queue_error_wait_cancelled);
        }
    }
    pub fn removable(entry: *const Entry) bool {
        return !entry.open and entry.retired and entry.first_child == null and entry.notifications == 0;
    }
    pub fn remove(self: *Store, entry: *Entry) void {
        std.debug.assert(removable(entry));
        if (entry.parent) |enclosing| {
            if (entry.previous_child) |previous| previous.next_child = entry.next_child else enclosing.first_child = entry.next_child;
            if (entry.next_child) |next| next.previous_child = entry.previous_child;
        }
        var place = self.tree.getEntryForExisting(&entry.index);
        place.set(null);
        entry.* = .{};
    }
};

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
    const input: abi.GfxVirtualRequest = .{ .kind = 1, .adapter_id = 3, .memory_generation = 9,
        .byte_length = 16384, .alignment = 4096, .deadline_ns = 100 };
    try store.start(&range, app, provider, input, 1);
    const old = range.handle;
    try t.expectError(error.WrongOwner, store.owned(old, other));
    try t.expect(store.take(provider, 2).? == &range);
    const token: abi.GfxVirtualToken = .{ .opaque0 = 9, .opaque1 = 8 };
    var completion: abi.GfxVirtualCompletion = .{ .resource = publicHandle(old), .result = 1, .token = token, .address = 0x90000000 };
    try store.complete(provider, completion);
    var request: abi.GfxVirtualRequest = .{ .kind = 2, .adapter_id = 3, .memory_generation = 9, .parent = publicHandle(old),
        .reference = .{ .id = 2, .generation = 6 }, .byte_offset = 4096, .virtual_offset = 8192, .byte_length = 4096, .deadline_ns = 100 };
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
    try t.expect(binding.claim.? == .retire and !binding.retired);
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
    completion.operation = 1; completion.address = 0;
    try store.complete(provider, completion);
    range.close(true, abi.gfx_queue_error_wait_cancelled); range.notifications = 0; store.remove(&range);
    try store.start(&range, other, provider, input, 11);
    store.closeProvider(provider);
    try t.expect(range.retired and range.result == abi.gfx_queue_error_device_lost and store.take(provider, 12) == null);
    range.close(true, abi.gfx_queue_error_wait_cancelled); range.notifications = 0; store.remove(&range);
    request.byte_offset = std.math.maxInt(u64) - 4095;
    try t.expectError(error.Invalid, validate(request, 1));
    try t.expect(store.tree.getMin() == null);
}
