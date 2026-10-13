const std = @import("std");
const blocks = @import("blocks.zig");
const backing_store = @import("backing_store.zig");
const page_batch = @import("page_batch.zig");
const paging = @import("paging.zig");
const phys = @import("phys.zig");
const reclaim = @import("reclaim.zig");
const owner_locks = @import("owner_locks.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const k = @import("../kernel/log.zig");
const sync = @import("../sched/sync.zig");
const task_context = @import("../sched/task_context.zig");
const scheduler = @import("../sched/scheduler.zig");
const r4sys_api = @import("../program/r4sys.zig");

pub const MAX_RANGES: usize = 1024;
pub const MAX_COMMIT_SPANS: usize = 4096;
pub const MAX_PAGE_STATE_SPANS: usize = 8192;
pub const WINDOW_COUNT: usize = 9;
pub const RANGE_ID_INDEX_CAPACITY: usize = MAX_RANGES * 2;
const INVALID_RANGE_POSITION: u16 = std.math.maxInt(u16);

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

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
    graphics = 8,
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
    Busy,
    OutsideWindow,
};

pub const ReserveRequest = struct {
    eviction_allowed: bool = true,
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
    eviction_allowed: bool = true,
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

pub const Stats = extern struct {
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

pub const OwnerStats = extern struct {
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
    resident_commit_batches: u64 = 0,
    resident_commit_max_batch_pages: u64 = 0,
    resident_commit_unlocked_zero_bytes: u64 = 0,
    resident_rollback_batches: u64 = 0,
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
    eviction_redirtied_pages: u64 = 0,
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

const CommitSpan = extern struct {
    slot_used: bool,
    range_id: u32,
    base: u64,
    len: u64,
    prev: OptionalSlot,
    next: OptionalSlot,
};
const CommitSpanStats = extern struct {
    commit_span_active: u32,
    commit_span_lookups: u64,
    commit_span_steps: u64,
    commit_span_step_max: u32,
};
extern fn r4os_rust_vm_commit_spans_init() *const [MAX_COMMIT_SPANS]CommitSpan;
extern fn r4os_rust_vm_commit_span_change(operation: u8, range_id: u32, base: u64, len: u64, reserved: ?*OptionalSlot) u32;
extern fn r4os_rust_vm_commit_span_scan(head: OptionalSlot, base: u64, len: u64, covers: bool) bool;
extern fn r4os_rust_vm_commit_span_cursor(free: bool) usize;
extern fn r4os_rust_vm_commit_span_stats(output: *CommitSpanStats) void;
fn commitSpanActiveCount() u32 {
    return @intCast(r4os_rust_vm_commit_span_cursor(false));
}
fn commitSpanFreeHead() ?u16 {
    const slot = r4os_rust_vm_commit_span_cursor(true);
    return if (slot == std.math.maxInt(usize)) null else @intCast(slot);
}
comptime {
    if (@sizeOf(CommitSpan) != 32 or @offsetOf(CommitSpan, "prev") != 24 or @sizeOf(CommitSpanStats) != 32)
        @compileError("Rust VM commit span C layout mismatch");
}

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

const OptionalSlot = extern struct {
    value: u16 = 0,
    present: bool = false,
    fn get(self: OptionalSlot) ?u16 {
        return if (self.present) self.value else null;
    }
    fn from(value: ?u16) OptionalSlot {
        return if (value) |slot| .{ .value = slot, .present = true } else .{};
    }
};
const Range = extern struct {
    slot_used: bool = false,
    id: u32 = 0,
    eviction_allowed: bool = true,
    window: Window = .temp_kernel,
    kind: blocks.Kind = .virtual_range,
    owner: blocks.Owner = .kernel,
    owner_id: u64 = 0,
    status: blocks.Status = .reserved,
    name_ptr: [*]const u8 = "",
    name_len: usize = 0,
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
    pager_io_count: u32 = 0,
    // A resident commit owns this range across short VM-owner sections.
    // Its private extent is zeroed through the HHDM before PTE publication.
    // Failed rollback keeps both the mapped prefix and any private extent
    // reachable by ID until a later commit/release can finish the cleanup.
    resident_commit_active: bool = false,
    resident_commit_failed: bool = false,
    resident_commit_base: u64 = 0,
    resident_commit_bytes: u64 = 0,
    resident_prepared: phys.FrameExtent = .{ .base = 0, .count = 0 },
    partial_uncommit_base: u64 = 0,
    partial_uncommit_len: u64 = 0,
    partial_uncommit_cursor: u64 = 0,
    partial_uncommit_accounted: bool = false,
    commit_span_head: OptionalSlot = .{},
    page_state_span_head: OptionalSlot = .{},

    fn active(self: Range) bool {
        return self.slot_used and self.status != .released;
    }
};

extern fn r4os_rust_vm_ranges_init() *[MAX_RANGES]Range;
extern fn r4os_rust_vm_range_info(id: u32, output: *Range) bool;
extern fn r4os_rust_vm_stats(output: *Stats) void;
extern fn r4os_rust_vm_owner_stats(owner: u8, owner_id: u64, window: u8, kind: u8, output: *OwnerStats) void;
extern fn r4os_rust_vm_window_usage(window: u8, output: *WindowUsage) void;
comptime {
    if (@sizeOf(Stats) != 104 or @sizeOf(OwnerStats) != 64 or @sizeOf(WindowUsage) != 24)
        @compileError("Rust VM query C layout mismatch");
}
extern fn r4os_rust_vm_initialized() bool;
extern fn r4os_rust_vm_publish_ready() void;
extern fn r4os_rust_vm_commit(id: u32, offset: u64, len: u64) u32;
extern fn r4os_rust_vm_commit_resident(id: u32, offset: u64, len: u64) u32;
extern fn r4os_rust_vm_uncommit(id: u32, offset: u64, len: u64) u32;
extern fn r4os_rust_vm_release(id: u32) u32;
extern fn r4os_rust_vm_finish_rollback(id: u32) u32;
extern fn r4os_rust_vm_protect_guard(id: u32, offset: u64, len: u64, clear: bool) u32;
extern fn r4os_rust_vm_partial(range: *Range, operation: u8, base: u64, len: u64, reset: bool) void;
extern fn r4os_rust_vm_uncommit_span(range: *Range, base: u64, len: u64, skip: bool) u32;
const ResidentStats = extern struct {
    resident_commit_batches: u64,
    resident_commit_max_batch_pages: u64,
    resident_commit_unlocked_zero_bytes: u64,
    resident_rollback_batches: u64,
};
extern fn r4os_rust_vm_transaction_stats(output: *ResidentStats) void;
comptime {
    if (@sizeOf(ResidentStats) != 32) @compileError("Rust resident VM stats layout mismatch");
}
export fn r4os_zig_vm_reschedule() callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM reschedule requested by host fixture");
    _ = scheduler.safeReschedulePoint();
}
// Fixed demand-pager cuts under the admitted VM owner, retaining the exact
// range identity. Rust ends all range references before calling these.
export fn r4os_zig_vm_demand_commit(range: *Range, offset: u64, len: u64) callconv(.c) u32 {
    if (@import("builtin").is_test) @panic("kernel demand commit requested by host fixture");
    demandCommit(range, offset, len) catch |err| return errorCode(err);
    return 0;
}
fn demandCommit(range: *Range, offset: u64, len: u64) Error!void {
    try validateUncommitted(range.*, range.base + offset, len);
    const next_committed = checkedAdd(range.committed_bytes, len) orelse return Error.Overflow;
    try addCommitSpan(range.id, range.base + offset, len);
    addPageStateSpan(range.id, offset / paging.PAGE_SIZE, len / paging.PAGE_SIZE, page_state_flag_committed | (if (range.eviction_allowed) @as(u32, 0) else page_state_flag_pinned), 0, 0, 0) catch |err| {
        removeCommitSpan(range.id, range.base + offset, len) catch {};
        return err;
    };
    range.committed_bytes = next_committed;
    range.status = .committed;
    blocks.setCommitted(range.block_id, range.committed_bytes) catch |err| return convertBlockError(err);
}
export fn r4os_zig_vm_uncommit_demand(range: *Range, base: u64, len: u64, skip: bool) callconv(.c) u32 {
    if (@import("builtin").is_test) @panic("kernel demand uncommit requested by host fixture");
    uncommitDemandSpan(range, base, len, skip) catch |err| return errorCode(err);
    return 0;
}
export fn r4os_zig_vm_pager_cleanup(range: *Range, operation: u8, slot: usize) callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM pager cleanup requested by host fixture");
    switch (operation) {
        0 => {
            _ = backing_store.releaseVmRegion(@intCast(range.owner_id), range.id);
        },
        1 => removePageStateForRange(range.id),
        2 => removeRangeAddressIndex(slot),
        else => unreachable,
    }
}
export fn r4os_zig_vm_resident_page_state(id: u32, first_page: u64, pages: u64, mark: bool) callconv(.c) u32 {
    if (@import("builtin").is_test) @panic("kernel resident page-state requested by host fixture");
    residentPageState(id, first_page, pages, mark) catch |err| return errorCode(err);
    return 0;
}
fn residentPageState(id: u32, first_page: u64, pages: u64, mark: bool) Error!void {
    if (mark) {
        try pageStateSet(id, first_page, pages, page_state_flag_resident, 0, null, true);
    } else {
        try addPageStateSpan(id, first_page, pages, page_state_flag_committed | page_state_flag_pinned, 0, 0, 0);
    }
}
extern fn r4os_rust_vm_allocate(range: *const Range, reason: u32, requested: u64, single: bool, output: *phys.FrameExtent) u32;
extern fn r4os_rust_vm_can_commit(range: *const Range, pages: u64) bool;
extern fn r4os_rust_vm_release_prepared(base: u64, count: u64, single: bool) void;
extern fn r4os_rust_vm_uncommit_extent(range: *Range, address: u64, base: u64, count: u64, logical: bool, single: bool) u32;
// Reclaim retains its original owner until 0.83.17. Rust ends the input
// snapshot's reference and rejects calls under the VM owner before this cut.
export fn r4os_zig_vm_reclaim(reason: u32, frames: u32) callconv(.c) u32 {
    if (@import("builtin").is_test) @panic("kernel VM reclaim requested by host fixture");
    return reclaim.reclaimFrames(@enumFromInt(reason), frames).returned_frames;
}
const RawReserve = extern struct {
    eviction_allowed: bool,
    window: u8,
    kind: u8,
    owner: u8,
    owner_id: u64,
    name_ptr: [*]const u8,
    name_len: usize,
    flags: u64,
    base: u64,
    len: u64,
};
extern fn r4os_rust_vm_reserve_at(request: *const RawReserve, output: *u32) u32;
comptime {
    if (@sizeOf(OptionalSlot) != 4 or @sizeOf(RawReserve) != 56 or
        @sizeOf(Range) != 216 or @offsetOf(Range, "commit_span_head") != 202 or
        @offsetOf(Range, "page_state_span_head") != 206)
        @compileError("Rust VM range C layout mismatch");
}

var pager_activity: sync.WaitQueue = sync.WaitQueue.init();
var pager_sequence: u64 = 0;

// Rust owns these indices; the legacy pager reads their resident arrays
// under the VM owner until its migration.
const IndexStats = extern struct {
    range_index_capacity: u32,
    range_index_entries: u32,
    range_index_tombstones: u32,
    range_index_lookups: u64,
    range_index_hits: u64,
    range_index_misses: u64,
    range_index_probe_total: u64,
    range_index_probe_max: u32,
    range_index_probe_last: u32,
    range_index_rebuilds: u64,
    range_index_insert_failures: u64,
    range_free_slot_lookups: u64,
    range_free_slot_probe_total: u64,
    range_free_slot_probe_max: u32,
    range_free_slot_probe_last: u32,
    range_address_entries: u32,
    range_address_lookups: u64,
    range_address_probe_total: u64,
    range_address_probe_max: u32,
    range_address_probe_last: u32,
};
extern fn r4os_rust_vm_index_init(order: **const [MAX_RANGES]u16, positions: **const [MAX_RANGES]u16) void;
extern fn r4os_rust_vm_index_query(operation: u8, argument: u64) usize;
extern fn r4os_rust_vm_index_change(operation: u8, id: u32, slot: usize) bool;
extern fn r4os_rust_vm_index_stats(output: *IndexStats) void;

comptime {
    if (@sizeOf(IndexStats) != 128)
        @compileError("Rust VM index C layout mismatch");
}
fn indexSlot(operation: u8, argument: u64) ?usize {
    const slot = r4os_rust_vm_index_query(operation, argument);
    return if (slot == std.math.maxInt(usize)) null else slot;
}
fn rangeAddressCount() usize {
    return r4os_rust_vm_index_query(5, 0);
}
fn rangeIndexEntries() usize {
    return r4os_rust_vm_index_query(6, 0);
}

var ranges: *[MAX_RANGES]Range = undefined;
var range_address_order: *const [MAX_RANGES]u16 = undefined;
var range_address_position: *const [MAX_RANGES]u16 = undefined;
var commit_spans: *const [MAX_COMMIT_SPANS]CommitSpan = undefined;
var page_state_spans: [MAX_PAGE_STATE_SPANS]PageStateSpan = .{PageStateSpan{}} ** MAX_PAGE_STATE_SPANS;
var page_state_summary: PageStateSummary = .{};
var page_state_span_free_head: ?u16 = null;
var page_state_span_active_count: u32 = 0;
var reclaim_cursor_range_slot: ?u16 = null;
var reclaim_cursor_page: u64 = 0;
var hot_path_stats: HotPathStats = .{};

pub fn init() bool {
    ranges = r4os_rust_vm_ranges_init();
    var i: usize = 0;
    r4os_rust_vm_index_init(&range_address_order, &range_address_position);
    commit_spans = r4os_rust_vm_commit_spans_init();
    page_state_span_free_head = null;
    i = 0;
    while (i < page_state_spans.len) : (i += 1) {
        page_state_spans[i] = .{ .next = page_state_span_free_head };
        page_state_span_free_head = @intCast(i);
    }
    page_state_summary = .{};
    page_state_span_active_count = 0;
    reclaim_cursor_range_slot = null;
    reclaim_cursor_page = 0;
    hot_path_stats = .{};
    r4os_rust_vm_publish_ready();
    reclaim.registerVmReclaimer(reclaimEvictFrames);
    return true;
}

pub fn reserve(req: ReserveRequest) Error!u32 {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    if (!r4os_rust_vm_initialized()) return Error.NotInitialized;
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
        .eviction_allowed = req.eviction_allowed,
    });
}

