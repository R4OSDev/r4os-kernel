//! Resident page-state spans and exact split/coalescing ownership. Every
//! Engine operation is bounded under VM admission and performs no waiting I/O.
#![allow(non_upper_case_globals)]
use super::{
    vm_index,
    vm_pager_types::*,
    vm_ranges::{self, OptionalSlot, Range},
};
use core::cell::UnsafeCell;

pub(super) struct State {
    pub spans: [PageStateSpan; MAX_PAGE_STATE_SPANS],
    pub summary: PageStateSummary,
    pub free: Option<u16>,
    pub active: u32,
    pub cursor_slot: Option<u16>,
    pub cursor_page: u64,
    pub hot: HotPathStats,
}
impl State {
    const INIT: Self = Self {
        spans: [PageStateSpan::EMPTY; MAX_PAGE_STATE_SPANS],
        summary: PageStateSummary::INIT,
        free: None,
        active: 0,
        cursor_slot: None,
        cursor_page: 0,
        hot: HotPathStats::INIT,
    };
    pub fn reset(&mut self) {
        self.free = None;
        for (index, span) in self.spans.iter_mut().enumerate() {
            *span = PageStateSpan {
                next: self.free,
                ..PageStateSpan::EMPTY
            };
            self.free = Some(index as u16);
        }
        self.summary = PageStateSummary::INIT;
        self.active = 0;
        self.cursor_slot = None;
        self.cursor_page = 0;
        self.hot = HotPathStats::INIT;
    }
    fn steps(&mut self, steps: usize) {
        let value = steps.min(u32::MAX as usize) as u32;
        self.hot.page_state_span_steps = self
            .hot
            .page_state_span_steps
            .wrapping_add(u64::from(value));
        self.hot.page_state_span_step_max = self.hot.page_state_span_step_max.max(value);
    }
    fn lookup(&mut self) {
        self.hot.page_state_span_lookups = self.hot.page_state_span_lookups.wrapping_add(1);
    }
    pub fn fault_in_range(&mut self, range: Range, page: u64) -> Option<FaultPageState> {
        self.lookup();
        let mut current = range.page_state_span_head.get();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            let Some(end) = span.first_page.checked_add(span.page_count) else {
                self.steps(steps);
                return None;
            };
            if page < span.first_page {
                break;
            }
            if page < end {
                self.steps(steps);
                return Some(FaultPageState {
                    flags: span.flags,
                    slot_reservation_id: span.slot_reservation_id,
                    slot_index: if span.flags & page_state_flag_slot_bound != 0 {
                        span.slot_index + (page - span.first_page)
                    } else {
                        0
                    },
                    slot_generation: span.slot_generation,
                });
            }
            current = span.next;
        }
        self.steps(steps);
        None
    }
    pub fn add(
        &mut self,
        id: u32,
        first: u64,
        pages: u64,
        flags: u32,
        binding: SlotBinding,
    ) -> Result<(), u32> {
        if pages == 0 {
            return Ok(());
        }
        first.checked_add(pages).ok_or(OVERFLOW)?;
        let index = index(id).ok_or(NOT_FOUND)?;
        self.lookup();
        let mut previous = None;
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            if self.spans[usize::from(slot)].first_page >= first {
                break;
            }
            previous = Some(slot);
            current = self.spans[usize::from(slot)].next;
        }
        self.steps(steps);
        let slot = self.allocate().ok_or(TABLE_FULL)?;
        self.spans[usize::from(slot)] = PageStateSpan {
            slot_used: true,
            range_id: id,
            first_page: first,
            page_count: pages,
            flags,
            slot_reservation_id: binding.reservation_id,
            slot_index: binding.slot_index,
            slot_generation: binding.slot_generation,
            prev: previous,
            next: current,
        };
        if let Some(previous) = previous {
            self.spans[usize::from(previous)].next = Some(slot)
        } else {
            vm_ranges::update(index, |range| {
                range.page_state_span_head = OptionalSlot::from(Some(slot))
            })
        }
        if let Some(next) = current {
            self.spans[usize::from(next)].prev = Some(slot)
        }
        self.coalesce(index)?;
        self.summary.transitions = self.summary.transitions.wrapping_add(1);
        Ok(())
    }
    // Preserve the original independent flags, binding and transition contract.
    #[allow(clippy::too_many_arguments)]
    pub fn set(
        &mut self,
        id: u32,
        first: u64,
        pages: u64,
        add: u32,
        remove: u32,
        binding: Option<SlotBinding>,
        count: bool,
    ) -> Result<(), u32> {
        if pages == 0 {
            return Ok(());
        }
        let end = first.checked_add(pages).ok_or(OVERFLOW)?;
        let index = index(id).ok_or(NOT_FOUND)?;
        let mut reserved = self.reserve_splits(vm_ranges::read(index), first, end)?;
        let result = self.set_reserved(
            index,
            first,
            end,
            add,
            remove,
            binding,
            count,
            &mut reserved,
        );
        self.release_reserved(&mut reserved);
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn set_reserved(
        &mut self,
        index: usize,
        first: u64,
        end: u64,
        add: u32,
        remove: u32,
        binding: Option<SlotBinding>,
        count: bool,
        reserved: &mut SplitSlots,
    ) -> Result<(), u32> {
        self.split_boundaries(vm_ranges::read(index), first, end, reserved)?;
        let mut changed = false;
        self.lookup();
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = &mut self.spans[usize::from(slot)];
            if span.first_page >= end {
                break;
            }
            let span_end = span
                .first_page
                .checked_add(span.page_count)
                .ok_or(OVERFLOW)?;
            if span.first_page >= first && span_end <= end {
                let before = span.flags;
                span.flags |= add;
                span.flags &= !remove;
                if let Some(binding) = binding {
                    span.slot_reservation_id = binding.reservation_id;
                    span.slot_index = binding.slot_index + (span.first_page - first);
                    span.slot_generation = binding.slot_generation;
                }
                if span.flags & page_state_flag_slot_bound == 0 {
                    span.slot_reservation_id = 0;
                    span.slot_index = 0;
                    span.slot_generation = 0;
                }
                if before != span.flags || binding.is_some() {
                    changed = true
                }
            }
            current = span.next;
        }
        self.steps(steps);
        self.coalesce(index)?;
        if changed && count {
            self.summary.transitions = self.summary.transitions.wrapping_add(1)
        }
        Ok(())
    }
    pub fn reserve_splits(
        &mut self,
        range: Range,
        first: u64,
        end: u64,
    ) -> Result<SplitSlots, u32> {
        let mut needed = 0;
        self.lookup();
        let mut current = range.page_state_span_head.get();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            if span.first_page >= end {
                break;
            }
            let span_end = span
                .first_page
                .checked_add(span.page_count)
                .ok_or(OVERFLOW)?;
            if first > span.first_page && first < span_end {
                needed += 1
            }
            if end > span.first_page && end < span_end {
                needed += 1
            }
            if needed == 2 {
                break;
            }
            current = span.next;
        }
        self.steps(steps);
        if self.active as usize + needed > MAX_PAGE_STATE_SPANS {
            return Err(TABLE_FULL);
        }
        let mut reserved = [None, None];
        for value in reserved.iter_mut().take(needed) {
            let Some(slot) = self.allocate() else {
                self.release_reserved(&mut reserved);
                return Err(TABLE_FULL);
            };
            *value = Some(slot);
        }
        Ok(reserved)
    }
    fn split_boundaries(
        &mut self,
        range: Range,
        first: u64,
        end: u64,
        reserved: &mut SplitSlots,
    ) -> Result<(), u32> {
        self.split_boundary(range, first, reserved)?;
        if end != first {
            self.split_boundary(range, end, reserved)?
        }
        Ok(())
    }
    fn split_boundary(
        &mut self,
        range: Range,
        boundary: u64,
        reserved: &mut SplitSlots,
    ) -> Result<(), u32> {
        let mut current = range.page_state_span_head.get();
        while let Some(slot) = current {
            let span = self.spans[usize::from(slot)];
            if boundary <= span.first_page {
                return Ok(());
            }
            let end = span
                .first_page
                .checked_add(span.page_count)
                .ok_or(OVERFLOW)?;
            if boundary < end {
                let right = reserved
                    .iter_mut()
                    .find_map(Option::take)
                    .ok_or(TABLE_FULL)?;
                self.split_span(slot, boundary, right);
                return Ok(());
            }
            current = span.next;
        }
        Ok(())
    }
    fn split_span(&mut self, slot: u16, split: u64, right: u16) {
        let span = self.spans[usize::from(slot)];
        let end = span.first_page + span.page_count;
        self.spans[usize::from(right)] = PageStateSpan {
            slot_used: true,
            range_id: span.range_id,
            first_page: split,
            page_count: end - split,
            flags: span.flags,
            slot_reservation_id: span.slot_reservation_id,
            slot_index: if span.flags & page_state_flag_slot_bound != 0 {
                span.slot_index + (split - span.first_page)
            } else {
                0
            },
            slot_generation: span.slot_generation,
            prev: Some(slot),
            next: span.next,
        };
        self.spans[usize::from(slot)].next = Some(right);
        self.spans[usize::from(slot)].page_count = split - span.first_page;
        if let Some(next) = span.next {
            self.spans[usize::from(next)].prev = Some(right)
        }
    }
    pub fn remove_reserved(
        &mut self,
        range: Range,
        first: u64,
        end: u64,
        reserved: &mut SplitSlots,
    ) -> Result<(), u32> {
        self.split_boundaries(range, first, end, reserved)?;
        let raw_index = unsafe { vm_index::r4os_rust_vm_index_query(2, u64::from(range.id)) };
        if raw_index == usize::MAX {
            return Err(NOT_FOUND);
        }
        let index = raw_index;
        let mut removed = 0u64;
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        while let Some(slot) = current {
            let span = self.spans[usize::from(slot)];
            if span.first_page >= end {
                break;
            }
            let span_end = span
                .first_page
                .checked_add(span.page_count)
                .ok_or(OVERFLOW)?;
            if span.first_page >= first && span_end <= end {
                removed = removed.wrapping_add(span.page_count);
                self.unlink(index, slot);
                self.release(slot);
            }
            current = span.next;
        }
        self.summary.cleanup_pages = self.summary.cleanup_pages.wrapping_add(removed);
        self.coalesce(index)
    }
    pub fn remove_for_range(&mut self, id: u32) {
        let Some(index) = index(id) else { return };
        let mut removed = 0u64;
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        while let Some(slot) = current {
            let next = self.spans[usize::from(slot)].next;
            removed = removed.wrapping_add(self.spans[usize::from(slot)].page_count);
            self.release(slot);
            current = next;
        }
        vm_ranges::update(index, |range| {
            range.page_state_span_head = OptionalSlot::NONE
        });
        self.summary.cleanup_pages = self.summary.cleanup_pages.wrapping_add(removed);
    }
    fn coalesce(&mut self, index: usize) -> Result<(), u32> {
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        while let Some(slot) = current {
            let left = self.spans[usize::from(slot)];
            let Some(next) = left.next else { break };
            let right = self.spans[usize::from(next)];
            let end = left
                .first_page
                .checked_add(left.page_count)
                .ok_or(OVERFLOW)?;
            if end == right.first_page && mergeable(left, right) {
                self.spans[usize::from(slot)].page_count += right.page_count;
                self.unlink(index, next);
                self.release(next);
            } else {
                current = Some(next)
            }
        }
        Ok(())
    }
    pub fn fill_counts(&mut self, id: u32, first: u64, pages: u64, result: &mut PageStateResult) {
        let Some(end) = first.checked_add(pages) else {
            return;
        };
        result.span_count = self.active;
        result.flags |= page_state_flag_hardware_dirty_synced;
        let Some(index) = index(id) else { return };
        let mut first_slot_seen = false;
        self.lookup();
        let mut current = vm_ranges::read(index).page_state_span_head.get();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            if span.first_page >= end {
                break;
            }
            if let Some(span_end) = span.first_page.checked_add(span.page_count)
                && first < span_end
                && span.first_page < end
            {
                let overlap_start = span.first_page.max(first);
                let overlap_end = span_end.min(end);
                let count = overlap_end - overlap_start;
                result.flags |= span.flags;
                if span.flags & page_state_flag_committed != 0 {
                    result.committed_pages += count
                }
                if span.flags & page_state_flag_resident != 0 {
                    result.resident_pages += count
                }
                if span.flags & page_state_flag_dirty != 0 {
                    result.dirty_pages += count
                }
                if span.flags & page_state_flag_pinned != 0 {
                    result.pinned_pages += count
                }
                if span.flags & page_state_flag_busy != 0 {
                    result.busy_pages += count
                }
                if span.flags & page_state_flag_error != 0 {
                    result.error_pages += count
                }
                if span.flags & page_state_flag_slot_bound != 0 {
                    result.slot_bound_pages += count;
                    if !first_slot_seen {
                        result.slot_reservation_id = span.slot_reservation_id;
                        result.slot_index = span.slot_index + (overlap_start - span.first_page);
                        result.slot_generation = span.slot_generation;
                        first_slot_seen = true;
                    }
                }
            }
            current = span.next;
        }
        self.steps(steps);
        if result.committed_pages >= result.resident_pages {
            result.nonresident_pages = result.committed_pages - result.resident_pages
        }
        if result.committed_pages >= result.dirty_pages {
            result.clean_pages = result.committed_pages - result.dirty_pages
        }
        self.copy_counters(result);
    }
    pub fn make_result(&self, input: PageStateInput) -> PageStateResult {
        let mut result = PageStateResult {
            operation: input.operation,
            flags: page_state_flag_vm_owned_state
                | page_state_flag_explicit_request
                | page_state_flag_eviction_enabled
                | page_state_flag_no_swap
                | page_state_flag_page_sized
                | page_state_flag_fault_page_in,
            region_id: input.region_id,
            span_count: self.active,
            region_offset: input.region_offset,
            page_count: input.page_count,
            page_count_lo: input.page_count.min(u64::from(u32::MAX)) as u32,
            slot_reservation_id: input.slot_reservation_id,
            slot_index: input.slot_index,
            slot_generation: input.slot_generation,
            ..PageStateResult::INIT
        };
        self.copy_counters(&mut result);
        result
    }
    fn allocate(&mut self) -> Option<u16> {
        let slot = self.free?;
        self.free = self.spans[usize::from(slot)].next;
        self.spans[usize::from(slot)] = PageStateSpan::EMPTY;
        self.active = self.active.saturating_add(1);
        Some(slot)
    }
    fn release(&mut self, slot: u16) {
        self.spans[usize::from(slot)] = PageStateSpan {
            next: self.free,
            ..PageStateSpan::EMPTY
        };
        self.free = Some(slot);
        if self.active != 0 {
            self.active -= 1
        }
    }
    pub fn release_reserved(&mut self, reserved: &mut SplitSlots) {
        for value in reserved {
            if let Some(slot) = value.take() {
                self.release(slot)
            }
        }
    }
    fn unlink(&mut self, index: usize, slot: u16) {
        let span = self.spans[usize::from(slot)];
        if let Some(previous) = span.prev {
            self.spans[usize::from(previous)].next = span.next
        } else {
            vm_ranges::update(index, |range| {
                range.page_state_span_head = OptionalSlot::from(span.next)
            })
        }
        if let Some(next) = span.next {
            self.spans[usize::from(next)].prev = span.prev
        }
    }
}
pub(super) fn mergeable(a: PageStateSpan, b: PageStateSpan) -> bool {
    if a.flags != b.flags {
        return false;
    }
    if a.slot_reservation_id != b.slot_reservation_id || a.slot_generation != b.slot_generation {
        return false;
    }
    if a.flags & page_state_flag_slot_bound == 0 {
        return true;
    }
    b.slot_index == a.slot_index + a.page_count
}
pub(super) fn index(id: u32) -> Option<usize> {
    let slot = unsafe { vm_index::r4os_rust_vm_index_query(1, u64::from(id)) };
    (slot != usize::MAX).then_some(slot)
}
pub(super) fn fault(id: u32, page: u64) -> Option<FaultPageState> {
    let index = index(id)?;
    let range = vm_ranges::read(index);
    with(|state| state.fault_in_range(range, page))
}
pub(super) fn set(
    id: u32,
    first: u64,
    pages: u64,
    add: u32,
    remove: u32,
    binding: Option<SlotBinding>,
    count: bool,
) -> Result<(), u32> {
    with(|state| state.set(id, first, pages, add, remove, binding, count))
}
struct Storage(UnsafeCell<State>);
// SAFETY: existing VM owner admits every operation, or singleton BSP reset
// precedes publication. Closures cannot retain a borrow across PTE or I/O work.
unsafe impl Sync for Storage {}
static STORAGE: Storage = Storage(UnsafeCell::new(State::INIT));
pub(super) fn with<T>(operation: impl FnOnce(&mut State) -> T) -> T {
    operation(unsafe { &mut *STORAGE.0.get() })
}

