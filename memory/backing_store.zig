const std = @import("std");

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

const SlotExtent = struct {
    reservation_id: u32 = 0,
    owner_kind: u32 = 0,
    owner_id: u32 = 0,
    region_id: u32 = 0,
    start_slot: u64 = 0,
    slot_count: u64 = 0,
    state: u32 = 0,
};

var state: Summary = .{};
var slot_state: SlotSummary = .{};
var pager_gate_state: PagerGateSummary = .{};
var page_io_state: PageIoSummary = .{};
var slot_extents: [SLOT_TABLE_MAX_RANGES]SlotExtent = .{SlotExtent{}} ** SLOT_TABLE_MAX_RANGES;
var active_first_cluster: u32 = 0;
var active_file_size: u64 = 0;
var active_cluster_bytes: u32 = 0;
var active_capacity_slots: u64 = 0;
var active_backing_path: [MAX_BACKING_PATH:0]u8 = .{0} ** MAX_BACKING_PATH;
var active_backing_path_valid = false;
var slot_generation: u64 = 0;
var next_reservation_id: u32 = 1;

pub fn probe(input: Input) Result {
    var result = Result{
        .requested_bytes = input.requested_bytes,
        .file_size = input.file_size,
        .cluster_bytes = input.cluster_bytes,
        .first_cluster = input.first_cluster,
    };

    if ((input.flags & ~supported_probe_flags) != 0) {
        result.status = status_unsupported_flags;
        result.blockers |= blocker_unsupported_flags;
        record(result);
        return result;
    }

    if (input.requested_bytes == 0) {
        result.status = status_invalid_request;
        result.blockers |= blocker_invalid_request;
        record(result);
        return result;
    }

    if ((input.requested_bytes & (PAGE_BYTES - 1)) != 0) {
        result.status = status_invalid_request;
        result.blockers |= blocker_unaligned_request;
        record(result);
        return result;
    }
    result.flags |= flag_page_aligned_request;

    if (!input.fat32) {
        result.status = status_unsupported_fs;
        result.blockers |= blocker_unsupported_fs;
        record(result);
        return result;
    }
    result.flags |= flag_fat32;

    if (!input.file_exists) {
        result.status = status_missing_file;
        result.blockers |= blocker_missing_file;
        record(result);
        return result;
    }

    if (input.is_dir) {
        result.status = status_directory;
        result.blockers |= blocker_directory;
        record(result);
        return result;
    }

    result.flags |= flag_file_backed | flag_existing_file;
    result.available_bytes = input.file_size;
    if (input.file_size < input.requested_bytes) {
        result.status = status_too_small;
        result.blockers |= blocker_too_small;
        record(result);
        return result;
    }

    result.status = status_ready;
    rememberActivePath(input.path);
    record(result);
    return result;
}

pub fn summary() Summary {
    return state;
}

pub fn activeBackingPath() ?[*:0]const u8 {
    if (!active_backing_path_valid) return null;
    return @ptrCast(&active_backing_path);
}

pub fn activeBackingResult() ?Result {
    if (!active_backing_path_valid or active_capacity_slots == 0 or active_file_size == 0) return null;
    const available_bytes = active_capacity_slots * SLOT_BYTES;
    return .{
        .status = status_ready,
        .flags = flag_file_backed |
            flag_existing_file |
            flag_fat32 |
            flag_reserve_only |
            flag_pager_disabled |
            flag_uses_fs_api |
            flag_no_second_io_path |
            flag_page_aligned_request,
        .requested_bytes = available_bytes,
        .available_bytes = available_bytes,
        .file_size = active_file_size,
        .cluster_bytes = active_cluster_bytes,
        .first_cluster = active_first_cluster,
    };
}

pub fn slotProbe(input: SlotInput) SlotResult {
    var result = SlotResult{
        .operation = input.operation,
        .requested_slots = input.requested_slots,
        .reservation_id = input.reservation_id,
        .owner_kind = input.owner_kind,
        .owner_id = input.owner_id,
        .region_id = input.region_id,
    };

    if ((input.flags & ~supported_slot_flags) != 0) {
        result.status = slot_status_unsupported_flags;
        result.blockers |= slot_blocker_unsupported_flags;
        recordSlot(&result);
        return result;
    }

    if (!validSlotOwnerKind(input.owner_kind)) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_invalid_owner;
        recordSlot(&result);
        return result;
    }

    if (input.backing.status != status_ready) {
        result.status = slot_status_backing_unavailable;
        result.blockers |= slot_blocker_backing_not_ready;
        recordSlot(&result);
        return result;
    }
    result.flags |= slot_flag_file_backed | slot_flag_backing_ready;

    if ((input.backing.available_bytes & (SLOT_BYTES - 1)) != 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_unaligned_backing;
        recordSlot(&result);
        return result;
    }

    const capacity_slots = input.backing.available_bytes / SLOT_BYTES;
    if (capacity_slots == 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_zero_capacity;
        recordSlot(&result);
        return result;
    }

    ensureSlotBacking(input.backing, capacity_slots);
    result.capacity_slots = active_capacity_slots;
    fillSlotCounts(&result);

    switch (input.operation) {
        slot_operation_probe => {
            result.status = slot_status_ready;
        },
        slot_operation_reserve => reserveSlots(input, &result),
        slot_operation_release => releaseSlots(input, &result),
        slot_operation_mark_error => markSlotsError(input, &result),
        slot_operation_recover => recoverSlots(input, &result),
        else => {
            result.status = slot_status_unsupported_operation;
            result.blockers |= slot_blocker_unsupported_operation;
        },
    }

    fillSlotCounts(&result);
    recordSlot(&result);
    return result;
}

