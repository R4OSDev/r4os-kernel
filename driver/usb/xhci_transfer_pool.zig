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

pub const Entry = struct {
    generation: u16 = 0,
    state: State = .free,
    kind: Kind = .control,
    match: event_router.Match = event_router.transferMatch(0, 0, 0),
    timeout_token: u64 = 0,
    completion: event_router.Event = .{},
};

pub const Snapshot = struct {
    active: u32 = 0,
    high_water: u32 = 0,
    submitted: u64 = 0,
    completed: u64 = 0,
    timed_out: u64 = 0,
    cancelled: u64 = 0,
    failed: u64 = 0,
    stale_handles: u64 = 0,
};

pub const Pool = struct {
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
        self.* = .{};
    }

    pub fn begin(self: *Pool, kind: Kind, match: event_router.Match, timeout_token: u64) ?u32 {
        const index = self.freeIndex() orelse return null;
        const generation = self.takeGeneration();
        self.entries[index] = .{
            .generation = generation,
            .state = .submitted,
            .kind = kind,
            .match = match,
            .timeout_token = timeout_token,
        };
        self.active += 1;
        self.submitted +%= 1;
        if (self.active > self.high_water) self.high_water = self.active;
        return encodeHandle(index, generation);
    }

    pub fn owns(self: *const Pool, event: event_router.Event) bool {
        for (self.entries) |entry| {
            if (!isLive(entry.state)) continue;
            if (event_router.matches(event, entry.match)) return true;
        }
        return false;
    }

    pub fn matchingHandle(self: *const Pool, event: event_router.Event) ?u32 {
        for (self.entries, 0..) |entry, index| {
            if (!isLive(entry.state)) continue;
            if (!event_router.matches(event, entry.match)) continue;
            return encodeHandle(index, entry.generation);
        }
        return null;
    }

    pub fn complete(self: *Pool, handle: u32, event: event_router.Event) bool {
        const entry = self.entryForHandle(handle) orelse return false;
        if (entry.state != .submitted or !event_router.matches(event, entry.match)) return false;
        entry.completion = event;
        entry.state = if (event.code == 1 or event.code == 13) .completed else .failed;
        self.active -= 1;
        if (entry.state == .completed) {
            self.completed +%= 1;
        } else {
            self.failed +%= 1;
        }
        return true;
    }

    pub fn markTimeout(self: *Pool, handle: u32) bool {
        const entry = self.entryForHandle(handle) orelse return false;
        if (entry.state != .submitted) return false;
        entry.state = .timed_out;
        self.active -= 1;
        self.timed_out +%= 1;
        return true;
    }

    pub fn cancel(self: *Pool, handle: u32) bool {
        const entry = self.entryForHandle(handle) orelse return false;
        if (entry.state != .submitted and entry.state != .timed_out) return false;
        if (entry.state == .submitted) self.active -= 1;
        entry.state = .cancelled;
        self.cancelled +%= 1;
        return true;
    }

    pub fn release(self: *Pool, handle: u32) bool {
        const entry = self.entryForHandle(handle) orelse return false;
        if (entry.state == .submitted) return false;
        entry.* = .{};
        return true;
    }

    pub fn matchForHandle(self: *const Pool, handle: u32) ?event_router.Match {
        const decoded = decodeHandle(handle) orelse return null;
        const entry = &self.entries[decoded.index];
        if (entry.generation != decoded.generation or entry.state == .free) return null;
        return entry.match;
    }

    pub fn stateForHandle(self: *const Pool, handle: u32) ?State {
        const decoded = decodeHandle(handle) orelse return null;
        const entry = &self.entries[decoded.index];
        if (entry.generation != decoded.generation or entry.state == .free) return null;
        return entry.state;
    }

    pub fn timedOutHandle(self: *const Pool, slot_id: u8, endpoint_id: u8) ?u32 {
        for (self.entries, 0..) |entry, index| {
            if (entry.state != .timed_out) continue;
            const transfer = switch (entry.match) {
                .transfer => |value| value,
                .command => continue,
            };
            if (transfer.slot_id == slot_id and transfer.endpoint_id == endpoint_id) {
                return encodeHandle(index, entry.generation);
            }
        }
        return null;
    }

    pub fn cancelEndpoint(self: *Pool, slot_id: u8, endpoint_id: u8) u32 {
        var count: u32 = 0;
        for (&self.entries) |*entry| {
            if (entry.state == .free) continue;
            const transfer = switch (entry.match) {
                .transfer => |value| value,
                .command => continue,
            };
            if (transfer.slot_id != slot_id or transfer.endpoint_id != endpoint_id) continue;
            if (entry.state == .submitted) self.active -= 1;
            if (entry.state == .submitted or entry.state == .timed_out) {
                entry.state = .cancelled;
                self.cancelled +%= 1;
            }
            entry.* = .{};
            count += 1;
        }
        return count;
    }

    pub fn purgeSlot(self: *Pool, slot_id: u8) u32 {
        var count: u32 = 0;
        for (&self.entries) |*entry| {
            if (entry.state == .free) continue;
            const transfer = switch (entry.match) {
                .transfer => |value| value,
                .command => continue,
            };
            if (transfer.slot_id != slot_id) continue;
            if (entry.state == .submitted) self.active -= 1;
            if (entry.state == .submitted or entry.state == .timed_out) self.cancelled +%= 1;
            entry.* = .{};
            count += 1;
        }
        return count;
    }

    pub fn snapshot(self: *const Pool) Snapshot {
        return .{
            .active = self.active,
            .high_water = self.high_water,
            .submitted = self.submitted,
            .completed = self.completed,
            .timed_out = self.timed_out,
            .cancelled = self.cancelled,
            .failed = self.failed,
            .stale_handles = self.stale_handles,
        };
    }

    fn freeIndex(self: *const Pool) ?usize {
        for (self.entries, 0..) |entry, index| {
            if (entry.state == .free) return index;
        }
        return null;
    }

    fn takeGeneration(self: *Pool) u16 {
        const generation = self.next_generation;
        self.next_generation +%= 1;
        if (self.next_generation == 0) self.next_generation = 1;
        return generation;
    }

    fn entryForHandle(self: *Pool, handle: u32) ?*Entry {
        const decoded = decodeHandle(handle) orelse {
            self.stale_handles +%= 1;
            return null;
        };
        const entry = &self.entries[decoded.index];
        if (entry.generation != decoded.generation or entry.state == .free) {
            self.stale_handles +%= 1;
            return null;
        }
        return entry;
    }
};

const DecodedHandle = struct {
    index: usize,
    generation: u16,
};

fn encodeHandle(index: usize, generation: u16) u32 {
    return (@as(u32, generation) << 8) | @as(u32, @intCast(index + 1));
}

fn decodeHandle(handle: u32) ?DecodedHandle {
    const raw_index = handle & 0xff;
    const generation: u16 = @truncate(handle >> 8);
    if (raw_index == 0 or raw_index > MAX_TRANSFERS or generation == 0) return null;
    return .{ .index = @intCast(raw_index - 1), .generation = generation };
}

fn isLive(state: State) bool {
    return state == .submitted or state == .timed_out;
}

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
