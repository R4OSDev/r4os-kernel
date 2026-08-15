const boot_info = @import("../bootloader/boot_info.zig");
const k = @import("../kernel/log.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");

pub const MAX_BLOCKS: usize = 8192;
pub const KIND_COUNT: usize = 14;
pub const OWNER_COUNT: usize = 8;
pub const STATUS_COUNT: usize = 7;

pub const Kind = enum(u8) {
    boot = 0,
    kernel = 1,
    kernel_heap = 2,
    page_table = 3,
    virtual_range = 4,
    program_image = 5,
    app_heap = 6,
    app_stack = 7,
    dma = 8,
    mmio = 9,
    framebuffer = 10,
    reserved = 11,
    free = 12,
    unknown = 13,
};

pub const Owner = enum(u8) {
    kernel = 0,
    driver = 1,
    protocol = 2,
    r4x_instance = 3,
    task = 4,
    device = 5,
    bootloader = 6,
    system = 7,
};

pub const Status = enum(u8) {
    free = 0,
    reserved = 1,
    committed = 2,
    guard = 3,
    mapped = 4,
    released = 5,
    @"error" = 6,
};

pub const Error = error{
    NotInitialized,
    TableFull,
    EmptyRange,
    Overlap,
    NotFound,
    NotFree,
    InvalidBytes,
    InvalidRange,
    Overflow,
};

pub const MemoryBlock = struct {
    slot_used: bool = false,
    id: u32 = 0,
    kind: Kind = .unknown,
    owner: Owner = .system,
    owner_id: u64 = 0,
    status: Status = .free,
    name: []const u8 = "",
    phys_base: u64 = 0,
    phys_len: u64 = 0,
    virt_base: u64 = 0,
    virt_len: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,

    pub fn active(self: MemoryBlock) bool {
        return self.slot_used and self.status != .released;
    }
};

pub const RegisterRequest = struct {
    kind: Kind = .unknown,
    owner: Owner = .system,
    owner_id: u64 = 0,
    status: Status = .reserved,
    name: []const u8 = "",
    phys_base: u64 = 0,
    phys_len: u64 = 0,
    virt_base: u64 = 0,
    virt_len: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
};

pub const UpdateRequest = struct {
    kind: ?Kind = null,
    owner: ?Owner = null,
    owner_id: ?u64 = null,
    status: ?Status = null,
    name: ?[]const u8 = null,
    reserved_bytes: ?u64 = null,
    committed_bytes: ?u64 = null,
};

pub const SnapshotResult = struct {
    copied: usize = 0,
    total: usize = 0,
    truncated: bool = false,
};

pub const Summary = struct {
    total_slots_used: u64 = 0,
    active_blocks: u64 = 0,
    released_blocks: u64 = 0,
    error_blocks: u64 = 0,
    physical_bytes: u64 = 0,
    virtual_bytes: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    free_physical_bytes: u64 = 0,
    largest_free_phys_base: u64 = 0,
    largest_free_phys_len: u64 = 0,
    by_kind: [KIND_COUNT]u64 = .{0} ** KIND_COUNT,
    by_owner: [OWNER_COUNT]u64 = .{0} ** OWNER_COUNT,
    by_status: [STATUS_COUNT]u64 = .{0} ** STATUS_COUNT,
    overflow: bool = false,
};

// Complete, already validated metadata mutation for one physical release.
// The plan owns concrete destination slots and IDs, so applying it after a
// PTE unmap performs no searches, allocations or fallible arithmetic.
pub const PhysicalReleasePlan = struct {
    target_index: usize,
    target_released: MemoryBlock,
    write_indices: [3]usize = .{0} ** 3,
    write_entries: [3]MemoryBlock = .{MemoryBlock{}} ** 3,
    write_count: u8 = 0,
    merge_primary_index: ?usize = null,
    merge_primary_entry: MemoryBlock = .{},
    merge_secondary_index: ?usize = null,
    merge_secondary_released: MemoryBlock = .{},
    next_id_after: u32,
    irq_flags: u64 = 0,
    active: bool = false,
};

