const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const PAGE_BYTES: u64 = 4096;
pub const SLOT_BYTES: u64 = PAGE_BYTES;
pub const MAX_BACKING_PATH: usize = 128;
pub const SLOT_TABLE_MAX_RANGES: usize = 256;
pub const slot_table_max_ranges: u32 = 256;

pub const status_unavailable: u32 = 0;
pub const status_ready: u32 = 1;
pub const status_invalid_request: u32 = 2;
pub const status_missing_file: u32 = 3;
pub const status_directory: u32 = 4;
pub const status_too_small: u32 = 5;
pub const status_unsupported_fs: u32 = 6;
pub const status_unsupported_flags: u32 = 7;

pub const flag_file_backed: u32 = 1 << 0;
pub const flag_existing_file: u32 = 1 << 1;
pub const flag_fat32: u32 = 1 << 2;
pub const flag_reserve_only: u32 = 1 << 3;
pub const flag_pager_disabled: u32 = 1 << 4;
pub const flag_uses_fs_api: u32 = 1 << 5;
pub const flag_no_second_io_path: u32 = 1 << 6;
pub const flag_page_aligned_request: u32 = 1 << 7;

pub const blocker_invalid_request: u32 = 1 << 0;
pub const blocker_unsupported_flags: u32 = 1 << 1;
pub const blocker_missing_file: u32 = 1 << 2;
pub const blocker_directory: u32 = 1 << 3;
pub const blocker_too_small: u32 = 1 << 4;
pub const blocker_unsupported_fs: u32 = 1 << 5;
pub const blocker_unaligned_request: u32 = 1 << 6;

pub const supported_probe_flags: u32 = 0;

pub const slot_operation_probe: u32 = 0;
pub const slot_operation_reserve: u32 = 1;
pub const slot_operation_release: u32 = 2;
pub const slot_operation_mark_error: u32 = 3;
pub const slot_operation_recover: u32 = 4;

pub const slot_owner_kind_diagnostic: u32 = 1;
pub const slot_owner_kind_r4x_instance: u32 = 2;
pub const slot_owner_kind_vm_region: u32 = 3;
pub const slot_owner_kind_pager: u32 = 4;

pub const slot_status_unavailable: u32 = 0;
pub const slot_status_ready: u32 = 1;
pub const slot_status_reserved: u32 = 2;
pub const slot_status_released: u32 = 3;
pub const slot_status_error_marked: u32 = 4;
pub const slot_status_recovered: u32 = 5;
pub const slot_status_invalid_request: u32 = 6;
pub const slot_status_backing_unavailable: u32 = 7;
pub const slot_status_insufficient_capacity: u32 = 8;
pub const slot_status_table_full: u32 = 9;
pub const slot_status_reservation_not_found: u32 = 10;
pub const slot_status_unsupported_flags: u32 = 11;
pub const slot_status_unsupported_operation: u32 = 12;
pub const slot_status_owner_mismatch: u32 = 13;

pub const slot_flag_file_backed: u32 = 1 << 0;
pub const slot_flag_backing_ready: u32 = 1 << 1;
pub const slot_flag_metadata_only: u32 = 1 << 2;
pub const slot_flag_range_table: u32 = 1 << 3;
pub const slot_flag_page_sized_slots: u32 = 1 << 4;
pub const slot_flag_pager_disabled: u32 = 1 << 5;
pub const slot_flag_eviction_disabled: u32 = 1 << 6;
pub const slot_flag_no_page_io: u32 = 1 << 7;
pub const slot_flag_recovery_available: u32 = 1 << 8;

pub const slot_blocker_invalid_request: u32 = 1 << 0;
pub const slot_blocker_unsupported_flags: u32 = 1 << 1;
pub const slot_blocker_unsupported_operation: u32 = 1 << 2;
pub const slot_blocker_backing_not_ready: u32 = 1 << 3;
pub const slot_blocker_zero_capacity: u32 = 1 << 4;
pub const slot_blocker_insufficient_capacity: u32 = 1 << 5;
pub const slot_blocker_table_full: u32 = 1 << 6;
pub const slot_blocker_reservation_not_found: u32 = 1 << 7;
pub const slot_blocker_unaligned_backing: u32 = 1 << 8;
pub const slot_blocker_owner_mismatch: u32 = 1 << 9;
pub const slot_blocker_invalid_owner: u32 = 1 << 10;

pub const supported_slot_flags: u32 = 0;

pub const pager_gate_status_unavailable: u32 = 0;
pub const pager_gate_status_ready: u32 = 1;
pub const pager_gate_status_invalid_request: u32 = 2;
pub const pager_gate_status_backing_unavailable: u32 = 3;
pub const pager_gate_status_vm_region_missing: u32 = 4;
pub const pager_gate_status_no_nonresident_commit: u32 = 5;
pub const pager_gate_status_insufficient_capacity: u32 = 6;
pub const pager_gate_status_rollback_failed: u32 = 7;
pub const pager_gate_status_unsupported_flags: u32 = 8;
pub const pager_gate_status_table_full: u32 = 9;

pub const pager_gate_flag_file_backed: u32 = 1 << 0;
pub const pager_gate_flag_backing_ready: u32 = 1 << 1;
pub const pager_gate_flag_metadata_only: u32 = 1 << 2;
pub const pager_gate_flag_vm_region_attached: u32 = 1 << 3;
pub const pager_gate_flag_commit_gate: u32 = 1 << 4;
pub const pager_gate_flag_fault_gate: u32 = 1 << 5;
pub const pager_gate_flag_slot_reservation_tested: u32 = 1 << 6;
pub const pager_gate_flag_rollback_complete: u32 = 1 << 7;
pub const pager_gate_flag_pager_disabled: u32 = 1 << 8;
pub const pager_gate_flag_eviction_disabled: u32 = 1 << 9;
pub const pager_gate_flag_no_page_io: u32 = 1 << 10;
pub const pager_gate_flag_no_swap: u32 = 1 << 11;
pub const pager_gate_flag_no_second_io_path: u32 = 1 << 12;
pub const pager_gate_flag_page_sized_slots: u32 = 1 << 13;

pub const pager_gate_blocker_invalid_request: u32 = 1 << 0;
pub const pager_gate_blocker_unsupported_flags: u32 = 1 << 1;
pub const pager_gate_blocker_backing_not_ready: u32 = 1 << 2;
pub const pager_gate_blocker_vm_region_missing: u32 = 1 << 3;
pub const pager_gate_blocker_vm_region_not_r4x: u32 = 1 << 4;
pub const pager_gate_blocker_no_nonresident_commit: u32 = 1 << 5;
pub const pager_gate_blocker_unaligned_request: u32 = 1 << 6;
pub const pager_gate_blocker_insufficient_capacity: u32 = 1 << 7;
pub const pager_gate_blocker_table_full: u32 = 1 << 8;
pub const pager_gate_blocker_rollback_failed: u32 = 1 << 9;

pub const supported_pager_gate_flags: u32 = 0;

pub const page_io_operation_page_out: u32 = 1;
pub const page_io_operation_page_in: u32 = 2;

pub const page_io_status_unavailable: u32 = 0;
pub const page_io_status_ready: u32 = 1;
pub const page_io_status_page_out_ok: u32 = 2;
pub const page_io_status_page_in_ok: u32 = 3;
pub const page_io_status_invalid_request: u32 = 4;
pub const page_io_status_backing_unavailable: u32 = 5;
pub const page_io_status_vm_region_missing: u32 = 6;
pub const page_io_status_reservation_not_found: u32 = 7;
pub const page_io_status_slot_not_valid: u32 = 8;
pub const page_io_status_io_failed: u32 = 9;
pub const page_io_status_partial_io: u32 = 10;
pub const page_io_status_unsupported_flags: u32 = 11;
pub const page_io_status_slot_error: u32 = 12;
pub const page_io_status_owner_mismatch: u32 = 13;
pub const page_io_status_stale_generation: u32 = 14;
pub const page_io_status_slot_already_valid: u32 = 15;

pub const page_io_flag_file_backed: u32 = 1 << 0;
pub const page_io_flag_backing_ready: u32 = 1 << 1;
pub const page_io_flag_vm_region_attached: u32 = 1 << 2;
pub const page_io_flag_slot_reserved: u32 = 1 << 3;
pub const page_io_flag_slot_valid: u32 = 1 << 4;
pub const page_io_flag_slot_dirty: u32 = 1 << 5;
pub const page_io_flag_slot_clean: u32 = 1 << 6;
pub const page_io_flag_explicit_request: u32 = 1 << 7;
pub const page_io_flag_uses_fs_api: u32 = 1 << 8;
pub const page_io_flag_no_second_io_path: u32 = 1 << 9;
pub const page_io_flag_pager_disabled: u32 = 1 << 10;
pub const page_io_flag_eviction_disabled: u32 = 1 << 11;
pub const page_io_flag_no_swap: u32 = 1 << 12;
pub const page_io_flag_page_sized_slots: u32 = 1 << 13;
pub const page_io_flag_page_out: u32 = 1 << 14;
pub const page_io_flag_page_in: u32 = 1 << 15;
pub const page_io_flag_owner_matched: u32 = 1 << 16;
pub const page_io_flag_generation_checked: u32 = 1 << 17;
pub const page_io_flag_multi_page: u32 = 1 << 18;
pub const page_io_flag_eviction_request: u32 = 1 << 19;
pub const page_io_flag_retry_request: u32 = 1 << 20;
pub const page_io_flag_retryable_failure: u32 = 1 << 21;
pub const page_io_flag_permanent_failure: u32 = 1 << 22;
pub const page_io_flag_data_preserved: u32 = 1 << 23;

pub const page_io_blocker_invalid_request: u32 = 1 << 0;
pub const page_io_blocker_unsupported_flags: u32 = 1 << 1;
pub const page_io_blocker_backing_not_ready: u32 = 1 << 2;
pub const page_io_blocker_vm_region_missing: u32 = 1 << 3;
pub const page_io_blocker_vm_region_not_r4x: u32 = 1 << 4;
pub const page_io_blocker_unaligned_region_offset: u32 = 1 << 5;
pub const page_io_blocker_region_offset_outside_commit: u32 = 1 << 6;
pub const page_io_blocker_reservation_not_found: u32 = 1 << 7;
pub const page_io_blocker_slot_index_out_of_range: u32 = 1 << 8;
pub const page_io_blocker_slot_not_valid: u32 = 1 << 9;
pub const page_io_blocker_io_failed: u32 = 1 << 10;
pub const page_io_blocker_partial_io: u32 = 1 << 11;
pub const page_io_blocker_slot_error: u32 = 1 << 12;
pub const page_io_blocker_owner_mismatch: u32 = 1 << 13;
pub const page_io_blocker_stale_generation: u32 = 1 << 14;
pub const page_io_blocker_slot_already_valid: u32 = 1 << 15;
pub const page_io_blocker_invalid_owner: u32 = 1 << 16;

pub const supported_page_io_flags: u32 = page_io_flag_eviction_request | page_io_flag_retry_request;
pub const page_io_policy_retry_limit: u32 = 1;
pub const page_io_policy_backoff_ticks: u32 = 0;

const extent_state_reserved: u32 = 1 << 0;
const extent_state_valid: u32 = 1 << 1;
const extent_state_dirty: u32 = 1 << 2;
const extent_state_error: u32 = 1 << 3;

pub const Input = struct {
    requested_bytes: u64 = 0,
    flags: u32 = 0,
    path: ?[*:0]const u8 = null,
    file_exists: bool = false,
    is_dir: bool = false,
    fat32: bool = false,
    file_size: u64 = 0,
    cluster_bytes: u32 = 0,
    first_cluster: u32 = 0,
};

pub const Result = struct {
    path: [MAX_BACKING_PATH:0]u8 = .{0} ** MAX_BACKING_PATH,
    status: u32 = status_unavailable,
    flags: u32 = flag_reserve_only | flag_pager_disabled | flag_uses_fs_api | flag_no_second_io_path,
    blockers: u32 = 0,
    requested_bytes: u64 = 0,
    available_bytes: u64 = 0,
    file_size: u64 = 0,
    cluster_bytes: u32 = 0,
    first_cluster: u32 = 0,
    pager_enabled: u32 = 0,
    anonymous_paging_enabled: u32 = 0,
};

pub const Summary = struct {
    enabled: bool = true,
    probes: u64 = 0,
    ready: u64 = 0,
    failures: u64 = 0,
    last_status: u32 = status_unavailable,
    last_flags: u32 = 0,
    last_blockers: u32 = 0,
    last_requested_bytes: u64 = 0,
    last_available_bytes: u64 = 0,
    last_file_size: u64 = 0,
    last_cluster_bytes: u32 = 0,
    last_first_cluster: u32 = 0,
};

