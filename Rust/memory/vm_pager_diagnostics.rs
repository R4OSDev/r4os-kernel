//! Existing explicit VM metadata invariant and passive merged telemetry.
use super::{
    owner::Guard, vm_commit_spans, vm_index, vm_page_state as ps, vm_pager_types::*, vm_query,
    vm_ranges, vm_transactions,
};
use core::mem::MaybeUninit;
fn query(op: u8, value: u64) -> usize {
    unsafe { vm_index::r4os_rust_vm_index_query(op, value) }
}
fn count() -> usize {
    query(5, 0)
}
fn order(position: usize) -> usize {
    vm_index::neighbors(position).1.expect("VM address order")
}
fn seen(words: &mut [u64], slot: u16) -> bool {
    let index = usize::from(slot);
    let word = index / 64;
    let mask = 1u64 << (index % 64);
    if words[word] & mask != 0 {
        return false;
    }
    words[word] |= mask;
    true
}
pub(super) fn invariant() -> bool {
    let _held = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() || count() != query(6, 0) {
        return false;
    }
    let mut commit_seen = [0u64; 4096 / 64];
    let mut page_seen = [0u64; MAX_PAGE_STATE_SPANS / 64];
    let mut active_ranges = 0;
    let mut linked_commits = 0;
    let mut linked_pages = 0;
    for slot in 0..vm_ranges::RANGES {
        let range = vm_ranges::read(slot);
        if !range.active() {
            if vm_index::position(slot).is_some() {
                return false;
            }
            continue;
        }
        active_ranges += 1;
        let Some(position) = vm_index::position(slot) else {
            return false;
        };
        if position >= count() || order(position) != slot || query(2, u64::from(range.id)) != slot {
            return false;
        }
        let mut previous = None;
        let mut cursor = range.commit_span_head.get();
        let mut steps = 0;
        while let Some(span_slot) = cursor {
            steps += 1;
            if steps > 4096 || !seen(&mut commit_seen, span_slot) {
                return false;
            }
            let span = vm_commit_spans::read(usize::from(span_slot));
            if !span.slot_used
                || span.range_id != range.id
                || span.prev.get() != previous
                || span.len == 0
            {
                return false;
            }
            if let Some(before) = previous {
                let before = vm_commit_spans::read(usize::from(before));
                let Some(end) = before.base.checked_add(before.len) else {
                    return false;
                };
                if end >= span.base {
                    return false;
                }
            }
            previous = Some(span_slot);
            linked_commits += 1;
            cursor = span.next.get();
        }
        let mut previous = None;
        let mut cursor = range.page_state_span_head.get();
        let mut steps = 0;
        while let Some(span_slot) = cursor {
            steps += 1;
            if steps > MAX_PAGE_STATE_SPANS || !seen(&mut page_seen, span_slot) {
                return false;
            }
            let span = ps::with(|s| s.spans[usize::from(span_slot)]);
            if !span.slot_used
                || span.range_id != range.id
                || span.prev != previous
                || span.page_count == 0
            {
                return false;
            }
            if let Some(before) = previous {
                let before = ps::with(|s| s.spans[usize::from(before)]);
                let Some(end) = before.first_page.checked_add(before.page_count) else {
                    return false;
                };
                if end > span.first_page || (end == span.first_page && ps::mergeable(before, span))
                {
                    return false;
                }
            }
            previous = Some(span_slot);
            linked_pages += 1;
            cursor = span.next;
        }
    }
    let commits = unsafe { vm_commit_spans::r4os_rust_vm_commit_span_cursor(false) };
    let pages = ps::with(|s| s.active as usize);
    if active_ranges != count() || linked_commits != commits || linked_pages != pages {
        return false;
    }
    for position in 0..count() {
        let slot = order(position);
        let range = vm_ranges::read(slot);
        if !range.active() || vm_index::position(slot) != Some(position) {
            return false;
        }
        if position != 0 {
            let previous = vm_ranges::read(order(position - 1));
            let Some(end) = previous.base.checked_add(previous.len) else {
                return false;
            };
            if end > range.base {
                return false;
            }
        }
    }
    let mut free_commits = 0;
    let first = unsafe { vm_commit_spans::r4os_rust_vm_commit_span_cursor(true) };
    let mut cursor = (first != usize::MAX).then(|| u16::try_from(first).expect("commit free head"));
    while let Some(slot) = cursor {
        free_commits += 1;
        if free_commits > 4096 {
            return false;
        }
        let span = vm_commit_spans::read(usize::from(slot));
        if span.slot_used || !seen(&mut commit_seen, slot) {
            return false;
        }
        cursor = span.next.get();
    }
    let mut free_pages = 0;
    let mut cursor = ps::with(|s| s.free);
    while let Some(slot) = cursor {
        free_pages += 1;
        if free_pages > MAX_PAGE_STATE_SPANS {
            return false;
        }
        let span = ps::with(|s| s.spans[usize::from(slot)]);
        if span.slot_used || !seen(&mut page_seen, slot) {
            return false;
        }
        cursor = span.next;
    }
    if linked_commits + free_commits != 4096 || linked_pages + free_pages != MAX_PAGE_STATE_SPANS {
        return false;
    }
    if let Some(slot) = ps::with(|s| s.cursor_slot) {
        let slot = usize::from(slot);
        if !vm_ranges::read(slot).active() || vm_index::position(slot).is_none() {
            return false;
        }
    }
    vm_ranges::budget_invariant()
}