const Table = struct {
    entries: []MemoryBlock,
    next_id: u32 = 1,

    fn reset(self: *Table) void {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) self.entries[i] = .{};
        self.next_id = 1;
    }

    fn register(self: *Table, req: RegisterRequest) Error!u32 {
        validateRequest(req) catch |err| return err;
        if (self.overlapsActive(req.phys_base, req.phys_len, true, 0) or
            self.overlapsActive(req.virt_base, req.virt_len, false, 0))
        {
            return Error.Overlap;
        }

        const slot = self.freeSlot() orelse return Error.TableFull;
        const id = self.allocId() catch |err| return err;
        self.entries[slot] = .{
            .slot_used = true,
            .id = id,
            .kind = req.kind,
            .owner = req.owner,
            .owner_id = req.owner_id,
            .status = req.status,
            .name = req.name,
            .phys_base = req.phys_base,
            .phys_len = req.phys_len,
            .virt_base = req.virt_base,
            .virt_len = req.virt_len,
            .reserved_bytes = req.reserved_bytes,
            .committed_bytes = req.committed_bytes,
        };
        return id;
    }

    fn claimPhysicalRange(
        self: *Table,
        base: u64,
        len: u64,
        kind: Kind,
        owner: Owner,
        owner_id: u64,
        name: []const u8,
    ) Error!u32 {
        if (len == 0 or checkedEnd(base, len) == null) return Error.EmptyRange;

        const free_index = self.containingFreePhysical(base, len) orelse return Error.NotFree;
        const free = self.entries[free_index];
        const free_end = checkedEnd(free.phys_base, free.phys_len) orelse return Error.Overflow;
        const claim_end = checkedEnd(base, len) orelse return Error.Overflow;
        const before_len = base - free.phys_base;
        const after_len = free_end - claim_end;

        // Claim is a private allocation transaction. PMM callers return the
        // frame when this function fails, so every touched block slot and the
        // ID cursor must also roll back exactly on a late split/merge error.
        var journal = PhysicalMutationJournal.init(self.next_id);
        try journal.remember(self.entries, free_index);
        errdefer journal.rollback(self);

        self.entries[free_index].status = .released;

        if (before_len != 0) {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
            _ = try self.register(.{
                .kind = .free,
                .owner = .system,
                .status = .free,
                .name = "free",
                .phys_base = free.phys_base,
                .phys_len = before_len,
            });
        }

        const claim_request = RegisterRequest{
            .kind = kind,
            .owner = owner,
            .owner_id = owner_id,
            .status = .committed,
            .name = name,
            .phys_base = base,
            .phys_len = len,
            .reserved_bytes = len,
            .committed_bytes = len,
        };
        if (try self.physicalMergeSlot(claim_request)) |merge_slot| {
            try journal.remember(self.entries, merge_slot);
        } else {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
        }
        const id = try self.addOrMergePhysical(claim_request);

        if (after_len != 0) {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
            _ = try self.register(.{
                .kind = .free,
                .owner = .system,
                .status = .free,
                .name = "free",
                .phys_base = claim_end,
                .phys_len = after_len,
            });
        }

        return id;
    }

    fn preparePhysicalRangeRelease(self: *Table, base: u64, len: u64) Error!PhysicalReleasePlan {
        return self.buildPhysicalReleasePlan(base, len) catch |err| switch (err) {
            // Coalescing is semantically neutral and may recover ordinary
            // metadata pressure. The second plan is still entirely private;
            // no claimed block has been mutated on either failure path.
            Error.TableFull => {
                self.coalescePhysical();
                return self.buildPhysicalReleasePlan(base, len);
            },
            else => return err,
        };
    }

    fn buildPhysicalReleasePlan(self: *const Table, base: u64, len: u64) Error!PhysicalReleasePlan {
        if (len == 0) return Error.EmptyRange;
        const release_end = checkedEnd(base, len) orelse return Error.Overflow;
        const target_index = self.containingClaimedPhysical(base, len) orelse return Error.NotFound;
        const block = self.entries[target_index];
        if (block.virt_len != 0 or block.kind == .free or block.status == .free) return Error.InvalidRange;
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
        const before_len = base - block.phys_base;
        const after_len = block_end - release_end;

        var plan = PhysicalReleasePlan{
            .target_index = target_index,
            .target_released = releasedEntry(block),
            .next_id_after = self.next_id,
        };

        const free_merge = try self.planReleasedFreeMerge(base, release_end, block.id);
        plan.merge_primary_index = free_merge.primary_index;
        plan.merge_primary_entry = free_merge.primary_entry;
        plan.merge_secondary_index = free_merge.secondary_index;
        plan.merge_secondary_released = free_merge.secondary_released;

        var requests: [3]RegisterRequest = .{RegisterRequest{}} ** 3;
        var request_count: usize = 0;
        if (before_len != 0) {
            requests[request_count] = .{
                .kind = block.kind,
                .owner = block.owner,
                .owner_id = block.owner_id,
                .status = block.status,
                .name = block.name,
                .phys_base = block.phys_base,
                .phys_len = before_len,
                .reserved_bytes = splitPhysicalBytes(block.reserved_bytes, before_len),
                .committed_bytes = splitPhysicalBytes(block.committed_bytes, before_len),
            };
            request_count += 1;
        }
        if (after_len != 0) {
            requests[request_count] = .{
                .kind = block.kind,
                .owner = block.owner,
                .owner_id = block.owner_id,
                .status = block.status,
                .name = block.name,
                .phys_base = release_end,
                .phys_len = after_len,
                .reserved_bytes = splitPhysicalBytes(block.reserved_bytes, after_len),
                .committed_bytes = splitPhysicalBytes(block.committed_bytes, after_len),
            };
            request_count += 1;
        }
        if (free_merge.primary_index == null) {
            requests[request_count] = .{
                .kind = .free,
                .owner = .system,
                .status = .free,
                .name = "free",
                .phys_base = base,
                .phys_len = len,
            };
            request_count += 1;
        }

        // The old target slot is always reusable. Only actual outputs that
        // cannot merge need further slots; this avoids false TableFull for an
        // edge release beside an existing free interval.
        var slots: [3]usize = .{0} ** 3;
        var slot_count: usize = 0;
        if (request_count != 0) {
            slots[slot_count] = target_index;
            slot_count += 1;
        }
        var slot_index: usize = 0;
        while (slot_count < request_count and slot_index < self.entries.len) : (slot_index += 1) {
            if (slot_index == target_index) continue;
            const entry = self.entries[slot_index];
            if (entry.slot_used and entry.status != .released) continue;
            slots[slot_count] = slot_index;
            slot_count += 1;
        }
        if (slot_count != request_count) return Error.TableFull;

        var next_id = self.next_id;
        var output_index: usize = 0;
        while (output_index < request_count) : (output_index += 1) {
            const req = requests[output_index];
            try validateRequest(req);
            if (self.overlapsActive(req.phys_base, req.phys_len, true, block.id)) return Error.Overlap;
            const id = takePlannedId(&next_id) orelse return Error.Overflow;
            plan.write_indices[output_index] = slots[output_index];
            plan.write_entries[output_index] = entryFromRequest(id, req);
        }
        plan.write_count = @intCast(request_count);
        plan.next_id_after = next_id;
        return plan;
    }

    const ReleasedFreeMerge = struct {
        primary_index: ?usize = null,
        primary_entry: MemoryBlock = .{},
        secondary_index: ?usize = null,
        secondary_released: MemoryBlock = .{},
    };

    fn planReleasedFreeMerge(self: *const Table, base: u64, release_end: u64, ignore_id: u32) Error!ReleasedFreeMerge {
        var left_index: ?usize = null;
        var right_index: ?usize = null;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const entry = self.entries[i];
            if (!entry.active() or entry.id == ignore_id or entry.kind != .free or entry.owner != .system or
                entry.owner_id != 0 or entry.status != .free or entry.virt_len != 0 or !strEq(entry.name, "free"))
            {
                continue;
            }
            const entry_end = checkedEnd(entry.phys_base, entry.phys_len) orelse return Error.Overflow;
            if (entry_end == base) {
                if (left_index != null) return Error.Overlap;
                left_index = i;
            }
            if (entry.phys_base == release_end) {
                if (right_index != null) return Error.Overlap;
                right_index = i;
            }
        }
        if (left_index == null and right_index == null) return .{};

        const primary_index = left_index orelse right_index.?;
        var primary = self.entries[primary_index];
        const merged_base = if (left_index) |left| self.entries[left].phys_base else base;
        const merged_end = if (right_index) |right|
            checkedEnd(self.entries[right].phys_base, self.entries[right].phys_len) orelse return Error.Overflow
        else
            release_end;
        if (merged_end < merged_base) return Error.Overflow;
        primary.phys_base = merged_base;
        primary.phys_len = merged_end - merged_base;
        primary.reserved_bytes = 0;
        primary.committed_bytes = 0;

        var result = ReleasedFreeMerge{
            .primary_index = primary_index,
            .primary_entry = primary,
        };
        if (left_index != null and right_index != null and left_index.? != right_index.?) {
            result.secondary_index = right_index.?;
            result.secondary_released = releasedEntry(self.entries[right_index.?]);
        }
        return result;
    }

    fn commitPhysicalRangeRelease(self: *Table, plan: PhysicalReleasePlan, coalesce_now: bool) void {
        self.entries[plan.target_index] = plan.target_released;
        if (plan.merge_primary_index) |index| self.entries[index] = plan.merge_primary_entry;
        if (plan.merge_secondary_index) |index| self.entries[index] = plan.merge_secondary_released;
        var i: usize = 0;
        while (i < @as(usize, plan.write_count)) : (i += 1) {
            self.entries[plan.write_indices[i]] = plan.write_entries[i];
        }
        self.next_id = plan.next_id_after;
        if (coalesce_now) self.coalescePhysical();
    }

    fn retagPhysicalRange(
        self: *Table,
        base: u64,
        len: u64,
        kind: Kind,
        owner: Owner,
        owner_id: u64,
        status: Status,
        name: []const u8,
    ) Error!u32 {
        if (len == 0 or checkedEnd(base, len) == null) return Error.EmptyRange;

        const idx = self.containingClaimedPhysical(base, len) orelse return Error.NotFound;
        const block = self.entries[idx];
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
        const tag_end = checkedEnd(base, len) orelse return Error.Overflow;
        const before_len = base - block.phys_base;
        const after_len = block_end - tag_end;

        self.entries[idx].status = .released;
        self.entries[idx].reserved_bytes = 0;
        self.entries[idx].committed_bytes = 0;

        if (before_len != 0) {
            _ = try self.register(.{
                .kind = block.kind,
                .owner = block.owner,
                .owner_id = block.owner_id,
                .status = block.status,
                .name = block.name,
                .phys_base = block.phys_base,
                .phys_len = before_len,
                .reserved_bytes = splitPhysicalBytes(block.reserved_bytes, before_len),
                .committed_bytes = splitPhysicalBytes(block.committed_bytes, before_len),
            });
        }

        const tagged_id = try self.addOrMergePhysical(.{
            .kind = kind,
            .owner = owner,
            .owner_id = owner_id,
            .status = status,
            .name = name,
            .phys_base = base,
            .phys_len = len,
            .reserved_bytes = if (status == .free or status == .released) 0 else len,
            .committed_bytes = if (status == .committed or status == .mapped) len else 0,
        });

        if (after_len != 0) {
            _ = try self.register(.{
                .kind = block.kind,
                .owner = block.owner,
                .owner_id = block.owner_id,
                .status = block.status,
                .name = block.name,
                .phys_base = tag_end,
                .phys_len = after_len,
                .reserved_bytes = splitPhysicalBytes(block.reserved_bytes, after_len),
                .committed_bytes = splitPhysicalBytes(block.committed_bytes, after_len),
            });
        }

        self.coalescePhysical();
        return tagged_id;
    }

    fn addOrMergePhysical(self: *Table, req: RegisterRequest) Error!u32 {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            var block = self.entries[i];
            if (!block.active()) continue;
            if (block.kind != req.kind or
                block.owner != req.owner or
                block.owner_id != req.owner_id or
                block.status != req.status or
                block.virt_len != 0 or
                !strEq(block.name, req.name))
            {
                continue;
            }
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
            const req_end = checkedEnd(req.phys_base, req.phys_len) orelse return Error.Overflow;
            if (block_end == req.phys_base) {
                block.phys_len = checkedAdd(block.phys_len, req.phys_len) orelse return Error.Overflow;
                block.reserved_bytes = checkedAdd(block.reserved_bytes, req.reserved_bytes) orelse return Error.Overflow;
                block.committed_bytes = checkedAdd(block.committed_bytes, req.committed_bytes) orelse return Error.Overflow;
                self.entries[i] = block;
                return block.id;
            }
            if (req_end == block.phys_base) {
                block.phys_base = req.phys_base;
                block.phys_len = checkedAdd(block.phys_len, req.phys_len) orelse return Error.Overflow;
                block.reserved_bytes = checkedAdd(block.reserved_bytes, req.reserved_bytes) orelse return Error.Overflow;
                block.committed_bytes = checkedAdd(block.committed_bytes, req.committed_bytes) orelse return Error.Overflow;
                self.entries[i] = block;
                return block.id;
            }
        }

        return self.register(req);
    }

    fn physicalMergeSlot(self: *const Table, req: RegisterRequest) Error!?usize {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active()) continue;
            if (block.kind != req.kind or
                block.owner != req.owner or
                block.owner_id != req.owner_id or
                block.status != req.status or
                block.virt_len != 0 or
                !strEq(block.name, req.name))
            {
                continue;
            }
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
            const req_end = checkedEnd(req.phys_base, req.phys_len) orelse return Error.Overflow;
            if (block_end == req.phys_base or req_end == block.phys_base) return i;
        }
        return null;
    }

    fn update(self: *Table, id: u32, req: UpdateRequest) Error!void {
        const idx = self.indexById(id) orelse return Error.NotFound;
        var block = self.entries[idx];
        if (!block.active()) return Error.NotFound;

        if (req.kind) |value| block.kind = value;
        if (req.owner) |value| block.owner = value;
        if (req.owner_id) |value| block.owner_id = value;
        if (req.status) |value| block.status = value;
        if (req.name) |value| block.name = value;
        if (req.reserved_bytes) |value| block.reserved_bytes = value;
        if (req.committed_bytes) |value| block.committed_bytes = value;
        validateBytes(block.reserved_bytes, block.committed_bytes) catch |err| return err;

        self.entries[idx] = block;
    }

    fn setCommitted(self: *Table, id: u32, bytes: u64) Error!void {
        try self.update(id, .{ .committed_bytes = bytes, .status = if (bytes == 0) .reserved else .committed });
    }

    fn commit(self: *Table, id: u32, bytes: u64) Error!void {
        const idx = self.indexById(id) orelse return Error.NotFound;
        const current = self.entries[idx].committed_bytes;
        const next = checkedAdd(current, bytes) orelse return Error.Overflow;
        try self.setCommitted(id, next);
    }

    fn uncommit(self: *Table, id: u32, bytes: u64) Error!void {
        const idx = self.indexById(id) orelse return Error.NotFound;
        const current = self.entries[idx].committed_bytes;
        if (bytes > current) return Error.InvalidBytes;
        try self.setCommitted(id, current - bytes);
    }

    fn release(self: *Table, id: u32) Error!void {
        const idx = self.indexById(id) orelse return Error.NotFound;
        self.entries[idx].status = .released;
        self.entries[idx].reserved_bytes = 0;
        self.entries[idx].committed_bytes = 0;
    }

    fn snapshot(self: *const Table, out: []MemoryBlock) SnapshotResult {
        var result: SnapshotResult = .{};
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active()) continue;
            result.total += 1;
            if (result.copied < out.len) {
                out[result.copied] = block;
                result.copied += 1;
            } else {
                result.truncated = true;
            }
        }
        return result;
    }

    fn firstByOwner(self: *const Table, owner: Owner, owner_id: u64) ?MemoryBlock {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (block.active() and block.owner == owner and block.owner_id == owner_id) return block;
        }
        return null;
    }

    fn firstContainingVirtual(self: *const Table, addr: u64) ?MemoryBlock {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active() or block.virt_len == 0) continue;
            const end = checkedEnd(block.virt_base, block.virt_len) orelse continue;
            if (addr >= block.virt_base and addr < end) return block;
        }
        return null;
    }

    fn countByOwner(self: *const Table, owner: Owner, owner_id: u64) u64 {
        var count: u64 = 0;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (block.active() and block.owner == owner and block.owner_id == owner_id) count += 1;
        }
        return count;
    }

    fn activeAt(self: *const Table, active_index: u32) ?MemoryBlock {
        var seen: u32 = 0;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active()) continue;
            if (seen == active_index) return block;
            seen += 1;
        }
        return null;
    }

    fn summary(self: *const Table) Summary {
        var s: Summary = .{};
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.slot_used) continue;
            s.total_slots_used += 1;
            if (block.status == .released) {
                s.released_blocks += 1;
                continue;
            }
            s.active_blocks += 1;
            if (block.status == .@"error") s.error_blocks += 1;
            checkedAddInto(&s.physical_bytes, block.phys_len, &s.overflow);
            checkedAddInto(&s.virtual_bytes, block.virt_len, &s.overflow);
            checkedAddInto(&s.reserved_bytes, block.reserved_bytes, &s.overflow);
            checkedAddInto(&s.committed_bytes, block.committed_bytes, &s.overflow);
            checkedAddInto(&s.by_kind[@intFromEnum(block.kind)], 1, &s.overflow);
            checkedAddInto(&s.by_owner[@intFromEnum(block.owner)], 1, &s.overflow);
            checkedAddInto(&s.by_status[@intFromEnum(block.status)], 1, &s.overflow);
            if (block.kind == .free and block.status == .free and block.phys_len != 0) {
                checkedAddInto(&s.free_physical_bytes, block.phys_len, &s.overflow);
                if (block.phys_len > s.largest_free_phys_len) {
                    s.largest_free_phys_base = block.phys_base;
                    s.largest_free_phys_len = block.phys_len;
                }
            }
        }
        return s;
    }

    fn freeSlot(self: *const Table) ?usize {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            if (!self.entries[i].slot_used or self.entries[i].status == .released) return i;
        }
        return null;
    }

    fn allocId(self: *Table) Error!u32 {
        if (self.next_id == 0) return Error.Overflow;
        const id = self.next_id;
        self.next_id +%= 1;
        return id;
    }

    fn indexById(self: *const Table, id: u32) ?usize {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            if (self.entries[i].slot_used and self.entries[i].id == id) return i;
        }
        return null;
    }

    fn containingFreePhysical(self: *const Table, base: u64, len: u64) ?usize {
        const end = checkedEnd(base, len) orelse return null;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active() or block.kind != .free or block.status != .free) continue;
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse continue;
            if (base >= block.phys_base and end <= block_end) return i;
        }
        return null;
    }

    fn containingClaimedPhysical(self: *const Table, base: u64, len: u64) ?usize {
        const end = checkedEnd(base, len) orelse return null;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active() or block.kind == .free or block.status == .free or block.phys_len == 0) continue;
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse continue;
            if (base >= block.phys_base and end <= block_end) return i;
        }
        return null;
    }

    fn overlapsActive(self: *const Table, base: u64, len: u64, physical: bool, ignore_id: u32) bool {
        if (len == 0) return false;
        const end = checkedEnd(base, len) orelse return true;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active() or block.id == ignore_id) continue;
            const other_base = if (physical) block.phys_base else block.virt_base;
            const other_len = if (physical) block.phys_len else block.virt_len;
            if (other_len == 0) continue;
            const other_end = checkedEnd(other_base, other_len) orelse return true;
            if (base < other_end and other_base < end) return true;
        }
        return false;
    }

    fn coalescePhysical(self: *Table) void {
        var changed = true;
        while (changed) {
            changed = false;
            var a: usize = 0;
            while (a < self.entries.len) : (a += 1) {
                if (!self.entries[a].active() or self.entries[a].virt_len != 0 or self.entries[a].phys_len == 0) continue;
                var b: usize = a + 1;
                while (b < self.entries.len) : (b += 1) {
                    if (!canMergePhysical(self.entries[a], self.entries[b])) continue;
                    const a_end = checkedEnd(self.entries[a].phys_base, self.entries[a].phys_len) orelse continue;
                    const b_end = checkedEnd(self.entries[b].phys_base, self.entries[b].phys_len) orelse continue;
                    if (a_end == self.entries[b].phys_base) {
                        self.entries[a].phys_len = checkedAdd(self.entries[a].phys_len, self.entries[b].phys_len) orelse continue;
                        self.entries[a].reserved_bytes = checkedAdd(self.entries[a].reserved_bytes, self.entries[b].reserved_bytes) orelse continue;
                        self.entries[a].committed_bytes = checkedAdd(self.entries[a].committed_bytes, self.entries[b].committed_bytes) orelse continue;
                        self.entries[b].status = .released;
                        self.entries[b].reserved_bytes = 0;
                        self.entries[b].committed_bytes = 0;
                        changed = true;
                        break;
                    }
                    if (b_end == self.entries[a].phys_base) {
                        self.entries[a].phys_base = self.entries[b].phys_base;
                        self.entries[a].phys_len = checkedAdd(self.entries[a].phys_len, self.entries[b].phys_len) orelse continue;
                        self.entries[a].reserved_bytes = checkedAdd(self.entries[a].reserved_bytes, self.entries[b].reserved_bytes) orelse continue;
                        self.entries[a].committed_bytes = checkedAdd(self.entries[a].committed_bytes, self.entries[b].committed_bytes) orelse continue;
                        self.entries[b].status = .released;
                        self.entries[b].reserved_bytes = 0;
                        self.entries[b].committed_bytes = 0;
                        changed = true;
                        break;
                    }
                }
                if (changed) break;
            }
        }
    }
};

