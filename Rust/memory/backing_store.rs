//! Backing identity, reservations and pager transfer metadata have one owner.
//! These algorithms never perform file I/O or wait. The existing VM owner
//! admits each call; no Engine borrow survives that owner or a C return.
#![allow(non_upper_case_globals)]
use super::backing_types::*;
use core::cell::UnsafeCell;
use core::ptr;

struct Engine {
    state: Summary,
    slot_state: SlotSummary,
    pager_gate_state: PagerGateSummary,
    page_io_state: PageIoSummary,
    slot_extents: [SlotExtent; SLOT_TABLE_MAX_RANGES],
    active_first_cluster: u32,
    active_file_size: u64,
    active_cluster_bytes: u32,
    active_capacity_slots: u64,
    active_backing_path: [u8; MAX_BACKING_PATH + 1],
    active_backing_path_valid: bool,
    slot_generation: u64,
    next_reservation_id: u32,
}
impl Engine {
    const INIT: Self = Self {
        state: Summary::INIT,
        slot_state: SlotSummary::INIT,
        pager_gate_state: PagerGateSummary::INIT,
        page_io_state: PageIoSummary::INIT,
        slot_extents: [SlotExtent::INIT; SLOT_TABLE_MAX_RANGES],
        active_first_cluster: 0,
        active_file_size: 0,
        active_cluster_bytes: 0,
        active_capacity_slots: 0,
        active_backing_path: [0; MAX_BACKING_PATH + 1],
        active_backing_path_valid: false,
        slot_generation: 0,
        next_reservation_id: 1,
    };

