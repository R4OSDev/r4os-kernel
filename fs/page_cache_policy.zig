const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

// Private C facade. Native Rust owns all cache policy transitions; the
// existing cache owner serializes these bounded calls without callbacks.
extern fn r4os_rust_fs_cache_capacity(u64, u64, *Capacity) void;
extern fn r4os_rust_fs_cache_fill_limit(u16, u16, u16, u16) u16;
extern fn r4os_rust_fs_cache_fill_attempts(bool) u8;
extern fn r4os_rust_fs_cache_fill_mask(u8, u8, u8, bool) u8;
extern fn r4os_rust_fs_cache_age_due(u64, u64, u64) bool;
extern fn r4os_rust_fs_cache_sequential(*Sequential, u8, u64, u64, u16, ?*SequentialDecision) void;
extern fn r4os_rust_fs_cache_readahead(*ReadAhead, u8, u64, u16, ?*const ReadAheadRequest, bool, ?*ReadAheadRequest) bool;
extern fn r4os_rust_fs_cache_index(*Index, u8, usize, usize, bool) usize;

// Exactly one productive index, natively initialized before page_cache.init.
pub extern var R4OS_FS_CACHE_INDEX: Index;

pub const max_entries: usize = 512;
pub const max_devices: usize = 8;
pub const no_index: u16 = 0xFFFF;
pub const no_device: u8 = 0xFF;

// Per-device hysteresis. The background owner starts draining at 256 KB and
// keeps making bounded progress until the device is back at 128 KB. Explicit
// flush remains the only durability barrier.
pub const dirty_high_pages: u16 = 64;
pub const dirty_low_pages: u16 = 32;

// Metadata stays fixed and bounded, while committed payload capacity follows
// the RAM available at boot and contracts under current PMM pressure. The
// thresholds deliberately leave substantially more free memory than the
// cache can consume itself.
pub const min_capacity_pages: u16 = 64;
pub const max_capacity_pages: u16 = @intCast(max_entries);
pub const capacity_ram_divisor_frames: u64 = 128;
pub const pressure_critical_free_frames: u64 = 1024;
pub const pressure_low_free_frames: u64 = 4096;
pub const pressure_moderate_free_frames: u64 = 8192;
pub const read_ahead_max_pages: u16 = 16;