const PhysicalMutationJournal = struct {
    const MAX_TOUCHED_SLOTS: usize = 4;

    indices: [MAX_TOUCHED_SLOTS]usize = .{0} ** MAX_TOUCHED_SLOTS,
    entries: [MAX_TOUCHED_SLOTS]MemoryBlock = .{MemoryBlock{}} ** MAX_TOUCHED_SLOTS,
    count: usize = 0,
    next_id: u32,

    fn init(next_id: u32) PhysicalMutationJournal {
        return .{ .next_id = next_id };
    }

    fn remember(self: *PhysicalMutationJournal, entries: []const MemoryBlock, index: usize) Error!void {
        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            if (self.indices[i] == index) return;
        }
        if (self.count >= self.indices.len) return Error.TableFull;
        self.indices[self.count] = index;
        self.entries[self.count] = entries[index];
        self.count += 1;
    }

    fn rollback(self: *const PhysicalMutationJournal, target: *Table) void {
        var remaining = self.count;
        while (remaining != 0) {
            remaining -= 1;
            target.entries[self.indices[remaining]] = self.entries[remaining];
        }
        target.next_id = self.next_id;
    }
};

var storage: [MAX_BLOCKS]MemoryBlock = .{MemoryBlock{}} ** MAX_BLOCKS;
var table: Table = .{ .entries = storage[0..] };
var initialized = false;

