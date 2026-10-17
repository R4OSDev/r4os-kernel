//! Passive memory observations, explicit reclaim and owned heap checks.
//! Owner snapshots are copied; block names never escape a released catalog.
use super::snapshot;
use crate::{
    contract as c,
    fs::page_cache,
    memory::{blocks, layout, phys, reclaim, vm_pager_api, vm_query},
    program::context,
};
use core::{mem, mem::MaybeUninit};
/// # Safety
/// Caller retains a resident exclusive v1 header and its advertised output
/// capacity. The exact task unwind token survives every wait and publication.
pub unsafe extern "C" fn kernel_heap_check(out: *mut c::ProgramKernelHeapCheck) -> i32 {
    use crate::memory::heap_check;
    if out.is_null()
        || unsafe { (*out).version } != c::kernel_heap_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelHeapCheck>() as u32
    {
        return -1;
    }
    let Some(admission) = heap_check::Admission::acquire(true) else {
        unsafe {
            out.write(c::ProgramKernelHeapCheck::default());
        }
        return 0;
    };
    let report = heap_check::run(
        &admission,
        c::kernel_heap_check_case_rust_allocator,
        c::kernel_heap_check_case_resident_commit,
        crate::program::process::should_close,
    );
    unsafe {
        out.write(report);
    }
    i32::from(report.status != c::kernel_heap_check_status_unavailable)
}

