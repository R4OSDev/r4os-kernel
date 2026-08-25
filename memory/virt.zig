const std = @import("std");
const blocks = @import("blocks.zig");
const backing_store = @import("backing_store.zig");
const paging = @import("paging.zig");
const phys = @import("phys.zig");
const reclaim = @import("reclaim.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const k = @import("../kernel/log.zig");
const r4sys_api = @import("../program/r4sys.zig");

pub const MAX_RANGES: usize = 1024;
pub const MAX_COMMIT_SPANS: usize = 4096;
pub const MAX_PAGE_STATE_SPANS: usize = 8192;
pub const WINDOW_COUNT: usize = 8;
pub const RANGE_ID_INDEX_CAPACITY: usize = MAX_RANGES * 2;
const INVALID_RANGE_POSITION: u16 = std.math.maxInt(u16);

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;
const TB: u64 = 1024 * GB;

const TEMP_KERNEL_BASE: u64 = 0xFFFF_FF00_0000_0000;
const KERNEL_HEAP_BASE: u64 = 0xFFFF_FF10_0000_0000;
const PROGRAM_IMAGE_BASE: u64 = 0xFFFF_FF20_0000_0000;
const APP_HEAP_BASE: u64 = 0xFFFF_FF30_0000_0000;
const APP_STACK_BASE: u64 = 0xFFFF_FF40_0000_0000;
const MMIO_BASE: u64 = 0xFFFF_FF50_0000_0000;
const KERNEL_STACK_BASE: u64 = 0xFFFF_FF60_0000_0000;
const R4X_VM_BASE: u64 = 0xFFFF_FB00_0000_0000;

const APP_SYSTEM_RESERVE_FRAMES: u64 = 384;
const PAGE_FAULT_PRESENT: u64 = 1 << 0;
pub const R4X_VM_PROBE_BYTES: u64 = 4 * GB;
pub const page_state_probe_version: u32 = 1;

pub const page_state_operation_query: u32 = 0;
pub const page_state_operation_mark_dirty: u32 = 1;
pub const page_state_operation_mark_clean: u32 = 2;
pub const page_state_operation_bind_slot: u32 = 3;
pub const page_state_operation_clear_slot: u32 = 4;
pub const page_state_operation_mark_pinned: u32 = 5;
pub const page_state_operation_clear_pinned: u32 = 6;
pub const page_state_operation_mark_busy: u32 = 7;
pub const page_state_operation_clear_busy: u32 = 8;
pub const page_state_operation_mark_error: u32 = 9;
pub const page_state_operation_clear_error: u32 = 10;

pub const page_state_status_unavailable: u32 = 0;
pub const page_state_status_ready: u32 = 1;
pub const page_state_status_invalid_request: u32 = 2;
pub const page_state_status_region_missing: u32 = 3;
pub const page_state_status_region_not_r4x: u32 = 4;
pub const page_state_status_unaligned_request: u32 = 5;
pub const page_state_status_outside_commit: u32 = 6;
pub const page_state_status_table_full: u32 = 7;
pub const page_state_status_not_initialized: u32 = 8;
pub const page_state_status_unsupported_operation: u32 = 9;
pub const page_state_status_unsupported_flags: u32 = 10;

pub const page_state_flag_committed: u32 = 1 << 0;
pub const page_state_flag_resident: u32 = 1 << 1;
pub const page_state_flag_dirty: u32 = 1 << 2;
pub const page_state_flag_pinned: u32 = 1 << 3;
pub const page_state_flag_busy: u32 = 1 << 4;
pub const page_state_flag_error: u32 = 1 << 5;
pub const page_state_flag_slot_bound: u32 = 1 << 6;
pub const page_state_flag_hardware_dirty_synced: u32 = 1 << 16;
pub const page_state_flag_vm_owned_state: u32 = 1 << 17;
pub const page_state_flag_explicit_request: u32 = 1 << 18;
pub const page_state_flag_no_eviction: u32 = 1 << 19;
pub const page_state_flag_no_swap: u32 = 1 << 20;
pub const page_state_flag_no_fault_io: u32 = 1 << 21;
pub const page_state_flag_page_sized: u32 = 1 << 22;
pub const page_state_flag_fault_page_in: u32 = 1 << 23;
pub const page_state_flag_eviction_enabled: u32 = 1 << 24;

pub const page_state_blocker_invalid_request: u32 = 1 << 0;
pub const page_state_blocker_unsupported_flags: u32 = 1 << 1;
pub const page_state_blocker_region_missing: u32 = 1 << 2;
pub const page_state_blocker_region_not_r4x: u32 = 1 << 3;
pub const page_state_blocker_unaligned_request: u32 = 1 << 4;
pub const page_state_blocker_outside_commit: u32 = 1 << 5;
pub const page_state_blocker_table_full: u32 = 1 << 6;
pub const page_state_blocker_not_initialized: u32 = 1 << 7;
pub const page_state_blocker_unsupported_operation: u32 = 1 << 8;
pub const page_state_supported_flags: u32 = 0;

pub const Window = enum(u8) {
    temp_kernel = 0,
    kernel_heap = 1,
    program_image = 2,
    app_heap = 3,
    app_stack = 4,
    mmio = 5,
    r4x_vm = 6,
    // 0.56.15: Kernel-Task-Stacks mit Guard-Page (sched/task.zig).
    kernel_stack = 7,
};

pub const Error = error{
    NotInitialized,
    TableFull,
    EmptyRange,
    BadAlignment,
    Overflow,
    NoSpace,
    Overlap,
    NotFound,
    AlreadyCommitted,
    NotCommitted,
    GuardRange,
    OutOfMemory,
    MapFailed,
    OutsideWindow,
};

pub const ReserveRequest = struct {
    window: Window,
    len: u64,
    alignment: u64 = paging.PAGE_SIZE,
    kind: blocks.Kind = .virtual_range,
    owner: blocks.Owner = .kernel,
    owner_id: u64 = 0,
    name: []const u8 = "virtual-range",
    flags: u64 = paging.WRITABLE | paging.NO_EXECUTE,
};

pub const ReserveAtRequest = struct {
    window: Window,
    base: u64,
    len: u64,
    kind: blocks.Kind = .virtual_range,
    owner: blocks.Owner = .kernel,
    owner_id: u64 = 0,
    name: []const u8 = "virtual-range",
    flags: u64 = paging.WRITABLE | paging.NO_EXECUTE,
};

pub const RangeInfo = struct {
    id: u32 = 0,
    window: Window = .temp_kernel,
    kind: blocks.Kind = .virtual_range,
    owner: blocks.Owner = .kernel,
    owner_id: u64 = 0,
    status: blocks.Status = .reserved,
    name: []const u8 = "",
    flags: u64 = paging.WRITABLE | paging.NO_EXECUTE,
    base: u64 = 0,
    len: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    peak_resident_bytes: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    guard_base: u64 = 0,
    guard_len: u64 = 0,
};

pub const Stats = struct {
    active_ranges: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    peak_resident_bytes: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    guard_bytes: u64 = 0,
    largest_free_virtual_base: u64 = 0,
    largest_free_virtual_len: u64 = 0,
    app_system_reserve_frames: u64 = APP_SYSTEM_RESERVE_FRAMES,
    app_available_frames: u64 = 0,
    overflow: bool = false,
};

pub const OwnerStats = struct {
    active_ranges: u64 = 0,
    reserved_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    peak_resident_bytes: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    overflow: bool = false,
};

pub const HotPathStats = struct {
    range_index_capacity: u32 = @intCast(RANGE_ID_INDEX_CAPACITY),
    range_index_entries: u32 = 0,
    range_index_tombstones: u32 = 0,
    range_index_lookups: u64 = 0,
    range_index_hits: u64 = 0,
    range_index_misses: u64 = 0,
    range_index_probe_total: u64 = 0,
    range_index_probe_max: u32 = 0,
    range_index_probe_last: u32 = 0,
    range_index_rebuilds: u64 = 0,
    range_index_insert_failures: u64 = 0,
    range_free_slot_lookups: u64 = 0,
    range_free_slot_probe_total: u64 = 0,
    range_free_slot_probe_max: u32 = 0,
    range_free_slot_probe_last: u32 = 0,
    range_address_entries: u32 = 0,
    range_address_lookups: u64 = 0,
    range_address_probe_total: u64 = 0,
    range_address_probe_max: u32 = 0,
    range_address_probe_last: u32 = 0,
    commit_span_active: u32 = 0,
    commit_span_lookups: u64 = 0,
    commit_span_steps: u64 = 0,
    commit_span_step_max: u32 = 0,
    page_state_span_active: u32 = 0,
    page_state_span_lookups: u64 = 0,
    page_state_span_steps: u64 = 0,
    page_state_span_step_max: u32 = 0,
    reclaim_cursor_range_steps: u64 = 0,
    reclaim_cursor_span_steps: u64 = 0,
    reclaim_cursor_page_steps: u64 = 0,
    reclaim_cursor_wraps: u64 = 0,
};

pub const PageStateInput = struct {
    operation: u32 = page_state_operation_query,
    region_id: u32 = 0,
    region_offset: u64 = 0,
    page_count: u64 = 1,
    slot_reservation_id: u32 = 0,
    slot_index: u64 = 0,
    slot_generation: u64 = 0,
    flags: u32 = 0,
};

pub const PageStateResult = struct {
    version: u32 = page_state_probe_version,
    status: u32 = page_state_status_unavailable,
    operation: u32 = page_state_operation_query,
    flags: u32 = 0,
    blockers: u32 = 0,
    region_id: u32 = 0,
    page_size: u32 = 4096,
    max_spans: u32 = MAX_PAGE_STATE_SPANS,
    span_count: u32 = 0,
    region_offset: u64 = 0,
    page_count: u64 = 0,
    committed_pages: u64 = 0,
    resident_pages: u64 = 0,
    nonresident_pages: u64 = 0,
    dirty_pages: u64 = 0,
    clean_pages: u64 = 0,
    pinned_pages: u64 = 0,
    busy_pages: u64 = 0,
    error_pages: u64 = 0,
    slot_bound_pages: u64 = 0,
    slot_reservation_id: u32 = 0,
    page_count_lo: u32 = 0,
    slot_index: u64 = 0,
    slot_generation: u64 = 0,
    total_transitions: u64 = 0,
    dirty_marks: u64 = 0,
    clean_marks: u64 = 0,
    slot_binds: u64 = 0,
    slot_clears: u64 = 0,
    pinned_marks: u64 = 0,
    pinned_clears: u64 = 0,
    busy_marks: u64 = 0,
    busy_clears: u64 = 0,
    error_marks: u64 = 0,
    error_clears: u64 = 0,
    table_full_failures: u64 = 0,
    cleanup_pages: u64 = 0,
};

pub const PageStateSummary = struct {
    enabled: bool = true,
    last_status: u32 = page_state_status_unavailable,
    last_operation: u32 = page_state_operation_query,
    last_flags: u32 = 0,
    last_blockers: u32 = 0,
    last_region_id: u32 = 0,
    last_region_offset: u64 = 0,
    last_page_count: u64 = 0,
    last_slot_reservation_id: u32 = 0,
    last_slot_index: u64 = 0,
    last_slot_generation: u64 = 0,
    committed_pages: u64 = 0,
    resident_pages: u64 = 0,
    nonresident_pages: u64 = 0,
    dirty_pages: u64 = 0,
    clean_pages: u64 = 0,
    pinned_pages: u64 = 0,
    busy_pages: u64 = 0,
    error_pages: u64 = 0,
    fault_page_ins: u64 = 0,
    fault_page_in_failures: u64 = 0,
    page_out_nonresident_pages: u64 = 0,
    eviction_attempts: u64 = 0,
    eviction_successes: u64 = 0,
    eviction_failures: u64 = 0,
    eviction_candidates: u64 = 0,
    eviction_page_outs: u64 = 0,
    eviction_clean_pages: u64 = 0,
    eviction_dirty_pages: u64 = 0,
    eviction_returned_frames: u64 = 0,
    eviction_no_backing: u64 = 0,
    eviction_no_candidate: u64 = 0,
    eviction_slot_failures: u64 = 0,
    eviction_io_failures: u64 = 0,
    eviction_skipped_nonresident: u64 = 0,
    eviction_skipped_pinned: u64 = 0,
    eviction_skipped_busy: u64 = 0,
    eviction_skipped_error: u64 = 0,
    eviction_skipped_unmapped: u64 = 0,
    pager_failed_page_outs: u64 = 0,
    pager_failed_page_ins: u64 = 0,
    pager_data_preserved_pages: u64 = 0,
    pager_data_lost_pages: u64 = 0,
    pager_dirty_preserved_pages: u64 = 0,
    pager_disabled_eviction_gates: u64 = 0,
    slot_bound_pages: u64 = 0,
    span_count: u32 = 0,
    max_spans: u32 = MAX_PAGE_STATE_SPANS,
    transitions: u64 = 0,
    dirty_marks: u64 = 0,
    clean_marks: u64 = 0,
    slot_binds: u64 = 0,
    slot_clears: u64 = 0,
    pinned_marks: u64 = 0,
    pinned_clears: u64 = 0,
    busy_marks: u64 = 0,
    busy_clears: u64 = 0,
    error_marks: u64 = 0,
    error_clears: u64 = 0,
    table_full_failures: u64 = 0,
    cleanup_pages: u64 = 0,
};

const WindowDef = struct {
    base: u64,
    len: u64,
    name: []const u8,
};

const FreeSpan = struct {
    base: u64,
    len: u64,
};

const CommitSpan = struct {
    slot_used: bool = false,
    range_id: u32 = 0,
    base: u64 = 0,
    len: u64 = 0,
    prev: ?u16 = null,
    next: ?u16 = null,
};

const PageStateSpan = struct {
    slot_used: bool = false,
    range_id: u32 = 0,
    first_page: u64 = 0,
    page_count: u64 = 0,
    flags: u32 = 0,
    slot_reservation_id: u32 = 0,
    slot_index: u64 = 0,
    slot_generation: u64 = 0,
    prev: ?u16 = null,
    next: ?u16 = null,
};

const FaultPageState = struct {
    flags: u32 = 0,
    slot_reservation_id: u32 = 0,
    slot_index: u64 = 0,
    slot_generation: u64 = 0,
};

const EvictionOutcome = enum(u8) {
    skipped,
    returned,
    failed,
};

const EvictionStep = struct {
    outcome: EvictionOutcome = .skipped,
    returned_frames: u32 = 0,
    returned_bytes: u64 = 0,
    page_outs: u64 = 0,
    dirty: bool = false,
};

const Range = struct {
    slot_used: bool = false,
    id: u32 = 0,
    window: Window = .temp_kernel,
    kind: blocks.Kind = .virtual_range,
    owner: blocks.Owner = .kernel,
    owner_id: u64 = 0,
    status: blocks.Status = .reserved,
    name: []const u8 = "",
    flags: u64 = paging.WRITABLE | paging.NO_EXECUTE,
    base: u64 = 0,
    len: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    peak_resident_bytes: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    guard_base: u64 = 0,
    guard_len: u64 = 0,
    block_id: u32 = 0,
    // A multi-page uncommit may stop before a later page if its physical
    // metadata cannot be represented. Keep the exact operation so the same
    // caller can resume over already-unmapped pages without lying about the
    // current committed-byte count or losing the range as a retry anchor.
    partial_uncommit_base: u64 = 0,
    partial_uncommit_len: u64 = 0,
    partial_uncommit_cursor: u64 = 0,
    partial_uncommit_accounted: bool = false,
    commit_span_head: ?u16 = null,
    page_state_span_head: ?u16 = null,

    fn active(self: Range) bool {
        return self.slot_used and self.status != .released;
    }
};

const RangeIdIndexState = enum(u8) {
    empty = 0,
    used = 1,
    tombstone = 2,
};

const RangeIdIndexEntry = struct {
    state: RangeIdIndexState = .empty,
    id: u32 = 0,
    slot: usize = 0,
};

const WINDOWS = [_]WindowDef{
    .{ .base = TEMP_KERNEL_BASE, .len = 16 * MB, .name = "temp-kernel" },
    // 0.56.6: 32 MB -> 1 GB virtuelles Fenster. Committed bleibt bedarfsweise
    // und ist in heap.zig RAM-abhaengig gedeckelt; die Fensterbasen liegen
    // 64 GB auseinander, 1 GB kollidiert mit nichts.
    .{ .base = KERNEL_HEAP_BASE, .len = 1 * GB, .name = "kernel-heap" },
    .{ .base = PROGRAM_IMAGE_BASE, .len = 512 * MB, .name = "program-image" },
    .{ .base = APP_HEAP_BASE, .len = 0, .name = "app-heap-retired" },
    // Eight GiB keeps the 8-MiB per-program-thread guard/reserve policy while
    // removing the old 128-thread virtual-address ceiling. The next window is
    // 64 GiB away, so this remains isolated with ample unmapped separation.
    .{ .base = APP_STACK_BASE, .len = 8 * GB, .name = "app-stack" },
    .{ .base = MMIO_BASE, .len = 512 * MB, .name = "mmio" },
    .{ .base = R4X_VM_BASE, .len = 4 * TB, .name = "r4x-vm" },
    .{ .base = KERNEL_STACK_BASE, .len = 64 * MB, .name = "kernel-stack" },
};

var ranges: [MAX_RANGES]Range = .{Range{}} ** MAX_RANGES;
var range_id_index: [RANGE_ID_INDEX_CAPACITY]RangeIdIndexEntry = .{RangeIdIndexEntry{}} ** RANGE_ID_INDEX_CAPACITY;
var range_address_order: [MAX_RANGES]u16 = .{0} ** MAX_RANGES;
var range_address_position: [MAX_RANGES]u16 = .{INVALID_RANGE_POSITION} ** MAX_RANGES;
var commit_spans: [MAX_COMMIT_SPANS]CommitSpan = .{CommitSpan{}} ** MAX_COMMIT_SPANS;
var page_state_spans: [MAX_PAGE_STATE_SPANS]PageStateSpan = .{PageStateSpan{}} ** MAX_PAGE_STATE_SPANS;
var page_state_summary: PageStateSummary = .{};
var range_id_index_entries: usize = 0;
var range_id_index_tombstones: usize = 0;
var range_address_count: usize = 0;
var commit_span_free_head: ?u16 = null;
var commit_span_active_count: u32 = 0;
var page_state_span_free_head: ?u16 = null;
var page_state_span_active_count: u32 = 0;
var reclaim_cursor_range_slot: ?u16 = null;
var reclaim_cursor_page: u64 = 0;
var next_free_range_slot: usize = 0;
var hot_path_stats: HotPathStats = .{};
var next_id: u32 = 1;
var initialized = false;

pub fn init() bool {
    var i: usize = 0;
    while (i < ranges.len) : (i += 1) ranges[i] = .{};
    i = 0;
    while (i < range_id_index.len) : (i += 1) range_id_index[i] = .{};
    i = 0;
    while (i < range_address_position.len) : (i += 1) range_address_position[i] = INVALID_RANGE_POSITION;
    commit_span_free_head = null;
    i = 0;
    while (i < commit_spans.len) : (i += 1) {
        commit_spans[i] = .{ .next = commit_span_free_head };
        commit_span_free_head = @intCast(i);
    }
    page_state_span_free_head = null;
    i = 0;
    while (i < page_state_spans.len) : (i += 1) {
        page_state_spans[i] = .{ .next = page_state_span_free_head };
        page_state_span_free_head = @intCast(i);
    }
    page_state_summary = .{};
    range_id_index_entries = 0;
    range_id_index_tombstones = 0;
    range_address_count = 0;
    commit_span_active_count = 0;
    page_state_span_active_count = 0;
    reclaim_cursor_range_slot = null;
    reclaim_cursor_page = 0;
    next_free_range_slot = 0;
    hot_path_stats = .{};
    next_id = 1;
    initialized = true;
    reclaim.registerVmReclaimer(reclaimEvictFrames);
    return true;
}

pub fn reserve(req: ReserveRequest) Error!u32 {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const len = normalizeLen(req.len) catch |err| return err;
    const alignment = normalizeAlignment(req.alignment) catch |err| return err;
    const base = firstFit(req.window, len, alignment) catch |err| return err;
    return reserveAtInternal(.{
        .window = req.window,
        .base = base,
        .len = len,
        .kind = req.kind,
        .owner = req.owner,
        .owner_id = req.owner_id,
        .name = req.name,
        .flags = req.flags,
    });
}

pub fn reserveAt(req: ReserveAtRequest) Error!u32 {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const len = normalizeLen(req.len) catch |err| return err;
    if (!isAligned(req.base, paging.PAGE_SIZE)) return Error.BadAlignment;
    return reserveAtInternal(.{
        .window = req.window,
        .base = req.base,
        .len = len,
        .kind = req.kind,
        .owner = req.owner,
        .owner_id = req.owner_id,
        .name = req.name,
        .flags = req.flags,
    });
}

pub fn commit(id: u32, offset: u64, len_raw: u64) Error!void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const len = normalizeLen(len_raw) catch |err| return err;
    if (!isAligned(offset, paging.PAGE_SIZE)) return Error.BadAlignment;
    const idx = indexById(id) orelse return Error.NotFound;
    const range = &ranges[idx];
    try validateInside(range.*, offset, len);
    try validateNotGuard(range.*, range.base + offset, len);
    try validateUncommitted(range.*, range.base + offset, len);

    if (range.window == .r4x_vm) {
        const next_committed = checkedAdd(range.committed_bytes, len) orelse return Error.Overflow;
        try addCommitSpan(range.id, range.base + offset, len);
        addPageStateSpan(range.id, offset / paging.PAGE_SIZE, len / paging.PAGE_SIZE, page_state_flag_committed, 0, 0, 0) catch |err| {
            removeCommitSpan(range.id, range.base + offset, len) catch {};
            return err;
        };
        range.committed_bytes = next_committed;
        range.status = .committed;
        blocks.setCommitted(range.block_id, range.committed_bytes) catch |err| return convertBlockError(err);
        return;
    }

    const pages = len / paging.PAGE_SIZE;
    if (!canCommitPages(range.*, pages)) return Error.OutOfMemory;

    var done: u64 = 0;
    while (done < pages) : (done += 1) {
        const virt = range.base + offset + done * paging.PAGE_SIZE;
        const frame = allocClaimedFrame(range.*, .vm_commit) catch |err| {
            rollbackCommit(range, range.base + offset, done);
            return err;
        };
        if (!paging.mapPage(virt, frame, range.flags)) {
            releaseClaimedFrame(frame);
            rollbackCommit(range, range.base + offset, done);
            return Error.MapFailed;
        }
        const mem: [*]u8 = @ptrFromInt(virt);
        @memset(mem[0..@intCast(paging.PAGE_SIZE)], 0);
        range.committed_bytes += paging.PAGE_SIZE;
        recordResident(range, paging.PAGE_SIZE);
    }

    range.status = .committed;
    blocks.setCommitted(range.block_id, range.committed_bytes) catch |err| return convertBlockError(err);
}