pub fn initFromBootInfo() bool {
    table.reset();
    initialized = true;

    const entries = boot_info.memoryMap();
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid or entry.length == 0) continue;
        _ = register(memoryMapRequest(entry)) catch return false;
    }
    return true;
}

pub fn register(req: RegisterRequest) Error!u32 {
    if (!initialized) return Error.NotInitialized;
    return table.register(req);
}

pub fn claimPhysicalRange(
    base: u64,
    len: u64,
    kind: Kind,
    owner: Owner,
    owner_id: u64,
    name: []const u8,
) Error!u32 {
    if (!initialized) return Error.NotInitialized;
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return table.claimPhysicalRange(base, len, kind, owner, owner_id, name);
}

pub fn releasePhysicalRange(base: u64, len: u64) Error!void {
    var plan = try preparePhysicalRangeRelease(base, len);
    finishPhysicalRangeRelease(&plan, true);
}

pub fn preparePhysicalRangeRelease(base: u64, len: u64) Error!PhysicalReleasePlan {
    if (!initialized) return Error.NotInitialized;
    const irq_flags = interrupts.saveAndDisable();
    var plan = table.preparePhysicalRangeRelease(base, len) catch |err| {
        interrupts.restore(irq_flags);
        return err;
    };
    plan.irq_flags = irq_flags;
    plan.active = true;
    return plan;
}

