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

pub const Capacity = struct {
    ram_pages: u16,
    active_pages: u16,
    pressure_level: u8,
    read_ahead_pages: u16,
};

pub fn capacityForMemory(reference_free_frames: u64, current_free_frames: u64) Capacity {
    const raw_pages = reference_free_frames / capacity_ram_divisor_frames;
    const ram_pages: u16 = @intCast(@min(
        @as(u64, max_capacity_pages),
        @max(@as(u64, min_capacity_pages), raw_pages),
    ));

    var active_pages = ram_pages;
    var pressure_level: u8 = 0;
    if (current_free_frames <= pressure_critical_free_frames) {
        active_pages = @min(ram_pages, min_capacity_pages);
        pressure_level = 3;
    } else if (current_free_frames <= pressure_low_free_frames) {
        active_pages = @min(ram_pages, 128);
        pressure_level = 2;
    } else if (current_free_frames <= pressure_moderate_free_frames) {
        active_pages = @min(ram_pages, 256);
        pressure_level = 1;
    }

    const unconstrained_window = @max(
        @as(u16, 1),
        @min(read_ahead_max_pages, active_pages / 32),
    );
    const read_ahead_pages: u16 = switch (pressure_level) {
        3 => 0,
        2 => 1,
        1 => @min(unconstrained_window, 4),
        else => unconstrained_window,
    };
    return .{
        .ram_pages = ram_pages,
        .active_pages = active_pages,
        .pressure_level = pressure_level,
        .read_ahead_pages = read_ahead_pages,
    };
}

/// Bounds one contiguous page fill by demand, the backend's sector limit and
/// the currently admitted cache capacity. A backend limit below one cache
/// page still admits one page; block.zig then performs its required splits.
pub fn fillRunPageLimit(
    requested_pages: u16,
    max_sectors_per_request: u16,
    page_sectors: u16,
    capacity_pages: u16,
) u16 {
    if (requested_pages == 0 or page_sectors == 0 or capacity_pages == 0) return 0;
    var device_pages = requested_pages;
    if (max_sectors_per_request != 0) {
        device_pages = @max(@as(u16, 1), max_sectors_per_request / page_sectors);
    }
    return @min(requested_pages, @min(device_pages, capacity_pages));
}

/// USB mass storage owns its exact transport retry below the cache. Other
/// backends retain one cache-level retry after an unsuccessful first attempt.
pub fn fillAttemptLimit(owns_transport_retry: bool) u8 {
    return if (owns_transport_retry) 1 else 2;
}

/// A contiguous media read may update only sectors that were absent when the
/// cache identities were pinned. Existing valid sectors include dirty data
/// and must never be replaced by the staging image.
pub fn fillMissingMask(valid_before: u8, readable_mask: u8) u8 {
    return readable_mask & ~valid_before;
}

/// Publication is atomic at run level: success exposes every readable sector,
/// while failure preserves the exact pre-I/O validity mask.
pub fn fillPublishedMask(valid_before: u8, readable_mask: u8, success: bool) u8 {
    return if (success) valid_before | readable_mask else valid_before;
}

pub const SequentialDecision = struct {
    next_page: u64 = 0,
    pages: u16 = 0,
    random_reset: bool = false,
};

pub const Sequential = struct {
    have_last: bool = false,
    last_end_page: u64 = 0,
    window_pages: u16 = 0,

    /// Observes a completed half-open demand range in cache-page units.
    /// Adjacent reads double the bounded window. A first observation and a
    /// random jump merely establish a new cursor: neither is sufficient
    /// evidence for speculative I/O.
    pub fn observe(self: *Sequential, first_page: u64, end_page: u64, max_window: u16) SequentialDecision {
        if (end_page <= first_page) return .{};
        const random_reset = self.have_last and first_page != self.last_end_page;
        const sequential = self.have_last and first_page == self.last_end_page;
        const demand_pages = end_page - first_page;

        if (max_window == 0) {
            self.window_pages = 0;
        } else if (sequential) {
            const grown = if (self.window_pages == 0)
                @as(u16, 1)
            else
                self.window_pages *| 2;
            self.window_pages = @min(max_window, grown);
        } else {
            _ = demand_pages;
            self.window_pages = 0;
        }
        self.have_last = true;
        self.last_end_page = end_page;
        return .{
            .next_page = end_page,
            .pages = self.window_pages,
            .random_reset = random_reset,
        };
    }

    pub fn reset(self: *Sequential) void {
        self.* = .{};
    }
};

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
    pages: u16,
    generation: u64,
};

pub const ReadAhead = struct {
    generation: u64 = 1,
    pending: bool = false,
    pending_page: u64 = 0,
    pending_pages: u16 = 0,
    inflight: bool = false,
    inflight_page: u64 = 0,
    inflight_pages: u16 = 0,
    resident_pages: u16 = 0,
    sequential: Sequential = .{},

    // Returns true when an older pending/in-flight request was superseded.
    pub fn schedule(self: *ReadAhead, page: u64, pages: u16) bool {
        if (pages == 0) return false;
        if ((self.pending and self.pending_page == page and self.pending_pages == pages) or
            (self.inflight and self.inflight_page == page and self.inflight_pages == pages)) return false;
        var cancelled = false;
        if (self.pending) cancelled = true;
        if (self.inflight and
            (self.inflight_page != page or self.inflight_pages != pages)) cancelled = true;
        if (cancelled) self.bumpGeneration();
        self.pending = true;
        self.pending_page = page;
        self.pending_pages = pages;
        return cancelled;
    }

    // A demand request supersedes queued work. An in-flight read of exactly
    // the demanded page is retained so the caller can consume it as a hit.
    pub fn demand(self: *ReadAhead, page: u64) bool {
        var cancelled = false;
        if (self.pending) {
            self.pending = false;
            self.pending_pages = 0;
            cancelled = true;
        }
        if (self.inflight and !containsPage(self.inflight_page, self.inflight_pages, page)) {
            cancelled = true;
            self.bumpGeneration();
        }
        return cancelled;
    }

    pub fn cancelAll(self: *ReadAhead) bool {
        const cancelled = self.pending or self.inflight;
        self.pending = false;
        self.pending_pages = 0;
        if (self.inflight) self.bumpGeneration();
        self.sequential.reset();
        return cancelled;
    }

    pub fn begin(self: *ReadAhead) ?ReadAheadRequest {
        if (!self.pending or self.inflight) return null;
        self.pending = false;
        self.inflight = true;
        self.inflight_page = self.pending_page;
        self.inflight_pages = self.pending_pages;
        self.pending_pages = 0;
        return .{
            .page = self.inflight_page,
            .pages = self.inflight_pages,
            .generation = self.generation,
        };
    }

    // True means the completed page still belongs to the current plan and
    // may be published as speculative residency.
    pub fn complete(self: *ReadAhead, request: ReadAheadRequest, success: bool) bool {
        const current = self.inflight and
            self.inflight_page == request.page and
            self.inflight_pages == request.pages and
            self.generation == request.generation;
        if (self.inflight and self.inflight_page == request.page) {
            self.inflight = false;
            self.inflight_pages = 0;
        }
        return current and success;
    }

    pub fn consumeResident(self: *ReadAhead) void {
        if (self.resident_pages != 0) self.resident_pages -= 1;
    }

    fn bumpGeneration(self: *ReadAhead) void {
        self.generation +%= 1;
        if (self.generation == 0) self.generation = 1;
    }

    fn containsPage(first: u64, pages: u16, page: u64) bool {
        if (pages == 0 or page < first) return false;
        return page - first < pages;
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