pub const SlotInput = struct {
    operation: u32 = slot_operation_probe,
    requested_slots: u64 = 0,
    reservation_id: u32 = 0,
    owner_kind: u32 = slot_owner_kind_diagnostic,
    owner_id: u32 = 0,
    region_id: u32 = 0,
    flags: u32 = 0,
    backing: Result = .{},
};

pub const SlotResult = struct {
    version: u32 = 1,
    status: u32 = slot_status_unavailable,
    operation: u32 = slot_operation_probe,
    flags: u32 = slot_flag_metadata_only |
        slot_flag_range_table |
        slot_flag_page_sized_slots |
        slot_flag_pager_disabled |
        slot_flag_recovery_available,
    blockers: u32 = 0,
    slot_bytes: u32 = 4096,
    capacity_slots: u64 = 0,
    requested_slots: u64 = 0,
    reserved_slots: u64 = 0,
    free_slots: u64 = 0,
    valid_slots: u64 = 0,
    dirty_slots: u64 = 0,
    error_slots: u64 = 0,
    range_count: u32 = 0,
    max_ranges: u32 = slot_table_max_ranges,
    reservation_id: u32 = 0,
    owner_kind: u32 = slot_owner_kind_diagnostic,
    owner_id: u32 = 0,
    region_id: u32 = 0,
    first_slot: u64 = 0,
    slot_count: u64 = 0,
    generation: u64 = 0,
    pager_enabled: u32 = 0,
    eviction_enabled: u32 = 1,
    page_in_enabled: u32 = 1,
    page_out_enabled: u32 = 1,
    total_probes: u64 = 0,
    total_reserves: u64 = 0,
    total_releases: u64 = 0,
    total_error_marks: u64 = 0,
    total_recoveries: u64 = 0,
    total_failures: u64 = 0,
};

pub const SlotSummary = struct {
    enabled: bool = true,
    last_status: u32 = slot_status_unavailable,
    last_operation: u32 = slot_operation_probe,
    last_flags: u32 = 0,
    last_blockers: u32 = 0,
    slot_bytes: u32 = 4096,
    capacity_slots: u64 = 0,
    reserved_slots: u64 = 0,
    free_slots: u64 = 0,
    valid_slots: u64 = 0,
    dirty_slots: u64 = 0,
    error_slots: u64 = 0,
    range_count: u32 = 0,
    max_ranges: u32 = slot_table_max_ranges,
    last_reservation_id: u32 = 0,
    last_owner_kind: u32 = slot_owner_kind_diagnostic,
    last_owner_id: u32 = 0,
    last_region_id: u32 = 0,
    last_first_slot: u64 = 0,
    last_slot_count: u64 = 0,
    generation: u64 = 0,
    probes: u64 = 0,
    reserves: u64 = 0,
    releases: u64 = 0,
    error_marks: u64 = 0,
    recoveries: u64 = 0,
    failures: u64 = 0,
    lifecycle_cleanups: u64 = 0,
    lifecycle_released_ranges: u64 = 0,
    lifecycle_released_slots: u64 = 0,
};

pub const PagerGateInput = struct {
    requested_bytes: u64 = 0,
    region_id: u32 = 0,
    owner_id: u32 = 0,
    flags: u32 = 0,
    vm_region_exists: bool = false,
    vm_region_is_r4x: bool = false,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    backing: Result = .{},
};

pub const PagerGateResult = struct {
    version: u32 = 1,
    status: u32 = pager_gate_status_unavailable,
    flags: u32 = pager_gate_flag_metadata_only |
        pager_gate_flag_commit_gate |
        pager_gate_flag_fault_gate |
        pager_gate_flag_pager_disabled |
        pager_gate_flag_no_page_io |
        pager_gate_flag_no_swap |
        pager_gate_flag_no_second_io_path |
        pager_gate_flag_page_sized_slots,
    blockers: u32 = 0,
    region_id: u32 = 0,
    owner_id: u32 = 0,
    slot_bytes: u32 = 4096,
    reserved0: u32 = 0,
    requested_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    nonresident_bytes: u64 = 0,
    requested_slots: u64 = 0,
    prepared_slots: u64 = 0,
    capacity_slots: u64 = 0,
    free_before_slots: u64 = 0,
    free_after_slots: u64 = 0,
    reserved_before_slots: u64 = 0,
    reserved_after_slots: u64 = 0,
    slot_reservation_id: u32 = 0,
    rollback_completed: u32 = 0,
    commit_gate_enabled: u32 = 1,
    fault_gate_enabled: u32 = 1,
    pager_enabled: u32 = 0,
    eviction_enabled: u32 = 1,
    page_in_enabled: u32 = 0,
    page_out_enabled: u32 = 0,
    slot_generation: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    total_probes: u64 = 0,
    total_ready: u64 = 0,
    total_rollbacks: u64 = 0,
    total_failures: u64 = 0,
};

pub const PagerGateSummary = struct {
    enabled: bool = true,
    last_status: u32 = pager_gate_status_unavailable,
    last_flags: u32 = 0,
    last_blockers: u32 = 0,
    last_region_id: u32 = 0,
    last_owner_id: u32 = 0,
    requested_bytes: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    nonresident_bytes: u64 = 0,
    requested_slots: u64 = 0,
    prepared_slots: u64 = 0,
    capacity_slots: u64 = 0,
    free_before_slots: u64 = 0,
    free_after_slots: u64 = 0,
    reserved_before_slots: u64 = 0,
    reserved_after_slots: u64 = 0,
    last_reservation_id: u32 = 0,
    rollback_completed: u32 = 0,
    commit_gate_enabled: u32 = 1,
    fault_gate_enabled: u32 = 1,
    pager_enabled: u32 = 0,
    eviction_enabled: u32 = 1,
    page_in_enabled: u32 = 0,
    page_out_enabled: u32 = 0,
    slot_generation: u64 = 0,
    fault_count: u64 = 0,
    failed_faults: u64 = 0,
    probes: u64 = 0,
    ready: u64 = 0,
    rollbacks: u64 = 0,
    failures: u64 = 0,
};

pub const PageIoInput = struct {
    operation: u32 = page_io_operation_page_out,
    region_id: u32 = 0,
    region_offset: u64 = 0,
    reservation_id: u32 = 0,
    slot_index: u64 = 0,
    page_count: u64 = 1,
    owner_kind: u32 = slot_owner_kind_diagnostic,
    owner_id: u32 = 0,
    expected_generation: u64 = 0,
    flags: u32 = 0,
    vm_region_exists: bool = false,
    vm_region_is_r4x: bool = false,
    // Verified by the VM owner for this exact offset and transfer, not inferred
    // from the sum of all committed spans.
    commit_covered: bool = false,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    backing: Result = .{},
    io_status: i32 = 0,
    io_bytes: u32 = 0,
};

pub const PageIoResult = struct {
    version: u32 = 1,
    status: u32 = page_io_status_unavailable,
    operation: u32 = page_io_operation_page_out,
    flags: u32 = page_io_flag_explicit_request |
        page_io_flag_uses_fs_api |
        page_io_flag_no_second_io_path |
        page_io_flag_pager_disabled |
        page_io_flag_no_swap |
        page_io_flag_page_sized_slots,
    blockers: u32 = 0,
    region_id: u32 = 0,
    reservation_id: u32 = 0,
    owner_kind: u32 = slot_owner_kind_diagnostic,
    owner_id: u32 = 0,
    slot_bytes: u32 = 4096,
    reserved0: u32 = 0,
    region_offset: u64 = 0,
    committed_bytes: u64 = 0,
    resident_bytes: u64 = 0,
    slot_index: u64 = 0,
    page_count: u64 = 1,
    transfer_bytes: u64 = 4096,
    expected_generation: u64 = 0,
    backing_slot: u64 = 0,
    backing_offset: u64 = 0,
    capacity_slots: u64 = 0,
    reserved_slots: u64 = 0,
    valid_slots: u64 = 0,
    dirty_slots: u64 = 0,
    error_slots: u64 = 0,
    io_bytes: u32 = 0,
    io_status: i32 = 0,
    pager_enabled: u32 = 0,
    eviction_enabled: u32 = 1,
    page_in_enabled: u32 = 1,
    page_out_enabled: u32 = 1,
    retry_limit: u32 = page_io_policy_retry_limit,
    backoff_ticks: u32 = page_io_policy_backoff_ticks,
    slot_generation: u64 = 0,
    total_prepares: u64 = 0,
    total_page_outs: u64 = 0,
    total_page_ins: u64 = 0,
    total_failures: u64 = 0,
    total_retry_attempts: u64 = 0,
    total_retryable_failures: u64 = 0,
    total_permanent_failures: u64 = 0,
    total_retry_limit_hits: u64 = 0,
    total_failed_page_outs: u64 = 0,
    total_failed_page_ins: u64 = 0,
    total_data_preserved_pages: u64 = 0,
    total_data_lost_pages: u64 = 0,
};

pub const PageIoSummary = struct {
    enabled: bool = true,
    last_status: u32 = page_io_status_unavailable,
    last_operation: u32 = 0,
    last_flags: u32 = 0,
    last_blockers: u32 = 0,
    last_region_id: u32 = 0,
    last_reservation_id: u32 = 0,
    last_owner_kind: u32 = slot_owner_kind_diagnostic,
    last_owner_id: u32 = 0,
    region_offset: u64 = 0,
    page_count: u64 = 1,
    transfer_bytes: u64 = 4096,
    expected_generation: u64 = 0,
    backing_slot: u64 = 0,
    backing_offset: u64 = 0,
    io_bytes: u32 = 0,
    io_status: i32 = 0,
    capacity_slots: u64 = 0,
    reserved_slots: u64 = 0,
    valid_slots: u64 = 0,
    dirty_slots: u64 = 0,
    error_slots: u64 = 0,
    pager_enabled: u32 = 0,
    eviction_enabled: u32 = 1,
    page_in_enabled: u32 = 1,
    page_out_enabled: u32 = 1,
    retry_limit: u32 = page_io_policy_retry_limit,
    backoff_ticks: u32 = page_io_policy_backoff_ticks,
    slot_generation: u64 = 0,
    prepares: u64 = 0,
    page_outs: u64 = 0,
    page_ins: u64 = 0,
    failures: u64 = 0,
    retry_attempts: u64 = 0,
    retryable_failures: u64 = 0,
    permanent_failures: u64 = 0,
    retry_limit_hits: u64 = 0,
    failed_page_outs: u64 = 0,
    failed_page_ins: u64 = 0,
    data_preserved_pages: u64 = 0,
    data_lost_pages: u64 = 0,
};


pub const LifecycleCleanupResult = struct {
    cleanup_count: u64 = 0,
    released_ranges: u64 = 0,
    released_slots: u64 = 0,
    released_valid_slots: u64 = 0,
    released_dirty_slots: u64 = 0,
    released_error_slots: u64 = 0,
};

// One native metadata owner. The existing VM lock also admits passive
// snapshots and lifecycle cleanup; no backing lock survives file I/O.
const AdmissionToken = extern struct {
    flags: u64 = 0,
    cpu_index: u32 = 0,
    previous_rank: u8 = 0,
    outermost: bool = false,
};
extern fn r4os_rust_backing_acquire(*AdmissionToken) callconv(.c) void;
extern fn r4os_rust_backing_release(*const AdmissionToken) callconv(.c) void;
fn enter() AdmissionToken {
    if (@import("builtin").is_test) return .{};
    var token: AdmissionToken = undefined;
    r4os_rust_backing_acquire(&token);
    return token;
}
fn leave(token: AdmissionToken) void {
    if (!@import("builtin").is_test) r4os_rust_backing_release(&token);
}
comptime {
    if (@sizeOf(AdmissionToken) != 16 or @alignOf(AdmissionToken) != 8 or
        @offsetOf(AdmissionToken, "cpu_index") != 8 or @offsetOf(AdmissionToken, "previous_rank") != 12 or
        @offsetOf(AdmissionToken, "outermost") != 13) @compileError("Native existing VM owner token geometry");
}

