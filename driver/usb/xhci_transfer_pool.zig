comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native generation-bound transfer owner; compatibility calls only.
extern fn r4os_rust_usb_pool_reset(*Pool) void;
extern fn r4os_rust_usb_pool_begin(*Pool, u8, *const event_router.NativeMatch, u64) u32;
extern fn r4os_rust_usb_pool_owns(*const Pool, *const event_router.Event) bool;
extern fn r4os_rust_usb_pool_matching(*const Pool, *const event_router.Event) u32;
extern fn r4os_rust_usb_pool_complete(*Pool, u32, *const event_router.Event) bool;
extern fn r4os_rust_usb_pool_timeout(*Pool, u32) bool;
extern fn r4os_rust_usb_pool_cancel(*Pool, u32) bool;
extern fn r4os_rust_usb_pool_release(*Pool, u32) bool;
extern fn r4os_rust_usb_pool_match(*const Pool, u32, *event_router.NativeMatch) bool;
extern fn r4os_rust_usb_pool_state(*const Pool, u32) u8;
extern fn r4os_rust_usb_pool_timed_out(*const Pool, u8, u8) u32;
extern fn r4os_rust_usb_pool_cancel_endpoint(*Pool, u8, u8) u32;
extern fn r4os_rust_usb_pool_purge_slot(*Pool, u8) u32;
extern fn r4os_rust_usb_pool_snapshot(*const Pool, *Snapshot) void;
const event_router = @import("xhci_event_router.zig");

pub const MAX_TRANSFERS: usize = 32;

pub const Kind = enum(u8) {
    control,
    bulk_in,
    bulk_out,
    interrupt_in,
};

pub const State = enum(u8) {
    free,
    submitted,
    completed,
    timed_out,
    cancelled,
    failed,
};

pub const Entry = extern struct {
    generation: u16 = 0,
    state: State = .free,
    kind: Kind = .control,
    match: event_router.NativeMatch = .{},
    timeout_token: u64 = 0,
    completion: event_router.Event = .{},
};

pub const Snapshot = extern struct {
    active: u32 = 0,
    high_water: u32 = 0,
    submitted: u64 = 0,
    completed: u64 = 0,
    timed_out: u64 = 0,
    cancelled: u64 = 0,
    failed: u64 = 0,
    stale_handles: u64 = 0,
};

pub const Pool = extern struct {
    entries: [MAX_TRANSFERS]Entry = .{Entry{}} ** MAX_TRANSFERS,
    next_generation: u16 = 1,
    active: u32 = 0,
    high_water: u32 = 0,
    submitted: u64 = 0,
    completed: u64 = 0,
    timed_out: u64 = 0,
    cancelled: u64 = 0,
    failed: u64 = 0,
    stale_handles: u64 = 0,

    pub fn init() Pool {
        return .{};
    }

    pub fn reset(self: *Pool) void {
        r4os_rust_usb_pool_reset(self);
    }

    pub fn begin(self: *Pool, kind: Kind, match: event_router.Match, timeout_token: u64) ?u32 {
        const wire = event_router.toNative(match);
        const handle = r4os_rust_usb_pool_begin(self, @intFromEnum(kind), &wire, timeout_token);
        return if (handle == 0) null else handle;
    }

    pub fn owns(self: *const Pool, event: event_router.Event) bool {
        return r4os_rust_usb_pool_owns(self, &event);
    }

    pub fn matchingHandle(self: *const Pool, event: event_router.Event) ?u32 {
        const handle = r4os_rust_usb_pool_matching(self, &event);
        return if (handle == 0) null else handle;
    }

    pub fn complete(self: *Pool, handle: u32, event: event_router.Event) bool {
        return r4os_rust_usb_pool_complete(self, handle, &event);
    }

    pub fn markTimeout(self: *Pool, handle: u32) bool {
        return r4os_rust_usb_pool_timeout(self, handle);
    }

    pub fn cancel(self: *Pool, handle: u32) bool {
        return r4os_rust_usb_pool_cancel(self, handle);
    }

    pub fn release(self: *Pool, handle: u32) bool {
        return r4os_rust_usb_pool_release(self, handle);
    }

    pub fn matchForHandle(self: *const Pool, handle: u32) ?event_router.Match {
        var out: event_router.NativeMatch = undefined;
        return if (r4os_rust_usb_pool_match(self, handle, &out)) event_router.fromNative(out) else null;
    }

    pub fn stateForHandle(self: *const Pool, handle: u32) ?State {
        const value = r4os_rust_usb_pool_state(self, handle);
        return if (value == 255) null else @enumFromInt(value);
    }

    pub fn timedOutHandle(self: *const Pool, slot_id: u8, endpoint_id: u8) ?u32 {
        const handle = r4os_rust_usb_pool_timed_out(self, slot_id, endpoint_id);
        return if (handle == 0) null else handle;
    }

    pub fn cancelEndpoint(self: *Pool, slot_id: u8, endpoint_id: u8) u32 {
        return r4os_rust_usb_pool_cancel_endpoint(self, slot_id, endpoint_id);
    }

    pub fn purgeSlot(self: *Pool, slot_id: u8) u32 {
        return r4os_rust_usb_pool_purge_slot(self, slot_id);
    }

    pub fn snapshot(self: *const Pool) Snapshot {
        var out: Snapshot = undefined;
        r4os_rust_usb_pool_snapshot(self, &out);
        return out;
    }
};


