const std = @import("std");

pub const max_entries: usize = 512;
pub const max_devices: usize = 8;
pub const no_index: u16 = 0xFFFF;
pub const no_device: u8 = 0xFF;

// Per-device hysteresis. The background owner starts draining at 256 KB and
// keeps making bounded progress until the device is back at 128 KB. Explicit
// flush remains the only durability barrier.
pub const dirty_high_pages: u16 = 64;
pub const dirty_low_pages: u16 = 32;

pub const Queue = enum(u8) {
    detached,
    free,
    clean,
    dirty,
    busy_clean,
    busy_dirty,
};

pub const Link = struct {
    prev: u16 = no_index,
    next: u16 = no_index,
    device: u8 = no_device,
    queue: Queue = .detached,
};

pub const Device = struct {
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

pub const ReadAheadRequest = struct {
    page: u64,
    generation: u64,
};

pub const ReadAhead = struct {
    generation: u64 = 1,
    pending: bool = false,
    pending_page: u64 = 0,
    inflight: bool = false,
    inflight_page: u64 = 0,
    resident_pages: u16 = 0,

    // Returns true when an older pending/in-flight request was superseded.
    pub fn schedule(self: *ReadAhead, page: u64) bool {
        if ((self.pending and self.pending_page == page) or
            (self.inflight and self.inflight_page == page)) return false;
        var cancelled = false;
        if (self.pending and self.pending_page != page) cancelled = true;
        if (self.inflight and self.inflight_page != page) cancelled = true;
        if (cancelled) self.bumpGeneration();
        self.pending = true;
        self.pending_page = page;
        return cancelled;
    }

    // A demand request supersedes queued work. An in-flight read of exactly
    // the demanded page is retained so the caller can consume it as a hit.
    pub fn demand(self: *ReadAhead, page: u64) bool {
        var cancelled = false;
        if (self.pending) {
            self.pending = false;
            cancelled = true;
        }
        if (self.inflight and self.inflight_page != page) {
            cancelled = true;
            self.bumpGeneration();
        }
        return cancelled;
    }

    pub fn cancelAll(self: *ReadAhead) bool {
        const cancelled = self.pending or self.inflight;
        self.pending = false;
        if (self.inflight) self.bumpGeneration();
        return cancelled;
    }

    pub fn begin(self: *ReadAhead) ?ReadAheadRequest {
        if (!self.pending or self.inflight) return null;
        self.pending = false;
        self.inflight = true;
        self.inflight_page = self.pending_page;
        return .{ .page = self.inflight_page, .generation = self.generation };
    }

    // True means the completed page still belongs to the current plan and
    // may be published as speculative residency.
    pub fn complete(self: *ReadAhead, request: ReadAheadRequest, success: bool) bool {
        const current = self.inflight and
            self.inflight_page == request.page and
            self.generation == request.generation;
        if (self.inflight and self.inflight_page == request.page) self.inflight = false;
        return current and success;
    }

    pub fn consumeResident(self: *ReadAhead) void {
        if (self.resident_pages != 0) self.resident_pages -= 1;
    }

    fn bumpGeneration(self: *ReadAhead) void {
        self.generation +%= 1;
        if (self.generation == 0) self.generation = 1;
    }
};

pub const Index = struct {
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
        var result = Index{};
        var index: usize = 0;
        while (index < max_entries) : (index += 1) {
            const prev = if (index == 0) no_index else @as(u16, @intCast(index - 1));
            const next = if (index + 1 == max_entries) no_index else @as(u16, @intCast(index + 1));
            result.links[index] = .{ .prev = prev, .next = next, .queue = .free };
        }
        result.free_head = 0;
        result.free_tail = max_entries - 1;
        result.free_count = max_entries;
        return result;
    }

    pub fn claimFree(self: *Index) ?usize {
        const raw = self.free_head;
        if (raw == no_index) return null;
        const index: usize = raw;
        const next = self.links[index].next;
        self.free_head = next;
        if (next == no_index) {
            self.free_tail = no_index;
        } else {
            self.links[@as(usize, next)].prev = no_index;
        }
        self.free_count -= 1;
        self.links[index] = .{};
        return index;
    }

    pub fn attachClean(self: *Index, index: usize, device_index: usize) bool {
        if (!validEntry(index) or !validDevice(device_index)) return false;
        if (self.links[index].queue != .detached) return false;
        const device: u8 = @intCast(device_index);
        self.links[index].device = device;
        self.links[index].queue = .clean;
        self.linkCleanTail(index, device_index);
        self.devices[device_index].entries += 1;
        self.devices[device_index].clean += 1;
        return true;
    }

    pub fn touchClean(self: *Index, index: usize) void {
        if (!validEntry(index) or self.links[index].queue != .clean) return;
        const device_index: usize = self.links[index].device;
        if (self.devices[device_index].clean_tail == @as(u16, @intCast(index))) return;
        self.unlinkClean(index, device_index);
        self.linkCleanTail(index, device_index);
    }

    pub fn markDirty(self: *Index, index: usize) bool {
        if (!validEntry(index)) return false;
        const queue = self.links[index].queue;
        if (queue == .dirty or queue == .busy_dirty) return true;
        if (queue != .clean) return false;
        const device_index: usize = self.links[index].device;
        self.unlinkClean(index, device_index);
        self.devices[device_index].clean -= 1;
        self.devices[device_index].dirty += 1;
        if (self.devices[device_index].dirty > self.devices[device_index].dirty_high_water) {
            self.devices[device_index].dirty_high_water = self.devices[device_index].dirty;
        }
        if (self.devices[device_index].dirty >= dirty_high_pages) {
            self.devices[device_index].pressure_active = true;
        }
        self.links[index].queue = .dirty;
        self.linkDirtyTail(index, device_index);
        return true;
    }

    // Transitions a fully written page back to the clean LRU. For a pinned
    // writeback the page remains detached until unpin() publishes it.
    pub fn clearDirty(self: *Index, index: usize) bool {
        if (!validEntry(index)) return false;
        const queue = self.links[index].queue;
        if (queue != .dirty and queue != .busy_dirty) return queue == .clean or queue == .busy_clean;
        const device_index: usize = self.links[index].device;
        if (queue == .dirty) self.unlinkDirty(index, device_index);
        if (queue == .busy_dirty) self.devices[device_index].busy_dirty -= 1;
        self.devices[device_index].dirty -= 1;
        self.devices[device_index].clean += 1;
        if (self.devices[device_index].dirty <= dirty_low_pages) {
            self.devices[device_index].pressure_active = false;
        }
        if (queue == .dirty) {
            self.links[index].queue = .clean;
            self.linkCleanTail(index, device_index);
        } else {
            self.links[index].queue = .busy_clean;
        }
        return true;
    }

    pub fn pin(self: *Index, index: usize) bool {
        if (!validEntry(index)) return false;
        const device_index: usize = self.links[index].device;
        switch (self.links[index].queue) {
            .clean => {
                self.unlinkClean(index, device_index);
                self.links[index].queue = .busy_clean;
            },
            .dirty => {
                self.unlinkDirty(index, device_index);
                self.links[index].queue = .busy_dirty;
                self.devices[device_index].busy_dirty += 1;
            },
            else => return false,
        }
        self.busy_count += 1;
        return true;
    }

    pub fn unpin(self: *Index, index: usize, retry_oldest: bool) bool {
        if (!validEntry(index)) return false;
        const device_index: usize = self.links[index].device;
        switch (self.links[index].queue) {
            .busy_clean => {
                self.links[index].queue = .clean;
                self.linkCleanTail(index, device_index);
            },
            .busy_dirty => {
                self.devices[device_index].busy_dirty -= 1;
                self.links[index].queue = .dirty;
                if (retry_oldest) {
                    self.linkDirtyHead(index, device_index);
                } else {
                    self.linkDirtyTail(index, device_index);
                }
            },
            else => return false,
        }
        self.busy_count -= 1;
        return true;
    }

    // Removes a clean or dirty identity without returning its slot to the
    // free queue. Replacement can immediately attach a new device identity.
    pub fn detach(self: *Index, index: usize) bool {
        if (!validEntry(index)) return false;
        const device_index: usize = self.links[index].device;
        switch (self.links[index].queue) {
            .clean => {
                self.unlinkClean(index, device_index);
                self.devices[device_index].clean -= 1;
            },
            .dirty => {
                self.unlinkDirty(index, device_index);
                self.devices[device_index].dirty -= 1;
                if (self.devices[device_index].dirty <= dirty_low_pages) {
                    self.devices[device_index].pressure_active = false;
                }
            },
            .detached => return true,
            else => return false,
        }
        self.devices[device_index].entries -= 1;
        self.links[index] = .{};
        return true;
    }

    pub fn release(self: *Index, index: usize) bool {
        if (!validEntry(index)) return false;
        if (self.links[index].queue != .detached and !self.detach(index)) return false;
        self.links[index] = .{
            .prev = self.free_tail,
            .queue = .free,
        };
        if (self.free_tail == no_index) {
            self.free_head = @intCast(index);
        } else {
            self.links[@as(usize, self.free_tail)].next = @intCast(index);
        }
        self.free_tail = @intCast(index);
        self.free_count += 1;
        return true;
    }

    pub fn cleanVictim(self: *Index, preferred_device: ?usize) ?usize {
        if (preferred_device) |device_index| {
            if (validDevice(device_index)) {
                self.clean_device_probes +%= 1;
                if (self.devices[device_index].clean_head != no_index) {
                    return self.devices[device_index].clean_head;
                }
            }
        }
        var probes: usize = 0;
        while (probes < max_devices) : (probes += 1) {
            const device_index = (@as(usize, self.clean_device_cursor) + probes) % max_devices;
            self.clean_device_probes +%= 1;
            const head = self.devices[device_index].clean_head;
            if (head == no_index) continue;
            self.clean_device_cursor = @intCast((device_index + 1) % max_devices);
            return head;
        }
        return null;
    }

    pub fn dirtyHead(self: *const Index, device_index: usize) ?usize {
        if (!validDevice(device_index)) return null;
        const head = self.devices[device_index].dirty_head;
        return if (head == no_index) null else head;
    }

    pub fn nextDirty(self: *const Index, index: usize) ?usize {
        if (!validEntry(index) or self.links[index].queue != .dirty) return null;
        const next = self.links[index].next;
        return if (next == no_index) null else next;
    }

    pub fn nextDirtyDevice(self: *Index, pressure_only: bool) ?usize {
        var probes: usize = 0;
        while (probes < max_devices) : (probes += 1) {
            const device_index = (@as(usize, self.dirty_device_cursor) + probes) % max_devices;
            self.dirty_device_probes +%= 1;
            const device = self.devices[device_index];
            if (device.dirty_head == no_index or (pressure_only and !device.pressure_active)) continue;
            self.dirty_device_cursor = @intCast((device_index + 1) % max_devices);
            return device_index;
        }
        return null;
    }

    pub fn entryCount(self: *const Index) u16 {
        return @as(u16, @intCast(max_entries)) - self.free_count;
    }

    pub fn dirtyCount(self: *const Index) u16 {
        var total: u16 = 0;
        for (self.devices) |device| total += device.dirty;
        return total;
    }

    pub fn cleanCount(self: *const Index) u16 {
        var total: u16 = 0;
        for (self.devices) |device| total += device.clean;
        return total;
    }

    fn unlinkClean(self: *Index, index: usize, device_index: usize) void {
        self.unlinkFromList(index, &self.devices[device_index].clean_head, &self.devices[device_index].clean_tail);
    }

    fn unlinkDirty(self: *Index, index: usize, device_index: usize) void {
        self.unlinkFromList(index, &self.devices[device_index].dirty_head, &self.devices[device_index].dirty_tail);
    }

    fn unlinkFromList(self: *Index, index: usize, head: *u16, tail: *u16) void {
        const prev = self.links[index].prev;
        const next = self.links[index].next;
        if (prev == no_index) head.* = next else self.links[@as(usize, prev)].next = next;
        if (next == no_index) tail.* = prev else self.links[@as(usize, next)].prev = prev;
        self.links[index].prev = no_index;
        self.links[index].next = no_index;
    }

    fn linkCleanTail(self: *Index, index: usize, device_index: usize) void {
        self.linkTail(index, &self.devices[device_index].clean_head, &self.devices[device_index].clean_tail);
    }

    fn linkDirtyTail(self: *Index, index: usize, device_index: usize) void {
        self.linkTail(index, &self.devices[device_index].dirty_head, &self.devices[device_index].dirty_tail);
    }

    fn linkDirtyHead(self: *Index, index: usize, device_index: usize) void {
        const old_head = self.devices[device_index].dirty_head;
        self.links[index].prev = no_index;
        self.links[index].next = old_head;
        if (old_head == no_index) {
            self.devices[device_index].dirty_tail = @intCast(index);
        } else {
            self.links[@as(usize, old_head)].prev = @intCast(index);
        }
        self.devices[device_index].dirty_head = @intCast(index);
    }

    fn linkTail(self: *Index, index: usize, head: *u16, tail: *u16) void {
        const old_tail = tail.*;
        self.links[index].prev = old_tail;
        self.links[index].next = no_index;
        if (old_tail == no_index) head.* = @intCast(index) else self.links[@as(usize, old_tail)].next = @intCast(index);
        tail.* = @intCast(index);
    }
};

pub fn ageDue(now: u64, dirty_since: u64, max_age: u64) bool {
    if (dirty_since == 0) return false;
    return now >= dirty_since and now - dirty_since >= max_age;
}

fn validEntry(index: usize) bool {
    return index < max_entries;
}

fn validDevice(device_index: usize) bool {
    return device_index < max_devices;
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
    try std.testing.expect(!state.schedule(80));
    const first = state.begin().?;
    try std.testing.expectEqual(@as(u64, 80), first.page);
    try std.testing.expect(state.schedule(160));
    try std.testing.expect(!state.complete(first, true));
    const second = state.begin().?;
    try std.testing.expectEqual(@as(u64, 160), second.page);
    try std.testing.expect(state.demand(24));
    try std.testing.expect(!state.complete(second, true));

    try std.testing.expect(!state.schedule(240));
    const matching = state.begin().?;
    try std.testing.expect(!state.demand(240));
    try std.testing.expect(state.complete(matching, true));
    state.resident_pages = 1;
    state.consumeResident();
    try std.testing.expectEqual(@as(u16, 0), state.resident_pages);
}