// Applies a previously validated plan after the caller has unmapped the PTE.
// There are deliberately no failure paths here: every output slot, merge and
// block ID is protected by the interrupt-disabled UP transaction begun in
// preparePhysicalRangeRelease(). SMP will replace this token with a table
// spinlock while retaining the same prepare/cancel/commit boundary.
pub fn commitPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    finishPhysicalRangeRelease(plan, false);
}

pub fn cancelPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    if (!plan.active) return;
    plan.active = false;
    interrupts.restore(plan.irq_flags);
}

fn finishPhysicalRangeRelease(plan: *PhysicalReleasePlan, coalesce_now: bool) void {
    if (!plan.active) unreachable;
    table.commitPhysicalRangeRelease(plan.*, coalesce_now);
    plan.active = false;
    interrupts.restore(plan.irq_flags);
}

// Compatibility entrypoint for callers that do not need to hold the plan
// across an external operation. VM teardown uses prepare+commit directly and
// therefore never repeats the full-table preflight after unmapping a page.
pub fn releasePhysicalRangeDeferred(base: u64, len: u64) Error!void {
    var plan = try preparePhysicalRangeRelease(base, len);
    finishPhysicalRangeRelease(&plan, false);
}

pub fn coalescePhysicalRanges() void {
    if (!initialized) return;
    table.coalescePhysical();
}

