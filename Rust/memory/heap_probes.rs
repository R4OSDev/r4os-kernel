//! Existing explicitly requested heap boot checks and bounded SMP acceptance.
//! These checks use the production allocator and retain the original oracles.
use super::{
    blocks, heap, paging, phys, vm_pager_api as vm, vm_query, vm_ranges, vm_transactions as tx,
};
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
fn vm_stats() -> super::vm_pager_types::HotPathStats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        vm::r4os_rust_vm_hot_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn release(p: *mut u8, len: usize) -> bool {
    unsafe { heap::r4os_rust_heap_free(p, len) == 0 }
}
fn fail(reason: &[u8]) -> bool {
    puts(b"HEAPCHECK FAIL reason=");
    puts(reason);
    puts(b"\r\n");
    false
}
fn cycles() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

pub(super) fn self_test() -> bool {
    if !heap::r4os_rust_heap_initialized() {
        return fail(b"not-initialized");
    }
    if !heap::r4os_rust_heap_allocator_probe() {
        return fail(b"rust-allocator");
    }
    let sizes = [24, 400, 6000];
    let mut held = [core::ptr::null_mut(); 3];
    for (i, size) in sizes.iter().copied().enumerate() {
        let p = heap::r4os_rust_heap_alloc(size, 8);
        if p.is_null() {
            return fail(b"alloc-basic");
        }
        unsafe {
            p.write_bytes(0xa0 + i as u8, size);
        }
        held[i] = p;
    }
    for (i, (p, size)) in held.into_iter().zip(sizes).enumerate() {
        if unsafe { core::slice::from_raw_parts(p, size) }
            .iter()
            .any(|b| *b != 0xa0 + i as u8)
        {
            return fail(b"pattern");
        }
        if !release(p, size) {
            return fail(b"free-basic");
        }
    }
    let aligned = heap::r4os_rust_heap_alloc(100, 4096);
    if aligned.is_null() {
        return fail(b"alloc-aligned");
    }
    if aligned.addr() & 4095 != 0 {
        return fail(b"alignment");
    }
    if !release(aligned, 100) {
        return fail(b"free-aligned");
    }
    let df_before = stats().double_free_errors;
    let probe = heap::r4os_rust_heap_alloc(64, 1);
    if probe.is_null() {
        return fail(b"alloc-df");
    }
    if !release(probe, 64) {
        return fail(b"free-df1");
    }
    if release(probe, 64) {
        return fail(b"double-free-accepted");
    }
    if stats().double_free_errors != df_before + 1 {
        return fail(b"double-free-count");
    }
    let mut slots: [Option<(*mut u8, usize)>; 200] = [None; 200];
    let mut rng = 0x9e37_79b9_7f4a_7c15u64;
    let (mut alloc_cycles, mut free_cycles, mut alloc_ops, mut free_ops) = (0u64, 0u64, 0u64, 0u64);
    let start = cycles();
    for _ in 0..2000 {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let slot = ((rng >> 33) % 200) as usize;
        if let Some((p, len)) = slots[slot] {
            let t = cycles();
            let ok = release(p, len);
            free_cycles = free_cycles.wrapping_add(cycles().wrapping_sub(t));
            free_ops += 1;
            if !ok {
                return fail(b"churn-free");
            }
            slots[slot] = None;
        } else {
            let len = 16 + ((rng >> 8) & 0x7ff) as usize;
            let t = cycles();
            let p = heap::r4os_rust_heap_alloc(len, 8);
            alloc_cycles = alloc_cycles.wrapping_add(cycles().wrapping_sub(t));
            alloc_ops += 1;
            if p.is_null() {
                return fail(b"churn-alloc");
            }
            slots[slot] = Some((p, len));
        }
    }
    for entry in &mut slots {
        if let Some((p, len)) = *entry {
            if !release(p, len) {
                return fail(b"churn-cleanup");
            }
            *entry = None;
        }
    }
    let total = cycles().wrapping_sub(start);
    let s = stats();
    if s.used_bytes != 0 || s.active_blocks != 0 {
        return fail(b"leak");
    }
    puts(b"HEAPCHECK OK ops=");
    dec(2000);
    puts(b" alloc_avg=");
    dec(alloc_cycles.checked_div(alloc_ops).unwrap_or(0));
    puts(b" free_avg=");
    dec(free_cycles.checked_div(free_ops).unwrap_or(0));
    puts(b" churn=");
    dec(total);
    puts(b" committed=");
    dec(s.committed_bytes as u64);
    puts(b" largest_free=");
    dec(s.largest_free as u64);
    puts(b"\r\n");
    true
}