fn allocClaimedFrame(range: Range, reclaim_reason: reclaim.Reason) Error!u64 {
    const max_attempts = phys.stats().total_frames;
    var attempts: u64 = 0;
    var reclaimed = false;
    while (attempts < max_attempts) : (attempts += 1) {
        const frame = phys.allocFrame() orelse {
            if (!reclaimed) {
                reclaimed = true;
                // Reclaimers may reach filesystem/block I/O and yield.  The
                // transitional SMP owner boundary is deliberately no-sleep;
                // callers holding it fail with controlled OOM instead of
                // lending the global owner lock across a context switch.
                if (interrupts.inLegacyCriticalSection()) return Error.OutOfMemory;
                if (reclaim.reclaimFrames(reclaim_reason, 1).returned_frames > 0) {
                    attempts = 0;
                    continue;
                }
            }
            return Error.OutOfMemory;
        };
        _ = blocks.claimPhysicalRange(frame, paging.PAGE_SIZE, range.kind, range.owner, range.owner_id, range.name) catch |err| {
            if (err == error.NotFree) {
                continue;
            }
            phys.freeFrame(frame);
            return convertBlockError(err);
        };
        return frame;
    }
    return Error.OutOfMemory;
}

fn releaseClaimedFrame(frame: u64) void {
    var release_plan: blocks.PhysicalReleasePlan = undefined;
    blocks.preparePhysicalRangeRelease(frame, paging.PAGE_SIZE, &release_plan) catch return;
    defer blocks.cancelPhysicalRangeRelease(&release_plan);
    phys.freeFrame(frame);
    blocks.commitPhysicalRangeRelease(&release_plan);
}

pub fn handleDemandFault(addr: u64, error_code: u64, owner: blocks.Owner, owner_id: u64) bool {
    const owner_irq_flags = interrupts.saveAndDisable();
    var owner_locked = true;
    defer if (owner_locked) interrupts.restore(owner_irq_flags);
    if (!initialized) return false;
    if ((error_code & PAGE_FAULT_PRESENT) != 0) return false;
    const fault_page = alignDownValue(addr, paging.PAGE_SIZE);
    const range = rangeByAddress(fault_page) orelse return false;
    if (range.window != .r4x_vm or range.owner != owner or range.owner_id != owner_id) return false;
    if (range.kind != .virtual_range) return false;
    if (range.guard_len != 0 and rangesOverlap(fault_page, paging.PAGE_SIZE, range.guard_base, range.guard_len)) {
        recordDemandFaultFailure(range);
        return false;
    }
    if (!commitSpanCoversForRange(range, fault_page, paging.PAGE_SIZE)) {
        recordDemandFaultFailure(range);
        return false;
    }
    if (paging.isMapped(fault_page)) return false;
    if (!canCommitPages(range.*, 1)) {
        recordDemandFaultFailure(range);
        return false;
    }

    const page_index = (fault_page - range.base) / paging.PAGE_SIZE;
    if (pageStateForFaultInRange(range, page_index)) |fault_state| {
        if ((fault_state.flags & page_state_flag_slot_bound) != 0) {
            // Backing I/O may sleep. Pageable VM remains a BSP owner in the
            // SMP foundation; release the transitional no-sleep boundary and
            // retain the existing busy/generation/lifecycle transaction.
            if (percpu.currentIndex() != 0) {
                recordDemandFaultFailure(range);
                return false;
            }
            owner_locked = false;
            interrupts.restore(owner_irq_flags);
            return handlePageInFault(range, fault_page, page_index, fault_state);
        }
    }

    const frame = allocClaimedFrame(range.*, .vm_fault) catch {
        recordDemandFaultFailure(range);
        return false;
    };
    if (!paging.mapPage(fault_page, frame, range.flags)) {
        releaseClaimedFrame(frame);
        recordDemandFaultFailure(range);
        return false;
    }

    const mem: [*]u8 = @ptrFromInt(fault_page);
    @memset(mem[0..@intCast(paging.PAGE_SIZE)], 0);
    _ = paging.clearDirty(fault_page);
    recordDemandFaultSuccess(range);
    pageStateSet(range.id, page_index, 1, page_state_flag_resident, page_state_flag_dirty, null, true) catch {};
    return true;
}

pub fn uncommit(id: u32, offset: u64, len_raw: u64) Error!void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const len = normalizeLen(len_raw) catch |err| return err;
    if (!isAligned(offset, paging.PAGE_SIZE)) return Error.BadAlignment;
    const idx = indexById(id) orelse return Error.NotFound;
    const range = &ranges[idx];
    try validateInside(range.*, offset, len);
    const start = range.base + offset;
    try validateNotGuard(range.*, start, len);
    const resuming_partial = range.partial_uncommit_len != 0;
    if (resuming_partial) {
        if (range.partial_uncommit_base != start or range.partial_uncommit_len != len) return Error.NotCommitted;
    } else {
        try validateCommitted(range.*, start, len);
        publishPartialUncommit(range, start, len, true);
    }
    uncommitSpan(range, start, len, resuming_partial) catch |err| {
        range.status = if (range.committed_bytes == 0) .reserved else .committed;
        blocks.setCommitted(range.block_id, range.committed_bytes) catch |block_err| return convertBlockError(block_err);
        return err;
    };
    range.status = if (range.committed_bytes == 0) .reserved else .committed;
    blocks.setCommitted(range.block_id, range.committed_bytes) catch |err| return convertBlockError(err);
    clearPartialUncommit(range);
}

pub fn release(id: u32) Error!void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const idx = indexById(id) orelse return Error.NotFound;
    var range = &ranges[idx];
    if (range.window == .r4x_vm and range.owner == .r4x_instance and range.owner_id <= std.math.maxInt(u32)) {
        _ = backing_store.releaseVmRegion(@intCast(range.owner_id), range.id);
    }
    // Releasing a sparse reservation must scale with its committed pages, not
    // with the virtual reserve. Callers that know a compact committed span
    // (notably guarded program stacks) uncommit it first; do not walk every
    // page of an already-empty multi-megabyte reservation afterwards.
    const resuming_release = range.partial_uncommit_len != 0 and
        range.partial_uncommit_base == range.base and range.partial_uncommit_len == range.len;
    if (!resuming_release) publishPartialUncommit(range, range.base, range.len, true);
    if (range.committed_bytes != 0) {
        uncommitSpan(range, range.base, range.len, true) catch |err| {
            blocks.setCommitted(range.block_id, range.committed_bytes) catch |block_err| return convertBlockError(block_err);
            return err;
        };
    }
    // Once the public ID index is removed there is no lookup path for a
    // retry. Keep block release, ID tombstone and Range retirement in one
    // IRQ-atomic no-yield publication boundary.
    const release_irq_flags = interrupts.saveAndDisable();
    blocks.release(range.block_id) catch |err| {
        interrupts.restore(release_irq_flags);
        return convertBlockError(err);
    };
    removeCommitSpansForRange(range.id);
    removePageStateForRange(range.id);
    removeRangeIdIndex(range.id);
    removeRangeAddressIndex(idx);
    range.status = .released;
    range.committed_bytes = 0;
    range.resident_bytes = 0;
    range.peak_resident_bytes = 0;
    range.fault_count = 0;
    range.failed_faults = 0;
    range.guard_base = 0;
    range.guard_len = 0;
    range.partial_uncommit_base = 0;
    range.partial_uncommit_len = 0;
    range.partial_uncommit_cursor = 0;
    range.partial_uncommit_accounted = false;
    next_free_range_slot = idx;
    interrupts.restore(release_irq_flags);
}