const RawInput = extern struct {
    requested_bytes: u64,
    flags: u32,
    path: ?[*:0]const u8,
    file_exists: bool,
    is_dir: bool,
    fat32: bool,
    file_size: u64,
    cluster_bytes: u32,
    first_cluster: u32,
};
fn rawInput(value: Input) RawInput {
    return .{
        .requested_bytes = value.requested_bytes,
        .flags = value.flags,
        .path = value.path,
        .file_exists = value.file_exists,
        .is_dir = value.is_dir,
        .fat32 = value.fat32,
        .file_size = value.file_size,
        .cluster_bytes = value.cluster_bytes,
        .first_cluster = value.first_cluster,
    };
}
fn fromRawInput(value: RawInput) Input {
    return .{
        .requested_bytes = value.requested_bytes,
        .flags = value.flags,
        .path = value.path,
        .file_exists = value.file_exists,
        .is_dir = value.is_dir,
        .fat32 = value.fat32,
        .file_size = value.file_size,
        .cluster_bytes = value.cluster_bytes,
        .first_cluster = value.first_cluster,
    };
}
const RawResult = extern struct {
    path: [MAX_BACKING_PATH:0]u8,
    status: u32,
    flags: u32,
    blockers: u32,
    requested_bytes: u64,
    available_bytes: u64,
    file_size: u64,
    cluster_bytes: u32,
    first_cluster: u32,
    pager_enabled: u32,
    anonymous_paging_enabled: u32,
};
fn rawResult(value: Result) RawResult {
    return .{
        .path = value.path,
        .status = value.status,
        .flags = value.flags,
        .blockers = value.blockers,
        .requested_bytes = value.requested_bytes,
        .available_bytes = value.available_bytes,
        .file_size = value.file_size,
        .cluster_bytes = value.cluster_bytes,
        .first_cluster = value.first_cluster,
        .pager_enabled = value.pager_enabled,
        .anonymous_paging_enabled = value.anonymous_paging_enabled,
    };
}
fn fromRawResult(value: RawResult) Result {
    return .{
        .path = value.path,
        .status = value.status,
        .flags = value.flags,
        .blockers = value.blockers,
        .requested_bytes = value.requested_bytes,
        .available_bytes = value.available_bytes,
        .file_size = value.file_size,
        .cluster_bytes = value.cluster_bytes,
        .first_cluster = value.first_cluster,
        .pager_enabled = value.pager_enabled,
        .anonymous_paging_enabled = value.anonymous_paging_enabled,
    };
}
const RawSummary = extern struct {
    enabled: bool,
    probes: u64,
    ready: u64,
    failures: u64,
    last_status: u32,
    last_flags: u32,
    last_blockers: u32,
    last_requested_bytes: u64,
    last_available_bytes: u64,
    last_file_size: u64,
    last_cluster_bytes: u32,
    last_first_cluster: u32,
};
fn rawSummary(value: Summary) RawSummary {
    return .{
        .enabled = value.enabled,
        .probes = value.probes,
        .ready = value.ready,
        .failures = value.failures,
        .last_status = value.last_status,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_requested_bytes = value.last_requested_bytes,
        .last_available_bytes = value.last_available_bytes,
        .last_file_size = value.last_file_size,
        .last_cluster_bytes = value.last_cluster_bytes,
        .last_first_cluster = value.last_first_cluster,
    };
}
fn fromRawSummary(value: RawSummary) Summary {
    return .{
        .enabled = value.enabled,
        .probes = value.probes,
        .ready = value.ready,
        .failures = value.failures,
        .last_status = value.last_status,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_requested_bytes = value.last_requested_bytes,
        .last_available_bytes = value.last_available_bytes,
        .last_file_size = value.last_file_size,
        .last_cluster_bytes = value.last_cluster_bytes,
        .last_first_cluster = value.last_first_cluster,
    };
}
const RawSlotInput = extern struct {
    operation: u32,
    requested_slots: u64,
    reservation_id: u32,
    owner_kind: u32,
    owner_id: u32,
    region_id: u32,
    flags: u32,
    backing: RawResult,
};
fn rawSlotInput(value: SlotInput) RawSlotInput {
    return .{
        .operation = value.operation,
        .requested_slots = value.requested_slots,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .region_id = value.region_id,
        .flags = value.flags,
        .backing = rawResult(value.backing),
    };
}
fn fromRawSlotInput(value: RawSlotInput) SlotInput {
    return .{
        .operation = value.operation,
        .requested_slots = value.requested_slots,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .region_id = value.region_id,
        .flags = value.flags,
        .backing = fromRawResult(value.backing),
    };
}
const RawSlotResult = extern struct {
    version: u32,
    status: u32,
    operation: u32,
    flags: u32,
    blockers: u32,
    slot_bytes: u32,
    capacity_slots: u64,
    requested_slots: u64,
    reserved_slots: u64,
    free_slots: u64,
    valid_slots: u64,
    dirty_slots: u64,
    error_slots: u64,
    range_count: u32,
    max_ranges: u32,
    reservation_id: u32,
    owner_kind: u32,
    owner_id: u32,
    region_id: u32,
    first_slot: u64,
    slot_count: u64,
    generation: u64,
    pager_enabled: u32,
    eviction_enabled: u32,
    page_in_enabled: u32,
    page_out_enabled: u32,
    total_probes: u64,
    total_reserves: u64,
    total_releases: u64,
    total_error_marks: u64,
    total_recoveries: u64,
    total_failures: u64,
};
fn rawSlotResult(value: SlotResult) RawSlotResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .slot_bytes = value.slot_bytes,
        .capacity_slots = value.capacity_slots,
        .requested_slots = value.requested_slots,
        .reserved_slots = value.reserved_slots,
        .free_slots = value.free_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .range_count = value.range_count,
        .max_ranges = value.max_ranges,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .region_id = value.region_id,
        .first_slot = value.first_slot,
        .slot_count = value.slot_count,
        .generation = value.generation,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .total_probes = value.total_probes,
        .total_reserves = value.total_reserves,
        .total_releases = value.total_releases,
        .total_error_marks = value.total_error_marks,
        .total_recoveries = value.total_recoveries,
        .total_failures = value.total_failures,
    };
}
fn fromRawSlotResult(value: RawSlotResult) SlotResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .slot_bytes = value.slot_bytes,
        .capacity_slots = value.capacity_slots,
        .requested_slots = value.requested_slots,
        .reserved_slots = value.reserved_slots,
        .free_slots = value.free_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .range_count = value.range_count,
        .max_ranges = value.max_ranges,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .region_id = value.region_id,
        .first_slot = value.first_slot,
        .slot_count = value.slot_count,
        .generation = value.generation,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .total_probes = value.total_probes,
        .total_reserves = value.total_reserves,
        .total_releases = value.total_releases,
        .total_error_marks = value.total_error_marks,
        .total_recoveries = value.total_recoveries,
        .total_failures = value.total_failures,
    };
}
const RawSlotSummary = extern struct {
    enabled: bool,
    last_status: u32,
    last_operation: u32,
    last_flags: u32,
    last_blockers: u32,
    slot_bytes: u32,
    capacity_slots: u64,
    reserved_slots: u64,
    free_slots: u64,
    valid_slots: u64,
    dirty_slots: u64,
    error_slots: u64,
    range_count: u32,
    max_ranges: u32,
    last_reservation_id: u32,
    last_owner_kind: u32,
    last_owner_id: u32,
    last_region_id: u32,
    last_first_slot: u64,
    last_slot_count: u64,
    generation: u64,
    probes: u64,
    reserves: u64,
    releases: u64,
    error_marks: u64,
    recoveries: u64,
    failures: u64,
    lifecycle_cleanups: u64,
    lifecycle_released_ranges: u64,
    lifecycle_released_slots: u64,
};
fn rawSlotSummary(value: SlotSummary) RawSlotSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .slot_bytes = value.slot_bytes,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .free_slots = value.free_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .range_count = value.range_count,
        .max_ranges = value.max_ranges,
        .last_reservation_id = value.last_reservation_id,
        .last_owner_kind = value.last_owner_kind,
        .last_owner_id = value.last_owner_id,
        .last_region_id = value.last_region_id,
        .last_first_slot = value.last_first_slot,
        .last_slot_count = value.last_slot_count,
        .generation = value.generation,
        .probes = value.probes,
        .reserves = value.reserves,
        .releases = value.releases,
        .error_marks = value.error_marks,
        .recoveries = value.recoveries,
        .failures = value.failures,
        .lifecycle_cleanups = value.lifecycle_cleanups,
        .lifecycle_released_ranges = value.lifecycle_released_ranges,
        .lifecycle_released_slots = value.lifecycle_released_slots,
    };
}
fn fromRawSlotSummary(value: RawSlotSummary) SlotSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .slot_bytes = value.slot_bytes,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .free_slots = value.free_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .range_count = value.range_count,
        .max_ranges = value.max_ranges,
        .last_reservation_id = value.last_reservation_id,
        .last_owner_kind = value.last_owner_kind,
        .last_owner_id = value.last_owner_id,
        .last_region_id = value.last_region_id,
        .last_first_slot = value.last_first_slot,
        .last_slot_count = value.last_slot_count,
        .generation = value.generation,
        .probes = value.probes,
        .reserves = value.reserves,
        .releases = value.releases,
        .error_marks = value.error_marks,
        .recoveries = value.recoveries,
        .failures = value.failures,
        .lifecycle_cleanups = value.lifecycle_cleanups,
        .lifecycle_released_ranges = value.lifecycle_released_ranges,
        .lifecycle_released_slots = value.lifecycle_released_slots,
    };
}
const RawPagerGateInput = extern struct {
    requested_bytes: u64,
    region_id: u32,
    owner_id: u32,
    flags: u32,
    vm_region_exists: bool,
    vm_region_is_r4x: bool,
    committed_bytes: u64,
    resident_bytes: u64,
    fault_count: u64,
    failed_faults: u64,
    backing: RawResult,
};
fn rawPagerGateInput(value: PagerGateInput) RawPagerGateInput {
    return .{
        .requested_bytes = value.requested_bytes,
        .region_id = value.region_id,
        .owner_id = value.owner_id,
        .flags = value.flags,
        .vm_region_exists = value.vm_region_exists,
        .vm_region_is_r4x = value.vm_region_is_r4x,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .backing = rawResult(value.backing),
    };
}
fn fromRawPagerGateInput(value: RawPagerGateInput) PagerGateInput {
    return .{
        .requested_bytes = value.requested_bytes,
        .region_id = value.region_id,
        .owner_id = value.owner_id,
        .flags = value.flags,
        .vm_region_exists = value.vm_region_exists,
        .vm_region_is_r4x = value.vm_region_is_r4x,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .backing = fromRawResult(value.backing),
    };
}
const RawPagerGateResult = extern struct {
    version: u32,
    status: u32,
    flags: u32,
    blockers: u32,
    region_id: u32,
    owner_id: u32,
    slot_bytes: u32,
    reserved0: u32,
    requested_bytes: u64,
    committed_bytes: u64,
    resident_bytes: u64,
    nonresident_bytes: u64,
    requested_slots: u64,
    prepared_slots: u64,
    capacity_slots: u64,
    free_before_slots: u64,
    free_after_slots: u64,
    reserved_before_slots: u64,
    reserved_after_slots: u64,
    slot_reservation_id: u32,
    rollback_completed: u32,
    commit_gate_enabled: u32,
    fault_gate_enabled: u32,
    pager_enabled: u32,
    eviction_enabled: u32,
    page_in_enabled: u32,
    page_out_enabled: u32,
    slot_generation: u64,
    fault_count: u64,
    failed_faults: u64,
    total_probes: u64,
    total_ready: u64,
    total_rollbacks: u64,
    total_failures: u64,
};
fn rawPagerGateResult(value: PagerGateResult) RawPagerGateResult {
    return .{
        .version = value.version,
        .status = value.status,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .owner_id = value.owner_id,
        .slot_bytes = value.slot_bytes,
        .reserved0 = value.reserved0,
        .requested_bytes = value.requested_bytes,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .nonresident_bytes = value.nonresident_bytes,
        .requested_slots = value.requested_slots,
        .prepared_slots = value.prepared_slots,
        .capacity_slots = value.capacity_slots,
        .free_before_slots = value.free_before_slots,
        .free_after_slots = value.free_after_slots,
        .reserved_before_slots = value.reserved_before_slots,
        .reserved_after_slots = value.reserved_after_slots,
        .slot_reservation_id = value.slot_reservation_id,
        .rollback_completed = value.rollback_completed,
        .commit_gate_enabled = value.commit_gate_enabled,
        .fault_gate_enabled = value.fault_gate_enabled,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .slot_generation = value.slot_generation,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .total_probes = value.total_probes,
        .total_ready = value.total_ready,
        .total_rollbacks = value.total_rollbacks,
        .total_failures = value.total_failures,
    };
}
fn fromRawPagerGateResult(value: RawPagerGateResult) PagerGateResult {
    return .{
        .version = value.version,
        .status = value.status,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .owner_id = value.owner_id,
        .slot_bytes = value.slot_bytes,
        .reserved0 = value.reserved0,
        .requested_bytes = value.requested_bytes,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .nonresident_bytes = value.nonresident_bytes,
        .requested_slots = value.requested_slots,
        .prepared_slots = value.prepared_slots,
        .capacity_slots = value.capacity_slots,
        .free_before_slots = value.free_before_slots,
        .free_after_slots = value.free_after_slots,
        .reserved_before_slots = value.reserved_before_slots,
        .reserved_after_slots = value.reserved_after_slots,
        .slot_reservation_id = value.slot_reservation_id,
        .rollback_completed = value.rollback_completed,
        .commit_gate_enabled = value.commit_gate_enabled,
        .fault_gate_enabled = value.fault_gate_enabled,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .slot_generation = value.slot_generation,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .total_probes = value.total_probes,
        .total_ready = value.total_ready,
        .total_rollbacks = value.total_rollbacks,
        .total_failures = value.total_failures,
    };
}
const RawPagerGateSummary = extern struct {
    enabled: bool,
    last_status: u32,
    last_flags: u32,
    last_blockers: u32,
    last_region_id: u32,
    last_owner_id: u32,
    requested_bytes: u64,
    committed_bytes: u64,
    resident_bytes: u64,
    nonresident_bytes: u64,
    requested_slots: u64,
    prepared_slots: u64,
    capacity_slots: u64,
    free_before_slots: u64,
    free_after_slots: u64,
    reserved_before_slots: u64,
    reserved_after_slots: u64,
    last_reservation_id: u32,
    rollback_completed: u32,
    commit_gate_enabled: u32,
    fault_gate_enabled: u32,
    pager_enabled: u32,
    eviction_enabled: u32,
    page_in_enabled: u32,
    page_out_enabled: u32,
    slot_generation: u64,
    fault_count: u64,
    failed_faults: u64,
    probes: u64,
    ready: u64,
    rollbacks: u64,
    failures: u64,
};
fn rawPagerGateSummary(value: PagerGateSummary) RawPagerGateSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_owner_id = value.last_owner_id,
        .requested_bytes = value.requested_bytes,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .nonresident_bytes = value.nonresident_bytes,
        .requested_slots = value.requested_slots,
        .prepared_slots = value.prepared_slots,
        .capacity_slots = value.capacity_slots,
        .free_before_slots = value.free_before_slots,
        .free_after_slots = value.free_after_slots,
        .reserved_before_slots = value.reserved_before_slots,
        .reserved_after_slots = value.reserved_after_slots,
        .last_reservation_id = value.last_reservation_id,
        .rollback_completed = value.rollback_completed,
        .commit_gate_enabled = value.commit_gate_enabled,
        .fault_gate_enabled = value.fault_gate_enabled,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .slot_generation = value.slot_generation,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .probes = value.probes,
        .ready = value.ready,
        .rollbacks = value.rollbacks,
        .failures = value.failures,
    };
}
fn fromRawPagerGateSummary(value: RawPagerGateSummary) PagerGateSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_owner_id = value.last_owner_id,
        .requested_bytes = value.requested_bytes,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .nonresident_bytes = value.nonresident_bytes,
        .requested_slots = value.requested_slots,
        .prepared_slots = value.prepared_slots,
        .capacity_slots = value.capacity_slots,
        .free_before_slots = value.free_before_slots,
        .free_after_slots = value.free_after_slots,
        .reserved_before_slots = value.reserved_before_slots,
        .reserved_after_slots = value.reserved_after_slots,
        .last_reservation_id = value.last_reservation_id,
        .rollback_completed = value.rollback_completed,
        .commit_gate_enabled = value.commit_gate_enabled,
        .fault_gate_enabled = value.fault_gate_enabled,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .slot_generation = value.slot_generation,
        .fault_count = value.fault_count,
        .failed_faults = value.failed_faults,
        .probes = value.probes,
        .ready = value.ready,
        .rollbacks = value.rollbacks,
        .failures = value.failures,
    };
}
const RawPageIoInput = extern struct {
    operation: u32,
    region_id: u32,
    region_offset: u64,
    reservation_id: u32,
    slot_index: u64,
    page_count: u64,
    owner_kind: u32,
    owner_id: u32,
    expected_generation: u64,
    flags: u32,
    vm_region_exists: bool,
    vm_region_is_r4x: bool,
    commit_covered: bool,
    committed_bytes: u64,
    resident_bytes: u64,
    backing: RawResult,
    io_status: i32,
    io_bytes: u32,
};
fn rawPageIoInput(value: PageIoInput) RawPageIoInput {
    return .{
        .operation = value.operation,
        .region_id = value.region_id,
        .region_offset = value.region_offset,
        .reservation_id = value.reservation_id,
        .slot_index = value.slot_index,
        .page_count = value.page_count,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .expected_generation = value.expected_generation,
        .flags = value.flags,
        .vm_region_exists = value.vm_region_exists,
        .vm_region_is_r4x = value.vm_region_is_r4x,
        .commit_covered = value.commit_covered,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .backing = rawResult(value.backing),
        .io_status = value.io_status,
        .io_bytes = value.io_bytes,
    };
}
fn fromRawPageIoInput(value: RawPageIoInput) PageIoInput {
    return .{
        .operation = value.operation,
        .region_id = value.region_id,
        .region_offset = value.region_offset,
        .reservation_id = value.reservation_id,
        .slot_index = value.slot_index,
        .page_count = value.page_count,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .expected_generation = value.expected_generation,
        .flags = value.flags,
        .vm_region_exists = value.vm_region_exists,
        .vm_region_is_r4x = value.vm_region_is_r4x,
        .commit_covered = value.commit_covered,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .backing = fromRawResult(value.backing),
        .io_status = value.io_status,
        .io_bytes = value.io_bytes,
    };
}
const RawPageIoResult = extern struct {
    version: u32,
    status: u32,
    operation: u32,
    flags: u32,
    blockers: u32,
    region_id: u32,
    reservation_id: u32,
    owner_kind: u32,
    owner_id: u32,
    slot_bytes: u32,
    reserved0: u32,
    region_offset: u64,
    committed_bytes: u64,
    resident_bytes: u64,
    slot_index: u64,
    page_count: u64,
    transfer_bytes: u64,
    expected_generation: u64,
    backing_slot: u64,
    backing_offset: u64,
    capacity_slots: u64,
    reserved_slots: u64,
    valid_slots: u64,
    dirty_slots: u64,
    error_slots: u64,
    io_bytes: u32,
    io_status: i32,
    pager_enabled: u32,
    eviction_enabled: u32,
    page_in_enabled: u32,
    page_out_enabled: u32,
    retry_limit: u32,
    backoff_ticks: u32,
    slot_generation: u64,
    total_prepares: u64,
    total_page_outs: u64,
    total_page_ins: u64,
    total_failures: u64,
    total_retry_attempts: u64,
    total_retryable_failures: u64,
    total_permanent_failures: u64,
    total_retry_limit_hits: u64,
    total_failed_page_outs: u64,
    total_failed_page_ins: u64,
    total_data_preserved_pages: u64,
    total_data_lost_pages: u64,
};
fn rawPageIoResult(value: PageIoResult) RawPageIoResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .slot_bytes = value.slot_bytes,
        .reserved0 = value.reserved0,
        .region_offset = value.region_offset,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .slot_index = value.slot_index,
        .page_count = value.page_count,
        .transfer_bytes = value.transfer_bytes,
        .expected_generation = value.expected_generation,
        .backing_slot = value.backing_slot,
        .backing_offset = value.backing_offset,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .io_bytes = value.io_bytes,
        .io_status = value.io_status,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .retry_limit = value.retry_limit,
        .backoff_ticks = value.backoff_ticks,
        .slot_generation = value.slot_generation,
        .total_prepares = value.total_prepares,
        .total_page_outs = value.total_page_outs,
        .total_page_ins = value.total_page_ins,
        .total_failures = value.total_failures,
        .total_retry_attempts = value.total_retry_attempts,
        .total_retryable_failures = value.total_retryable_failures,
        .total_permanent_failures = value.total_permanent_failures,
        .total_retry_limit_hits = value.total_retry_limit_hits,
        .total_failed_page_outs = value.total_failed_page_outs,
        .total_failed_page_ins = value.total_failed_page_ins,
        .total_data_preserved_pages = value.total_data_preserved_pages,
        .total_data_lost_pages = value.total_data_lost_pages,
    };
}
fn fromRawPageIoResult(value: RawPageIoResult) PageIoResult {
    return .{
        .version = value.version,
        .status = value.status,
        .operation = value.operation,
        .flags = value.flags,
        .blockers = value.blockers,
        .region_id = value.region_id,
        .reservation_id = value.reservation_id,
        .owner_kind = value.owner_kind,
        .owner_id = value.owner_id,
        .slot_bytes = value.slot_bytes,
        .reserved0 = value.reserved0,
        .region_offset = value.region_offset,
        .committed_bytes = value.committed_bytes,
        .resident_bytes = value.resident_bytes,
        .slot_index = value.slot_index,
        .page_count = value.page_count,
        .transfer_bytes = value.transfer_bytes,
        .expected_generation = value.expected_generation,
        .backing_slot = value.backing_slot,
        .backing_offset = value.backing_offset,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .io_bytes = value.io_bytes,
        .io_status = value.io_status,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .retry_limit = value.retry_limit,
        .backoff_ticks = value.backoff_ticks,
        .slot_generation = value.slot_generation,
        .total_prepares = value.total_prepares,
        .total_page_outs = value.total_page_outs,
        .total_page_ins = value.total_page_ins,
        .total_failures = value.total_failures,
        .total_retry_attempts = value.total_retry_attempts,
        .total_retryable_failures = value.total_retryable_failures,
        .total_permanent_failures = value.total_permanent_failures,
        .total_retry_limit_hits = value.total_retry_limit_hits,
        .total_failed_page_outs = value.total_failed_page_outs,
        .total_failed_page_ins = value.total_failed_page_ins,
        .total_data_preserved_pages = value.total_data_preserved_pages,
        .total_data_lost_pages = value.total_data_lost_pages,
    };
}
const RawPageIoSummary = extern struct {
    enabled: bool,
    last_status: u32,
    last_operation: u32,
    last_flags: u32,
    last_blockers: u32,
    last_region_id: u32,
    last_reservation_id: u32,
    last_owner_kind: u32,
    last_owner_id: u32,
    region_offset: u64,
    page_count: u64,
    transfer_bytes: u64,
    expected_generation: u64,
    backing_slot: u64,
    backing_offset: u64,
    io_bytes: u32,
    io_status: i32,
    capacity_slots: u64,
    reserved_slots: u64,
    valid_slots: u64,
    dirty_slots: u64,
    error_slots: u64,
    pager_enabled: u32,
    eviction_enabled: u32,
    page_in_enabled: u32,
    page_out_enabled: u32,
    retry_limit: u32,
    backoff_ticks: u32,
    slot_generation: u64,
    prepares: u64,
    page_outs: u64,
    page_ins: u64,
    failures: u64,
    retry_attempts: u64,
    retryable_failures: u64,
    permanent_failures: u64,
    retry_limit_hits: u64,
    failed_page_outs: u64,
    failed_page_ins: u64,
    data_preserved_pages: u64,
    data_lost_pages: u64,
};
fn rawPageIoSummary(value: PageIoSummary) RawPageIoSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_reservation_id = value.last_reservation_id,
        .last_owner_kind = value.last_owner_kind,
        .last_owner_id = value.last_owner_id,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .transfer_bytes = value.transfer_bytes,
        .expected_generation = value.expected_generation,
        .backing_slot = value.backing_slot,
        .backing_offset = value.backing_offset,
        .io_bytes = value.io_bytes,
        .io_status = value.io_status,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .retry_limit = value.retry_limit,
        .backoff_ticks = value.backoff_ticks,
        .slot_generation = value.slot_generation,
        .prepares = value.prepares,
        .page_outs = value.page_outs,
        .page_ins = value.page_ins,
        .failures = value.failures,
        .retry_attempts = value.retry_attempts,
        .retryable_failures = value.retryable_failures,
        .permanent_failures = value.permanent_failures,
        .retry_limit_hits = value.retry_limit_hits,
        .failed_page_outs = value.failed_page_outs,
        .failed_page_ins = value.failed_page_ins,
        .data_preserved_pages = value.data_preserved_pages,
        .data_lost_pages = value.data_lost_pages,
    };
}
fn fromRawPageIoSummary(value: RawPageIoSummary) PageIoSummary {
    return .{
        .enabled = value.enabled,
        .last_status = value.last_status,
        .last_operation = value.last_operation,
        .last_flags = value.last_flags,
        .last_blockers = value.last_blockers,
        .last_region_id = value.last_region_id,
        .last_reservation_id = value.last_reservation_id,
        .last_owner_kind = value.last_owner_kind,
        .last_owner_id = value.last_owner_id,
        .region_offset = value.region_offset,
        .page_count = value.page_count,
        .transfer_bytes = value.transfer_bytes,
        .expected_generation = value.expected_generation,
        .backing_slot = value.backing_slot,
        .backing_offset = value.backing_offset,
        .io_bytes = value.io_bytes,
        .io_status = value.io_status,
        .capacity_slots = value.capacity_slots,
        .reserved_slots = value.reserved_slots,
        .valid_slots = value.valid_slots,
        .dirty_slots = value.dirty_slots,
        .error_slots = value.error_slots,
        .pager_enabled = value.pager_enabled,
        .eviction_enabled = value.eviction_enabled,
        .page_in_enabled = value.page_in_enabled,
        .page_out_enabled = value.page_out_enabled,
        .retry_limit = value.retry_limit,
        .backoff_ticks = value.backoff_ticks,
        .slot_generation = value.slot_generation,
        .prepares = value.prepares,
        .page_outs = value.page_outs,
        .page_ins = value.page_ins,
        .failures = value.failures,
        .retry_attempts = value.retry_attempts,
        .retryable_failures = value.retryable_failures,
        .permanent_failures = value.permanent_failures,
        .retry_limit_hits = value.retry_limit_hits,
        .failed_page_outs = value.failed_page_outs,
        .failed_page_ins = value.failed_page_ins,
        .data_preserved_pages = value.data_preserved_pages,
        .data_lost_pages = value.data_lost_pages,
    };
}
const RawLifecycleCleanupResult = extern struct {
    cleanup_count: u64,
    released_ranges: u64,
    released_slots: u64,
    released_valid_slots: u64,
    released_dirty_slots: u64,
    released_error_slots: u64,
};
fn rawLifecycleCleanupResult(value: LifecycleCleanupResult) RawLifecycleCleanupResult {
    return .{
        .cleanup_count = value.cleanup_count,
        .released_ranges = value.released_ranges,
        .released_slots = value.released_slots,
        .released_valid_slots = value.released_valid_slots,
        .released_dirty_slots = value.released_dirty_slots,
        .released_error_slots = value.released_error_slots,
    };
}
fn fromRawLifecycleCleanupResult(value: RawLifecycleCleanupResult) LifecycleCleanupResult {
    return .{
        .cleanup_count = value.cleanup_count,
        .released_ranges = value.released_ranges,
        .released_slots = value.released_slots,
        .released_valid_slots = value.released_valid_slots,
        .released_dirty_slots = value.released_dirty_slots,
        .released_error_slots = value.released_error_slots,
    };
}
extern fn r4os_rust_backing_probe(*const RawInput, *RawResult) callconv(.c) void;
pub fn probe(input: Input) Result {
    const token = enter();
    defer leave(token);
    const raw = rawInput(input);
    var output: RawResult = undefined;
    r4os_rust_backing_probe(&raw, &output);
    return fromRawResult(output);
}
extern fn r4os_rust_backing_slot_probe(*const RawSlotInput, *RawSlotResult) callconv(.c) void;
pub fn slotProbe(input: SlotInput) SlotResult {
    const token = enter();
    defer leave(token);
    const raw = rawSlotInput(input);
    var output: RawSlotResult = undefined;
    r4os_rust_backing_slot_probe(&raw, &output);
    return fromRawSlotResult(output);
}
extern fn r4os_rust_backing_pager_gate_probe(*const RawPagerGateInput, *RawPagerGateResult) callconv(.c) void;
pub fn pagerGateProbe(input: PagerGateInput) PagerGateResult {
    const token = enter();
    defer leave(token);
    const raw = rawPagerGateInput(input);
    var output: RawPagerGateResult = undefined;
    r4os_rust_backing_pager_gate_probe(&raw, &output);
    return fromRawPagerGateResult(output);
}
extern fn r4os_rust_backing_page_io_prepare(*const RawPageIoInput, *RawPageIoResult) callconv(.c) void;
pub fn pageIoPrepare(input: PageIoInput) PageIoResult {
    const token = enter();
    defer leave(token);
    const raw = rawPageIoInput(input);
    var output: RawPageIoResult = undefined;
    r4os_rust_backing_page_io_prepare(&raw, &output);
    return fromRawPageIoResult(output);
}
extern fn r4os_rust_backing_page_io_complete(*const RawPageIoInput, *RawPageIoResult) callconv(.c) void;
pub fn pageIoComplete(input: PageIoInput) PageIoResult {
    const token = enter();
    defer leave(token);
    const raw = rawPageIoInput(input);
    var output: RawPageIoResult = undefined;
    r4os_rust_backing_page_io_complete(&raw, &output);
    return fromRawPageIoResult(output);
}
extern fn r4os_rust_backing_summary(*RawSummary) callconv(.c) void;
pub fn summary() Summary {
    const token = enter();
    defer leave(token);
    var output: RawSummary = undefined;
    r4os_rust_backing_summary(&output);
    return fromRawSummary(output);
}
extern fn r4os_rust_backing_slot_summary(*RawSlotSummary) callconv(.c) void;
pub fn slotSummary() SlotSummary {
    const token = enter();
    defer leave(token);
    var output: RawSlotSummary = undefined;
    r4os_rust_backing_slot_summary(&output);
    return fromRawSlotSummary(output);
}
extern fn r4os_rust_backing_pager_gate_summary(*RawPagerGateSummary) callconv(.c) void;
pub fn pagerGateSummary() PagerGateSummary {
    const token = enter();
    defer leave(token);
    var output: RawPagerGateSummary = undefined;
    r4os_rust_backing_pager_gate_summary(&output);
    return fromRawPagerGateSummary(output);
}
extern fn r4os_rust_backing_page_io_summary(*RawPageIoSummary) callconv(.c) void;
pub fn pageIoSummary() PageIoSummary {
    const token = enter();
    defer leave(token);
    var output: RawPageIoSummary = undefined;
    r4os_rust_backing_page_io_summary(&output);
    return fromRawPageIoSummary(output);
}
extern fn r4os_rust_backing_release_r4x_owner(owner_id: u32, output: *RawLifecycleCleanupResult) callconv(.c) void;
pub fn releaseR4xOwner(owner_id: u32) LifecycleCleanupResult {
    const token = enter();
    defer leave(token);
    var output: RawLifecycleCleanupResult = undefined;
    r4os_rust_backing_release_r4x_owner(owner_id, &output);
    return fromRawLifecycleCleanupResult(output);
}
extern fn r4os_rust_backing_release_vm_region(owner_id: u32, region_id: u32, output: *RawLifecycleCleanupResult) callconv(.c) void;
pub fn releaseVmRegion(owner_id: u32, region_id: u32) LifecycleCleanupResult {
    const token = enter();
    defer leave(token);
    var output: RawLifecycleCleanupResult = undefined;
    r4os_rust_backing_release_vm_region(owner_id, region_id, &output);
    return fromRawLifecycleCleanupResult(output);
}
extern fn r4os_rust_backing_active_path() callconv(.c) ?[*:0]const u8;
pub fn activeBackingPath() ?[*:0]const u8 {
    const token = enter();
    defer leave(token);
    return r4os_rust_backing_active_path();
}
extern fn r4os_rust_backing_active_result(*RawResult) callconv(.c) bool;
pub fn activeBackingResult() ?Result {
    const token = enter();
    defer leave(token);
    var output: RawResult = undefined;
    return if (r4os_rust_backing_active_result(&output)) fromRawResult(output) else null;
}