pub fn reserveAt(req: ReserveAtRequest) Error!u32 {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    if (!r4os_rust_vm_initialized()) return Error.NotInitialized;
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
        .eviction_allowed = req.eviction_allowed,
    });
}

pub fn commit(id: u32, offset: u64, len_raw: u64) Error!void {
    try policyCheck(r4os_rust_vm_commit(id, offset, len_raw));
}

/// Retry only a failed eager commit, never an unrelated partial uncommit.
/// A failed retry leaves the range and its outstanding frames owned by ID.
pub fn finishResidentRollback(id: u32) Error!void {
    try policyCheck(r4os_rust_vm_finish_rollback(id));
}

// Bounded physical preparation for callers which must survive loss of their
// backing disk. The VM owner covers metadata, mapping and zeroing, never I/O
// or reclaim. Pinning is part of the same operation, so no page can be evicted
// between preparation and the caller's destructive operation.
// On rollback failure the normal partial-uncommit state retains mappings and
// frames until a later release obtains the required TLB acknowledgements.
pub fn commitResident(id: u32, offset: u64, len_raw: u64) Error!void {
    try policyCheck(r4os_rust_vm_commit_resident(id, offset, len_raw));
}

// A PMM/MemoryBlock discrepancy must never make a frame owned by another
// block allocatable. Return only subranges which the block tree still marks
// canonical-free; inconsistent pages stay conservatively used.