pub const Capacity = extern struct {
    ram_pages: u16,
    active_pages: u16,
    pressure_level: u8,
    read_ahead_pages: u16,
};
pub fn capacityForMemory(reference_free_frames: u64, current_free_frames: u64) Capacity {
    var result: Capacity = undefined;
    r4os_rust_fs_cache_capacity(reference_free_frames, current_free_frames, &result);
    return result;
}
pub fn fillRunPageLimit(requested_pages: u16, max_sectors_per_request: u16, page_sectors: u16, capacity_pages: u16) u16 {
    return r4os_rust_fs_cache_fill_limit(requested_pages, max_sectors_per_request, page_sectors, capacity_pages);
}
pub fn fillAttemptLimit(owns_transport_retry: bool) u8 {
    return r4os_rust_fs_cache_fill_attempts(owns_transport_retry);
}
pub fn fillMissingMask(valid_before: u8, readable_mask: u8) u8 {
    return r4os_rust_fs_cache_fill_mask(0, valid_before, readable_mask, false);
}
pub fn fillPublishedMask(valid_before: u8, readable_mask: u8, success: bool) u8 {
    return r4os_rust_fs_cache_fill_mask(1, valid_before, readable_mask, success);
}
pub const SequentialDecision = extern struct {
    next_page: u64 = 0,
    pages: u16 = 0,
    random_reset: bool = false,
};
pub const Sequential = extern struct {
    have_last: bool = false,
    last_end_page: u64 = 0,
    window_pages: u16 = 0,
    pub fn observe(self: *Sequential, first_page: u64, end_page: u64, max_window: u16) SequentialDecision {
        var result: SequentialDecision = undefined;
        r4os_rust_fs_cache_sequential(self, 0, first_page, end_page, max_window, &result);
        return result;
    }
    pub fn reset(self: *Sequential) void {
        r4os_rust_fs_cache_sequential(self, 1, 0, 0, 0, null);
    }
};
pub const Queue = enum(u8) { detached, free, clean, dirty, busy_clean, busy_dirty };
pub const Link = extern struct {
    prev: u16 = no_index,
    next: u16 = no_index,
    device: u8 = no_device,
    queue: Queue = .detached,
};
pub const Device = extern struct {
    clean_head: u16 = no_index,
    clean_tail: u16 = no_index,
    dirty_head: u16 = no_index,
    dirty_tail: u16 = no_index,
    entries: u16 = 0,
    clean: u16 = 0,
    dirty: u16 = 0,
    busy_dirty: u16 = 0,
    dirty_high_water: u16 = 0,
    pressure_active: bool = false,
};
pub const ReadAheadRequest = extern struct { page: u64, pages: u16, generation: u64 };
pub const ReadAhead = extern struct {
    generation: u64 = 1,
    pending: bool = false,
    pending_page: u64 = 0,
    pending_pages: u16 = 0,
    inflight: bool = false,
    inflight_page: u64 = 0,
    inflight_pages: u16 = 0,
    resident_pages: u16 = 0,
    sequential: Sequential = .{},
    pub fn schedule(self: *ReadAhead, page: u64, pages: u16) bool {
        return r4os_rust_fs_cache_readahead(self, 0, page, pages, null, false, null);
    }
    pub fn demand(self: *ReadAhead, page: u64) bool {
        return r4os_rust_fs_cache_readahead(self, 1, page, 0, null, false, null);
    }
    pub fn cancelAll(self: *ReadAhead) bool {
        return r4os_rust_fs_cache_readahead(self, 2, 0, 0, null, false, null);
    }
    pub fn begin(self: *ReadAhead) ?ReadAheadRequest {
        var result: ReadAheadRequest = undefined;
        return if (r4os_rust_fs_cache_readahead(self, 3, 0, 0, null, false, &result)) result else null;
    }
    pub fn complete(self: *ReadAhead, request: ReadAheadRequest, success: bool) bool {
        return r4os_rust_fs_cache_readahead(self, 4, 0, 0, &request, success, null);
    }
    pub fn consumeResident(self: *ReadAhead) void {
        _ = r4os_rust_fs_cache_readahead(self, 5, 0, 0, null, false, null);
    }
};
pub const Index = extern struct {
    links: [max_entries]Link = .{Link{}} ** max_entries,
    devices: [max_devices]Device = .{Device{}} ** max_devices,
    free_head: u16 = no_index,
    free_tail: u16 = no_index,
    free_count: u16 = 0,
    busy_count: u16 = 0,
    clean_device_cursor: u8 = 0,
    dirty_device_cursor: u8 = 0,
    clean_device_probes: u64 = 0,
    dirty_device_probes: u64 = 0,
    pub fn init() Index {
        var result: Index = undefined;
        _ = r4os_rust_fs_cache_index(&result, 0, 0, 0, false);
        return result;
    }
    pub fn claimFree(self: *Index) ?usize {
        return optional(r4os_rust_fs_cache_index(self, 1, 0, 0, false));
    }
    pub fn attachClean(self: *Index, index: usize, device_index: usize) bool {
        return r4os_rust_fs_cache_index(self, 2, index, device_index, false) != 0;
    }
    pub fn touchClean(self: *Index, index: usize) void {
        _ = r4os_rust_fs_cache_index(self, 3, index, 0, false);
    }
    pub fn markDirty(self: *Index, index: usize) bool {
        return r4os_rust_fs_cache_index(self, 4, index, 0, false) != 0;
    }
    pub fn clearDirty(self: *Index, index: usize) bool {
        return r4os_rust_fs_cache_index(self, 5, index, 0, false) != 0;
    }
    pub fn pin(self: *Index, index: usize) bool {
        return r4os_rust_fs_cache_index(self, 6, index, 0, false) != 0;
    }
    pub fn unpin(self: *Index, index: usize, retry_oldest: bool) bool {
        return r4os_rust_fs_cache_index(self, 7, index, 0, retry_oldest) != 0;
    }
    pub fn detach(self: *Index, index: usize) bool {
        return r4os_rust_fs_cache_index(self, 8, index, 0, false) != 0;
    }
    pub fn release(self: *Index, index: usize) bool {
        return r4os_rust_fs_cache_index(self, 9, index, 0, false) != 0;
    }
    pub fn cleanVictim(self: *Index, preferred_device: ?usize) ?usize {
        return optional(r4os_rust_fs_cache_index(self, 10, 0, preferred_device orelse std.math.maxInt(usize), false));
    }
    pub fn dirtyHead(self: *const Index, device_index: usize) ?usize {
        return optional(r4os_rust_fs_cache_index(@constCast(self), 11, 0, device_index, false));
    }
    pub fn nextDirty(self: *const Index, index: usize) ?usize {
        return optional(r4os_rust_fs_cache_index(@constCast(self), 12, index, 0, false));
    }
    pub fn nextDirtyDevice(self: *Index, pressure_only: bool) ?usize {
        return optional(r4os_rust_fs_cache_index(self, 13, 0, 0, pressure_only));
    }
    pub fn entryCount(self: *const Index) u16 {
        return @intCast(r4os_rust_fs_cache_index(@constCast(self), 14, 0, 0, false));
    }
    pub fn dirtyCount(self: *const Index) u16 {
        return @intCast(r4os_rust_fs_cache_index(@constCast(self), 15, 0, 0, false));
    }
    pub fn cleanCount(self: *const Index) u16 {
        return @intCast(r4os_rust_fs_cache_index(@constCast(self), 16, 0, 0, false));
    }
};
fn optional(raw: usize) ?usize {
    return if (raw == std.math.maxInt(usize)) null else raw;
}
pub fn ageDue(now: u64, dirty_since: u64, max_age: u64) bool {
    return r4os_rust_fs_cache_age_due(now, dirty_since, max_age);
}
comptime {
    if (@sizeOf(Capacity) != 8 or @offsetOf(Capacity, "read_ahead_pages") != 6 or
        @sizeOf(SequentialDecision) != 16 or @sizeOf(Sequential) != 24 or
        @offsetOf(Sequential, "last_end_page") != 8 or @sizeOf(Link) != 6 or
        @sizeOf(Device) != 20 or @sizeOf(ReadAheadRequest) != 24 or
        @sizeOf(ReadAhead) != 72 or @offsetOf(ReadAhead, "sequential") != 48 or
        @sizeOf(Index) != 3264 or @offsetOf(Index, "devices") != 3072 or
        @offsetOf(Index, "clean_device_probes") != 3248)
        @compileError("Private native filesystem cache policy layout changed");
}