pub fn retagPhysicalRange(
    base: u64,
    len: u64,
    kind: Kind,
    owner: Owner,
    owner_id: u64,
    status: Status,
    name: []const u8,
) Error!u32 {
    if (!initialized) return Error.NotInitialized;
    return table.retagPhysicalRange(base, len, kind, owner, owner_id, status, name);
}

pub fn update(id: u32, req: UpdateRequest) Error!void {
    if (!initialized) return Error.NotInitialized;
    return table.update(id, req);
}

pub fn setCommitted(id: u32, bytes: u64) Error!void {
    if (!initialized) return Error.NotInitialized;
    return table.setCommitted(id, bytes);
}

pub fn commit(id: u32, bytes: u64) Error!void {
    if (!initialized) return Error.NotInitialized;
    return table.commit(id, bytes);
}

pub fn uncommit(id: u32, bytes: u64) Error!void {
    if (!initialized) return Error.NotInitialized;
    return table.uncommit(id, bytes);
}

pub fn release(id: u32) Error!void {
    if (!initialized) return Error.NotInitialized;
    return table.release(id);
}

pub fn snapshot(out: []MemoryBlock) SnapshotResult {
    if (!initialized) return .{};
    return table.snapshot(out);
}

pub fn firstByOwner(owner: Owner, owner_id: u64) ?MemoryBlock {
    if (!initialized) return null;
    return table.firstByOwner(owner, owner_id);
}