fn allocClaimedFrame(range: Range, reclaim_reason: reclaim.Reason) Error!u64 {
    var extent: phys.FrameExtent = undefined;
    try policyCheck(r4os_rust_vm_allocate(&range, @intFromEnum(reclaim_reason), 1, true, &extent));
    return extent.base;
}

fn releaseClaimedFrame(frame: u64) void {
    r4os_rust_vm_release_prepared(frame, 1, true);
}

pub fn handleDemandFault(addr: u64, error_code: u64, owner: blocks.Owner, owner_id: u64) bool {
    var waited_region: ?u32 = null;
    while (true) {
        const sequence = pager_activity.readSequence(&pager_sequence);
        const owner_irq_flags = owner_locks.virtual_memory.acquire();
        var owner_locked = true;
        defer if (owner_locked) owner_locks.virtual_memory.release(owner_irq_flags);
        if (!r4os_rust_vm_initialized()) return false;
        if ((error_code & PAGE_FAULT_PRESENT) != 0) return false;
        const fault_page = alignDownValue(addr, paging.PAGE_SIZE);
        const range = rangeByAddress(fault_page) orelse return false;
        if (range.window != .r4x_vm or range.owner != owner or range.owner_id != owner_id) return false;
        if (range.kind != .virtual_range or range.partial_uncommit_len != 0) return false;
        if (waited_region) |id| if (range.id != id) return false;
        if (range.guard_len != 0 and rangesOverlap(fault_page, paging.PAGE_SIZE, range.guard_base, range.guard_len)) {
            recordDemandFaultFailure(range);
            return false;
        }
        if (!commitSpanCoversForRange(range, fault_page, paging.PAGE_SIZE)) {
            recordDemandFaultFailure(range);
            return false;
        }
        const page_index = (fault_page - range.base) / paging.PAGE_SIZE;
        if (pageStateForFaultInRange(range, page_index)) |fault_state| {
            if ((fault_state.flags & page_state_flag_busy) != 0 and range.pager_io_count != 0 and !paging.isMapped(fault_page)) {
                if (percpu.currentIndex() != 0) return false;
                waited_region = range.id;
                owner_locked = false;
                owner_locks.virtual_memory.release(owner_irq_flags);
                var wait = PagerWait{ .sequence = sequence };
                if (pager_activity.waitUnless(sync.WAIT_FOREVER, "vm-page-in", pagerUnchanged, &wait) != .signaled) return false;
                continue;
            }
            if (paging.isMapped(fault_page)) return waited_region != null;
            if (!canCommitPages(range.*, 1)) {
                recordDemandFaultFailure(range);
                return false;
            }
            if ((fault_state.flags & page_state_flag_slot_bound) != 0) {
                if (percpu.currentIndex() != 0) {
                    recordDemandFaultFailure(range);
                    return false;
                }
                owner_locked = false;
                return handlePageInFault(range, fault_page, page_index, owner_irq_flags);
            }
        }
        if (paging.isMapped(fault_page)) return waited_region != null;
        if (!canCommitPages(range.*, 1)) {
            recordDemandFaultFailure(range);
            return false;
        }

        const frame = allocClaimedFrame(range.*, .vm_fault) catch {
            recordDemandFaultFailure(range);
            return false;
        };
        const mem: [*]u8 = @ptrFromInt(phys.physToVirt(frame));
        @memset(mem[0..@intCast(paging.PAGE_SIZE)], 0);
        if (!paging.mapPage(fault_page, frame, range.flags)) {
            releaseClaimedFrame(frame);
            recordDemandFaultFailure(range);
            return false;
        }

        recordDemandFaultSuccess(range);
        pageStateSet(range.id, page_index, 1, page_state_flag_resident, page_state_flag_dirty, null, true) catch {};
        return true;
    }
}

pub fn uncommit(id: u32, offset: u64, len_raw: u64) Error!void {
    try policyCheck(r4os_rust_vm_uncommit(id, offset, len_raw));
}

pub fn release(id: u32) Error!void {
    try policyCheck(r4os_rust_vm_release(id));
}

pub fn releaseOwner(owner: blocks.Owner, owner_id: u64, kind: ?blocks.Kind) Error!u64 {
    var released: u64 = 0;
    while (true) {
        const owner_irq_flags = owner_locks.virtual_memory.acquire();
        if (!r4os_rust_vm_initialized()) {
            owner_locks.virtual_memory.release(owner_irq_flags);
            return Error.NotInitialized;
        }
        const candidate = firstOwnedRange(owner, owner_id, kind);
        owner_locks.virtual_memory.release(owner_irq_flags);
        const id = candidate orelse break;
        release(id) catch |err| {
            if (err == Error.NotFound) continue;
            return err;
        };
        released += 1;
    }
    return released;
}

pub fn protectGuard(id: u32, offset: u64, len_raw: u64) Error!void {
    try policyCheck(r4os_rust_vm_protect_guard(id, offset, len_raw, false));
}

pub fn clearGuard(id: u32) Error!void {
    try policyCheck(r4os_rust_vm_protect_guard(id, 0, 0, true));
}

pub fn rangeInfo(id: u32) ?RangeInfo {
    var range: Range = undefined;
    return if (r4os_rust_vm_range_info(id, &range)) infoFromRange(range) else null;
}

