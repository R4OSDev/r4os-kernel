//! Private VM page-state and pager values; no second public ABI.
#![allow(non_upper_case_globals)]
use crate::sched::task_context::Unwind;
pub const MAX_PAGE_STATE_SPANS: usize = 8192;
pub const PAGE: u64 = 4096;
pub const TABLE_FULL: u32 = 2;
pub const BAD_ALIGNMENT: u32 = 4;
pub const OVERFLOW: u32 = 5;
pub const NOT_FOUND: u32 = 8;
pub const NOT_COMMITTED: u32 = 10;
pub const MAP_FAILED: u32 = 13;
pub const BUSY: u32 = 14;
pub const OUTSIDE_WINDOW: u32 = 15;
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
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HotPathStats {
    pub resident_commit_batches: u64,
    pub resident_commit_max_batch_pages: u64,
    pub resident_commit_unlocked_zero_bytes: u64,
    pub resident_rollback_batches: u64,
    pub range_index_capacity: u32,
    pub range_index_entries: u32,
    pub range_index_tombstones: u32,
    pub range_index_lookups: u64,
    pub range_index_hits: u64,
    pub range_index_misses: u64,
    pub range_index_probe_total: u64,
    pub range_index_probe_max: u32,
    pub range_index_probe_last: u32,
    pub range_index_rebuilds: u64,
    pub range_index_insert_failures: u64,
    pub range_free_slot_lookups: u64,
    pub range_free_slot_probe_total: u64,
    pub range_free_slot_probe_max: u32,
    pub range_free_slot_probe_last: u32,
    pub range_address_entries: u32,
    pub range_address_lookups: u64,
    pub range_address_probe_total: u64,
    pub range_address_probe_max: u32,
    pub range_address_probe_last: u32,
    pub commit_span_active: u32,
    pub commit_span_lookups: u64,
    pub commit_span_steps: u64,
    pub commit_span_step_max: u32,
    pub page_state_span_active: u32,
    pub page_state_span_lookups: u64,
    pub page_state_span_steps: u64,
    pub page_state_span_step_max: u32,
    pub reclaim_cursor_range_steps: u64,
    pub reclaim_cursor_span_steps: u64,
    pub reclaim_cursor_page_steps: u64,
    pub reclaim_cursor_wraps: u64,
}
impl HotPathStats {
    pub const INIT: Self = Self {
        resident_commit_batches: 0,
        resident_commit_max_batch_pages: 0,
        resident_commit_unlocked_zero_bytes: 0,
        resident_rollback_batches: 0,
        range_index_capacity: 2048,
        range_index_entries: 0,
        range_index_tombstones: 0,
        range_index_lookups: 0,
        range_index_hits: 0,
        range_index_misses: 0,
        range_index_probe_total: 0,
        range_index_probe_max: 0,
        range_index_probe_last: 0,
        range_index_rebuilds: 0,
        range_index_insert_failures: 0,
        range_free_slot_lookups: 0,
        range_free_slot_probe_total: 0,
        range_free_slot_probe_max: 0,
        range_free_slot_probe_last: 0,
        range_address_entries: 0,
        range_address_lookups: 0,
        range_address_probe_total: 0,
        range_address_probe_max: 0,
        range_address_probe_last: 0,
        commit_span_active: 0,
        commit_span_lookups: 0,
        commit_span_steps: 0,
        commit_span_step_max: 0,
        page_state_span_active: 0,
        page_state_span_lookups: 0,
        page_state_span_steps: 0,
        page_state_span_step_max: 0,
        reclaim_cursor_range_steps: 0,
        reclaim_cursor_span_steps: 0,
        reclaim_cursor_page_steps: 0,
        reclaim_cursor_wraps: 0,
    };
}
impl Default for HotPathStats {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageStateInput {
    pub operation: u32,
    pub region_id: u32,
    pub region_offset: u64,
    pub page_count: u64,
    pub slot_reservation_id: u32,
    pub slot_index: u64,
    pub slot_generation: u64,
    pub flags: u32,
}
impl PageStateInput {
    pub const INIT: Self = Self {
        operation: page_state_operation_query,
        region_id: 0,
        region_offset: 0,
        page_count: 1,
        slot_reservation_id: 0,
        slot_index: 0,
        slot_generation: 0,
        flags: 0,
    };
}
impl Default for PageStateInput {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageStateResult {
    pub version: u32,
    pub status: u32,
    pub operation: u32,
    pub flags: u32,
    pub blockers: u32,
    pub region_id: u32,
    pub page_size: u32,
    pub max_spans: u32,
    pub span_count: u32,
    pub region_offset: u64,
    pub page_count: u64,
    pub committed_pages: u64,
    pub resident_pages: u64,
    pub nonresident_pages: u64,
    pub dirty_pages: u64,
    pub clean_pages: u64,
    pub pinned_pages: u64,
    pub busy_pages: u64,
    pub error_pages: u64,
    pub slot_bound_pages: u64,
    pub slot_reservation_id: u32,
    pub page_count_lo: u32,
    pub slot_index: u64,
    pub slot_generation: u64,
    pub total_transitions: u64,
    pub dirty_marks: u64,
    pub clean_marks: u64,
    pub slot_binds: u64,
    pub slot_clears: u64,
    pub pinned_marks: u64,
    pub pinned_clears: u64,
    pub busy_marks: u64,
    pub busy_clears: u64,
    pub error_marks: u64,
    pub error_clears: u64,
    pub table_full_failures: u64,
    pub cleanup_pages: u64,
}
impl PageStateResult {
    pub const INIT: Self = Self {
        version: page_state_probe_version,
        status: page_state_status_unavailable,
        operation: page_state_operation_query,
        flags: 0,
        blockers: 0,
        region_id: 0,
        page_size: 4096,
        max_spans: MAX_PAGE_STATE_SPANS as u32,
        span_count: 0,
        region_offset: 0,
        page_count: 0,
        committed_pages: 0,
        resident_pages: 0,
        nonresident_pages: 0,
        dirty_pages: 0,
        clean_pages: 0,
        pinned_pages: 0,
        busy_pages: 0,
        error_pages: 0,
        slot_bound_pages: 0,
        slot_reservation_id: 0,
        page_count_lo: 0,
        slot_index: 0,
        slot_generation: 0,
        total_transitions: 0,
        dirty_marks: 0,
        clean_marks: 0,
        slot_binds: 0,
        slot_clears: 0,
        pinned_marks: 0,
        pinned_clears: 0,
        busy_marks: 0,
        busy_clears: 0,
        error_marks: 0,
        error_clears: 0,
        table_full_failures: 0,
        cleanup_pages: 0,
    };
}
impl Default for PageStateResult {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageStateSummary {
    pub enabled: bool,
    pub last_status: u32,
    pub last_operation: u32,
    pub last_flags: u32,
    pub last_blockers: u32,
    pub last_region_id: u32,
    pub last_region_offset: u64,
    pub last_page_count: u64,
    pub last_slot_reservation_id: u32,
    pub last_slot_index: u64,
    pub last_slot_generation: u64,
    pub committed_pages: u64,
    pub resident_pages: u64,
    pub nonresident_pages: u64,
    pub dirty_pages: u64,
    pub clean_pages: u64,
    pub pinned_pages: u64,
    pub busy_pages: u64,
    pub error_pages: u64,
    pub fault_page_ins: u64,
    pub fault_page_in_failures: u64,
    pub page_out_nonresident_pages: u64,
    pub eviction_attempts: u64,
    pub eviction_successes: u64,
    pub eviction_failures: u64,
    pub eviction_candidates: u64,
    pub eviction_page_outs: u64,
    pub eviction_clean_pages: u64,
    pub eviction_dirty_pages: u64,
    pub eviction_returned_frames: u64,
    pub eviction_no_backing: u64,
    pub eviction_no_candidate: u64,
    pub eviction_slot_failures: u64,
    pub eviction_io_failures: u64,
    pub eviction_skipped_nonresident: u64,
    pub eviction_skipped_pinned: u64,
    pub eviction_skipped_busy: u64,
    pub eviction_redirtied_pages: u64,
    pub eviction_skipped_error: u64,
    pub eviction_skipped_unmapped: u64,
    pub pager_failed_page_outs: u64,
    pub pager_failed_page_ins: u64,
    pub pager_data_preserved_pages: u64,
    pub pager_data_lost_pages: u64,
    pub pager_dirty_preserved_pages: u64,
    pub pager_disabled_eviction_gates: u64,
    pub slot_bound_pages: u64,
    pub span_count: u32,
    pub max_spans: u32,
    pub transitions: u64,
    pub dirty_marks: u64,
    pub clean_marks: u64,
    pub slot_binds: u64,
    pub slot_clears: u64,
    pub pinned_marks: u64,
    pub pinned_clears: u64,
    pub busy_marks: u64,
    pub busy_clears: u64,
    pub error_marks: u64,
    pub error_clears: u64,
    pub table_full_failures: u64,
    pub cleanup_pages: u64,
}
impl PageStateSummary {
    pub const INIT: Self = Self {
        enabled: true,
        last_status: page_state_status_unavailable,
        last_operation: page_state_operation_query,
        last_flags: 0,
        last_blockers: 0,
        last_region_id: 0,
        last_region_offset: 0,
        last_page_count: 0,
        last_slot_reservation_id: 0,
        last_slot_index: 0,
        last_slot_generation: 0,
        committed_pages: 0,
        resident_pages: 0,
        nonresident_pages: 0,
        dirty_pages: 0,
        clean_pages: 0,
        pinned_pages: 0,
        busy_pages: 0,
        error_pages: 0,
        fault_page_ins: 0,
        fault_page_in_failures: 0,
        page_out_nonresident_pages: 0,
        eviction_attempts: 0,
        eviction_successes: 0,
        eviction_failures: 0,
        eviction_candidates: 0,
        eviction_page_outs: 0,
        eviction_clean_pages: 0,
        eviction_dirty_pages: 0,
        eviction_returned_frames: 0,
        eviction_no_backing: 0,
        eviction_no_candidate: 0,
        eviction_slot_failures: 0,
        eviction_io_failures: 0,
        eviction_skipped_nonresident: 0,
        eviction_skipped_pinned: 0,
        eviction_skipped_busy: 0,
        eviction_redirtied_pages: 0,
        eviction_skipped_error: 0,
        eviction_skipped_unmapped: 0,
        pager_failed_page_outs: 0,
        pager_failed_page_ins: 0,
        pager_data_preserved_pages: 0,
        pager_data_lost_pages: 0,
        pager_dirty_preserved_pages: 0,
        pager_disabled_eviction_gates: 0,
        slot_bound_pages: 0,
        span_count: 0,
        max_spans: MAX_PAGE_STATE_SPANS as u32,
        transitions: 0,
        dirty_marks: 0,
        clean_marks: 0,
        slot_binds: 0,
        slot_clears: 0,
        pinned_marks: 0,
        pinned_clears: 0,
        busy_marks: 0,
        busy_clears: 0,
        error_marks: 0,
        error_clears: 0,
        table_full_failures: 0,
        cleanup_pages: 0,
    };
}
impl Default for PageStateSummary {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FaultPageState {
    pub flags: u32,
    pub slot_reservation_id: u32,
    pub slot_index: u64,
    pub slot_generation: u64,
}
impl FaultPageState {
    pub const INIT: Self = Self {
        flags: 0,
        slot_reservation_id: 0,
        slot_index: 0,
        slot_generation: 0,
    };
}
impl Default for FaultPageState {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotBinding {
    pub reservation_id: u32,
    pub slot_index: u64,
    pub slot_generation: u64,
}
impl SlotBinding {
    pub const INIT: Self = Self {
        reservation_id: 0,
        slot_index: 0,
        slot_generation: 0,
    };
}
impl Default for SlotBinding {
    fn default() -> Self {
        Self::INIT
    }
}
#[derive(Clone, Copy)]
pub(super) struct PageStateSpan {
    pub slot_used: bool,
    pub range_id: u32,
    pub first_page: u64,
    pub page_count: u64,
    pub flags: u32,
    pub slot_reservation_id: u32,
    pub slot_index: u64,
    pub slot_generation: u64,
    pub prev: Option<u16>,
    pub next: Option<u16>,
}
impl PageStateSpan {
    pub const EMPTY: Self = Self {
        slot_used: false,
        range_id: 0,
        first_page: 0,
        page_count: 0,
        flags: 0,
        slot_reservation_id: 0,
        slot_index: 0,
        slot_generation: 0,
        prev: None,
        next: None,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageWriteback {
    pub region_id: u32,
    pub region_offset: u64,
    pub page_count: u64,
    pub unwind: Unwind,
    pub active: bool,
    pub completed: bool,
}
impl PageWriteback {
    pub const EMPTY: Self = Self {
        region_id: 0,
        region_offset: 0,
        page_count: 0,
        unwind: Unwind::EMPTY,
        active: false,
        completed: false,
    };
}
pub(super) type SplitSlots = [Option<u16>; 2];
#[derive(Clone, Copy)]
pub(super) enum EvictionOutcome {
    Skipped,
    Returned,
    Failed,
}
#[derive(Clone, Copy)]
pub(super) struct EvictionStep {
    pub outcome: EvictionOutcome,
    pub returned_frames: u32,
    pub returned_bytes: u64,
    pub page_outs: u64,
    pub dirty: bool,
}
impl EvictionStep {
    pub const EMPTY: Self = Self {
        outcome: EvictionOutcome::Skipped,
        returned_frames: 0,
        returned_bytes: 0,
        page_outs: 0,
        dirty: false,
    };
}

const _: () = {
    assert!(core::mem::size_of::<HotPathStats>() == 248);
    assert!(core::mem::align_of::<HotPathStats>() == 8);
    assert!(core::mem::offset_of!(HotPathStats, resident_commit_batches) == 0);
    assert!(core::mem::offset_of!(HotPathStats, resident_commit_max_batch_pages) == 8);
    assert!(core::mem::offset_of!(HotPathStats, resident_commit_unlocked_zero_bytes) == 16);
    assert!(core::mem::offset_of!(HotPathStats, resident_rollback_batches) == 24);
    assert!(core::mem::offset_of!(HotPathStats, range_index_capacity) == 32);
    assert!(core::mem::offset_of!(HotPathStats, range_index_entries) == 36);
    assert!(core::mem::offset_of!(HotPathStats, range_index_tombstones) == 40);
    assert!(core::mem::offset_of!(HotPathStats, range_index_lookups) == 48);
    assert!(core::mem::offset_of!(HotPathStats, range_index_hits) == 56);
    assert!(core::mem::offset_of!(HotPathStats, range_index_misses) == 64);
    assert!(core::mem::offset_of!(HotPathStats, range_index_probe_total) == 72);
    assert!(core::mem::offset_of!(HotPathStats, range_index_probe_max) == 80);
    assert!(core::mem::offset_of!(HotPathStats, range_index_probe_last) == 84);
    assert!(core::mem::offset_of!(HotPathStats, range_index_rebuilds) == 88);
    assert!(core::mem::offset_of!(HotPathStats, range_index_insert_failures) == 96);
    assert!(core::mem::offset_of!(HotPathStats, range_free_slot_lookups) == 104);
    assert!(core::mem::offset_of!(HotPathStats, range_free_slot_probe_total) == 112);
    assert!(core::mem::offset_of!(HotPathStats, range_free_slot_probe_max) == 120);
    assert!(core::mem::offset_of!(HotPathStats, range_free_slot_probe_last) == 124);
    assert!(core::mem::offset_of!(HotPathStats, range_address_entries) == 128);
    assert!(core::mem::offset_of!(HotPathStats, range_address_lookups) == 136);
    assert!(core::mem::offset_of!(HotPathStats, range_address_probe_total) == 144);
    assert!(core::mem::offset_of!(HotPathStats, range_address_probe_max) == 152);
    assert!(core::mem::offset_of!(HotPathStats, range_address_probe_last) == 156);
    assert!(core::mem::offset_of!(HotPathStats, commit_span_active) == 160);
    assert!(core::mem::offset_of!(HotPathStats, commit_span_lookups) == 168);
    assert!(core::mem::offset_of!(HotPathStats, commit_span_steps) == 176);
    assert!(core::mem::offset_of!(HotPathStats, commit_span_step_max) == 184);
    assert!(core::mem::offset_of!(HotPathStats, page_state_span_active) == 188);
    assert!(core::mem::offset_of!(HotPathStats, page_state_span_lookups) == 192);
    assert!(core::mem::offset_of!(HotPathStats, page_state_span_steps) == 200);
    assert!(core::mem::offset_of!(HotPathStats, page_state_span_step_max) == 208);
    assert!(core::mem::offset_of!(HotPathStats, reclaim_cursor_range_steps) == 216);
    assert!(core::mem::offset_of!(HotPathStats, reclaim_cursor_span_steps) == 224);
    assert!(core::mem::offset_of!(HotPathStats, reclaim_cursor_page_steps) == 232);
    assert!(core::mem::offset_of!(HotPathStats, reclaim_cursor_wraps) == 240);
    assert!(core::mem::size_of::<PageStateInput>() == 56);
    assert!(core::mem::align_of::<PageStateInput>() == 8);
    assert!(core::mem::offset_of!(PageStateInput, operation) == 0);
    assert!(core::mem::offset_of!(PageStateInput, region_id) == 4);
    assert!(core::mem::offset_of!(PageStateInput, region_offset) == 8);
    assert!(core::mem::offset_of!(PageStateInput, page_count) == 16);
    assert!(core::mem::offset_of!(PageStateInput, slot_reservation_id) == 24);
    assert!(core::mem::offset_of!(PageStateInput, slot_index) == 32);
    assert!(core::mem::offset_of!(PageStateInput, slot_generation) == 40);
    assert!(core::mem::offset_of!(PageStateInput, flags) == 48);
    assert!(core::mem::size_of::<PageStateResult>() == 256);
    assert!(core::mem::align_of::<PageStateResult>() == 8);
    assert!(core::mem::offset_of!(PageStateResult, version) == 0);
    assert!(core::mem::offset_of!(PageStateResult, status) == 4);
    assert!(core::mem::offset_of!(PageStateResult, operation) == 8);
    assert!(core::mem::offset_of!(PageStateResult, flags) == 12);
    assert!(core::mem::offset_of!(PageStateResult, blockers) == 16);
    assert!(core::mem::offset_of!(PageStateResult, region_id) == 20);
    assert!(core::mem::offset_of!(PageStateResult, page_size) == 24);
    assert!(core::mem::offset_of!(PageStateResult, max_spans) == 28);
    assert!(core::mem::offset_of!(PageStateResult, span_count) == 32);
    assert!(core::mem::offset_of!(PageStateResult, region_offset) == 40);
    assert!(core::mem::offset_of!(PageStateResult, page_count) == 48);
    assert!(core::mem::offset_of!(PageStateResult, committed_pages) == 56);
    assert!(core::mem::offset_of!(PageStateResult, resident_pages) == 64);
    assert!(core::mem::offset_of!(PageStateResult, nonresident_pages) == 72);
    assert!(core::mem::offset_of!(PageStateResult, dirty_pages) == 80);
    assert!(core::mem::offset_of!(PageStateResult, clean_pages) == 88);
    assert!(core::mem::offset_of!(PageStateResult, pinned_pages) == 96);
    assert!(core::mem::offset_of!(PageStateResult, busy_pages) == 104);
    assert!(core::mem::offset_of!(PageStateResult, error_pages) == 112);
    assert!(core::mem::offset_of!(PageStateResult, slot_bound_pages) == 120);
    assert!(core::mem::offset_of!(PageStateResult, slot_reservation_id) == 128);
    assert!(core::mem::offset_of!(PageStateResult, page_count_lo) == 132);
    assert!(core::mem::offset_of!(PageStateResult, slot_index) == 136);
    assert!(core::mem::offset_of!(PageStateResult, slot_generation) == 144);
    assert!(core::mem::offset_of!(PageStateResult, total_transitions) == 152);
    assert!(core::mem::offset_of!(PageStateResult, dirty_marks) == 160);
    assert!(core::mem::offset_of!(PageStateResult, clean_marks) == 168);
    assert!(core::mem::offset_of!(PageStateResult, slot_binds) == 176);
    assert!(core::mem::offset_of!(PageStateResult, slot_clears) == 184);
    assert!(core::mem::offset_of!(PageStateResult, pinned_marks) == 192);
    assert!(core::mem::offset_of!(PageStateResult, pinned_clears) == 200);
    assert!(core::mem::offset_of!(PageStateResult, busy_marks) == 208);
    assert!(core::mem::offset_of!(PageStateResult, busy_clears) == 216);
    assert!(core::mem::offset_of!(PageStateResult, error_marks) == 224);
    assert!(core::mem::offset_of!(PageStateResult, error_clears) == 232);
    assert!(core::mem::offset_of!(PageStateResult, table_full_failures) == 240);
    assert!(core::mem::offset_of!(PageStateResult, cleanup_pages) == 248);
    assert!(core::mem::size_of::<PageStateSummary>() == 464);
    assert!(core::mem::align_of::<PageStateSummary>() == 8);
    assert!(core::mem::offset_of!(PageStateSummary, enabled) == 0);
    assert!(core::mem::offset_of!(PageStateSummary, last_status) == 4);
    assert!(core::mem::offset_of!(PageStateSummary, last_operation) == 8);
    assert!(core::mem::offset_of!(PageStateSummary, last_flags) == 12);
    assert!(core::mem::offset_of!(PageStateSummary, last_blockers) == 16);
    assert!(core::mem::offset_of!(PageStateSummary, last_region_id) == 20);
    assert!(core::mem::offset_of!(PageStateSummary, last_region_offset) == 24);
    assert!(core::mem::offset_of!(PageStateSummary, last_page_count) == 32);
    assert!(core::mem::offset_of!(PageStateSummary, last_slot_reservation_id) == 40);
    assert!(core::mem::offset_of!(PageStateSummary, last_slot_index) == 48);
    assert!(core::mem::offset_of!(PageStateSummary, last_slot_generation) == 56);
    assert!(core::mem::offset_of!(PageStateSummary, committed_pages) == 64);
    assert!(core::mem::offset_of!(PageStateSummary, resident_pages) == 72);
    assert!(core::mem::offset_of!(PageStateSummary, nonresident_pages) == 80);
    assert!(core::mem::offset_of!(PageStateSummary, dirty_pages) == 88);
    assert!(core::mem::offset_of!(PageStateSummary, clean_pages) == 96);
    assert!(core::mem::offset_of!(PageStateSummary, pinned_pages) == 104);
    assert!(core::mem::offset_of!(PageStateSummary, busy_pages) == 112);
    assert!(core::mem::offset_of!(PageStateSummary, error_pages) == 120);
    assert!(core::mem::offset_of!(PageStateSummary, fault_page_ins) == 128);
    assert!(core::mem::offset_of!(PageStateSummary, fault_page_in_failures) == 136);
    assert!(core::mem::offset_of!(PageStateSummary, page_out_nonresident_pages) == 144);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_attempts) == 152);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_successes) == 160);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_failures) == 168);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_candidates) == 176);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_page_outs) == 184);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_clean_pages) == 192);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_dirty_pages) == 200);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_returned_frames) == 208);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_no_backing) == 216);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_no_candidate) == 224);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_slot_failures) == 232);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_io_failures) == 240);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_skipped_nonresident) == 248);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_skipped_pinned) == 256);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_skipped_busy) == 264);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_redirtied_pages) == 272);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_skipped_error) == 280);
    assert!(core::mem::offset_of!(PageStateSummary, eviction_skipped_unmapped) == 288);
    assert!(core::mem::offset_of!(PageStateSummary, pager_failed_page_outs) == 296);
    assert!(core::mem::offset_of!(PageStateSummary, pager_failed_page_ins) == 304);
    assert!(core::mem::offset_of!(PageStateSummary, pager_data_preserved_pages) == 312);
    assert!(core::mem::offset_of!(PageStateSummary, pager_data_lost_pages) == 320);
    assert!(core::mem::offset_of!(PageStateSummary, pager_dirty_preserved_pages) == 328);
    assert!(core::mem::offset_of!(PageStateSummary, pager_disabled_eviction_gates) == 336);
    assert!(core::mem::offset_of!(PageStateSummary, slot_bound_pages) == 344);
    assert!(core::mem::offset_of!(PageStateSummary, span_count) == 352);
    assert!(core::mem::offset_of!(PageStateSummary, max_spans) == 356);
    assert!(core::mem::offset_of!(PageStateSummary, transitions) == 360);
    assert!(core::mem::offset_of!(PageStateSummary, dirty_marks) == 368);
    assert!(core::mem::offset_of!(PageStateSummary, clean_marks) == 376);
    assert!(core::mem::offset_of!(PageStateSummary, slot_binds) == 384);
    assert!(core::mem::offset_of!(PageStateSummary, slot_clears) == 392);
    assert!(core::mem::offset_of!(PageStateSummary, pinned_marks) == 400);
    assert!(core::mem::offset_of!(PageStateSummary, pinned_clears) == 408);
    assert!(core::mem::offset_of!(PageStateSummary, busy_marks) == 416);
    assert!(core::mem::offset_of!(PageStateSummary, busy_clears) == 424);
    assert!(core::mem::offset_of!(PageStateSummary, error_marks) == 432);
    assert!(core::mem::offset_of!(PageStateSummary, error_clears) == 440);
    assert!(core::mem::offset_of!(PageStateSummary, table_full_failures) == 448);
    assert!(core::mem::offset_of!(PageStateSummary, cleanup_pages) == 456);
    assert!(core::mem::size_of::<FaultPageState>() == 24);
    assert!(core::mem::align_of::<FaultPageState>() == 8);
    assert!(core::mem::offset_of!(FaultPageState, flags) == 0);
    assert!(core::mem::offset_of!(FaultPageState, slot_reservation_id) == 4);
    assert!(core::mem::offset_of!(FaultPageState, slot_index) == 8);
    assert!(core::mem::offset_of!(FaultPageState, slot_generation) == 16);
    assert!(core::mem::size_of::<SlotBinding>() == 24);
    assert!(core::mem::align_of::<SlotBinding>() == 8);
    assert!(core::mem::offset_of!(SlotBinding, reservation_id) == 0);
    assert!(core::mem::offset_of!(SlotBinding, slot_index) == 8);
    assert!(core::mem::offset_of!(SlotBinding, slot_generation) == 16);
    assert!(core::mem::size_of::<PageWriteback>() == 48);
    assert!(core::mem::offset_of!(PageWriteback, unwind) == 24);
    assert!(core::mem::offset_of!(PageWriteback, active) == 40);
};