pub fn slotSummary() SlotSummary {
    return slot_state;
}

pub const LifecycleCleanupResult = struct {
    cleanup_count: u64 = 0,
    released_ranges: u64 = 0,
    released_slots: u64 = 0,
    released_valid_slots: u64 = 0,
    released_dirty_slots: u64 = 0,
    released_error_slots: u64 = 0,
};

pub fn releaseR4xOwner(owner_id: u32) LifecycleCleanupResult {
    var result = cleanupOwner(slot_owner_kind_r4x_instance, owner_id);
    const vm_result = cleanupOwner(slot_owner_kind_vm_region, owner_id);
    result.cleanup_count +%= vm_result.cleanup_count;
    result.released_ranges +%= vm_result.released_ranges;
    result.released_slots +%= vm_result.released_slots;
    result.released_valid_slots +%= vm_result.released_valid_slots;
    result.released_dirty_slots +%= vm_result.released_dirty_slots;
    result.released_error_slots +%= vm_result.released_error_slots;
    return result;
}

pub fn releaseVmRegion(owner_id: u32, region_id: u32) LifecycleCleanupResult {
    var result = cleanupRegion(slot_owner_kind_vm_region, owner_id, region_id);
    const instance_result = cleanupRegion(slot_owner_kind_r4x_instance, owner_id, region_id);
    result.cleanup_count +%= instance_result.cleanup_count;
    result.released_ranges +%= instance_result.released_ranges;
    result.released_slots +%= instance_result.released_slots;
    result.released_valid_slots +%= instance_result.released_valid_slots;
    result.released_dirty_slots +%= instance_result.released_dirty_slots;
    result.released_error_slots +%= instance_result.released_error_slots;
    return result;
}

pub fn pagerGateProbe(input: PagerGateInput) PagerGateResult {
    const nonresident_bytes = if (input.committed_bytes > input.resident_bytes) input.committed_bytes - input.resident_bytes else 0;
    var result = PagerGateResult{
        .region_id = input.region_id,
        .owner_id = input.owner_id,
        .requested_bytes = if (input.requested_bytes != 0) input.requested_bytes else nonresident_bytes,
        .committed_bytes = input.committed_bytes,
        .resident_bytes = input.resident_bytes,
        .nonresident_bytes = nonresident_bytes,
        .fault_count = input.fault_count,
        .failed_faults = input.failed_faults,
    };

    if ((input.flags & ~supported_pager_gate_flags) != 0) {
        result.status = pager_gate_status_unsupported_flags;
        result.blockers |= pager_gate_blocker_unsupported_flags;
        recordPagerGate(&result);
        return result;
    }

    if (!input.vm_region_exists) {
        result.status = pager_gate_status_vm_region_missing;
        result.blockers |= pager_gate_blocker_vm_region_missing;
        recordPagerGate(&result);
        return result;
    }
    result.flags |= pager_gate_flag_vm_region_attached;

    if (!input.vm_region_is_r4x) {
        result.status = pager_gate_status_invalid_request;
        result.blockers |= pager_gate_blocker_vm_region_not_r4x;
        recordPagerGate(&result);
        return result;
    }

    if (input.backing.status != status_ready) {
        result.status = pager_gate_status_backing_unavailable;
        result.blockers |= pager_gate_blocker_backing_not_ready;
        recordPagerGate(&result);
        return result;
    }
    result.flags |= pager_gate_flag_file_backed | pager_gate_flag_backing_ready;

    if (result.requested_bytes == 0) {
        result.status = pager_gate_status_no_nonresident_commit;
        result.blockers |= pager_gate_blocker_no_nonresident_commit;
        recordPagerGate(&result);
        return result;
    }

    if ((result.requested_bytes & (PAGE_BYTES - 1)) != 0) {
        result.status = pager_gate_status_invalid_request;
        result.blockers |= pager_gate_blocker_unaligned_request;
        recordPagerGate(&result);
        return result;
    }

    const requested_slots = result.requested_bytes / SLOT_BYTES;
    result.requested_slots = requested_slots;
    const probe_result = slotProbe(.{
        .operation = slot_operation_probe,
        .requested_slots = 0,
        .reservation_id = 0,
        .owner_id = input.owner_id,
        .flags = 0,
        .backing = input.backing,
    });
    result.capacity_slots = probe_result.capacity_slots;
    result.free_before_slots = probe_result.free_slots;
    result.reserved_before_slots = probe_result.reserved_slots;
    result.free_after_slots = probe_result.free_slots;
    result.reserved_after_slots = probe_result.reserved_slots;
    result.slot_generation = probe_result.generation;

    if (probe_result.status != slot_status_ready) {
        mapSlotFailureToPagerGate(probe_result, &result);
        recordPagerGate(&result);
        return result;
    }

    if (requested_slots > probe_result.free_slots) {
        result.status = pager_gate_status_insufficient_capacity;
        result.blockers |= pager_gate_blocker_insufficient_capacity;
        recordPagerGate(&result);
        return result;
    }

    const reserve_result = slotProbe(.{
        .operation = slot_operation_reserve,
        .requested_slots = requested_slots,
        .reservation_id = 0,
        .owner_id = input.owner_id,
        .flags = 0,
        .backing = input.backing,
    });
    result.capacity_slots = reserve_result.capacity_slots;
    result.free_after_slots = reserve_result.free_slots;
    result.reserved_after_slots = reserve_result.reserved_slots;
    result.slot_generation = reserve_result.generation;
    result.slot_reservation_id = reserve_result.reservation_id;

    if (reserve_result.status != slot_status_reserved) {
        mapSlotFailureToPagerGate(reserve_result, &result);
        recordPagerGate(&result);
        return result;
    }
    result.flags |= pager_gate_flag_slot_reservation_tested;
    result.prepared_slots = reserve_result.slot_count;

    const release_result = slotProbe(.{
        .operation = slot_operation_release,
        .requested_slots = 0,
        .reservation_id = reserve_result.reservation_id,
        .owner_id = input.owner_id,
        .flags = 0,
        .backing = input.backing,
    });
    result.capacity_slots = release_result.capacity_slots;
    result.free_after_slots = release_result.free_slots;
    result.reserved_after_slots = release_result.reserved_slots;
    result.slot_generation = release_result.generation;

    if (release_result.status != slot_status_released) {
        result.status = pager_gate_status_rollback_failed;
        result.blockers |= pager_gate_blocker_rollback_failed;
        recordPagerGate(&result);
        return result;
    }

    result.status = pager_gate_status_ready;
    result.flags |= pager_gate_flag_rollback_complete;
    result.rollback_completed = 1;
    result.slot_reservation_id = release_result.reservation_id;

    const final_probe = slotProbe(.{
        .operation = slot_operation_probe,
        .requested_slots = 0,
        .reservation_id = 0,
        .owner_id = input.owner_id,
        .flags = 0,
        .backing = input.backing,
    });
    result.capacity_slots = final_probe.capacity_slots;
    result.free_after_slots = final_probe.free_slots;
    result.reserved_after_slots = final_probe.reserved_slots;
    result.slot_generation = final_probe.generation;

    recordPagerGate(&result);
    return result;
}