pub fn pageStateProbe(input: PageStateInput) PageStateResult {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
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
    if (range.pager_io_count != 0 and input.operation != page_state_operation_query) {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
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
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
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
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    var summary = page_state_summary;
    summary.span_count = activePageStateSpanCount();
    return summary;
}

pub fn recordPagerPolicyFailure(input: PageStateInput, page_out: bool) void {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
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

pub const WindowUsage = extern struct { active_ranges: u32 = 0, committed_bytes: u64 = 0, reserved_bytes: u64 = 0 };

pub fn windowUsage(window: Window) WindowUsage {
    var result: WindowUsage = undefined;
    r4os_rust_vm_window_usage(@intFromEnum(window), &result);
    return result;
}

pub fn stats() Stats {
    var result: Stats = undefined;
    r4os_rust_vm_stats(&result);
    return result;
}

pub fn ownerStats(owner: blocks.Owner, owner_id: u64, window_filter: ?Window, kind_filter: ?blocks.Kind) OwnerStats {
    var result: OwnerStats = undefined;
    r4os_rust_vm_owner_stats(@intFromEnum(owner), owner_id, if (window_filter) |window| @intFromEnum(window) else std.math.maxInt(u8), if (kind_filter) |kind| @intFromEnum(kind) else std.math.maxInt(u8), &result);
    return result;
}

pub fn dumpStats() void {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
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
    const request: RawReserve = .{ .eviction_allowed = req.eviction_allowed, .window = @intFromEnum(req.window), .kind = @intFromEnum(req.kind), .owner = @intFromEnum(req.owner), .owner_id = req.owner_id, .name_ptr = req.name.ptr, .name_len = req.name.len, .flags = req.flags, .base = req.base, .len = req.len };
    var id: u32 = undefined;
    try policyCheck(r4os_rust_vm_reserve_at(&request, &id));
    return id;
}

fn firstFit(window: Window, len: u64, alignment: u64) Error!u64 {
    var base: u64 = undefined;
    try policyCheck(r4os_rust_vm_first_fit(@intFromEnum(window), len, alignment, &base));
    return base;
}

fn uncommitSpan(range: *Range, start: u64, len: u64, skip_unmapped: bool) Error!void {
    try policyCheck(r4os_rust_vm_uncommit_span(range, start, len, skip_unmapped));
}

// Marker publication and removal are tiny IRQ-atomic state transitions. A
// timer hard-kill may happen between pages, but can never observe a torn
// base/length/accounted tuple.
fn publishPartialUncommit(range: *Range, base: u64, len: u64, reset_accounted: bool) void {
    r4os_rust_vm_partial(range, 0, base, len, reset_accounted);
}

fn advancePartialUncommitCursor(range: *Range, cursor: u64) void {
    r4os_rust_vm_partial(range, 1, cursor, 0, false);
}

fn clearPartialUncommit(range: *Range) void {
    r4os_rust_vm_partial(range, 2, 0, 0, false);
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
    var committed = range.commit_span_head.get();
    while (virt < end) {
        // Commit metadata is ordered and remains intact until this request
        // completes. Resume through its spans, jumping over reserved holes.
        while (committed) |slot| {
            const span = &commit_spans[slot];
            if (span.base + span.len > virt) break;
            committed = span.next.get();
        }
        const slot = committed orelse break;
        const span = &commit_spans[slot];
        virt = @max(virt, span.base);
        if (virt >= end) break;
        const span_end = @min(end, span.base + span.len);
        const frame = paging.mappedFrame(virt) orelse {
            virt += paging.PAGE_SIZE;
            advancePartialUncommitCursor(range, virt);
            continue;
        };
        // Eager resident buffers commonly contain contiguous extents. Use
        // the same bounded, acknowledged unmap as other VM, retaining both
        // PTEs and frames if invalidation fails. Sparse holes remain valid.
        const max_pages = page_batch.boundedPageCount((span_end - virt) / paging.PAGE_SIZE);
        const claimed_pages = blocks.claimedPhysicalPrefix(frame, max_pages * paging.PAGE_SIZE) / paging.PAGE_SIZE;
        if (claimed_pages == 0) return Error.NotCommitted;
        var run_pages: u64 = 1;
        while (run_pages < claimed_pages) : (run_pages += 1) {
            const next = paging.mappedFrame(virt + run_pages * paging.PAGE_SIZE) orelse break;
            if (next != frame + run_pages * paging.PAGE_SIZE) break;
        }
        try uncommitExtent(range, virt, .{ .base = frame, .count = run_pages }, false);
        virt += run_pages * paging.PAGE_SIZE;
        advancePartialUncommitCursor(range, virt);
    }

    try removeCommitSpanReserved(range.id, start, len, &commit_split);
    try removePageStateRangeReserved(range, first_page, end_page, &page_splits);
    const irq_flags = owner_locks.virtual_memory.acquire();
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
    owner_locks.virtual_memory.release(irq_flags);
}

fn uncommitPage(range: *Range, virt: u64, count_logical_commit: bool) Error!void {
    try policyCheck(r4os_rust_vm_uncommit_extent(range, virt, 0, 1, count_logical_commit, true));
}

fn uncommitExtent(range: *Range, virt: u64, extent: phys.FrameExtent, count_logical_commit: bool) Error!void {
    try policyCheck(r4os_rust_vm_uncommit_extent(range, virt, extent.base, extent.count, count_logical_commit, false));
}

fn validateInside(range: Range, offset: u64, len: u64) Error!void {
    try policyCheck(r4os_rust_vm_inside(range.len, offset, len));
}

fn validateNotGuard(range: Range, base: u64, len: u64) Error!void {
    try policyCheck(r4os_rust_vm_guard(base, len, range.guard_base, range.guard_len));
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

fn canCommitPages(range: Range, pages: u64) bool {
    return r4os_rust_vm_can_commit(&range, pages);
}

fn addCommitSpan(range_id: u32, base: u64, len: u64) Error!void {
    try policyCheck(r4os_rust_vm_commit_span_change(0, range_id, base, len, null));
}

fn removeCommitSpan(range_id: u32, base: u64, len: u64) Error!void {
    var reserved = try reserveCommitSplitSlot(range_id, base, len);
    defer if (reserved) |slot| releaseCommitSpanSlot(slot);
    try removeCommitSpanReserved(range_id, base, len, &reserved);
}

fn reserveCommitSplitSlot(range_id: u32, base: u64, len: u64) Error!?u16 {
    var reserved: OptionalSlot = .{};
    try policyCheck(r4os_rust_vm_commit_span_change(3, range_id, base, len, &reserved));
    return reserved.get();
}

fn removeCommitSpanReserved(range_id: u32, base: u64, len: u64, reserved: *?u16) Error!void {
    var raw = OptionalSlot.from(reserved.*);
    const code = r4os_rust_vm_commit_span_change(1, range_id, base, len, &raw);
    reserved.* = raw.get();
    try policyCheck(code);
}

fn removeCommitSpansForRange(range_id: u32) void {
    _ = r4os_rust_vm_commit_span_change(2, range_id, 0, 0, null);
}

fn commitSpanOverlaps(range_id: u32, base: u64, len: u64) bool {
    const range_index = indexById(range_id) orelse return false;
    return commitSpanOverlapsForRange(&ranges[range_index], base, len);
}

fn commitSpanOverlapsForRange(range: *const Range, base: u64, len: u64) bool {
    return r4os_rust_vm_commit_span_scan(range.commit_span_head, base, len, false);
}

fn commitSpanCovers(range_id: u32, base: u64, len: u64) bool {
    const range_index = indexById(range_id) orelse return false;
    return commitSpanCoversForRange(&ranges[range_index], base, len);
}

fn commitSpanCoversForRange(range: *const Range, base: u64, len: u64) bool {
    return r4os_rust_vm_commit_span_scan(range.commit_span_head, base, len, true);
}

fn releaseCommitSpanSlot(slot: u16) void {
    _ = r4os_rust_vm_commit_span_change(4, 0, slot, 0, null);
}

fn rangeByAddress(addr: u64) ?*Range {
    return &ranges[indexSlot(4, addr) orelse return null];
}

fn pageStateForFault(range_id: u32, page_index: u64) ?FaultPageState {
    const range_index = indexById(range_id) orelse return null;
    return pageStateForFaultInRange(&ranges[range_index], page_index);
}

fn pageStateForFaultInRange(range: *const Range, page_index: u64) ?FaultPageState {
    hot_path_stats.page_state_span_lookups +%= 1;
    var current = range.page_state_span_head.get();
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

// A writeback keeps the region and source mappings alive across filesystem
// waits. Writers may continue; a new dirty bit prevents final eviction.
pub const PageWriteback = struct {
    region_id: u32,
    region_offset: u64,
    page_count: u64,
    unwind: task_context.UnwindToken,
    active: bool = true,
    completed: bool = false,
};

pub fn beginPageWriteback(region_id: u32, region_offset: u64, page_count: u64, page_ptr: [*]const u8) Error!PageWriteback {
    const token = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(token);
    if (percpu.currentIndex() != 0) return Error.Busy;
    const idx = indexById(region_id) orelse return Error.NotFound;
    const range = &ranges[idx];
    const bytes = checkedMul(page_count, paging.PAGE_SIZE) orelse return Error.Overflow;
    if (page_count == 0 or !isAligned(region_offset, paging.PAGE_SIZE)) return Error.BadAlignment;
    try validateInside(range.*, region_offset, bytes);
    if (range.window != .r4x_vm or range.partial_uncommit_len != 0 or range.pager_io_count != 0) return Error.Busy;
    const start = range.base + region_offset;
    if (@intFromPtr(page_ptr) != start) return Error.OutsideWindow;
    if (!commitSpanCoversForRange(range, start, bytes)) return Error.NotCommitted;
    const first = region_offset / paging.PAGE_SIZE;
    for (0..@intCast(page_count)) |i| {
        const state = pageStateForFaultInRange(range, first + i) orelse return Error.NotCommitted;
        if ((state.flags & (page_state_flag_busy | page_state_flag_pinned)) != 0) return Error.Busy;
        if ((state.flags & page_state_flag_resident) == 0 or !paging.isMapped(start + i * paging.PAGE_SIZE)) return Error.NotCommitted;
    }
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) return Error.Busy;
    errdefer _ = task_context.leaveUnwind(unwind);
    try pageStateSet(region_id, first, page_count, page_state_flag_busy, page_state_flag_dirty, null, true);
    errdefer pageStateSet(region_id, first, page_count, page_state_flag_dirty, page_state_flag_busy, null, true) catch {};
    for (0..@intCast(page_count)) |i| {
        // The acknowledged invalidation starts a new hardware dirty epoch.
        // If it fails, the original mapping and data remain owned.
        if (!paging.clearDirty(start + i * paging.PAGE_SIZE)) return Error.MapFailed;
    }
    range.pager_io_count += 1;
    return .{ .region_id = region_id, .region_offset = region_offset, .page_count = page_count, .unwind = unwind };
}

pub fn endPageWriteback(writeback: *PageWriteback) void {
    if (!writeback.active) return;
    const token = owner_locks.virtual_memory.acquire();
    if (indexById(writeback.region_id)) |idx| {
        const range = &ranges[idx];
        pageStateSet(range.id, writeback.region_offset / paging.PAGE_SIZE, writeback.page_count, if (writeback.completed) 0 else page_state_flag_dirty, page_state_flag_busy, null, true) catch {};
        std.debug.assert(range.pager_io_count != 0);
        range.pager_io_count -= 1;
    }
    writeback.active = false;
    owner_locks.virtual_memory.release(token);
    _ = task_context.leaveUnwind(writeback.unwind);
    _ = pager_activity.bumpSequenceAndWakeAll(&pager_sequence);
}

pub fn completePageWriteback(writeback: *PageWriteback, binding: PageStateInput) Error!u64 {
    if (!writeback.active or binding.region_id != writeback.region_id or
        binding.region_offset != writeback.region_offset or binding.page_count != writeback.page_count) return Error.NotFound;
    const token = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(token);
    const idx = indexById(binding.region_id) orelse return Error.NotFound;
    const range = &ranges[idx];
    const first = binding.region_offset / paging.PAGE_SIZE;
    try pageStateSet(range.id, first, binding.page_count, page_state_flag_slot_bound, 0, .{
        .reservation_id = binding.slot_reservation_id,
        .slot_index = binding.slot_index,
        .slot_generation = binding.slot_generation,
    }, true);
    page_state_summary.slot_binds +%= binding.page_count;
    syncHardwareDirty(range.*, first, binding.page_count);
    for (0..@intCast(binding.page_count)) |i| {
        const state = pageStateForFaultInRange(range, first + i) orelse return Error.NotCommitted;
        if ((state.flags & page_state_flag_dirty) != 0) {
            // The file may contain an earlier or mixed snapshot. Keep the
            // complete resident span dirty until a later quiet writeback.
            try pageStateSet(range.id, first, binding.page_count, page_state_flag_dirty, 0, null, true);
            page_state_summary.eviction_redirtied_pages +%= binding.page_count;
            writeback.completed = true;
            return 0;
        }
    }
    const returned = try makePageRangeNonresident(range.id, binding.region_offset, binding.page_count);
    page_state_summary.page_out_nonresident_pages +%= returned;
    writeback.completed = true;
    return returned;
}

pub fn commitCovers(region_id: u32, offset: u64, page_count: u64) bool {
    const token = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(token);
    const idx = indexById(region_id) orelse return false;
    const range = &ranges[idx];
    const bytes = checkedMul(page_count, paging.PAGE_SIZE) orelse return false;
    if (page_count == 0 or !isAligned(offset, paging.PAGE_SIZE) or range.partial_uncommit_len != 0) return false;
    validateInside(range.*, offset, bytes) catch return false;
    return commitSpanCoversForRange(range, range.base + offset, bytes);
}

const PagerWait = struct { sequence: u64 };

fn pagerUnchanged(raw: *anyopaque) bool {
    const wait: *PagerWait = @ptrCast(@alignCast(raw));
    return pager_sequence == wait.sequence;
}

fn handlePageInFault(range: *Range, fault_page: u64, page_index: u64, acquired: owner_locks.Token) bool {
    // The demand-fault owner is handed over without an unlock gap.
    var token = acquired;
    defer owner_locks.virtual_memory.release(token);
    if (paging.isMapped(fault_page)) return true;
    if (!commitSpanCoversForRange(range, fault_page, paging.PAGE_SIZE) or range.partial_uncommit_len != 0) return false;
    const fault_state = pageStateForFaultInRange(range, page_index) orelse return false;
    if (fault_state.slot_reservation_id == 0 or
        (fault_state.flags & (page_state_flag_busy | page_state_flag_pinned | page_state_flag_error)) != 0 or
        range.owner_id > std.math.maxInt(u32))
    {
        return recordPageInFaultFailure(range, page_index, false);
    }

    const backing = backing_store.activeBackingResult() orelse return recordPageInFaultFailure(range, page_index, true);
    const active_path = backing_store.activeBackingPath() orelse return recordPageInFaultFailure(range, page_index, true);
    var path: [backing_store.MAX_BACKING_PATH:0]u8 = .{0} ** backing_store.MAX_BACKING_PATH;
    const path_len = std.mem.len(active_path);
    if (path_len >= path.len) return recordPageInFaultFailure(range, page_index, true);
    @memcpy(path[0..path_len], active_path[0..path_len]);
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) return recordPageInFaultFailure(range, page_index, false);
    defer _ = task_context.leaveUnwind(unwind);
    pageStateSet(range.id, page_index, 1, page_state_flag_busy, 0, null, true) catch {
        return recordPageInFaultFailure(range, page_index, true);
    };
    range.pager_io_count += 1;
    defer {
        pageStateSet(range.id, page_index, 1, 0, page_state_flag_busy, null, true) catch {};
        range.pager_io_count -= 1;
        // Publish before waking and never enter a wait while holding VM.
        owner_locks.virtual_memory.release(token);
        _ = pager_activity.bumpSequenceAndWakeAll(&pager_sequence);
        token = owner_locks.virtual_memory.acquire();
    }

    const page_io_input = backing_store.PageIoInput{
        .operation = backing_store.page_io_operation_page_in,
        .region_id = range.id,
        .region_offset = fault_page - range.base,
        .reservation_id = fault_state.slot_reservation_id,
        .slot_index = fault_state.slot_index,
        .page_count = 1,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = @intCast(range.owner_id),
        .expected_generation = fault_state.slot_generation,
        .vm_region_exists = true,
        .vm_region_is_r4x = true,
        .commit_covered = true,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .backing = backing,
    };
    const prepared = backing_store.pageIoPrepare(page_io_input);
    if (prepared.status != backing_store.page_io_status_ready) return recordPageInFaultFailure(range, page_index, true);

    // Keep the new physical page private in the permanent RAM direct map.
    // No guest PTE exists until I/O and its reservation checks are complete.
    const claim_owner = range.*;
    owner_locks.virtual_memory.release(token);
    const maybe_frame = allocClaimedFrame(claim_owner, .vm_fault);
    token = owner_locks.virtual_memory.acquire();
    const frame = maybe_frame catch return recordPageInFaultFailure(range, page_index, false);
    var published = false;
    defer if (!published) releaseClaimedFrame(frame);
    const mem: [*]u8 = @ptrFromInt(phys.physToVirt(frame));
    owner_locks.virtual_memory.release(token);
    const io_status = r4sys_api.fileReadAt64(&path, prepared.backing_offset, mem, @intCast(prepared.transfer_bytes));
    token = owner_locks.virtual_memory.acquire();

    var complete_input = page_io_input;
    complete_input.io_status = io_status;
    complete_input.io_bytes = if (io_status > 0) @intCast(io_status) else 0;
    const completed = backing_store.pageIoComplete(complete_input);
    if (completed.status != backing_store.page_io_status_page_in_ok) return recordPageInFaultFailure(range, page_index, true);
    if (!paging.mapPage(fault_page, frame, range.flags)) return recordPageInFaultFailure(range, page_index, false);
    published = true;
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

pub fn reclaimEvictFrames(reason: reclaim.Reason, requested_frames_raw: u32, vm_owner_id: ?u64) reclaim.SourceResult {
    _ = reason;
    const requested_frames = if (requested_frames_raw == 0) 1 else requested_frames_raw;
    var result = reclaim.SourceResult{};
    if (!r4os_rust_vm_initialized()) return result;
    // The pager transaction intentionally spans filesystem/block waits and
    // therefore cannot hold the VM owner.
    // Normal VM and lifecycle work remains BSP-owned; AP callers fail closed.
    if (percpu.currentIndex() != 0) {
        result.failures = 1;
        return result;
    }

    page_state_summary.eviction_attempts +%= 1;
    const selected_backing = blk: {
        const token = owner_locks.virtual_memory.acquire();
        defer owner_locks.virtual_memory.release(token);
        break :blk backing_store.activeBackingResult();
    };
    const backing = selected_backing orelse {
        page_state_summary.eviction_no_backing +%= 1;
        page_state_summary.eviction_failures +%= 1;
        page_state_summary.pager_disabled_eviction_gates +%= 1;
        result.failures = 1;
        return result;
    };
    const path: [*:0]const u8 = &backing.path;

    while (result.returned_frames < requested_frames) {
        const step = evictOneResidentPage(backing, path, vm_owner_id);
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
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    if (!r4os_rust_vm_initialized() or backing_store.activeBackingResult() == null) return 0;
    var bytes: u64 = 0;
    var range_pos: usize = 0;
    while (range_pos < rangeAddressCount()) : (range_pos += 1) {
        const range = ranges[range_address_order[range_pos]];
        var current = range.page_state_span_head.get();
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
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    if (!r4os_rust_vm_initialized() or backing_store.activeBackingResult() == null) return 0;
    var bytes: u64 = 0;
    var range_pos: usize = 0;
    while (range_pos < rangeAddressCount()) : (range_pos += 1) {
        const range = ranges[range_address_order[range_pos]];
        var current = range.page_state_span_head.get();
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

fn evictOneResidentPage(backing: backing_store.Result, path: [*:0]const u8, vm_owner_id: ?u64) EvictionStep {
    const token = owner_locks.virtual_memory.acquire();
    var locked = true;
    defer if (locked) owner_locks.virtual_memory.release(token);
    if (rangeAddressCount() == 0) return .{ .outcome = .skipped };
    var start_pos: usize = 0;
    var start_page: u64 = 0;
    if (reclaim_cursor_range_slot) |cursor_slot| {
        const raw_position = range_address_position[cursor_slot];
        if (raw_position != INVALID_RANGE_POSITION and @as(usize, raw_position) < rangeAddressCount()) {
            start_pos = raw_position;
            start_page = reclaim_cursor_page;
        }
    }

    var visited: usize = 0;
    var range_pos = start_pos;
    while (visited < rangeAddressCount()) : (visited += 1) {
        const range_slot = range_address_order[range_pos];
        var range = &ranges[range_slot];
        hot_path_stats.reclaim_cursor_range_steps +%= 1;
        const page_floor = if (visited == 0) start_page else 0;

        if (range.active() and range.pager_io_count == 0 and range.partial_uncommit_len == 0 and range.window == .r4x_vm and range.owner == .r4x_instance and
            range.owner_id != 0 and range.owner_id <= std.math.maxInt(u32) and
            (vm_owner_id == null or range.owner_id == vm_owner_id.?))
        {
            var span_cursor = range.page_state_span_head.get();
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
                    const id = range.id;
                    locked = false;
                    owner_locks.virtual_memory.release(token);
                    return evictResidentPage(id, page_index, backing, path);
                }
            }
        }

        range_pos += 1;
        if (range_pos == rangeAddressCount()) {
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
    if (next_pos == rangeAddressCount()) {
        next_pos = 0;
        hot_path_stats.reclaim_cursor_wraps +%= 1;
    }
    reclaim_cursor_range_slot = range_address_order[next_pos];
    reclaim_cursor_page = 0;
}

fn evictResidentPage(region_id: u32, page_index: u64, backing: backing_store.Result, path: [*:0]const u8) EvictionStep {
    var token = owner_locks.virtual_memory.acquire();
    var locked = true;
    defer if (locked) owner_locks.virtual_memory.release(token);
    const idx = indexById(region_id) orelse return .{ .outcome = .skipped };
    const range = &ranges[idx];
    if (range.pager_io_count != 0 or range.partial_uncommit_len != 0) return .{ .outcome = .skipped };
    const state = pageStateForFaultInRange(range, page_index) orelse return .{ .outcome = .skipped };
    if ((state.flags & (page_state_flag_busy | page_state_flag_pinned | page_state_flag_error)) != 0) return .{ .outcome = .skipped };
    const virt = range.base + page_index * paging.PAGE_SIZE;
    const dirty = (state.flags & page_state_flag_dirty) != 0 or paging.pageDirty(virt);
    if ((state.flags & page_state_flag_slot_bound) != 0 and !dirty) return evictCleanSlotBoundPage(range, page_index, state, backing);

    const page_ptr: [*]u8 = @ptrFromInt(virt);
    var writeback = beginPageWriteback(region_id, page_index * paging.PAGE_SIZE, 1, page_ptr) catch return .{ .outcome = .failed, .dirty = dirty };
    // This defer runs after the final VM owner release below.
    owner_locks.virtual_memory.release(token);
    locked = false;
    defer endPageWriteback(&writeback);
    token = owner_locks.virtual_memory.acquire();
    locked = true;
    defer {
        owner_locks.virtual_memory.release(token);
        locked = false;
    }

    var owned_path: [backing_store.MAX_BACKING_PATH:0]u8 = .{0} ** backing_store.MAX_BACKING_PATH;
    const path_len = std.mem.len(path);
    if (path_len >= owned_path.len) return .{ .outcome = .failed, .dirty = dirty };
    @memcpy(owned_path[0..path_len], path[0..path_len]);
    const owner_id: u32 = @intCast(range.owner_id);
    var reservation_id = state.slot_reservation_id;
    var slot_index = state.slot_index;
    var expected_generation = state.slot_generation;
    var reserved_new_slot = false;
    if ((state.flags & page_state_flag_slot_bound) == 0) {
        const reserved = backing_store.slotProbe(.{
            .operation = backing_store.slot_operation_reserve,
            .requested_slots = 1,
            .owner_kind = backing_store.slot_owner_kind_vm_region,
            .owner_id = owner_id,
            .region_id = region_id,
            .backing = backing,
        });
        if (reserved.status != backing_store.slot_status_reserved) {
            page_state_summary.eviction_slot_failures +%= 1;
            recordPagerPageOutFailure(dirty);
            return .{ .outcome = .failed, .dirty = dirty };
        }
        reservation_id = reserved.reservation_id;
        slot_index = 0;
        expected_generation = reserved.generation;
        reserved_new_slot = true;
    }
    const input = backing_store.PageIoInput{
        .operation = backing_store.page_io_operation_page_out,
        .region_id = region_id,
        .region_offset = writeback.region_offset,
        .reservation_id = reservation_id,
        .slot_index = slot_index,
        .page_count = 1,
        .owner_kind = backing_store.slot_owner_kind_vm_region,
        .owner_id = owner_id,
        .expected_generation = expected_generation,
        .flags = backing_store.page_io_flag_eviction_request,
        .vm_region_exists = true,
        .vm_region_is_r4x = true,
        .commit_covered = true,
        .committed_bytes = range.committed_bytes,
        .resident_bytes = range.resident_bytes,
        .backing = backing,
    };
    const prepared = backing_store.pageIoPrepare(input);
    if (prepared.status != backing_store.page_io_status_ready) {
        if (reserved_new_slot) releaseEvictionSlot(backing, reservation_id, owner_id, region_id);
        page_state_summary.eviction_slot_failures +%= 1;
        recordPagerPageOutFailure(dirty);
        return .{ .outcome = .failed, .dirty = dirty };
    }
    owner_locks.virtual_memory.release(token);
    const io_status = r4sys_api.fileWriteAt(&owned_path, prepared.backing_offset, page_ptr, @intCast(prepared.transfer_bytes));
    token = owner_locks.virtual_memory.acquire();
    var complete_input = input;
    complete_input.io_status = io_status;
    complete_input.io_bytes = if (io_status > 0) @intCast(io_status) else 0;
    const completed = backing_store.pageIoComplete(complete_input);
    if (completed.status != backing_store.page_io_status_page_out_ok) {
        if (reserved_new_slot) releaseEvictionSlot(backing, reservation_id, owner_id, region_id);
        markEvictionError(region_id, page_index);
        page_state_summary.eviction_io_failures +%= 1;
        recordPagerPageOutFailure(dirty);
        return .{ .outcome = .failed, .dirty = dirty };
    }
    const returned = completePageWriteback(&writeback, .{
        .region_id = region_id,
        .region_offset = completed.region_offset,
        .page_count = 1,
        .slot_reservation_id = completed.reservation_id,
        .slot_index = completed.slot_index,
        .slot_generation = completed.slot_generation,
    }) catch {
        markEvictionError(region_id, page_index);
        recordPagerPageOutFailure(true);
        return .{ .outcome = .failed, .dirty = dirty };
    };
    page_state_summary.eviction_page_outs +%= 1;
    return .{ .outcome = if (returned == 0) .skipped else .returned, .returned_frames = @intCast(returned), .returned_bytes = returned * paging.PAGE_SIZE, .page_outs = 1, .dirty = dirty };
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
        .commit_covered = commitSpanCoversForRange(range, range.base + page_index * paging.PAGE_SIZE, paging.PAGE_SIZE),
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
    if (!r4os_rust_vm_initialized()) {
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
            if (!range.eviction_allowed) return Error.Busy;
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
    var current = range.page_state_span_head.get();
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
        range.page_state_span_head = OptionalSlot.from(slot);
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
    var current = range.page_state_span_head.get();
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
    var current = range.page_state_span_head.get();
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
    var current = range.page_state_span_head.get();
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
    var current = range.page_state_span_head.get();
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
    var current = range.page_state_span_head.get();
    while (current) |slot| {
        const next = page_state_spans[slot].next;
        removed_pages +%= page_state_spans[slot].page_count;
        releasePageStateSpanSlot(slot);
        current = next;
    }
    range.page_state_span_head = OptionalSlot.from(null);
    page_state_summary.cleanup_pages +%= removed_pages;
}

fn coalescePageStateSpansForRange(range: *Range) Error!void {
    var current = range.page_state_span_head.get();
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
    var current = range.page_state_span_head.get();
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
        range.page_state_span_head = OptionalSlot.from(next);
    }
    if (next) |next_slot| page_state_spans[next_slot].prev = previous;
}

fn pageRangesOverlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) bool {
    return a_start < b_end and b_start < a_end;
}

fn alignDownValue(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn infoFromRange(range: Range) RangeInfo {
    return .{
        .id = range.id,
        .window = range.window,
        .kind = range.kind,
        .owner = range.owner,
        .owner_id = range.owner_id,
        .status = range.status,
        .name = range.name_ptr[0..range.name_len],
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

fn lowerBoundAddress(base: u64) usize {
    return r4os_rust_vm_index_query(3, base);
}

fn removeRangeAddressIndex(slot: usize) void {
    const raw_pos = range_address_position[slot];
    if (raw_pos == INVALID_RANGE_POSITION) return;
    const pos: usize = raw_pos;
    if (pos >= rangeAddressCount() or range_address_order[pos] != slot) return;

    if (reclaim_cursor_range_slot != null and reclaim_cursor_range_slot.? == slot) {
        if (rangeAddressCount() > 1) {
            const next_pos = if (pos + 1 < rangeAddressCount()) pos + 1 else 0;
            reclaim_cursor_range_slot = range_address_order[next_pos];
            reclaim_cursor_page = 0;
        } else {
            reclaim_cursor_range_slot = null;
            reclaim_cursor_page = 0;
        }
    }

    _ = r4os_rust_vm_index_change(3, 0, slot);
}

fn indexById(id: u32) ?usize {
    return indexSlot(1, id);
}

fn recordPageStateSpanSteps(steps: usize) void {
    const value: u32 = @intCast(@min(steps, std.math.maxInt(u32)));
    hot_path_stats.page_state_span_steps +%= value;
    if (value > hot_path_stats.page_state_span_step_max) hot_path_stats.page_state_span_step_max = value;
}

pub fn hotPathStats() HotPathStats {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    var out = hot_path_stats;
    var resident: ResidentStats = undefined;
    r4os_rust_vm_transaction_stats(&resident);
    inline for (@typeInfo(ResidentStats).@"struct".fields) |field|
        @field(out, field.name) = @field(resident, field.name);
    var indices: IndexStats = undefined;
    r4os_rust_vm_index_stats(&indices);
    inline for (@typeInfo(IndexStats).@"struct".fields) |field|
        @field(out, field.name) = @field(indices, field.name);
    var commits: CommitSpanStats = undefined;
    r4os_rust_vm_commit_span_stats(&commits);
    inline for (@typeInfo(CommitSpanStats).@"struct".fields) |field|
        @field(out, field.name) = @field(commits, field.name);
    out.page_state_span_active = page_state_span_active_count;
    return out;
}

pub fn metadataInvariant() bool {
    const owner_irq_flags = owner_locks.virtual_memory.acquire();
    defer owner_locks.virtual_memory.release(owner_irq_flags);
    if (!r4os_rust_vm_initialized() or rangeAddressCount() != rangeIndexEntries()) return false;
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
        if (position == INVALID_RANGE_POSITION or @as(usize, position) >= rangeAddressCount() or
            range_address_order[position] != slot_index or rawIndexById(range.id) != slot_index)
        {
            return false;
        }

        var previous_commit: ?u16 = null;
        var commit_cursor = range.commit_span_head.get();
        var commit_steps: usize = 0;
        while (commit_cursor) |span_slot| : (commit_cursor = commit_spans[span_slot].next.get()) {
            commit_steps += 1;
            if (commit_steps > MAX_COMMIT_SPANS or !markMetadataSeen(commit_seen[0..], span_slot)) return false;
            const span = commit_spans[span_slot];
            if (!span.slot_used or span.range_id != range.id or span.prev.get() != previous_commit or span.len == 0) return false;
            if (previous_commit) |previous_slot| {
                const previous_end = checkedEnd(commit_spans[previous_slot].base, commit_spans[previous_slot].len) orelse return false;
                if (previous_end >= span.base) return false;
            }
            previous_commit = span_slot;
            linked_commit_spans += 1;
        }

        var previous_page: ?u16 = null;
        var page_cursor = range.page_state_span_head.get();
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
    if (active_ranges != rangeAddressCount() or linked_commit_spans != commitSpanActiveCount() or
        linked_page_spans != page_state_span_active_count) return false;

    var position: usize = 0;
    while (position < rangeAddressCount()) : (position += 1) {
        const slot: usize = range_address_order[position];
        if (!ranges[slot].active() or range_address_position[slot] != position) return false;
        if (position != 0) {
            const previous = ranges[range_address_order[position - 1]];
            const previous_end = checkedEnd(previous.base, previous.len) orelse return false;
            if (previous_end > ranges[slot].base) return false;
        }
    }

    var free_commit_count: usize = 0;
    var free_commit = commitSpanFreeHead();
    while (free_commit) |span_slot| : (free_commit = commit_spans[span_slot].next.get()) {
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
    return indexSlot(2, id);
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

const RawWindowDef = extern struct { base: u64, len: u64, name: [*]const u8, name_len: usize };
const FitRecord = extern struct { base: u64, len: u64, window: u8 };
extern fn r4os_rust_vm_window(window: u8, output: *RawWindowDef) void;
extern fn r4os_rust_vm_normalize(value: u64, alignment: bool, output: *u64) u32;
extern fn r4os_rust_vm_inside(range_len: u64, offset: u64, len: u64) u32;
extern fn r4os_rust_vm_guard(base: u64, len: u64, guard_base: u64, guard_len: u64) u32;
extern fn r4os_rust_vm_overlap(a_base: u64, a_len: u64, b_base: u64, b_len: u64) bool;
extern fn r4os_rust_vm_first_fit(window: u8, len: u64, alignment: u64, output: *u64) u32;
fn errorCode(err: Error) u32 {
    return switch (err) {
        Error.NotInitialized => 1,
        Error.TableFull => 2,
        Error.EmptyRange => 3,
        Error.BadAlignment => 4,
        Error.Overflow => 5,
        Error.NoSpace => 6,
        Error.Overlap => 7,
        Error.NotFound => 8,
        Error.AlreadyCommitted => 9,
        Error.NotCommitted => 10,
        Error.GuardRange => 11,
        Error.OutOfMemory => 12,
        Error.MapFailed => 13,
        Error.Busy => 14,
        Error.OutsideWindow => 15,
    };
}
fn policyCheck(code: u32) Error!void {
    return switch (code) {
        0 => {},
        1 => Error.NotInitialized,
        2 => Error.TableFull,
        3 => Error.EmptyRange,
        4 => Error.BadAlignment,
        5 => Error.Overflow,
        6 => Error.NoSpace,
        7 => Error.Overlap,
        8 => Error.NotFound,
        9 => Error.AlreadyCommitted,
        10 => Error.NotCommitted,
        11 => Error.GuardRange,
        12 => Error.OutOfMemory,
        13 => Error.MapFailed,
        14 => Error.Busy,
        15 => Error.OutsideWindow,
        else => unreachable,
    };
}
// Fixed resident metadata readers for the still active legacy range owner.
// Caller retains the VM owner; no state pointer or callback leaves this cut.
export fn r4os_zig_vm_fit_start(base: u64, count: *usize) callconv(.c) usize {
    count.* = rangeAddressCount();
    return lowerBoundAddress(base);
}
export fn r4os_zig_vm_fit_record(position: usize, output: *FitRecord) callconv(.c) bool {
    if (position >= rangeAddressCount()) return false;
    const range = &ranges[range_address_order[position]];
    output.* = .{ .base = range.base, .len = range.len, .window = @intFromEnum(range.window) };
    return true;
}
comptime {
    if (@sizeOf(RawWindowDef) != 32 or @sizeOf(FitRecord) != 24)
        @compileError("Rust VM policy C layout mismatch");
}

fn windowDef(window: Window) WindowDef {
    var raw: RawWindowDef = undefined;
    r4os_rust_vm_window(@intFromEnum(window), &raw);
    return .{ .base = raw.base, .len = raw.len, .name = raw.name[0..raw.name_len] };
}

fn normalizeLen(len: u64) Error!u64 {
    var normalized: u64 = undefined;
    try policyCheck(r4os_rust_vm_normalize(len, false, &normalized));
    return normalized;
}

fn normalizeAlignment(alignment: u64) Error!u64 {
    var normalized: u64 = undefined;
    try policyCheck(r4os_rust_vm_normalize(alignment, true, &normalized));
    return normalized;
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

fn isAligned(value: u64, alignment: u64) bool {
    return (value & (alignment - 1)) == 0;
}

fn isPowerOfTwo(value: u64) bool {
    return value != 0 and (value & (value - 1)) == 0;
}

fn rangesOverlap(a_base: u64, a_len: u64, b_base: u64, b_len: u64) bool {
    return r4os_rust_vm_overlap(a_base, a_len, b_base, b_len);
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