struct Reservation(u32);
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.0 != 0 {
            tx::r4os_rust_vm_release(self.0);
        }
    }
}
fn reserve(window: u8, owner: u8, len: u64, name: &'static [u8]) -> Option<Reservation> {
    let r = vm_ranges::Reserve {
        eviction_allowed: true,
        window,
        kind: 4,
        owner,
        owner_id: 0,
        name_ptr: name.as_ptr(),
        name_len: name.len(),
        flags: 2 | (1 << 63),
        base: 0,
        len,
    };
    let mut id = 0;
    if unsafe { vm_ranges::r4os_rust_vm_reserve(&r, 4096, &mut id) } == 0 {
        Some(Reservation(id))
    } else {
        None
    }
}
fn range(id: u32) -> Option<vm_ranges::Range> {
    let mut r = MaybeUninit::uninit();
    unsafe { vm_query::r4os_rust_vm_range_info(id, r.as_mut_ptr()).then(|| r.assume_init()) }
}
fn resident_commit_probe() -> bool {
    const BYTES: u64 = 257 * 4096;
    let before = vm_stats();
    let Some(mut allocation) = reserve(0, 0, BYTES, b"resident-commit-probe") else {
        return false;
    };
    let id = allocation.0;
    let Some(info) = range(id) else {
        return false;
    };
    let memory = core::ptr::with_exposed_provenance_mut::<u8>(info.base as usize);
    if tx::r4os_rust_vm_commit(id, BYTES - 4096, 4096) != 0 {
        return false;
    }
    unsafe {
        memory.add(BYTES as usize - 1).write(0xa5);
    }
    if tx::r4os_rust_vm_commit(id, 0, BYTES) != 9 {
        return false;
    }
    let Some(partial) = range(id) else {
        return false;
    };
    if partial.committed_bytes != 4096
        || partial.resident_bytes != 4096
        || unsafe { memory.add(BYTES as usize - 1).read() } != 0xa5
    {
        return false;
    }
    for offset in (0..BYTES - 4096).step_by(4096) {
        let mut frame = 0;
        if unsafe { paging::r4os_rust_paging_read(info.base + offset, 3, false, &mut frame) } {
            return false;
        }
    }
    if tx::r4os_rust_vm_uncommit(id, BYTES - 4096, 4096) != 0
        || tx::r4os_rust_vm_commit(id, 0, BYTES) != 0
    {
        return false;
    }
    if unsafe { core::slice::from_raw_parts(memory, BYTES as usize) }
        .iter()
        .any(|b| *b != 0)
    {
        return false;
    }
    let Some(complete) = range(id) else {
        return false;
    };
    if complete.committed_bytes != BYTES || complete.resident_bytes != BYTES {
        return false;
    }
    if tx::r4os_rust_vm_release(id) != 0 {
        return false;
    }
    allocation.0 = 0;
    let after = vm_stats();
    if after.resident_commit_batches - before.resident_commit_batches < 6
        || after.resident_commit_max_batch_pages > 64
        || after.resident_commit_unlocked_zero_bytes - before.resident_commit_unlocked_zero_bytes
            < BYTES
        || after.resident_rollback_batches == before.resident_rollback_batches
    {
        return false;
    }
    let impossible = (phys_stats().free_frames + 1) * 4096;
    let Some(oom) = reserve(4, 3, impossible, b"resident-oom-probe") else {
        return false;
    };
    if tx::r4os_rust_vm_commit(oom.0, 0, impossible) != 12 {
        return false;
    }
    range(oom.0).is_some_and(|empty| empty.committed_bytes == 0 && empty.resident_bytes == 0)
}

pub(crate) fn acceptance() -> crate::arch::smp_probe::HeapReport {
    const ITERATIONS: u64 = 16;
    const REQUEST: usize = 192 * 1024;
    let before = stats();
    let b0 = block_stats();
    let p0 = phys_stats();
    let map0 = paging_stats();
    let mut completed = 0u64;
    while completed < ITERATIONS {
        let p = heap::r4os_rust_heap_alloc(REQUEST, 16);
        if p.is_null() {
            break;
        }
        let pattern = (0x51 + completed) as u8;
        unsafe {
            p.write_bytes(pattern, REQUEST);
        }
        if unsafe { p.read() != pattern || p.add(REQUEST - 1).read() != pattern } {
            release(p, REQUEST);
            break;
        }
        if !release(p, REQUEST) {
            break;
        }
        completed += 1;
    }
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
    let ok = completed == ITERATIONS
        && restored
        && retention
        && churn
        && accounting
        && after.commit_failures == before.commit_failures
        && after.uncommit_failures == before.uncommit_failures
        && poison >= REQUEST as u64 * ITERATIONS
        && resident_commit_probe();
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