pub fn pagerGateSummary() PagerGateSummary {
    return pager_gate_state;
}

pub fn pageIoPrepare(input: PageIoInput) PageIoResult {
    var result = makePageIoResult(input);
    const extent_index = validatePageIo(input, &result) orelse {
        recordPageIo(&result);
        return result;
    };
    const extent = slot_extents[extent_index];
    fillPageIoSlotState(&result);
    result.status = page_io_status_ready;
    result.backing_slot = extent.start_slot + input.slot_index;
    result.backing_offset = result.backing_slot * SLOT_BYTES;
    recordPageIo(&result);
    return result;
}

pub fn pageIoComplete(input: PageIoInput) PageIoResult {
    var result = makePageIoResult(input);
    const extent_index = validatePageIo(input, &result) orelse {
        recordPageIo(&result);
        return result;
    };
    var extent = &slot_extents[extent_index];
    fillPageIoSlotState(&result);
    result.backing_slot = extent.start_slot + input.slot_index;
    result.backing_offset = result.backing_slot * SLOT_BYTES;
    result.io_status = input.io_status;
    result.io_bytes = input.io_bytes;

    const expected_io_status: i32 = @intCast(result.transfer_bytes);
    if (input.io_status != expected_io_status or @as(u64, input.io_bytes) != result.transfer_bytes) {
        extent.state |= extent_state_error;
        slot_generation +%= 1;
        result.slot_generation = slot_generation;
        if (input.io_status > 0 or input.io_bytes != 0) {
            result.status = page_io_status_partial_io;
            result.blockers |= page_io_blocker_partial_io;
        } else {
            result.status = page_io_status_io_failed;
            result.blockers |= page_io_blocker_io_failed;
        }
        fillPageIoSlotState(&result);
        recordPageIo(&result);
        return result;
    }

    switch (input.operation) {
        page_io_operation_page_out => {
            extent.state |= extent_state_valid;
            extent.state &= ~extent_state_dirty;
            slot_generation +%= 1;
            result.status = page_io_status_page_out_ok;
            result.flags |= page_io_flag_slot_valid | page_io_flag_slot_clean;
        },
        page_io_operation_page_in => {
            result.status = page_io_status_page_in_ok;
        },
        else => {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_request;
        },
    }
    result.slot_generation = slot_generation;
    fillPageIoSlotState(&result);
    recordPageIo(&result);
    return result;
}

pub fn pageIoSummary() PageIoSummary {
    return page_io_state;
}

fn record(result: Result) void {
    state.enabled = true;
    state.probes +%= 1;
    if (result.status == status_ready) {
        state.ready +%= 1;
    } else {
        state.failures +%= 1;
    }
    state.last_status = result.status;
    state.last_flags = result.flags;
    state.last_blockers = result.blockers;
    state.last_requested_bytes = result.requested_bytes;
    state.last_available_bytes = result.available_bytes;
    state.last_file_size = result.file_size;
    state.last_cluster_bytes = result.cluster_bytes;
    state.last_first_cluster = result.first_cluster;
}

fn ensureSlotBacking(backing: Result, capacity_slots: u64) void {
    if (active_first_cluster == backing.first_cluster and
        active_file_size == backing.file_size and
        active_capacity_slots == capacity_slots)
    {
        return;
    }

    slot_extents = .{SlotExtent{}} ** SLOT_TABLE_MAX_RANGES;
    active_first_cluster = backing.first_cluster;
    active_file_size = backing.file_size;
    active_cluster_bytes = backing.cluster_bytes;
    active_capacity_slots = capacity_slots;
    slot_generation +%= 1;
}

