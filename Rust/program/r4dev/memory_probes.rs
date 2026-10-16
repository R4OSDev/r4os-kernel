//! Original explicit backing/pager probes. Every VM metadata admission ends
//! before filesystem I/O; exact native writeback tokens retain caller/region
//! lifetime across the original transfer and terminal completion receipt.
use crate::contract as c;
use crate::{
    contract::*,
    memory::{
        backing_store as store, backing_types as b, owner, vm_pager_api as vm, vm_pager_types as v,
        vm_query, vm_ranges,
    },
    program::system::{directory, io},
};
use core::{mem, mem::MaybeUninit};
unsafe fn backing(path: *const u8, bytes: u64, flags: u32) -> b::Result {
    unsafe {
        let mut f = FileInfo::default();
        let rc = directory::r4os_rust_sys_file_info(path, &mut f);
        let mut d = DriveInfo::default();
        let mut cluster = 0;
        if f.drive.is_ascii_uppercase()
            && directory::r4os_rust_sys_drive_info(u32::from(f.drive - b'A'), &mut d) > 0
            && d.cluster_bytes != 0
        {
            cluster = d.cluster_bytes
        }
        let input = b::Input {
            requested_bytes: bytes,
            flags,
            path,
            file_exists: rc > 0 && f.exists != 0,
            is_dir: f.is_dir != 0,
            // This is the original shared filesystem-capability flag, even
            // when the supported backing file actually resides on NTFS.
            fat32: rc >= 0,
            file_size: f.size,
            cluster_bytes: cluster,
            first_cluster: f.first_cluster,
        };
        let mut out = MaybeUninit::uninit();
        {
            let _owner = owner::Guard::acquire(5);
            store::r4os_rust_backing_probe(&input, out.as_mut_ptr());
        }
        out.assume_init()
    }
}
unsafe fn backing_summary() -> b::Summary {
    let _owner = owner::Guard::acquire(5);
    unsafe { super::snapshot(store::r4os_rust_backing_summary) }
}
unsafe fn slot_summary() -> b::SlotSummary {
    let _owner = owner::Guard::acquire(5);
    unsafe { super::snapshot(store::r4os_rust_backing_slot_summary) }
}
fn clamp64(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}
unsafe fn range(id: u32) -> Option<vm_ranges::Range> {
    let mut out = MaybeUninit::uninit();
    if unsafe { vm_query::r4os_rust_vm_range_info(id, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}

/// # Safety
/// Caller retains readable resident path and exclusive canonical output for
/// the original filesystem observation and bounded backing metadata probe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_backing_store_probe(
    path: *const u8,
    bytes: u64,
    flags: u32,
    out: *mut ProgramMemoryBackingStoreProbe,
) -> i32 {
    unsafe {
        let result = backing(path, bytes, flags);
        let backing_summary = backing_summary();
        out.write(ProgramMemoryBackingStoreProbe {
            version: c::memory_backing_store_probe_version,
            size: mem::size_of::<ProgramMemoryBackingStoreProbe>() as u32,
            status: result.status,
            flags: result.flags,
            blockers: result.blockers,
            pager_enabled: result.pager_enabled,
            anonymous_paging_enabled: result.anonymous_paging_enabled,
            cluster_bytes: result.cluster_bytes,
            requested_bytes: result.requested_bytes,
            available_bytes: result.available_bytes,
            file_size: result.file_size,
            first_cluster: result.first_cluster,
            reserved0: 0,
            total_probes: backing_summary.probes,
            total_ready: backing_summary.ready,
            total_failures: backing_summary.failures,
            reserved1: [0; 4],
        });
        i32::from(result.status == c::memory_backing_store_status_ready)
    }
}
/// # Safety
/// Caller retains the canonical operation/path/output and exact reservation
/// identity to completion. The existing native owner validates all tuples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_backing_store_slot_probe(
    path: *const u8,
    bytes: u64,
    operation: u32,
    requested_slots: u64,
    reservation_id: u32,
    owner_kind: u32,
    owner_id: u32,
    region_id: u32,
    flags: u32,
    out: *mut ProgramMemoryBackingStoreSlotProbe,
) -> i32 {
    unsafe {
        let backing = backing(path, bytes, 0);
        let input = b::SlotInput {
            operation,
            requested_slots,
            reservation_id,
            owner_kind,
            owner_id,
            region_id,
            flags,
            backing,
        };
        let mut value = MaybeUninit::uninit();
        {
            let _owner = owner::Guard::acquire(5);
            store::r4os_rust_backing_slot_probe(&input, value.as_mut_ptr());
        }
        let result = value.assume_init();
        out.write(ProgramMemoryBackingStoreSlotProbe {
            version: c::memory_backing_store_slot_probe_version,
            size: mem::size_of::<ProgramMemoryBackingStoreSlotProbe>() as u32,
            status: result.status,
            operation: result.operation,
            flags: result.flags,
            blockers: result.blockers,
            slot_bytes: result.slot_bytes,
            max_ranges: result.max_ranges,
            pager_enabled: result.pager_enabled,
            eviction_enabled: result.eviction_enabled,
            page_in_enabled: result.page_in_enabled,
            page_out_enabled: result.page_out_enabled,
            reservation_id: result.reservation_id,
            owner_kind: result.owner_kind,
            owner_id: result.owner_id,
            region_id: result.region_id,
            range_count: result.range_count,
            reserved0: 0,
            capacity_slots: result.capacity_slots,
            requested_slots: result.requested_slots,
            reserved_slots: result.reserved_slots,
            free_slots: result.free_slots,
            valid_slots: result.valid_slots,
            dirty_slots: result.dirty_slots,
            error_slots: result.error_slots,
            first_slot: result.first_slot,
            slot_count: result.slot_count,
            generation: result.generation,
            total_probes: result.total_probes,
            total_reserves: result.total_reserves,
            total_releases: result.total_releases,
            total_error_marks: result.total_error_marks,
            total_recoveries: result.total_recoveries,
            total_failures: result.total_failures,
            lifecycle_cleanup_count: slot_summary().lifecycle_cleanups,
            lifecycle_released_ranges: slot_summary().lifecycle_released_ranges,
            lifecycle_released_slots: slot_summary().lifecycle_released_slots,
            reserved1: [0; 1],
        });
        i32::from(matches!(
            result.status,
            c::memory_backing_store_slot_status_ready
                | c::memory_backing_store_slot_status_reserved
                | c::memory_backing_store_slot_status_released
                | c::memory_backing_store_slot_status_error_marked
                | c::memory_backing_store_slot_status_recovered
        ))
    }
}
/// # Safety
/// Resident path/canonical output retain caller ownership across the existing
/// filesystem calls. VM region metadata is copied, never borrowed over I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_pager_gate_probe(
    path: *const u8,
    bytes: u64,
    region_id: u32,
    requested_bytes: u64,
    flags: u32,
    out: *mut ProgramMemoryPagerGateProbe,
) -> i32 {
    unsafe {
        let backing = backing(path, bytes, 0);
        let region = range(region_id);
        let mut input = b::PagerGateInput {
            requested_bytes,
            region_id,
            flags,
            backing,
            ..Default::default()
        };
        if let Some(r) = region {
            input.vm_region_exists = true;
            input.vm_region_is_r4x = r.window == vm_ranges::R4X_VM_WINDOW;
            input.owner_id = clamp64(r.owner_id);
            input.committed_bytes = r.committed_bytes;
            input.resident_bytes = r.resident_bytes;
            input.fault_count = r.fault_count;
            input.failed_faults = r.failed_faults;
        }
        let mut value = MaybeUninit::uninit();
        {
            let _owner = owner::Guard::acquire(5);
            store::r4os_rust_backing_pager_gate_probe(&input, value.as_mut_ptr());
        }
        let result = value.assume_init();
        out.write(ProgramMemoryPagerGateProbe {
            version: c::memory_pager_gate_probe_version,
            size: mem::size_of::<ProgramMemoryPagerGateProbe>() as u32,
            status: result.status,
            flags: result.flags,
            blockers: result.blockers,
            region_id: result.region_id,
            owner_id: result.owner_id,
            slot_bytes: result.slot_bytes,
            requested_bytes: result.requested_bytes,
            committed_bytes: result.committed_bytes,
            resident_bytes: result.resident_bytes,
            nonresident_bytes: result.nonresident_bytes,
            requested_slots: result.requested_slots,
            prepared_slots: result.prepared_slots,
            capacity_slots: result.capacity_slots,
            free_before_slots: result.free_before_slots,
            free_after_slots: result.free_after_slots,
            reserved_before_slots: result.reserved_before_slots,
            reserved_after_slots: result.reserved_after_slots,
            slot_reservation_id: result.slot_reservation_id,
            rollback_completed: result.rollback_completed,
            commit_gate_enabled: result.commit_gate_enabled,
            fault_gate_enabled: result.fault_gate_enabled,
            pager_enabled: result.pager_enabled,
            eviction_enabled: result.eviction_enabled,
            page_in_enabled: result.page_in_enabled,
            page_out_enabled: result.page_out_enabled,
            reserved0: [0; 2],
            slot_generation: result.slot_generation,
            fault_count: result.fault_count,
            failed_faults: result.failed_faults,
            total_probes: result.total_probes,
            total_ready: result.total_ready,
            total_rollbacks: result.total_rollbacks,
            total_failures: result.total_failures,
            reserved1: [0; 4],
        });
        i32::from(result.status == c::memory_pager_gate_status_ready)
    }
}
struct Writeback(Option<v::PageWriteback>);
impl Drop for Writeback {
    fn drop(&mut self) {
        if let Some(token) = self.0.as_mut() {
            unsafe { vm::r4os_rust_vm_end_writeback(token) }
        }
    }
}
fn policy_failure(result: b::PageIoResult) {
    if result.owner_kind != c::memory_backing_store_slot_owner_kind_vm_region
        || matches!(
            result.status,
            c::memory_page_io_status_ready
                | c::memory_page_io_status_page_out_ok
                | c::memory_page_io_status_page_in_ok
        )
    {
        return;
    }
    let input = v::PageStateInput {
        region_id: result.region_id,
        region_offset: result.region_offset,
        page_count: result.page_count,
        ..Default::default()
    };
    unsafe {
        vm::r4os_rust_vm_policy_failure(
            &input,
            result.operation == c::memory_page_io_operation_page_out,
        )
    }
}
unsafe fn fill_page_io(out: *mut ProgramMemoryPageIoProbe, result: b::PageIoResult) {
    unsafe {
        out.write(ProgramMemoryPageIoProbe {
            version: c::memory_page_io_probe_version,
            size: mem::size_of::<ProgramMemoryPageIoProbe>() as u32,
            status: result.status,
            operation: result.operation,
            flags: result.flags,
            blockers: result.blockers,
            region_id: result.region_id,
            reservation_id: result.reservation_id,
            owner_kind: result.owner_kind,
            owner_id: result.owner_id,
            slot_bytes: result.slot_bytes,
            io_bytes: result.io_bytes,
            io_status: result.io_status,
            page_count_lo: clamp64(result.page_count),
            region_offset: result.region_offset,
            committed_bytes: result.committed_bytes,
            resident_bytes: result.resident_bytes,
            slot_index: result.slot_index,
            page_count: result.page_count,
            transfer_bytes: result.transfer_bytes,
            expected_generation: result.expected_generation,
            backing_slot: result.backing_slot,
            backing_offset: result.backing_offset,
            capacity_slots: result.capacity_slots,
            reserved_slots: result.reserved_slots,
            valid_slots: result.valid_slots,
            dirty_slots: result.dirty_slots,
            error_slots: result.error_slots,
            pager_enabled: result.pager_enabled,
            eviction_enabled: result.eviction_enabled,
            page_in_enabled: result.page_in_enabled,
            page_out_enabled: result.page_out_enabled,
            slot_generation: result.slot_generation,
            total_prepares: result.total_prepares,
            total_page_outs: result.total_page_outs,
            total_page_ins: result.total_page_ins,
            total_failures: result.total_failures,
            retry_limit: result.retry_limit,
            backoff_ticks: result.backoff_ticks,
            reserved1: [0; 2],
            total_retry_attempts: result.total_retry_attempts,
            total_retryable_failures: result.total_retryable_failures,
            total_permanent_failures: result.total_permanent_failures,
            total_retry_limit_hits: result.total_retry_limit_hits,
            total_failed_page_outs: result.total_failed_page_outs,
            total_failed_page_ins: result.total_failed_page_ins,
            total_data_preserved_pages: result.total_data_preserved_pages,
            total_data_lost_pages: result.total_data_lost_pages,
        })
    }
}
/// # Safety
/// Exact resident zero-terminated path, disjoint canonical output and the
/// original readable/writable page-count buffer remain owned through I/O.
/// Region/reservation/generation inputs retain the existing native checks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_page_io_probe(
    path: *const u8,
    bytes: u64,
    operation: u32,
    region_id: u32,
    region_offset: u64,
    reservation_id: u32,
    slot_index: u64,
    page_count: u64,
    owner_kind: u32,
    owner_id: u32,
    expected_generation: u64,
    page: *mut u8,
    flags: u32,
    out: *mut ProgramMemoryPageIoProbe,
) -> i32 {
    unsafe {
        let backing = backing(path, bytes, 0);
        let region = range(region_id);
        let mut input = b::PageIoInput {
            operation,
            region_id,
            region_offset,
            reservation_id,
            slot_index,
            page_count,
            owner_kind,
            owner_id,
            expected_generation,
            flags,
            commit_covered: vm::r4os_rust_vm_commit_covers(region_id, region_offset, page_count),
            backing,
            ..Default::default()
        };
        if let Some(r) = region {
            input.vm_region_exists = true;
            input.vm_region_is_r4x = r.window == vm_ranges::R4X_VM_WINDOW;
            input.committed_bytes = r.committed_bytes;
            input.resident_bytes = r.resident_bytes;
        }
        let mut value = MaybeUninit::uninit();
        {
            let _owner = owner::Guard::acquire(5);
            store::r4os_rust_backing_page_io_prepare(&input, value.as_mut_ptr());
        }
        let prepared = value.assume_init();
        if prepared.status != c::memory_page_io_status_ready {
            policy_failure(prepared);
            fill_page_io(out, prepared);
            return 0;
        }
        let mut writeback = Writeback(None);
        if operation == c::memory_page_io_operation_page_out
            && owner_kind == c::memory_backing_store_slot_owner_kind_vm_region
        {
            let mut token = MaybeUninit::uninit();
            if vm::r4os_rust_vm_begin_writeback(
                region_id,
                region_offset,
                page_count,
                page,
                token.as_mut_ptr(),
            ) != 0
            {
                let mut failed = prepared;
                failed.status = c::memory_page_io_status_invalid_request;
                failed.blockers |= b::page_io_blocker_invalid_request;
                fill_page_io(out, failed);
                return 0;
            }
            writeback.0 = Some(token.assume_init());
        }
        let len = prepared
            .transfer_bytes
            .try_into()
            .expect("original page transfer fits u32");
        let io_status = match operation {
            c::memory_page_io_operation_page_out => {
                io::r4os_rust_sys_file_write_at(path, prepared.backing_offset, page, len)
            }
            c::memory_page_io_operation_page_in => {
                io::r4os_rust_sys_file_read_at(path, prepared.backing_offset, page, len)
            }
            _ => 0,
        };
        let mut complete = input;
        complete.io_status = io_status;
        complete.io_bytes = if io_status > 0 { io_status as u32 } else { 0 };
        let mut value = MaybeUninit::uninit();
        {
            let _owner = owner::Guard::acquire(5);
            store::r4os_rust_backing_page_io_complete(&complete, value.as_mut_ptr());
        }
        let mut completed = value.assume_init();
        if !matches!(
            completed.status,
            c::memory_page_io_status_page_out_ok | c::memory_page_io_status_page_in_ok
        ) {
            policy_failure(completed)
        }
        if completed.status == c::memory_page_io_status_page_out_ok
            && completed.owner_kind == c::memory_backing_store_slot_owner_kind_vm_region
        {
            let binding = v::PageStateInput {
                region_id: completed.region_id,
                region_offset: completed.region_offset,
                page_count: completed.page_count,
                slot_reservation_id: completed.reservation_id,
                slot_index: completed.slot_index,
                slot_generation: completed.slot_generation,
                ..Default::default()
            };
            let mut count = 0;
            if vm::r4os_rust_vm_complete_writeback(
                writeback.0.as_mut().expect("original admitted writeback"),
                &binding,
                &mut count,
            ) != 0
            {
                completed.status = c::memory_page_io_status_invalid_request;
                completed.blockers |= b::page_io_blocker_invalid_request;
                policy_failure(completed)
            }
        }
        fill_page_io(out, completed);
        i32::from(matches!(
            completed.status,
            c::memory_page_io_status_page_out_ok | c::memory_page_io_status_page_in_ok
        ))
    }
}
/// # Safety
/// Exclusive canonical result and exact region/slot/generation operation input
/// retain the original owner contract for this explicitly requested probe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_vm_page_state_probe(
    region_id: u32,
    region_offset: u64,
    page_count: u64,
    operation: u32,
    slot_reservation_id: u32,
    slot_index: u64,
    slot_generation: u64,
    flags: u32,
    out: *mut ProgramMemoryVmPageStateProbe,
) -> i32 {
    unsafe {
        let input = v::PageStateInput {
            operation,
            region_id,
            region_offset,
            page_count,
            slot_reservation_id,
            slot_index,
            slot_generation,
            flags,
        };
        let mut value = MaybeUninit::uninit();
        vm::r4os_rust_vm_page_state_probe(&input, value.as_mut_ptr());
        let result = value.assume_init();
        out.write(ProgramMemoryVmPageStateProbe {
            version: c::memory_vm_page_state_probe_version,
            size: mem::size_of::<ProgramMemoryVmPageStateProbe>() as u32,
            status: result.status,
            operation: result.operation,
            flags: result.flags,
            blockers: result.blockers,
            region_id: result.region_id,
            page_size: result.page_size,
            max_spans: result.max_spans,
            span_count: result.span_count,
            slot_reservation_id: result.slot_reservation_id,
            page_count_lo: result.page_count_lo,
            region_offset: result.region_offset,
            page_count: result.page_count,
            committed_pages: result.committed_pages,
            resident_pages: result.resident_pages,
            nonresident_pages: result.nonresident_pages,
            dirty_pages: result.dirty_pages,
            clean_pages: result.clean_pages,
            pinned_pages: result.pinned_pages,
            busy_pages: result.busy_pages,
            error_pages: result.error_pages,
            slot_bound_pages: result.slot_bound_pages,
            slot_index: result.slot_index,
            slot_generation: result.slot_generation,
            total_transitions: result.total_transitions,
            dirty_marks: result.dirty_marks,
            clean_marks: result.clean_marks,
            slot_binds: result.slot_binds,
            slot_clears: result.slot_clears,
            pinned_marks: result.pinned_marks,
            pinned_clears: result.pinned_clears,
            busy_marks: result.busy_marks,
            busy_clears: result.busy_clears,
            error_marks: result.error_marks,
            error_clears: result.error_clears,
            table_full_failures: result.table_full_failures,
            cleanup_pages: result.cleanup_pages,
            reserved1: [0; 4],
        });
        i32::from(result.status == c::memory_vm_page_state_status_ready)
    }
}