test "free and per-device clean selection stay indexed" {
    var index = Index.init();
    const a = index.claimFree().?;
    const b = index.claimFree().?;
    const c = index.claimFree().?;
    try std.testing.expect(index.attachClean(a, 0));
    try std.testing.expect(index.attachClean(b, 1));
    try std.testing.expect(index.attachClean(c, 0));
    try std.testing.expectEqual(a, index.cleanVictim(0).?);
    index.touchClean(a);
    try std.testing.expectEqual(c, index.cleanVictim(0).?);
    try std.testing.expectEqual(@as(u16, max_entries - 3), index.free_count);
    try std.testing.expect(index.detach(c));
    try std.testing.expect(index.release(c));
    try std.testing.expectEqual(@as(u16, max_entries - 2), index.free_count);
}

test "dirty pressure uses hysteresis and failed writeback keeps the oldest page" {
    var index = Index.init();
    var slots: [dirty_high_pages]usize = undefined;
    for (&slots, 0..) |*slot, n| {
        slot.* = index.claimFree().?;
        try std.testing.expect(index.attachClean(slot.*, 2));
        try std.testing.expect(index.markDirty(slot.*));
        try std.testing.expectEqual(@as(usize, n + 1), index.devices[2].dirty);
    }
    try std.testing.expect(index.devices[2].pressure_active);
    const oldest = index.dirtyHead(2).?;
    try std.testing.expect(index.pin(oldest));
    try std.testing.expectEqual(@as(u16, 1), index.devices[2].busy_dirty);
    try std.testing.expect(index.unpin(oldest, true));
    try std.testing.expectEqual(@as(u16, 0), index.devices[2].busy_dirty);
    try std.testing.expectEqual(oldest, index.dirtyHead(2).?);

    var drained: usize = 0;
    while (index.devices[2].dirty > dirty_low_pages) : (drained += 1) {
        const page = index.dirtyHead(2).?;
        try std.testing.expect(index.pin(page));
        try std.testing.expect(index.clearDirty(page));
        try std.testing.expectEqual(@as(u16, 0), index.devices[2].busy_dirty);
        try std.testing.expect(index.unpin(page, false));
    }
    try std.testing.expectEqual(@as(usize, dirty_high_pages - dirty_low_pages), drained);
    try std.testing.expect(!index.devices[2].pressure_active);
}