test "parallel endpoint-bound transfers complete independently" {
    const testing = @import("std").testing;
    var pool = Pool.init();
    const hid = pool.begin(.interrupt_in, event_router.transferMatch(2, 3, 0x2000), 11) orelse return error.NoHidSlot;
    const storage = pool.begin(.bulk_in, event_router.transferMatch(4, 5, 0x4000), 12) orelse return error.NoStorageSlot;
    try testing.expectEqual(@as(u32, 2), pool.snapshot().active);

    const storage_event: event_router.Event = .{ .event_type = 32, .code = 1, .slot_id = 4, .endpoint_id = 5, .parameter = 0x400f };
    try testing.expect(pool.owns(storage_event));
    try testing.expect(pool.complete(storage, storage_event));
    try testing.expectEqual(State.completed, pool.stateForHandle(storage).?);
    try testing.expectEqual(State.submitted, pool.stateForHandle(hid).?);
    try testing.expectEqual(@as(u32, 1), pool.snapshot().active);
}

test "ring-wrap TD pointers and cancellation stay generation-safe" {
    const testing = @import("std").testing;
    var pool = Pool.init();
    var pointers: [event_router.MAX_TRANSFER_TRB_POINTERS]u64 = .{0} ** event_router.MAX_TRANSFER_TRB_POINTERS;
    pointers[0] = 0x4fe0;
    pointers[1] = 0x4000;
    pointers[2] = 0x4010;
    const first = pool.begin(.bulk_out, event_router.transferTdMatch(7, 4, pointers, 3), 99) orelse return error.NoSlot;
    try testing.expect(pool.cancel(first));
    try testing.expect(pool.release(first));

    const second = pool.begin(.bulk_out, event_router.transferMatch(7, 4, 0x5000), 100) orelse return error.NoSlot;
    try testing.expect(first != second);
    try testing.expect(!pool.markTimeout(first));
    try testing.expect(pool.markTimeout(second));
    try testing.expectEqual(State.timed_out, pool.stateForHandle(second).?);
    try testing.expectEqual(@as(u64, 1), pool.snapshot().stale_handles);
}

test "hot-unplug purges only the removed slot" {
    const testing = @import("std").testing;
    var pool = Pool.init();
    _ = pool.begin(.bulk_in, event_router.transferMatch(3, 3, 0x3000), 1) orelse return error.NoSlot;
    const survivor = pool.begin(.interrupt_in, event_router.transferMatch(4, 5, 0x4000), 2) orelse return error.NoSlot;
    try testing.expectEqual(@as(u32, 1), pool.purgeSlot(3));
    try testing.expectEqual(State.submitted, pool.stateForHandle(survivor).?);
    try testing.expectEqual(@as(u32, 1), pool.snapshot().active);
}

test "failed segment completion terminates only its exact transfer" {
    const testing = @import("std").testing;
    var pool = Pool.init();
    var pointers: [event_router.MAX_TRANSFER_TRB_POINTERS]u64 = .{0} ** event_router.MAX_TRANSFER_TRB_POINTERS;
    pointers[0] = 0x9fe0;
    pointers[1] = 0x9000;
    pointers[2] = 0x9010;
    const failed = pool.begin(.bulk_in, event_router.transferTdMatch(8, 3, pointers, 3), 41) orelse return error.NoSlot;
    const survivor = pool.begin(.interrupt_in, event_router.transferMatch(9, 5, 0xa000), 42) orelse return error.NoSlot;
    const event: event_router.Event = .{
        .event_type = event_router.TRANSFER_EVENT_TYPE,
        .code = 4,
        .slot_id = 8,
        .endpoint_id = 3,
        .parameter = 0x900f,
    };
    try testing.expect(pool.complete(failed, event));
    try testing.expectEqual(State.failed, pool.stateForHandle(failed).?);
    try testing.expectEqual(State.submitted, pool.stateForHandle(survivor).?);
    try testing.expectEqual(@as(u64, 1), pool.snapshot().failed);
    try testing.expectEqual(@as(u32, 1), pool.snapshot().active);
}

comptime {
    if (@sizeOf(Entry) != 304 or @offsetOf(Entry, "match") != 8 or @offsetOf(Entry, "completion") != 280 or
        @sizeOf(Pool) != 9792 or @offsetOf(Pool, "next_generation") != 9728 or @offsetOf(Pool, "submitted") != 9744 or
        @sizeOf(Snapshot) != 56) @compileError("USB pool private C layout");
}