pub fn releaseOwner(owner: blocks.Owner, owner_id: u64, kind: ?blocks.Kind) u64 {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return 0;
    var released: u64 = 0;
    while (true) {
        const id = firstOwnedRange(owner, owner_id, kind) orelse break;
        release(id) catch break;
        released += 1;
    }
    return released;
}

pub fn protectGuard(id: u32, offset: u64, len_raw: u64) Error!void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const len = normalizeLen(len_raw) catch |err| return err;
    if (!isAligned(offset, paging.PAGE_SIZE)) return Error.BadAlignment;
    const idx = indexById(id) orelse return Error.NotFound;
    var range = &ranges[idx];
    try validateInside(range.*, offset, len);
    const guard_base = range.base + offset;
    if (overlapsMapped(guard_base, len) or commitSpanOverlaps(range.id, guard_base, len)) return Error.AlreadyCommitted;
    range.guard_base = guard_base;
    range.guard_len = len;
}

pub fn clearGuard(id: u32) Error!void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return Error.NotInitialized;
    const idx = indexById(id) orelse return Error.NotFound;
    ranges[idx].guard_base = 0;
    ranges[idx].guard_len = 0;
}

pub fn rangeInfo(id: u32) ?RangeInfo {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized) return null;
    const idx = indexById(id) orelse return null;
    const range = ranges[idx];
    if (!range.active()) return null;
    return infoFromRange(range);
}

pub fn pageStateProbe(input: PageStateInput) PageStateResult {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var result = makePageStateResult(input);
    if (!validatePageStateInput(input, &result)) {
        recordPageState(&result);
        return result;
    }

    const idx = indexById(input.region_id) orelse {
        result.status = page_state_status_region_missing;
        result.blockers |= page_state_blocker_region_missing;
        recordPageState(&result);
        return result;
    };
    const range = &ranges[idx];
    if (range.window != .r4x_vm or range.kind != .virtual_range) {
        result.status = page_state_status_region_not_r4x;
        result.blockers |= page_state_blocker_region_not_r4x;
        recordPageState(&result);
        return result;
    }
    const byte_count = checkedMul(input.page_count, paging.PAGE_SIZE) orelse {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
        recordPageState(&result);
        return result;
    };
    if (!commitSpanCoversForRange(range, range.base + input.region_offset, byte_count)) {
        result.status = page_state_status_outside_commit;
        result.blockers |= page_state_blocker_outside_commit;
        recordPageState(&result);
        return result;
    }

    const first_page = input.region_offset / paging.PAGE_SIZE;
    syncHardwareDirty(range.*, first_page, input.page_count);
    applyPageStateOperation(range.*, input, first_page, &result) catch |err| {
        result.status = if (err == Error.TableFull) page_state_status_table_full else page_state_status_invalid_request;
        result.blockers |= if (err == Error.TableFull) page_state_blocker_table_full else page_state_blocker_invalid_request;
        if (err == Error.TableFull) page_state_summary.table_full_failures +%= 1;
        recordPageState(&result);
        return result;
    };

    syncHardwareDirty(range.*, first_page, input.page_count);
    fillPageStateCounts(input.region_id, first_page, input.page_count, &result);
    result.status = page_state_status_ready;
    recordPageState(&result);
    return result;
}

pub fn applyPageIoState(input: PageStateInput, page_out: bool) PageStateResult {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var adjusted = input;
    adjusted.operation = if (page_out) page_state_operation_bind_slot else page_state_operation_query;
    var result = pageStateProbe(adjusted);
    if (page_out and result.status == page_state_status_ready) {
        adjusted.operation = page_state_operation_mark_clean;
        result = pageStateProbe(adjusted);
        if (result.status == page_state_status_ready) {
            page_state_summary.page_out_nonresident_pages +%= makePageRangeNonresident(adjusted.region_id, adjusted.region_offset, adjusted.page_count) catch 0;
            adjusted.operation = page_state_operation_query;
            result = pageStateProbe(adjusted);
        }
    }
    return result;
}

pub fn pageStateSummary() PageStateSummary {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var summary = page_state_summary;
    summary.span_count = activePageStateSpanCount();
    return summary;
}

pub fn recordPagerPolicyFailure(input: PageStateInput, page_out: bool) void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (input.page_count == 0) return;
    const idx = indexById(input.region_id) orelse return;
    const range = ranges[idx];
    if (!range.active() or range.window != .r4x_vm) return;
    if ((input.region_offset & (paging.PAGE_SIZE - 1)) != 0) return;
    const first_page = input.region_offset / paging.PAGE_SIZE;

    if (page_out) {
        page_state_summary.pager_failed_page_outs +%= 1;
    } else {
        page_state_summary.pager_failed_page_ins +%= 1;
    }
    page_state_summary.pager_data_preserved_pages +%= input.page_count;

    var dirty_pages: u64 = 0;
    var offset: u64 = 0;
    while (offset < input.page_count) : (offset += 1) {
        const page_index = first_page + offset;
        if (pageStateForFault(input.region_id, page_index)) |state| {
            if ((state.flags & page_state_flag_dirty) != 0) {
                dirty_pages +%= 1;
                continue;
            }
        }
        const virt = range.base + page_index * paging.PAGE_SIZE;
        if (paging.isMapped(virt) and paging.pageDirty(virt)) dirty_pages +%= 1;
    }
    if (page_out) page_state_summary.pager_dirty_preserved_pages +%= dirty_pages;
}

pub fn stats() Stats {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var s: Stats = .{};
    if (!initialized) return s;

    var i: usize = 0;
    while (i < ranges.len) : (i += 1) {
        const range = ranges[i];
        if (!range.active()) continue;
        s.active_ranges += 1;
        checkedAddInto(&s.reserved_bytes, range.len, &s.overflow);
        checkedAddInto(&s.committed_bytes, range.committed_bytes, &s.overflow);
        checkedAddInto(&s.resident_bytes, range.resident_bytes, &s.overflow);
        checkedAddInto(&s.fault_count, range.fault_count, &s.overflow);
        checkedAddInto(&s.failed_faults, range.failed_faults, &s.overflow);
        if (range.peak_resident_bytes > s.peak_resident_bytes) s.peak_resident_bytes = range.peak_resident_bytes;
        checkedAddInto(&s.guard_bytes, range.guard_len, &s.overflow);
    }

    var window_index: usize = 0;
    while (window_index < WINDOWS.len) : (window_index += 1) {
        const window: Window = @enumFromInt(window_index);
        const largest = largestFreeInWindow(window);
        if (largest.len > s.largest_free_virtual_len) {
            s.largest_free_virtual_base = largest.base;
            s.largest_free_virtual_len = largest.len;
        }
    }

    const p = phys.stats();
    if (p.free_frames > APP_SYSTEM_RESERVE_FRAMES) {
        s.app_available_frames = p.free_frames - APP_SYSTEM_RESERVE_FRAMES;
    }
    return s;
}

pub fn ownerStats(owner: blocks.Owner, owner_id: u64, window_filter: ?Window, kind_filter: ?blocks.Kind) OwnerStats {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var s: OwnerStats = .{};
    if (!initialized) return s;

    var i: usize = 0;
    while (i < ranges.len) : (i += 1) {
        const range = ranges[i];
        if (!range.active()) continue;
        if (range.owner != owner or range.owner_id != owner_id) continue;
        if (window_filter) |window| {
            if (range.window != window) continue;
        }
        if (kind_filter) |kind| {
            if (range.kind != kind) continue;
        }
        s.active_ranges += 1;
        checkedAddInto(&s.reserved_bytes, range.len, &s.overflow);
        checkedAddInto(&s.committed_bytes, range.committed_bytes, &s.overflow);
        checkedAddInto(&s.resident_bytes, range.resident_bytes, &s.overflow);
        checkedAddInto(&s.fault_count, range.fault_count, &s.overflow);
        checkedAddInto(&s.failed_faults, range.failed_faults, &s.overflow);
        if (range.peak_resident_bytes > s.peak_resident_bytes) s.peak_resident_bytes = range.peak_resident_bytes;
    }
    return s;
}

pub fn dumpStats() void {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    const s = stats();
    k.puts("  Virtual ranges: active=");
    k.putDec(s.active_ranges);
    k.puts(" reserved=");
    k.putDec(s.reserved_bytes);
    k.puts(" committed=");
    k.putDec(s.committed_bytes);
    k.puts(" resident=");
    k.putDec(s.resident_bytes);
    k.puts(" peak-resident=");
    k.putDec(s.peak_resident_bytes);
    k.puts(" guard=");
    k.putDec(s.guard_bytes);
    k.puts(" faults=");
    k.putDec(s.fault_count);
    k.puts("/");
    k.putDec(s.failed_faults);
    k.puts(" overflow=");
    k.puts(if (s.overflow) "yes" else "no");
    k.puts("\r\n");

    k.puts("  Virtual largest free: base=0x");
    k.putHex(s.largest_free_virtual_base, 16);
    k.puts(" len=0x");
    k.putHex(s.largest_free_virtual_len, 16);
    k.puts("\r\n");

    k.puts("  App memory reserve: frames=");
    k.putDec(s.app_system_reserve_frames);
    k.puts(" app_available_frames=");
    k.putDec(s.app_available_frames);
    k.puts("\r\n");
}

pub fn windowBase(window: Window) u64 {
    return windowDef(window).base;
}

pub fn windowLen(window: Window) u64 {
    return windowDef(window).len;
}

pub fn windowName(window: Window) []const u8 {
    return windowDef(window).name;
}

fn reserveAtInternal(req: ReserveAtRequest) Error!u32 {
    const window = windowDef(req.window);
    const end = checkedEnd(req.base, req.len) orelse return Error.Overflow;
    const window_end = checkedEnd(window.base, window.len) orelse return Error.Overflow;
    if (req.base < window.base or end > window_end) return Error.OutsideWindow;
    if (overlapsExisting(req.base, req.len)) return Error.Overlap;

    const slot = freeSlot() orelse return Error.TableFull;
    const id = allocId() catch |err| return err;
    const block_id = blocks.register(.{
        .kind = req.kind,
        .owner = req.owner,
        .owner_id = req.owner_id,
        .status = .reserved,
        .name = req.name,
        .virt_base = req.base,
        .virt_len = req.len,
        .reserved_bytes = req.len,
        .committed_bytes = 0,
    }) catch |err| return convertBlockError(err);

    ranges[slot] = .{
        .slot_used = true,
        .id = id,
        .window = req.window,
        .kind = req.kind,
        .owner = req.owner,
        .owner_id = req.owner_id,
        .status = .reserved,
        .name = req.name,
        .flags = req.flags,
        .base = req.base,
        .len = req.len,
        .committed_bytes = 0,
        .block_id = block_id,
    };
    if (!insertRangeIdIndex(id, slot)) {
        ranges[slot] = .{};
        blocks.release(block_id) catch {};
        next_free_range_slot = slot;
        return Error.TableFull;
    }
    insertRangeAddressIndex(slot);
    return id;
}

fn firstFit(window: Window, len: u64, alignment: u64) Error!u64 {
    const def = windowDef(window);
    const window_end = checkedEnd(def.base, def.len) orelse return Error.Overflow;
    var candidate = alignUpChecked(def.base, alignment) orelse return Error.Overflow;

    var pos = lowerBoundAddress(def.base);
    while (pos < range_address_count) : (pos += 1) {
        const range = ranges[range_address_order[pos]];
        if (range.base >= window_end) break;
        if (range.window != window) continue;
        const candidate_end = checkedEnd(candidate, len) orelse return Error.Overflow;
        if (candidate_end > window_end) return Error.NoSpace;
        if (candidate_end <= range.base) return candidate;
        const range_end = checkedEnd(range.base, range.len) orelse return Error.Overflow;
        if (range_end > candidate) {
            const next = alignUpChecked(range_end, alignment) orelse return Error.Overflow;
            if (next <= candidate) return Error.Overflow;
            candidate = next;
        }
    }
    const candidate_end = checkedEnd(candidate, len) orelse return Error.Overflow;
    return if (candidate_end <= window_end) candidate else Error.NoSpace;
}

fn uncommitSpan(range: *Range, start: u64, len: u64, skip_unmapped: bool) Error!void {
    if (range.window == .r4x_vm) return uncommitDemandSpan(range, start, len, skip_unmapped);

    const end = checkedEnd(start, len) orelse return Error.Overflow;
    var virt = if (range.partial_uncommit_cursor >= start and range.partial_uncommit_cursor <= end)
        range.partial_uncommit_cursor
    else
        start;
    while (virt < end) : (virt += paging.PAGE_SIZE) {
        if (!paging.isMapped(virt)) {
            if (skip_unmapped) {
                advancePartialUncommitCursor(range, virt + paging.PAGE_SIZE);
                continue;
            }
            return Error.NotCommitted;
        }
        try uncommitPage(range, virt, true);
        advancePartialUncommitCursor(range, virt + paging.PAGE_SIZE);
    }
}

// Marker publication and removal are tiny IRQ-atomic state transitions. A
// timer hard-kill may happen between pages, but can never observe a torn
// base/length/accounted tuple.
fn publishPartialUncommit(range: *Range, base: u64, len: u64, reset_accounted: bool) void {
    const irq_flags = interrupts.saveAndDisable();
    range.partial_uncommit_base = base;
    range.partial_uncommit_len = len;
    if (reset_accounted) {
        range.partial_uncommit_cursor = base;
        range.partial_uncommit_accounted = false;
    }
    interrupts.restore(irq_flags);
}

fn advancePartialUncommitCursor(range: *Range, cursor: u64) void {
    const irq_flags = interrupts.saveAndDisable();
    range.partial_uncommit_cursor = cursor;
    interrupts.restore(irq_flags);
}

fn clearPartialUncommit(range: *Range) void {
    const irq_flags = interrupts.saveAndDisable();
    range.partial_uncommit_base = 0;
    range.partial_uncommit_len = 0;
    range.partial_uncommit_cursor = 0;
    range.partial_uncommit_accounted = false;
    interrupts.restore(irq_flags);
}

fn uncommitDemandSpan(range: *Range, start: u64, len: u64, skip_unmapped: bool) Error!void {
    if (!skip_unmapped and !commitSpanCoversForRange(range, start, len)) return Error.NotCommitted;
    const end = checkedEnd(start, len) orelse return Error.Overflow;
    var commit_split = try reserveCommitSplitSlot(range.id, start, len);
    defer if (commit_split) |slot| releaseCommitSpanSlot(slot);
    const first_page = (start - range.base) / paging.PAGE_SIZE;
    const end_page = first_page + len / paging.PAGE_SIZE;
    var page_splits = try reservePageStateSplitSlots(range, first_page, end_page);
    defer releaseReservedPageStateSlots(&page_splits);

    var virt = if (range.partial_uncommit_cursor >= start and range.partial_uncommit_cursor <= end)
        range.partial_uncommit_cursor
    else
        start;
    while (virt < end) : (virt += paging.PAGE_SIZE) {
        if (paging.isMapped(virt)) {
            try uncommitPage(range, virt, false);
        }
        advancePartialUncommitCursor(range, virt + paging.PAGE_SIZE);
    }

    try removeCommitSpanReserved(range.id, start, len, &commit_split);
    try removePageStateRangeReserved(range, first_page, end_page, &page_splits);
    const irq_flags = interrupts.saveAndDisable();
    if (!range.partial_uncommit_accounted) {
        if (skip_unmapped and len > range.committed_bytes) {
            range.committed_bytes = 0;
        } else if (range.committed_bytes >= len) {
            range.committed_bytes -= len;
        } else {
            range.committed_bytes = 0;
        }
        range.partial_uncommit_accounted = true;
    }
    interrupts.restore(irq_flags);
}

