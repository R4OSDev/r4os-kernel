const std = @import("std");
const boot_info = @import("../bootloader/boot_info.zig");
const k = @import("../kernel/log.zig");
const owner_locks = @import("owner_locks.zig");

pub const MAX_BLOCKS: usize = 8192;
pub const KIND_COUNT: usize = 14;
pub const OWNER_COUNT: usize = 8;
pub const STATUS_COUNT: usize = 7;
pub const ID_INDEX_CAPACITY: usize = MAX_BLOCKS * 2;
const FREE_SLOT_WORDS: usize = MAX_BLOCKS / 64;

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

pub const HotPathStats = struct {
    physical_index_entries: u32 = 0,
    physical_lookups: u64 = 0,
    physical_steps: u64 = 0,
    physical_step_max: u32 = 0,
    physical_mutations: u64 = 0,
    physical_rebuilds: u64 = 0,
    id_index_entries: u32 = 0,
    id_index_lookups: u64 = 0,
    id_index_steps: u64 = 0,
    id_index_step_max: u32 = 0,
    free_slot_lookups: u64 = 0,
    free_slot_word_steps: u64 = 0,
    free_slot_word_step_max: u32 = 0,
    claim_transactions: u64 = 0,
    claim_rollbacks: u64 = 0,
};

const PhysicalNode = struct {
    in_tree: bool = false,
    left: ?u16 = null,
    right: ?u16 = null,
    height: u8 = 1,
};

const IdIndexState = enum(u8) {
    empty = 0,
    used = 1,
    tombstone = 2,
};

const IdIndexEntry = struct {
    state: IdIndexState = .empty,
    id: u32 = 0,
    slot: u16 = 0,
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
    lock_token: owner_locks.Token = .{},
    active: bool = false,
};