pub fn firstContainingVirtual(addr: u64) ?MemoryBlock {
    if (!initialized) return null;
    return table.firstContainingVirtual(addr);
}

pub fn countByOwner(owner: Owner, owner_id: u64) u64 {
    if (!initialized) return 0;
    return table.countByOwner(owner, owner_id);
}

pub fn activeAt(active_index: u32) ?MemoryBlock {
    if (!initialized) return null;
    return table.activeAt(active_index);
}

pub fn summary() Summary {
    if (!initialized) return .{};
    return table.summary();
}

pub fn dumpSummary() void {
    const s = summary();
    k.puts("  MemoryBlocks: active=");
    k.putDec(s.active_blocks);
    k.puts(" released=");
    k.putDec(s.released_blocks);
    k.puts(" errors=");
    k.putDec(s.error_blocks);
    k.puts(" overflow=");
    k.puts(if (s.overflow) "yes" else "no");
    k.puts("\r\n");

    k.puts("  MemoryBlocks bytes: phys=");
    k.putDec(s.physical_bytes);
    k.puts(" virt=");
    k.putDec(s.virtual_bytes);
    k.puts(" reserved=");
    k.putDec(s.reserved_bytes);
    k.puts(" committed=");
    k.putDec(s.committed_bytes);
    k.puts("\r\n");

    k.puts("  MemoryBlocks free phys=");
    k.putDec(s.free_physical_bytes);
    k.puts(" largest=0x");
    k.putHex(s.largest_free_phys_base, 16);
    k.puts(" len=0x");
    k.putHex(s.largest_free_phys_len, 16);
    k.puts("\r\n");

    dumpKindCounts(s);
}

pub fn kindName(kind: Kind) []const u8 {
    return switch (kind) {
        .boot => "boot",
        .kernel => "kernel",
        .kernel_heap => "kernel_heap",
        .page_table => "page_table",
        .virtual_range => "virtual_range",
        .program_image => "program_image",
        .app_heap => "app_heap",
        .app_stack => "app_stack",
        .dma => "dma",
        .mmio => "mmio",
        .framebuffer => "framebuffer",
        .reserved => "reserved",
        .free => "free",
        .unknown => "unknown",
    };
}

pub fn ownerName(owner: Owner) []const u8 {
    return switch (owner) {
        .kernel => "kernel",
        .driver => "driver",
        .protocol => "protocol",
        .r4x_instance => "r4x_instance",
        .task => "task",
        .device => "device",
        .bootloader => "bootloader",
        .system => "system",
    };
}

pub fn statusName(status: Status) []const u8 {
    return switch (status) {
        .free => "free",
        .reserved => "reserved",
        .committed => "committed",
        .guard => "guard",
        .mapped => "mapped",
        .released => "released",
        .@"error" => "error",
    };
}