fn uncommitPage(range: *Range, virt: u64, count_logical_commit: bool) Error!void {
    const frame = paging.mappedFrame(virt) orelse return Error.NotCommitted;
    var release_plan: blocks.PhysicalReleasePlan = undefined;
    blocks.preparePhysicalRangeRelease(frame, paging.PAGE_SIZE, &release_plan) catch |err| return convertBlockError(err);
    defer blocks.cancelPhysicalRangeRelease(&release_plan);
    if (!paging.unmapPage(virt)) {
        return Error.MapFailed;
    }
    // The plan keeps interrupts disabled. Free the PMM frame before the
    // no-fail metadata commit restores IF, otherwise a hard kill in the tiny
    // gap could strand an unmapped frame as permanently used.
    phys.freeFrame(frame);
    if (range.resident_bytes >= paging.PAGE_SIZE) {
        range.resident_bytes -= paging.PAGE_SIZE;
    } else if (range.resident_bytes != 0) {
        range.resident_bytes = 0;
    }
    if (count_logical_commit) {
        if (range.committed_bytes >= paging.PAGE_SIZE) {
            range.committed_bytes -= paging.PAGE_SIZE;
        } else {
            range.committed_bytes = 0;
        }
    }
    blocks.commitPhysicalRangeRelease(&release_plan);
}

fn rollbackCommit(range: *Range, start: u64, page_count: u64) void {
    defer blocks.coalescePhysicalRanges();
    var i: u64 = 0;
    while (i < page_count) : (i += 1) {
        uncommitPage(range, start + i * paging.PAGE_SIZE, true) catch {};
    }
    blocks.setCommitted(range.block_id, range.committed_bytes) catch {};
}

fn validateInside(range: Range, offset: u64, len: u64) Error!void {
    const end = checkedEnd(offset, len) orelse return Error.Overflow;
    if (end > range.len) return Error.OutsideWindow;
}

fn validateNotGuard(range: Range, base: u64, len: u64) Error!void {
    if (range.guard_len == 0) return;
    if (rangesOverlap(base, len, range.guard_base, range.guard_len)) return Error.GuardRange;
}

fn validateUncommitted(range: Range, base: u64, len: u64) Error!void {
    if (range.window == .r4x_vm) {
        if (commitSpanOverlaps(range.id, base, len)) return Error.AlreadyCommitted;
        return;
    }

    var offset: u64 = 0;
    while (offset < len) : (offset += paging.PAGE_SIZE) {
        if (paging.isMapped(base + offset)) return Error.AlreadyCommitted;
    }
}

fn validateCommitted(range: Range, base: u64, len: u64) Error!void {
    if (range.window == .r4x_vm) {
        if (!commitSpanCovers(range.id, base, len)) return Error.NotCommitted;
        return;
    }

    var offset: u64 = 0;
    while (offset < len) : (offset += paging.PAGE_SIZE) {
        if (!paging.isMapped(base + offset)) return Error.NotCommitted;
    }
}

fn overlapsMapped(base: u64, len: u64) bool {
    var offset: u64 = 0;
    while (offset < len) : (offset += paging.PAGE_SIZE) {
        if (paging.isMapped(base + offset)) return true;
    }
    return false;
}

fn canCommitPages(range: Range, pages: u64) bool {
    if (!isAppRange(range)) return true;
    const p = phys.stats();
    return p.free_frames > pages + APP_SYSTEM_RESERVE_FRAMES;
}

fn isAppRange(range: Range) bool {
    return range.owner == .r4x_instance or range.kind == .app_stack or range.kind == .program_image;
}

fn addCommitSpan(range_id: u32, base: u64, len: u64) Error!void {
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    const end = checkedEnd(base, len) orelse return Error.Overflow;
    hot_path_stats.commit_span_lookups +%= 1;

    var previous: ?u16 = null;
    var current = range.commit_span_head;
    var steps: usize = 0;
    while (current) |slot| {
        steps += 1;
        if (commit_spans[slot].base >= base) break;
        previous = slot;
        current = commit_spans[slot].next;
    }
    recordCommitSpanSteps(steps);

    if (previous) |slot| {
        const previous_end = checkedEnd(commit_spans[slot].base, commit_spans[slot].len) orelse return Error.Overflow;
        if (previous_end > base) return Error.Overlap;
        if (previous_end == base) {
            if (current) |next_slot| {
                if (commit_spans[next_slot].base < end) return Error.Overlap;
                if (commit_spans[next_slot].base == end) {
                    const next_end = checkedEnd(commit_spans[next_slot].base, commit_spans[next_slot].len) orelse return Error.Overflow;
                    commit_spans[slot].len = next_end - commit_spans[slot].base;
                    unlinkCommitSpan(range, next_slot);
                    releaseCommitSpanSlot(next_slot);
                    return;
                }
            }
            commit_spans[slot].len = end - commit_spans[slot].base;
            return;
        }
    }
    if (current) |slot| {
        if (commit_spans[slot].base < end) return Error.Overlap;
        if (commit_spans[slot].base == end) {
            const next_end = checkedEnd(commit_spans[slot].base, commit_spans[slot].len) orelse return Error.Overflow;
            commit_spans[slot].base = base;
            commit_spans[slot].len = next_end - base;
            return;
        }
    }

    const slot = allocCommitSpanSlot() orelse return Error.TableFull;
    commit_spans[slot] = .{
        .slot_used = true,
        .range_id = range_id,
        .base = base,
        .len = len,
        .prev = previous,
        .next = current,
    };
    if (previous) |prev_slot| {
        commit_spans[prev_slot].next = slot;
    } else {
        range.commit_span_head = slot;
    }
    if (current) |next_slot| commit_spans[next_slot].prev = slot;
}

fn removeCommitSpan(range_id: u32, base: u64, len: u64) Error!void {
    var reserved = try reserveCommitSplitSlot(range_id, base, len);
    defer if (reserved) |slot| releaseCommitSpanSlot(slot);
    try removeCommitSpanReserved(range_id, base, len, &reserved);
}

fn removeCommitSpanNeedsSlot(range_id: u32, base: u64, len: u64) Error!bool {
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    const release_end = checkedEnd(base, len) orelse return Error.Overflow;
    hot_path_stats.commit_span_lookups +%= 1;
    var current = range.commit_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = commit_spans[slot].next) {
        steps += 1;
        const span = commit_spans[slot];
        if (span.base >= release_end) break;
        if (!rangesOverlap(base, len, span.base, span.len)) continue;
        const span_end = checkedEnd(span.base, span.len) orelse return Error.Overflow;
        if (span.base < base and span_end > release_end) {
            recordCommitSpanSteps(steps);
            return true;
        }
    }
    recordCommitSpanSteps(steps);
    return false;
}

fn reserveCommitSplitSlot(range_id: u32, base: u64, len: u64) Error!?u16 {
    if (!try removeCommitSpanNeedsSlot(range_id, base, len)) return null;
    return allocCommitSpanSlot() orelse Error.TableFull;
}

fn removeCommitSpanReserved(range_id: u32, base: u64, len: u64, reserved: *?u16) Error!void {
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    const release_end = checkedEnd(base, len) orelse return Error.Overflow;
    hot_path_stats.commit_span_lookups +%= 1;

    var current = range.commit_span_head;
    var steps: usize = 0;
    while (current) |slot| {
        steps += 1;
        const next = commit_spans[slot].next;
        const span_base = commit_spans[slot].base;
        if (span_base >= release_end) break;
        const span_end = checkedEnd(span_base, commit_spans[slot].len) orelse return Error.Overflow;
        if (span_end <= base) {
            current = next;
            continue;
        }

        const keep_left = span_base < base;
        const keep_right = span_end > release_end;
        if (keep_left and keep_right) {
            const right_slot = reserved.* orelse return Error.TableFull;
            reserved.* = null;
            const old_next = commit_spans[slot].next;
            commit_spans[slot].len = base - span_base;
            commit_spans[right_slot] = .{
                .slot_used = true,
                .range_id = range_id,
                .base = release_end,
                .len = span_end - release_end,
                .prev = slot,
                .next = old_next,
            };
            commit_spans[slot].next = right_slot;
            if (old_next) |next_slot| commit_spans[next_slot].prev = right_slot;
            break;
        } else if (keep_left) {
            commit_spans[slot].len = base - span_base;
        } else if (keep_right) {
            commit_spans[slot].base = release_end;
            commit_spans[slot].len = span_end - release_end;
            break;
        } else {
            unlinkCommitSpan(range, slot);
            releaseCommitSpanSlot(slot);
        }
        current = next;
    }
    recordCommitSpanSteps(steps);
}

fn removeCommitSpansForRange(range_id: u32) void {
    const range_index = indexById(range_id) orelse return;
    const range = &ranges[range_index];
    var current = range.commit_span_head;
    while (current) |slot| {
        const next = commit_spans[slot].next;
        releaseCommitSpanSlot(slot);
        current = next;
    }
    range.commit_span_head = null;
}

fn commitSpanOverlaps(range_id: u32, base: u64, len: u64) bool {
    const range_index = indexById(range_id) orelse return false;
    return commitSpanOverlapsForRange(&ranges[range_index], base, len);
}

fn commitSpanOverlapsForRange(range: *const Range, base: u64, len: u64) bool {
    const end = checkedEnd(base, len) orelse return false;
    hot_path_stats.commit_span_lookups +%= 1;
    var current = range.commit_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = commit_spans[slot].next) {
        steps += 1;
        const span = commit_spans[slot];
        if (span.base >= end) break;
        const span_end = checkedEnd(span.base, span.len) orelse {
            recordCommitSpanSteps(steps);
            return true;
        };
        if (span_end > base) {
            recordCommitSpanSteps(steps);
            return true;
        }
    }
    recordCommitSpanSteps(steps);
    return false;
}

fn commitSpanCovers(range_id: u32, base: u64, len: u64) bool {
    const range_index = indexById(range_id) orelse return false;
    return commitSpanCoversForRange(&ranges[range_index], base, len);
}

fn commitSpanCoversForRange(range: *const Range, base: u64, len: u64) bool {
    const end = checkedEnd(base, len) orelse return false;
    hot_path_stats.commit_span_lookups +%= 1;
    var cursor = base;
    var current = range.commit_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = commit_spans[slot].next) {
        steps += 1;
        const span = commit_spans[slot];
        const span_end = checkedEnd(span.base, span.len) orelse {
            recordCommitSpanSteps(steps);
            return false;
        };
        if (span_end <= cursor) continue;
        if (span.base > cursor) break;
        cursor = @min(span_end, end);
        if (cursor == end) {
            recordCommitSpanSteps(steps);
            return true;
        }
    }
    recordCommitSpanSteps(steps);
    return false;
}

fn allocCommitSpanSlot() ?u16 {
    const slot = commit_span_free_head orelse return null;
    commit_span_free_head = commit_spans[slot].next;
    commit_spans[slot] = .{};
    commit_span_active_count +|= 1;
    return slot;
}

fn releaseCommitSpanSlot(slot: u16) void {
    commit_spans[slot] = .{ .next = commit_span_free_head };
    commit_span_free_head = slot;
    if (commit_span_active_count != 0) commit_span_active_count -= 1;
}

fn unlinkCommitSpan(range: *Range, slot: u16) void {
    const previous = commit_spans[slot].prev;
    const next = commit_spans[slot].next;
    if (previous) |prev_slot| {
        commit_spans[prev_slot].next = next;
    } else {
        range.commit_span_head = next;
    }
    if (next) |next_slot| commit_spans[next_slot].prev = previous;
}

fn rangeByAddress(addr: u64) ?*Range {
    hot_path_stats.range_address_lookups +%= 1;
    var probes: usize = 0;
    var low: usize = 0;
    var high = range_address_count;
    while (low < high) {
        probes += 1;
        const mid = low + (high - low) / 2;
        if (ranges[range_address_order[mid]].base <= addr) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    recordRangeAddressProbe(probes);
    if (low == 0) return null;
    const slot: usize = range_address_order[low - 1];
    const end = checkedEnd(ranges[slot].base, ranges[slot].len) orelse return null;
    if (addr < end) {
        return &ranges[slot];
    }
    return null;
}

fn pageStateForFault(range_id: u32, page_index: u64) ?FaultPageState {
    const range_index = indexById(range_id) orelse return null;
    return pageStateForFaultInRange(&ranges[range_index], page_index);
}

fn pageStateForFaultInRange(range: *const Range, page_index: u64) ?FaultPageState {
    hot_path_stats.page_state_span_lookups +%= 1;
    var current = range.page_state_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = page_state_spans[slot].next) {
        steps += 1;
        const span = page_state_spans[slot];
        const span_end = checkedAdd(span.first_page, span.page_count) orelse {
            recordPageStateSpanSteps(steps);
            return null;
        };
        if (page_index < span.first_page) break;
        if (page_index >= span_end) continue;
        recordPageStateSpanSteps(steps);
        return .{
            .flags = span.flags,
            .slot_reservation_id = span.slot_reservation_id,
            .slot_index = if ((span.flags & page_state_flag_slot_bound) != 0) span.slot_index + (page_index - span.first_page) else 0,
            .slot_generation = span.slot_generation,
        };
    }
    recordPageStateSpanSteps(steps);
    return null;
}