fn rememberActivePath(path: ?[*:0]const u8) void {
    const path_z = path orelse {
        active_backing_path_valid = false;
        active_backing_path[0] = 0;
        return;
    };
    var len: usize = 0;
    while (len + 1 < active_backing_path.len and path_z[len] != 0) : (len += 1) {
        active_backing_path[len] = path_z[len];
    }
    if (path_z[len] != 0) {
        active_backing_path_valid = false;
        active_backing_path[0] = 0;
        return;
    }
    active_backing_path[len] = 0;
    var clear = len + 1;
    while (clear < active_backing_path.len) : (clear += 1) active_backing_path[clear] = 0;
    active_backing_path_valid = true;
}

fn reserveSlots(input: SlotInput, result: *SlotResult) void {
    if (input.requested_slots == 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_invalid_request;
        return;
    }

    const reserved = countReservedSlots();
    const free = if (active_capacity_slots >= reserved) active_capacity_slots - reserved else 0;
    if (input.requested_slots > free) {
        result.status = slot_status_insufficient_capacity;
        result.blockers |= slot_blocker_insufficient_capacity;
        return;
    }

    const extent_index = findFreeExtentIndex() orelse {
        result.status = slot_status_table_full;
        result.blockers |= slot_blocker_table_full;
        return;
    };
    const first_slot = findFreeSlotRange(input.requested_slots, active_capacity_slots) orelse {
        result.status = slot_status_insufficient_capacity;
        result.blockers |= slot_blocker_insufficient_capacity;
        return;
    };
    const reservation_id = allocateReservationId() orelse {
        result.status = slot_status_table_full;
        result.blockers |= slot_blocker_table_full;
        return;
    };

    slot_extents[extent_index] = .{
        .reservation_id = reservation_id,
        .owner_kind = input.owner_kind,
        .owner_id = input.owner_id,
        .region_id = input.region_id,
        .start_slot = first_slot,
        .slot_count = input.requested_slots,
        .state = extent_state_reserved,
    };
    slot_generation +%= 1;

    result.status = slot_status_reserved;
    result.reservation_id = reservation_id;
    result.owner_kind = input.owner_kind;
    result.owner_id = input.owner_id;
    result.region_id = input.region_id;
    result.first_slot = first_slot;
    result.slot_count = input.requested_slots;
}

fn releaseSlots(input: SlotInput, result: *SlotResult) void {
    if (input.reservation_id == 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_invalid_request;
        return;
    }

    const index = findExtentByReservation(input.reservation_id) orelse {
        result.status = slot_status_reservation_not_found;
        result.blockers |= slot_blocker_reservation_not_found;
        return;
    };

    const extent = slot_extents[index];
    if (!ownerMatches(extent, input.owner_kind, input.owner_id, input.region_id)) {
        result.status = slot_status_owner_mismatch;
        result.blockers |= slot_blocker_owner_mismatch;
        result.reservation_id = extent.reservation_id;
        result.owner_kind = extent.owner_kind;
        result.owner_id = extent.owner_id;
        result.region_id = extent.region_id;
        result.first_slot = extent.start_slot;
        result.slot_count = extent.slot_count;
        return;
    }

    result.status = slot_status_released;
    result.reservation_id = extent.reservation_id;
    result.owner_kind = extent.owner_kind;
    result.owner_id = extent.owner_id;
    result.region_id = extent.region_id;
    result.first_slot = extent.start_slot;
    result.slot_count = extent.slot_count;
    slot_extents[index] = .{};
    slot_generation +%= 1;
}

fn markSlotsError(input: SlotInput, result: *SlotResult) void {
    if (input.reservation_id == 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_invalid_request;
        return;
    }

    const index = findExtentByReservation(input.reservation_id) orelse {
        result.status = slot_status_reservation_not_found;
        result.blockers |= slot_blocker_reservation_not_found;
        return;
    };

    const before = slot_extents[index];
    if (!ownerMatches(before, input.owner_kind, input.owner_id, input.region_id)) {
        result.status = slot_status_owner_mismatch;
        result.blockers |= slot_blocker_owner_mismatch;
        result.reservation_id = before.reservation_id;
        result.owner_kind = before.owner_kind;
        result.owner_id = before.owner_id;
        result.region_id = before.region_id;
        result.first_slot = before.start_slot;
        result.slot_count = before.slot_count;
        return;
    }

    slot_extents[index].state |= extent_state_error;
    const extent = slot_extents[index];
    result.status = slot_status_error_marked;
    result.reservation_id = extent.reservation_id;
    result.owner_kind = extent.owner_kind;
    result.owner_id = extent.owner_id;
    result.region_id = extent.region_id;
    result.first_slot = extent.start_slot;
    result.slot_count = extent.slot_count;
    slot_generation +%= 1;
}

fn recoverSlots(input: SlotInput, result: *SlotResult) void {
    if (input.reservation_id == 0) {
        result.status = slot_status_invalid_request;
        result.blockers |= slot_blocker_invalid_request;
        return;
    }

    const index = findExtentByReservation(input.reservation_id) orelse {
        result.status = slot_status_reservation_not_found;
        result.blockers |= slot_blocker_reservation_not_found;
        return;
    };

    const before = slot_extents[index];
    if (!ownerMatches(before, input.owner_kind, input.owner_id, input.region_id)) {
        result.status = slot_status_owner_mismatch;
        result.blockers |= slot_blocker_owner_mismatch;
        result.reservation_id = before.reservation_id;
        result.owner_kind = before.owner_kind;
        result.owner_id = before.owner_id;
        result.region_id = before.region_id;
        result.first_slot = before.start_slot;
        result.slot_count = before.slot_count;
        return;
    }

    slot_extents[index].state = extent_state_reserved;
    const extent = slot_extents[index];
    result.status = slot_status_recovered;
    result.reservation_id = extent.reservation_id;
    result.owner_kind = extent.owner_kind;
    result.owner_id = extent.owner_id;
    result.region_id = extent.region_id;
    result.first_slot = extent.start_slot;
    result.slot_count = extent.slot_count;
    slot_generation +%= 1;
}