comptime {
    if (@sizeOf(RawInput) != 48 or @alignOf(RawInput) != 8) @compileError("Native backing Input geometry");
    if (@offsetOf(RawInput, "requested_bytes") != 0) @compileError("Native backing Input.requested_bytes offset");
    if (@offsetOf(RawInput, "flags") != 8) @compileError("Native backing Input.flags offset");
    if (@offsetOf(RawInput, "path") != 16) @compileError("Native backing Input.path offset");
    if (@offsetOf(RawInput, "file_exists") != 24) @compileError("Native backing Input.file_exists offset");
    if (@offsetOf(RawInput, "is_dir") != 25) @compileError("Native backing Input.is_dir offset");
    if (@offsetOf(RawInput, "fat32") != 26) @compileError("Native backing Input.fat32 offset");
    if (@offsetOf(RawInput, "file_size") != 32) @compileError("Native backing Input.file_size offset");
    if (@offsetOf(RawInput, "cluster_bytes") != 40) @compileError("Native backing Input.cluster_bytes offset");
    if (@offsetOf(RawInput, "first_cluster") != 44) @compileError("Native backing Input.first_cluster offset");
    if (@sizeOf(RawResult) != 184 or @alignOf(RawResult) != 8) @compileError("Native backing Result geometry");
    if (@offsetOf(RawResult, "path") != 0) @compileError("Native backing Result.path offset");
    if (@offsetOf(RawResult, "status") != 132) @compileError("Native backing Result.status offset");
    if (@offsetOf(RawResult, "flags") != 136) @compileError("Native backing Result.flags offset");
    if (@offsetOf(RawResult, "blockers") != 140) @compileError("Native backing Result.blockers offset");
    if (@offsetOf(RawResult, "requested_bytes") != 144) @compileError("Native backing Result.requested_bytes offset");
    if (@offsetOf(RawResult, "available_bytes") != 152) @compileError("Native backing Result.available_bytes offset");
    if (@offsetOf(RawResult, "file_size") != 160) @compileError("Native backing Result.file_size offset");
    if (@offsetOf(RawResult, "cluster_bytes") != 168) @compileError("Native backing Result.cluster_bytes offset");
    if (@offsetOf(RawResult, "first_cluster") != 172) @compileError("Native backing Result.first_cluster offset");
    if (@offsetOf(RawResult, "pager_enabled") != 176) @compileError("Native backing Result.pager_enabled offset");
    if (@offsetOf(RawResult, "anonymous_paging_enabled") != 180) @compileError("Native backing Result.anonymous_paging_enabled offset");
    if (@sizeOf(RawSummary) != 80 or @alignOf(RawSummary) != 8) @compileError("Native backing Summary geometry");
    if (@offsetOf(RawSummary, "enabled") != 0) @compileError("Native backing Summary.enabled offset");
    if (@offsetOf(RawSummary, "probes") != 8) @compileError("Native backing Summary.probes offset");
    if (@offsetOf(RawSummary, "ready") != 16) @compileError("Native backing Summary.ready offset");
    if (@offsetOf(RawSummary, "failures") != 24) @compileError("Native backing Summary.failures offset");
    if (@offsetOf(RawSummary, "last_status") != 32) @compileError("Native backing Summary.last_status offset");
    if (@offsetOf(RawSummary, "last_flags") != 36) @compileError("Native backing Summary.last_flags offset");
    if (@offsetOf(RawSummary, "last_blockers") != 40) @compileError("Native backing Summary.last_blockers offset");
    if (@offsetOf(RawSummary, "last_requested_bytes") != 48) @compileError("Native backing Summary.last_requested_bytes offset");
    if (@offsetOf(RawSummary, "last_available_bytes") != 56) @compileError("Native backing Summary.last_available_bytes offset");
    if (@offsetOf(RawSummary, "last_file_size") != 64) @compileError("Native backing Summary.last_file_size offset");
    if (@offsetOf(RawSummary, "last_cluster_bytes") != 72) @compileError("Native backing Summary.last_cluster_bytes offset");
    if (@offsetOf(RawSummary, "last_first_cluster") != 76) @compileError("Native backing Summary.last_first_cluster offset");
    if (@sizeOf(RawSlotInput) != 224 or @alignOf(RawSlotInput) != 8) @compileError("Native backing SlotInput geometry");
    if (@offsetOf(RawSlotInput, "operation") != 0) @compileError("Native backing SlotInput.operation offset");
    if (@offsetOf(RawSlotInput, "requested_slots") != 8) @compileError("Native backing SlotInput.requested_slots offset");
    if (@offsetOf(RawSlotInput, "reservation_id") != 16) @compileError("Native backing SlotInput.reservation_id offset");
    if (@offsetOf(RawSlotInput, "owner_kind") != 20) @compileError("Native backing SlotInput.owner_kind offset");
    if (@offsetOf(RawSlotInput, "owner_id") != 24) @compileError("Native backing SlotInput.owner_id offset");
    if (@offsetOf(RawSlotInput, "region_id") != 28) @compileError("Native backing SlotInput.region_id offset");
    if (@offsetOf(RawSlotInput, "flags") != 32) @compileError("Native backing SlotInput.flags offset");
    if (@offsetOf(RawSlotInput, "backing") != 40) @compileError("Native backing SlotInput.backing offset");
    if (@sizeOf(RawSlotResult) != 192 or @alignOf(RawSlotResult) != 8) @compileError("Native backing SlotResult geometry");
    if (@offsetOf(RawSlotResult, "version") != 0) @compileError("Native backing SlotResult.version offset");
    if (@offsetOf(RawSlotResult, "status") != 4) @compileError("Native backing SlotResult.status offset");
    if (@offsetOf(RawSlotResult, "operation") != 8) @compileError("Native backing SlotResult.operation offset");
    if (@offsetOf(RawSlotResult, "flags") != 12) @compileError("Native backing SlotResult.flags offset");
    if (@offsetOf(RawSlotResult, "blockers") != 16) @compileError("Native backing SlotResult.blockers offset");
    if (@offsetOf(RawSlotResult, "slot_bytes") != 20) @compileError("Native backing SlotResult.slot_bytes offset");
    if (@offsetOf(RawSlotResult, "capacity_slots") != 24) @compileError("Native backing SlotResult.capacity_slots offset");
    if (@offsetOf(RawSlotResult, "requested_slots") != 32) @compileError("Native backing SlotResult.requested_slots offset");
    if (@offsetOf(RawSlotResult, "reserved_slots") != 40) @compileError("Native backing SlotResult.reserved_slots offset");
    if (@offsetOf(RawSlotResult, "free_slots") != 48) @compileError("Native backing SlotResult.free_slots offset");
    if (@offsetOf(RawSlotResult, "valid_slots") != 56) @compileError("Native backing SlotResult.valid_slots offset");
    if (@offsetOf(RawSlotResult, "dirty_slots") != 64) @compileError("Native backing SlotResult.dirty_slots offset");
    if (@offsetOf(RawSlotResult, "error_slots") != 72) @compileError("Native backing SlotResult.error_slots offset");
    if (@offsetOf(RawSlotResult, "range_count") != 80) @compileError("Native backing SlotResult.range_count offset");
    if (@offsetOf(RawSlotResult, "max_ranges") != 84) @compileError("Native backing SlotResult.max_ranges offset");
    if (@offsetOf(RawSlotResult, "reservation_id") != 88) @compileError("Native backing SlotResult.reservation_id offset");
    if (@offsetOf(RawSlotResult, "owner_kind") != 92) @compileError("Native backing SlotResult.owner_kind offset");
    if (@offsetOf(RawSlotResult, "owner_id") != 96) @compileError("Native backing SlotResult.owner_id offset");
    if (@offsetOf(RawSlotResult, "region_id") != 100) @compileError("Native backing SlotResult.region_id offset");
    if (@offsetOf(RawSlotResult, "first_slot") != 104) @compileError("Native backing SlotResult.first_slot offset");
    if (@offsetOf(RawSlotResult, "slot_count") != 112) @compileError("Native backing SlotResult.slot_count offset");
    if (@offsetOf(RawSlotResult, "generation") != 120) @compileError("Native backing SlotResult.generation offset");
    if (@offsetOf(RawSlotResult, "pager_enabled") != 128) @compileError("Native backing SlotResult.pager_enabled offset");
    if (@offsetOf(RawSlotResult, "eviction_enabled") != 132) @compileError("Native backing SlotResult.eviction_enabled offset");
    if (@offsetOf(RawSlotResult, "page_in_enabled") != 136) @compileError("Native backing SlotResult.page_in_enabled offset");
    if (@offsetOf(RawSlotResult, "page_out_enabled") != 140) @compileError("Native backing SlotResult.page_out_enabled offset");
    if (@offsetOf(RawSlotResult, "total_probes") != 144) @compileError("Native backing SlotResult.total_probes offset");
    if (@offsetOf(RawSlotResult, "total_reserves") != 152) @compileError("Native backing SlotResult.total_reserves offset");
    if (@offsetOf(RawSlotResult, "total_releases") != 160) @compileError("Native backing SlotResult.total_releases offset");
    if (@offsetOf(RawSlotResult, "total_error_marks") != 168) @compileError("Native backing SlotResult.total_error_marks offset");
    if (@offsetOf(RawSlotResult, "total_recoveries") != 176) @compileError("Native backing SlotResult.total_recoveries offset");
    if (@offsetOf(RawSlotResult, "total_failures") != 184) @compileError("Native backing SlotResult.total_failures offset");
    if (@sizeOf(RawSlotSummary) != 192 or @alignOf(RawSlotSummary) != 8) @compileError("Native backing SlotSummary geometry");
    if (@offsetOf(RawSlotSummary, "enabled") != 0) @compileError("Native backing SlotSummary.enabled offset");
    if (@offsetOf(RawSlotSummary, "last_status") != 4) @compileError("Native backing SlotSummary.last_status offset");
    if (@offsetOf(RawSlotSummary, "last_operation") != 8) @compileError("Native backing SlotSummary.last_operation offset");
    if (@offsetOf(RawSlotSummary, "last_flags") != 12) @compileError("Native backing SlotSummary.last_flags offset");
    if (@offsetOf(RawSlotSummary, "last_blockers") != 16) @compileError("Native backing SlotSummary.last_blockers offset");
    if (@offsetOf(RawSlotSummary, "slot_bytes") != 20) @compileError("Native backing SlotSummary.slot_bytes offset");
    if (@offsetOf(RawSlotSummary, "capacity_slots") != 24) @compileError("Native backing SlotSummary.capacity_slots offset");
    if (@offsetOf(RawSlotSummary, "reserved_slots") != 32) @compileError("Native backing SlotSummary.reserved_slots offset");
    if (@offsetOf(RawSlotSummary, "free_slots") != 40) @compileError("Native backing SlotSummary.free_slots offset");
    if (@offsetOf(RawSlotSummary, "valid_slots") != 48) @compileError("Native backing SlotSummary.valid_slots offset");
    if (@offsetOf(RawSlotSummary, "dirty_slots") != 56) @compileError("Native backing SlotSummary.dirty_slots offset");
    if (@offsetOf(RawSlotSummary, "error_slots") != 64) @compileError("Native backing SlotSummary.error_slots offset");
    if (@offsetOf(RawSlotSummary, "range_count") != 72) @compileError("Native backing SlotSummary.range_count offset");
    if (@offsetOf(RawSlotSummary, "max_ranges") != 76) @compileError("Native backing SlotSummary.max_ranges offset");
    if (@offsetOf(RawSlotSummary, "last_reservation_id") != 80) @compileError("Native backing SlotSummary.last_reservation_id offset");
    if (@offsetOf(RawSlotSummary, "last_owner_kind") != 84) @compileError("Native backing SlotSummary.last_owner_kind offset");
    if (@offsetOf(RawSlotSummary, "last_owner_id") != 88) @compileError("Native backing SlotSummary.last_owner_id offset");
    if (@offsetOf(RawSlotSummary, "last_region_id") != 92) @compileError("Native backing SlotSummary.last_region_id offset");
    if (@offsetOf(RawSlotSummary, "last_first_slot") != 96) @compileError("Native backing SlotSummary.last_first_slot offset");
    if (@offsetOf(RawSlotSummary, "last_slot_count") != 104) @compileError("Native backing SlotSummary.last_slot_count offset");
    if (@offsetOf(RawSlotSummary, "generation") != 112) @compileError("Native backing SlotSummary.generation offset");
    if (@offsetOf(RawSlotSummary, "probes") != 120) @compileError("Native backing SlotSummary.probes offset");
    if (@offsetOf(RawSlotSummary, "reserves") != 128) @compileError("Native backing SlotSummary.reserves offset");
    if (@offsetOf(RawSlotSummary, "releases") != 136) @compileError("Native backing SlotSummary.releases offset");
    if (@offsetOf(RawSlotSummary, "error_marks") != 144) @compileError("Native backing SlotSummary.error_marks offset");
    if (@offsetOf(RawSlotSummary, "recoveries") != 152) @compileError("Native backing SlotSummary.recoveries offset");
    if (@offsetOf(RawSlotSummary, "failures") != 160) @compileError("Native backing SlotSummary.failures offset");
    if (@offsetOf(RawSlotSummary, "lifecycle_cleanups") != 168) @compileError("Native backing SlotSummary.lifecycle_cleanups offset");
    if (@offsetOf(RawSlotSummary, "lifecycle_released_ranges") != 176) @compileError("Native backing SlotSummary.lifecycle_released_ranges offset");
    if (@offsetOf(RawSlotSummary, "lifecycle_released_slots") != 184) @compileError("Native backing SlotSummary.lifecycle_released_slots offset");
    if (@sizeOf(RawPagerGateInput) != 240 or @alignOf(RawPagerGateInput) != 8) @compileError("Native backing PagerGateInput geometry");
    if (@offsetOf(RawPagerGateInput, "requested_bytes") != 0) @compileError("Native backing PagerGateInput.requested_bytes offset");
    if (@offsetOf(RawPagerGateInput, "region_id") != 8) @compileError("Native backing PagerGateInput.region_id offset");
    if (@offsetOf(RawPagerGateInput, "owner_id") != 12) @compileError("Native backing PagerGateInput.owner_id offset");
    if (@offsetOf(RawPagerGateInput, "flags") != 16) @compileError("Native backing PagerGateInput.flags offset");
    if (@offsetOf(RawPagerGateInput, "vm_region_exists") != 20) @compileError("Native backing PagerGateInput.vm_region_exists offset");
    if (@offsetOf(RawPagerGateInput, "vm_region_is_r4x") != 21) @compileError("Native backing PagerGateInput.vm_region_is_r4x offset");
    if (@offsetOf(RawPagerGateInput, "committed_bytes") != 24) @compileError("Native backing PagerGateInput.committed_bytes offset");
    if (@offsetOf(RawPagerGateInput, "resident_bytes") != 32) @compileError("Native backing PagerGateInput.resident_bytes offset");
    if (@offsetOf(RawPagerGateInput, "fault_count") != 40) @compileError("Native backing PagerGateInput.fault_count offset");
    if (@offsetOf(RawPagerGateInput, "failed_faults") != 48) @compileError("Native backing PagerGateInput.failed_faults offset");
    if (@offsetOf(RawPagerGateInput, "backing") != 56) @compileError("Native backing PagerGateInput.backing offset");
    if (@sizeOf(RawPagerGateResult) != 208 or @alignOf(RawPagerGateResult) != 8) @compileError("Native backing PagerGateResult geometry");
    if (@offsetOf(RawPagerGateResult, "version") != 0) @compileError("Native backing PagerGateResult.version offset");
    if (@offsetOf(RawPagerGateResult, "status") != 4) @compileError("Native backing PagerGateResult.status offset");
    if (@offsetOf(RawPagerGateResult, "flags") != 8) @compileError("Native backing PagerGateResult.flags offset");
    if (@offsetOf(RawPagerGateResult, "blockers") != 12) @compileError("Native backing PagerGateResult.blockers offset");
    if (@offsetOf(RawPagerGateResult, "region_id") != 16) @compileError("Native backing PagerGateResult.region_id offset");
    if (@offsetOf(RawPagerGateResult, "owner_id") != 20) @compileError("Native backing PagerGateResult.owner_id offset");
    if (@offsetOf(RawPagerGateResult, "slot_bytes") != 24) @compileError("Native backing PagerGateResult.slot_bytes offset");
    if (@offsetOf(RawPagerGateResult, "reserved0") != 28) @compileError("Native backing PagerGateResult.reserved0 offset");
    if (@offsetOf(RawPagerGateResult, "requested_bytes") != 32) @compileError("Native backing PagerGateResult.requested_bytes offset");
    if (@offsetOf(RawPagerGateResult, "committed_bytes") != 40) @compileError("Native backing PagerGateResult.committed_bytes offset");
    if (@offsetOf(RawPagerGateResult, "resident_bytes") != 48) @compileError("Native backing PagerGateResult.resident_bytes offset");
    if (@offsetOf(RawPagerGateResult, "nonresident_bytes") != 56) @compileError("Native backing PagerGateResult.nonresident_bytes offset");
    if (@offsetOf(RawPagerGateResult, "requested_slots") != 64) @compileError("Native backing PagerGateResult.requested_slots offset");
    if (@offsetOf(RawPagerGateResult, "prepared_slots") != 72) @compileError("Native backing PagerGateResult.prepared_slots offset");
    if (@offsetOf(RawPagerGateResult, "capacity_slots") != 80) @compileError("Native backing PagerGateResult.capacity_slots offset");
    if (@offsetOf(RawPagerGateResult, "free_before_slots") != 88) @compileError("Native backing PagerGateResult.free_before_slots offset");
    if (@offsetOf(RawPagerGateResult, "free_after_slots") != 96) @compileError("Native backing PagerGateResult.free_after_slots offset");
    if (@offsetOf(RawPagerGateResult, "reserved_before_slots") != 104) @compileError("Native backing PagerGateResult.reserved_before_slots offset");
    if (@offsetOf(RawPagerGateResult, "reserved_after_slots") != 112) @compileError("Native backing PagerGateResult.reserved_after_slots offset");
    if (@offsetOf(RawPagerGateResult, "slot_reservation_id") != 120) @compileError("Native backing PagerGateResult.slot_reservation_id offset");
    if (@offsetOf(RawPagerGateResult, "rollback_completed") != 124) @compileError("Native backing PagerGateResult.rollback_completed offset");
    if (@offsetOf(RawPagerGateResult, "commit_gate_enabled") != 128) @compileError("Native backing PagerGateResult.commit_gate_enabled offset");
    if (@offsetOf(RawPagerGateResult, "fault_gate_enabled") != 132) @compileError("Native backing PagerGateResult.fault_gate_enabled offset");
    if (@offsetOf(RawPagerGateResult, "pager_enabled") != 136) @compileError("Native backing PagerGateResult.pager_enabled offset");
    if (@offsetOf(RawPagerGateResult, "eviction_enabled") != 140) @compileError("Native backing PagerGateResult.eviction_enabled offset");
    if (@offsetOf(RawPagerGateResult, "page_in_enabled") != 144) @compileError("Native backing PagerGateResult.page_in_enabled offset");
    if (@offsetOf(RawPagerGateResult, "page_out_enabled") != 148) @compileError("Native backing PagerGateResult.page_out_enabled offset");
    if (@offsetOf(RawPagerGateResult, "slot_generation") != 152) @compileError("Native backing PagerGateResult.slot_generation offset");
    if (@offsetOf(RawPagerGateResult, "fault_count") != 160) @compileError("Native backing PagerGateResult.fault_count offset");
    if (@offsetOf(RawPagerGateResult, "failed_faults") != 168) @compileError("Native backing PagerGateResult.failed_faults offset");
    if (@offsetOf(RawPagerGateResult, "total_probes") != 176) @compileError("Native backing PagerGateResult.total_probes offset");
    if (@offsetOf(RawPagerGateResult, "total_ready") != 184) @compileError("Native backing PagerGateResult.total_ready offset");
    if (@offsetOf(RawPagerGateResult, "total_rollbacks") != 192) @compileError("Native backing PagerGateResult.total_rollbacks offset");
    if (@offsetOf(RawPagerGateResult, "total_failures") != 200) @compileError("Native backing PagerGateResult.total_failures offset");
    if (@sizeOf(RawPagerGateSummary) != 200 or @alignOf(RawPagerGateSummary) != 8) @compileError("Native backing PagerGateSummary geometry");
    if (@offsetOf(RawPagerGateSummary, "enabled") != 0) @compileError("Native backing PagerGateSummary.enabled offset");
    if (@offsetOf(RawPagerGateSummary, "last_status") != 4) @compileError("Native backing PagerGateSummary.last_status offset");
    if (@offsetOf(RawPagerGateSummary, "last_flags") != 8) @compileError("Native backing PagerGateSummary.last_flags offset");
    if (@offsetOf(RawPagerGateSummary, "last_blockers") != 12) @compileError("Native backing PagerGateSummary.last_blockers offset");
    if (@offsetOf(RawPagerGateSummary, "last_region_id") != 16) @compileError("Native backing PagerGateSummary.last_region_id offset");
    if (@offsetOf(RawPagerGateSummary, "last_owner_id") != 20) @compileError("Native backing PagerGateSummary.last_owner_id offset");
    if (@offsetOf(RawPagerGateSummary, "requested_bytes") != 24) @compileError("Native backing PagerGateSummary.requested_bytes offset");
    if (@offsetOf(RawPagerGateSummary, "committed_bytes") != 32) @compileError("Native backing PagerGateSummary.committed_bytes offset");
    if (@offsetOf(RawPagerGateSummary, "resident_bytes") != 40) @compileError("Native backing PagerGateSummary.resident_bytes offset");
    if (@offsetOf(RawPagerGateSummary, "nonresident_bytes") != 48) @compileError("Native backing PagerGateSummary.nonresident_bytes offset");
    if (@offsetOf(RawPagerGateSummary, "requested_slots") != 56) @compileError("Native backing PagerGateSummary.requested_slots offset");
    if (@offsetOf(RawPagerGateSummary, "prepared_slots") != 64) @compileError("Native backing PagerGateSummary.prepared_slots offset");
    if (@offsetOf(RawPagerGateSummary, "capacity_slots") != 72) @compileError("Native backing PagerGateSummary.capacity_slots offset");
    if (@offsetOf(RawPagerGateSummary, "free_before_slots") != 80) @compileError("Native backing PagerGateSummary.free_before_slots offset");
    if (@offsetOf(RawPagerGateSummary, "free_after_slots") != 88) @compileError("Native backing PagerGateSummary.free_after_slots offset");
    if (@offsetOf(RawPagerGateSummary, "reserved_before_slots") != 96) @compileError("Native backing PagerGateSummary.reserved_before_slots offset");
    if (@offsetOf(RawPagerGateSummary, "reserved_after_slots") != 104) @compileError("Native backing PagerGateSummary.reserved_after_slots offset");
    if (@offsetOf(RawPagerGateSummary, "last_reservation_id") != 112) @compileError("Native backing PagerGateSummary.last_reservation_id offset");
    if (@offsetOf(RawPagerGateSummary, "rollback_completed") != 116) @compileError("Native backing PagerGateSummary.rollback_completed offset");
    if (@offsetOf(RawPagerGateSummary, "commit_gate_enabled") != 120) @compileError("Native backing PagerGateSummary.commit_gate_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "fault_gate_enabled") != 124) @compileError("Native backing PagerGateSummary.fault_gate_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "pager_enabled") != 128) @compileError("Native backing PagerGateSummary.pager_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "eviction_enabled") != 132) @compileError("Native backing PagerGateSummary.eviction_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "page_in_enabled") != 136) @compileError("Native backing PagerGateSummary.page_in_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "page_out_enabled") != 140) @compileError("Native backing PagerGateSummary.page_out_enabled offset");
    if (@offsetOf(RawPagerGateSummary, "slot_generation") != 144) @compileError("Native backing PagerGateSummary.slot_generation offset");
    if (@offsetOf(RawPagerGateSummary, "fault_count") != 152) @compileError("Native backing PagerGateSummary.fault_count offset");
    if (@offsetOf(RawPagerGateSummary, "failed_faults") != 160) @compileError("Native backing PagerGateSummary.failed_faults offset");
    if (@offsetOf(RawPagerGateSummary, "probes") != 168) @compileError("Native backing PagerGateSummary.probes offset");
    if (@offsetOf(RawPagerGateSummary, "ready") != 176) @compileError("Native backing PagerGateSummary.ready offset");
    if (@offsetOf(RawPagerGateSummary, "rollbacks") != 184) @compileError("Native backing PagerGateSummary.rollbacks offset");
    if (@offsetOf(RawPagerGateSummary, "failures") != 192) @compileError("Native backing PagerGateSummary.failures offset");
    if (@sizeOf(RawPageIoInput) != 272 or @alignOf(RawPageIoInput) != 8) @compileError("Native backing PageIoInput geometry");
    if (@offsetOf(RawPageIoInput, "operation") != 0) @compileError("Native backing PageIoInput.operation offset");
    if (@offsetOf(RawPageIoInput, "region_id") != 4) @compileError("Native backing PageIoInput.region_id offset");
    if (@offsetOf(RawPageIoInput, "region_offset") != 8) @compileError("Native backing PageIoInput.region_offset offset");
    if (@offsetOf(RawPageIoInput, "reservation_id") != 16) @compileError("Native backing PageIoInput.reservation_id offset");
    if (@offsetOf(RawPageIoInput, "slot_index") != 24) @compileError("Native backing PageIoInput.slot_index offset");
    if (@offsetOf(RawPageIoInput, "page_count") != 32) @compileError("Native backing PageIoInput.page_count offset");
    if (@offsetOf(RawPageIoInput, "owner_kind") != 40) @compileError("Native backing PageIoInput.owner_kind offset");
    if (@offsetOf(RawPageIoInput, "owner_id") != 44) @compileError("Native backing PageIoInput.owner_id offset");
    if (@offsetOf(RawPageIoInput, "expected_generation") != 48) @compileError("Native backing PageIoInput.expected_generation offset");
    if (@offsetOf(RawPageIoInput, "flags") != 56) @compileError("Native backing PageIoInput.flags offset");
    if (@offsetOf(RawPageIoInput, "vm_region_exists") != 60) @compileError("Native backing PageIoInput.vm_region_exists offset");
    if (@offsetOf(RawPageIoInput, "vm_region_is_r4x") != 61) @compileError("Native backing PageIoInput.vm_region_is_r4x offset");
    if (@offsetOf(RawPageIoInput, "commit_covered") != 62) @compileError("Native backing PageIoInput.commit_covered offset");
    if (@offsetOf(RawPageIoInput, "committed_bytes") != 64) @compileError("Native backing PageIoInput.committed_bytes offset");
    if (@offsetOf(RawPageIoInput, "resident_bytes") != 72) @compileError("Native backing PageIoInput.resident_bytes offset");
    if (@offsetOf(RawPageIoInput, "backing") != 80) @compileError("Native backing PageIoInput.backing offset");
    if (@offsetOf(RawPageIoInput, "io_status") != 264) @compileError("Native backing PageIoInput.io_status offset");
    if (@offsetOf(RawPageIoInput, "io_bytes") != 268) @compileError("Native backing PageIoInput.io_bytes offset");
    if (@sizeOf(RawPageIoResult) != 296 or @alignOf(RawPageIoResult) != 8) @compileError("Native backing PageIoResult geometry");
    if (@offsetOf(RawPageIoResult, "version") != 0) @compileError("Native backing PageIoResult.version offset");
    if (@offsetOf(RawPageIoResult, "status") != 4) @compileError("Native backing PageIoResult.status offset");
    if (@offsetOf(RawPageIoResult, "operation") != 8) @compileError("Native backing PageIoResult.operation offset");
    if (@offsetOf(RawPageIoResult, "flags") != 12) @compileError("Native backing PageIoResult.flags offset");
    if (@offsetOf(RawPageIoResult, "blockers") != 16) @compileError("Native backing PageIoResult.blockers offset");
    if (@offsetOf(RawPageIoResult, "region_id") != 20) @compileError("Native backing PageIoResult.region_id offset");
    if (@offsetOf(RawPageIoResult, "reservation_id") != 24) @compileError("Native backing PageIoResult.reservation_id offset");
    if (@offsetOf(RawPageIoResult, "owner_kind") != 28) @compileError("Native backing PageIoResult.owner_kind offset");
    if (@offsetOf(RawPageIoResult, "owner_id") != 32) @compileError("Native backing PageIoResult.owner_id offset");
    if (@offsetOf(RawPageIoResult, "slot_bytes") != 36) @compileError("Native backing PageIoResult.slot_bytes offset");
    if (@offsetOf(RawPageIoResult, "reserved0") != 40) @compileError("Native backing PageIoResult.reserved0 offset");
    if (@offsetOf(RawPageIoResult, "region_offset") != 48) @compileError("Native backing PageIoResult.region_offset offset");
    if (@offsetOf(RawPageIoResult, "committed_bytes") != 56) @compileError("Native backing PageIoResult.committed_bytes offset");
    if (@offsetOf(RawPageIoResult, "resident_bytes") != 64) @compileError("Native backing PageIoResult.resident_bytes offset");
    if (@offsetOf(RawPageIoResult, "slot_index") != 72) @compileError("Native backing PageIoResult.slot_index offset");
    if (@offsetOf(RawPageIoResult, "page_count") != 80) @compileError("Native backing PageIoResult.page_count offset");
    if (@offsetOf(RawPageIoResult, "transfer_bytes") != 88) @compileError("Native backing PageIoResult.transfer_bytes offset");
    if (@offsetOf(RawPageIoResult, "expected_generation") != 96) @compileError("Native backing PageIoResult.expected_generation offset");
    if (@offsetOf(RawPageIoResult, "backing_slot") != 104) @compileError("Native backing PageIoResult.backing_slot offset");
    if (@offsetOf(RawPageIoResult, "backing_offset") != 112) @compileError("Native backing PageIoResult.backing_offset offset");
    if (@offsetOf(RawPageIoResult, "capacity_slots") != 120) @compileError("Native backing PageIoResult.capacity_slots offset");
    if (@offsetOf(RawPageIoResult, "reserved_slots") != 128) @compileError("Native backing PageIoResult.reserved_slots offset");
    if (@offsetOf(RawPageIoResult, "valid_slots") != 136) @compileError("Native backing PageIoResult.valid_slots offset");
    if (@offsetOf(RawPageIoResult, "dirty_slots") != 144) @compileError("Native backing PageIoResult.dirty_slots offset");
    if (@offsetOf(RawPageIoResult, "error_slots") != 152) @compileError("Native backing PageIoResult.error_slots offset");
    if (@offsetOf(RawPageIoResult, "io_bytes") != 160) @compileError("Native backing PageIoResult.io_bytes offset");
    if (@offsetOf(RawPageIoResult, "io_status") != 164) @compileError("Native backing PageIoResult.io_status offset");
    if (@offsetOf(RawPageIoResult, "pager_enabled") != 168) @compileError("Native backing PageIoResult.pager_enabled offset");
    if (@offsetOf(RawPageIoResult, "eviction_enabled") != 172) @compileError("Native backing PageIoResult.eviction_enabled offset");
    if (@offsetOf(RawPageIoResult, "page_in_enabled") != 176) @compileError("Native backing PageIoResult.page_in_enabled offset");
    if (@offsetOf(RawPageIoResult, "page_out_enabled") != 180) @compileError("Native backing PageIoResult.page_out_enabled offset");
    if (@offsetOf(RawPageIoResult, "retry_limit") != 184) @compileError("Native backing PageIoResult.retry_limit offset");
    if (@offsetOf(RawPageIoResult, "backoff_ticks") != 188) @compileError("Native backing PageIoResult.backoff_ticks offset");
    if (@offsetOf(RawPageIoResult, "slot_generation") != 192) @compileError("Native backing PageIoResult.slot_generation offset");
    if (@offsetOf(RawPageIoResult, "total_prepares") != 200) @compileError("Native backing PageIoResult.total_prepares offset");
    if (@offsetOf(RawPageIoResult, "total_page_outs") != 208) @compileError("Native backing PageIoResult.total_page_outs offset");
    if (@offsetOf(RawPageIoResult, "total_page_ins") != 216) @compileError("Native backing PageIoResult.total_page_ins offset");
    if (@offsetOf(RawPageIoResult, "total_failures") != 224) @compileError("Native backing PageIoResult.total_failures offset");
    if (@offsetOf(RawPageIoResult, "total_retry_attempts") != 232) @compileError("Native backing PageIoResult.total_retry_attempts offset");
    if (@offsetOf(RawPageIoResult, "total_retryable_failures") != 240) @compileError("Native backing PageIoResult.total_retryable_failures offset");
    if (@offsetOf(RawPageIoResult, "total_permanent_failures") != 248) @compileError("Native backing PageIoResult.total_permanent_failures offset");
    if (@offsetOf(RawPageIoResult, "total_retry_limit_hits") != 256) @compileError("Native backing PageIoResult.total_retry_limit_hits offset");
    if (@offsetOf(RawPageIoResult, "total_failed_page_outs") != 264) @compileError("Native backing PageIoResult.total_failed_page_outs offset");
    if (@offsetOf(RawPageIoResult, "total_failed_page_ins") != 272) @compileError("Native backing PageIoResult.total_failed_page_ins offset");
    if (@offsetOf(RawPageIoResult, "total_data_preserved_pages") != 280) @compileError("Native backing PageIoResult.total_data_preserved_pages offset");
    if (@offsetOf(RawPageIoResult, "total_data_lost_pages") != 288) @compileError("Native backing PageIoResult.total_data_lost_pages offset");
    if (@sizeOf(RawPageIoSummary) != 264 or @alignOf(RawPageIoSummary) != 8) @compileError("Native backing PageIoSummary geometry");
    if (@offsetOf(RawPageIoSummary, "enabled") != 0) @compileError("Native backing PageIoSummary.enabled offset");
    if (@offsetOf(RawPageIoSummary, "last_status") != 4) @compileError("Native backing PageIoSummary.last_status offset");
    if (@offsetOf(RawPageIoSummary, "last_operation") != 8) @compileError("Native backing PageIoSummary.last_operation offset");
    if (@offsetOf(RawPageIoSummary, "last_flags") != 12) @compileError("Native backing PageIoSummary.last_flags offset");
    if (@offsetOf(RawPageIoSummary, "last_blockers") != 16) @compileError("Native backing PageIoSummary.last_blockers offset");
    if (@offsetOf(RawPageIoSummary, "last_region_id") != 20) @compileError("Native backing PageIoSummary.last_region_id offset");
    if (@offsetOf(RawPageIoSummary, "last_reservation_id") != 24) @compileError("Native backing PageIoSummary.last_reservation_id offset");
    if (@offsetOf(RawPageIoSummary, "last_owner_kind") != 28) @compileError("Native backing PageIoSummary.last_owner_kind offset");
    if (@offsetOf(RawPageIoSummary, "last_owner_id") != 32) @compileError("Native backing PageIoSummary.last_owner_id offset");
    if (@offsetOf(RawPageIoSummary, "region_offset") != 40) @compileError("Native backing PageIoSummary.region_offset offset");
    if (@offsetOf(RawPageIoSummary, "page_count") != 48) @compileError("Native backing PageIoSummary.page_count offset");
    if (@offsetOf(RawPageIoSummary, "transfer_bytes") != 56) @compileError("Native backing PageIoSummary.transfer_bytes offset");
    if (@offsetOf(RawPageIoSummary, "expected_generation") != 64) @compileError("Native backing PageIoSummary.expected_generation offset");
    if (@offsetOf(RawPageIoSummary, "backing_slot") != 72) @compileError("Native backing PageIoSummary.backing_slot offset");
    if (@offsetOf(RawPageIoSummary, "backing_offset") != 80) @compileError("Native backing PageIoSummary.backing_offset offset");
    if (@offsetOf(RawPageIoSummary, "io_bytes") != 88) @compileError("Native backing PageIoSummary.io_bytes offset");
    if (@offsetOf(RawPageIoSummary, "io_status") != 92) @compileError("Native backing PageIoSummary.io_status offset");
    if (@offsetOf(RawPageIoSummary, "capacity_slots") != 96) @compileError("Native backing PageIoSummary.capacity_slots offset");
    if (@offsetOf(RawPageIoSummary, "reserved_slots") != 104) @compileError("Native backing PageIoSummary.reserved_slots offset");
    if (@offsetOf(RawPageIoSummary, "valid_slots") != 112) @compileError("Native backing PageIoSummary.valid_slots offset");
    if (@offsetOf(RawPageIoSummary, "dirty_slots") != 120) @compileError("Native backing PageIoSummary.dirty_slots offset");
    if (@offsetOf(RawPageIoSummary, "error_slots") != 128) @compileError("Native backing PageIoSummary.error_slots offset");
    if (@offsetOf(RawPageIoSummary, "pager_enabled") != 136) @compileError("Native backing PageIoSummary.pager_enabled offset");
    if (@offsetOf(RawPageIoSummary, "eviction_enabled") != 140) @compileError("Native backing PageIoSummary.eviction_enabled offset");
    if (@offsetOf(RawPageIoSummary, "page_in_enabled") != 144) @compileError("Native backing PageIoSummary.page_in_enabled offset");
    if (@offsetOf(RawPageIoSummary, "page_out_enabled") != 148) @compileError("Native backing PageIoSummary.page_out_enabled offset");
    if (@offsetOf(RawPageIoSummary, "retry_limit") != 152) @compileError("Native backing PageIoSummary.retry_limit offset");
    if (@offsetOf(RawPageIoSummary, "backoff_ticks") != 156) @compileError("Native backing PageIoSummary.backoff_ticks offset");
    if (@offsetOf(RawPageIoSummary, "slot_generation") != 160) @compileError("Native backing PageIoSummary.slot_generation offset");
    if (@offsetOf(RawPageIoSummary, "prepares") != 168) @compileError("Native backing PageIoSummary.prepares offset");
    if (@offsetOf(RawPageIoSummary, "page_outs") != 176) @compileError("Native backing PageIoSummary.page_outs offset");
    if (@offsetOf(RawPageIoSummary, "page_ins") != 184) @compileError("Native backing PageIoSummary.page_ins offset");
    if (@offsetOf(RawPageIoSummary, "failures") != 192) @compileError("Native backing PageIoSummary.failures offset");
    if (@offsetOf(RawPageIoSummary, "retry_attempts") != 200) @compileError("Native backing PageIoSummary.retry_attempts offset");
    if (@offsetOf(RawPageIoSummary, "retryable_failures") != 208) @compileError("Native backing PageIoSummary.retryable_failures offset");
    if (@offsetOf(RawPageIoSummary, "permanent_failures") != 216) @compileError("Native backing PageIoSummary.permanent_failures offset");
    if (@offsetOf(RawPageIoSummary, "retry_limit_hits") != 224) @compileError("Native backing PageIoSummary.retry_limit_hits offset");
    if (@offsetOf(RawPageIoSummary, "failed_page_outs") != 232) @compileError("Native backing PageIoSummary.failed_page_outs offset");
    if (@offsetOf(RawPageIoSummary, "failed_page_ins") != 240) @compileError("Native backing PageIoSummary.failed_page_ins offset");
    if (@offsetOf(RawPageIoSummary, "data_preserved_pages") != 248) @compileError("Native backing PageIoSummary.data_preserved_pages offset");
    if (@offsetOf(RawPageIoSummary, "data_lost_pages") != 256) @compileError("Native backing PageIoSummary.data_lost_pages offset");
    if (@sizeOf(RawLifecycleCleanupResult) != 48 or @alignOf(RawLifecycleCleanupResult) != 8) @compileError("Native backing LifecycleCleanupResult geometry");
    if (@offsetOf(RawLifecycleCleanupResult, "cleanup_count") != 0) @compileError("Native backing LifecycleCleanupResult.cleanup_count offset");
    if (@offsetOf(RawLifecycleCleanupResult, "released_ranges") != 8) @compileError("Native backing LifecycleCleanupResult.released_ranges offset");
    if (@offsetOf(RawLifecycleCleanupResult, "released_slots") != 16) @compileError("Native backing LifecycleCleanupResult.released_slots offset");
    if (@offsetOf(RawLifecycleCleanupResult, "released_valid_slots") != 24) @compileError("Native backing LifecycleCleanupResult.released_valid_slots offset");
    if (@offsetOf(RawLifecycleCleanupResult, "released_dirty_slots") != 32) @compileError("Native backing LifecycleCleanupResult.released_dirty_slots offset");
    if (@offsetOf(RawLifecycleCleanupResult, "released_error_slots") != 40) @compileError("Native backing LifecycleCleanupResult.released_error_slots offset");
}
test "backing identity cannot replace live reservations" {
    const old = probe(.{ .path = "C:\\TEMP\\OLD.BIN", .file_exists = true, .fat32 = true, .file_size = 8192, .requested_bytes = 8192, .cluster_bytes = 4096, .first_cluster = 40 });
    const reserved = slotProbe(.{ .backing = old, .operation = slot_operation_reserve, .requested_slots = 1, .owner_kind = slot_owner_kind_vm_region, .owner_id = 7, .region_id = 9 });
    try std.testing.expectEqual(slot_status_reserved, reserved.status);
    const replacement = probe(.{ .path = "C:\\TEMP\\NEW.BIN", .file_exists = true, .fat32 = true, .file_size = 16384, .requested_bytes = 16384, .cluster_bytes = 4096, .first_cluster = 42 });
    const denied = slotProbe(.{ .backing = replacement });
    try std.testing.expectEqual(slot_status_backing_unavailable, denied.status);
    try std.testing.expectEqual(@as(u64, 1), denied.reserved_slots);
    try std.testing.expectEqualStrings("C:\\TEMP\\OLD.BIN", std.mem.sliceTo(activeBackingPath().?, 0));
    const released = slotProbe(.{ .backing = old, .operation = slot_operation_release, .reservation_id = reserved.reservation_id, .owner_kind = slot_owner_kind_vm_region, .owner_id = 7, .region_id = 9 });
    try std.testing.expectEqual(slot_status_released, released.status);
    const accepted = slotProbe(.{ .backing = replacement });
    try std.testing.expectEqual(slot_status_ready, accepted.status);
    try std.testing.expectEqual(@as(u64, 0), accepted.reserved_slots);
    try std.testing.expectEqualStrings("C:\\TEMP\\NEW.BIN", std.mem.sliceTo(activeBackingPath().?, 0));
}
