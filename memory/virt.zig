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


pub const WindowUsage = extern struct { active_ranges: u32 = 0, committed_bytes: u64 = 0, reserved_bytes: u64 = 0 };
pub const PageWriteback = struct {
    region_id: u32,
    region_offset: u64,
    page_count: u64,
    unwind: task_context.UnwindToken,
    active: bool = true,
    completed: bool = false,
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

};
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
const RawHotPathStats = extern struct {
    resident_commit_batches: u64,
    resident_commit_max_batch_pages: u64,
    resident_commit_unlocked_zero_bytes: u64,
    resident_rollback_batches: u64,
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
    commit_span_active: u32,
    commit_span_lookups: u64,
    commit_span_steps: u64,
    commit_span_step_max: u32,
    page_state_span_active: u32,
    page_state_span_lookups: u64,
    page_state_span_steps: u64,
    page_state_span_step_max: u32,
    reclaim_cursor_range_steps: u64,
    reclaim_cursor_span_steps: u64,
    reclaim_cursor_page_steps: u64,
    reclaim_cursor_wraps: u64,
};
fn rawHotPathStats(value: HotPathStats) RawHotPathStats {
    return .{
        .resident_commit_batches = value.resident_commit_batches,
        .resident_commit_max_batch_pages = value.resident_commit_max_batch_pages,
        .resident_commit_unlocked_zero_bytes = value.resident_commit_unlocked_zero_bytes,
        .resident_rollback_batches = value.resident_rollback_batches,
        .range_index_capacity = value.range_index_capacity,
        .range_index_entries = value.range_index_entries,
        .range_index_tombstones = value.range_index_tombstones,
        .range_index_lookups = value.range_index_lookups,
        .range_index_hits = value.range_index_hits,
        .range_index_misses = value.range_index_misses,
        .range_index_probe_total = value.range_index_probe_total,
        .range_index_probe_max = value.range_index_probe_max,
        .range_index_probe_last = value.range_index_probe_last,
        .range_index_rebuilds = value.range_index_rebuilds,
        .range_index_insert_failures = value.range_index_insert_failures,
        .range_free_slot_lookups = value.range_free_slot_lookups,
        .range_free_slot_probe_total = value.range_free_slot_probe_total,
        .range_free_slot_probe_max = value.range_free_slot_probe_max,
        .range_free_slot_probe_last = value.range_free_slot_probe_last,
        .range_address_entries = value.range_address_entries,
        .range_address_lookups = value.range_address_lookups,
        .range_address_probe_total = value.range_address_probe_total,
        .range_address_probe_max = value.range_address_probe_max,
        .range_address_probe_last = value.range_address_probe_last,
        .commit_span_active = value.commit_span_active,
        .commit_span_lookups = value.commit_span_lookups,
        .commit_span_steps = value.commit_span_steps,
        .commit_span_step_max = value.commit_span_step_max,
        .page_state_span_active = value.page_state_span_active,
        .page_state_span_lookups = value.page_state_span_lookups,
        .page_state_span_steps = value.page_state_span_steps,
        .page_state_span_step_max = value.page_state_span_step_max,
        .reclaim_cursor_range_steps = value.reclaim_cursor_range_steps,
        .reclaim_cursor_span_steps = value.reclaim_cursor_span_steps,
        .reclaim_cursor_page_steps = value.reclaim_cursor_page_steps,
        .reclaim_cursor_wraps = value.reclaim_cursor_wraps,
    };
}
fn fromRawHotPathStats(value: RawHotPathStats) HotPathStats {
    return .{
        .resident_commit_batches = value.resident_commit_batches,
        .resident_commit_max_batch_pages = value.resident_commit_max_batch_pages,
        .resident_commit_unlocked_zero_bytes = value.resident_commit_unlocked_zero_bytes,
        .resident_rollback_batches = value.resident_rollback_batches,
        .range_index_capacity = value.range_index_capacity,
        .range_index_entries = value.range_index_entries,
        .range_index_tombstones = value.range_index_tombstones,
        .range_index_lookups = value.range_index_lookups,
        .range_index_hits = value.range_index_hits,
        .range_index_misses = value.range_index_misses,
        .range_index_probe_total = value.range_index_probe_total,
        .range_index_probe_max = value.range_index_probe_max,
        .range_index_probe_last = value.range_index_probe_last,
        .range_index_rebuilds = value.range_index_rebuilds,
        .range_index_insert_failures = value.range_index_insert_failures,
        .range_free_slot_lookups = value.range_free_slot_lookups,
        .range_free_slot_probe_total = value.range_free_slot_probe_total,
        .range_free_slot_probe_max = value.range_free_slot_probe_max,
        .range_free_slot_probe_last = value.range_free_slot_probe_last,
        .range_address_entries = value.range_address_entries,
        .range_address_lookups = value.range_address_lookups,
        .range_address_probe_total = value.range_address_probe_total,
        .range_address_probe_max = value.range_address_probe_max,
        .range_address_probe_last = value.range_address_probe_last,
        .commit_span_active = value.commit_span_active,
        .commit_span_lookups = value.commit_span_lookups,
        .commit_span_steps = value.commit_span_steps,
        .commit_span_step_max = value.commit_span_step_max,
        .page_state_span_active = value.page_state_span_active,
        .page_state_span_lookups = value.page_state_span_lookups,
        .page_state_span_steps = value.page_state_span_steps,
        .page_state_span_step_max = value.page_state_span_step_max,
        .reclaim_cursor_range_steps = value.reclaim_cursor_range_steps,
        .reclaim_cursor_span_steps = value.reclaim_cursor_span_steps,
        .reclaim_cursor_page_steps = value.reclaim_cursor_page_steps,
        .reclaim_cursor_wraps = value.reclaim_cursor_wraps,
    };
}
const RawPageStateInput = extern struct {
    operation: u32,
    region_id: u32,
    region_offset: u64,
    page_count: u64,
    slot_reservation_id: u32,
    slot_index: u64,
    slot_generation: u64,
    flags: u32,
};
fn rawPageStateInput(value: PageStateInput) RawPageStateInput {
    return .{
        .operation = value.operation,
        .region_id = value.region_id,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .slot_reservation_id = value.slot_reservation_id,
        .slot_index = value.slot_index,
        .slot_generation = value.slot_generation,
        .flags = value.flags,
    };
}
fn fromRawPageStateInput(value: RawPageStateInput) PageStateInput {
    return .{
        .operation = value.operation,
        .region_id = value.region_id,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .slot_reservation_id = value.slot_reservation_id,
        .slot_index = value.slot_index,
        .slot_generation = value.slot_generation,
        .flags = value.flags,
    };
}
const RawPageStateResult = extern struct {
    version: u32,
    status: u32,
    operation: u32,
    flags: u32,
    blockers: u32,
    region_id: u32,
    page_size: u32,
    max_spans: u32,
    span_count: u32,
    region_offset: u64,
    page_count: u64,
    committed_pages: u64,
    resident_pages: u64,
    nonresident_pages: u64,
    dirty_pages: u64,
    clean_pages: u64,
    pinned_pages: u64,
    busy_pages: u64,
    error_pages: u64,
    slot_bound_pages: u64,
    slot_reservation_id: u32,
    page_count_lo: u32,
    slot_index: u64,
    slot_generation: u64,
    total_transitions: u64,
    dirty_marks: u64,
    clean_marks: u64,
    slot_binds: u64,
    slot_clears: u64,
    pinned_marks: u64,
    pinned_clears: u64,
    busy_marks: u64,
    busy_clears: u64,
    error_marks: u64,
    error_clears: u64,
    table_full_failures: u64,
    cleanup_pages: u64,
};
fn rawPageStateResult(value: PageStateResult) RawPageStateResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .page_size = value.page_size,
        .max_spans = value.max_spans,
        .span_count = value.span_count,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .committed_pages = value.committed_pages,
        .resident_pages = value.resident_pages,
        .nonresident_pages = value.nonresident_pages,
        .dirty_pages = value.dirty_pages,
        .clean_pages = value.clean_pages,
        .pinned_pages = value.pinned_pages,
        .busy_pages = value.busy_pages,
        .error_pages = value.error_pages,
        .slot_bound_pages = value.slot_bound_pages,
        .slot_reservation_id = value.slot_reservation_id,
        .page_count_lo = value.page_count_lo,
        .slot_index = value.slot_index,
        .slot_generation = value.slot_generation,
        .total_transitions = value.total_transitions,
        .dirty_marks = value.dirty_marks,
        .clean_marks = value.clean_marks,
        .slot_binds = value.slot_binds,
        .slot_clears = value.slot_clears,
        .pinned_marks = value.pinned_marks,
        .pinned_clears = value.pinned_clears,
        .busy_marks = value.busy_marks,
        .busy_clears = value.busy_clears,
        .error_marks = value.error_marks,
        .error_clears = value.error_clears,
        .table_full_failures = value.table_full_failures,
        .cleanup_pages = value.cleanup_pages,
    };
}
fn fromRawPageStateResult(value: RawPageStateResult) PageStateResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .page_size = value.page_size,
        .max_spans = value.max_spans,
        .span_count = value.span_count,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .committed_pages = value.committed_pages,
        .resident_pages = value.resident_pages,
        .nonresident_pages = value.nonresident_pages,
        .dirty_pages = value.dirty_pages,
        .clean_pages = value.clean_pages,
        .pinned_pages = value.pinned_pages,
        .busy_pages = value.busy_pages,
        .error_pages = value.error_pages,
        .slot_bound_pages = value.slot_bound_pages,
        .slot_reservation_id = value.slot_reservation_id,
        .page_count_lo = value.page_count_lo,
        .slot_index = value.slot_index,
        .slot_generation = value.slot_generation,
        .total_transitions = value.total_transitions,
        .dirty_marks = value.dirty_marks,
        .clean_marks = value.clean_marks,
        .slot_binds = value.slot_binds,
        .slot_clears = value.slot_clears,
        .pinned_marks = value.pinned_marks,
        .pinned_clears = value.pinned_clears,
        .busy_marks = value.busy_marks,
        .busy_clears = value.busy_clears,
        .error_marks = value.error_marks,
        .error_clears = value.error_clears,
        .table_full_failures = value.table_full_failures,
        .cleanup_pages = value.cleanup_pages,
    };
}
const RawPageStateSummary = extern struct {
    enabled: bool,
    last_status: u32,
    last_operation: u32,
    last_flags: u32,
    last_blockers: u32,
    last_region_id: u32,
    last_region_offset: u64,
    last_page_count: u64,
    last_slot_reservation_id: u32,
    last_slot_index: u64,
    last_slot_generation: u64,
    committed_pages: u64,
    resident_pages: u64,
    nonresident_pages: u64,
    dirty_pages: u64,
    clean_pages: u64,
    pinned_pages: u64,
    busy_pages: u64,
    error_pages: u64,
    fault_page_ins: u64,
    fault_page_in_failures: u64,
    page_out_nonresident_pages: u64,
    eviction_attempts: u64,
    eviction_successes: u64,
    eviction_failures: u64,
    eviction_candidates: u64,
    eviction_page_outs: u64,
    eviction_clean_pages: u64,
    eviction_dirty_pages: u64,
    eviction_returned_frames: u64,
    eviction_no_backing: u64,
    eviction_no_candidate: u64,
    eviction_slot_failures: u64,
    eviction_io_failures: u64,
    eviction_skipped_nonresident: u64,
    eviction_skipped_pinned: u64,
    eviction_skipped_busy: u64,
    eviction_redirtied_pages: u64,
    eviction_skipped_error: u64,
    eviction_skipped_unmapped: u64,
    pager_failed_page_outs: u64,
    pager_failed_page_ins: u64,
    pager_data_preserved_pages: u64,
    pager_data_lost_pages: u64,
    pager_dirty_preserved_pages: u64,
    pager_disabled_eviction_gates: u64,
    slot_bound_pages: u64,
    span_count: u32,
    max_spans: u32,
    transitions: u64,
    dirty_marks: u64,
    clean_marks: u64,
    slot_binds: u64,
    slot_clears: u64,
    pinned_marks: u64,
    pinned_clears: u64,
    busy_marks: u64,
    busy_clears: u64,
    error_marks: u64,
    error_clears: u64,
    table_full_failures: u64,
    cleanup_pages: u64,
};
fn rawPageStateSummary(value: PageStateSummary) RawPageStateSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_region_offset = value.last_region_offset,
        .last_page_count = value.last_page_count,
        .last_slot_reservation_id = value.last_slot_reservation_id,
        .last_slot_index = value.last_slot_index,
        .last_slot_generation = value.last_slot_generation,
        .committed_pages = value.committed_pages,
        .resident_pages = value.resident_pages,
        .nonresident_pages = value.nonresident_pages,
        .dirty_pages = value.dirty_pages,
        .clean_pages = value.clean_pages,
        .pinned_pages = value.pinned_pages,
        .busy_pages = value.busy_pages,
        .error_pages = value.error_pages,
        .fault_page_ins = value.fault_page_ins,
        .fault_page_in_failures = value.fault_page_in_failures,
        .page_out_nonresident_pages = value.page_out_nonresident_pages,
        .eviction_attempts = value.eviction_attempts,
        .eviction_successes = value.eviction_successes,
        .eviction_failures = value.eviction_failures,
        .eviction_candidates = value.eviction_candidates,
        .eviction_page_outs = value.eviction_page_outs,
        .eviction_clean_pages = value.eviction_clean_pages,
        .eviction_dirty_pages = value.eviction_dirty_pages,
        .eviction_returned_frames = value.eviction_returned_frames,
        .eviction_no_backing = value.eviction_no_backing,
        .eviction_no_candidate = value.eviction_no_candidate,
        .eviction_slot_failures = value.eviction_slot_failures,
        .eviction_io_failures = value.eviction_io_failures,
        .eviction_skipped_nonresident = value.eviction_skipped_nonresident,
        .eviction_skipped_pinned = value.eviction_skipped_pinned,
        .eviction_skipped_busy = value.eviction_skipped_busy,
        .eviction_redirtied_pages = value.eviction_redirtied_pages,
        .eviction_skipped_error = value.eviction_skipped_error,
        .eviction_skipped_unmapped = value.eviction_skipped_unmapped,
        .pager_failed_page_outs = value.pager_failed_page_outs,
        .pager_failed_page_ins = value.pager_failed_page_ins,
        .pager_data_preserved_pages = value.pager_data_preserved_pages,
        .pager_data_lost_pages = value.pager_data_lost_pages,
        .pager_dirty_preserved_pages = value.pager_dirty_preserved_pages,
        .pager_disabled_eviction_gates = value.pager_disabled_eviction_gates,
        .slot_bound_pages = value.slot_bound_pages,
        .span_count = value.span_count,
        .max_spans = value.max_spans,
        .transitions = value.transitions,
        .dirty_marks = value.dirty_marks,
        .clean_marks = value.clean_marks,
        .slot_binds = value.slot_binds,
        .slot_clears = value.slot_clears,
        .pinned_marks = value.pinned_marks,
        .pinned_clears = value.pinned_clears,
        .busy_marks = value.busy_marks,
        .busy_clears = value.busy_clears,
        .error_marks = value.error_marks,
        .error_clears = value.error_clears,
        .table_full_failures = value.table_full_failures,
        .cleanup_pages = value.cleanup_pages,
    };
}
fn fromRawPageStateSummary(value: RawPageStateSummary) PageStateSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_region_offset = value.last_region_offset,
        .last_page_count = value.last_page_count,
        .last_slot_reservation_id = value.last_slot_reservation_id,
        .last_slot_index = value.last_slot_index,
        .last_slot_generation = value.last_slot_generation,
        .committed_pages = value.committed_pages,
        .resident_pages = value.resident_pages,
        .nonresident_pages = value.nonresident_pages,
        .dirty_pages = value.dirty_pages,
        .clean_pages = value.clean_pages,
        .pinned_pages = value.pinned_pages,
        .busy_pages = value.busy_pages,
        .error_pages = value.error_pages,
        .fault_page_ins = value.fault_page_ins,
        .fault_page_in_failures = value.fault_page_in_failures,
        .page_out_nonresident_pages = value.page_out_nonresident_pages,
        .eviction_attempts = value.eviction_attempts,
        .eviction_successes = value.eviction_successes,
        .eviction_failures = value.eviction_failures,
        .eviction_candidates = value.eviction_candidates,
        .eviction_page_outs = value.eviction_page_outs,
        .eviction_clean_pages = value.eviction_clean_pages,
        .eviction_dirty_pages = value.eviction_dirty_pages,
        .eviction_returned_frames = value.eviction_returned_frames,
        .eviction_no_backing = value.eviction_no_backing,
        .eviction_no_candidate = value.eviction_no_candidate,
        .eviction_slot_failures = value.eviction_slot_failures,
        .eviction_io_failures = value.eviction_io_failures,
        .eviction_skipped_nonresident = value.eviction_skipped_nonresident,
        .eviction_skipped_pinned = value.eviction_skipped_pinned,
        .eviction_skipped_busy = value.eviction_skipped_busy,
        .eviction_redirtied_pages = value.eviction_redirtied_pages,
        .eviction_skipped_error = value.eviction_skipped_error,
        .eviction_skipped_unmapped = value.eviction_skipped_unmapped,
        .pager_failed_page_outs = value.pager_failed_page_outs,
        .pager_failed_page_ins = value.pager_failed_page_ins,
        .pager_data_preserved_pages = value.pager_data_preserved_pages,
        .pager_data_lost_pages = value.pager_data_lost_pages,
        .pager_dirty_preserved_pages = value.pager_dirty_preserved_pages,
        .pager_disabled_eviction_gates = value.pager_disabled_eviction_gates,
        .slot_bound_pages = value.slot_bound_pages,
        .span_count = value.span_count,
        .max_spans = value.max_spans,
        .transitions = value.transitions,
        .dirty_marks = value.dirty_marks,
        .clean_marks = value.clean_marks,
        .slot_binds = value.slot_binds,
        .slot_clears = value.slot_clears,
        .pinned_marks = value.pinned_marks,
        .pinned_clears = value.pinned_clears,
        .busy_marks = value.busy_marks,
        .busy_clears = value.busy_clears,
        .error_marks = value.error_marks,
        .error_clears = value.error_clears,
        .table_full_failures = value.table_full_failures,
        .cleanup_pages = value.cleanup_pages,
    };
}
const RawWriteback = extern struct {
    region_id: u32,
    region_offset: u64,
    page_count: u64,
    unwind: task_context.UnwindToken,
    active: bool,
    completed: bool,
};
fn rawWriteback(value: PageWriteback) RawWriteback {
    return .{ .region_id=value.region_id, .region_offset=value.region_offset, .page_count=value.page_count,
        .unwind=value.unwind, .active=value.active, .completed=value.completed };
}
fn fromRawWriteback(value: RawWriteback) PageWriteback {
    return .{ .region_id=value.region_id, .region_offset=value.region_offset, .page_count=value.page_count,
        .unwind=value.unwind, .active=value.active, .completed=value.completed };
}
const RawReclaim = extern struct { returned_frames: u32, returned_bytes: u64, page_outs: u64, failures: u64 };
const RawWindow = extern struct { base: u64, len: u64, name: [*]const u8, name_len: usize };
extern fn r4os_rust_vm_pager_init() callconv(.c) bool;
extern fn r4os_rust_vm_reserve(*const RawReserve, u64, *u32) callconv(.c) u32;
extern fn r4os_rust_vm_reserve_fixed(*const RawReserve, *u32) callconv(.c) u32;
extern fn r4os_rust_vm_commit(u32, u64, u64) callconv(.c) u32;
extern fn r4os_rust_vm_finish_rollback(u32) callconv(.c) u32;
extern fn r4os_rust_vm_commit_resident(u32, u64, u64) callconv(.c) u32;
extern fn r4os_rust_vm_demand_fault(u64, u64, u8, u64) callconv(.c) bool;
extern fn r4os_rust_vm_uncommit(u32, u64, u64) callconv(.c) u32;
extern fn r4os_rust_vm_release(u32) callconv(.c) u32;
extern fn r4os_rust_vm_release_owner(u8, u64, u8, bool, *u64) callconv(.c) u32;
extern fn r4os_rust_vm_protect_guard(u32, u64, u64, bool) callconv(.c) u32;
extern fn r4os_rust_vm_range_info(u32, *Range) callconv(.c) bool;
extern fn r4os_rust_vm_page_state_probe(*const RawPageStateInput, *RawPageStateResult) callconv(.c) void;
extern fn r4os_rust_vm_apply_page_io(*const RawPageStateInput, bool, *RawPageStateResult) callconv(.c) void;
extern fn r4os_rust_vm_page_state_summary(*RawPageStateSummary) callconv(.c) void;
extern fn r4os_rust_vm_policy_failure(*const RawPageStateInput, bool) callconv(.c) void;
extern fn r4os_rust_vm_window_usage(u8, *WindowUsage) callconv(.c) void;
extern fn r4os_rust_vm_stats(*Stats) callconv(.c) void;
extern fn r4os_rust_vm_owner_stats(u8, u64, u8, u8, *OwnerStats) callconv(.c) void;
extern fn r4os_rust_vm_dump_stats() callconv(.c) void;
extern fn r4os_rust_vm_window(u8, *RawWindow) callconv(.c) void;
extern fn r4os_rust_vm_begin_writeback(u32, u64, u64, [*]const u8, *RawWriteback) callconv(.c) u32;
extern fn r4os_rust_vm_end_writeback(*RawWriteback) callconv(.c) void;
extern fn r4os_rust_vm_complete_writeback(*RawWriteback, *const RawPageStateInput, *u64) callconv(.c) u32;
extern fn r4os_rust_vm_commit_covers(u32, u64, u64) callconv(.c) bool;
extern fn r4os_rust_vm_reclaim_evict(u32, u32, u64, bool, *RawReclaim) callconv(.c) void;
extern fn r4os_rust_vm_evictable_bytes(bool) callconv(.c) u64;
extern fn r4os_rust_vm_hot_stats(*RawHotPathStats) callconv(.c) void;
extern fn r4os_rust_vm_metadata_invariant() callconv(.c) bool;

