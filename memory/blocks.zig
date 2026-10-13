const k = @import("../kernel/log.zig");
const owner_locks = @import("owner_locks.zig");

pub const MAX_BLOCKS: usize = 8192;
pub const KIND_COUNT: usize = 14;
pub const OWNER_COUNT: usize = 8;
pub const STATUS_COUNT: usize = 7;
pub const ID_INDEX_CAPACITY: usize = MAX_BLOCKS * 2;

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

pub const SnapshotResult = extern struct {
    copied: usize = 0,
    total: usize = 0,
    truncated: bool = false,
};

pub const Summary = extern struct {
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

pub const HotPathStats = extern struct {
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

// The catalogue and all three indices have one Rust owner. This facade only
// marshals the not-yet-migrated callers' native slices/options to private C.
const RawBlock = extern struct {
    slot_used: bool = false,
    id: u32 = 0,
    kind: u8 = @intFromEnum(Kind.unknown),
    owner: u8 = @intFromEnum(Owner.system),
    status: u8 = @intFromEnum(Status.free),
    owner_id: u64 = 0,
    name_ptr: [*]const u8 = "",
    name_len: usize = 0,
    phys_base: u64 = 0,
    phys_len: u64 = 0,
    virt_base: u64 = 0,
    virt_len: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
};
const RawUpdate = extern struct {
    mask: u32 = 0,
    kind: u8 = 0,
    owner: u8 = 0,
    status: u8 = 0,
    owner_id: u64 = 0,
    name_ptr: [*]const u8 = "",
    name_len: usize = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
};

// Values and the exact CPU-bound token only; no Rust reference is retained
// over the caller's PTE change and generation-checked TLB completion.
pub const PhysicalReleasePlan = extern struct {
    target_index: usize,
    target_released: RawBlock,
    write_indices: [3]usize,
    write_entries: [3]RawBlock,
    write_count: u8,
    merge_primary_index: usize,
    merge_primary_entry: RawBlock,
    merge_secondary_index: usize,
    merge_secondary_released: RawBlock,
    next_id_after: u32,
    lock_token: owner_locks.Token,
    active: bool,
};
extern fn r4os_rust_blocks_init() bool;
extern fn r4os_rust_blocks_register(req: *const RawBlock, out: *u32) u32;
extern fn r4os_rust_blocks_range(req: *const RawBlock, retag: bool, out: *u32) u32;
extern fn r4os_rust_blocks_prefix(base: u64, max_len: u64, free: bool) u64;
extern fn r4os_rust_blocks_prepare(base: u64, len: u64, plan: *PhysicalReleasePlan) u32;
extern fn r4os_rust_blocks_finish(plan: *PhysicalReleasePlan, coalesce: bool, cancel: bool) void;
extern fn r4os_rust_blocks_change(id: u32, operation: u8, bytes: u64, req: ?*const RawUpdate) u32;
extern fn r4os_rust_blocks_coalesce() void;
extern fn r4os_rust_blocks_read(operation: u8, owner: u8, number: u64, out: *RawBlock) bool;
extern fn r4os_rust_blocks_count(owner: u8, owner_id: u64) u64;
extern fn r4os_rust_blocks_snapshot(context: ?*anyopaque, capacity: usize, out: *SnapshotResult) void;
extern fn r4os_rust_blocks_summary(out: *Summary) void;
extern fn r4os_rust_blocks_stats(out: *HotPathStats) void;
extern fn r4os_rust_blocks_invariant() bool;

fn check(code: u32) Error!void {
    return switch (code) {
        0 => {},
        1 => Error.NotInitialized,
        2 => Error.TableFull,
        3 => Error.EmptyRange,
        4 => Error.Overlap,
        5 => Error.NotFound,
        6 => Error.NotFree,
        7 => Error.InvalidBytes,
        8 => Error.InvalidRange,
        9 => Error.Overflow,
        else => unreachable,
    };
}
fn requestRaw(req: RegisterRequest) RawBlock {
    return .{
        .kind = @intFromEnum(req.kind),
        .owner = @intFromEnum(req.owner),
        .owner_id = req.owner_id,
        .status = @intFromEnum(req.status),
        .name_ptr = req.name.ptr,
        .name_len = req.name.len,
        .phys_base = req.phys_base,
        .phys_len = req.phys_len,
        .virt_base = req.virt_base,
        .virt_len = req.virt_len,
        .reserved_bytes = req.reserved_bytes,
        .committed_bytes = req.committed_bytes,
    };
}
fn blockNative(raw: RawBlock) MemoryBlock {
    return .{
        .slot_used = raw.slot_used,
        .id = raw.id,
        .kind = @enumFromInt(raw.kind),
        .owner = @enumFromInt(raw.owner),
        .owner_id = raw.owner_id,
        .status = @enumFromInt(raw.status),
        .name = raw.name_ptr[0..raw.name_len],
        .phys_base = raw.phys_base,
        .phys_len = raw.phys_len,
        .virt_base = raw.virt_base,
        .virt_len = raw.virt_len,
        .reserved_bytes = raw.reserved_bytes,
        .committed_bytes = raw.committed_bytes,
    };
}
pub fn initFromBootInfo() bool {
    return r4os_rust_blocks_init();
}
pub fn register(req: RegisterRequest) Error!u32 {
    const raw = requestRaw(req);
    var id: u32 = undefined;
    try check(r4os_rust_blocks_register(&raw, &id));
    return id;
}
pub fn claimPhysicalRange(base: u64, len: u64, kind: Kind, owner: Owner, owner_id: u64, name: []const u8) Error!u32 {
    const raw = requestRaw(.{ .phys_base = base, .phys_len = len, .kind = kind, .owner = owner, .owner_id = owner_id, .name = name });
    var id: u32 = undefined;
    try check(r4os_rust_blocks_range(&raw, false, &id));
    return id;
}
pub fn freePhysicalPrefix(base: u64, max_len: u64) u64 {
    return r4os_rust_blocks_prefix(base, max_len, true);
}
pub fn claimedPhysicalPrefix(base: u64, max_len: u64) u64 {
    return r4os_rust_blocks_prefix(base, max_len, false);
}
pub fn preparePhysicalRangeRelease(base: u64, len: u64, plan: *PhysicalReleasePlan) Error!void {
    try check(r4os_rust_blocks_prepare(base, len, plan));
}
pub fn commitPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    r4os_rust_blocks_finish(plan, false, false);
}
pub fn cancelPhysicalRangeRelease(plan: *PhysicalReleasePlan) void {
    r4os_rust_blocks_finish(plan, false, true);
}
pub fn releasePhysicalRange(base: u64, len: u64) Error!void {
    var plan: PhysicalReleasePlan = undefined;
    try preparePhysicalRangeRelease(base, len, &plan);
    r4os_rust_blocks_finish(&plan, true, false);
}
pub fn releasePhysicalRangeDeferred(base: u64, len: u64) Error!void {
    var plan: PhysicalReleasePlan = undefined;
    try preparePhysicalRangeRelease(base, len, &plan);
    commitPhysicalRangeRelease(&plan);
}
pub fn coalescePhysicalRanges() void {
    r4os_rust_blocks_coalesce();
}
pub fn retagPhysicalRange(base: u64, len: u64, kind: Kind, owner: Owner, owner_id: u64, status: Status, name: []const u8) Error!u32 {
    const raw = requestRaw(.{ .phys_base = base, .phys_len = len, .kind = kind, .owner = owner, .owner_id = owner_id, .status = status, .name = name });
    var id: u32 = undefined;
    try check(r4os_rust_blocks_range(&raw, true, &id));
    return id;
}
pub fn update(id: u32, req: UpdateRequest) Error!void {
    var raw: RawUpdate = .{};
    if (req.kind) |value| {
        raw.mask |= 1;
        raw.kind = @intFromEnum(value);
    }
    if (req.owner) |value| {
        raw.mask |= 2;
        raw.owner = @intFromEnum(value);
    }
    if (req.owner_id) |value| {
        raw.mask |= 4;
        raw.owner_id = value;
    }
    if (req.status) |value| {
        raw.mask |= 8;
        raw.status = @intFromEnum(value);
    }
    if (req.name) |value| {
        raw.mask |= 16;
        raw.name_ptr = value.ptr;
        raw.name_len = value.len;
    }
    if (req.reserved_bytes) |value| {
        raw.mask |= 32;
        raw.reserved_bytes = value;
    }
    if (req.committed_bytes) |value| {
        raw.mask |= 64;
        raw.committed_bytes = value;
    }
    try check(r4os_rust_blocks_change(id, 0, 0, &raw));
}
pub fn setCommitted(id: u32, bytes: u64) Error!void {
    try check(r4os_rust_blocks_change(id, 1, bytes, null));
}
pub fn commit(id: u32, bytes: u64) Error!void {
    try check(r4os_rust_blocks_change(id, 2, bytes, null));
}
pub fn uncommit(id: u32, bytes: u64) Error!void {
    try check(r4os_rust_blocks_change(id, 3, bytes, null));
}
pub fn release(id: u32) Error!void {
    try check(r4os_rust_blocks_change(id, 4, 0, null));
}
export fn r4os_zig_blocks_snapshot_write(context: ?*anyopaque, index: usize, raw: *const RawBlock) callconv(.c) void {
    // Bounded conversion to the caller's existing output: no allocation,
    // blocking operation, owner reentry or retained Rust pointer.
    const out: [*]MemoryBlock = @ptrCast(@alignCast(context.?));
    out[index] = blockNative(raw.*);
}
pub fn snapshot(out: []MemoryBlock) SnapshotResult {
    var result: SnapshotResult = undefined;
    r4os_rust_blocks_snapshot(out.ptr, out.len, &result);
    return result;
}
fn readBlock(operation: u8, owner: u8, number: u64) ?MemoryBlock {
    var raw: RawBlock = undefined;
    return if (r4os_rust_blocks_read(operation, owner, number, &raw)) blockNative(raw) else null;
}
pub fn firstByOwner(owner: Owner, owner_id: u64) ?MemoryBlock {
    return readBlock(0, @intFromEnum(owner), owner_id);
}
pub fn firstContainingVirtual(addr: u64) ?MemoryBlock {
    return readBlock(1, 0, addr);
}
pub fn countByOwner(owner: Owner, owner_id: u64) u64 {
    return r4os_rust_blocks_count(@intFromEnum(owner), owner_id);
}
pub fn activeAt(active_index: u32) ?MemoryBlock {
    return readBlock(2, 0, active_index);
}
pub fn summary() Summary {
    var result: Summary = undefined;
    r4os_rust_blocks_summary(&result);
    return result;
}
pub fn hotPathStats() HotPathStats {
    var result: HotPathStats = undefined;
    r4os_rust_blocks_stats(&result);
    return result;
}
pub fn indexInvariant() bool {
    return r4os_rust_blocks_invariant();
}
comptime {
    if (@sizeOf(RawBlock) != 88 or @offsetOf(RawBlock, "owner_id") != 16 or
        @offsetOf(RawBlock, "phys_base") != 40 or @sizeOf(RawUpdate) != 48 or
        @sizeOf(SnapshotResult) != 24 or @sizeOf(Summary) != 328 or
        @sizeOf(HotPathStats) != 120 or @sizeOf(PhysicalReleasePlan) != 616 or
        @offsetOf(PhysicalReleasePlan, "lock_token") != 592)
        @compileError("Rust memory catalogue C layout mismatch");
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