test "round-robin dirty devices prevent one device from monopolizing drains" {
    var index = Index.init();
    for (0..3) |device| {
        const slot = index.claimFree().?;
        try std.testing.expect(index.attachClean(slot, device));
        try std.testing.expect(index.markDirty(slot));
    }
    try std.testing.expectEqual(@as(usize, 0), index.nextDirtyDevice(false).?);
    try std.testing.expectEqual(@as(usize, 1), index.nextDirtyDevice(false).?);
    try std.testing.expectEqual(@as(usize, 2), index.nextDirtyDevice(false).?);
    try std.testing.expectEqual(@as(usize, 0), index.nextDirtyDevice(false).?);
    try std.testing.expect(index.dirty_device_probes <= max_devices * 4);
}

test "shutdown-style drain preserves per-device FIFO and empties all devices" {
    var index = Index.init();
    var expected: [2][3]usize = undefined;
    for (0..2) |device| {
        for (0..3) |n| {
            const slot = index.claimFree().?;
            expected[device][n] = slot;
            try std.testing.expect(index.attachClean(slot, device));
            try std.testing.expect(index.markDirty(slot));
        }
    }
    var seen: [2]usize = .{ 0, 0 };
    while (index.nextDirtyDevice(false)) |device| {
        const page = index.dirtyHead(device).?;
        try std.testing.expectEqual(expected[device][seen[device]], page);
        seen[device] += 1;
        try std.testing.expect(index.pin(page));
        try std.testing.expect(index.clearDirty(page));
        try std.testing.expect(index.unpin(page, false));
    }
    try std.testing.expectEqual([2]usize{ 3, 3 }, seen);
    try std.testing.expectEqual(@as(u16, 0), index.dirtyCount());
}

test "dirty age threshold is monotone and wrap-safe by refusal" {
    try std.testing.expect(!ageDue(99, 100, 10));
    try std.testing.expect(!ageDue(109, 100, 10));
    try std.testing.expect(ageDue(110, 100, 10));
    try std.testing.expect(!ageDue(1000, 0, 10));
}

test "read-ahead is replaceable, demand-cancellable, and generation bound" {
    var state = ReadAhead{};
    try std.testing.expect(!state.schedule(80, 2));
    const first = state.begin().?;
    try std.testing.expectEqual(@as(u64, 80), first.page);
    try std.testing.expectEqual(@as(u16, 2), first.pages);
    try std.testing.expect(state.schedule(160, 4));
    try std.testing.expect(!state.complete(first, true));
    const second = state.begin().?;
    try std.testing.expectEqual(@as(u64, 160), second.page);
    try std.testing.expect(state.demand(24));
    try std.testing.expect(!state.complete(second, true));

    try std.testing.expect(!state.schedule(240, 3));
    const matching = state.begin().?;
    try std.testing.expect(!state.demand(242));
    try std.testing.expect(state.complete(matching, true));
    state.resident_pages = 1;
    state.consumeResident();
    try std.testing.expectEqual(@as(u16, 0), state.resident_pages);
}