fn fillSlotCounts(result: *SlotResult) void {
    var reserved: u64 = 0;
    var valid: u64 = 0;
    var dirty: u64 = 0;
    var errors: u64 = 0;
    var ranges: u32 = 0;

    var index: usize = 0;
    while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
        const extent = slot_extents[index];
        if (!extentActive(extent)) continue;
        ranges += 1;
        reserved +%= extent.slot_count;
        if ((extent.state & extent_state_valid) != 0) valid +%= extent.slot_count;
        if ((extent.state & extent_state_dirty) != 0) dirty +%= extent.slot_count;
        if ((extent.state & extent_state_error) != 0) errors +%= extent.slot_count;
    }

    result.capacity_slots = active_capacity_slots;
    result.reserved_slots = reserved;
    result.free_slots = if (active_capacity_slots >= reserved) active_capacity_slots - reserved else 0;
    result.valid_slots = valid;
    result.dirty_slots = dirty;
    result.error_slots = errors;
    result.range_count = ranges;
    result.max_ranges = slot_table_max_ranges;
    result.generation = slot_generation;
}

fn recordSlot(result: *SlotResult) void {
    slot_state.enabled = true;
    slot_state.probes +%= 1;

    switch (result.status) {
        slot_status_reserved => slot_state.reserves +%= 1,
        slot_status_released => slot_state.releases +%= 1,
        slot_status_error_marked => slot_state.error_marks +%= 1,
        slot_status_recovered => slot_state.recoveries +%= 1,
        slot_status_ready => {},
        else => slot_state.failures +%= 1,
    }

    result.total_probes = slot_state.probes;
    result.total_reserves = slot_state.reserves;
    result.total_releases = slot_state.releases;
    result.total_error_marks = slot_state.error_marks;
    result.total_recoveries = slot_state.recoveries;
    result.total_failures = slot_state.failures;

    slot_state.last_status = result.status;
    slot_state.last_operation = result.operation;
    slot_state.last_flags = result.flags;
    slot_state.last_blockers = result.blockers;
    slot_state.capacity_slots = result.capacity_slots;
    slot_state.reserved_slots = result.reserved_slots;
    slot_state.free_slots = result.free_slots;
    slot_state.valid_slots = result.valid_slots;
    slot_state.dirty_slots = result.dirty_slots;
    slot_state.error_slots = result.error_slots;
    slot_state.range_count = result.range_count;
    slot_state.max_ranges = result.max_ranges;
    slot_state.last_reservation_id = result.reservation_id;
    slot_state.last_owner_kind = result.owner_kind;
    slot_state.last_owner_id = result.owner_id;
    slot_state.last_region_id = result.region_id;
    slot_state.last_first_slot = result.first_slot;
    slot_state.last_slot_count = result.slot_count;
    slot_state.generation = result.generation;
}

fn recordPagerGate(result: *PagerGateResult) void {
    pager_gate_state.enabled = true;
    pager_gate_state.probes +%= 1;
    if (result.status == pager_gate_status_ready) {
        pager_gate_state.ready +%= 1;
        if (result.rollback_completed != 0) pager_gate_state.rollbacks +%= 1;
    } else {
        pager_gate_state.failures +%= 1;
    }

    result.total_probes = pager_gate_state.probes;
    result.total_ready = pager_gate_state.ready;
    result.total_rollbacks = pager_gate_state.rollbacks;
    result.total_failures = pager_gate_state.failures;

    pager_gate_state.last_status = result.status;
    pager_gate_state.last_flags = result.flags;
    pager_gate_state.last_blockers = result.blockers;
    pager_gate_state.last_region_id = result.region_id;
    pager_gate_state.last_owner_id = result.owner_id;
    pager_gate_state.requested_bytes = result.requested_bytes;
    pager_gate_state.committed_bytes = result.committed_bytes;
    pager_gate_state.resident_bytes = result.resident_bytes;
    pager_gate_state.nonresident_bytes = result.nonresident_bytes;
    pager_gate_state.requested_slots = result.requested_slots;
    pager_gate_state.prepared_slots = result.prepared_slots;
    pager_gate_state.capacity_slots = result.capacity_slots;
    pager_gate_state.free_before_slots = result.free_before_slots;
    pager_gate_state.free_after_slots = result.free_after_slots;
    pager_gate_state.reserved_before_slots = result.reserved_before_slots;
    pager_gate_state.reserved_after_slots = result.reserved_after_slots;
    pager_gate_state.last_reservation_id = result.slot_reservation_id;
    pager_gate_state.rollback_completed = result.rollback_completed;
    pager_gate_state.commit_gate_enabled = result.commit_gate_enabled;
    pager_gate_state.fault_gate_enabled = result.fault_gate_enabled;
    pager_gate_state.pager_enabled = result.pager_enabled;
    pager_gate_state.eviction_enabled = result.eviction_enabled;
    pager_gate_state.page_in_enabled = result.page_in_enabled;
    pager_gate_state.page_out_enabled = result.page_out_enabled;
    pager_gate_state.slot_generation = result.slot_generation;
    pager_gate_state.fault_count = result.fault_count;
    pager_gate_state.failed_faults = result.failed_faults;
}