const Table = struct {
    entries: []MemoryBlock,
    next_id: u32 = 1,
    physical_nodes: [MAX_BLOCKS]PhysicalNode = .{PhysicalNode{}} ** MAX_BLOCKS,
    physical_root: ?u16 = null,
    physical_entry_count: u32 = 0,
    id_index: [ID_INDEX_CAPACITY]IdIndexEntry = .{IdIndexEntry{}} ** ID_INDEX_CAPACITY,
    id_index_entries: u32 = 0,
    id_index_tombstones: u32 = 0,
    free_slot_words: [FREE_SLOT_WORDS]u64 = .{std.math.maxInt(u64)} ** FREE_SLOT_WORDS,
    next_free_slot_word: usize = 0,
    hot_path_stats: HotPathStats = .{},

    fn reset(self: *Table) void {
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) self.entries[i] = .{};
        i = 0;
        while (i < self.physical_nodes.len) : (i += 1) self.physical_nodes[i] = .{};
        i = 0;
        while (i < self.id_index.len) : (i += 1) self.id_index[i] = .{};
        i = 0;
        while (i < self.free_slot_words.len) : (i += 1) self.free_slot_words[i] = std.math.maxInt(u64);
        self.next_id = 1;
        self.physical_root = null;
        self.physical_entry_count = 0;
        self.id_index_entries = 0;
        self.id_index_tombstones = 0;
        self.next_free_slot_word = 0;
        self.hot_path_stats = .{};
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
        self.replaceEntry(slot, .{
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
        });
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
        self.hot_path_stats.claim_transactions +%= 1;

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
        errdefer {
            self.hot_path_stats.claim_rollbacks +%= 1;
            journal.rollback(self);
        }

        self.replaceEntry(free_index, releasedEntry(free));

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

    fn preparePhysicalRangeRelease(self: *Table, base: u64, len: u64, plan: *PhysicalReleasePlan) Error!void {
        self.buildPhysicalReleasePlan(base, len, plan) catch |err| switch (err) {
            // Coalescing is semantically neutral and may recover ordinary
            // metadata pressure. The second plan is still entirely private;
            // no claimed block has been mutated on either failure path.
            Error.TableFull => {
                self.coalescePhysical();
                return self.buildPhysicalReleasePlan(base, len, plan);
            },
            else => return err,
        };
    }

    fn buildPhysicalReleasePlan(self: *Table, base: u64, len: u64, plan: *PhysicalReleasePlan) Error!void {
        if (len == 0) return Error.EmptyRange;
        const release_end = checkedEnd(base, len) orelse return Error.Overflow;
        const target_index = self.containingClaimedPhysical(base, len) orelse return Error.NotFound;
        const block = self.entries[target_index];
        if (block.virt_len != 0 or block.kind == .free or block.status == .free) return Error.InvalidRange;
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
        const before_len = base - block.phys_base;
        const after_len = block_end - release_end;

        plan.* = PhysicalReleasePlan{
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
        self.collectFreeSlots(&slots, &slot_count, request_count, target_index);
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
    }

    const ReleasedFreeMerge = struct {
        primary_index: ?usize = null,
        primary_entry: MemoryBlock = .{},
        secondary_index: ?usize = null,
        secondary_released: MemoryBlock = .{},
    };

    fn planReleasedFreeMerge(self: *Table, base: u64, release_end: u64, ignore_id: u32) Error!ReleasedFreeMerge {
        var left_index = self.physicalPredecessorBefore(base, ignore_id);
        if (left_index) |index| {
            const entry = self.entries[index];
            const entry_end = checkedEnd(entry.phys_base, entry.phys_len) orelse return Error.Overflow;
            if (entry_end != base or !isCanonicalFree(entry)) left_index = null;
        }
        var right_index = self.physicalLowerBound(release_end);
        if (right_index) |index| {
            const entry = self.entries[index];
            if (entry.id == ignore_id) right_index = self.physicalSuccessor(index);
        }
        if (right_index) |index| {
            const entry = self.entries[index];
            if (entry.phys_base != release_end or !isCanonicalFree(entry)) right_index = null;
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
        self.replaceEntry(plan.target_index, plan.target_released);
        if (plan.merge_primary_index) |index| self.replaceEntry(index, plan.merge_primary_entry);
        if (plan.merge_secondary_index) |index| self.replaceEntry(index, plan.merge_secondary_released);
        var i: usize = 0;
        while (i < @as(usize, plan.write_count)) : (i += 1) {
            self.replaceEntry(plan.write_indices[i], plan.write_entries[i]);
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

        var journal = PhysicalMutationJournal.init(self.next_id);
        try journal.remember(self.entries, idx);
        errdefer journal.rollback(self);
        self.replaceEntry(idx, releasedEntry(block));

        if (before_len != 0) {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
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

        const tag_request = RegisterRequest{
            .kind = kind,
            .owner = owner,
            .owner_id = owner_id,
            .status = status,
            .name = name,
            .phys_base = base,
            .phys_len = len,
            .reserved_bytes = if (status == .free or status == .released) 0 else len,
            .committed_bytes = if (status == .committed or status == .mapped) len else 0,
        };
        if (try self.physicalMergeSlot(tag_request)) |merge_slot| {
            try journal.remember(self.entries, merge_slot);
        } else {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
        }
        const tagged_id = try self.addOrMergePhysical(tag_request);

        if (after_len != 0) {
            try journal.remember(self.entries, self.freeSlot() orelse return Error.TableFull);
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
        if (try self.physicalMergeSlot(req)) |index| {
            var block = self.entries[index];
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
            const req_end = checkedEnd(req.phys_base, req.phys_len) orelse return Error.Overflow;
            const combined_phys = checkedAdd(block.phys_len, req.phys_len) orelse return Error.Overflow;
            const combined_reserved = checkedAdd(block.reserved_bytes, req.reserved_bytes) orelse return Error.Overflow;
            const combined_committed = checkedAdd(block.committed_bytes, req.committed_bytes) orelse return Error.Overflow;
            if (block_end == req.phys_base) {
                block.phys_len = combined_phys;
            } else if (req_end == block.phys_base) {
                block.phys_base = req.phys_base;
                block.phys_len = combined_phys;
            } else {
                unreachable;
            }
            block.reserved_bytes = combined_reserved;
            block.committed_bytes = combined_committed;
            self.replaceEntry(index, block);
            return block.id;
        }

        return self.register(req);
    }

    fn physicalMergeSlot(self: *Table, req: RegisterRequest) Error!?usize {
        const req_end = checkedEnd(req.phys_base, req.phys_len) orelse return Error.Overflow;
        if (self.physicalPredecessorBefore(req.phys_base, 0)) |left| {
            const block = self.entries[left];
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return Error.Overflow;
            if (block_end == req.phys_base and canMergePhysicalRequest(block, req)) return left;
        }
        if (self.physicalLowerBound(req_end)) |right| {
            const block = self.entries[right];
            if (block.phys_base == req_end and canMergePhysicalRequest(block, req)) return right;
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

        self.replaceEntry(idx, block);
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
        self.replaceEntry(idx, releasedEntry(self.entries[idx]));
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

    fn freeSlot(self: *Table) ?usize {
        self.hot_path_stats.free_slot_lookups +%= 1;
        var scanned: usize = 0;
        while (scanned < self.free_slot_words.len) : (scanned += 1) {
            const word_index = (self.next_free_slot_word + scanned) % self.free_slot_words.len;
            const word = self.free_slot_words[word_index];
            if (word == 0) continue;
            const bit: u6 = @intCast(@ctz(word));
            const slot = word_index * 64 + bit;
            self.next_free_slot_word = word_index;
            self.recordFreeSlotSteps(scanned + 1);
            return slot;
        }
        self.recordFreeSlotSteps(scanned);
        return null;
    }

    fn collectFreeSlots(self: *Table, slots: *[3]usize, slot_count: *usize, wanted: usize, excluded: usize) void {
        var word_index: usize = 0;
        var words_scanned: usize = 0;
        while (slot_count.* < wanted and word_index < self.free_slot_words.len) : (word_index += 1) {
            words_scanned += 1;
            var word = self.free_slot_words[word_index];
            while (word != 0 and slot_count.* < wanted) {
                const bit: u6 = @intCast(@ctz(word));
                const slot = word_index * 64 + bit;
                word &= ~(@as(u64, 1) << bit);
                if (slot == excluded) continue;
                slots[slot_count.*] = slot;
                slot_count.* += 1;
            }
        }
        self.recordFreeSlotSteps(words_scanned);
    }

    fn allocId(self: *Table) Error!u32 {
        if (self.next_id == 0) return Error.Overflow;
        const id = self.next_id;
        self.next_id +%= 1;
        return id;
    }

    fn indexById(self: *Table, id: u32) ?usize {
        self.hot_path_stats.id_index_lookups +%= 1;
        var steps: usize = 0;
        var position = idIndexStart(id);
        while (steps < self.id_index.len) : (steps += 1) {
            const index_entry = self.id_index[position];
            switch (index_entry.state) {
                .empty => {
                    self.recordIdIndexSteps(steps + 1);
                    return null;
                },
                .tombstone => {},
                .used => {
                    if (index_entry.id == id) {
                        self.recordIdIndexSteps(steps + 1);
                        const slot: usize = index_entry.slot;
                        if (slot < self.entries.len and self.entries[slot].slot_used and self.entries[slot].id == id) return slot;
                        return null;
                    }
                },
            }
            position = (position + 1) % self.id_index.len;
        }
        self.recordIdIndexSteps(steps);
        return null;
    }

    fn containingFreePhysical(self: *Table, base: u64, len: u64) ?usize {
        const end = checkedEnd(base, len) orelse return null;
        const index = self.physicalFloor(base) orelse return null;
        const block = self.entries[index];
        if (!isCanonicalFree(block)) return null;
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return null;
        if (end <= block_end) return index;
        return null;
    }

    fn containingClaimedPhysical(self: *Table, base: u64, len: u64) ?usize {
        const end = checkedEnd(base, len) orelse return null;
        const index = self.physicalFloor(base) orelse return null;
        const block = self.entries[index];
        if (!block.active() or block.kind == .free or block.status == .free or block.phys_len == 0) return null;
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return null;
        if (end <= block_end) return index;
        return null;
    }

    fn freePhysicalPrefix(self: *Table, base: u64, max_len: u64) u64 {
        if (max_len == 0) return 0;
        const index = self.physicalFloor(base) orelse return 0;
        const block = self.entries[index];
        if (!isCanonicalFree(block) or base < block.phys_base) return 0;
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return 0;
        if (base >= block_end) return 0;
        return @min(max_len, block_end - base);
    }

    fn claimedPhysicalPrefix(self: *Table, base: u64, max_len: u64) u64 {
        if (max_len == 0) return 0;
        const index = self.physicalFloor(base) orelse return 0;
        const block = self.entries[index];
        if (!block.active() or block.kind == .free or block.status == .free or
            block.phys_len == 0 or base < block.phys_base)
        {
            return 0;
        }
        const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return 0;
        if (base >= block_end) return 0;
        return @min(max_len, block_end - base);
    }

    fn overlapsActive(self: *Table, base: u64, len: u64, physical: bool, ignore_id: u32) bool {
        if (len == 0) return false;
        const end = checkedEnd(base, len) orelse return true;
        if (physical) return self.physicalOverlap(base, end, ignore_id);
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const block = self.entries[i];
            if (!block.active() or block.id == ignore_id) continue;
            const other_base = block.virt_base;
            const other_len = block.virt_len;
            if (other_len == 0) continue;
            const other_end = checkedEnd(other_base, other_len) orelse return true;
            if (base < other_end and other_base < end) return true;
        }
        return false;
    }

    fn coalescePhysical(self: *Table) void {
        var current = self.physicalMinimum(self.physical_root);
        while (current) |left_index| {
            const right_index = self.physicalSuccessor(left_index) orelse break;
            const left = self.entries[left_index];
            const right = self.entries[right_index];
            const left_end = checkedEnd(left.phys_base, left.phys_len) orelse {
                current = @intCast(right_index);
                continue;
            };
            if (left_end != right.phys_base or !canMergePhysical(left, right)) {
                current = @intCast(right_index);
                continue;
            }
            const phys_len = checkedAdd(left.phys_len, right.phys_len) orelse {
                current = @intCast(right_index);
                continue;
            };
            const reserved_bytes = checkedAdd(left.reserved_bytes, right.reserved_bytes) orelse {
                current = @intCast(right_index);
                continue;
            };
            const committed_bytes = checkedAdd(left.committed_bytes, right.committed_bytes) orelse {
                current = @intCast(right_index);
                continue;
            };
            var merged = left;
            merged.phys_len = phys_len;
            merged.reserved_bytes = reserved_bytes;
            merged.committed_bytes = committed_bytes;
            self.replaceEntry(left_index, merged);
            self.replaceEntry(right_index, releasedEntry(right));
        }
    }

    fn replaceEntry(self: *Table, index: usize, entry: MemoryBlock) void {
        if (self.physical_nodes[index].in_tree) self.physicalRemove(@intCast(index));
        if (self.entries[index].slot_used) self.idIndexRemove(self.entries[index].id);

        self.entries[index] = entry;
        self.setSlotReusable(index, !entry.slot_used or entry.status == .released);
        if (entry.slot_used) self.idIndexInsert(entry.id, @intCast(index));
        if (usesPhysicalIndex(entry)) self.physicalInsert(@intCast(index));
        self.hot_path_stats.physical_mutations +%= 1;
    }

    fn setSlotReusable(self: *Table, index: usize, reusable: bool) void {
        const word_index = index / 64;
        const bit: u6 = @intCast(index % 64);
        const mask = @as(u64, 1) << bit;
        if (reusable) {
            self.free_slot_words[word_index] |= mask;
            if (word_index < self.next_free_slot_word) self.next_free_slot_word = word_index;
        } else {
            self.free_slot_words[word_index] &= ~mask;
        }
    }

    fn idIndexInsert(self: *Table, id: u32, slot: u16) void {
        var position = idIndexStart(id);
        var steps: usize = 0;
        while (steps < self.id_index.len) : (steps += 1) {
            if (self.id_index[position].state == .empty) {
                self.id_index[position] = .{ .state = .used, .id = id, .slot = slot };
                self.id_index_entries +|= 1;
                return;
            }
            if (self.id_index[position].state == .used and self.id_index[position].id == id) {
                self.id_index[position].slot = slot;
                return;
            }
            position = (position + 1) % self.id_index.len;
        }
        unreachable;
    }

    fn idIndexRemove(self: *Table, id: u32) void {
        var position = idIndexStart(id);
        var steps: usize = 0;
        while (steps < self.id_index.len) : (steps += 1) {
            const entry = self.id_index[position];
            if (entry.state == .empty) return;
            if (entry.state == .used and entry.id == id) {
                self.id_index[position] = .{};
                if (self.id_index_entries != 0) self.id_index_entries -= 1;
                var rehash = (position + 1) % self.id_index.len;
                while (self.id_index[rehash].state == .used) : (rehash = (rehash + 1) % self.id_index.len) {
                    const displaced = self.id_index[rehash];
                    self.id_index[rehash] = .{};
                    if (self.id_index_entries != 0) self.id_index_entries -= 1;
                    self.idIndexInsert(displaced.id, displaced.slot);
                }
                return;
            }
            position = (position + 1) % self.id_index.len;
        }
    }

    fn physicalInsert(self: *Table, slot: u16) void {
        self.physical_nodes[slot] = .{ .in_tree = true };
        self.physical_root = self.physicalInsertAt(self.physical_root, slot);
        self.physical_entry_count +|= 1;
    }

    fn physicalInsertAt(self: *Table, root: ?u16, slot: u16) u16 {
        const root_slot = root orelse return slot;
        if (self.physicalLess(slot, root_slot)) {
            self.physical_nodes[root_slot].left = self.physicalInsertAt(self.physical_nodes[root_slot].left, slot);
        } else {
            self.physical_nodes[root_slot].right = self.physicalInsertAt(self.physical_nodes[root_slot].right, slot);
        }
        return self.rebalancePhysical(root_slot);
    }

    fn physicalRemove(self: *Table, slot: u16) void {
        if (!self.physical_nodes[slot].in_tree) return;
        self.physical_root = self.physicalRemoveAt(self.physical_root, slot);
        self.physical_nodes[slot] = .{};
        if (self.physical_entry_count != 0) self.physical_entry_count -= 1;
    }

    fn physicalRemoveAt(self: *Table, root: ?u16, slot: u16) ?u16 {
        const root_slot = root orelse return null;
        if (self.physicalLess(slot, root_slot)) {
            self.physical_nodes[root_slot].left = self.physicalRemoveAt(self.physical_nodes[root_slot].left, slot);
            return self.rebalancePhysical(root_slot);
        }
        if (self.physicalLess(root_slot, slot)) {
            self.physical_nodes[root_slot].right = self.physicalRemoveAt(self.physical_nodes[root_slot].right, slot);
            return self.rebalancePhysical(root_slot);
        }

        const left = self.physical_nodes[root_slot].left;
        const right = self.physical_nodes[root_slot].right;
        if (left == null) return right;
        if (right == null) return left;
        const detached = self.detachPhysicalMinimum(right.?);
        self.physical_nodes[detached.minimum].left = left;
        self.physical_nodes[detached.minimum].right = detached.root;
        return self.rebalancePhysical(detached.minimum);
    }

    const DetachedPhysicalMinimum = struct {
        root: ?u16,
        minimum: u16,
    };

    fn detachPhysicalMinimum(self: *Table, root_slot: u16) DetachedPhysicalMinimum {
        const left = self.physical_nodes[root_slot].left;
        if (left == null) {
            return .{ .root = self.physical_nodes[root_slot].right, .minimum = root_slot };
        }
        const detached = self.detachPhysicalMinimum(left.?);
        self.physical_nodes[root_slot].left = detached.root;
        return .{ .root = self.rebalancePhysical(root_slot), .minimum = detached.minimum };
    }

    fn rebalancePhysical(self: *Table, root_slot: u16) u16 {
        self.updatePhysicalHeight(root_slot);
        const balance = @as(i16, self.physicalHeight(self.physical_nodes[root_slot].left)) -
            @as(i16, self.physicalHeight(self.physical_nodes[root_slot].right));
        if (balance > 1) {
            const left = self.physical_nodes[root_slot].left.?;
            if (self.physicalHeight(self.physical_nodes[left].left) < self.physicalHeight(self.physical_nodes[left].right)) {
                self.physical_nodes[root_slot].left = self.rotatePhysicalLeft(left);
            }
            return self.rotatePhysicalRight(root_slot);
        }
        if (balance < -1) {
            const right = self.physical_nodes[root_slot].right.?;
            if (self.physicalHeight(self.physical_nodes[right].right) < self.physicalHeight(self.physical_nodes[right].left)) {
                self.physical_nodes[root_slot].right = self.rotatePhysicalRight(right);
            }
            return self.rotatePhysicalLeft(root_slot);
        }
        return root_slot;
    }

    fn rotatePhysicalLeft(self: *Table, root_slot: u16) u16 {
        const right = self.physical_nodes[root_slot].right.?;
        const transfer = self.physical_nodes[right].left;
        self.physical_nodes[right].left = root_slot;
        self.physical_nodes[root_slot].right = transfer;
        self.updatePhysicalHeight(root_slot);
        self.updatePhysicalHeight(right);
        return right;
    }

    fn rotatePhysicalRight(self: *Table, root_slot: u16) u16 {
        const left = self.physical_nodes[root_slot].left.?;
        const transfer = self.physical_nodes[left].right;
        self.physical_nodes[left].right = root_slot;
        self.physical_nodes[root_slot].left = transfer;
        self.updatePhysicalHeight(root_slot);
        self.updatePhysicalHeight(left);
        return left;
    }

    fn physicalHeight(self: *const Table, slot: ?u16) u8 {
        return if (slot) |index| self.physical_nodes[index].height else 0;
    }

    fn updatePhysicalHeight(self: *Table, slot: u16) void {
        self.physical_nodes[slot].height = 1 + @max(
            self.physicalHeight(self.physical_nodes[slot].left),
            self.physicalHeight(self.physical_nodes[slot].right),
        );
    }

    fn physicalLess(self: *const Table, left: u16, right: u16) bool {
        const left_base = self.entries[left].phys_base;
        const right_base = self.entries[right].phys_base;
        return left_base < right_base or (left_base == right_base and left < right);
    }

    fn physicalFloor(self: *Table, base: u64) ?usize {
        self.hot_path_stats.physical_lookups +%= 1;
        var current = self.physical_root;
        var result: ?u16 = null;
        var steps: usize = 0;
        while (current) |slot| {
            steps += 1;
            if (self.entries[slot].phys_base <= base) {
                result = slot;
                current = self.physical_nodes[slot].right;
            } else {
                current = self.physical_nodes[slot].left;
            }
        }
        self.recordPhysicalSteps(steps);
        return if (result) |slot| slot else null;
    }

    fn physicalLowerBound(self: *Table, base: u64) ?usize {
        self.hot_path_stats.physical_lookups +%= 1;
        var current = self.physical_root;
        var result: ?u16 = null;
        var steps: usize = 0;
        while (current) |slot| {
            steps += 1;
            if (self.entries[slot].phys_base >= base) {
                result = slot;
                current = self.physical_nodes[slot].left;
            } else {
                current = self.physical_nodes[slot].right;
            }
        }
        self.recordPhysicalSteps(steps);
        return if (result) |slot| slot else null;
    }

    fn physicalPredecessorBefore(self: *Table, base: u64, ignore_id: u32) ?usize {
        self.hot_path_stats.physical_lookups +%= 1;
        var current = self.physical_root;
        var result: ?u16 = null;
        var steps: usize = 0;
        while (current) |slot| {
            steps += 1;
            if (self.entries[slot].phys_base < base) {
                result = slot;
                current = self.physical_nodes[slot].right;
            } else {
                current = self.physical_nodes[slot].left;
            }
        }
        if (result) |slot| {
            if (ignore_id != 0 and self.entries[slot].id == ignore_id) result = self.physicalPredecessor(slot);
        }
        self.recordPhysicalSteps(steps);
        return if (result) |slot| slot else null;
    }

    fn physicalPredecessor(self: *Table, target: u16) ?u16 {
        var current = self.physical_root;
        var result: ?u16 = null;
        while (current) |slot| {
            if (self.physicalLess(slot, target)) {
                result = slot;
                current = self.physical_nodes[slot].right;
            } else {
                current = self.physical_nodes[slot].left;
            }
        }
        return result;
    }

    fn physicalSuccessor(self: *Table, target_index: usize) ?usize {
        const target: u16 = @intCast(target_index);
        self.hot_path_stats.physical_lookups +%= 1;
        var current = self.physical_root;
        var result: ?u16 = null;
        var steps: usize = 0;
        while (current) |slot| {
            steps += 1;
            if (self.physicalLess(target, slot)) {
                result = slot;
                current = self.physical_nodes[slot].left;
            } else {
                current = self.physical_nodes[slot].right;
            }
        }
        self.recordPhysicalSteps(steps);
        return if (result) |slot| slot else null;
    }

    fn physicalMinimum(self: *const Table, root: ?u16) ?u16 {
        var current = root orelse return null;
        while (self.physical_nodes[current].left) |left| current = left;
        return current;
    }

    fn physicalOverlap(self: *Table, base: u64, end: u64, ignore_id: u32) bool {
        if (self.physicalFloor(base)) |floor_index| {
            const block = self.entries[floor_index];
            if (block.id != ignore_id) {
                const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return true;
                if (base < block_end and block.phys_base < end) return true;
            }
        }
        var next = self.physicalLowerBound(base);
        if (next) |index| {
            if (self.entries[index].id == ignore_id) next = self.physicalSuccessor(index);
        }
        if (next) |index| {
            const block = self.entries[index];
            const block_end = checkedEnd(block.phys_base, block.phys_len) orelse return true;
            if (base < block_end and block.phys_base < end) return true;
        }
        return false;
    }

    fn rebuildIndexes(self: *Table) void {
        self.hot_path_stats.physical_rebuilds +%= 1;
        var i: usize = 0;
        while (i < self.physical_nodes.len) : (i += 1) self.physical_nodes[i] = .{};
        i = 0;
        while (i < self.id_index.len) : (i += 1) self.id_index[i] = .{};
        i = 0;
        while (i < self.free_slot_words.len) : (i += 1) self.free_slot_words[i] = 0;
        self.physical_root = null;
        self.physical_entry_count = 0;
        self.id_index_entries = 0;
        self.id_index_tombstones = 0;
        self.next_free_slot_word = 0;
        i = 0;
        while (i < self.entries.len) : (i += 1) {
            const entry = self.entries[i];
            self.setSlotReusable(i, !entry.slot_used or entry.status == .released);
            if (entry.slot_used) self.idIndexInsert(entry.id, @intCast(i));
            if (usesPhysicalIndex(entry)) self.physicalInsert(@intCast(i));
        }
    }

    const PhysicalValidation = struct {
        valid: bool = true,
        count: u32 = 0,
        height: u8 = 0,
        minimum: ?u16 = null,
        maximum: ?u16 = null,
    };

    fn indexInvariant(self: *const Table) bool {
        const physical = self.validatePhysicalTree(self.physical_root, 0);
        if (!physical.valid or physical.count != self.physical_entry_count) return false;
        var physical_entries: u32 = 0;
        var id_entries: u32 = 0;
        var i: usize = 0;
        while (i < self.entries.len) : (i += 1) {
            const entry = self.entries[i];
            const word = self.free_slot_words[i / 64];
            const bit: u6 = @intCast(i % 64);
            const reusable = (word & (@as(u64, 1) << bit)) != 0;
            if (reusable != (!entry.slot_used or entry.status == .released)) return false;
            if (entry.slot_used) {
                id_entries +|= 1;
                if (self.idIndexFindRaw(entry.id) != i) return false;
            }
            if (usesPhysicalIndex(entry)) {
                physical_entries +|= 1;
                if (!self.physical_nodes[i].in_tree) return false;
            } else if (self.physical_nodes[i].in_tree) {
                return false;
            }
        }
        return physical_entries == self.physical_entry_count and id_entries == self.id_index_entries;
    }

    fn validatePhysicalTree(self: *const Table, root: ?u16, depth: u8) PhysicalValidation {
        const slot = root orelse return .{};
        if (depth > 64 or !self.physical_nodes[slot].in_tree or !usesPhysicalIndex(self.entries[slot])) return .{ .valid = false };
        const left = self.validatePhysicalTree(self.physical_nodes[slot].left, depth + 1);
        if (!left.valid) return .{ .valid = false };
        const right = self.validatePhysicalTree(self.physical_nodes[slot].right, depth + 1);
        if (!right.valid) return .{ .valid = false };
        if (left.maximum) |left_max| {
            if (!self.physicalLess(left_max, slot)) return .{ .valid = false };
            const left_end = checkedEnd(self.entries[left_max].phys_base, self.entries[left_max].phys_len) orelse return .{ .valid = false };
            if (left_end > self.entries[slot].phys_base) return .{ .valid = false };
        }
        if (right.minimum) |right_min| {
            if (!self.physicalLess(slot, right_min)) return .{ .valid = false };
            const slot_end = checkedEnd(self.entries[slot].phys_base, self.entries[slot].phys_len) orelse return .{ .valid = false };
            if (slot_end > self.entries[right_min].phys_base) return .{ .valid = false };
        }
        const expected_height: u8 = 1 + @max(left.height, right.height);
        if (self.physical_nodes[slot].height != expected_height) return .{ .valid = false };
        const balance = @as(i16, left.height) - @as(i16, right.height);
        if (balance < -1 or balance > 1) return .{ .valid = false };
        return .{
            .count = left.count +| right.count +| 1,
            .height = expected_height,
            .minimum = left.minimum orelse slot,
            .maximum = right.maximum orelse slot,
        };
    }

    fn idIndexFindRaw(self: *const Table, id: u32) ?usize {
        var position = idIndexStart(id);
        var steps: usize = 0;
        while (steps < self.id_index.len) : (steps += 1) {
            const entry = self.id_index[position];
            if (entry.state == .empty) return null;
            if (entry.state == .used and entry.id == id) return entry.slot;
            position = (position + 1) % self.id_index.len;
        }
        return null;
    }

    fn recordPhysicalSteps(self: *Table, steps: usize) void {
        const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
        self.hot_path_stats.physical_steps +%= value;
        if (value > self.hot_path_stats.physical_step_max) self.hot_path_stats.physical_step_max = value;
    }

    fn recordIdIndexSteps(self: *Table, steps: usize) void {
        const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
        self.hot_path_stats.id_index_steps +%= value;
        if (value > self.hot_path_stats.id_index_step_max) self.hot_path_stats.id_index_step_max = value;
    }

    fn recordFreeSlotSteps(self: *Table, steps: usize) void {
        const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
        self.hot_path_stats.free_slot_word_steps +%= value;
        if (value > self.hot_path_stats.free_slot_word_step_max) self.hot_path_stats.free_slot_word_step_max = value;
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
        target.rebuildIndexes();
    }
};

var storage: [MAX_BLOCKS]MemoryBlock = .{MemoryBlock{}} ** MAX_BLOCKS;
var table: Table = .{ .entries = storage[0..] };
var initialized = false;

pub fn initFromBootInfo() bool {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
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
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
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
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    return table.claimPhysicalRange(base, len, kind, owner, owner_id, name);
}

/// Returns the prefix, starting at `base`, which belongs to one canonical
/// free physical block. VM extent allocation uses this read-only preflight to
/// keep a PMM batch inside the same metadata transaction.
pub fn freePhysicalPrefix(base: u64, max_len: u64) u64 {
    if (!initialized) return 0;
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    return table.freePhysicalPrefix(base, max_len);
}

/// Returns the prefix, starting at `base`, which belongs to one claimed
/// physical block. This bounds range release plans without a second tree
/// walk for each page.
pub fn claimedPhysicalPrefix(base: u64, max_len: u64) u64 {
    if (!initialized) return 0;
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    return table.claimedPhysicalPrefix(base, max_len);
}

pub fn releasePhysicalRange(base: u64, len: u64) Error!void {
    var plan: PhysicalReleasePlan = undefined;
    try preparePhysicalRangeRelease(base, len, &plan);
    finishPhysicalRangeRelease(&plan, true);
}

pub fn preparePhysicalRangeRelease(base: u64, len: u64, plan: *PhysicalReleasePlan) Error!void {
    if (!initialized) return Error.NotInitialized;
    const lock_token = owner_locks.physical_memory.acquire();
    table.preparePhysicalRangeRelease(base, len, plan) catch |err| {
        owner_locks.physical_memory.release(lock_token);
        return err;
    };
    plan.lock_token = lock_token;
    plan.active = true;
}

// Applies a previously validated plan after the caller has unmapped the PTE.
// There are deliberately no failure paths here: every output slot, merge and
// block ID is protected by the physical-metadata owner lock acquired in
// preparePhysicalRangeRelease().  The token deliberately spans the external
// PTE mutation, so no competing allocation can observe half a release.
pub fn commitPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    finishPhysicalRangeRelease(plan, false);
}

pub fn cancelPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    if (!plan.active) return;
    plan.active = false;
    owner_locks.physical_memory.release(plan.lock_token);
}

fn finishPhysicalRangeRelease(plan: *PhysicalReleasePlan, coalesce_now: bool) void {
    if (!plan.active) unreachable;
    table.commitPhysicalRangeRelease(plan.*, coalesce_now);
    plan.active = false;
    owner_locks.physical_memory.release(plan.lock_token);
}

// Compatibility entrypoint for callers that do not need to hold the plan
// across an external operation. VM teardown uses prepare+commit directly and
// therefore never repeats the full-table preflight after unmapping a page.
pub fn releasePhysicalRangeDeferred(base: u64, len: u64) Error!void {
    var plan: PhysicalReleasePlan = undefined;
    try preparePhysicalRangeRelease(base, len, &plan);
    finishPhysicalRangeRelease(&plan, false);
}

pub fn coalescePhysicalRanges() void {
    if (!initialized) return;
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
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
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.retagPhysicalRange(base, len, kind, owner, owner_id, status, name);
}

pub fn update(id: u32, req: UpdateRequest) Error!void {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.update(id, req);
}

pub fn setCommitted(id: u32, bytes: u64) Error!void {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.setCommitted(id, bytes);
}

pub fn commit(id: u32, bytes: u64) Error!void {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.commit(id, bytes);
}

pub fn uncommit(id: u32, bytes: u64) Error!void {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.uncommit(id, bytes);
}

pub fn release(id: u32) Error!void {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return Error.NotInitialized;
    return table.release(id);
}

pub fn snapshot(out: []MemoryBlock) SnapshotResult {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return .{};
    return table.snapshot(out);
}

pub fn firstByOwner(owner: Owner, owner_id: u64) ?MemoryBlock {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return null;
    return table.firstByOwner(owner, owner_id);
}

pub fn firstContainingVirtual(addr: u64) ?MemoryBlock {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return null;
    return table.firstContainingVirtual(addr);
}

pub fn countByOwner(owner: Owner, owner_id: u64) u64 {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return 0;
    return table.countByOwner(owner, owner_id);
}

pub fn activeAt(active_index: u32) ?MemoryBlock {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return null;
    return table.activeAt(active_index);
}

pub fn summary() Summary {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return .{};
    return table.summary();
}

pub fn hotPathStats() HotPathStats {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    if (!initialized) return .{};
    var result = table.hot_path_stats;
    result.physical_index_entries = table.physical_entry_count;
    result.id_index_entries = table.id_index_entries;
    return result;
}

pub fn indexInvariant() bool {
    const lock_token = owner_locks.physical_memory.acquire();
    defer owner_locks.physical_memory.release(lock_token);
    return initialized and table.indexInvariant();
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

fn idIndexStart(id: u32) usize {
    const mixed = @as(u64, id) *% 11400714819323198485;
    return @intCast(mixed % ID_INDEX_CAPACITY);
}

fn usesPhysicalIndex(block: MemoryBlock) bool {
    return block.active() and block.phys_len != 0;
}

fn isCanonicalFree(block: MemoryBlock) bool {
    return block.active() and
        block.kind == .free and
        block.owner == .system and
        block.owner_id == 0 and
        block.status == .free and
        block.virt_len == 0 and
        strEq(block.name, "free");
}

fn canMergePhysicalRequest(block: MemoryBlock, req: RegisterRequest) bool {
    return block.active() and
        block.virt_len == 0 and
        block.phys_len != 0 and
        block.kind == req.kind and
        block.owner == req.owner and
        block.owner_id == req.owner_id and
        block.status == req.status and
        strEq(block.name, req.name);
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