    // SAFETY: a nonnull path denotes a readable NUL-terminated string for
    // this call. It is inspected only after the original admission checks.
    unsafe fn probe(&mut self, input: Input) -> Result {
        let mut result = Result {
            requested_bytes: input.requested_bytes,
            file_size: input.file_size,
            cluster_bytes: input.cluster_bytes,
            first_cluster: input.first_cluster,
            ..Result::INIT
        };
        if input.flags & !supported_probe_flags != 0 {
            result.status = status_unsupported_flags;
            result.blockers |= blocker_unsupported_flags;
        } else if input.requested_bytes == 0 {
            result.status = status_invalid_request;
            result.blockers |= blocker_invalid_request;
        } else if input.requested_bytes & (PAGE_BYTES - 1) != 0 {
            result.status = status_invalid_request;
            result.blockers |= blocker_unaligned_request;
        } else {
            result.flags |= flag_page_aligned_request;
            if !input.fat32 {
                result.status = status_unsupported_fs;
                result.blockers |= blocker_unsupported_fs;
            } else {
                result.flags |= flag_fat32;
                if !input.file_exists {
                    result.status = status_missing_file;
                    result.blockers |= blocker_missing_file;
                } else if input.is_dir {
                    result.status = status_directory;
                    result.blockers |= blocker_directory;
                } else {
                    result.flags |= flag_file_backed | flag_existing_file;
                    result.available_bytes = input.file_size;
                    if input.file_size < input.requested_bytes {
                        result.status = status_too_small;
                        result.blockers |= blocker_too_small;
                    } else {
                        result.status = status_ready;
                        if !input.path.is_null() {
                            let mut len = 0;
                            while unsafe { input.path.add(len).read() } != 0 {
                                len += 1;
                            }
                            // The Zig sentinel array has128 logical bytes
                            // and a separate physical sentinel at index128.
                            if len >= MAX_BACKING_PATH {
                                result.status = status_invalid_request;
                                result.blockers |= blocker_invalid_request;
                            } else {
                                unsafe {
                                    ptr::copy_nonoverlapping(
                                        input.path,
                                        result.path.as_mut_ptr(),
                                        len,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        self.record(result);
        result
    }

    fn active_backing_result(&self) -> Option<Result> {
        if !self.active_backing_path_valid
            || self.active_capacity_slots == 0
            || self.active_file_size == 0
        {
            return None;
        }
        let available_bytes = self.active_capacity_slots * SLOT_BYTES;
        Some(Result {
            path: self.active_backing_path,
            status: status_ready,
            flags: flag_file_backed
                | flag_existing_file
                | flag_fat32
                | flag_reserve_only
                | flag_pager_disabled
                | flag_uses_fs_api
                | flag_no_second_io_path
                | flag_page_aligned_request,
            requested_bytes: available_bytes,
            available_bytes,
            file_size: self.active_file_size,
            cluster_bytes: self.active_cluster_bytes,
            first_cluster: self.active_first_cluster,
            ..Result::INIT
        })
    }

    fn slot_probe(&mut self, input: SlotInput) -> SlotResult {
        let mut result = SlotResult {
            operation: input.operation,
            requested_slots: input.requested_slots,
            reservation_id: input.reservation_id,
            owner_kind: input.owner_kind,
            owner_id: input.owner_id,
            region_id: input.region_id,
            ..SlotResult::INIT
        };
        if input.flags & !supported_slot_flags != 0 {
            result.status = slot_status_unsupported_flags;
            result.blockers |= slot_blocker_unsupported_flags;
        } else if !valid_slot_owner_kind(input.owner_kind) {
            result.status = slot_status_invalid_request;
            result.blockers |= slot_blocker_invalid_owner;
        } else if input.backing.status != status_ready {
            result.status = slot_status_backing_unavailable;
            result.blockers |= slot_blocker_backing_not_ready;
        } else {
            result.flags |= slot_flag_file_backed | slot_flag_backing_ready;
            if input.backing.available_bytes & (SLOT_BYTES - 1) != 0 {
                result.status = slot_status_invalid_request;
                result.blockers |= slot_blocker_unaligned_backing;
            } else {
                let capacity_slots = input.backing.available_bytes / SLOT_BYTES;
                if capacity_slots == 0 {
                    result.status = slot_status_invalid_request;
                    result.blockers |= slot_blocker_zero_capacity;
                } else if !self.ensure_slot_backing(input.backing, capacity_slots) {
                    result.status = slot_status_backing_unavailable;
                    result.blockers |= slot_blocker_backing_not_ready;
                    result.capacity_slots = self.active_capacity_slots;
                    self.fill_slot_counts(&mut result);
                } else {
                    result.capacity_slots = self.active_capacity_slots;
                    self.fill_slot_counts(&mut result);
                    match input.operation {
                        slot_operation_probe => result.status = slot_status_ready,
                        slot_operation_reserve => self.reserve_slots(input, &mut result),
                        slot_operation_release => self.release_slots(input, &mut result),
                        slot_operation_mark_error => self.mark_slots_error(input, &mut result),
                        slot_operation_recover => self.recover_slots(input, &mut result),
                        _ => {
                            result.status = slot_status_unsupported_operation;
                            result.blockers |= slot_blocker_unsupported_operation;
                        }
                    }
                    self.fill_slot_counts(&mut result);
                }
            }
        }
        self.record_slot(&mut result);
        result
    }

    fn release_r4x_owner(&mut self, owner_id: u32) -> LifecycleCleanupResult {
        let mut result = self.cleanup_matching(slot_owner_kind_r4x_instance, owner_id, 0, false);
        let vm = self.cleanup_matching(slot_owner_kind_vm_region, owner_id, 0, false);
        add_cleanup(&mut result, vm);
        result
    }
    fn release_vm_region(&mut self, owner_id: u32, region_id: u32) -> LifecycleCleanupResult {
        let mut result = self.cleanup_region(slot_owner_kind_vm_region, owner_id, region_id);
        let instance = self.cleanup_region(slot_owner_kind_r4x_instance, owner_id, region_id);
        add_cleanup(&mut result, instance);
        result
    }
    fn pager_gate_probe(&mut self, input: PagerGateInput) -> PagerGateResult {
        let nonresident_bytes = input.committed_bytes.saturating_sub(input.resident_bytes);
        let mut result = PagerGateResult {
            region_id: input.region_id,
            owner_id: input.owner_id,
            requested_bytes: if input.requested_bytes != 0 {
                input.requested_bytes
            } else {
                nonresident_bytes
            },
            committed_bytes: input.committed_bytes,
            resident_bytes: input.resident_bytes,
            nonresident_bytes,
            fault_count: input.fault_count,
            failed_faults: input.failed_faults,
            ..PagerGateResult::INIT
        };
        self.pager_gate(input, &mut result);
        self.record_pager_gate(&mut result);
        result
    }
    fn pager_gate(&mut self, input: PagerGateInput, result: &mut PagerGateResult) {
        if input.flags & !supported_pager_gate_flags != 0 {
            result.status = pager_gate_status_unsupported_flags;
            result.blockers |= pager_gate_blocker_unsupported_flags;
            return;
        }
        if !input.vm_region_exists {
            result.status = pager_gate_status_vm_region_missing;
            result.blockers |= pager_gate_blocker_vm_region_missing;
            return;
        }
        result.flags |= pager_gate_flag_vm_region_attached;
        if !input.vm_region_is_r4x {
            result.status = pager_gate_status_invalid_request;
            result.blockers |= pager_gate_blocker_vm_region_not_r4x;
            return;
        }
        if input.backing.status != status_ready {
            result.status = pager_gate_status_backing_unavailable;
            result.blockers |= pager_gate_blocker_backing_not_ready;
            return;
        }
        result.flags |= pager_gate_flag_file_backed | pager_gate_flag_backing_ready;
        if result.requested_bytes == 0 {
            result.status = pager_gate_status_no_nonresident_commit;
            result.blockers |= pager_gate_blocker_no_nonresident_commit;
            return;
        }
        if result.requested_bytes & (PAGE_BYTES - 1) != 0 {
            result.status = pager_gate_status_invalid_request;
            result.blockers |= pager_gate_blocker_unaligned_request;
            return;
        }
        let requested_slots = result.requested_bytes / SLOT_BYTES;
        result.requested_slots = requested_slots;
        let base = SlotInput {
            owner_id: input.owner_id,
            backing: input.backing,
            ..SlotInput::INIT
        };
        let probe = self.slot_probe(base);
        result.capacity_slots = probe.capacity_slots;
        result.free_before_slots = probe.free_slots;
        result.reserved_before_slots = probe.reserved_slots;
        gate_counts(result, probe);
        if probe.status != slot_status_ready {
            map_slot_failure_to_pager_gate(probe, result);
            return;
        }
        if requested_slots > probe.free_slots {
            result.status = pager_gate_status_insufficient_capacity;
            result.blockers |= pager_gate_blocker_insufficient_capacity;
            return;
        }
        let reserved = self.slot_probe(SlotInput {
            operation: slot_operation_reserve,
            requested_slots,
            ..base
        });
        gate_counts(result, reserved);
        result.slot_reservation_id = reserved.reservation_id;
        if reserved.status != slot_status_reserved {
            map_slot_failure_to_pager_gate(reserved, result);
            return;
        }
        result.flags |= pager_gate_flag_slot_reservation_tested;
        result.prepared_slots = reserved.slot_count;
        let released = self.slot_probe(SlotInput {
            operation: slot_operation_release,
            reservation_id: reserved.reservation_id,
            ..base
        });
        gate_counts(result, released);
        if released.status != slot_status_released {
            result.status = pager_gate_status_rollback_failed;
            result.blockers |= pager_gate_blocker_rollback_failed;
            return;
        }
        result.status = pager_gate_status_ready;
        result.flags |= pager_gate_flag_rollback_complete;
        result.rollback_completed = 1;
        result.slot_reservation_id = released.reservation_id;
        gate_counts(result, self.slot_probe(base));
    }
    fn page_io_prepare(&mut self, input: PageIoInput) -> PageIoResult {
        let mut result = make_page_io_result(input);
        if let Some(index) = self.validate_page_io(input, &mut result) {
            let extent = self.slot_extents[index];
            self.fill_page_io_slot_state(&mut result);
            result.status = page_io_status_ready;
            result.backing_slot = extent.start_slot + input.slot_index;
            result.backing_offset = result.backing_slot * SLOT_BYTES;
        }
        self.record_page_io(&mut result);
        result
    }
    fn page_io_complete(&mut self, input: PageIoInput) -> PageIoResult {
        let mut result = make_page_io_result(input);
        let Some(index) = self.validate_page_io(input, &mut result) else {
            self.record_page_io(&mut result);
            return result;
        };
        self.fill_page_io_slot_state(&mut result);
        result.backing_slot = self.slot_extents[index].start_slot + input.slot_index;
        result.backing_offset = result.backing_slot * SLOT_BYTES;
        result.io_status = input.io_status;
        result.io_bytes = input.io_bytes;
        let expected_io_status = i32::try_from(result.transfer_bytes).expect("validated transfer");
        if input.io_status != expected_io_status
            || u64::from(input.io_bytes) != result.transfer_bytes
        {
            self.slot_extents[index].state |= extent_state_error;
            self.slot_generation = self.slot_generation.wrapping_add(1);
            result.slot_generation = self.slot_generation;
            if input.io_status > 0 || input.io_bytes != 0 {
                result.status = page_io_status_partial_io;
                result.blockers |= page_io_blocker_partial_io;
            } else {
                result.status = page_io_status_io_failed;
                result.blockers |= page_io_blocker_io_failed;
            }
            self.fill_page_io_slot_state(&mut result);
            self.record_page_io(&mut result);
            return result;
        }
        match input.operation {
            page_io_operation_page_out => {
                self.slot_extents[index].state |= extent_state_valid;
                self.slot_extents[index].state &= !extent_state_dirty;
                self.slot_generation = self.slot_generation.wrapping_add(1);
                result.status = page_io_status_page_out_ok;
                result.flags |= page_io_flag_slot_valid | page_io_flag_slot_clean;
            }
            page_io_operation_page_in => result.status = page_io_status_page_in_ok,
            _ => {
                result.status = page_io_status_invalid_request;
                result.blockers |= page_io_blocker_invalid_request;
            }
        }
        result.slot_generation = self.slot_generation;
        self.fill_page_io_slot_state(&mut result);
        self.record_page_io(&mut result);
        result
    }
    fn ensure_slot_backing(&mut self, backing: Result, capacity_slots: u64) -> bool {
        if self.active_first_cluster == backing.first_cluster
            && self.active_cluster_bytes == backing.cluster_bytes
            && self.active_file_size == backing.file_size
            && self.active_capacity_slots == capacity_slots
            && path_eq(&self.active_backing_path, &backing.path)
        {
            return true;
        }
        // Live reservations retain the exact file and its only page-in map.
        if self.slot_extents.iter().copied().any(extent_active) {
            return false;
        }
        self.slot_extents.fill(SlotExtent::INIT);
        self.active_first_cluster = backing.first_cluster;
        self.active_file_size = backing.file_size;
        self.active_cluster_bytes = backing.cluster_bytes;
        self.active_capacity_slots = capacity_slots;
        self.remember_active_path((backing.path[0] != 0).then_some(&backing.path));
        self.slot_generation = self.slot_generation.wrapping_add(1);
        true
    }
    fn remember_active_path(&mut self, path: Option<&[u8; MAX_BACKING_PATH + 1]>) {
        let Some(path) = path else {
            self.active_backing_path_valid = false;
            self.active_backing_path[0] = 0;
            return;
        };
        let mut len = 0;
        while len + 1 < MAX_BACKING_PATH && path[len] != 0 {
            self.active_backing_path[len] = path[len];
            len += 1;
        }
        if path[len] != 0 {
            self.active_backing_path_valid = false;
            self.active_backing_path[0] = 0;
            return;
        }
        self.active_backing_path[len] = 0;
        let mut clear = len + 1;
        while clear < MAX_BACKING_PATH {
            self.active_backing_path[clear] = 0;
            clear += 1;
        }
        self.active_backing_path_valid = true;
    }
    fn next_slot_generation(&mut self) -> u64 {
        self.slot_generation = self.slot_generation.wrapping_add(1);
        if self.slot_generation == 0 {
            self.slot_generation = 1;
        }
        self.slot_generation
    }
    fn reserve_slots(&mut self, input: SlotInput, result: &mut SlotResult) {
        if input.requested_slots == 0 {
            result.status = slot_status_invalid_request;
            result.blockers |= slot_blocker_invalid_request;
            return;
        }
        let reserved = self.count_reserved_slots();
        let free = self.active_capacity_slots.saturating_sub(reserved);
        if input.requested_slots > free {
            result.status = slot_status_insufficient_capacity;
            result.blockers |= slot_blocker_insufficient_capacity;
            return;
        }
        let Some(index) = self.find_free_extent_index() else {
            result.status = slot_status_table_full;
            result.blockers |= slot_blocker_table_full;
            return;
        };
        let Some(first_slot) =
            self.find_free_slot_range(input.requested_slots, self.active_capacity_slots)
        else {
            result.status = slot_status_insufficient_capacity;
            result.blockers |= slot_blocker_insufficient_capacity;
            return;
        };
        let Some(reservation_id) = self.allocate_reservation_id() else {
            result.status = slot_status_table_full;
            result.blockers |= slot_blocker_table_full;
            return;
        };
        self.slot_extents[index] = SlotExtent {
            reservation_id,
            owner_kind: input.owner_kind,
            owner_id: input.owner_id,
            region_id: input.region_id,
            start_slot: first_slot,
            slot_count: input.requested_slots,
            state: extent_state_reserved,
            generation: self.next_slot_generation(),
        };
        result.status = slot_status_reserved;
        result.reservation_id = reservation_id;
        result.owner_kind = input.owner_kind;
        result.owner_id = input.owner_id;
        result.region_id = input.region_id;
        result.first_slot = first_slot;
        result.slot_count = input.requested_slots;
    }
    fn release_slots(&mut self, input: SlotInput, result: &mut SlotResult) {
        let Some(index) = self.admit_extent(input, result) else {
            return;
        };
        let extent = self.slot_extents[index];
        result.status = slot_status_released;
        slot_identity(result, extent);
        self.slot_extents[index] = SlotExtent::INIT;
        self.slot_generation = self.slot_generation.wrapping_add(1);
    }
    fn mark_slots_error(&mut self, input: SlotInput, result: &mut SlotResult) {
        let Some(index) = self.admit_extent(input, result) else {
            return;
        };
        self.slot_extents[index].state |= extent_state_error;
        result.status = slot_status_error_marked;
        slot_identity(result, self.slot_extents[index]);
        self.slot_generation = self.slot_generation.wrapping_add(1);
    }
    fn recover_slots(&mut self, input: SlotInput, result: &mut SlotResult) {
        let Some(index) = self.admit_extent(input, result) else {
            return;
        };
        self.slot_extents[index].state = extent_state_reserved;
        self.slot_extents[index].generation = self.next_slot_generation();
        result.status = slot_status_recovered;
        slot_identity(result, self.slot_extents[index]);
        self.slot_generation = self.slot_generation.wrapping_add(1);
    }
    fn admit_extent(&self, input: SlotInput, result: &mut SlotResult) -> Option<usize> {
        if input.reservation_id == 0 {
            result.status = slot_status_invalid_request;
            result.blockers |= slot_blocker_invalid_request;
            return None;
        }
        let Some(index) = self.find_extent_by_reservation(input.reservation_id) else {
            result.status = slot_status_reservation_not_found;
            result.blockers |= slot_blocker_reservation_not_found;
            return None;
        };
        let extent = self.slot_extents[index];
        if !owner_matches(extent, input.owner_kind, input.owner_id, input.region_id) {
            result.status = slot_status_owner_mismatch;
            result.blockers |= slot_blocker_owner_mismatch;
            slot_identity(result, extent);
            return None;
        }
        Some(index)
    }
    fn fill_slot_counts(&self, result: &mut SlotResult) {
        let mut reserved = 0u64;
        let mut valid = 0u64;
        let mut dirty = 0u64;
        let mut errors = 0u64;
        let mut ranges = 0u32;
        for extent in self.slot_extents.iter().copied() {
            if !extent_active(extent) {
                continue;
            }
            ranges += 1;
            reserved = reserved.wrapping_add(extent.slot_count);
            if extent.state & extent_state_valid != 0 {
                valid = valid.wrapping_add(extent.slot_count)
            }
            if extent.state & extent_state_dirty != 0 {
                dirty = dirty.wrapping_add(extent.slot_count)
            }
            if extent.state & extent_state_error != 0 {
                errors = errors.wrapping_add(extent.slot_count)
            }
        }
        result.capacity_slots = self.active_capacity_slots;
        result.reserved_slots = reserved;
        result.free_slots = self.active_capacity_slots.saturating_sub(reserved);
        result.valid_slots = valid;
        result.dirty_slots = dirty;
        result.error_slots = errors;
        result.range_count = ranges;
        result.max_ranges = slot_table_max_ranges;
        result.generation = self
            .find_extent_by_reservation(result.reservation_id)
            .map_or(self.slot_generation, |index| {
                self.slot_extents[index].generation
            });
    }
    fn validate_page_io(&self, input: PageIoInput, result: &mut PageIoResult) -> Option<usize> {
        if input.flags & !supported_page_io_flags != 0 {
            result.status = page_io_status_unsupported_flags;
            result.blockers |= page_io_blocker_unsupported_flags;
            return None;
        }
        if !valid_slot_owner_kind(input.owner_kind) {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_owner;
            return None;
        }
        if input.operation != page_io_operation_page_out
            && input.operation != page_io_operation_page_in
        {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_request;
            return None;
        }
        if input.page_count == 0 || input.page_count > u64::MAX / PAGE_BYTES {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_request;
            return None;
        }
        let transfer_bytes = input.page_count * PAGE_BYTES;
        result.transfer_bytes = transfer_bytes;
        if transfer_bytes > i32::MAX as u64 {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_request;
            return None;
        }
        if input.page_count > 1 {
            result.flags |= page_io_flag_multi_page
        }
        if !input.vm_region_exists {
            result.status = page_io_status_vm_region_missing;
            result.blockers |= page_io_blocker_vm_region_missing;
            return None;
        }
        result.flags |= page_io_flag_vm_region_attached;
        if !input.vm_region_is_r4x {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_vm_region_not_r4x;
            return None;
        }
        if input.region_offset & (PAGE_BYTES - 1) != 0 {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_unaligned_region_offset;
            return None;
        }
        if input.region_offset > u64::MAX - transfer_bytes || !input.commit_covered {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_region_offset_outside_commit;
            return None;
        }
        if input.backing.status != status_ready {
            result.status = page_io_status_backing_unavailable;
            result.blockers |= page_io_blocker_backing_not_ready;
            return None;
        }
        result.flags |= page_io_flag_file_backed | page_io_flag_backing_ready;
        if input.reservation_id == 0 {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_invalid_request;
            return None;
        }
        let capacity_slots = input.backing.available_bytes / SLOT_BYTES;
        if self.active_first_cluster != input.backing.first_cluster
            || self.active_file_size != input.backing.file_size
            || self.active_cluster_bytes != input.backing.cluster_bytes
            || self.active_capacity_slots != capacity_slots
            || !path_eq(&self.active_backing_path, &input.backing.path)
        {
            result.status = page_io_status_stale_generation;
            result.blockers |= page_io_blocker_stale_generation;
            return None;
        }
        result.capacity_slots = self.active_capacity_slots;
        self.fill_page_io_slot_state(result);
        let Some(index) = self.find_extent_by_reservation(input.reservation_id) else {
            result.status = page_io_status_reservation_not_found;
            result.blockers |= page_io_blocker_reservation_not_found;
            return None;
        };
        let extent = self.slot_extents[index];
        result.flags |= page_io_flag_slot_reserved;
        if !owner_matches(extent, input.owner_kind, input.owner_id, input.region_id) {
            result.status = page_io_status_owner_mismatch;
            result.blockers |= page_io_blocker_owner_mismatch;
            result.owner_kind = extent.owner_kind;
            result.owner_id = extent.owner_id;
            return None;
        }
        result.flags |= page_io_flag_owner_matched;
        if input.expected_generation != 0 {
            result.flags |= page_io_flag_generation_checked;
            if input.expected_generation != extent.generation {
                result.status = page_io_status_stale_generation;
                result.blockers |= page_io_blocker_stale_generation;
                result.slot_generation = extent.generation;
                return None;
            }
        }
        if input.slot_index >= extent.slot_count
            || input.page_count > extent.slot_count - input.slot_index
        {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_slot_index_out_of_range;
            return None;
        }
        if input.operation == page_io_operation_page_out
            && extent.state & extent_state_valid == 0
            && (input.slot_index != 0 || input.page_count != extent.slot_count)
        {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_slot_index_out_of_range;
            return None;
        }
        if extent.state & extent_state_valid != 0 {
            result.flags |= page_io_flag_slot_valid
        }
        result.flags |= if extent.state & extent_state_dirty != 0 {
            page_io_flag_slot_dirty
        } else {
            page_io_flag_slot_clean
        };
        if extent.state & extent_state_error != 0 {
            result.status = page_io_status_slot_error;
            result.blockers |= page_io_blocker_slot_error;
            return None;
        }
        if input.operation == page_io_operation_page_out
            && extent.state & extent_state_valid != 0
            && input.flags & page_io_flag_eviction_request == 0
        {
            result.status = page_io_status_slot_already_valid;
            result.blockers |= page_io_blocker_slot_already_valid;
            return None;
        }
        if input.operation == page_io_operation_page_in && extent.state & extent_state_valid == 0 {
            result.status = page_io_status_slot_not_valid;
            result.blockers |= page_io_blocker_slot_not_valid;
            return None;
        }
        let backing_slot = extent.start_slot + input.slot_index;
        let backing_offset = backing_slot * SLOT_BYTES;
        if backing_offset > u64::MAX - transfer_bytes
            || backing_offset + transfer_bytes > input.backing.available_bytes
        {
            result.status = page_io_status_invalid_request;
            result.blockers |= page_io_blocker_slot_index_out_of_range;
            return None;
        }
        result.backing_slot = backing_slot;
        result.backing_offset = backing_offset;
        result.slot_generation = extent.generation;
        Some(index)
    }
    fn fill_page_io_slot_state(&self, result: &mut PageIoResult) {
        let mut slot = SlotResult {
            reservation_id: result.reservation_id,
            ..SlotResult::INIT
        };
        self.fill_slot_counts(&mut slot);
        result.capacity_slots = slot.capacity_slots;
        result.reserved_slots = slot.reserved_slots;
        result.valid_slots = slot.valid_slots;
        result.dirty_slots = slot.dirty_slots;
        result.error_slots = slot.error_slots;
        result.slot_generation = slot.generation;
    }
    fn record_page_io_failure_policy(&mut self, result: &mut PageIoResult) {
        if page_io_failure_retryable(result.status) {
            result.flags |= page_io_flag_retryable_failure;
            self.page_io_state.retryable_failures =
                self.page_io_state.retryable_failures.wrapping_add(1);
        } else {
            result.flags |= page_io_flag_permanent_failure;
            self.page_io_state.permanent_failures =
                self.page_io_state.permanent_failures.wrapping_add(1);
        }
        if result.flags & page_io_flag_retry_request != 0
            && !page_io_failure_retryable(result.status)
        {
            self.page_io_state.retry_limit_hits =
                self.page_io_state.retry_limit_hits.wrapping_add(1);
        }
        if result.operation == page_io_operation_page_out {
            self.page_io_state.failed_page_outs =
                self.page_io_state.failed_page_outs.wrapping_add(1);
            self.page_io_state.data_preserved_pages = self
                .page_io_state
                .data_preserved_pages
                .wrapping_add(result.page_count);
            result.flags |= page_io_flag_data_preserved;
        } else if result.operation == page_io_operation_page_in {
            self.page_io_state.failed_page_ins = self.page_io_state.failed_page_ins.wrapping_add(1);
            if matches!(
                result.status,
                page_io_status_io_failed
                    | page_io_status_partial_io
                    | page_io_status_backing_unavailable
            ) {
                self.page_io_state.data_preserved_pages = self
                    .page_io_state
                    .data_preserved_pages
                    .wrapping_add(result.page_count);
                result.flags |= page_io_flag_data_preserved;
            }
        }
    }
    fn count_reserved_slots(&self) -> u64 {
        let mut reserved = 0u64;
        for extent in self.slot_extents.iter().copied() {
            if extent_active(extent) {
                reserved = reserved.wrapping_add(extent.slot_count)
            }
        }
        reserved
    }
    fn find_free_extent_index(&self) -> Option<usize> {
        self.slot_extents
            .iter()
            .position(|extent| !extent_active(*extent))
    }
    fn find_free_slot_range(&self, requested_slots: u64, capacity_slots: u64) -> Option<u64> {
        if requested_slots == 0 || requested_slots > capacity_slots {
            return None;
        }
        let mut start = 0;
        while start <= capacity_slots - requested_slots {
            let end = start + requested_slots;
            let mut moved = false;
            for extent in self.slot_extents.iter().copied() {
                if !extent_active(extent) {
                    continue;
                }
                let extent_end = extent.start_slot + extent.slot_count;
                if start < extent_end && end > extent.start_slot {
                    start = extent_end;
                    moved = true;
                    break;
                }
            }
            if !moved {
                return Some(start);
            }
        }
        None
    }
    fn allocate_reservation_id(&mut self) -> Option<u32> {
        let start = self.next_reservation_id;
        loop {
            let id = self.next_reservation_id;
            self.next_reservation_id = self.next_reservation_id.wrapping_add(1);
            if self.next_reservation_id == 0 {
                self.next_reservation_id = 1
            }
            if id != 0 && self.find_extent_by_reservation(id).is_none() {
                return Some(id);
            }
            if self.next_reservation_id == start {
                return None;
            }
        }
    }
    fn cleanup_region(
        &mut self,
        owner_kind: u32,
        owner_id: u32,
        region_id: u32,
    ) -> LifecycleCleanupResult {
        if region_id == 0 {
            return LifecycleCleanupResult::INIT;
        }
        self.cleanup_matching(owner_kind, owner_id, region_id, true)
    }
    fn cleanup_matching(
        &mut self,
        owner_kind: u32,
        owner_id: u32,
        region_id: u32,
        match_region: bool,
    ) -> LifecycleCleanupResult {
        if !valid_slot_owner_kind(owner_kind) {
            return LifecycleCleanupResult::INIT;
        }
        let mut result = LifecycleCleanupResult::INIT;
        for index in 0..SLOT_TABLE_MAX_RANGES {
            let extent = self.slot_extents[index];
            if !extent_active(extent) {
                continue;
            }
            if extent.owner_kind != owner_kind || extent.owner_id != owner_id {
                continue;
            }
            if match_region && extent.region_id != region_id {
                continue;
            }
            result.released_ranges = result.released_ranges.wrapping_add(1);
            result.released_slots = result.released_slots.wrapping_add(extent.slot_count);
            if extent.state & extent_state_valid != 0 {
                result.released_valid_slots =
                    result.released_valid_slots.wrapping_add(extent.slot_count)
            }
            if extent.state & extent_state_dirty != 0 {
                result.released_dirty_slots =
                    result.released_dirty_slots.wrapping_add(extent.slot_count)
            }
            if extent.state & extent_state_error != 0 {
                result.released_error_slots =
                    result.released_error_slots.wrapping_add(extent.slot_count)
            }
            self.slot_extents[index] = SlotExtent::INIT;
        }
        if result.released_ranges != 0 {
            result.cleanup_count = 1;
            self.slot_generation = self.slot_generation.wrapping_add(1);
            self.slot_state.lifecycle_cleanups = self.slot_state.lifecycle_cleanups.wrapping_add(1);
            self.slot_state.lifecycle_released_ranges = self
                .slot_state
                .lifecycle_released_ranges
                .wrapping_add(result.released_ranges);
            self.slot_state.lifecycle_released_slots = self
                .slot_state
                .lifecycle_released_slots
                .wrapping_add(result.released_slots);
            self.slot_state.last_owner_kind = owner_kind;
            self.slot_state.last_owner_id = owner_id;
            self.slot_state.last_region_id = if match_region { region_id } else { 0 };
            self.refresh_slot_summary_counts();
        }
        result
    }
    fn refresh_slot_summary_counts(&mut self) {
        let mut result = SlotResult::INIT;
        self.fill_slot_counts(&mut result);
        self.slot_state.capacity_slots = result.capacity_slots;
        self.slot_state.reserved_slots = result.reserved_slots;
        self.slot_state.free_slots = result.free_slots;
        self.slot_state.valid_slots = result.valid_slots;
        self.slot_state.dirty_slots = result.dirty_slots;
        self.slot_state.error_slots = result.error_slots;
        self.slot_state.range_count = result.range_count;
        self.slot_state.max_ranges = result.max_ranges;
        self.slot_state.generation = self.slot_generation;
    }
    fn find_extent_by_reservation(&self, reservation_id: u32) -> Option<usize> {
        self.slot_extents
            .iter()
            .position(|extent| extent_active(*extent) && extent.reservation_id == reservation_id)
    }
}

fn path_bytes(path: &[u8; MAX_BACKING_PATH + 1]) -> &[u8] {
    &path[..path.iter().position(|v| *v == 0).unwrap_or(path.len())]
}
fn path_eq(a: &[u8; MAX_BACKING_PATH + 1], b: &[u8; MAX_BACKING_PATH + 1]) -> bool {
    path_bytes(a).eq_ignore_ascii_case(path_bytes(b))
}
fn extent_active(extent: SlotExtent) -> bool {
    extent.state & extent_state_reserved != 0 && extent.slot_count != 0
}
fn valid_slot_owner_kind(kind: u32) -> bool {
    matches!(
        kind,
        slot_owner_kind_diagnostic
            | slot_owner_kind_r4x_instance
            | slot_owner_kind_vm_region
            | slot_owner_kind_pager
    )
}
fn owner_matches(extent: SlotExtent, kind: u32, id: u32, region: u32) -> bool {
    if extent.owner_kind != kind || extent.owner_id != id {
        return false;
    }
    if extent.owner_kind == slot_owner_kind_vm_region {
        return extent.region_id != 0 && extent.region_id == region;
    }
    if extent.region_id != 0 && region != 0 {
        return extent.region_id == region;
    }
    extent.region_id == 0 || region == 0
}
fn slot_identity(result: &mut SlotResult, extent: SlotExtent) {
    result.reservation_id = extent.reservation_id;
    result.owner_kind = extent.owner_kind;
    result.owner_id = extent.owner_id;
    result.region_id = extent.region_id;
    result.first_slot = extent.start_slot;
    result.slot_count = extent.slot_count;
}
fn add_cleanup(result: &mut LifecycleCleanupResult, other: LifecycleCleanupResult) {
    result.cleanup_count = result.cleanup_count.wrapping_add(other.cleanup_count);
    result.released_ranges = result.released_ranges.wrapping_add(other.released_ranges);
    result.released_slots = result.released_slots.wrapping_add(other.released_slots);
    result.released_valid_slots = result
        .released_valid_slots
        .wrapping_add(other.released_valid_slots);
    result.released_dirty_slots = result
        .released_dirty_slots
        .wrapping_add(other.released_dirty_slots);
    result.released_error_slots = result
        .released_error_slots
        .wrapping_add(other.released_error_slots);
}
fn gate_counts(result: &mut PagerGateResult, slot: SlotResult) {
    result.capacity_slots = slot.capacity_slots;
    result.free_after_slots = slot.free_slots;
    result.reserved_after_slots = slot.reserved_slots;
    result.slot_generation = slot.generation;
}
fn map_slot_failure_to_pager_gate(slot: SlotResult, result: &mut PagerGateResult) {
    (result.status, result.blockers) = match slot.status {
        slot_status_backing_unavailable => (
            pager_gate_status_backing_unavailable,
            result.blockers | pager_gate_blocker_backing_not_ready,
        ),
        slot_status_insufficient_capacity => (
            pager_gate_status_insufficient_capacity,
            result.blockers | pager_gate_blocker_insufficient_capacity,
        ),
        slot_status_table_full => (
            pager_gate_status_table_full,
            result.blockers | pager_gate_blocker_table_full,
        ),
        _ => (
            pager_gate_status_invalid_request,
            result.blockers | pager_gate_blocker_invalid_request,
        ),
    };
}
fn make_page_io_result(input: PageIoInput) -> PageIoResult {
    let mut result = PageIoResult {
        operation: input.operation,
        region_id: input.region_id,
        reservation_id: input.reservation_id,
        owner_kind: input.owner_kind,
        owner_id: input.owner_id,
        region_offset: input.region_offset,
        committed_bytes: input.committed_bytes,
        resident_bytes: input.resident_bytes,
        slot_index: input.slot_index,
        page_count: input.page_count,
        expected_generation: input.expected_generation,
        io_status: input.io_status,
        io_bytes: input.io_bytes,
        ..PageIoResult::INIT
    };
    match input.operation {
        page_io_operation_page_out => result.flags |= page_io_flag_page_out,
        page_io_operation_page_in => result.flags |= page_io_flag_page_in,
        _ => {}
    }
    if input.flags & page_io_flag_eviction_request != 0 {
        result.flags |= page_io_flag_eviction_request
    }
    if input.flags & page_io_flag_retry_request != 0 {
        result.flags |= page_io_flag_retry_request
    }
    result
}
fn page_io_failure_retryable(status: u32) -> bool {
    matches!(
        status,
        page_io_status_backing_unavailable | page_io_status_io_failed | page_io_status_partial_io
    )
}

struct Storage(UnsafeCell<Engine>);
// SAFETY: every call is bounded and admitted by the existing VM owner. No
// Engine reference or extent lease escapes to file I/O, a wait or its caller.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Engine::INIT));

/// # Safety
/// Output owns one C token for the existing VM owner. The caller must consume
/// that exact CPU-bound token in release before any waiting operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_acquire(output: *mut super::owner::Token) {
    unsafe { output.write(super::owner::named(5).acquire()) };
}
/// # Safety
/// Input is the exact live token returned by acquire on its original CPU.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_release(input: *const super::owner::Token) {
    super::owner::named(5).release(unsafe { input.read() });
}

// The C facade supplies VM admission, including passive snapshots and owner
// cleanup. Native pager callers use the same owner, never another registry.
unsafe fn admitted<T>(operation: impl FnOnce(&mut Engine) -> T) -> T {
    operation(unsafe { &mut *STATE.0.get() })
}

impl Engine {
    fn record(&mut self, result: Result) {
        self.state.enabled = true;
        self.state.probes = self.state.probes.wrapping_add(1);
        if result.status == status_ready {
            self.state.ready = self.state.ready.wrapping_add(1);
        } else {
            self.state.failures = self.state.failures.wrapping_add(1);
        }
        self.state.last_status = result.status;
        self.state.last_flags = result.flags;
        self.state.last_blockers = result.blockers;
        self.state.last_requested_bytes = result.requested_bytes;
        self.state.last_available_bytes = result.available_bytes;
        self.state.last_file_size = result.file_size;
        self.state.last_cluster_bytes = result.cluster_bytes;
        self.state.last_first_cluster = result.first_cluster;
    }
    fn record_slot(&mut self, result: &mut SlotResult) {
        self.slot_state.enabled = true;
        self.slot_state.probes = self.slot_state.probes.wrapping_add(1);
        match result.status {
            slot_status_reserved => {
                self.slot_state.reserves = self.slot_state.reserves.wrapping_add(1)
            }
            slot_status_released => {
                self.slot_state.releases = self.slot_state.releases.wrapping_add(1)
            }
            slot_status_error_marked => {
                self.slot_state.error_marks = self.slot_state.error_marks.wrapping_add(1)
            }
            slot_status_recovered => {
                self.slot_state.recoveries = self.slot_state.recoveries.wrapping_add(1)
            }
            slot_status_ready => {}
            _ => self.slot_state.failures = self.slot_state.failures.wrapping_add(1),
        }
        result.total_probes = self.slot_state.probes;
        result.total_reserves = self.slot_state.reserves;
        result.total_releases = self.slot_state.releases;
        result.total_error_marks = self.slot_state.error_marks;
        result.total_recoveries = self.slot_state.recoveries;
        result.total_failures = self.slot_state.failures;
        self.slot_state.last_status = result.status;
        self.slot_state.last_operation = result.operation;
        self.slot_state.last_flags = result.flags;
        self.slot_state.last_blockers = result.blockers;
        self.slot_state.capacity_slots = result.capacity_slots;
        self.slot_state.reserved_slots = result.reserved_slots;
        self.slot_state.free_slots = result.free_slots;
        self.slot_state.valid_slots = result.valid_slots;
        self.slot_state.dirty_slots = result.dirty_slots;
        self.slot_state.error_slots = result.error_slots;
        self.slot_state.range_count = result.range_count;
        self.slot_state.max_ranges = result.max_ranges;
        self.slot_state.last_reservation_id = result.reservation_id;
        self.slot_state.last_owner_kind = result.owner_kind;
        self.slot_state.last_owner_id = result.owner_id;
        self.slot_state.last_region_id = result.region_id;
        self.slot_state.last_first_slot = result.first_slot;
        self.slot_state.last_slot_count = result.slot_count;
        self.slot_state.generation = self.slot_generation;
    }
    fn record_pager_gate(&mut self, result: &mut PagerGateResult) {
        self.pager_gate_state.enabled = true;
        self.pager_gate_state.probes = self.pager_gate_state.probes.wrapping_add(1);
        if result.status == pager_gate_status_ready {
            self.pager_gate_state.ready = self.pager_gate_state.ready.wrapping_add(1);
            if result.rollback_completed != 0 {
                self.pager_gate_state.rollbacks = self.pager_gate_state.rollbacks.wrapping_add(1);
            }
        } else {
            self.pager_gate_state.failures = self.pager_gate_state.failures.wrapping_add(1);
        }
        result.total_probes = self.pager_gate_state.probes;
        result.total_ready = self.pager_gate_state.ready;
        result.total_rollbacks = self.pager_gate_state.rollbacks;
        result.total_failures = self.pager_gate_state.failures;
        self.pager_gate_state.last_status = result.status;
        self.pager_gate_state.last_flags = result.flags;
        self.pager_gate_state.last_blockers = result.blockers;
        self.pager_gate_state.last_region_id = result.region_id;
        self.pager_gate_state.last_owner_id = result.owner_id;
        self.pager_gate_state.requested_bytes = result.requested_bytes;
        self.pager_gate_state.committed_bytes = result.committed_bytes;
        self.pager_gate_state.resident_bytes = result.resident_bytes;
        self.pager_gate_state.nonresident_bytes = result.nonresident_bytes;
        self.pager_gate_state.requested_slots = result.requested_slots;
        self.pager_gate_state.prepared_slots = result.prepared_slots;
        self.pager_gate_state.capacity_slots = result.capacity_slots;
        self.pager_gate_state.free_before_slots = result.free_before_slots;
        self.pager_gate_state.free_after_slots = result.free_after_slots;
        self.pager_gate_state.reserved_before_slots = result.reserved_before_slots;
        self.pager_gate_state.reserved_after_slots = result.reserved_after_slots;
        self.pager_gate_state.last_reservation_id = result.slot_reservation_id;
        self.pager_gate_state.rollback_completed = result.rollback_completed;
        self.pager_gate_state.commit_gate_enabled = result.commit_gate_enabled;
        self.pager_gate_state.fault_gate_enabled = result.fault_gate_enabled;
        self.pager_gate_state.pager_enabled = result.pager_enabled;
        self.pager_gate_state.eviction_enabled = result.eviction_enabled;
        self.pager_gate_state.page_in_enabled = result.page_in_enabled;
        self.pager_gate_state.page_out_enabled = result.page_out_enabled;
        self.pager_gate_state.slot_generation = result.slot_generation;
        self.pager_gate_state.fault_count = result.fault_count;
        self.pager_gate_state.failed_faults = result.failed_faults;
    }
    fn record_page_io(&mut self, result: &mut PageIoResult) {
        self.page_io_state.enabled = true;
        if result.flags & page_io_flag_retry_request != 0 {
            self.page_io_state.retry_attempts = self.page_io_state.retry_attempts.wrapping_add(1);
        }
        if result.status == page_io_status_ready {
            self.page_io_state.prepares = self.page_io_state.prepares.wrapping_add(1);
        } else if result.status == page_io_status_page_out_ok {
            self.page_io_state.page_outs = self.page_io_state.page_outs.wrapping_add(1);
        } else if result.status == page_io_status_page_in_ok {
            self.page_io_state.page_ins = self.page_io_state.page_ins.wrapping_add(1);
        } else {
            self.page_io_state.failures = self.page_io_state.failures.wrapping_add(1);
            self.record_page_io_failure_policy(result);
        }
        result.total_prepares = self.page_io_state.prepares;
        result.total_page_outs = self.page_io_state.page_outs;
        result.total_page_ins = self.page_io_state.page_ins;
        result.total_failures = self.page_io_state.failures;
        result.total_retry_attempts = self.page_io_state.retry_attempts;
        result.total_retryable_failures = self.page_io_state.retryable_failures;
        result.total_permanent_failures = self.page_io_state.permanent_failures;
        result.total_retry_limit_hits = self.page_io_state.retry_limit_hits;
        result.total_failed_page_outs = self.page_io_state.failed_page_outs;
        result.total_failed_page_ins = self.page_io_state.failed_page_ins;
        result.total_data_preserved_pages = self.page_io_state.data_preserved_pages;
        result.total_data_lost_pages = self.page_io_state.data_lost_pages;
        self.page_io_state.last_status = result.status;
        self.page_io_state.last_operation = result.operation;
        self.page_io_state.last_flags = result.flags;
        self.page_io_state.last_blockers = result.blockers;
        self.page_io_state.last_region_id = result.region_id;
        self.page_io_state.last_reservation_id = result.reservation_id;
        self.page_io_state.last_owner_kind = result.owner_kind;
        self.page_io_state.last_owner_id = result.owner_id;
        self.page_io_state.region_offset = result.region_offset;
        self.page_io_state.page_count = result.page_count;
        self.page_io_state.transfer_bytes = result.transfer_bytes;
        self.page_io_state.expected_generation = result.expected_generation;
        self.page_io_state.backing_slot = result.backing_slot;
        self.page_io_state.backing_offset = result.backing_offset;
        self.page_io_state.io_bytes = result.io_bytes;
        self.page_io_state.io_status = result.io_status;
        self.page_io_state.capacity_slots = result.capacity_slots;
        self.page_io_state.reserved_slots = result.reserved_slots;
        self.page_io_state.valid_slots = result.valid_slots;
        self.page_io_state.dirty_slots = result.dirty_slots;
        self.page_io_state.error_slots = result.error_slots;
        self.page_io_state.pager_enabled = result.pager_enabled;
        self.page_io_state.eviction_enabled = result.eviction_enabled;
        self.page_io_state.page_in_enabled = result.page_in_enabled;
        self.page_io_state.page_out_enabled = result.page_out_enabled;
        self.page_io_state.retry_limit = result.retry_limit;
        self.page_io_state.backoff_ticks = result.backoff_ticks;
        self.page_io_state.slot_generation = result.slot_generation;
    }
}

/// # Safety
/// VM owner admits this bounded call; input/output are exact disjoint C
/// values. A nonnull input path is a readable NUL string until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_probe(input: *const Input, output: *mut Result) {
    let value = unsafe { input.read() };
    let result = unsafe { admitted(|engine| engine.probe(value)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits this bounded call; input/output are exact disjoint C
/// values. A nonnull input path is a readable NUL string until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_slot_probe(
    input: *const SlotInput,
    output: *mut SlotResult,
) {
    let value = unsafe { input.read() };
    let result = unsafe { admitted(|engine| engine.slot_probe(value)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits this bounded call; input/output are exact disjoint C
/// values. A nonnull input path is a readable NUL string until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_pager_gate_probe(
    input: *const PagerGateInput,
    output: *mut PagerGateResult,
) {
    let value = unsafe { input.read() };
    let result = unsafe { admitted(|engine| engine.pager_gate_probe(value)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits this bounded call; input/output are exact disjoint C
/// values. A nonnull input path is a readable NUL string until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_page_io_prepare(
    input: *const PageIoInput,
    output: *mut PageIoResult,
) {
    let value = unsafe { input.read() };
    let result = unsafe { admitted(|engine| engine.page_io_prepare(value)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits this bounded call; input/output are exact disjoint C
/// values. A nonnull input path is a readable NUL string until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_page_io_complete(
    input: *const PageIoInput,
    output: *mut PageIoResult,
) {
    let value = unsafe { input.read() };
    let result = unsafe { admitted(|engine| engine.page_io_complete(value)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits the passive snapshot; output is one writable C value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_summary(output: *mut Summary) {
    let result = unsafe { admitted(|engine| engine.state) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits the passive snapshot; output is one writable C value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_slot_summary(output: *mut SlotSummary) {
    let result = unsafe { admitted(|engine| engine.slot_state) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits the passive snapshot; output is one writable C value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_pager_gate_summary(output: *mut PagerGateSummary) {
    let result = unsafe { admitted(|engine| engine.pager_gate_state) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner admits the passive snapshot; output is one writable C value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_page_io_summary(output: *mut PageIoSummary) {
    let result = unsafe { admitted(|engine| engine.page_io_state) };
    unsafe { output.write(result) };
}

/// # Safety
/// Existing VM owner admits exact owner cleanup; output is disjoint storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_release_r4x_owner(
    owner_id: u32,
    output: *mut LifecycleCleanupResult,
) {
    let result = unsafe { admitted(|engine| engine.release_r4x_owner(owner_id)) };
    unsafe { output.write(result) };
}

/// # Safety
/// Existing VM owner admits exact owner cleanup; output is disjoint storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_release_vm_region(
    owner_id: u32,
    region_id: u32,
    output: *mut LifecycleCleanupResult,
) {
    let result = unsafe { admitted(|engine| engine.release_vm_region(owner_id, region_id)) };
    unsafe { output.write(result) };
}

/// # Safety
/// VM owner holds the exact backing identity. The returned resident path
/// remains borrowed under that owner or a live reservation of that identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_active_path() -> *const u8 {
    unsafe {
        admitted(|engine| {
            if engine.active_backing_path_valid {
                engine.active_backing_path.as_ptr()
            } else {
                ptr::null()
            }
        })
    }
}
/// # Safety
/// VM owner admits the snapshot; output is one disjoint writable C value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_backing_active_result(output: *mut Result) -> bool {
    let value = unsafe { admitted(|engine| engine.active_backing_result()) };
    if let Some(value) = value {
        unsafe { output.write(value) };
        true
    } else {
        false
    }
}