fn makePageIoResult(input: PageIoInput) PageIoResult {
    var result = PageIoResult{
        .operation = input.operation,
        .region_id = input.region_id,
        .reservation_id = input.reservation_id,
        .owner_kind = input.owner_kind,
        .owner_id = input.owner_id,
        .region_offset = input.region_offset,
        .committed_bytes = input.committed_bytes,
        .resident_bytes = input.resident_bytes,
        .slot_index = input.slot_index,
        .page_count = input.page_count,
        .expected_generation = input.expected_generation,
        .io_status = input.io_status,
        .io_bytes = input.io_bytes,
    };
    switch (input.operation) {
        page_io_operation_page_out => result.flags |= page_io_flag_page_out,
        page_io_operation_page_in => result.flags |= page_io_flag_page_in,
        else => {},
    }
    if ((input.flags & page_io_flag_eviction_request) != 0) result.flags |= page_io_flag_eviction_request;
    if ((input.flags & page_io_flag_retry_request) != 0) result.flags |= page_io_flag_retry_request;
    return result;
}

fn validatePageIo(input: PageIoInput, result: *PageIoResult) ?usize {
    if ((input.flags & ~supported_page_io_flags) != 0) {
        result.status = page_io_status_unsupported_flags;
        result.blockers |= page_io_blocker_unsupported_flags;
        return null;
    }

    if (!validSlotOwnerKind(input.owner_kind)) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_invalid_owner;
        return null;
    }

    if (input.operation != page_io_operation_page_out and input.operation != page_io_operation_page_in) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_invalid_request;
        return null;
    }

    if (input.page_count == 0 or input.page_count > std.math.maxInt(u64) / PAGE_BYTES) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_invalid_request;
        return null;
    }
    const transfer_bytes = input.page_count * PAGE_BYTES;
    result.transfer_bytes = transfer_bytes;
    if (transfer_bytes > std.math.maxInt(i32)) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_invalid_request;
        return null;
    }
    if (input.page_count > 1) result.flags |= page_io_flag_multi_page;

    if (!input.vm_region_exists) {
        result.status = page_io_status_vm_region_missing;
        result.blockers |= page_io_blocker_vm_region_missing;
        return null;
    }
    result.flags |= page_io_flag_vm_region_attached;

    if (!input.vm_region_is_r4x) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_vm_region_not_r4x;
        return null;
    }

    if ((input.region_offset & (PAGE_BYTES - 1)) != 0) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_unaligned_region_offset;
        return null;
    }
    const region_end = input.region_offset + transfer_bytes;
    if (region_end < input.region_offset or region_end > input.committed_bytes) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_region_offset_outside_commit;
        return null;
    }

    if (input.backing.status != status_ready) {
        result.status = page_io_status_backing_unavailable;
        result.blockers |= page_io_blocker_backing_not_ready;
        return null;
    }
    result.flags |= page_io_flag_file_backed | page_io_flag_backing_ready;

    if (input.reservation_id == 0) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_invalid_request;
        return null;
    }

    const capacity_slots = input.backing.available_bytes / SLOT_BYTES;
    ensureSlotBacking(input.backing, capacity_slots);
    result.capacity_slots = active_capacity_slots;
    fillPageIoSlotState(result);

    if (input.expected_generation != 0) {
        result.flags |= page_io_flag_generation_checked;
        if (input.expected_generation != slot_generation) {
            result.status = page_io_status_stale_generation;
            result.blockers |= page_io_blocker_stale_generation;
            result.slot_generation = slot_generation;
            return null;
        }
    }

    const extent_index = findExtentByReservation(input.reservation_id) orelse {
        result.status = page_io_status_reservation_not_found;
        result.blockers |= page_io_blocker_reservation_not_found;
        return null;
    };
    const extent = slot_extents[extent_index];
    result.flags |= page_io_flag_slot_reserved;
    if (!ownerMatches(extent, input.owner_kind, input.owner_id, input.region_id)) {
        result.status = page_io_status_owner_mismatch;
        result.blockers |= page_io_blocker_owner_mismatch;
        result.owner_kind = extent.owner_kind;
        result.owner_id = extent.owner_id;
        return null;
    }
    result.flags |= page_io_flag_owner_matched;
    if (input.slot_index >= extent.slot_count or input.page_count > extent.slot_count - input.slot_index) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_slot_index_out_of_range;
        return null;
    }
    if ((extent.state & extent_state_valid) != 0) result.flags |= page_io_flag_slot_valid;
    if ((extent.state & extent_state_dirty) != 0) {
        result.flags |= page_io_flag_slot_dirty;
    } else {
        result.flags |= page_io_flag_slot_clean;
    }
    if ((extent.state & extent_state_error) != 0) {
        result.status = page_io_status_slot_error;
        result.blockers |= page_io_blocker_slot_error;
        return null;
    }
    if (input.operation == page_io_operation_page_out and
        (extent.state & extent_state_valid) != 0 and
        (input.flags & page_io_flag_eviction_request) == 0)
    {
        result.status = page_io_status_slot_already_valid;
        result.blockers |= page_io_blocker_slot_already_valid;
        return null;
    }
    if (input.operation == page_io_operation_page_in and (extent.state & extent_state_valid) == 0) {
        result.status = page_io_status_slot_not_valid;
        result.blockers |= page_io_blocker_slot_not_valid;
        return null;
    }

    const backing_slot = extent.start_slot + input.slot_index;
    const backing_offset = backing_slot * SLOT_BYTES;
    if (backing_offset > std.math.maxInt(u64) - transfer_bytes or backing_offset + transfer_bytes > input.backing.available_bytes) {
        result.status = page_io_status_invalid_request;
        result.blockers |= page_io_blocker_slot_index_out_of_range;
        return null;
    }

    result.backing_slot = backing_slot;
    result.backing_offset = backing_offset;
    result.slot_generation = slot_generation;
    return extent_index;
}

