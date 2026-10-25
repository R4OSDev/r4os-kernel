//! Existing explicitly requested heap boot checks and bounded SMP acceptance.
//! These checks use the production allocator and retain the original oracles.
use super::{blocks, heap, heap_check, paging, phys};
use crate::boot::log_route::{dec, puts};
use core::mem::MaybeUninit;

fn stats() -> heap::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        heap::r4os_rust_heap_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn phys_stats() -> phys::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        phys::r4os_rust_phys_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn block_stats() -> blocks::HotStats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        blocks::r4os_rust_blocks_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn paging_stats() -> paging::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        paging::r4os_rust_paging_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn fail(reason: &[u8]) -> bool {
    puts(b"HEAPCHECK FAIL reason=");
    puts(reason);
    puts(b"\r\n");
    false
}
pub(super) fn self_test() -> bool {
    let report = heap_check::boot_range(1, 5);
    if report.status != crate::contract::kernel_heap_check_status_passed {
        return fail(b"owned-heap-check");
    }
    let s = stats();
    puts(b"HEAPCHECK OK ops=");
    dec(u64::from(report.churn_ops));
    puts(b" committed=");
    dec(s.committed_bytes as u64);
    puts(b" largest_free=");
    dec(s.largest_free as u64);
    puts(b"\r\n");
    true
}

pub(crate) fn acceptance() -> crate::arch::smp_probe::HeapReport {
    const ITERATIONS: u64 = 16;
    const REQUEST: usize = 192 * 1024;
    let before = stats();
    let b0 = block_stats();
    let p0 = phys_stats();
    let map0 = paging_stats();
    let own = heap_check::boot_range(6, 6);
    let completed = u64::from(own.acceptance_iterations);
    let after = stats();
    let b1 = block_stats();
    let p1 = phys_stats();
    let map1 = paging_stats();
    let commits = after.commit_calls.saturating_sub(before.commit_calls);
    let pages = after
        .committed_pages_total
        .saturating_sub(before.committed_pages_total);
    let uncommits = after.uncommit_calls.saturating_sub(before.uncommit_calls);
    let suppressed = after
        .release_suppressed
        .saturating_sub(before.release_suppressed);
    let poison = after.poison_bytes.saturating_sub(before.poison_bytes);
    let pressure = after.retained_tail_pages == 0;
    let claims = b1.claim_transactions.saturating_sub(b0.claim_transactions);
    let extents = p1.extent_allocations.saturating_sub(p0.extent_allocations);
    let maps = map1.map_batches.saturating_sub(map0.map_batches);
    let unmaps = map1.unmap_batches.saturating_sub(map0.unmap_batches);
    let restored =
        after.used_bytes == before.used_bytes && after.active_blocks == before.active_blocks;
    let retention = after.pages <= before.pages + super::heap_policy::MAX_GROWTH_PAGES;
    let churn = if pressure {
        commits <= ITERATIONS && uncommits <= ITERATIONS
    } else {
        commits <= 2 && uncommits <= 1 && suppressed + uncommits >= ITERATIONS
    };
    let accounting = map1.map_batch_rollbacks == map0.map_batch_rollbacks
        && b1.claim_rollbacks == b0.claim_rollbacks
        && ((pages == 0 && extents == 0 && maps == 0)
            || (pages > 0 && extents > 0 && maps > 0 && claims <= pages));
    let ok = own.status == crate::contract::kernel_heap_check_status_passed
        && completed == ITERATIONS
        && restored
        && retention
        && churn
        && accounting
        && after.commit_failures == before.commit_failures
        && after.uncommit_failures == before.uncommit_failures
        && (if heap::FULL_POISON {
            poison >= REQUEST as u64 * ITERATIONS
        } else {
            poison == 0
        })
        && heap_check::boot_range(7, 7).status == crate::contract::kernel_heap_check_status_passed;
    crate::arch::smp_probe::HeapReport {
        ok,
        iterations: completed,
        commit_calls: commits,
        commit_pages: pages,
        uncommit_calls: uncommits,
        release_suppressed: suppressed,
        poison_bytes: poison,
        retained_pages: after.retained_tail_pages as u64,
        block_claims: claims,
        extent_allocations: extents,
        map_batches: maps,
        unmap_batches: unmaps,
        under_pressure: pressure,
    }
}