fn handlePageInFault(range: *Range, fault_page: u64, page_index: u64, fault_state: FaultPageState) bool {
    if (fault_state.slot_reservation_id == 0 or
        (fault_state.flags & (page_state_flag_busy | page_state_flag_pinned | page_state_flag_error)) != 0 or
        range.owner_id > std.math.maxInt(u32))
    {
        return recordPageInFaultFailure(range, page_index, false);
    }

    const backing = backing_store.activeBackingResult() orelse return recordPageInFaultFailure(range, page_index, true);
    const path = backing_store.activeBackingPath() orelse return recordPageInFaultFailure(range, page_index, true);
    pageStateSet(range.id, page_index, 1, page_state_flag_busy, 0, null, true) catch {
        return recordPageInFaultFailure(range, page_index, true);
    };

    const owner_id_u32: u32 = @intCast(range.owner_id);
    const page_io_input = backing_store.PageIoInput{
        .operation = backing_store.page_io_operation_page_in,
        .region_id = range.id,
        .region_offset = fault_page - range.base,
        .reservation_id = fault_state.slot_reservation_id,
        .slot_index = fault_state.slot_index,
        .page_count = 1,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = owner_id_u32,
        .expected_generation = fault_state.slot_generation,
        .vm_region_exists = true,
        .vm_region_is_r4x = true,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .backing = backing,
    };

    const prepared = backing_store.pageIoPrepare(page_io_input);
    if (prepared.status != backing_store.page_io_status_ready) {
        return recordPageInFaultFailure(range, page_index, true);
    }

    const frame = allocClaimedFrame(range.*, .vm_fault) catch {
        return recordPageInFaultFailure(range, page_index, false);
    };
    if (!paging.mapPage(fault_page, frame, range.flags)) {
        releaseClaimedFrame(frame);
        return recordPageInFaultFailure(range, page_index, false);
    }

    const mem: [*]u8 = @ptrFromInt(fault_page);
    const transfer_len: u32 = @intCast(prepared.transfer_bytes);
    const io_status = r4sys_api.fileReadAt64(path, prepared.backing_offset, mem, transfer_len);
    const io_bytes: u32 = if (io_status > 0) @intCast(io_status) else 0;

    var complete_input = page_io_input;
    complete_input.io_status = io_status;
    complete_input.io_bytes = io_bytes;
    const completed = backing_store.pageIoComplete(complete_input);
    if (completed.status != backing_store.page_io_status_page_in_ok) {
        var release_plan: blocks.PhysicalReleasePlan = undefined;
        blocks.preparePhysicalRangeRelease(frame, paging.PAGE_SIZE, &release_plan) catch {
            return recordPageInFaultFailure(range, page_index, true);
        };
        defer blocks.cancelPhysicalRangeRelease(&release_plan);
        if (!paging.unmapPage(fault_page)) return recordPageInFaultFailure(range, page_index, true);
        phys.freeFrame(frame);
        blocks.commitPhysicalRangeRelease(&release_plan);
        return recordPageInFaultFailure(range, page_index, true);
    }

    _ = paging.clearDirty(fault_page);
    recordDemandFaultSuccess(range);
    page_state_summary.fault_page_ins +%= 1;
    pageStateSet(range.id, page_index, 1, page_state_flag_resident, page_state_flag_dirty | page_state_flag_busy | page_state_flag_error, null, true) catch {};
    return true;
}

fn recordPageInFaultFailure(range: *Range, page_index: u64, mark_error: bool) bool {
    recordDemandFaultFailure(range);
    page_state_summary.fault_page_in_failures +%= 1;
    page_state_summary.pager_failed_page_ins +%= 1;
    if (mark_error) page_state_summary.pager_data_preserved_pages +%= 1;
    const add_flags: u32 = if (mark_error) page_state_flag_error else 0;
    pageStateSet(range.id, page_index, 1, add_flags, page_state_flag_busy, null, true) catch {};
    return false;
}

fn makePageRangeNonresident(region_id: u32, region_offset: u64, page_count: u64) Error!u64 {
    if (page_count == 0 or !isAligned(region_offset, paging.PAGE_SIZE)) return Error.BadAlignment;
    const byte_count = checkedMul(page_count, paging.PAGE_SIZE) orelse return Error.Overflow;
    const idx = indexById(region_id) orelse return Error.NotFound;
    const range = &ranges[idx];
    try validateInside(range.*, region_offset, byte_count);
    if (!commitSpanCoversForRange(range, range.base + region_offset, byte_count)) return Error.NotCommitted;
    const first_page = region_offset / paging.PAGE_SIZE;
    defer blocks.coalescePhysicalRanges();
    var nonresident_pages: u64 = 0;
    var i: u64 = 0;
    while (i < page_count) : (i += 1) {
        const virt = range.base + region_offset + i * paging.PAGE_SIZE;
        if (!paging.isMapped(virt)) continue;
        try uncommitPage(range, virt, false);
        nonresident_pages +%= 1;
    }
    try pageStateSet(region_id, first_page, page_count, 0, page_state_flag_resident | page_state_flag_dirty, null, true);
    return nonresident_pages;
}

pub fn reclaimEvictFrames(reason: reclaim.Reason, requested_frames_raw: u32) reclaim.SourceResult {
    _ = reason;
    const requested_frames = if (requested_frames_raw == 0) 1 else requested_frames_raw;
    var result = reclaim.SourceResult{};
    if (!initialized) return result;
    // The pager transaction intentionally spans filesystem/block waits and
    // therefore cannot hold the transitional saveAndDisable owner lock.
    // Normal VM and lifecycle work remains BSP-owned; AP callers fail closed.
    if (percpu.currentIndex() != 0) {
        result.failures = 1;
        return result;
    }

    page_state_summary.eviction_attempts +%= 1;
    const backing = backing_store.activeBackingResult() orelse {
        page_state_summary.eviction_no_backing +%= 1;
        page_state_summary.eviction_failures +%= 1;
        page_state_summary.pager_disabled_eviction_gates +%= 1;
        result.failures = 1;
        return result;
    };
    const path = backing_store.activeBackingPath() orelse {
        page_state_summary.eviction_no_backing +%= 1;
        page_state_summary.eviction_failures +%= 1;
        page_state_summary.pager_disabled_eviction_gates +%= 1;
        result.failures = 1;
        return result;
    };

    while (result.returned_frames < requested_frames) {
        const step = evictOneResidentPage(backing, path);
        switch (step.outcome) {
            .returned => {
                result.returned_frames = result.returned_frames +| step.returned_frames;
                result.returned_bytes +%= step.returned_bytes;
                result.page_outs +%= step.page_outs;
                if (step.dirty) {
                    page_state_summary.eviction_dirty_pages +%= step.returned_frames;
                } else {
                    page_state_summary.eviction_clean_pages +%= step.returned_frames;
                }
                page_state_summary.eviction_successes +%= 1;
                page_state_summary.eviction_returned_frames +%= step.returned_frames;
            },
            .skipped => {
                page_state_summary.eviction_no_candidate +%= 1;
                break;
            },
            .failed => {
                page_state_summary.eviction_failures +%= 1;
                result.failures +%= 1;
                break;
            },
        }
    }

    if (result.returned_frames == 0 and result.failures == 0) {
        page_state_summary.eviction_failures +%= 1;
        result.failures = 1;
    }
    return result;
}

pub fn evictableBytes() u64 {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized or backing_store.activeBackingResult() == null) return 0;
    var bytes: u64 = 0;
    var range_pos: usize = 0;
    while (range_pos < range_address_count) : (range_pos += 1) {
        const range = ranges[range_address_order[range_pos]];
        var current = range.page_state_span_head;
        while (current) |slot| : (current = page_state_spans[slot].next) {
            const span = page_state_spans[slot];
            const evictable_flags = page_state_flag_committed | page_state_flag_resident;
            if ((span.flags & evictable_flags) != evictable_flags) continue;
            if ((span.flags & (page_state_flag_pinned | page_state_flag_busy | page_state_flag_error)) != 0) continue;
            bytes +%= span.page_count * paging.PAGE_SIZE;
        }
    }
    return bytes;
}

pub fn evictableDirtyBytes() u64 {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized or backing_store.activeBackingResult() == null) return 0;
    var bytes: u64 = 0;
    var range_pos: usize = 0;
    while (range_pos < range_address_count) : (range_pos += 1) {
        const range = ranges[range_address_order[range_pos]];
        var current = range.page_state_span_head;
        while (current) |slot| : (current = page_state_spans[slot].next) {
            const span = page_state_spans[slot];
            const evictable_flags = page_state_flag_committed | page_state_flag_resident | page_state_flag_dirty;
            if ((span.flags & evictable_flags) != evictable_flags) continue;
            if ((span.flags & (page_state_flag_pinned | page_state_flag_busy | page_state_flag_error)) != 0) continue;
            bytes +%= span.page_count * paging.PAGE_SIZE;
        }
    }
    return bytes;
}

fn evictOneResidentPage(backing: backing_store.Result, path: [*:0]const u8) EvictionStep {
    if (range_address_count == 0) return .{ .outcome = .skipped };
    var start_pos: usize = 0;
    var start_page: u64 = 0;
    if (reclaim_cursor_range_slot) |cursor_slot| {
        const raw_position = range_address_position[cursor_slot];
        if (raw_position != INVALID_RANGE_POSITION and @as(usize, raw_position) < range_address_count) {
            start_pos = raw_position;
            start_page = reclaim_cursor_page;
        }
    }

    var visited: usize = 0;
    var range_pos = start_pos;
    while (visited < range_address_count) : (visited += 1) {
        const range_slot = range_address_order[range_pos];
        var range = &ranges[range_slot];
        hot_path_stats.reclaim_cursor_range_steps +%= 1;
        const page_floor = if (visited == 0) start_page else 0;

        if (range.active() and range.window == .r4x_vm and range.owner == .r4x_instance and
            range.owner_id != 0 and range.owner_id <= std.math.maxInt(u32))
        {
            var span_cursor = range.page_state_span_head;
            while (span_cursor) |span_slot| : (span_cursor = page_state_spans[span_slot].next) {
                hot_path_stats.reclaim_cursor_span_steps +%= 1;
                const span = page_state_spans[span_slot];
                if ((span.flags & page_state_flag_committed) == 0) continue;
                const span_end = checkedAdd(span.first_page, span.page_count) orelse continue;
                if (span_end <= page_floor) continue;

                var page_index = @max(span.first_page, page_floor);
                while (page_index < span_end) : (page_index += 1) {
                    hot_path_stats.reclaim_cursor_page_steps +%= 1;
                    setReclaimCursorAfterPage(range_pos, page_index + 1);
                    const virt = range.base + page_index * paging.PAGE_SIZE;
                    if (range.guard_len != 0 and rangesOverlap(virt, paging.PAGE_SIZE, range.guard_base, range.guard_len)) {
                        page_state_summary.pager_disabled_eviction_gates +%= 1;
                        continue;
                    }
                    if ((span.flags & page_state_flag_resident) == 0) {
                        page_state_summary.eviction_skipped_nonresident +%= 1;
                        continue;
                    }
                    if ((span.flags & page_state_flag_pinned) != 0) {
                        page_state_summary.eviction_skipped_pinned +%= 1;
                        page_state_summary.pager_disabled_eviction_gates +%= 1;
                        continue;
                    }
                    if ((span.flags & page_state_flag_busy) != 0) {
                        page_state_summary.eviction_skipped_busy +%= 1;
                        page_state_summary.pager_disabled_eviction_gates +%= 1;
                        continue;
                    }
                    if ((span.flags & page_state_flag_error) != 0) {
                        page_state_summary.eviction_skipped_error +%= 1;
                        page_state_summary.pager_disabled_eviction_gates +%= 1;
                        continue;
                    }
                    if (!paging.isMapped(virt)) {
                        page_state_summary.eviction_skipped_unmapped +%= 1;
                        continue;
                    }

                    syncHardwareDirty(range.*, page_index, 1);
                    const state = pageStateForFaultInRange(range, page_index) orelse {
                        page_state_summary.eviction_skipped_nonresident +%= 1;
                        continue;
                    };
                    if ((state.flags & page_state_flag_resident) == 0) {
                        page_state_summary.eviction_skipped_nonresident +%= 1;
                        continue;
                    }
                    if ((state.flags & (page_state_flag_pinned | page_state_flag_busy | page_state_flag_error)) != 0) {
                        if ((state.flags & page_state_flag_pinned) != 0) page_state_summary.eviction_skipped_pinned +%= 1;
                        if ((state.flags & page_state_flag_busy) != 0) page_state_summary.eviction_skipped_busy +%= 1;
                        if ((state.flags & page_state_flag_error) != 0) page_state_summary.eviction_skipped_error +%= 1;
                        page_state_summary.pager_disabled_eviction_gates +%= 1;
                        continue;
                    }

                    page_state_summary.eviction_candidates +%= 1;
                    return evictResidentPage(range, page_index, state, backing, path);
                }
            }
        }

        range_pos += 1;
        if (range_pos == range_address_count) {
            range_pos = 0;
            hot_path_stats.reclaim_cursor_wraps +%= 1;
        }
        reclaim_cursor_range_slot = range_address_order[range_pos];
        reclaim_cursor_page = 0;
    }
    return .{ .outcome = .skipped };
}

fn setReclaimCursorAfterPage(range_pos: usize, next_page: u64) void {
    const slot = range_address_order[range_pos];
    const range_pages = ranges[slot].len / paging.PAGE_SIZE;
    if (next_page < range_pages) {
        reclaim_cursor_range_slot = slot;
        reclaim_cursor_page = next_page;
        return;
    }
    var next_pos = range_pos + 1;
    if (next_pos == range_address_count) {
        next_pos = 0;
        hot_path_stats.reclaim_cursor_wraps +%= 1;
    }
    reclaim_cursor_range_slot = range_address_order[next_pos];
    reclaim_cursor_page = 0;
}

fn evictResidentPage(range: *Range, page_index: u64, state: FaultPageState, backing: backing_store.Result, path: [*:0]const u8) EvictionStep {
    const virt = range.base + page_index * paging.PAGE_SIZE;
    const region_offset = page_index * paging.PAGE_SIZE;
    const dirty = ((state.flags & page_state_flag_dirty) != 0) or paging.pageDirty(virt);
    pageStateSet(range.id, page_index, 1, page_state_flag_busy, 0, null, true) catch {
        page_state_summary.eviction_failures +%= 1;
        return .{ .outcome = .failed };
    };

    if ((state.flags & page_state_flag_slot_bound) != 0 and !dirty) {
        return evictCleanSlotBoundPage(range, page_index, state, backing);
    }

    const owner_id: u32 = @intCast(range.owner_id);
    var reservation_id = state.slot_reservation_id;
    var slot_index = state.slot_index;
    var expected_generation = state.slot_generation;
    var reserved_new_slot = false;

    if ((state.flags & page_state_flag_slot_bound) == 0) {
        const reserve_result = backing_store.slotProbe(.{
            .operation = backing_store.slot_operation_reserve,
            .requested_slots = 1,
            .owner_kind = backing_store.slot_owner_kind_vm_region,
            .owner_id = owner_id,
            .region_id = range.id,
            .backing = backing,
        });
        if (reserve_result.status != backing_store.slot_status_reserved) {
            clearEvictionBusy(range.id, page_index);
            page_state_summary.eviction_slot_failures +%= 1;
            recordPagerPageOutFailure(dirty);
            return .{ .outcome = .failed };
        }
        reservation_id = reserve_result.reservation_id;
        slot_index = 0;
        expected_generation = reserve_result.generation;
        reserved_new_slot = true;
    }

    const input = backing_store.PageIoInput{
        .operation = backing_store.page_io_operation_page_out,
        .region_id = range.id,
        .region_offset = region_offset,
        .reservation_id = reservation_id,
        .slot_index = slot_index,
        .page_count = 1,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = owner_id,
        .expected_generation = expected_generation,
        .flags = backing_store.page_io_flag_eviction_request,
        .vm_region_exists = true,
        .vm_region_is_r4x = true,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .backing = backing,
    };

    const prepared = backing_store.pageIoPrepare(input);
    if (prepared.status != backing_store.page_io_status_ready) {
        if (reserved_new_slot) releaseEvictionSlot(backing, reservation_id, owner_id, range.id);
        clearEvictionBusy(range.id, page_index);
        page_state_summary.eviction_slot_failures +%= 1;
        recordPagerPageOutFailure(dirty);
        return .{ .outcome = .failed, .dirty = dirty };
    }

    const page_ptr: [*]u8 = @ptrFromInt(virt);
    const transfer_len: u32 = @intCast(prepared.transfer_bytes);
    const io_status = r4sys_api.fileWriteAt(path, prepared.backing_offset, page_ptr, transfer_len);
    const io_bytes: u32 = if (io_status > 0) @intCast(io_status) else 0;

    var complete_input = input;
    complete_input.io_status = io_status;
    complete_input.io_bytes = io_bytes;
    const completed = backing_store.pageIoComplete(complete_input);
    if (completed.status != backing_store.page_io_status_page_out_ok) {
        if (reserved_new_slot) releaseEvictionSlot(backing, reservation_id, owner_id, range.id);
        markEvictionError(range.id, page_index);
        page_state_summary.eviction_io_failures +%= 1;
        recordPagerPageOutFailure(dirty);
        return .{ .outcome = .failed, .dirty = dirty };
    }

    const before_free = phys.stats().free_frames;
    const applied = applyPageIoState(.{
        .region_id = completed.region_id,
        .region_offset = completed.region_offset,
        .page_count = completed.page_count,
        .slot_reservation_id = completed.reservation_id,
        .slot_index = completed.slot_index,
        .slot_generation = completed.slot_generation,
    }, true);
    if (applied.status != page_state_status_ready) {
        markEvictionError(range.id, page_index);
        page_state_summary.eviction_failures +%= 1;
        recordPagerPageOutFailure(dirty);
        return .{ .outcome = .failed, .dirty = dirty };
    }
    clearEvictionBusy(range.id, page_index);
    const after_free = phys.stats().free_frames;
    const returned_frames: u32 = if (after_free > before_free) @intCast(after_free - before_free) else 0;
    if (returned_frames == 0) {
        page_state_summary.eviction_failures +%= 1;
        return .{ .outcome = .failed, .dirty = dirty };
    }
    page_state_summary.eviction_page_outs +%= 1;
    return .{
        .outcome = .returned,
        .returned_frames = returned_frames,
        .returned_bytes = @as(u64, returned_frames) * paging.PAGE_SIZE,
        .page_outs = 1,
        .dirty = dirty,
    };
}