fn fillPageIoSlotState(result: *PageIoResult) void {
    var slot_result = SlotResult{};
    fillSlotCounts(&slot_result);
    result.capacity_slots = slot_result.capacity_slots;
    result.reserved_slots = slot_result.reserved_slots;
    result.valid_slots = slot_result.valid_slots;
    result.dirty_slots = slot_result.dirty_slots;
    result.error_slots = slot_result.error_slots;
    result.slot_generation = slot_result.generation;
}

fn recordPageIo(result: *PageIoResult) void {
    page_io_state.enabled = true;
    if ((result.flags & page_io_flag_retry_request) != 0) page_io_state.retry_attempts +%= 1;
    if (result.status == page_io_status_ready) {
        page_io_state.prepares +%= 1;
    } else if (result.status == page_io_status_page_out_ok) {
        page_io_state.page_outs +%= 1;
    } else if (result.status == page_io_status_page_in_ok) {
        page_io_state.page_ins +%= 1;
    } else {
        page_io_state.failures +%= 1;
        recordPageIoFailurePolicy(result);
    }

    result.total_prepares = page_io_state.prepares;
    result.total_page_outs = page_io_state.page_outs;
    result.total_page_ins = page_io_state.page_ins;
    result.total_failures = page_io_state.failures;
    result.total_retry_attempts = page_io_state.retry_attempts;
    result.total_retryable_failures = page_io_state.retryable_failures;
    result.total_permanent_failures = page_io_state.permanent_failures;
    result.total_retry_limit_hits = page_io_state.retry_limit_hits;
    result.total_failed_page_outs = page_io_state.failed_page_outs;
    result.total_failed_page_ins = page_io_state.failed_page_ins;
    result.total_data_preserved_pages = page_io_state.data_preserved_pages;
    result.total_data_lost_pages = page_io_state.data_lost_pages;

    page_io_state.last_status = result.status;
    page_io_state.last_operation = result.operation;
    page_io_state.last_flags = result.flags;
    page_io_state.last_blockers = result.blockers;
    page_io_state.last_region_id = result.region_id;
    page_io_state.last_reservation_id = result.reservation_id;
    page_io_state.last_owner_kind = result.owner_kind;
    page_io_state.last_owner_id = result.owner_id;
    page_io_state.region_offset = result.region_offset;
    page_io_state.page_count = result.page_count;
    page_io_state.transfer_bytes = result.transfer_bytes;
    page_io_state.expected_generation = result.expected_generation;
    page_io_state.backing_slot = result.backing_slot;
    page_io_state.backing_offset = result.backing_offset;
    page_io_state.io_bytes = result.io_bytes;
    page_io_state.io_status = result.io_status;
    page_io_state.capacity_slots = result.capacity_slots;
    page_io_state.reserved_slots = result.reserved_slots;
    page_io_state.valid_slots = result.valid_slots;
    page_io_state.dirty_slots = result.dirty_slots;
    page_io_state.error_slots = result.error_slots;
    page_io_state.pager_enabled = result.pager_enabled;
    page_io_state.eviction_enabled = result.eviction_enabled;
    page_io_state.page_in_enabled = result.page_in_enabled;
    page_io_state.page_out_enabled = result.page_out_enabled;
    page_io_state.retry_limit = result.retry_limit;
    page_io_state.backoff_ticks = result.backoff_ticks;
    page_io_state.slot_generation = result.slot_generation;
}

fn recordPageIoFailurePolicy(result: *PageIoResult) void {
    if (pageIoFailureRetryable(result.status)) {
        result.flags |= page_io_flag_retryable_failure;
        page_io_state.retryable_failures +%= 1;
    } else {
        result.flags |= page_io_flag_permanent_failure;
        page_io_state.permanent_failures +%= 1;
    }

    if ((result.flags & page_io_flag_retry_request) != 0 and !pageIoFailureRetryable(result.status)) {
        page_io_state.retry_limit_hits +%= 1;
    }

    if (result.operation == page_io_operation_page_out) {
        page_io_state.failed_page_outs +%= 1;
        page_io_state.data_preserved_pages +%= result.page_count;
        result.flags |= page_io_flag_data_preserved;
    } else if (result.operation == page_io_operation_page_in) {
        page_io_state.failed_page_ins +%= 1;
        if (result.status == page_io_status_io_failed or result.status == page_io_status_partial_io or result.status == page_io_status_backing_unavailable) {
            page_io_state.data_preserved_pages +%= result.page_count;
            result.flags |= page_io_flag_data_preserved;
        }
    }
}

fn pageIoFailureRetryable(status: u32) bool {
    return switch (status) {
        page_io_status_backing_unavailable,
        page_io_status_io_failed,
        page_io_status_partial_io,
        => true,
        else => false,
    };
}

fn mapSlotFailureToPagerGate(slot_result: SlotResult, result: *PagerGateResult) void {
    switch (slot_result.status) {
        slot_status_backing_unavailable => {
            result.status = pager_gate_status_backing_unavailable;
            result.blockers |= pager_gate_blocker_backing_not_ready;
        },
        slot_status_insufficient_capacity => {
            result.status = pager_gate_status_insufficient_capacity;
            result.blockers |= pager_gate_blocker_insufficient_capacity;
        },
        slot_status_table_full => {
            result.status = pager_gate_status_table_full;
            result.blockers |= pager_gate_blocker_table_full;
        },
        slot_status_invalid_request => {
            result.status = pager_gate_status_invalid_request;
            result.blockers |= pager_gate_blocker_invalid_request;
        },
        else => {
            result.status = pager_gate_status_invalid_request;
            result.blockers |= pager_gate_blocker_invalid_request;
        },
    }
}

