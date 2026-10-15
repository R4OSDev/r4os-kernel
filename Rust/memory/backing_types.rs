//! Exact private backing/pager values; public ABI remains Contract-owned.
#![allow(non_upper_case_globals)]
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
pub const extent_state_reserved: u32 = 1 << 0;
pub const extent_state_valid: u32 = 1 << 1;
pub const extent_state_dirty: u32 = 1 << 2;
pub const extent_state_error: u32 = 1 << 3;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    pub requested_bytes: u64,
    pub flags: u32,
    pub path: *const u8,
    pub file_exists: bool,
    pub is_dir: bool,
    pub fat32: bool,
    pub file_size: u64,
    pub cluster_bytes: u32,
    pub first_cluster: u32,
}
impl Input {
    pub const INIT: Self = Self {
        requested_bytes: 0,
        flags: 0,
        path: core::ptr::null(),
        file_exists: false,
        is_dir: false,
        fat32: false,
        file_size: 0,
        cluster_bytes: 0,
        first_cluster: 0,
    };
}
impl Default for Input {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Result {
    pub path: [u8; MAX_BACKING_PATH + 1],
    pub status: u32,
    pub flags: u32,
    pub blockers: u32,
    pub requested_bytes: u64,
    pub available_bytes: u64,
    pub file_size: u64,
    pub cluster_bytes: u32,
    pub first_cluster: u32,
    pub pager_enabled: u32,
    pub anonymous_paging_enabled: u32,
}
impl Result {
    pub const INIT: Self = Self {
        path: [0; MAX_BACKING_PATH + 1],
        status: status_unavailable,
        flags: flag_reserve_only | flag_pager_disabled | flag_uses_fs_api | flag_no_second_io_path,
        blockers: 0,
        requested_bytes: 0,
        available_bytes: 0,
        file_size: 0,
        cluster_bytes: 0,
        first_cluster: 0,
        pager_enabled: 0,
        anonymous_paging_enabled: 0,
    };
}
impl Default for Result {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Summary {
    pub enabled: bool,
    pub probes: u64,
    pub ready: u64,
    pub failures: u64,
    pub last_status: u32,
    pub last_flags: u32,
    pub last_blockers: u32,
    pub last_requested_bytes: u64,
    pub last_available_bytes: u64,
    pub last_file_size: u64,
    pub last_cluster_bytes: u32,
    pub last_first_cluster: u32,
}
impl Summary {
    pub const INIT: Self = Self {
        enabled: true,
        probes: 0,
        ready: 0,
        failures: 0,
        last_status: status_unavailable,
        last_flags: 0,
        last_blockers: 0,
        last_requested_bytes: 0,
        last_available_bytes: 0,
        last_file_size: 0,
        last_cluster_bytes: 0,
        last_first_cluster: 0,
    };
}
impl Default for Summary {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotInput {
    pub operation: u32,
    pub requested_slots: u64,
    pub reservation_id: u32,
    pub owner_kind: u32,
    pub owner_id: u32,
    pub region_id: u32,
    pub flags: u32,
    pub backing: Result,
}
impl SlotInput {
    pub const INIT: Self = Self {
        operation: slot_operation_probe,
        requested_slots: 0,
        reservation_id: 0,
        owner_kind: slot_owner_kind_diagnostic,
        owner_id: 0,
        region_id: 0,
        flags: 0,
        backing: Result::INIT,
    };
}
impl Default for SlotInput {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotResult {
    pub version: u32,
    pub status: u32,
    pub operation: u32,
    pub flags: u32,
    pub blockers: u32,
    pub slot_bytes: u32,
    pub capacity_slots: u64,
    pub requested_slots: u64,
    pub reserved_slots: u64,
    pub free_slots: u64,
    pub valid_slots: u64,
    pub dirty_slots: u64,
    pub error_slots: u64,
    pub range_count: u32,
    pub max_ranges: u32,
    pub reservation_id: u32,
    pub owner_kind: u32,
    pub owner_id: u32,
    pub region_id: u32,
    pub first_slot: u64,
    pub slot_count: u64,
    pub generation: u64,
    pub pager_enabled: u32,
    pub eviction_enabled: u32,
    pub page_in_enabled: u32,
    pub page_out_enabled: u32,
    pub total_probes: u64,
    pub total_reserves: u64,
    pub total_releases: u64,
    pub total_error_marks: u64,
    pub total_recoveries: u64,
    pub total_failures: u64,
}
impl SlotResult {
    pub const INIT: Self = Self {
        version: 1,
        status: slot_status_unavailable,
        operation: slot_operation_probe,
        flags: slot_flag_metadata_only
            | slot_flag_range_table
            | slot_flag_page_sized_slots
            | slot_flag_pager_disabled
            | slot_flag_recovery_available,
        blockers: 0,
        slot_bytes: 4096,
        capacity_slots: 0,
        requested_slots: 0,
        reserved_slots: 0,
        free_slots: 0,
        valid_slots: 0,
        dirty_slots: 0,
        error_slots: 0,
        range_count: 0,
        max_ranges: slot_table_max_ranges,
        reservation_id: 0,
        owner_kind: slot_owner_kind_diagnostic,
        owner_id: 0,
        region_id: 0,
        first_slot: 0,
        slot_count: 0,
        generation: 0,
        pager_enabled: 0,
        eviction_enabled: 1,
        page_in_enabled: 1,
        page_out_enabled: 1,
        total_probes: 0,
        total_reserves: 0,
        total_releases: 0,
        total_error_marks: 0,
        total_recoveries: 0,
        total_failures: 0,
    };
}
impl Default for SlotResult {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotSummary {
    pub enabled: bool,
    pub last_status: u32,
    pub last_operation: u32,
    pub last_flags: u32,
    pub last_blockers: u32,
    pub slot_bytes: u32,
    pub capacity_slots: u64,
    pub reserved_slots: u64,
    pub free_slots: u64,
    pub valid_slots: u64,
    pub dirty_slots: u64,
    pub error_slots: u64,
    pub range_count: u32,
    pub max_ranges: u32,
    pub last_reservation_id: u32,
    pub last_owner_kind: u32,
    pub last_owner_id: u32,
    pub last_region_id: u32,
    pub last_first_slot: u64,
    pub last_slot_count: u64,
    pub generation: u64,
    pub probes: u64,
    pub reserves: u64,
    pub releases: u64,
    pub error_marks: u64,
    pub recoveries: u64,
    pub failures: u64,
    pub lifecycle_cleanups: u64,
    pub lifecycle_released_ranges: u64,
    pub lifecycle_released_slots: u64,
}
impl SlotSummary {
    pub const INIT: Self = Self {
        enabled: true,
        last_status: slot_status_unavailable,
        last_operation: slot_operation_probe,
        last_flags: 0,
        last_blockers: 0,
        slot_bytes: 4096,
        capacity_slots: 0,
        reserved_slots: 0,
        free_slots: 0,
        valid_slots: 0,
        dirty_slots: 0,
        error_slots: 0,
        range_count: 0,
        max_ranges: slot_table_max_ranges,
        last_reservation_id: 0,
        last_owner_kind: slot_owner_kind_diagnostic,
        last_owner_id: 0,
        last_region_id: 0,
        last_first_slot: 0,
        last_slot_count: 0,
        generation: 0,
        probes: 0,
        reserves: 0,
        releases: 0,
        error_marks: 0,
        recoveries: 0,
        failures: 0,
        lifecycle_cleanups: 0,
        lifecycle_released_ranges: 0,
        lifecycle_released_slots: 0,
    };
}
impl Default for SlotSummary {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PagerGateInput {
    pub requested_bytes: u64,
    pub region_id: u32,
    pub owner_id: u32,
    pub flags: u32,
    pub vm_region_exists: bool,
    pub vm_region_is_r4x: bool,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub backing: Result,
}
impl PagerGateInput {
    pub const INIT: Self = Self {
        requested_bytes: 0,
        region_id: 0,
        owner_id: 0,
        flags: 0,
        vm_region_exists: false,
        vm_region_is_r4x: false,
        committed_bytes: 0,
        resident_bytes: 0,
        fault_count: 0,
        failed_faults: 0,
        backing: Result::INIT,
    };
}
impl Default for PagerGateInput {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PagerGateResult {
    pub version: u32,
    pub status: u32,
    pub flags: u32,
    pub blockers: u32,
    pub region_id: u32,
    pub owner_id: u32,
    pub slot_bytes: u32,
    pub reserved0: u32,
    pub requested_bytes: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub nonresident_bytes: u64,
    pub requested_slots: u64,
    pub prepared_slots: u64,
    pub capacity_slots: u64,
    pub free_before_slots: u64,
    pub free_after_slots: u64,
    pub reserved_before_slots: u64,
    pub reserved_after_slots: u64,
    pub slot_reservation_id: u32,
    pub rollback_completed: u32,
    pub commit_gate_enabled: u32,
    pub fault_gate_enabled: u32,
    pub pager_enabled: u32,
    pub eviction_enabled: u32,
    pub page_in_enabled: u32,
    pub page_out_enabled: u32,
    pub slot_generation: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub total_probes: u64,
    pub total_ready: u64,
    pub total_rollbacks: u64,
    pub total_failures: u64,
}
impl PagerGateResult {
    pub const INIT: Self = Self {
        version: 1,
        status: pager_gate_status_unavailable,
        flags: pager_gate_flag_metadata_only
            | pager_gate_flag_commit_gate
            | pager_gate_flag_fault_gate
            | pager_gate_flag_pager_disabled
            | pager_gate_flag_no_page_io
            | pager_gate_flag_no_swap
            | pager_gate_flag_no_second_io_path
            | pager_gate_flag_page_sized_slots,
        blockers: 0,
        region_id: 0,
        owner_id: 0,
        slot_bytes: 4096,
        reserved0: 0,
        requested_bytes: 0,
        committed_bytes: 0,
        resident_bytes: 0,
        nonresident_bytes: 0,
        requested_slots: 0,
        prepared_slots: 0,
        capacity_slots: 0,
        free_before_slots: 0,
        free_after_slots: 0,
        reserved_before_slots: 0,
        reserved_after_slots: 0,
        slot_reservation_id: 0,
        rollback_completed: 0,
        commit_gate_enabled: 1,
        fault_gate_enabled: 1,
        pager_enabled: 0,
        eviction_enabled: 1,
        page_in_enabled: 0,
        page_out_enabled: 0,
        slot_generation: 0,
        fault_count: 0,
        failed_faults: 0,
        total_probes: 0,
        total_ready: 0,
        total_rollbacks: 0,
        total_failures: 0,
    };
}
impl Default for PagerGateResult {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PagerGateSummary {
    pub enabled: bool,
    pub last_status: u32,
    pub last_flags: u32,
    pub last_blockers: u32,
    pub last_region_id: u32,
    pub last_owner_id: u32,
    pub requested_bytes: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub nonresident_bytes: u64,
    pub requested_slots: u64,
    pub prepared_slots: u64,
    pub capacity_slots: u64,
    pub free_before_slots: u64,
    pub free_after_slots: u64,
    pub reserved_before_slots: u64,
    pub reserved_after_slots: u64,
    pub last_reservation_id: u32,
    pub rollback_completed: u32,
    pub commit_gate_enabled: u32,
    pub fault_gate_enabled: u32,
    pub pager_enabled: u32,
    pub eviction_enabled: u32,
    pub page_in_enabled: u32,
    pub page_out_enabled: u32,
    pub slot_generation: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub probes: u64,
    pub ready: u64,
    pub rollbacks: u64,
    pub failures: u64,
}
impl PagerGateSummary {
    pub const INIT: Self = Self {
        enabled: true,
        last_status: pager_gate_status_unavailable,
        last_flags: 0,
        last_blockers: 0,
        last_region_id: 0,
        last_owner_id: 0,
        requested_bytes: 0,
        committed_bytes: 0,
        resident_bytes: 0,
        nonresident_bytes: 0,
        requested_slots: 0,
        prepared_slots: 0,
        capacity_slots: 0,
        free_before_slots: 0,
        free_after_slots: 0,
        reserved_before_slots: 0,
        reserved_after_slots: 0,
        last_reservation_id: 0,
        rollback_completed: 0,
        commit_gate_enabled: 1,
        fault_gate_enabled: 1,
        pager_enabled: 0,
        eviction_enabled: 1,
        page_in_enabled: 0,
        page_out_enabled: 0,
        slot_generation: 0,
        fault_count: 0,
        failed_faults: 0,
        probes: 0,
        ready: 0,
        rollbacks: 0,
        failures: 0,
    };
}
impl Default for PagerGateSummary {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageIoInput {
    pub operation: u32,
    pub region_id: u32,
    pub region_offset: u64,
    pub reservation_id: u32,
    pub slot_index: u64,
    pub page_count: u64,
    pub owner_kind: u32,
    pub owner_id: u32,
    pub expected_generation: u64,
    pub flags: u32,
    pub vm_region_exists: bool,
    pub vm_region_is_r4x: bool,
    pub commit_covered: bool,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub backing: Result,
    pub io_status: i32,
    pub io_bytes: u32,
}
impl PageIoInput {
    pub const INIT: Self = Self {
        operation: page_io_operation_page_out,
        region_id: 0,
        region_offset: 0,
        reservation_id: 0,
        slot_index: 0,
        page_count: 1,
        owner_kind: slot_owner_kind_diagnostic,
        owner_id: 0,
        expected_generation: 0,
        flags: 0,
        vm_region_exists: false,
        vm_region_is_r4x: false,
        commit_covered: false,
        committed_bytes: 0,
        resident_bytes: 0,
        backing: Result::INIT,
        io_status: 0,
        io_bytes: 0,
    };
}
impl Default for PageIoInput {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageIoResult {
    pub version: u32,
    pub status: u32,
    pub operation: u32,
    pub flags: u32,
    pub blockers: u32,
    pub region_id: u32,
    pub reservation_id: u32,
    pub owner_kind: u32,
    pub owner_id: u32,
    pub slot_bytes: u32,
    pub reserved0: u32,
    pub region_offset: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub slot_index: u64,
    pub page_count: u64,
    pub transfer_bytes: u64,
    pub expected_generation: u64,
    pub backing_slot: u64,
    pub backing_offset: u64,
    pub capacity_slots: u64,
    pub reserved_slots: u64,
    pub valid_slots: u64,
    pub dirty_slots: u64,
    pub error_slots: u64,
    pub io_bytes: u32,
    pub io_status: i32,
    pub pager_enabled: u32,
    pub eviction_enabled: u32,
    pub page_in_enabled: u32,
    pub page_out_enabled: u32,
    pub retry_limit: u32,
    pub backoff_ticks: u32,
    pub slot_generation: u64,
    pub total_prepares: u64,
    pub total_page_outs: u64,
    pub total_page_ins: u64,
    pub total_failures: u64,
    pub total_retry_attempts: u64,
    pub total_retryable_failures: u64,
    pub total_permanent_failures: u64,
    pub total_retry_limit_hits: u64,
    pub total_failed_page_outs: u64,
    pub total_failed_page_ins: u64,
    pub total_data_preserved_pages: u64,
    pub total_data_lost_pages: u64,
}
impl PageIoResult {
    pub const INIT: Self = Self {
        version: 1,
        status: page_io_status_unavailable,
        operation: page_io_operation_page_out,
        flags: page_io_flag_explicit_request
            | page_io_flag_uses_fs_api
            | page_io_flag_no_second_io_path
            | page_io_flag_pager_disabled
            | page_io_flag_no_swap
            | page_io_flag_page_sized_slots,
        blockers: 0,
        region_id: 0,
        reservation_id: 0,
        owner_kind: slot_owner_kind_diagnostic,
        owner_id: 0,
        slot_bytes: 4096,
        reserved0: 0,
        region_offset: 0,
        committed_bytes: 0,
        resident_bytes: 0,
        slot_index: 0,
        page_count: 1,
        transfer_bytes: 4096,
        expected_generation: 0,
        backing_slot: 0,
        backing_offset: 0,
        capacity_slots: 0,
        reserved_slots: 0,
        valid_slots: 0,
        dirty_slots: 0,
        error_slots: 0,
        io_bytes: 0,
        io_status: 0,
        pager_enabled: 0,
        eviction_enabled: 1,
        page_in_enabled: 1,
        page_out_enabled: 1,
        retry_limit: page_io_policy_retry_limit,
        backoff_ticks: page_io_policy_backoff_ticks,
        slot_generation: 0,
        total_prepares: 0,
        total_page_outs: 0,
        total_page_ins: 0,
        total_failures: 0,
        total_retry_attempts: 0,
        total_retryable_failures: 0,
        total_permanent_failures: 0,
        total_retry_limit_hits: 0,
        total_failed_page_outs: 0,
        total_failed_page_ins: 0,
        total_data_preserved_pages: 0,
        total_data_lost_pages: 0,
    };
}
impl Default for PageIoResult {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageIoSummary {
    pub enabled: bool,
    pub last_status: u32,
    pub last_operation: u32,
    pub last_flags: u32,
    pub last_blockers: u32,
    pub last_region_id: u32,
    pub last_reservation_id: u32,
    pub last_owner_kind: u32,
    pub last_owner_id: u32,
    pub region_offset: u64,
    pub page_count: u64,
    pub transfer_bytes: u64,
    pub expected_generation: u64,
    pub backing_slot: u64,
    pub backing_offset: u64,
    pub io_bytes: u32,
    pub io_status: i32,
    pub capacity_slots: u64,
    pub reserved_slots: u64,
    pub valid_slots: u64,
    pub dirty_slots: u64,
    pub error_slots: u64,
    pub pager_enabled: u32,
    pub eviction_enabled: u32,
    pub page_in_enabled: u32,
    pub page_out_enabled: u32,
    pub retry_limit: u32,
    pub backoff_ticks: u32,
    pub slot_generation: u64,
    pub prepares: u64,
    pub page_outs: u64,
    pub page_ins: u64,
    pub failures: u64,
    pub retry_attempts: u64,
    pub retryable_failures: u64,
    pub permanent_failures: u64,
    pub retry_limit_hits: u64,
    pub failed_page_outs: u64,
    pub failed_page_ins: u64,
    pub data_preserved_pages: u64,
    pub data_lost_pages: u64,
}
impl PageIoSummary {
    pub const INIT: Self = Self {
        enabled: true,
        last_status: page_io_status_unavailable,
        last_operation: 0,
        last_flags: 0,
        last_blockers: 0,
        last_region_id: 0,
        last_reservation_id: 0,
        last_owner_kind: slot_owner_kind_diagnostic,
        last_owner_id: 0,
        region_offset: 0,
        page_count: 1,
        transfer_bytes: 4096,
        expected_generation: 0,
        backing_slot: 0,
        backing_offset: 0,
        io_bytes: 0,
        io_status: 0,
        capacity_slots: 0,
        reserved_slots: 0,
        valid_slots: 0,
        dirty_slots: 0,
        error_slots: 0,
        pager_enabled: 0,
        eviction_enabled: 1,
        page_in_enabled: 1,
        page_out_enabled: 1,
        retry_limit: page_io_policy_retry_limit,
        backoff_ticks: page_io_policy_backoff_ticks,
        slot_generation: 0,
        prepares: 0,
        page_outs: 0,
        page_ins: 0,
        failures: 0,
        retry_attempts: 0,
        retryable_failures: 0,
        permanent_failures: 0,
        retry_limit_hits: 0,
        failed_page_outs: 0,
        failed_page_ins: 0,
        data_preserved_pages: 0,
        data_lost_pages: 0,
    };
}
impl Default for PageIoSummary {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotExtent {
    pub reservation_id: u32,
    pub owner_kind: u32,
    pub owner_id: u32,
    pub region_id: u32,
    pub start_slot: u64,
    pub slot_count: u64,
    pub state: u32,
    pub generation: u64,
}
impl SlotExtent {
    pub const INIT: Self = Self {
        reservation_id: 0,
        owner_kind: 0,
        owner_id: 0,
        region_id: 0,
        start_slot: 0,
        slot_count: 0,
        state: 0,
        generation: 0,
    };
}
impl Default for SlotExtent {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LifecycleCleanupResult {
    pub cleanup_count: u64,
    pub released_ranges: u64,
    pub released_slots: u64,
    pub released_valid_slots: u64,
    pub released_dirty_slots: u64,
    pub released_error_slots: u64,
}
impl LifecycleCleanupResult {
    pub const INIT: Self = Self {
        cleanup_count: 0,
        released_ranges: 0,
        released_slots: 0,
        released_valid_slots: 0,
        released_dirty_slots: 0,
        released_error_slots: 0,
    };
}
impl Default for LifecycleCleanupResult {
    fn default() -> Self {
        Self::INIT
    }
}
const _: () = {
    assert!(core::mem::size_of::<Input>() == 48);
    assert!(core::mem::align_of::<Input>() == 8);
    assert!(core::mem::offset_of!(Input, requested_bytes) == 0);
    assert!(core::mem::offset_of!(Input, flags) == 8);
    assert!(core::mem::offset_of!(Input, path) == 16);
    assert!(core::mem::offset_of!(Input, file_exists) == 24);
    assert!(core::mem::offset_of!(Input, is_dir) == 25);
    assert!(core::mem::offset_of!(Input, fat32) == 26);
    assert!(core::mem::offset_of!(Input, file_size) == 32);
    assert!(core::mem::offset_of!(Input, cluster_bytes) == 40);
    assert!(core::mem::offset_of!(Input, first_cluster) == 44);
    assert!(core::mem::size_of::<Result>() == 184);
    assert!(core::mem::align_of::<Result>() == 8);
    assert!(core::mem::offset_of!(Result, path) == 0);
    assert!(core::mem::offset_of!(Result, status) == 132);
    assert!(core::mem::offset_of!(Result, flags) == 136);
    assert!(core::mem::offset_of!(Result, blockers) == 140);
    assert!(core::mem::offset_of!(Result, requested_bytes) == 144);
    assert!(core::mem::offset_of!(Result, available_bytes) == 152);
    assert!(core::mem::offset_of!(Result, file_size) == 160);
    assert!(core::mem::offset_of!(Result, cluster_bytes) == 168);
    assert!(core::mem::offset_of!(Result, first_cluster) == 172);
    assert!(core::mem::offset_of!(Result, pager_enabled) == 176);
    assert!(core::mem::offset_of!(Result, anonymous_paging_enabled) == 180);
    assert!(core::mem::size_of::<Summary>() == 80);
    assert!(core::mem::align_of::<Summary>() == 8);
    assert!(core::mem::offset_of!(Summary, enabled) == 0);
    assert!(core::mem::offset_of!(Summary, probes) == 8);
    assert!(core::mem::offset_of!(Summary, ready) == 16);
    assert!(core::mem::offset_of!(Summary, failures) == 24);
    assert!(core::mem::offset_of!(Summary, last_status) == 32);
    assert!(core::mem::offset_of!(Summary, last_flags) == 36);
    assert!(core::mem::offset_of!(Summary, last_blockers) == 40);
    assert!(core::mem::offset_of!(Summary, last_requested_bytes) == 48);
    assert!(core::mem::offset_of!(Summary, last_available_bytes) == 56);
    assert!(core::mem::offset_of!(Summary, last_file_size) == 64);
    assert!(core::mem::offset_of!(Summary, last_cluster_bytes) == 72);
    assert!(core::mem::offset_of!(Summary, last_first_cluster) == 76);
    assert!(core::mem::size_of::<SlotInput>() == 224);
    assert!(core::mem::align_of::<SlotInput>() == 8);
    assert!(core::mem::offset_of!(SlotInput, operation) == 0);
    assert!(core::mem::offset_of!(SlotInput, requested_slots) == 8);
    assert!(core::mem::offset_of!(SlotInput, reservation_id) == 16);
    assert!(core::mem::offset_of!(SlotInput, owner_kind) == 20);
    assert!(core::mem::offset_of!(SlotInput, owner_id) == 24);
    assert!(core::mem::offset_of!(SlotInput, region_id) == 28);
    assert!(core::mem::offset_of!(SlotInput, flags) == 32);
    assert!(core::mem::offset_of!(SlotInput, backing) == 40);
    assert!(core::mem::size_of::<SlotResult>() == 192);
    assert!(core::mem::align_of::<SlotResult>() == 8);
    assert!(core::mem::offset_of!(SlotResult, version) == 0);
    assert!(core::mem::offset_of!(SlotResult, status) == 4);
    assert!(core::mem::offset_of!(SlotResult, operation) == 8);
    assert!(core::mem::offset_of!(SlotResult, flags) == 12);
    assert!(core::mem::offset_of!(SlotResult, blockers) == 16);
    assert!(core::mem::offset_of!(SlotResult, slot_bytes) == 20);
    assert!(core::mem::offset_of!(SlotResult, capacity_slots) == 24);
    assert!(core::mem::offset_of!(SlotResult, requested_slots) == 32);
    assert!(core::mem::offset_of!(SlotResult, reserved_slots) == 40);
    assert!(core::mem::offset_of!(SlotResult, free_slots) == 48);
    assert!(core::mem::offset_of!(SlotResult, valid_slots) == 56);
    assert!(core::mem::offset_of!(SlotResult, dirty_slots) == 64);
    assert!(core::mem::offset_of!(SlotResult, error_slots) == 72);
    assert!(core::mem::offset_of!(SlotResult, range_count) == 80);
    assert!(core::mem::offset_of!(SlotResult, max_ranges) == 84);
    assert!(core::mem::offset_of!(SlotResult, reservation_id) == 88);
    assert!(core::mem::offset_of!(SlotResult, owner_kind) == 92);
    assert!(core::mem::offset_of!(SlotResult, owner_id) == 96);
    assert!(core::mem::offset_of!(SlotResult, region_id) == 100);
    assert!(core::mem::offset_of!(SlotResult, first_slot) == 104);
    assert!(core::mem::offset_of!(SlotResult, slot_count) == 112);
    assert!(core::mem::offset_of!(SlotResult, generation) == 120);
    assert!(core::mem::offset_of!(SlotResult, pager_enabled) == 128);
    assert!(core::mem::offset_of!(SlotResult, eviction_enabled) == 132);
    assert!(core::mem::offset_of!(SlotResult, page_in_enabled) == 136);
    assert!(core::mem::offset_of!(SlotResult, page_out_enabled) == 140);
    assert!(core::mem::offset_of!(SlotResult, total_probes) == 144);
    assert!(core::mem::offset_of!(SlotResult, total_reserves) == 152);
    assert!(core::mem::offset_of!(SlotResult, total_releases) == 160);
    assert!(core::mem::offset_of!(SlotResult, total_error_marks) == 168);
    assert!(core::mem::offset_of!(SlotResult, total_recoveries) == 176);
    assert!(core::mem::offset_of!(SlotResult, total_failures) == 184);
    assert!(core::mem::size_of::<SlotSummary>() == 192);
    assert!(core::mem::align_of::<SlotSummary>() == 8);
    assert!(core::mem::offset_of!(SlotSummary, enabled) == 0);
    assert!(core::mem::offset_of!(SlotSummary, last_status) == 4);
    assert!(core::mem::offset_of!(SlotSummary, last_operation) == 8);
    assert!(core::mem::offset_of!(SlotSummary, last_flags) == 12);
    assert!(core::mem::offset_of!(SlotSummary, last_blockers) == 16);
    assert!(core::mem::offset_of!(SlotSummary, slot_bytes) == 20);
    assert!(core::mem::offset_of!(SlotSummary, capacity_slots) == 24);
    assert!(core::mem::offset_of!(SlotSummary, reserved_slots) == 32);
    assert!(core::mem::offset_of!(SlotSummary, free_slots) == 40);
    assert!(core::mem::offset_of!(SlotSummary, valid_slots) == 48);
    assert!(core::mem::offset_of!(SlotSummary, dirty_slots) == 56);
    assert!(core::mem::offset_of!(SlotSummary, error_slots) == 64);
    assert!(core::mem::offset_of!(SlotSummary, range_count) == 72);
    assert!(core::mem::offset_of!(SlotSummary, max_ranges) == 76);
    assert!(core::mem::offset_of!(SlotSummary, last_reservation_id) == 80);
    assert!(core::mem::offset_of!(SlotSummary, last_owner_kind) == 84);
    assert!(core::mem::offset_of!(SlotSummary, last_owner_id) == 88);
    assert!(core::mem::offset_of!(SlotSummary, last_region_id) == 92);
    assert!(core::mem::offset_of!(SlotSummary, last_first_slot) == 96);
    assert!(core::mem::offset_of!(SlotSummary, last_slot_count) == 104);
    assert!(core::mem::offset_of!(SlotSummary, generation) == 112);
    assert!(core::mem::offset_of!(SlotSummary, probes) == 120);
    assert!(core::mem::offset_of!(SlotSummary, reserves) == 128);
    assert!(core::mem::offset_of!(SlotSummary, releases) == 136);
    assert!(core::mem::offset_of!(SlotSummary, error_marks) == 144);
    assert!(core::mem::offset_of!(SlotSummary, recoveries) == 152);
    assert!(core::mem::offset_of!(SlotSummary, failures) == 160);
    assert!(core::mem::offset_of!(SlotSummary, lifecycle_cleanups) == 168);
    assert!(core::mem::offset_of!(SlotSummary, lifecycle_released_ranges) == 176);
    assert!(core::mem::offset_of!(SlotSummary, lifecycle_released_slots) == 184);
    assert!(core::mem::size_of::<PagerGateInput>() == 240);
    assert!(core::mem::align_of::<PagerGateInput>() == 8);
    assert!(core::mem::offset_of!(PagerGateInput, requested_bytes) == 0);
    assert!(core::mem::offset_of!(PagerGateInput, region_id) == 8);
    assert!(core::mem::offset_of!(PagerGateInput, owner_id) == 12);
    assert!(core::mem::offset_of!(PagerGateInput, flags) == 16);
    assert!(core::mem::offset_of!(PagerGateInput, vm_region_exists) == 20);
    assert!(core::mem::offset_of!(PagerGateInput, vm_region_is_r4x) == 21);
    assert!(core::mem::offset_of!(PagerGateInput, committed_bytes) == 24);
    assert!(core::mem::offset_of!(PagerGateInput, resident_bytes) == 32);
    assert!(core::mem::offset_of!(PagerGateInput, fault_count) == 40);
    assert!(core::mem::offset_of!(PagerGateInput, failed_faults) == 48);
    assert!(core::mem::offset_of!(PagerGateInput, backing) == 56);
    assert!(core::mem::size_of::<PagerGateResult>() == 208);
    assert!(core::mem::align_of::<PagerGateResult>() == 8);
    assert!(core::mem::offset_of!(PagerGateResult, version) == 0);
    assert!(core::mem::offset_of!(PagerGateResult, status) == 4);
    assert!(core::mem::offset_of!(PagerGateResult, flags) == 8);
    assert!(core::mem::offset_of!(PagerGateResult, blockers) == 12);
    assert!(core::mem::offset_of!(PagerGateResult, region_id) == 16);
    assert!(core::mem::offset_of!(PagerGateResult, owner_id) == 20);
    assert!(core::mem::offset_of!(PagerGateResult, slot_bytes) == 24);
    assert!(core::mem::offset_of!(PagerGateResult, reserved0) == 28);
    assert!(core::mem::offset_of!(PagerGateResult, requested_bytes) == 32);
    assert!(core::mem::offset_of!(PagerGateResult, committed_bytes) == 40);
    assert!(core::mem::offset_of!(PagerGateResult, resident_bytes) == 48);
    assert!(core::mem::offset_of!(PagerGateResult, nonresident_bytes) == 56);
    assert!(core::mem::offset_of!(PagerGateResult, requested_slots) == 64);
    assert!(core::mem::offset_of!(PagerGateResult, prepared_slots) == 72);
    assert!(core::mem::offset_of!(PagerGateResult, capacity_slots) == 80);
    assert!(core::mem::offset_of!(PagerGateResult, free_before_slots) == 88);
    assert!(core::mem::offset_of!(PagerGateResult, free_after_slots) == 96);
    assert!(core::mem::offset_of!(PagerGateResult, reserved_before_slots) == 104);
    assert!(core::mem::offset_of!(PagerGateResult, reserved_after_slots) == 112);
    assert!(core::mem::offset_of!(PagerGateResult, slot_reservation_id) == 120);
    assert!(core::mem::offset_of!(PagerGateResult, rollback_completed) == 124);
    assert!(core::mem::offset_of!(PagerGateResult, commit_gate_enabled) == 128);
    assert!(core::mem::offset_of!(PagerGateResult, fault_gate_enabled) == 132);
    assert!(core::mem::offset_of!(PagerGateResult, pager_enabled) == 136);
    assert!(core::mem::offset_of!(PagerGateResult, eviction_enabled) == 140);
    assert!(core::mem::offset_of!(PagerGateResult, page_in_enabled) == 144);
    assert!(core::mem::offset_of!(PagerGateResult, page_out_enabled) == 148);
    assert!(core::mem::offset_of!(PagerGateResult, slot_generation) == 152);
    assert!(core::mem::offset_of!(PagerGateResult, fault_count) == 160);
    assert!(core::mem::offset_of!(PagerGateResult, failed_faults) == 168);
    assert!(core::mem::offset_of!(PagerGateResult, total_probes) == 176);
    assert!(core::mem::offset_of!(PagerGateResult, total_ready) == 184);
    assert!(core::mem::offset_of!(PagerGateResult, total_rollbacks) == 192);
    assert!(core::mem::offset_of!(PagerGateResult, total_failures) == 200);
    assert!(core::mem::size_of::<PagerGateSummary>() == 200);
    assert!(core::mem::align_of::<PagerGateSummary>() == 8);
    assert!(core::mem::offset_of!(PagerGateSummary, enabled) == 0);
    assert!(core::mem::offset_of!(PagerGateSummary, last_status) == 4);
    assert!(core::mem::offset_of!(PagerGateSummary, last_flags) == 8);
    assert!(core::mem::offset_of!(PagerGateSummary, last_blockers) == 12);
    assert!(core::mem::offset_of!(PagerGateSummary, last_region_id) == 16);
    assert!(core::mem::offset_of!(PagerGateSummary, last_owner_id) == 20);
    assert!(core::mem::offset_of!(PagerGateSummary, requested_bytes) == 24);
    assert!(core::mem::offset_of!(PagerGateSummary, committed_bytes) == 32);
    assert!(core::mem::offset_of!(PagerGateSummary, resident_bytes) == 40);
    assert!(core::mem::offset_of!(PagerGateSummary, nonresident_bytes) == 48);
    assert!(core::mem::offset_of!(PagerGateSummary, requested_slots) == 56);
    assert!(core::mem::offset_of!(PagerGateSummary, prepared_slots) == 64);
    assert!(core::mem::offset_of!(PagerGateSummary, capacity_slots) == 72);
    assert!(core::mem::offset_of!(PagerGateSummary, free_before_slots) == 80);
    assert!(core::mem::offset_of!(PagerGateSummary, free_after_slots) == 88);
    assert!(core::mem::offset_of!(PagerGateSummary, reserved_before_slots) == 96);
    assert!(core::mem::offset_of!(PagerGateSummary, reserved_after_slots) == 104);
    assert!(core::mem::offset_of!(PagerGateSummary, last_reservation_id) == 112);
    assert!(core::mem::offset_of!(PagerGateSummary, rollback_completed) == 116);
    assert!(core::mem::offset_of!(PagerGateSummary, commit_gate_enabled) == 120);
    assert!(core::mem::offset_of!(PagerGateSummary, fault_gate_enabled) == 124);
    assert!(core::mem::offset_of!(PagerGateSummary, pager_enabled) == 128);
    assert!(core::mem::offset_of!(PagerGateSummary, eviction_enabled) == 132);
    assert!(core::mem::offset_of!(PagerGateSummary, page_in_enabled) == 136);
    assert!(core::mem::offset_of!(PagerGateSummary, page_out_enabled) == 140);
    assert!(core::mem::offset_of!(PagerGateSummary, slot_generation) == 144);
    assert!(core::mem::offset_of!(PagerGateSummary, fault_count) == 152);
    assert!(core::mem::offset_of!(PagerGateSummary, failed_faults) == 160);
    assert!(core::mem::offset_of!(PagerGateSummary, probes) == 168);
    assert!(core::mem::offset_of!(PagerGateSummary, ready) == 176);
    assert!(core::mem::offset_of!(PagerGateSummary, rollbacks) == 184);
    assert!(core::mem::offset_of!(PagerGateSummary, failures) == 192);
    assert!(core::mem::size_of::<PageIoInput>() == 272);
    assert!(core::mem::align_of::<PageIoInput>() == 8);
    assert!(core::mem::offset_of!(PageIoInput, operation) == 0);
    assert!(core::mem::offset_of!(PageIoInput, region_id) == 4);
    assert!(core::mem::offset_of!(PageIoInput, region_offset) == 8);
    assert!(core::mem::offset_of!(PageIoInput, reservation_id) == 16);
    assert!(core::mem::offset_of!(PageIoInput, slot_index) == 24);
    assert!(core::mem::offset_of!(PageIoInput, page_count) == 32);
    assert!(core::mem::offset_of!(PageIoInput, owner_kind) == 40);
    assert!(core::mem::offset_of!(PageIoInput, owner_id) == 44);
    assert!(core::mem::offset_of!(PageIoInput, expected_generation) == 48);
    assert!(core::mem::offset_of!(PageIoInput, flags) == 56);
    assert!(core::mem::offset_of!(PageIoInput, vm_region_exists) == 60);
    assert!(core::mem::offset_of!(PageIoInput, vm_region_is_r4x) == 61);
    assert!(core::mem::offset_of!(PageIoInput, commit_covered) == 62);
    assert!(core::mem::offset_of!(PageIoInput, committed_bytes) == 64);
    assert!(core::mem::offset_of!(PageIoInput, resident_bytes) == 72);
    assert!(core::mem::offset_of!(PageIoInput, backing) == 80);
    assert!(core::mem::offset_of!(PageIoInput, io_status) == 264);
    assert!(core::mem::offset_of!(PageIoInput, io_bytes) == 268);
    assert!(core::mem::size_of::<PageIoResult>() == 296);
    assert!(core::mem::align_of::<PageIoResult>() == 8);
    assert!(core::mem::offset_of!(PageIoResult, version) == 0);
    assert!(core::mem::offset_of!(PageIoResult, status) == 4);
    assert!(core::mem::offset_of!(PageIoResult, operation) == 8);
    assert!(core::mem::offset_of!(PageIoResult, flags) == 12);
    assert!(core::mem::offset_of!(PageIoResult, blockers) == 16);
    assert!(core::mem::offset_of!(PageIoResult, region_id) == 20);
    assert!(core::mem::offset_of!(PageIoResult, reservation_id) == 24);
    assert!(core::mem::offset_of!(PageIoResult, owner_kind) == 28);
    assert!(core::mem::offset_of!(PageIoResult, owner_id) == 32);
    assert!(core::mem::offset_of!(PageIoResult, slot_bytes) == 36);
    assert!(core::mem::offset_of!(PageIoResult, reserved0) == 40);
    assert!(core::mem::offset_of!(PageIoResult, region_offset) == 48);
    assert!(core::mem::offset_of!(PageIoResult, committed_bytes) == 56);
    assert!(core::mem::offset_of!(PageIoResult, resident_bytes) == 64);
    assert!(core::mem::offset_of!(PageIoResult, slot_index) == 72);
    assert!(core::mem::offset_of!(PageIoResult, page_count) == 80);
    assert!(core::mem::offset_of!(PageIoResult, transfer_bytes) == 88);
    assert!(core::mem::offset_of!(PageIoResult, expected_generation) == 96);
    assert!(core::mem::offset_of!(PageIoResult, backing_slot) == 104);
    assert!(core::mem::offset_of!(PageIoResult, backing_offset) == 112);
    assert!(core::mem::offset_of!(PageIoResult, capacity_slots) == 120);
    assert!(core::mem::offset_of!(PageIoResult, reserved_slots) == 128);
    assert!(core::mem::offset_of!(PageIoResult, valid_slots) == 136);
    assert!(core::mem::offset_of!(PageIoResult, dirty_slots) == 144);
    assert!(core::mem::offset_of!(PageIoResult, error_slots) == 152);
    assert!(core::mem::offset_of!(PageIoResult, io_bytes) == 160);
    assert!(core::mem::offset_of!(PageIoResult, io_status) == 164);
    assert!(core::mem::offset_of!(PageIoResult, pager_enabled) == 168);
    assert!(core::mem::offset_of!(PageIoResult, eviction_enabled) == 172);
    assert!(core::mem::offset_of!(PageIoResult, page_in_enabled) == 176);
    assert!(core::mem::offset_of!(PageIoResult, page_out_enabled) == 180);
    assert!(core::mem::offset_of!(PageIoResult, retry_limit) == 184);
    assert!(core::mem::offset_of!(PageIoResult, backoff_ticks) == 188);
    assert!(core::mem::offset_of!(PageIoResult, slot_generation) == 192);
    assert!(core::mem::offset_of!(PageIoResult, total_prepares) == 200);
    assert!(core::mem::offset_of!(PageIoResult, total_page_outs) == 208);
    assert!(core::mem::offset_of!(PageIoResult, total_page_ins) == 216);
    assert!(core::mem::offset_of!(PageIoResult, total_failures) == 224);
    assert!(core::mem::offset_of!(PageIoResult, total_retry_attempts) == 232);
    assert!(core::mem::offset_of!(PageIoResult, total_retryable_failures) == 240);
    assert!(core::mem::offset_of!(PageIoResult, total_permanent_failures) == 248);
    assert!(core::mem::offset_of!(PageIoResult, total_retry_limit_hits) == 256);
    assert!(core::mem::offset_of!(PageIoResult, total_failed_page_outs) == 264);
    assert!(core::mem::offset_of!(PageIoResult, total_failed_page_ins) == 272);
    assert!(core::mem::offset_of!(PageIoResult, total_data_preserved_pages) == 280);
    assert!(core::mem::offset_of!(PageIoResult, total_data_lost_pages) == 288);
    assert!(core::mem::size_of::<PageIoSummary>() == 264);
    assert!(core::mem::align_of::<PageIoSummary>() == 8);
    assert!(core::mem::offset_of!(PageIoSummary, enabled) == 0);
    assert!(core::mem::offset_of!(PageIoSummary, last_status) == 4);
    assert!(core::mem::offset_of!(PageIoSummary, last_operation) == 8);
    assert!(core::mem::offset_of!(PageIoSummary, last_flags) == 12);
    assert!(core::mem::offset_of!(PageIoSummary, last_blockers) == 16);
    assert!(core::mem::offset_of!(PageIoSummary, last_region_id) == 20);
    assert!(core::mem::offset_of!(PageIoSummary, last_reservation_id) == 24);
    assert!(core::mem::offset_of!(PageIoSummary, last_owner_kind) == 28);
    assert!(core::mem::offset_of!(PageIoSummary, last_owner_id) == 32);
    assert!(core::mem::offset_of!(PageIoSummary, region_offset) == 40);
    assert!(core::mem::offset_of!(PageIoSummary, page_count) == 48);
    assert!(core::mem::offset_of!(PageIoSummary, transfer_bytes) == 56);
    assert!(core::mem::offset_of!(PageIoSummary, expected_generation) == 64);
    assert!(core::mem::offset_of!(PageIoSummary, backing_slot) == 72);
    assert!(core::mem::offset_of!(PageIoSummary, backing_offset) == 80);
    assert!(core::mem::offset_of!(PageIoSummary, io_bytes) == 88);
    assert!(core::mem::offset_of!(PageIoSummary, io_status) == 92);
    assert!(core::mem::offset_of!(PageIoSummary, capacity_slots) == 96);
    assert!(core::mem::offset_of!(PageIoSummary, reserved_slots) == 104);
    assert!(core::mem::offset_of!(PageIoSummary, valid_slots) == 112);
    assert!(core::mem::offset_of!(PageIoSummary, dirty_slots) == 120);
    assert!(core::mem::offset_of!(PageIoSummary, error_slots) == 128);
    assert!(core::mem::offset_of!(PageIoSummary, pager_enabled) == 136);
    assert!(core::mem::offset_of!(PageIoSummary, eviction_enabled) == 140);
    assert!(core::mem::offset_of!(PageIoSummary, page_in_enabled) == 144);
    assert!(core::mem::offset_of!(PageIoSummary, page_out_enabled) == 148);
    assert!(core::mem::offset_of!(PageIoSummary, retry_limit) == 152);
    assert!(core::mem::offset_of!(PageIoSummary, backoff_ticks) == 156);
    assert!(core::mem::offset_of!(PageIoSummary, slot_generation) == 160);
    assert!(core::mem::offset_of!(PageIoSummary, prepares) == 168);
    assert!(core::mem::offset_of!(PageIoSummary, page_outs) == 176);
    assert!(core::mem::offset_of!(PageIoSummary, page_ins) == 184);
    assert!(core::mem::offset_of!(PageIoSummary, failures) == 192);
    assert!(core::mem::offset_of!(PageIoSummary, retry_attempts) == 200);
    assert!(core::mem::offset_of!(PageIoSummary, retryable_failures) == 208);
    assert!(core::mem::offset_of!(PageIoSummary, permanent_failures) == 216);
    assert!(core::mem::offset_of!(PageIoSummary, retry_limit_hits) == 224);
    assert!(core::mem::offset_of!(PageIoSummary, failed_page_outs) == 232);
    assert!(core::mem::offset_of!(PageIoSummary, failed_page_ins) == 240);
    assert!(core::mem::offset_of!(PageIoSummary, data_preserved_pages) == 248);
    assert!(core::mem::offset_of!(PageIoSummary, data_lost_pages) == 256);
    assert!(core::mem::size_of::<SlotExtent>() == 48);
    assert!(core::mem::align_of::<SlotExtent>() == 8);
    assert!(core::mem::offset_of!(SlotExtent, reservation_id) == 0);
    assert!(core::mem::offset_of!(SlotExtent, owner_kind) == 4);
    assert!(core::mem::offset_of!(SlotExtent, owner_id) == 8);
    assert!(core::mem::offset_of!(SlotExtent, region_id) == 12);
    assert!(core::mem::offset_of!(SlotExtent, start_slot) == 16);
    assert!(core::mem::offset_of!(SlotExtent, slot_count) == 24);
    assert!(core::mem::offset_of!(SlotExtent, state) == 32);
    assert!(core::mem::offset_of!(SlotExtent, generation) == 40);
    assert!(core::mem::size_of::<LifecycleCleanupResult>() == 48);
    assert!(core::mem::align_of::<LifecycleCleanupResult>() == 8);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, cleanup_count) == 0);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, released_ranges) == 8);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, released_slots) == 16);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, released_valid_slots) == 24);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, released_dirty_slots) == 32);
    assert!(core::mem::offset_of!(LifecycleCleanupResult, released_error_slots) == 40);
};