fn evictCleanSlotBoundPage(range: *Range, page_index: u64, state: FaultPageState, backing: backing_store.Result) EvictionStep {
    const owner_id: u32 = @intCast(range.owner_id);
    const input = backing_store.PageIoInput{
        .operation = backing_store.page_io_operation_page_in,
        .region_id = range.id,
        .region_offset = page_index * paging.PAGE_SIZE,
        .reservation_id = state.slot_reservation_id,
        .slot_index = state.slot_index,
        .page_count = 1,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = owner_id,
        .expected_generation = state.slot_generation,
        .vm_region_exists = true,
        .vm_region_is_r4x = true,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .backing = backing,
    };
    const prepared = backing_store.pageIoPrepare(input);
    if (prepared.status != backing_store.page_io_status_ready) {
        clearEvictionBusy(range.id, page_index);
        page_state_summary.eviction_slot_failures +%= 1;
        return .{ .outcome = .failed };
    }

    const before_free = phys.stats().free_frames;
    page_state_summary.page_out_nonresident_pages +%= makePageRangeNonresident(range.id, page_index * paging.PAGE_SIZE, 1) catch {
        clearEvictionBusy(range.id, page_index);
        page_state_summary.eviction_failures +%= 1;
        return .{ .outcome = .failed };
    };
    clearEvictionBusy(range.id, page_index);
    const after_free = phys.stats().free_frames;
    const returned_frames: u32 = if (after_free > before_free) @intCast(after_free - before_free) else 0;
    if (returned_frames == 0) {
        page_state_summary.eviction_failures +%= 1;
        return .{ .outcome = .failed };
    }
    return .{
        .outcome = .returned,
        .returned_frames = returned_frames,
        .returned_bytes = @as(u64, returned_frames) * paging.PAGE_SIZE,
    };
}

fn clearEvictionBusy(range_id: u32, page_index: u64) void {
    pageStateSet(range_id, page_index, 1, 0, page_state_flag_busy, null, true) catch {};
}

fn recordPagerPageOutFailure(dirty: bool) void {
    page_state_summary.pager_failed_page_outs +%= 1;
    page_state_summary.pager_data_preserved_pages +%= 1;
    if (dirty) page_state_summary.pager_dirty_preserved_pages +%= 1;
}

fn markEvictionError(range_id: u32, page_index: u64) void {
    pageStateSet(range_id, page_index, 1, page_state_flag_error, page_state_flag_busy, null, true) catch {};
}

fn releaseEvictionSlot(backing: backing_store.Result, reservation_id: u32, owner_id: u32, region_id: u32) void {
    _ = backing_store.slotProbe(.{
        .operation = backing_store.slot_operation_release,
        .reservation_id = reservation_id,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = owner_id,
        .region_id = region_id,
        .backing = backing,
    });
}

fn recordResident(range: *Range, bytes: u64) void {
    range.resident_bytes = checkedAdd(range.resident_bytes, bytes) orelse ~@as(u64, 0);
    if (range.resident_bytes > range.peak_resident_bytes) range.peak_resident_bytes = range.resident_bytes;
}

fn recordDemandFaultSuccess(range: *Range) void {
    range.fault_count = checkedAdd(range.fault_count, 1) orelse ~@as(u64, 0);
    recordResident(range, paging.PAGE_SIZE);
}

fn recordDemandFaultFailure(range: *Range) void {
    range.failed_faults = checkedAdd(range.failed_faults, 1) orelse ~@as(u64, 0);
}

fn makePageStateResult(input: PageStateInput) PageStateResult {
    return .{
        .operation = input.operation,
        .flags = page_state_flag_vm_owned_state |
            page_state_flag_explicit_request |
            page_state_flag_eviction_enabled |
            page_state_flag_no_swap |
            page_state_flag_page_sized |
            page_state_flag_fault_page_in,
        .region_id = input.region_id,
        .page_size = @intCast(paging.PAGE_SIZE),
        .max_spans = @intCast(MAX_PAGE_STATE_SPANS),
        .span_count = activePageStateSpanCount(),
        .region_offset = input.region_offset,
        .page_count = input.page_count,
        .page_count_lo = if (input.page_count > std.math.maxInt(u32)) std.math.maxInt(u32) else @intCast(input.page_count),
        .slot_reservation_id = input.slot_reservation_id,
        .slot_index = input.slot_index,
        .slot_generation = input.slot_generation,
        .total_transitions = page_state_summary.transitions,
        .dirty_marks = page_state_summary.dirty_marks,
        .clean_marks = page_state_summary.clean_marks,
        .slot_binds = page_state_summary.slot_binds,
        .slot_clears = page_state_summary.slot_clears,
        .pinned_marks = page_state_summary.pinned_marks,
        .pinned_clears = page_state_summary.pinned_clears,
        .busy_marks = page_state_summary.busy_marks,
        .busy_clears = page_state_summary.busy_clears,
        .error_marks = page_state_summary.error_marks,
        .error_clears = page_state_summary.error_clears,
        .table_full_failures = page_state_summary.table_full_failures,
        .cleanup_pages = page_state_summary.cleanup_pages,
    };
}

fn validatePageStateInput(input: PageStateInput, result: *PageStateResult) bool {
    if (!initialized) {
        result.status = page_state_status_not_initialized;
        result.blockers |= page_state_blocker_not_initialized;
        return false;
    }
    if ((input.flags & ~page_state_supported_flags) != 0) {
        result.status = page_state_status_unsupported_flags;
        result.blockers |= page_state_blocker_unsupported_flags;
        return false;
    }
    if (!validPageStateOperation(input.operation)) {
        result.status = page_state_status_unsupported_operation;
        result.blockers |= page_state_blocker_unsupported_operation;
        return false;
    }
    if (input.region_id == 0 or input.page_count == 0) {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
        return false;
    }
    if (!isAligned(input.region_offset, paging.PAGE_SIZE)) {
        result.status = page_state_status_unaligned_request;
        result.blockers |= page_state_blocker_unaligned_request;
        return false;
    }
    if (input.page_count > std.math.maxInt(u64) / paging.PAGE_SIZE) {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
        return false;
    }
    return true;
}

fn validPageStateOperation(operation: u32) bool {
    return operation == page_state_operation_query or
        operation == page_state_operation_mark_dirty or
        operation == page_state_operation_mark_clean or
        operation == page_state_operation_bind_slot or
        operation == page_state_operation_clear_slot or
        operation == page_state_operation_mark_pinned or
        operation == page_state_operation_clear_pinned or
        operation == page_state_operation_mark_busy or
        operation == page_state_operation_clear_busy or
        operation == page_state_operation_mark_error or
        operation == page_state_operation_clear_error;
}

fn applyPageStateOperation(range: Range, input: PageStateInput, first_page: u64, result: *PageStateResult) Error!void {
    switch (input.operation) {
        page_state_operation_query => {},
        page_state_operation_mark_dirty => {
            try pageStateSet(range.id, first_page, input.page_count, page_state_flag_dirty, 0, null, true);
            applyDirtyToResidentPages(range, first_page, input.page_count, true);
            page_state_summary.dirty_marks +%= input.page_count;
        },
        page_state_operation_mark_clean => {
            try pageStateSet(range.id, first_page, input.page_count, 0, page_state_flag_dirty, null, true);
            applyDirtyToResidentPages(range, first_page, input.page_count, false);
            page_state_summary.clean_marks +%= input.page_count;
        },
        page_state_operation_bind_slot => {
            try pageStateSet(range.id, first_page, input.page_count, page_state_flag_slot_bound, 0, .{
                .reservation_id = input.slot_reservation_id,
                .slot_index = input.slot_index,
                .slot_generation = input.slot_generation,
            }, true);
            page_state_summary.slot_binds +%= input.page_count;
        },
        page_state_operation_clear_slot => {
            try pageStateSet(range.id, first_page, input.page_count, 0, page_state_flag_slot_bound, .{
                .reservation_id = 0,
                .slot_index = 0,
                .slot_generation = 0,
            }, true);
            page_state_summary.slot_clears +%= input.page_count;
        },
        page_state_operation_mark_pinned => {
            try pageStateSet(range.id, first_page, input.page_count, page_state_flag_pinned, 0, null, true);
            page_state_summary.pinned_marks +%= input.page_count;
        },
        page_state_operation_clear_pinned => {
            try pageStateSet(range.id, first_page, input.page_count, 0, page_state_flag_pinned, null, true);
            page_state_summary.pinned_clears +%= input.page_count;
        },
        page_state_operation_mark_busy => {
            try pageStateSet(range.id, first_page, input.page_count, page_state_flag_busy, 0, null, true);
            page_state_summary.busy_marks +%= input.page_count;
        },
        page_state_operation_clear_busy => {
            try pageStateSet(range.id, first_page, input.page_count, 0, page_state_flag_busy, null, true);
            page_state_summary.busy_clears +%= input.page_count;
        },
        page_state_operation_mark_error => {
            try pageStateSet(range.id, first_page, input.page_count, page_state_flag_error, 0, null, true);
            page_state_summary.error_marks +%= input.page_count;
        },
        page_state_operation_clear_error => {
            try pageStateSet(range.id, first_page, input.page_count, 0, page_state_flag_error, null, true);
            page_state_summary.error_clears +%= input.page_count;
        },
        else => unreachable,
    }
    result.total_transitions = page_state_summary.transitions;
}

const SlotBinding = struct {
    reservation_id: u32 = 0,
    slot_index: u64 = 0,
    slot_generation: u64 = 0,
};

fn addPageStateSpan(range_id: u32, first_page: u64, page_count: u64, flags: u32, reservation_id: u32, slot_index: u64, slot_generation: u64) Error!void {
    if (page_count == 0) return;
    _ = checkedAdd(first_page, page_count) orelse return Error.Overflow;
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    hot_path_stats.page_state_span_lookups +%= 1;

    var previous: ?u16 = null;
    var current = range.page_state_span_head;
    var steps: usize = 0;
    while (current) |current_slot| {
        steps += 1;
        if (page_state_spans[current_slot].first_page >= first_page) break;
        previous = current_slot;
        current = page_state_spans[current_slot].next;
    }
    recordPageStateSpanSteps(steps);

    const slot = allocPageStateSpanSlot() orelse return Error.TableFull;
    page_state_spans[slot] = .{
        .slot_used = true,
        .range_id = range_id,
        .first_page = first_page,
        .page_count = page_count,
        .flags = flags,
        .slot_reservation_id = reservation_id,
        .slot_index = slot_index,
        .slot_generation = slot_generation,
        .prev = previous,
        .next = current,
    };
    if (previous) |prev_slot| {
        page_state_spans[prev_slot].next = slot;
    } else {
        range.page_state_span_head = slot;
    }
    if (current) |next_slot| page_state_spans[next_slot].prev = slot;
    try coalescePageStateSpansForRange(range);
    page_state_summary.transitions +%= 1;
}

fn pageStateSet(range_id: u32, first_page: u64, page_count: u64, add_flags: u32, remove_flags: u32, binding: ?SlotBinding, count_transition: bool) Error!void {
    if (page_count == 0) return;
    const end_page = checkedAdd(first_page, page_count) orelse return Error.Overflow;
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    var reserved = try reservePageStateSplitSlots(range, first_page, end_page);
    defer releaseReservedPageStateSlots(&reserved);
    try splitPageStateBoundariesReserved(range, first_page, end_page, &reserved);

    var changed = false;
    hot_path_stats.page_state_span_lookups +%= 1;
    var current = range.page_state_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = page_state_spans[slot].next) {
        steps += 1;
        var span = &page_state_spans[slot];
        if (span.first_page >= end_page) break;
        const span_end = checkedAdd(span.first_page, span.page_count) orelse return Error.Overflow;
        if (span.first_page < first_page or span_end > end_page) continue;
        const before_flags = span.flags;
        span.flags |= add_flags;
        span.flags &= ~remove_flags;
        if (binding) |slot_binding| {
            span.slot_reservation_id = slot_binding.reservation_id;
            span.slot_index = slot_binding.slot_index + (span.first_page - first_page);
            span.slot_generation = slot_binding.slot_generation;
        }
        if ((span.flags & page_state_flag_slot_bound) == 0) {
            span.slot_reservation_id = 0;
            span.slot_index = 0;
            span.slot_generation = 0;
        }
        if (before_flags != span.flags or binding != null) changed = true;
    }
    recordPageStateSpanSteps(steps);

    try coalescePageStateSpansForRange(range);
    if (changed and count_transition) page_state_summary.transitions +%= 1;
}

fn reservePageStateSplitSlots(range: *const Range, first_page: u64, end_page: u64) Error![2]?u16 {
    var needed: usize = 0;
    hot_path_stats.page_state_span_lookups +%= 1;
    var current = range.page_state_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = page_state_spans[slot].next) {
        steps += 1;
        const span = page_state_spans[slot];
        if (span.first_page >= end_page) break;
        const span_end = checkedAdd(span.first_page, span.page_count) orelse return Error.Overflow;
        if (first_page > span.first_page and first_page < span_end) needed += 1;
        if (end_page > span.first_page and end_page < span_end) needed += 1;
        if (needed == 2) break;
    }
    recordPageStateSpanSteps(steps);

    if (@as(usize, page_state_span_active_count) + needed > MAX_PAGE_STATE_SPANS) return Error.TableFull;
    var reserved: [2]?u16 = .{ null, null };
    var i: usize = 0;
    while (i < needed) : (i += 1) {
        reserved[i] = allocPageStateSpanSlot() orelse {
            releaseReservedPageStateSlots(&reserved);
            return Error.TableFull;
        };
    }
    return reserved;
}