/// # Safety
/// Caller owns one resident canonical memory-summary output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_summary(out: *mut c::ProgramMemorySummary) -> i32 {
    unsafe {
        let b = snapshot(blocks::r4os_rust_blocks_summary);
        let v = snapshot(vm_query::r4os_rust_vm_stats);
        out.write(c::ProgramMemorySummary {
            total_slots_used: b.total_slots_used,
            active_blocks: b.active_blocks,
            released_blocks: b.released_blocks,
            error_blocks: b.error_blocks,
            physical_bytes: b.physical_bytes,
            virtual_bytes: b.virtual_bytes,
            reserved_bytes: b.reserved_bytes,
            committed_bytes: b.committed_bytes,
            free_physical_bytes: b.free_physical_bytes,
            largest_free_phys_base: b.largest_free_phys_base,
            largest_free_phys_len: b.largest_free_phys_len,
            largest_free_virtual_base: v.largest_free_virtual_base,
            largest_free_virtual_len: v.largest_free_virtual_len,
            app_system_reserve_frames: v.app_system_reserve_frames,
            app_available_frames: v.app_available_frames,
            by_kind: b.by_kind,
            by_owner: b.by_owner,
            by_status: b.by_status,
            overflow: u32::from(b.overflow || v.overflow),
            reserved0: 0,
        });
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4dev_memory_block_count() -> u32 {
    let b = unsafe { snapshot(blocks::r4os_rust_blocks_summary) };
    b.active_blocks.min(u64::from(u32::MAX)) as u32
}
/// # Safety
/// Caller owns one resident canonical block record to return. Catalog text is
/// copied while its exact physical-memory owner still retains the block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_block(
    index: u32,
    out: *mut c::ProgramMemoryBlockInfo,
) -> i32 {
    let Some(b) = blocks::diagnostic_active(index, true) else {
        unsafe { out.write(Default::default()) };
        return 0;
    };
    let e = b.info;
    unsafe {
        out.write(c::ProgramMemoryBlockInfo {
            id: e.id,
            kind: e.kind,
            owner: e.owner,
            status: e.status,
            reserved0: 0,
            owner_id: e.owner_id,
            phys_base: e.phys_base,
            phys_len: e.phys_len,
            virt_base: e.virt_base,
            virt_len: e.virt_len,
            reserved_bytes: e.reserved_bytes,
            committed_bytes: e.committed_bytes,
            name: b.name,
        })
    };
    1
}
fn pressure(total: u64, available: u64, headroom: u64) -> u32 {
    if total == 0 {
        return c::memory_pressure_level_unknown;
    }
    let available = available.min(headroom);
    let critical = (8 * 1024 * 1024).max(total / 128);
    let warning = (32 * 1024 * 1024).max(total / 64);
    let watch = (128 * 1024 * 1024).max(total / 16);
    if available == 0 || available <= critical {
        c::memory_pressure_level_critical
    } else if available <= warning {
        c::memory_pressure_level_warning
    } else if available <= watch {
        c::memory_pressure_level_watch
    } else {
        c::memory_pressure_level_normal
    }
}
/// # Safety
/// Exclusive resident canonical pressure output. This performs no reclaim.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_pressure_snapshot(
    out: *mut c::ProgramMemoryPressureSnapshot,
) -> i32 {
    unsafe {
        let p = snapshot(phys::r4os_rust_phys_stats);
        let b = snapshot(blocks::r4os_rust_blocks_summary);
        let v = snapshot(vm_query::r4os_rust_vm_stats);
        let cache = snapshot(page_cache::r4os_rust_pagecache_summary);
        let total = p.total_frames * layout::PAGE_SIZE;
        let free = p.free_frames * layout::PAGE_SIZE;
        let used = p.used_frames * layout::PAGE_SIZE;
        let vm_reclaim = vm_pager_api::r4os_rust_vm_evictable_bytes(false);
        let reclaimable = cache.pmm_reclaimable_bytes.saturating_add(vm_reclaim);
        let dirty = cache
            .pmm_dirty_bytes
            .saturating_add(vm_pager_api::r4os_rust_vm_evictable_bytes(true));
        let reserve = v.app_system_reserve_frames * layout::PAGE_SIZE;
        let available = v.app_available_frames * layout::PAGE_SIZE;
        let nonresident = v.committed_bytes.saturating_sub(v.resident_bytes);
        let budget = v.resident_bytes.saturating_add(available);
        let headroom = budget.saturating_sub(v.committed_bytes);
        let mut flags = c::memory_pressure_flag_no_pagefile
            | c::memory_pressure_flag_no_swap
            | c::memory_pressure_flag_commit_limited
            | c::memory_pressure_flag_demand_commit
            | c::memory_pressure_flag_profile_limits;
        if reclaimable > 0 {
            if cache.pmm_reclaimable_bytes > 0 {
                flags |= c::memory_pressure_flag_fs_cache_reclaim
            }
            if vm_reclaim > 0 {
                flags |= c::memory_pressure_flag_vm_page_reclaim
            }
        } else {
            flags |= c::memory_pressure_flag_no_reclaim
        }
        out.write(c::ProgramMemoryPressureSnapshot {
            version: c::memory_pressure_snapshot_version,
            size: mem::size_of::<c::ProgramMemoryPressureSnapshot>() as u32,
            flags,
            pressure_level: pressure(total, available, headroom),
            oom_policy_flags: c::memory_pressure_oom_alloc_returns_null
                | c::memory_pressure_oom_vm_returns_error
                | c::memory_pressure_oom_fault_escalates
                | c::memory_pressure_oom_no_overcommit,
            total_physical_bytes: total,
            free_physical_bytes: free,
            used_physical_bytes: used,
            largest_free_physical_bytes: b.largest_free_phys_len,
            app_system_reserve_bytes: reserve,
            app_available_bytes: available,
            virtual_reserved_bytes: v.reserved_bytes,
            virtual_committed_bytes: v.committed_bytes,
            virtual_resident_bytes: v.resident_bytes,
            largest_free_virtual_bytes: v.largest_free_virtual_len,
            committed_nonresident_bytes: nonresident,
            commit_budget_bytes: budget,
            commit_headroom_bytes: headroom,
            reclaimable_bytes: reclaimable,
            dirty_bytes: dirty,
            non_reclaimable_bytes: used.saturating_sub(reclaimable),
            fault_count: v.fault_count,
            failed_faults: v.failed_faults,
            ..Default::default()
        });
    }
    1
}
/// # Safety
/// Exclusive resident canonical reclaim result. The active request retains the
/// calling R4X owner's exact identity and existing cooperative wait lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_memory_reclaim_probe(
    frames: u32,
    out: *mut c::ProgramMemoryReclaimProbe,
) -> i32 {
    unsafe {
        let mut result = MaybeUninit::uninit();
        reclaim::r4os_rust_reclaim_frames(
            c::memory_reclaim_reason_diagnostic,
            frames.clamp(1, 64),
            context::r4os_rust_program_context_memory_owner(),
            true,
            result.as_mut_ptr(),
        );
        let r = result.assume_init();
        let s = snapshot(reclaim::r4os_rust_reclaim_summary);
        out.write(c::ProgramMemoryReclaimProbe {
            version: c::memory_reclaim_probe_version,
            size: mem::size_of::<c::ProgramMemoryReclaimProbe>() as u32,
            reason: r.reason,
            requested_frames: r.requested_frames,
            returned_frames: r.returned_frames,
            returned_bytes: r.returned_bytes,
            dirty_drains: r.dirty_drains,
            failed_drains: r.failed_drains,
            before_free_frames: r.before_free_frames,
            after_free_frames: r.after_free_frames,
            elapsed_ticks: r.elapsed_ticks,
            total_attempts: s.attempts,
            total_successes: s.successes,
            total_failures: s.failures,
            fs_returned_frames: r.fs_returned_frames,
            vm_returned_frames: r.vm_returned_frames,
            vm_page_outs_lo: r.vm_page_outs.min(u64::from(u32::MAX)) as u32,
            vm_failures_lo: r.vm_failures.min(u64::from(u32::MAX)) as u32,
            fs_returned_bytes: r.fs_returned_bytes,
            vm_returned_bytes: r.vm_returned_bytes,
            vm_page_outs: r.vm_page_outs,
            vm_failures: r.vm_failures,
            ..Default::default()
        });
        i32::from(r.returned_frames > 0)
    }
}
