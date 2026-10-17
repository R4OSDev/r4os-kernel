//! Original HeapCheck and SMP acceptance oracles, shared by boot and Spectra.
//! All live identities stay in State until their real return acknowledgment.
use super::{allocations::State, c};
use crate::memory::{
    paging, phys, vm_pager_api as vm, vm_pager_types, vm_query, vm_ranges, vm_transactions as tx,
};

fn pattern(pointer: *mut u8, len: usize, byte: u8) -> bool {
    unsafe {
        core::slice::from_raw_parts(pointer, len)
            .iter()
            .all(|b| *b == byte)
    }
}
pub(super) fn run(case: u32, state: &mut State, cancelled: fn() -> bool) -> bool {
    match case {
        c::kernel_heap_check_case_rust_allocator => {
            // Three tiny Rust dispatches may not wait while this ordinary
            // allocator owner is held. Only this case can change its snapshot.
            let _owner = crate::memory::owner::Guard::acquire(4);
            let before = super::stats();
            if !state.rust_allocate(0, 64, 64) {
                return false;
            }
            unsafe {
                state.pointer(0).write_bytes(0x6a, 64);
            }
            let original = state.pointer(0);
            if !state.rust_resize(0, 160) {
                return false;
            }
            let resized = state.pointer(0);
            let valid = resized.addr().is_multiple_of(64) && pattern(resized, 64, 0x6a);
            let released = state.release(0);
            let after = super::stats();
            let balanced = before.used_bytes == after.used_bytes
                && before.active_blocks == after.active_blocks;
            if !balanced {
                state.retain_rust_balance_failure(original, resized);
            }
            valid && released && balanced
        }
        c::kernel_heap_check_case_patterns => {
            let sizes = [24, 400, 6000];
            for (index, len) in sizes.into_iter().enumerate() {
                if cancelled() || !state.allocate(index, len, 8) {
                    return false;
                }
                unsafe {
                    state.pointer(index).write_bytes(0xa0 + index as u8, len);
                }
            }
            for (index, len) in sizes.into_iter().enumerate() {
                if !pattern(state.pointer(index), len, 0xa0 + index as u8) || !state.release(index)
                {
                    return false;
                }
            }
            true
        }
        c::kernel_heap_check_case_alignment => {
            state.allocate(0, 100, 4096)
                && state.pointer(0).addr().is_multiple_of(4096)
                && state.release(0)
        }
        c::kernel_heap_check_case_double_free => {
            // Retain the original minimum-alignment request, then reject an
            // aligned allocation's duplicate after possible prefix coalescing.
            state.allocate(0, 64, 1)
                && state.release(0)
                && state.allocate(0, 64, 4096)
                && state.duplicate_free(0)
        }
        c::kernel_heap_check_case_churn => {
            let mut rng = 0x9e37_79b9_7f4a_7c15u64;
            for _ in 0..2000 {
                if cancelled() {
                    return false;
                }
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let slot = ((rng >> 33) % 200) as usize;
                let ok = if state.occupied(slot) {
                    state.release(slot)
                } else {
                    state.allocate(slot, 16 + ((rng >> 8) & 0x7ff) as usize, 8)
                };
                if !ok {
                    return false;
                }
                state.report.churn_ops += 1;
            }
            state.cleanup();
            state.report.owned_live_blocks == 0 && state.report.cleanup_errors == 0
        }
        c::kernel_heap_check_case_acceptance => {
            const BYTES: usize = 192 * 1024;
            for index in 0..16 {
                if cancelled() || !state.allocate(0, BYTES, 16) {
                    return false;
                }
                let byte = 0x51 + index as u8;
                unsafe {
                    state.pointer(0).write_bytes(byte, BYTES);
                }
                if unsafe {
                    state.pointer(0).read() != byte
                        || state.pointer(0).add(BYTES - 1).read() != byte
                } || !state.release(0)
                {
                    return false;
                }
                state.report.acceptance_iterations += 1;
            }
            true
        }
        c::kernel_heap_check_case_resident_commit => resident(state, cancelled),
        _ => false,
    }
}
fn range(id: u32) -> Option<vm_ranges::Range> {
    let mut out = core::mem::MaybeUninit::uninit();
    unsafe { vm_query::r4os_rust_vm_range_info(id, out.as_mut_ptr()).then(|| out.assume_init()) }
}
fn resident(state: &mut State, cancelled: fn() -> bool) -> bool {
    const PAGE: u64 = 4096;
    const BYTES: u64 = 257 * PAGE;
    let mut before = vm_pager_types::HotPathStats::default();
    unsafe {
        vm::r4os_rust_vm_hot_stats(&mut before);
    }
    if !state.reserve(0, 0, 0, BYTES) {
        return false;
    }
    let id = state.reservation(0);
    let Some(info) = range(id) else {
        return false;
    };
    let memory = core::ptr::with_exposed_provenance_mut::<u8>(info.base as usize);
    if tx::r4os_rust_vm_commit(id, BYTES - PAGE, PAGE) != 0 {
        return false;
    }
    unsafe {
        memory.add(BYTES as usize - 1).write(0xa5);
    }
    if cancelled() || tx::r4os_rust_vm_commit(id, 0, BYTES) != 9 {
        return false;
    }
    state.report.expected_vm_errors += 1;
    let Some(partial) = range(id) else {
        return false;
    };
    if partial.committed_bytes != PAGE
        || partial.resident_bytes != PAGE
        || unsafe { memory.add(BYTES as usize - 1).read() } != 0xa5
    {
        return false;
    }
    for offset in (0..BYTES - PAGE).step_by(PAGE as usize) {
        let mut frame = 0;
        if unsafe { paging::r4os_rust_paging_read(info.base + offset, 3, false, &mut frame) } {
            return false;
        }
    }
    if cancelled()
        || tx::r4os_rust_vm_uncommit(id, BYTES - PAGE, PAGE) != 0
        || tx::r4os_rust_vm_commit(id, 0, BYTES) != 0
    {
        return false;
    }
    if !pattern(memory, BYTES as usize, 0) {
        return false;
    }
    let Some(complete) = range(id) else {
        return false;
    };
    if complete.committed_bytes != BYTES
        || complete.resident_bytes != BYTES
        || !state.release_reservation(0)
    {
        return false;
    }
    let mut after = vm_pager_types::HotPathStats::default();
    unsafe {
        vm::r4os_rust_vm_hot_stats(&mut after);
    }
    if after
        .resident_commit_batches
        .saturating_sub(before.resident_commit_batches)
        < 6
        || after.resident_commit_max_batch_pages > 64
        || after
            .resident_commit_unlocked_zero_bytes
            .saturating_sub(before.resident_commit_unlocked_zero_bytes)
            < BYTES
        || after.resident_rollback_batches == before.resident_rollback_batches
    {
        return false;
    }
    let mut physical = phys::Stats::default();
    unsafe {
        phys::r4os_rust_phys_stats(&mut physical);
    }
    // More than ALL frames remains impossible despite concurrent foreign frees.
    let Some(impossible) = physical
        .total_frames
        .checked_add(1)
        .and_then(|v| v.checked_mul(PAGE))
    else {
        return false;
    };
    if cancelled() || !state.reserve(1, 4, 3, impossible) {
        return false;
    }
    let oom = state.reservation(1);
    if tx::r4os_rust_vm_commit(oom, 0, impossible) != 12 {
        return false;
    }
    state.report.expected_vm_errors += 1;
    range(oom).is_some_and(|empty| empty.committed_bytes == 0 && empty.resident_bytes == 0)
        && state.release_reservation(1)
}