fn memoryMapRequest(entry: boot_info.MemoryMapEntry) RegisterRequest {
    const Mapped = struct {
        kind: Kind,
        owner: Owner,
        status: Status,
        name: []const u8,
        reserved: u64,
        committed: u64,
    };
    const mapped: Mapped = switch (entry.kind) {
        .usable => .{ .kind = Kind.free, .owner = Owner.system, .status = Status.free, .name = "free", .reserved = @as(u64, 0), .committed = @as(u64, 0) },
        .reserved => .{ .kind = Kind.reserved, .owner = Owner.system, .status = Status.reserved, .name = "reserved", .reserved = entry.length, .committed = @as(u64, 0) },
        .acpi_reclaimable => .{ .kind = Kind.reserved, .owner = Owner.system, .status = Status.reserved, .name = "acpi-reclaim", .reserved = entry.length, .committed = @as(u64, 0) },
        .acpi_nvs => .{ .kind = Kind.reserved, .owner = Owner.system, .status = Status.reserved, .name = "acpi-nvs", .reserved = entry.length, .committed = @as(u64, 0) },
        .bad_memory => .{ .kind = Kind.reserved, .owner = Owner.system, .status = Status.@"error", .name = "bad-memory", .reserved = entry.length, .committed = @as(u64, 0) },
        .bootloader_reclaimable => .{ .kind = Kind.boot, .owner = Owner.bootloader, .status = Status.reserved, .name = "bootloader", .reserved = entry.length, .committed = @as(u64, 0) },
        .kernel_and_modules => .{ .kind = Kind.kernel, .owner = Owner.kernel, .status = Status.committed, .name = "kernel", .reserved = entry.length, .committed = entry.length },
        .framebuffer => .{ .kind = Kind.framebuffer, .owner = Owner.device, .status = Status.mapped, .name = "framebuffer", .reserved = entry.length, .committed = entry.length },
        .unknown => .{ .kind = Kind.unknown, .owner = Owner.system, .status = Status.reserved, .name = "unknown", .reserved = entry.length, .committed = @as(u64, 0) },
    };

    return .{
        .kind = mapped.kind,
        .owner = mapped.owner,
        .status = mapped.status,
        .name = mapped.name,
        .phys_base = entry.base,
        .phys_len = entry.length,
        .reserved_bytes = mapped.reserved,
        .committed_bytes = mapped.committed,
    };
}

fn dumpKindCounts(s: Summary) void {
    var i: usize = 0;
    while (i < KIND_COUNT) : (i += 1) {
        if (s.by_kind[i] == 0) continue;
        k.puts("    ");
        k.puts(kindName(@enumFromInt(i)));
        k.puts(": ");
        k.putDec(s.by_kind[i]);
        k.puts("\r\n");
    }
}

fn validateRequest(req: RegisterRequest) Error!void {
    if ((req.phys_len == 0 and req.virt_len == 0) or
        checkedEnd(req.phys_base, req.phys_len) == null or
        checkedEnd(req.virt_base, req.virt_len) == null)
    {
        return Error.EmptyRange;
    }
    try validateBytes(req.reserved_bytes, req.committed_bytes);
}

fn validateBytes(reserved: u64, committed: u64) Error!void {
    if (reserved != 0 and committed > reserved) return Error.InvalidBytes;
}

fn releasedEntry(source: MemoryBlock) MemoryBlock {
    var result = source;
    result.status = .released;
    result.reserved_bytes = 0;
    result.committed_bytes = 0;
    return result;
}

fn entryFromRequest(id: u32, req: RegisterRequest) MemoryBlock {
    return .{
        .slot_used = true,
        .id = id,
        .kind = req.kind,
        .owner = req.owner,
        .owner_id = req.owner_id,
        .status = req.status,
        .name = req.name,
        .phys_base = req.phys_base,
        .phys_len = req.phys_len,
        .virt_base = req.virt_base,
        .virt_len = req.virt_len,
        .reserved_bytes = req.reserved_bytes,
        .committed_bytes = req.committed_bytes,
    };
}

fn takePlannedId(next_id: *u32) ?u32 {
    if (next_id.* == 0) return null;
    const id = next_id.*;
    next_id.* +%= 1;
    return id;
}

fn checkedEnd(base: u64, len: u64) ?u64 {
    if (len == 0) return base;
    return checkedAdd(base, len);
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn checkedAddInto(target: *u64, value: u64, overflow: *bool) void {
    if (checkedAdd(target.*, value)) |next| {
        target.* = next;
    } else {
        overflow.* = true;
        target.* = ~@as(u64, 0);
    }
}

fn splitPhysicalBytes(bytes: u64, part_len: u64) u64 {
    if (bytes == 0) return 0;
    return if (bytes >= part_len) part_len else bytes;
}

fn canMergePhysical(a: MemoryBlock, b: MemoryBlock) bool {
    return a.active() and
        b.active() and
        a.virt_len == 0 and
        b.virt_len == 0 and
        a.phys_len != 0 and
        b.phys_len != 0 and
        a.kind == b.kind and
        a.owner == b.owner and
        a.owner_id == b.owner_id and
        a.status == b.status and
        strEq(a.name, b.name);
}

fn strEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}