fn splitPageStateBoundariesReserved(range: *Range, first_page: u64, end_page: u64, reserved: *[2]?u16) Error!void {
    try splitPageStateBoundaryReserved(range, first_page, reserved);
    if (end_page != first_page) try splitPageStateBoundaryReserved(range, end_page, reserved);
}

fn splitPageStateBoundaryReserved(range: *Range, boundary: u64, reserved: *[2]?u16) Error!void {
    var current = range.page_state_span_head;
    while (current) |slot| : (current = page_state_spans[slot].next) {
        const span = page_state_spans[slot];
        if (boundary <= span.first_page) return;
        const span_end = checkedAdd(span.first_page, span.page_count) orelse return Error.Overflow;
        if (boundary >= span_end) continue;
        const right_slot = takeReservedPageStateSlot(reserved) orelse return Error.TableFull;
        splitPageStateSpanReserved(slot, boundary, right_slot);
        return;
    }
}

fn takeReservedPageStateSlot(reserved: *[2]?u16) ?u16 {
    var i: usize = 0;
    while (i < reserved.len) : (i += 1) {
        if (reserved[i]) |slot| {
            reserved[i] = null;
            return slot;
        }
    }
    return null;
}

fn splitPageStateSpanReserved(slot: u16, split_page: u64, right_slot: u16) void {
    var span = &page_state_spans[slot];
    const span_end = span.first_page + span.page_count;
    const right_count = span_end - split_page;
    const old_next = span.next;
    page_state_spans[right_slot] = .{
        .slot_used = true,
        .range_id = span.range_id,
        .first_page = split_page,
        .page_count = right_count,
        .flags = span.flags,
        .slot_reservation_id = span.slot_reservation_id,
        .slot_index = if ((span.flags & page_state_flag_slot_bound) != 0) span.slot_index + (split_page - span.first_page) else 0,
        .slot_generation = span.slot_generation,
        .prev = slot,
        .next = old_next,
    };
    span.next = right_slot;
    span.page_count = split_page - span.first_page;
    if (old_next) |next_slot| page_state_spans[next_slot].prev = right_slot;
}

fn removePageStateRange(range_id: u32, first_page: u64, page_count: u64) Error!void {
    if (page_count == 0) return;
    const end_page = checkedAdd(first_page, page_count) orelse return Error.Overflow;
    const range_index = indexById(range_id) orelse return Error.NotFound;
    const range = &ranges[range_index];
    var reserved = try reservePageStateSplitSlots(range, first_page, end_page);
    defer releaseReservedPageStateSlots(&reserved);
    try removePageStateRangeReserved(range, first_page, end_page, &reserved);
}

fn removePageStateRangeReserved(range: *Range, first_page: u64, end_page: u64, reserved: *[2]?u16) Error!void {
    try splitPageStateBoundariesReserved(range, first_page, end_page, reserved);
    var removed_pages: u64 = 0;
    var current = range.page_state_span_head;
    while (current) |slot| {
        const next = page_state_spans[slot].next;
        const span = page_state_spans[slot];
        if (span.first_page >= end_page) break;
        const span_end = checkedAdd(span.first_page, span.page_count) orelse return Error.Overflow;
        if (span.first_page >= first_page and span_end <= end_page) {
            removed_pages +%= span.page_count;
            unlinkPageStateSpan(range, slot);
            releasePageStateSpanSlot(slot);
        }
        current = next;
    }
    page_state_summary.cleanup_pages +%= removed_pages;
    try coalescePageStateSpansForRange(range);
}

fn removePageStateForRange(range_id: u32) void {
    const range_index = indexById(range_id) orelse return;
    const range = &ranges[range_index];
    var removed_pages: u64 = 0;
    var current = range.page_state_span_head;
    while (current) |slot| {
        const next = page_state_spans[slot].next;
        removed_pages +%= page_state_spans[slot].page_count;
        releasePageStateSpanSlot(slot);
        current = next;
    }
    range.page_state_span_head = null;
    page_state_summary.cleanup_pages +%= removed_pages;
}

fn coalescePageStateSpansForRange(range: *Range) Error!void {
    var current = range.page_state_span_head;
    while (current) |slot| {
        const next = page_state_spans[slot].next orelse break;
        const span_end = checkedAdd(page_state_spans[slot].first_page, page_state_spans[slot].page_count) orelse return Error.Overflow;
        if (span_end == page_state_spans[next].first_page and pageStateSpansMergeable(page_state_spans[slot], page_state_spans[next])) {
            page_state_spans[slot].page_count += page_state_spans[next].page_count;
            unlinkPageStateSpan(range, next);
            releasePageStateSpanSlot(next);
        } else {
            current = next;
        }
    }
}

fn pageStateSpansMergeable(a: PageStateSpan, b: PageStateSpan) bool {
    if (a.flags != b.flags) return false;
    if (a.slot_reservation_id != b.slot_reservation_id or a.slot_generation != b.slot_generation) return false;
    if ((a.flags & page_state_flag_slot_bound) == 0) return true;
    return b.slot_index == a.slot_index + a.page_count;
}

fn fillPageStateCounts(range_id: u32, first_page: u64, page_count: u64, result: *PageStateResult) void {
    const end_page = checkedAdd(first_page, page_count) orelse return;
    result.span_count = activePageStateSpanCount();
    result.flags |= page_state_flag_hardware_dirty_synced;
    const range_index = indexById(range_id) orelse return;
    const range = &ranges[range_index];
    var first_slot_seen = false;
    hot_path_stats.page_state_span_lookups +%= 1;
    var current = range.page_state_span_head;
    var steps: usize = 0;
    while (current) |slot| : (current = page_state_spans[slot].next) {
        steps += 1;
        const span = page_state_spans[slot];
        if (span.first_page >= end_page) break;
        const span_end = checkedAdd(span.first_page, span.page_count) orelse continue;
        if (!pageRangesOverlap(first_page, end_page, span.first_page, span_end)) continue;
        const overlap_start = if (span.first_page > first_page) span.first_page else first_page;
        const overlap_end = if (span_end < end_page) span_end else end_page;
        const pages = overlap_end - overlap_start;
        result.flags |= span.flags;
        if ((span.flags & page_state_flag_committed) != 0) result.committed_pages += pages;
        if ((span.flags & page_state_flag_resident) != 0) result.resident_pages += pages;
        if ((span.flags & page_state_flag_dirty) != 0) result.dirty_pages += pages;
        if ((span.flags & page_state_flag_pinned) != 0) result.pinned_pages += pages;
        if ((span.flags & page_state_flag_busy) != 0) result.busy_pages += pages;
        if ((span.flags & page_state_flag_error) != 0) result.error_pages += pages;
        if ((span.flags & page_state_flag_slot_bound) != 0) {
            result.slot_bound_pages += pages;
            if (!first_slot_seen) {
                result.slot_reservation_id = span.slot_reservation_id;
                result.slot_index = span.slot_index + (overlap_start - span.first_page);
                result.slot_generation = span.slot_generation;
                first_slot_seen = true;
            }
        }
    }
    recordPageStateSpanSteps(steps);
    if (result.committed_pages >= result.resident_pages) result.nonresident_pages = result.committed_pages - result.resident_pages;
    if (result.committed_pages >= result.dirty_pages) result.clean_pages = result.committed_pages - result.dirty_pages;
    result.total_transitions = page_state_summary.transitions;
    result.dirty_marks = page_state_summary.dirty_marks;
    result.clean_marks = page_state_summary.clean_marks;
    result.slot_binds = page_state_summary.slot_binds;
    result.slot_clears = page_state_summary.slot_clears;
    result.pinned_marks = page_state_summary.pinned_marks;
    result.pinned_clears = page_state_summary.pinned_clears;
    result.busy_marks = page_state_summary.busy_marks;
    result.busy_clears = page_state_summary.busy_clears;
    result.error_marks = page_state_summary.error_marks;
    result.error_clears = page_state_summary.error_clears;
    result.table_full_failures = page_state_summary.table_full_failures;
    result.cleanup_pages = page_state_summary.cleanup_pages;
}

fn syncHardwareDirty(range: Range, first_page: u64, page_count: u64) void {
    if (range.window != .r4x_vm or page_count == 0) return;
    var i: u64 = 0;
    while (i < page_count) : (i += 1) {
        const page_index = first_page + i;
        const virt = range.base + page_index * paging.PAGE_SIZE;
        if (!paging.isMapped(virt)) continue;
        if (paging.pageDirty(virt)) {
            pageStateSet(range.id, page_index, 1, page_state_flag_dirty, 0, null, false) catch {};
        }
    }
}

fn applyDirtyToResidentPages(range: Range, first_page: u64, page_count: u64, dirty: bool) void {
    var i: u64 = 0;
    while (i < page_count) : (i += 1) {
        const virt = range.base + (first_page + i) * paging.PAGE_SIZE;
        if (!paging.isMapped(virt)) continue;
        if (dirty) {
            _ = paging.markDirty(virt);
        } else {
            _ = paging.clearDirty(virt);
        }
    }
}

fn recordPageState(result: *const PageStateResult) void {
    page_state_summary.enabled = true;
    page_state_summary.last_status = result.status;
    page_state_summary.last_operation = result.operation;
    page_state_summary.last_flags = result.flags;
    page_state_summary.last_blockers = result.blockers;
    page_state_summary.last_region_id = result.region_id;
    page_state_summary.last_region_offset = result.region_offset;
    page_state_summary.last_page_count = result.page_count;
    page_state_summary.last_slot_reservation_id = result.slot_reservation_id;
    page_state_summary.last_slot_index = result.slot_index;
    page_state_summary.last_slot_generation = result.slot_generation;
    page_state_summary.committed_pages = result.committed_pages;
    page_state_summary.resident_pages = result.resident_pages;
    page_state_summary.nonresident_pages = result.nonresident_pages;
    page_state_summary.dirty_pages = result.dirty_pages;
    page_state_summary.clean_pages = result.clean_pages;
    page_state_summary.pinned_pages = result.pinned_pages;
    page_state_summary.busy_pages = result.busy_pages;
    page_state_summary.error_pages = result.error_pages;
    page_state_summary.slot_bound_pages = result.slot_bound_pages;
    page_state_summary.span_count = result.span_count;
}

fn activePageStateSpanCount() u32 {
    return page_state_span_active_count;
}

fn allocPageStateSpanSlot() ?u16 {
    const slot = page_state_span_free_head orelse return null;
    page_state_span_free_head = page_state_spans[slot].next;
    page_state_spans[slot] = .{};
    page_state_span_active_count +|= 1;
    return slot;
}

fn releasePageStateSpanSlot(slot: u16) void {
    page_state_spans[slot] = .{ .next = page_state_span_free_head };
    page_state_span_free_head = slot;
    if (page_state_span_active_count != 0) page_state_span_active_count -= 1;
}

fn releaseReservedPageStateSlots(reserved: *[2]?u16) void {
    var i: usize = 0;
    while (i < reserved.len) : (i += 1) {
        if (reserved[i]) |slot| {
            releasePageStateSpanSlot(slot);
            reserved[i] = null;
        }
    }
}

fn unlinkPageStateSpan(range: *Range, slot: u16) void {
    const previous = page_state_spans[slot].prev;
    const next = page_state_spans[slot].next;
    if (previous) |prev_slot| {
        page_state_spans[prev_slot].next = next;
    } else {
        range.page_state_span_head = next;
    }
    if (next) |next_slot| page_state_spans[next_slot].prev = previous;
}

fn pageRangesOverlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) bool {
    return a_start < b_end and b_start < a_end;
}