impl State {
    fn copy_counters(&self, result: &mut PageStateResult) {
        result.total_transitions = self.summary.transitions;
        result.dirty_marks = self.summary.dirty_marks;
        result.clean_marks = self.summary.clean_marks;
        result.slot_binds = self.summary.slot_binds;
        result.slot_clears = self.summary.slot_clears;
        result.pinned_marks = self.summary.pinned_marks;
        result.pinned_clears = self.summary.pinned_clears;
        result.busy_marks = self.summary.busy_marks;
        result.busy_clears = self.summary.busy_clears;
        result.error_marks = self.summary.error_marks;
        result.error_clears = self.summary.error_clears;
        result.table_full_failures = self.summary.table_full_failures;
        result.cleanup_pages = self.summary.cleanup_pages;
    }
    pub fn record(&mut self, result: &PageStateResult) {
        self.summary.enabled = true;
        self.summary.last_status = result.status;
        self.summary.last_operation = result.operation;
        self.summary.last_flags = result.flags;
        self.summary.last_blockers = result.blockers;
        self.summary.last_region_id = result.region_id;
        self.summary.last_region_offset = result.region_offset;
        self.summary.last_page_count = result.page_count;
        self.summary.last_slot_reservation_id = result.slot_reservation_id;
        self.summary.last_slot_index = result.slot_index;
        self.summary.last_slot_generation = result.slot_generation;
        self.summary.committed_pages = result.committed_pages;
        self.summary.resident_pages = result.resident_pages;
        self.summary.nonresident_pages = result.nonresident_pages;
        self.summary.dirty_pages = result.dirty_pages;
        self.summary.clean_pages = result.clean_pages;
        self.summary.pinned_pages = result.pinned_pages;
        self.summary.busy_pages = result.busy_pages;
        self.summary.error_pages = result.error_pages;
        self.summary.slot_bound_pages = result.slot_bound_pages;
        self.summary.span_count = result.span_count;
    }
}