test "RAM and pressure bound active capacity and speculation" {
    const abundant = capacityForMemory(256 * 1024, 200 * 1024);
    try std.testing.expectEqual(max_capacity_pages, abundant.ram_pages);
    try std.testing.expectEqual(max_capacity_pages, abundant.active_pages);
    try std.testing.expectEqual(read_ahead_max_pages, abundant.read_ahead_pages);

    const moderate = capacityForMemory(256 * 1024, pressure_moderate_free_frames);
    try std.testing.expectEqual(@as(u16, 256), moderate.active_pages);
    try std.testing.expectEqual(@as(u8, 1), moderate.pressure_level);
    try std.testing.expectEqual(@as(u16, 4), moderate.read_ahead_pages);

    const critical = capacityForMemory(256 * 1024, pressure_critical_free_frames);
    try std.testing.expectEqual(min_capacity_pages, critical.active_pages);
    try std.testing.expectEqual(@as(u8, 3), critical.pressure_level);
    try std.testing.expectEqual(@as(u16, 0), critical.read_ahead_pages);

    const small = capacityForMemory(4096, 4096);
    try std.testing.expectEqual(min_capacity_pages, small.ram_pages);
    try std.testing.expectEqual(min_capacity_pages, small.active_pages);
}

test "backend and cache budgets bound contiguous fill runs" {
    try std.testing.expectEqual(@as(u16, 16), fillRunPageLimit(32, 128, 8, 512));
    try std.testing.expectEqual(@as(u16, 32), fillRunPageLimit(32, 0, 8, 512));
    try std.testing.expectEqual(@as(u16, 1), fillRunPageLimit(32, 4, 8, 512));
    try std.testing.expectEqual(@as(u16, 6), fillRunPageLimit(32, 128, 8, 6));
    try std.testing.expectEqual(@as(u16, 0), fillRunPageLimit(0, 128, 8, 512));
}

test "fill publication preserves dirty-valid sectors and rolls back failure" {
    const valid_before: u8 = 0b0010_0101;
    const readable: u8 = 0b1111_1111;
    try std.testing.expectEqual(@as(u8, 0b1101_1010), fillMissingMask(valid_before, readable));
    try std.testing.expectEqual(readable, fillPublishedMask(valid_before, readable, true));
    try std.testing.expectEqual(valid_before, fillPublishedMask(valid_before, readable, false));
    try std.testing.expectEqual(@as(u8, 1), fillAttemptLimit(true));
    try std.testing.expectEqual(@as(u8, 2), fillAttemptLimit(false));
}

test "sequential window grows and random access resets it" {
    var sequence = Sequential{};
    const bulk = sequence.observe(0, 2, 16);
    try std.testing.expectEqual(@as(u16, 0), bulk.pages);
    try std.testing.expect(!bulk.random_reset);
    const adjacent = sequence.observe(2, 4, 16);
    try std.testing.expectEqual(@as(u16, 1), adjacent.pages);
    const grown = sequence.observe(4, 6, 16);
    try std.testing.expectEqual(@as(u16, 2), grown.pages);
    const random = sequence.observe(30, 31, 16);
    try std.testing.expect(random.random_reset);
    try std.testing.expectEqual(@as(u16, 0), random.pages);
    const resumed = sequence.observe(31, 32, 2);
    try std.testing.expectEqual(@as(u16, 1), resumed.pages);
    const bounded = sequence.observe(32, 33, 2);
    try std.testing.expectEqual(@as(u16, 2), bounded.pages);
}