fn alignDownValue(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn largestFreeInWindow(window: Window) FreeSpan {
    const def = windowDef(window);
    var best: FreeSpan = .{ .base = def.base, .len = 0 };
    var cursor = def.base;
    const window_end = checkedEnd(def.base, def.len) orelse return best;

    var pos = lowerBoundAddress(def.base);
    while (pos < range_address_count) : (pos += 1) {
        const range = ranges[range_address_order[pos]];
        if (range.base >= window_end) break;
        if (range.window != window) continue;
        if (range.base > cursor) {
            const free_len = range.base - cursor;
            if (free_len > best.len) best = .{ .base = cursor, .len = free_len };
        }
        const range_end = checkedEnd(range.base, range.len) orelse return best;
        if (range_end > cursor) cursor = range_end;
    }

    if (cursor < window_end and window_end - cursor > best.len) {
        best = .{ .base = cursor, .len = window_end - cursor };
    }

    return best;
}

fn infoFromRange(range: Range) RangeInfo {
    return .{
        .id = range.id,
        .window = range.window,
        .kind = range.kind,
        .owner = range.owner,
        .owner_id = range.owner_id,
        .status = range.status,
        .name = range.name,
        .flags = range.flags,
        .base = range.base,
        .len = range.len,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .peak_resident_bytes = range.peak_resident_bytes,
        .fault_count = range.fault_count,
        .failed_faults = range.failed_faults,
        .guard_base = range.guard_base,
        .guard_len = range.guard_len,
    };
}

fn freeSlot() ?usize {
    hot_path_stats.range_free_slot_lookups +%= 1;
    var probes: usize = 0;
    while (probes < ranges.len) : (probes += 1) {
        const i = (next_free_range_slot + probes) % ranges.len;
        if (!ranges[i].slot_used or ranges[i].status == .released) {
            recordFreeSlotProbe(probes + 1);
            next_free_range_slot = (i + 1) % ranges.len;
            return i;
        }
    }
    recordFreeSlotProbe(probes);
    return null;
}

fn lowerBoundAddress(base: u64) usize {
    var low: usize = 0;
    var high = range_address_count;
    while (low < high) {
        const mid = low + (high - low) / 2;
        if (ranges[range_address_order[mid]].base < base) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    return low;
}

fn insertRangeAddressIndex(slot: usize) void {
    const pos = lowerBoundAddress(ranges[slot].base);
    var i = range_address_count;
    while (i > pos) {
        range_address_order[i] = range_address_order[i - 1];
        range_address_position[range_address_order[i]] = @intCast(i);
        i -= 1;
    }
    range_address_order[pos] = @intCast(slot);
    range_address_position[slot] = @intCast(pos);
    range_address_count += 1;
}

fn removeRangeAddressIndex(slot: usize) void {
    const raw_pos = range_address_position[slot];
    if (raw_pos == INVALID_RANGE_POSITION) return;
    const pos: usize = raw_pos;
    if (pos >= range_address_count or range_address_order[pos] != slot) return;

    if (reclaim_cursor_range_slot != null and reclaim_cursor_range_slot.? == slot) {
        if (range_address_count > 1) {
            const next_pos = if (pos + 1 < range_address_count) pos + 1 else 0;
            reclaim_cursor_range_slot = range_address_order[next_pos];
            reclaim_cursor_page = 0;
        } else {
            reclaim_cursor_range_slot = null;
            reclaim_cursor_page = 0;
        }
    }

    var i = pos;
    while (i + 1 < range_address_count) : (i += 1) {
        range_address_order[i] = range_address_order[i + 1];
        range_address_position[range_address_order[i]] = @intCast(i);
    }
    range_address_count -= 1;
    range_address_position[slot] = INVALID_RANGE_POSITION;
}

fn allocId() Error!u32 {
    if (next_id == 0) return Error.Overflow;
    const id = next_id;
    next_id += 1;
    return id;
}

fn indexById(id: u32) ?usize {
    hot_path_stats.range_index_lookups +%= 1;
    var probes: usize = 0;
    var pos = rangeIdIndexStart(id);
    while (probes < range_id_index.len) : (probes += 1) {
        const entry = range_id_index[pos];
        switch (entry.state) {
            .empty => {
                recordRangeIndexProbe(probes + 1);
                hot_path_stats.range_index_misses +%= 1;
                return null;
            },
            .tombstone => {},
            .used => {
                if (entry.id == id) {
                    recordRangeIndexProbe(probes + 1);
                    if (entry.slot < ranges.len and ranges[entry.slot].id == id and ranges[entry.slot].active()) {
                        hot_path_stats.range_index_hits +%= 1;
                        return entry.slot;
                    }
                    hot_path_stats.range_index_misses +%= 1;
                    return null;
                }
            },
        }
        pos = (pos + 1) % range_id_index.len;
    }
    recordRangeIndexProbe(probes);
    hot_path_stats.range_index_misses +%= 1;
    return null;
}

fn rangeIdIndexStart(id: u32) usize {
    const mixed = @as(u64, id) *% 11400714819323198485;
    return @intCast(mixed % RANGE_ID_INDEX_CAPACITY);
}

fn insertRangeIdIndex(id: u32, slot: usize) bool {
    if (insertRangeIdIndexNoRebuild(id, slot)) return true;
    rebuildRangeIdIndex();
    if (insertRangeIdIndexNoRebuild(id, slot)) return true;
    hot_path_stats.range_index_insert_failures +%= 1;
    return false;
}

fn insertRangeIdIndexNoRebuild(id: u32, slot: usize) bool {
    var first_tombstone: ?usize = null;
    var probes: usize = 0;
    var pos = rangeIdIndexStart(id);
    while (probes < range_id_index.len) : (probes += 1) {
        switch (range_id_index[pos].state) {
            .empty => {
                const target = first_tombstone orelse pos;
                if (range_id_index[target].state == .tombstone and range_id_index_tombstones != 0) {
                    range_id_index_tombstones -= 1;
                }
                range_id_index[target] = .{ .state = .used, .id = id, .slot = slot };
                range_id_index_entries += 1;
                return true;
            },
            .tombstone => {
                if (first_tombstone == null) first_tombstone = pos;
            },
            .used => {
                if (range_id_index[pos].id == id) {
                    range_id_index[pos].slot = slot;
                    return true;
                }
            },
        }
        pos = (pos + 1) % range_id_index.len;
    }
    if (first_tombstone) |target| {
        range_id_index[target] = .{ .state = .used, .id = id, .slot = slot };
        range_id_index_entries += 1;
        if (range_id_index_tombstones != 0) range_id_index_tombstones -= 1;
        return true;
    }
    return false;
}

fn removeRangeIdIndex(id: u32) void {
    var probes: usize = 0;
    var pos = rangeIdIndexStart(id);
    while (probes < range_id_index.len) : (probes += 1) {
        switch (range_id_index[pos].state) {
            .empty => return,
            .tombstone => {},
            .used => {
                if (range_id_index[pos].id == id) {
                    range_id_index[pos] = .{ .state = .tombstone };
                    if (range_id_index_entries != 0) range_id_index_entries -= 1;
                    range_id_index_tombstones += 1;
                    return;
                }
            },
        }
        pos = (pos + 1) % range_id_index.len;
    }
}

fn rebuildRangeIdIndex() void {
    hot_path_stats.range_index_rebuilds +%= 1;
    var i: usize = 0;
    while (i < range_id_index.len) : (i += 1) range_id_index[i] = .{};
    range_id_index_entries = 0;
    range_id_index_tombstones = 0;
    i = 0;
    while (i < ranges.len) : (i += 1) {
        const range = ranges[i];
        if (!range.active()) continue;
        if (!insertRangeIdIndexNoRebuild(range.id, i)) {
            hot_path_stats.range_index_insert_failures +%= 1;
            return;
        }
    }
}

fn recordRangeIndexProbe(probes: usize) void {
    const p: u32 = @intCast(@min(probes, std.math.maxInt(u32)));
    hot_path_stats.range_index_probe_last = p;
    hot_path_stats.range_index_probe_total +%= p;
    if (p > hot_path_stats.range_index_probe_max) hot_path_stats.range_index_probe_max = p;
}

fn recordFreeSlotProbe(probes: usize) void {
    const p: u32 = @intCast(@min(probes, std.math.maxInt(u32)));
    hot_path_stats.range_free_slot_probe_last = p;
    hot_path_stats.range_free_slot_probe_total +%= p;
    if (p > hot_path_stats.range_free_slot_probe_max) hot_path_stats.range_free_slot_probe_max = p;
}

fn recordRangeAddressProbe(probes: usize) void {
    const p: u32 = @intCast(@min(probes, std.math.maxInt(u32)));
    hot_path_stats.range_address_probe_last = p;
    hot_path_stats.range_address_probe_total +%= p;
    if (p > hot_path_stats.range_address_probe_max) hot_path_stats.range_address_probe_max = p;
}

fn recordCommitSpanSteps(steps: usize) void {
    const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
    hot_path_stats.commit_span_steps +%= value;
    if (value > hot_path_stats.commit_span_step_max) hot_path_stats.commit_span_step_max = value;
}

fn recordPageStateSpanSteps(steps: usize) void {
    const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
    hot_path_stats.page_state_span_steps +%= value;
    if (value > hot_path_stats.page_state_span_step_max) hot_path_stats.page_state_span_step_max = value;
}

pub fn hotPathStats() HotPathStats {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    var out = hot_path_stats;
    out.range_index_capacity = @intCast(RANGE_ID_INDEX_CAPACITY);
    out.range_index_entries = @intCast(@min(range_id_index_entries, std.math.maxInt(u32)));
    out.range_index_tombstones = @intCast(@min(range_id_index_tombstones, std.math.maxInt(u32)));
    out.range_address_entries = @intCast(@min(range_address_count, std.math.maxInt(u32)));
    out.commit_span_active = commit_span_active_count;
    out.page_state_span_active = page_state_span_active_count;
    return out;
}

pub fn metadataInvariant() bool {
    const owner_irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(owner_irq_flags);
    if (!initialized or range_address_count != range_id_index_entries) return false;
    var commit_seen: [MAX_COMMIT_SPANS / 64]u64 = .{0} ** (MAX_COMMIT_SPANS / 64);
    var page_seen: [MAX_PAGE_STATE_SPANS / 64]u64 = .{0} ** (MAX_PAGE_STATE_SPANS / 64);
    var active_ranges: usize = 0;
    var linked_commit_spans: usize = 0;
    var linked_page_spans: usize = 0;

    var slot_index: usize = 0;
    while (slot_index < ranges.len) : (slot_index += 1) {
        const range = &ranges[slot_index];
        if (!range.active()) {
            if (range_address_position[slot_index] != INVALID_RANGE_POSITION) return false;
            continue;
        }
        active_ranges += 1;
        const position = range_address_position[slot_index];
        if (position == INVALID_RANGE_POSITION or @as(usize, position) >= range_address_count or
            range_address_order[position] != slot_index or rawIndexById(range.id) != slot_index)
        {
            return false;
        }

        var previous_commit: ?u16 = null;
        var commit_cursor = range.commit_span_head;
        var commit_steps: usize = 0;
        while (commit_cursor) |span_slot| : (commit_cursor = commit_spans[span_slot].next) {
            commit_steps += 1;
            if (commit_steps > MAX_COMMIT_SPANS or !markMetadataSeen(commit_seen[0..], span_slot)) return false;
            const span = commit_spans[span_slot];
            if (!span.slot_used or span.range_id != range.id or span.prev != previous_commit or span.len == 0) return false;
            if (previous_commit) |previous_slot| {
                const previous_end = checkedEnd(commit_spans[previous_slot].base, commit_spans[previous_slot].len) orelse return false;
                if (previous_end >= span.base) return false;
            }
            previous_commit = span_slot;
            linked_commit_spans += 1;
        }

        var previous_page: ?u16 = null;
        var page_cursor = range.page_state_span_head;
        var page_steps: usize = 0;
        while (page_cursor) |span_slot| : (page_cursor = page_state_spans[span_slot].next) {
            page_steps += 1;
            if (page_steps > MAX_PAGE_STATE_SPANS or !markMetadataSeen(page_seen[0..], span_slot)) return false;
            const span = page_state_spans[span_slot];
            if (!span.slot_used or span.range_id != range.id or span.prev != previous_page or span.page_count == 0) return false;
            if (previous_page) |previous_slot| {
                const previous_end = checkedAdd(page_state_spans[previous_slot].first_page, page_state_spans[previous_slot].page_count) orelse return false;
                if (previous_end > span.first_page or
                    (previous_end == span.first_page and pageStateSpansMergeable(page_state_spans[previous_slot], span))) return false;
            }
            previous_page = span_slot;
            linked_page_spans += 1;
        }
    }
    if (active_ranges != range_address_count or linked_commit_spans != commit_span_active_count or
        linked_page_spans != page_state_span_active_count) return false;

    var position: usize = 0;
    while (position < range_address_count) : (position += 1) {
        const slot: usize = range_address_order[position];
        if (!ranges[slot].active() or range_address_position[slot] != position) return false;
        if (position != 0) {
            const previous = ranges[range_address_order[position - 1]];
            const previous_end = checkedEnd(previous.base, previous.len) orelse return false;
            if (previous_end > ranges[slot].base) return false;
        }
    }

    var free_commit_count: usize = 0;
    var free_commit = commit_span_free_head;
    while (free_commit) |span_slot| : (free_commit = commit_spans[span_slot].next) {
        free_commit_count += 1;
        if (free_commit_count > MAX_COMMIT_SPANS or commit_spans[span_slot].slot_used or
            !markMetadataSeen(commit_seen[0..], span_slot)) return false;
    }
    var free_page_count: usize = 0;
    var free_page = page_state_span_free_head;
    while (free_page) |span_slot| : (free_page = page_state_spans[span_slot].next) {
        free_page_count += 1;
        if (free_page_count > MAX_PAGE_STATE_SPANS or page_state_spans[span_slot].slot_used or
            !markMetadataSeen(page_seen[0..], span_slot)) return false;
    }
    if (linked_commit_spans + free_commit_count != MAX_COMMIT_SPANS or
        linked_page_spans + free_page_count != MAX_PAGE_STATE_SPANS) return false;
    if (reclaim_cursor_range_slot) |cursor_slot| {
        if (!ranges[cursor_slot].active() or range_address_position[cursor_slot] == INVALID_RANGE_POSITION) return false;
    }
    return true;
}

fn rawIndexById(id: u32) ?usize {
    var probes: usize = 0;
    var position = rangeIdIndexStart(id);
    while (probes < range_id_index.len) : (probes += 1) {
        const entry = range_id_index[position];
        switch (entry.state) {
            .empty => return null,
            .tombstone => {},
            .used => if (entry.id == id) return entry.slot,
        }
        position = (position + 1) % range_id_index.len;
    }
    return null;
}

fn markMetadataSeen(seen: []u64, raw_index: u16) bool {
    const index: usize = raw_index;
    const word_index = index / 64;
    const bit: u6 = @intCast(index % 64);
    const mask = @as(u64, 1) << bit;
    if ((seen[word_index] & mask) != 0) return false;
    seen[word_index] |= mask;
    return true;
}

fn firstOwnedRange(owner: blocks.Owner, owner_id: u64, kind: ?blocks.Kind) ?u32 {
    var i: usize = 0;
    while (i < ranges.len) : (i += 1) {
        const range = ranges[i];
        if (!range.active()) continue;
        if (range.owner != owner or range.owner_id != owner_id) continue;
        if (kind) |wanted| {
            if (range.kind != wanted) continue;
        }
        return range.id;
    }
    return null;
}

fn overlapsExisting(base: u64, len: u64) bool {
    const end = checkedEnd(base, len) orelse return true;
    const pos = lowerBoundAddress(base);
    if (pos > 0) {
        const previous = ranges[range_address_order[pos - 1]];
        const previous_end = checkedEnd(previous.base, previous.len) orelse return true;
        if (previous_end > base) return true;
    }
    if (pos < range_address_count) {
        const next = ranges[range_address_order[pos]];
        if (next.base < end) return true;
    }
    return false;
}

fn windowDef(window: Window) WindowDef {
    return WINDOWS[@intFromEnum(window)];
}

fn normalizeLen(len: u64) Error!u64 {
    if (len == 0) return Error.EmptyRange;
    return alignUpChecked(len, paging.PAGE_SIZE) orelse return Error.Overflow;
}

fn normalizeAlignment(alignment: u64) Error!u64 {
    if (alignment < paging.PAGE_SIZE or !isPowerOfTwo(alignment)) return Error.BadAlignment;
    return alignment;
}

fn checkedEnd(base: u64, len: u64) ?u64 {
    const end = base +% len;
    if (end < base) return null;
    return end;
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const sum = a +% b;
    if (sum < a) return null;
    return sum;
}

fn checkedMul(a: u64, b: u64) ?u64 {
    if (a != 0 and b > std.math.maxInt(u64) / a) return null;
    return a * b;
}

fn alignUpChecked(value: u64, alignment: u64) ?u64 {
    const add = alignment - 1;
    const sum = value +% add;
    if (sum < value) return null;
    return sum & ~add;
}

fn checkedAddInto(target: *u64, value: u64, overflow: *bool) void {
    const next = target.* +% value;
    if (next < target.*) {
        overflow.* = true;
        target.* = ~@as(u64, 0);
    } else {
        target.* = next;
    }
}

fn isAligned(value: u64, alignment: u64) bool {
    return (value & (alignment - 1)) == 0;
}

fn isPowerOfTwo(value: u64) bool {
    return value != 0 and (value & (value - 1)) == 0;
}

fn rangesOverlap(a_base: u64, a_len: u64, b_base: u64, b_len: u64) bool {
    const a_end = checkedEnd(a_base, a_len) orelse return true;
    const b_end = checkedEnd(b_base, b_len) orelse return true;
    return a_base < b_end and b_base < a_end;
}

fn convertBlockError(err: blocks.Error) Error {
    return switch (err) {
        error.NotInitialized => Error.NotInitialized,
        error.TableFull => Error.TableFull,
        error.EmptyRange => Error.EmptyRange,
        error.Overlap => Error.Overlap,
        error.NotFound => Error.NotFound,
        error.NotFree => Error.Overlap,
        error.InvalidBytes => Error.Overflow,
        error.InvalidRange => Error.OutsideWindow,
        error.Overflow => Error.Overflow,
    };
}