fn countReservedSlots() u64 {
    var reserved: u64 = 0;
    var index: usize = 0;
    while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
        const extent = slot_extents[index];
        if (!extentActive(extent)) continue;
        reserved +%= extent.slot_count;
    }
    return reserved;
}

fn findFreeExtentIndex() ?usize {
    var index: usize = 0;
    while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
        if (!extentActive(slot_extents[index])) return index;
    }
    return null;
}

fn findFreeSlotRange(requested_slots: u64, capacity_slots: u64) ?u64 {
    if (requested_slots == 0 or requested_slots > capacity_slots) return null;

    var start: u64 = 0;
    while (start <= capacity_slots - requested_slots) {
        const end = start + requested_slots;
        var moved = false;
        var index: usize = 0;
        while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
            const extent = slot_extents[index];
            if (!extentActive(extent)) continue;
            const extent_end = extent.start_slot + extent.slot_count;
            if (start < extent_end and end > extent.start_slot) {
                start = extent_end;
                moved = true;
                break;
            }
        }
        if (!moved) return start;
    }
    return null;
}

fn allocateReservationId() ?u32 {
    const start = next_reservation_id;
    while (true) {
        const id = next_reservation_id;
        next_reservation_id +%= 1;
        if (next_reservation_id == 0) next_reservation_id = 1;
        if (id != 0 and findExtentByReservation(id) == null) return id;
        if (next_reservation_id == start) return null;
    }
}

fn cleanupOwner(owner_kind: u32, owner_id: u32) LifecycleCleanupResult {
    return cleanupMatching(owner_kind, owner_id, 0, false);
}

fn cleanupRegion(owner_kind: u32, owner_id: u32, region_id: u32) LifecycleCleanupResult {
    if (region_id == 0) return .{};
    return cleanupMatching(owner_kind, owner_id, region_id, true);
}

fn cleanupMatching(owner_kind: u32, owner_id: u32, region_id: u32, match_region: bool) LifecycleCleanupResult {
    if (!validSlotOwnerKind(owner_kind)) return .{};
    var result = LifecycleCleanupResult{};
    var index: usize = 0;
    while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
        const extent = slot_extents[index];
        if (!extentActive(extent)) continue;
        if (extent.owner_kind != owner_kind or extent.owner_id != owner_id) continue;
        if (match_region and extent.region_id != region_id) continue;

        result.released_ranges +%= 1;
        result.released_slots +%= extent.slot_count;
        if ((extent.state & extent_state_valid) != 0) result.released_valid_slots +%= extent.slot_count;
        if ((extent.state & extent_state_dirty) != 0) result.released_dirty_slots +%= extent.slot_count;
        if ((extent.state & extent_state_error) != 0) result.released_error_slots +%= extent.slot_count;
        slot_extents[index] = .{};
    }

    if (result.released_ranges != 0) {
        result.cleanup_count = 1;
        slot_generation +%= 1;
        slot_state.lifecycle_cleanups +%= 1;
        slot_state.lifecycle_released_ranges +%= result.released_ranges;
        slot_state.lifecycle_released_slots +%= result.released_slots;
        slot_state.last_owner_kind = owner_kind;
        slot_state.last_owner_id = owner_id;
        slot_state.last_region_id = if (match_region) region_id else 0;
        refreshSlotSummaryCounts();
    }
    return result;
}

fn refreshSlotSummaryCounts() void {
    var result = SlotResult{};
    fillSlotCounts(&result);
    slot_state.capacity_slots = result.capacity_slots;
    slot_state.reserved_slots = result.reserved_slots;
    slot_state.free_slots = result.free_slots;
    slot_state.valid_slots = result.valid_slots;
    slot_state.dirty_slots = result.dirty_slots;
    slot_state.error_slots = result.error_slots;
    slot_state.range_count = result.range_count;
    slot_state.max_ranges = result.max_ranges;
    slot_state.generation = result.generation;
}

fn findExtentByReservation(reservation_id: u32) ?usize {
    var index: usize = 0;
    while (index < SLOT_TABLE_MAX_RANGES) : (index += 1) {
        const extent = slot_extents[index];
        if (extentActive(extent) and extent.reservation_id == reservation_id) return index;
    }
    return null;
}

fn ownerMatches(extent: SlotExtent, owner_kind: u32, owner_id: u32, region_id: u32) bool {
    if (extent.owner_kind != owner_kind or extent.owner_id != owner_id) return false;
    if (extent.owner_kind == slot_owner_kind_vm_region) return extent.region_id != 0 and extent.region_id == region_id;
    if (extent.region_id != 0 and region_id != 0) return extent.region_id == region_id;
    return extent.region_id == 0 or region_id == 0;
}

fn validSlotOwnerKind(owner_kind: u32) bool {
    return owner_kind == slot_owner_kind_diagnostic or
        owner_kind == slot_owner_kind_r4x_instance or
        owner_kind == slot_owner_kind_vm_region or
        owner_kind == slot_owner_kind_pager;
}

fn extentActive(extent: SlotExtent) bool {
    return (extent.state & extent_state_reserved) != 0 and extent.slot_count != 0;
}