pub fn init() bool { return r4os_rust_vm_pager_init(); }
pub fn reserve(req: ReserveRequest) Error!u32 {
    const raw: RawReserve = .{ .eviction_allowed=req.eviction_allowed, .window=@intFromEnum(req.window),
        .kind=@intFromEnum(req.kind), .owner=@intFromEnum(req.owner), .owner_id=req.owner_id,
        .name_ptr=req.name.ptr, .name_len=req.name.len, .flags=req.flags, .base=0, .len=req.len };
    var id: u32 = undefined;
    try policyCheck(r4os_rust_vm_reserve(&raw, req.alignment, &id));
    return id;
}
pub fn reserveAt(req: ReserveAtRequest) Error!u32 {
    const raw: RawReserve = .{ .eviction_allowed=req.eviction_allowed, .window=@intFromEnum(req.window),
        .kind=@intFromEnum(req.kind), .owner=@intFromEnum(req.owner), .owner_id=req.owner_id,
        .name_ptr=req.name.ptr, .name_len=req.name.len, .flags=req.flags, .base=req.base, .len=req.len };
    var id: u32 = undefined;
    try policyCheck(r4os_rust_vm_reserve_fixed(&raw, &id));
    return id;
}
pub fn commit(id: u32, offset: u64, len: u64) Error!void { try policyCheck(r4os_rust_vm_commit(id, offset, len)); }
pub fn finishResidentRollback(id: u32) Error!void { try policyCheck(r4os_rust_vm_finish_rollback(id)); }
pub fn commitResident(id: u32, offset: u64, len: u64) Error!void { try policyCheck(r4os_rust_vm_commit_resident(id, offset, len)); }
pub fn handleDemandFault(address: u64, code: u64, owner: blocks.Owner, owner_id: u64) bool {
    return r4os_rust_vm_demand_fault(address, code, @intFromEnum(owner), owner_id);
}
pub fn uncommit(id: u32, offset: u64, len: u64) Error!void { try policyCheck(r4os_rust_vm_uncommit(id, offset, len)); }
pub fn release(id: u32) Error!void { try policyCheck(r4os_rust_vm_release(id)); }
pub fn releaseOwner(owner: blocks.Owner, owner_id: u64, kind: ?blocks.Kind) Error!u64 {
    var count: u64 = undefined;
    try policyCheck(r4os_rust_vm_release_owner(@intFromEnum(owner), owner_id, if (kind) |knd| @intFromEnum(knd) else 0, kind != null, &count));
    return count;
}
pub fn protectGuard(id: u32, offset: u64, len: u64) Error!void { try policyCheck(r4os_rust_vm_protect_guard(id, offset, len, false)); }
pub fn clearGuard(id: u32) Error!void { try policyCheck(r4os_rust_vm_protect_guard(id, 0, 0, true)); }
pub fn rangeInfo(id: u32) ?RangeInfo {
    var range: Range = undefined;
    return if (r4os_rust_vm_range_info(id, &range)) infoFromRange(range) else null;
}
pub fn pageStateProbe(input: PageStateInput) PageStateResult {
    const raw = rawPageStateInput(input);
    var out: RawPageStateResult = undefined;
    r4os_rust_vm_page_state_probe(&raw, &out);
    return fromRawPageStateResult(out);
}
pub fn applyPageIoState(input: PageStateInput, page_out: bool) PageStateResult {
    const raw = rawPageStateInput(input);
    var out: RawPageStateResult = undefined;
    r4os_rust_vm_apply_page_io(&raw, page_out, &out);
    return fromRawPageStateResult(out);
}
pub fn pageStateSummary() PageStateSummary {
    var out: RawPageStateSummary = undefined;
    r4os_rust_vm_page_state_summary(&out);
    return fromRawPageStateSummary(out);
}
pub fn recordPagerPolicyFailure(input: PageStateInput, page_out: bool) void {
    const raw = rawPageStateInput(input);
    r4os_rust_vm_policy_failure(&raw, page_out);
}
pub fn windowUsage(window: Window) WindowUsage {
    var out: WindowUsage = undefined;
    r4os_rust_vm_window_usage(@intFromEnum(window), &out);
    return out;
}
pub fn stats() Stats { var out: Stats = undefined; r4os_rust_vm_stats(&out); return out; }
pub fn ownerStats(owner: blocks.Owner, owner_id: u64, window: ?Window, kind: ?blocks.Kind) OwnerStats {
    var out: OwnerStats = undefined;
    r4os_rust_vm_owner_stats(@intFromEnum(owner), owner_id, if (window) |w| @intFromEnum(w) else 255, if (kind) |knd| @intFromEnum(knd) else 255, &out);
    return out;
}
pub fn dumpStats() void { r4os_rust_vm_dump_stats(); }
fn windowValue(window: Window) RawWindow { var out: RawWindow = undefined; r4os_rust_vm_window(@intFromEnum(window), &out); return out; }
pub fn windowBase(window: Window) u64 { return windowValue(window).base; }
pub fn windowLen(window: Window) u64 { return windowValue(window).len; }
pub fn windowName(window: Window) []const u8 { const value = windowValue(window); return value.name[0..value.name_len]; }
pub fn beginPageWriteback(id: u32, offset: u64, pages: u64, pointer: [*]const u8) Error!PageWriteback {
    var out: RawWriteback = undefined;
    try policyCheck(r4os_rust_vm_begin_writeback(id, offset, pages, pointer, &out));
    return fromRawWriteback(out);
}
pub fn endPageWriteback(value: *PageWriteback) void {
    var raw = rawWriteback(value.*);
    r4os_rust_vm_end_writeback(&raw);
    value.* = fromRawWriteback(raw);
}
pub fn completePageWriteback(value: *PageWriteback, binding: PageStateInput) Error!u64 {
    var raw = rawWriteback(value.*);
    const bound = rawPageStateInput(binding);
    var out: u64 = undefined;
    const code = r4os_rust_vm_complete_writeback(&raw, &bound, &out);
    value.* = fromRawWriteback(raw);
    try policyCheck(code);
    return out;
}
pub fn commitCovers(id: u32, offset: u64, pages: u64) bool { return r4os_rust_vm_commit_covers(id, offset, pages); }
pub fn reclaimEvictFrames(reason: reclaim.Reason, frames: u32, owner: ?u64) reclaim.SourceResult {
    var out: RawReclaim = undefined;
    r4os_rust_vm_reclaim_evict(@intFromEnum(reason), frames, owner orelse 0, owner != null, &out);
    return .{ .returned_frames=out.returned_frames, .returned_bytes=out.returned_bytes, .page_outs=out.page_outs, .failures=out.failures };
}
pub fn evictableBytes() u64 { return r4os_rust_vm_evictable_bytes(false); }
pub fn evictableDirtyBytes() u64 { return r4os_rust_vm_evictable_bytes(true); }
pub fn hotPathStats() HotPathStats { var out: RawHotPathStats = undefined; r4os_rust_vm_hot_stats(&out); return fromRawHotPathStats(out); }
pub fn metadataInvariant() bool { return r4os_rust_vm_metadata_invariant(); }
// Original still-separate primitive owners. These C leaves have no VM state.
export fn r4os_zig_vm_reclaimer_address() callconv(.c) usize { return @intFromPtr(&reclaimEvictFrames); }
export fn r4os_zig_vm_reschedule() callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM reschedule requested by host fixture");
    _ = scheduler.safeReschedulePoint();
}
export fn r4os_zig_vm_file_read(path: [*:0]const u8, offset: u64, buffer: [*]u8, bytes: u32) callconv(.c) i32 {
    if (@import("builtin").is_test) @panic("kernel VM file read requested by host fixture");
    return r4sys_api.fileReadAt64(path, offset, buffer, bytes);
}
export fn r4os_zig_vm_file_write(path: [*:0]const u8, offset: u64, buffer: [*]const u8, bytes: u32) callconv(.c) i32 {
    if (@import("builtin").is_test) @panic("kernel VM file write requested by host fixture");
    return r4sys_api.fileWriteAt(path, offset, buffer, bytes);
}
export fn r4os_zig_vm_puts(bytes: [*]const u8, len: usize) callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM output requested by host fixture");
    k.puts(bytes[0..len]);
}
export fn r4os_zig_vm_dec(value: u64) callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM output requested by host fixture");
    k.putDec(value);
}
export fn r4os_zig_vm_hex(value: u64, width: u8) callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel VM output requested by host fixture");
    k.putHex(value, width);
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
comptime {
    if (@sizeOf(RawHotPathStats) != 248 or @alignOf(RawHotPathStats) != 8) @compileError("Native VM HotPathStats geometry");
    if (@offsetOf(RawHotPathStats, "resident_commit_batches") != 0) @compileError("Native VM HotPathStats.resident_commit_batches offset");
    if (@offsetOf(RawHotPathStats, "resident_commit_max_batch_pages") != 8) @compileError("Native VM HotPathStats.resident_commit_max_batch_pages offset");
    if (@offsetOf(RawHotPathStats, "resident_commit_unlocked_zero_bytes") != 16) @compileError("Native VM HotPathStats.resident_commit_unlocked_zero_bytes offset");
    if (@offsetOf(RawHotPathStats, "resident_rollback_batches") != 24) @compileError("Native VM HotPathStats.resident_rollback_batches offset");
    if (@offsetOf(RawHotPathStats, "range_index_capacity") != 32) @compileError("Native VM HotPathStats.range_index_capacity offset");
    if (@offsetOf(RawHotPathStats, "range_index_entries") != 36) @compileError("Native VM HotPathStats.range_index_entries offset");
    if (@offsetOf(RawHotPathStats, "range_index_tombstones") != 40) @compileError("Native VM HotPathStats.range_index_tombstones offset");
    if (@offsetOf(RawHotPathStats, "range_index_lookups") != 48) @compileError("Native VM HotPathStats.range_index_lookups offset");
    if (@offsetOf(RawHotPathStats, "range_index_hits") != 56) @compileError("Native VM HotPathStats.range_index_hits offset");
    if (@offsetOf(RawHotPathStats, "range_index_misses") != 64) @compileError("Native VM HotPathStats.range_index_misses offset");
    if (@offsetOf(RawHotPathStats, "range_index_probe_total") != 72) @compileError("Native VM HotPathStats.range_index_probe_total offset");
    if (@offsetOf(RawHotPathStats, "range_index_probe_max") != 80) @compileError("Native VM HotPathStats.range_index_probe_max offset");
    if (@offsetOf(RawHotPathStats, "range_index_probe_last") != 84) @compileError("Native VM HotPathStats.range_index_probe_last offset");
    if (@offsetOf(RawHotPathStats, "range_index_rebuilds") != 88) @compileError("Native VM HotPathStats.range_index_rebuilds offset");
    if (@offsetOf(RawHotPathStats, "range_index_insert_failures") != 96) @compileError("Native VM HotPathStats.range_index_insert_failures offset");
    if (@offsetOf(RawHotPathStats, "range_free_slot_lookups") != 104) @compileError("Native VM HotPathStats.range_free_slot_lookups offset");
    if (@offsetOf(RawHotPathStats, "range_free_slot_probe_total") != 112) @compileError("Native VM HotPathStats.range_free_slot_probe_total offset");
    if (@offsetOf(RawHotPathStats, "range_free_slot_probe_max") != 120) @compileError("Native VM HotPathStats.range_free_slot_probe_max offset");
    if (@offsetOf(RawHotPathStats, "range_free_slot_probe_last") != 124) @compileError("Native VM HotPathStats.range_free_slot_probe_last offset");
    if (@offsetOf(RawHotPathStats, "range_address_entries") != 128) @compileError("Native VM HotPathStats.range_address_entries offset");
    if (@offsetOf(RawHotPathStats, "range_address_lookups") != 136) @compileError("Native VM HotPathStats.range_address_lookups offset");
    if (@offsetOf(RawHotPathStats, "range_address_probe_total") != 144) @compileError("Native VM HotPathStats.range_address_probe_total offset");
    if (@offsetOf(RawHotPathStats, "range_address_probe_max") != 152) @compileError("Native VM HotPathStats.range_address_probe_max offset");
    if (@offsetOf(RawHotPathStats, "range_address_probe_last") != 156) @compileError("Native VM HotPathStats.range_address_probe_last offset");
    if (@offsetOf(RawHotPathStats, "commit_span_active") != 160) @compileError("Native VM HotPathStats.commit_span_active offset");
    if (@offsetOf(RawHotPathStats, "commit_span_lookups") != 168) @compileError("Native VM HotPathStats.commit_span_lookups offset");
    if (@offsetOf(RawHotPathStats, "commit_span_steps") != 176) @compileError("Native VM HotPathStats.commit_span_steps offset");
    if (@offsetOf(RawHotPathStats, "commit_span_step_max") != 184) @compileError("Native VM HotPathStats.commit_span_step_max offset");
    if (@offsetOf(RawHotPathStats, "page_state_span_active") != 188) @compileError("Native VM HotPathStats.page_state_span_active offset");
    if (@offsetOf(RawHotPathStats, "page_state_span_lookups") != 192) @compileError("Native VM HotPathStats.page_state_span_lookups offset");
    if (@offsetOf(RawHotPathStats, "page_state_span_steps") != 200) @compileError("Native VM HotPathStats.page_state_span_steps offset");
    if (@offsetOf(RawHotPathStats, "page_state_span_step_max") != 208) @compileError("Native VM HotPathStats.page_state_span_step_max offset");
    if (@offsetOf(RawHotPathStats, "reclaim_cursor_range_steps") != 216) @compileError("Native VM HotPathStats.reclaim_cursor_range_steps offset");
    if (@offsetOf(RawHotPathStats, "reclaim_cursor_span_steps") != 224) @compileError("Native VM HotPathStats.reclaim_cursor_span_steps offset");
    if (@offsetOf(RawHotPathStats, "reclaim_cursor_page_steps") != 232) @compileError("Native VM HotPathStats.reclaim_cursor_page_steps offset");
    if (@offsetOf(RawHotPathStats, "reclaim_cursor_wraps") != 240) @compileError("Native VM HotPathStats.reclaim_cursor_wraps offset");
    if (@sizeOf(RawPageStateInput) != 56 or @alignOf(RawPageStateInput) != 8) @compileError("Native VM PageStateInput geometry");
    if (@offsetOf(RawPageStateInput, "operation") != 0) @compileError("Native VM PageStateInput.operation offset");
    if (@offsetOf(RawPageStateInput, "region_id") != 4) @compileError("Native VM PageStateInput.region_id offset");
    if (@offsetOf(RawPageStateInput, "region_offset") != 8) @compileError("Native VM PageStateInput.region_offset offset");
    if (@offsetOf(RawPageStateInput, "page_count") != 16) @compileError("Native VM PageStateInput.page_count offset");
    if (@offsetOf(RawPageStateInput, "slot_reservation_id") != 24) @compileError("Native VM PageStateInput.slot_reservation_id offset");
    if (@offsetOf(RawPageStateInput, "slot_index") != 32) @compileError("Native VM PageStateInput.slot_index offset");
    if (@offsetOf(RawPageStateInput, "slot_generation") != 40) @compileError("Native VM PageStateInput.slot_generation offset");
    if (@offsetOf(RawPageStateInput, "flags") != 48) @compileError("Native VM PageStateInput.flags offset");
    if (@sizeOf(RawPageStateResult) != 256 or @alignOf(RawPageStateResult) != 8) @compileError("Native VM PageStateResult geometry");
    if (@offsetOf(RawPageStateResult, "version") != 0) @compileError("Native VM PageStateResult.version offset");
    if (@offsetOf(RawPageStateResult, "status") != 4) @compileError("Native VM PageStateResult.status offset");
    if (@offsetOf(RawPageStateResult, "operation") != 8) @compileError("Native VM PageStateResult.operation offset");
    if (@offsetOf(RawPageStateResult, "flags") != 12) @compileError("Native VM PageStateResult.flags offset");
    if (@offsetOf(RawPageStateResult, "blockers") != 16) @compileError("Native VM PageStateResult.blockers offset");
    if (@offsetOf(RawPageStateResult, "region_id") != 20) @compileError("Native VM PageStateResult.region_id offset");
    if (@offsetOf(RawPageStateResult, "page_size") != 24) @compileError("Native VM PageStateResult.page_size offset");
    if (@offsetOf(RawPageStateResult, "max_spans") != 28) @compileError("Native VM PageStateResult.max_spans offset");
    if (@offsetOf(RawPageStateResult, "span_count") != 32) @compileError("Native VM PageStateResult.span_count offset");
    if (@offsetOf(RawPageStateResult, "region_offset") != 40) @compileError("Native VM PageStateResult.region_offset offset");
    if (@offsetOf(RawPageStateResult, "page_count") != 48) @compileError("Native VM PageStateResult.page_count offset");
    if (@offsetOf(RawPageStateResult, "committed_pages") != 56) @compileError("Native VM PageStateResult.committed_pages offset");
    if (@offsetOf(RawPageStateResult, "resident_pages") != 64) @compileError("Native VM PageStateResult.resident_pages offset");
    if (@offsetOf(RawPageStateResult, "nonresident_pages") != 72) @compileError("Native VM PageStateResult.nonresident_pages offset");
    if (@offsetOf(RawPageStateResult, "dirty_pages") != 80) @compileError("Native VM PageStateResult.dirty_pages offset");
    if (@offsetOf(RawPageStateResult, "clean_pages") != 88) @compileError("Native VM PageStateResult.clean_pages offset");
    if (@offsetOf(RawPageStateResult, "pinned_pages") != 96) @compileError("Native VM PageStateResult.pinned_pages offset");
    if (@offsetOf(RawPageStateResult, "busy_pages") != 104) @compileError("Native VM PageStateResult.busy_pages offset");
    if (@offsetOf(RawPageStateResult, "error_pages") != 112) @compileError("Native VM PageStateResult.error_pages offset");
    if (@offsetOf(RawPageStateResult, "slot_bound_pages") != 120) @compileError("Native VM PageStateResult.slot_bound_pages offset");
    if (@offsetOf(RawPageStateResult, "slot_reservation_id") != 128) @compileError("Native VM PageStateResult.slot_reservation_id offset");
    if (@offsetOf(RawPageStateResult, "page_count_lo") != 132) @compileError("Native VM PageStateResult.page_count_lo offset");
    if (@offsetOf(RawPageStateResult, "slot_index") != 136) @compileError("Native VM PageStateResult.slot_index offset");
    if (@offsetOf(RawPageStateResult, "slot_generation") != 144) @compileError("Native VM PageStateResult.slot_generation offset");
    if (@offsetOf(RawPageStateResult, "total_transitions") != 152) @compileError("Native VM PageStateResult.total_transitions offset");
    if (@offsetOf(RawPageStateResult, "dirty_marks") != 160) @compileError("Native VM PageStateResult.dirty_marks offset");
    if (@offsetOf(RawPageStateResult, "clean_marks") != 168) @compileError("Native VM PageStateResult.clean_marks offset");
    if (@offsetOf(RawPageStateResult, "slot_binds") != 176) @compileError("Native VM PageStateResult.slot_binds offset");
    if (@offsetOf(RawPageStateResult, "slot_clears") != 184) @compileError("Native VM PageStateResult.slot_clears offset");
    if (@offsetOf(RawPageStateResult, "pinned_marks") != 192) @compileError("Native VM PageStateResult.pinned_marks offset");
    if (@offsetOf(RawPageStateResult, "pinned_clears") != 200) @compileError("Native VM PageStateResult.pinned_clears offset");
    if (@offsetOf(RawPageStateResult, "busy_marks") != 208) @compileError("Native VM PageStateResult.busy_marks offset");
    if (@offsetOf(RawPageStateResult, "busy_clears") != 216) @compileError("Native VM PageStateResult.busy_clears offset");
    if (@offsetOf(RawPageStateResult, "error_marks") != 224) @compileError("Native VM PageStateResult.error_marks offset");
    if (@offsetOf(RawPageStateResult, "error_clears") != 232) @compileError("Native VM PageStateResult.error_clears offset");
    if (@offsetOf(RawPageStateResult, "table_full_failures") != 240) @compileError("Native VM PageStateResult.table_full_failures offset");
    if (@offsetOf(RawPageStateResult, "cleanup_pages") != 248) @compileError("Native VM PageStateResult.cleanup_pages offset");
    if (@sizeOf(RawPageStateSummary) != 464 or @alignOf(RawPageStateSummary) != 8) @compileError("Native VM PageStateSummary geometry");
    if (@offsetOf(RawPageStateSummary, "enabled") != 0) @compileError("Native VM PageStateSummary.enabled offset");
    if (@offsetOf(RawPageStateSummary, "last_status") != 4) @compileError("Native VM PageStateSummary.last_status offset");
    if (@offsetOf(RawPageStateSummary, "last_operation") != 8) @compileError("Native VM PageStateSummary.last_operation offset");
    if (@offsetOf(RawPageStateSummary, "last_flags") != 12) @compileError("Native VM PageStateSummary.last_flags offset");
    if (@offsetOf(RawPageStateSummary, "last_blockers") != 16) @compileError("Native VM PageStateSummary.last_blockers offset");
    if (@offsetOf(RawPageStateSummary, "last_region_id") != 20) @compileError("Native VM PageStateSummary.last_region_id offset");
    if (@offsetOf(RawPageStateSummary, "last_region_offset") != 24) @compileError("Native VM PageStateSummary.last_region_offset offset");
    if (@offsetOf(RawPageStateSummary, "last_page_count") != 32) @compileError("Native VM PageStateSummary.last_page_count offset");
    if (@offsetOf(RawPageStateSummary, "last_slot_reservation_id") != 40) @compileError("Native VM PageStateSummary.last_slot_reservation_id offset");
    if (@offsetOf(RawPageStateSummary, "last_slot_index") != 48) @compileError("Native VM PageStateSummary.last_slot_index offset");
    if (@offsetOf(RawPageStateSummary, "last_slot_generation") != 56) @compileError("Native VM PageStateSummary.last_slot_generation offset");
    if (@offsetOf(RawPageStateSummary, "committed_pages") != 64) @compileError("Native VM PageStateSummary.committed_pages offset");
    if (@offsetOf(RawPageStateSummary, "resident_pages") != 72) @compileError("Native VM PageStateSummary.resident_pages offset");
    if (@offsetOf(RawPageStateSummary, "nonresident_pages") != 80) @compileError("Native VM PageStateSummary.nonresident_pages offset");
    if (@offsetOf(RawPageStateSummary, "dirty_pages") != 88) @compileError("Native VM PageStateSummary.dirty_pages offset");
    if (@offsetOf(RawPageStateSummary, "clean_pages") != 96) @compileError("Native VM PageStateSummary.clean_pages offset");
    if (@offsetOf(RawPageStateSummary, "pinned_pages") != 104) @compileError("Native VM PageStateSummary.pinned_pages offset");
    if (@offsetOf(RawPageStateSummary, "busy_pages") != 112) @compileError("Native VM PageStateSummary.busy_pages offset");
    if (@offsetOf(RawPageStateSummary, "error_pages") != 120) @compileError("Native VM PageStateSummary.error_pages offset");
    if (@offsetOf(RawPageStateSummary, "fault_page_ins") != 128) @compileError("Native VM PageStateSummary.fault_page_ins offset");
    if (@offsetOf(RawPageStateSummary, "fault_page_in_failures") != 136) @compileError("Native VM PageStateSummary.fault_page_in_failures offset");
    if (@offsetOf(RawPageStateSummary, "page_out_nonresident_pages") != 144) @compileError("Native VM PageStateSummary.page_out_nonresident_pages offset");
    if (@offsetOf(RawPageStateSummary, "eviction_attempts") != 152) @compileError("Native VM PageStateSummary.eviction_attempts offset");
    if (@offsetOf(RawPageStateSummary, "eviction_successes") != 160) @compileError("Native VM PageStateSummary.eviction_successes offset");
    if (@offsetOf(RawPageStateSummary, "eviction_failures") != 168) @compileError("Native VM PageStateSummary.eviction_failures offset");
    if (@offsetOf(RawPageStateSummary, "eviction_candidates") != 176) @compileError("Native VM PageStateSummary.eviction_candidates offset");
    if (@offsetOf(RawPageStateSummary, "eviction_page_outs") != 184) @compileError("Native VM PageStateSummary.eviction_page_outs offset");
    if (@offsetOf(RawPageStateSummary, "eviction_clean_pages") != 192) @compileError("Native VM PageStateSummary.eviction_clean_pages offset");
    if (@offsetOf(RawPageStateSummary, "eviction_dirty_pages") != 200) @compileError("Native VM PageStateSummary.eviction_dirty_pages offset");
    if (@offsetOf(RawPageStateSummary, "eviction_returned_frames") != 208) @compileError("Native VM PageStateSummary.eviction_returned_frames offset");
    if (@offsetOf(RawPageStateSummary, "eviction_no_backing") != 216) @compileError("Native VM PageStateSummary.eviction_no_backing offset");
    if (@offsetOf(RawPageStateSummary, "eviction_no_candidate") != 224) @compileError("Native VM PageStateSummary.eviction_no_candidate offset");
    if (@offsetOf(RawPageStateSummary, "eviction_slot_failures") != 232) @compileError("Native VM PageStateSummary.eviction_slot_failures offset");
    if (@offsetOf(RawPageStateSummary, "eviction_io_failures") != 240) @compileError("Native VM PageStateSummary.eviction_io_failures offset");
    if (@offsetOf(RawPageStateSummary, "eviction_skipped_nonresident") != 248) @compileError("Native VM PageStateSummary.eviction_skipped_nonresident offset");
    if (@offsetOf(RawPageStateSummary, "eviction_skipped_pinned") != 256) @compileError("Native VM PageStateSummary.eviction_skipped_pinned offset");
    if (@offsetOf(RawPageStateSummary, "eviction_skipped_busy") != 264) @compileError("Native VM PageStateSummary.eviction_skipped_busy offset");
    if (@offsetOf(RawPageStateSummary, "eviction_redirtied_pages") != 272) @compileError("Native VM PageStateSummary.eviction_redirtied_pages offset");
    if (@offsetOf(RawPageStateSummary, "eviction_skipped_error") != 280) @compileError("Native VM PageStateSummary.eviction_skipped_error offset");
    if (@offsetOf(RawPageStateSummary, "eviction_skipped_unmapped") != 288) @compileError("Native VM PageStateSummary.eviction_skipped_unmapped offset");
    if (@offsetOf(RawPageStateSummary, "pager_failed_page_outs") != 296) @compileError("Native VM PageStateSummary.pager_failed_page_outs offset");
    if (@offsetOf(RawPageStateSummary, "pager_failed_page_ins") != 304) @compileError("Native VM PageStateSummary.pager_failed_page_ins offset");
    if (@offsetOf(RawPageStateSummary, "pager_data_preserved_pages") != 312) @compileError("Native VM PageStateSummary.pager_data_preserved_pages offset");
    if (@offsetOf(RawPageStateSummary, "pager_data_lost_pages") != 320) @compileError("Native VM PageStateSummary.pager_data_lost_pages offset");
    if (@offsetOf(RawPageStateSummary, "pager_dirty_preserved_pages") != 328) @compileError("Native VM PageStateSummary.pager_dirty_preserved_pages offset");
    if (@offsetOf(RawPageStateSummary, "pager_disabled_eviction_gates") != 336) @compileError("Native VM PageStateSummary.pager_disabled_eviction_gates offset");
    if (@offsetOf(RawPageStateSummary, "slot_bound_pages") != 344) @compileError("Native VM PageStateSummary.slot_bound_pages offset");
    if (@offsetOf(RawPageStateSummary, "span_count") != 352) @compileError("Native VM PageStateSummary.span_count offset");
    if (@offsetOf(RawPageStateSummary, "max_spans") != 356) @compileError("Native VM PageStateSummary.max_spans offset");
    if (@offsetOf(RawPageStateSummary, "transitions") != 360) @compileError("Native VM PageStateSummary.transitions offset");
    if (@offsetOf(RawPageStateSummary, "dirty_marks") != 368) @compileError("Native VM PageStateSummary.dirty_marks offset");
    if (@offsetOf(RawPageStateSummary, "clean_marks") != 376) @compileError("Native VM PageStateSummary.clean_marks offset");
    if (@offsetOf(RawPageStateSummary, "slot_binds") != 384) @compileError("Native VM PageStateSummary.slot_binds offset");
    if (@offsetOf(RawPageStateSummary, "slot_clears") != 392) @compileError("Native VM PageStateSummary.slot_clears offset");
    if (@offsetOf(RawPageStateSummary, "pinned_marks") != 400) @compileError("Native VM PageStateSummary.pinned_marks offset");
    if (@offsetOf(RawPageStateSummary, "pinned_clears") != 408) @compileError("Native VM PageStateSummary.pinned_clears offset");
    if (@offsetOf(RawPageStateSummary, "busy_marks") != 416) @compileError("Native VM PageStateSummary.busy_marks offset");
    if (@offsetOf(RawPageStateSummary, "busy_clears") != 424) @compileError("Native VM PageStateSummary.busy_clears offset");
    if (@offsetOf(RawPageStateSummary, "error_marks") != 432) @compileError("Native VM PageStateSummary.error_marks offset");
    if (@offsetOf(RawPageStateSummary, "error_clears") != 440) @compileError("Native VM PageStateSummary.error_clears offset");
    if (@offsetOf(RawPageStateSummary, "table_full_failures") != 448) @compileError("Native VM PageStateSummary.table_full_failures offset");
    if (@offsetOf(RawPageStateSummary, "cleanup_pages") != 456) @compileError("Native VM PageStateSummary.cleanup_pages offset");
    if (@sizeOf(OptionalSlot) != 4 or @sizeOf(RawReserve) != 56 or @sizeOf(Range) != 216 or
        @offsetOf(Range, "commit_span_head") != 202 or @offsetOf(Range, "page_state_span_head") != 206)
        @compileError("Native VM range/optional reservation C layout");
    if (@sizeOf(RawWriteback) != 48 or @offsetOf(RawWriteback,"unwind") != 24 or @offsetOf(RawWriteback,"active") != 40 or
        @offsetOf(RawWriteback,"completed") != 41 or @sizeOf(RawReclaim) != 32 or @sizeOf(RawWindow) != 32)
        @compileError("Native waiting VM C layout");
}