fn text(value: &[u8]) {
    unsafe { vm_puts(value.as_ptr(), value.len()) }
}
fn dec(value: u64) {
    crate::boot::log_route::dec(value)
}
fn hex(value: u64) {
    crate::boot::log_route::hex(value, 16)
}
pub(super) fn dump() {
    let _held = Guard::acquire(5);
    let mut value = vm_query::Stats::default();
    unsafe { vm_query::r4os_rust_vm_stats(&mut value) };
    text(b"  Virtual ranges: active=");
    dec(value.active_ranges);
    text(b" reserved=");
    dec(value.reserved_bytes);
    text(b" committed=");
    dec(value.committed_bytes);
    text(b" resident=");
    dec(value.resident_bytes);
    text(b" peak-resident=");
    dec(value.peak_resident_bytes);
    text(b" guard=");
    dec(value.guard_bytes);
    text(b" faults=");
    dec(value.fault_count);
    text(b"/");
    dec(value.failed_faults);
    text(b" overflow=");
    text(if value.overflow { b"yes" } else { b"no" });
    text(b"\r\n");
    text(b"  Virtual largest free: base=0x");
    hex(value.largest_free_virtual_base);
    text(b" len=0x");
    hex(value.largest_free_virtual_len);
    text(b"\r\n");
    text(b"  App memory reserve: frames=");
    dec(value.app_system_reserve_frames);
    text(b" app_available_frames=");
    dec(value.app_available_frames);
    text(b"\r\n");
}

pub(super) fn hot_stats() -> HotPathStats {
    let _held = Guard::acquire(5);
    let mut out = ps::with(|s| s.hot);
    let mut resident = MaybeUninit::<vm_transactions::Stats>::uninit();
    unsafe { vm_transactions::r4os_rust_vm_transaction_stats(resident.as_mut_ptr()) };
    let resident = unsafe { resident.assume_init() };
    out.resident_commit_batches = resident.resident_commit_batches;
    out.resident_commit_max_batch_pages = resident.resident_commit_max_batch_pages;
    out.resident_commit_unlocked_zero_bytes = resident.resident_commit_unlocked_zero_bytes;
    out.resident_rollback_batches = resident.resident_rollback_batches;
    let mut indices = MaybeUninit::<vm_index::Stats>::uninit();
    unsafe { vm_index::r4os_rust_vm_index_stats(indices.as_mut_ptr()) };
    let indices = unsafe { indices.assume_init() };
    out.range_index_capacity = indices.range_index_capacity;
    out.range_index_entries = indices.range_index_entries;
    out.range_index_tombstones = indices.range_index_tombstones;
    out.range_index_lookups = indices.range_index_lookups;
    out.range_index_hits = indices.range_index_hits;
    out.range_index_misses = indices.range_index_misses;
    out.range_index_probe_total = indices.range_index_probe_total;
    out.range_index_probe_max = indices.range_index_probe_max;
    out.range_index_probe_last = indices.range_index_probe_last;
    out.range_index_rebuilds = indices.range_index_rebuilds;
    out.range_index_insert_failures = indices.range_index_insert_failures;
    out.range_free_slot_lookups = indices.range_free_slot_lookups;
    out.range_free_slot_probe_total = indices.range_free_slot_probe_total;
    out.range_free_slot_probe_max = indices.range_free_slot_probe_max;
    out.range_free_slot_probe_last = indices.range_free_slot_probe_last;
    out.range_address_entries = indices.range_address_entries;
    out.range_address_lookups = indices.range_address_lookups;
    out.range_address_probe_total = indices.range_address_probe_total;
    out.range_address_probe_max = indices.range_address_probe_max;
    out.range_address_probe_last = indices.range_address_probe_last;
    let mut commits = MaybeUninit::<vm_commit_spans::Stats>::uninit();
    unsafe { vm_commit_spans::r4os_rust_vm_commit_span_stats(commits.as_mut_ptr()) };
    let commits = unsafe { commits.assume_init() };
    out.commit_span_active = commits.commit_span_active;
    out.commit_span_lookups = commits.commit_span_lookups;
    out.commit_span_steps = commits.commit_span_steps;
    out.commit_span_step_max = commits.commit_span_step_max;
    out.page_state_span_active = ps::with(|s| s.active);
    out
}

unsafe fn vm_puts(text: *const u8, len: usize) {
    unsafe { crate::boot::log_route::r4os_rust_log_write(0, text, len) }
}
