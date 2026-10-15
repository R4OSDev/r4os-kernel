//! Complete demand VM and waiting pager. Only stable indices, independent
//! value snapshots and exact task unwind tokens cross I/O or owner handoffs.
#![allow(non_upper_case_globals)]
use super::{
    backing_store as bs, backing_types as backing, blocks, owner, page_batch, paging, phys,
    reclaim, virt_policy, vm_commit_spans, vm_frames, vm_index, vm_page_state as ps,
    vm_pager_types::*,
    vm_ranges::{self, OptionalSlot, Range},
    vm_transactions,
};
use crate::{
    arch::percpu,
    sched::{sync_types, sync_wait, task_context},
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};

// Same CPU-bound VM owner; a token is explicitly consumed before waiting.
struct Held {
    token: Option<owner::Token>,
    _cpu_bound: core::marker::PhantomData<*mut ()>,
}
impl Held {
    fn acquire() -> Self {
        Self {
            token: Some(owner::named(5).acquire()),
            _cpu_bound: core::marker::PhantomData,
        }
    }
    fn from(token: owner::Token) -> Self {
        Self {
            token: Some(token),
            _cpu_bound: core::marker::PhantomData,
        }
    }
    fn release(&mut self) {
        owner::named(5).release(self.token.take().expect("VM owner held"));
    }
    fn reacquire(&mut self) {
        assert!(self.token.is_none());
        self.token = Some(owner::named(5).acquire());
    }
    fn take(&mut self) -> owner::Token {
        self.token.take().expect("VM owner held")
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            owner::named(5).release(token)
        }
    }
}
fn check(code: u32) -> Result<(), u32> {
    if code == 0 { Ok(()) } else { Err(code) }
}
fn ready() -> bool {
    vm_transactions::r4os_rust_vm_initialized()
}
fn lookup(id: u32) -> Result<usize, u32> {
    ps::index(id).ok_or(NOT_FOUND)
}
fn query(op: u8, value: u64) -> usize {
    unsafe { vm_index::r4os_rust_vm_index_query(op, value) }
}
fn count() -> usize {
    query(5, 0)
}
fn ordered(position: usize) -> usize {
    vm_index::neighbors(position).1.expect("VM ordered slot")
}
fn mapped(address: u64) -> bool {
    let mut value = 0;
    unsafe { paging::r4os_rust_paging_read(address, 3, false, &mut value) }
}
fn frame(address: u64) -> Option<u64> {
    let mut value = 0;
    unsafe { paging::r4os_rust_paging_read(address, 0, false, &mut value) }.then_some(value)
}
fn dirty(address: u64) -> bool {
    let mut value = 0;
    (unsafe { paging::r4os_rust_paging_read(address, 4, false, &mut value) }) && value != 0
}
fn inside(range: Range, offset: u64, bytes: u64) -> Result<(), u32> {
    check(virt_policy::r4os_rust_vm_inside(range.len, offset, bytes))
}
fn covered(range: Range, address: u64, bytes: u64) -> bool {
    unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_scan(range.commit_span_head, address, bytes, true)
    }
}
fn overlaps(range: Range, address: u64, bytes: u64) -> bool {
    unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_scan(
            range.commit_span_head,
            address,
            bytes,
            false,
        )
    }
}
fn backing_result() -> Option<backing::Result> {
    let mut value = MaybeUninit::uninit();
    unsafe { bs::r4os_rust_backing_active_result(value.as_mut_ptr()) }
        .then(|| unsafe { value.assume_init() })
}
fn slot_probe(input: backing::SlotInput) -> backing::SlotResult {
    let mut value = MaybeUninit::uninit();
    unsafe {
        bs::r4os_rust_backing_slot_probe(&input, value.as_mut_ptr());
        value.assume_init()
    }
}
fn io_prepare(input: backing::PageIoInput) -> backing::PageIoResult {
    let mut value = MaybeUninit::uninit();
    unsafe {
        bs::r4os_rust_backing_page_io_prepare(&input, value.as_mut_ptr());
        value.assume_init()
    }
}
fn io_complete(input: backing::PageIoInput) -> backing::PageIoResult {
    let mut value = MaybeUninit::uninit();
    unsafe {
        bs::r4os_rust_backing_page_io_complete(&input, value.as_mut_ptr());
        value.assume_init()
    }
}
fn alloc_claimed(range: Range, reason: u32) -> Result<u64, u32> {
    let mut value = MaybeUninit::uninit();
    check(unsafe {
        vm_frames::r4os_rust_vm_allocate(&range, reason, 1, true, value.as_mut_ptr())
    })?;
    Ok(unsafe { value.assume_init() }.base)
}
fn release_claimed(frame: u64) {
    vm_frames::r4os_rust_vm_release_prepared(frame, 1, true)
}
fn uncommit_page(index: usize, address: u64) -> Result<(), u32> {
    check(unsafe {
        vm_frames::r4os_rust_vm_uncommit_extent(vm_ranges::slot(index), address, 0, 1, false, true)
    })
}
fn free_frames() -> u64 {
    let mut value = phys::Stats::default();
    unsafe { phys::r4os_rust_phys_stats(&mut value) };
    value.free_frames
}
fn can_commit(range: Range) -> bool {
    unsafe { vm_frames::r4os_rust_vm_can_commit(&range, 1) }
}
fn fault_success(index: usize) {
    vm_ranges::update(index, |range| {
        range.fault_count = range.fault_count.saturating_add(1);
        range.resident_bytes = range.resident_bytes.saturating_add(PAGE);
        range.peak_resident_bytes = range.peak_resident_bytes.max(range.resident_bytes);
    });
}
fn fault_failure(index: usize) {
    vm_ranges::update(index, |range| {
        range.failed_faults = range.failed_faults.saturating_add(1)
    });
}
fn sync_dirty(range: Range, first: u64, pages: u64) {
    if range.window != 6 || pages == 0 {
        return;
    }
    for i in 0..pages {
        let page = first + i;
        let address = range.base + page * PAGE;
        if mapped(address) && dirty(address) {
            let _ = ps::set(range.id, page, 1, page_state_flag_dirty, 0, None, false);
        }
    }
}
fn apply_dirty(range: Range, first: u64, pages: u64, dirty: bool) {
    for i in 0..pages {
        let address = range.base + (first + i) * PAGE;
        if mapped(address) {
            paging::r4os_rust_paging_dirty(address, !dirty);
        }
    }
}
fn validate_state(input: PageStateInput, result: &mut PageStateResult) -> bool {
    if !ready() {
        result.status = page_state_status_not_initialized;
        result.blockers |= page_state_blocker_not_initialized;
    } else if input.flags & !page_state_supported_flags != 0 {
        result.status = page_state_status_unsupported_flags;
        result.blockers |= page_state_blocker_unsupported_flags;
    } else if input.operation > page_state_operation_clear_error {
        result.status = page_state_status_unsupported_operation;
        result.blockers |= page_state_blocker_unsupported_operation;
    } else if input.region_id == 0 || input.page_count == 0 {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
    } else if input.region_offset & (PAGE - 1) != 0 {
        result.status = page_state_status_unaligned_request;
        result.blockers |= page_state_blocker_unaligned_request;
    } else if input.page_count > u64::MAX / PAGE {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
    } else {
        return true;
    }
    false
}
fn apply_operation(
    range: Range,
    input: PageStateInput,
    first: u64,
    result: &mut PageStateResult,
) -> Result<(), u32> {
    let id = range.id;
    let pages = input.page_count;
    match input.operation {
        page_state_operation_query => {}
        page_state_operation_mark_dirty => {
            ps::set(id, first, pages, page_state_flag_dirty, 0, None, true)?;
            apply_dirty(range, first, pages, true);
            ps::with(|s| s.summary.dirty_marks = s.summary.dirty_marks.wrapping_add(pages));
        }
        page_state_operation_mark_clean => {
            ps::set(id, first, pages, 0, page_state_flag_dirty, None, true)?;
            apply_dirty(range, first, pages, false);
            ps::with(|s| s.summary.clean_marks = s.summary.clean_marks.wrapping_add(pages));
        }
        page_state_operation_bind_slot => {
            ps::set(
                id,
                first,
                pages,
                page_state_flag_slot_bound,
                0,
                Some(SlotBinding {
                    reservation_id: input.slot_reservation_id,
                    slot_index: input.slot_index,
                    slot_generation: input.slot_generation,
                }),
                true,
            )?;
            ps::with(|s| s.summary.slot_binds = s.summary.slot_binds.wrapping_add(pages));
        }
        page_state_operation_clear_slot => {
            ps::set(
                id,
                first,
                pages,
                0,
                page_state_flag_slot_bound,
                Some(SlotBinding::INIT),
                true,
            )?;
            ps::with(|s| s.summary.slot_clears = s.summary.slot_clears.wrapping_add(pages));
        }
        page_state_operation_mark_pinned => {
            ps::set(id, first, pages, page_state_flag_pinned, 0, None, true)?;
            ps::with(|s| s.summary.pinned_marks = s.summary.pinned_marks.wrapping_add(pages));
        }
        page_state_operation_clear_pinned => {
            if !range.eviction_allowed {
                return Err(BUSY);
            }
            ps::set(id, first, pages, 0, page_state_flag_pinned, None, true)?;
            ps::with(|s| s.summary.pinned_clears = s.summary.pinned_clears.wrapping_add(pages));
        }
        page_state_operation_mark_busy => {
            ps::set(id, first, pages, page_state_flag_busy, 0, None, true)?;
            ps::with(|s| s.summary.busy_marks = s.summary.busy_marks.wrapping_add(pages));
        }
        page_state_operation_clear_busy => {
            ps::set(id, first, pages, 0, page_state_flag_busy, None, true)?;
            ps::with(|s| s.summary.busy_clears = s.summary.busy_clears.wrapping_add(pages));
        }
        page_state_operation_mark_error => {
            ps::set(id, first, pages, page_state_flag_error, 0, None, true)?;
            ps::with(|s| s.summary.error_marks = s.summary.error_marks.wrapping_add(pages));
        }
        page_state_operation_clear_error => {
            ps::set(id, first, pages, 0, page_state_flag_error, None, true)?;
            ps::with(|s| s.summary.error_clears = s.summary.error_clears.wrapping_add(pages));
        }
        _ => unreachable!(),
    }
    result.total_transitions = ps::with(|s| s.summary.transitions);
    Ok(())
}
pub(super) fn page_state_probe(input: PageStateInput) -> PageStateResult {
    let _held = Held::acquire();
    let mut result = ps::with(|s| s.make_result(input));
    state_probe(input, &mut result);
    ps::with(|s| s.record(&result));
    result
}
fn state_probe(input: PageStateInput, result: &mut PageStateResult) {
    if !validate_state(input, result) {
        return;
    }
    let Some(index) = ps::index(input.region_id) else {
        result.status = page_state_status_region_missing;
        result.blockers |= page_state_blocker_region_missing;
        return;
    };
    let range = vm_ranges::read(index);
    if range.window != 6 || range.kind != 4 {
        result.status = page_state_status_region_not_r4x;
        result.blockers |= page_state_blocker_region_not_r4x;
        return;
    }
    if range.pager_io_count != 0 && input.operation != page_state_operation_query {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
        return;
    }
    let Some(bytes) = input.page_count.checked_mul(PAGE) else {
        result.status = page_state_status_invalid_request;
        result.blockers |= page_state_blocker_invalid_request;
        return;
    };
    if !covered(range, range.base + input.region_offset, bytes) {
        result.status = page_state_status_outside_commit;
        result.blockers |= page_state_blocker_outside_commit;
        return;
    }
    let first = input.region_offset / PAGE;
    sync_dirty(range, first, input.page_count);
    if let Err(error) = apply_operation(range, input, first, result) {
        result.status = if error == TABLE_FULL {
            page_state_status_table_full
        } else {
            page_state_status_invalid_request
        };
        result.blockers |= if error == TABLE_FULL {
            page_state_blocker_table_full
        } else {
            page_state_blocker_invalid_request
        };
        if error == TABLE_FULL {
            ps::with(|s| {
                s.summary.table_full_failures = s.summary.table_full_failures.wrapping_add(1)
            });
        }
        return;
    }
    sync_dirty(range, first, input.page_count);
    ps::with(|s| s.fill_counts(input.region_id, first, input.page_count, result));
    result.status = page_state_status_ready;
}
pub(super) fn apply_page_io(input: PageStateInput, page_out: bool) -> PageStateResult {
    let _held = Held::acquire();
    let mut adjusted = input;
    adjusted.operation = if page_out {
        page_state_operation_bind_slot
    } else {
        page_state_operation_query
    };
    let mut result = page_state_probe(adjusted);
    if page_out && result.status == page_state_status_ready {
        adjusted.operation = page_state_operation_mark_clean;
        result = page_state_probe(adjusted);
        if result.status == page_state_status_ready {
            let returned = make_nonresident(
                adjusted.region_id,
                adjusted.region_offset,
                adjusted.page_count,
            )
            .unwrap_or(0);
            ps::with(|s| {
                s.summary.page_out_nonresident_pages =
                    s.summary.page_out_nonresident_pages.wrapping_add(returned)
            });
            adjusted.operation = page_state_operation_query;
            result = page_state_probe(adjusted);
        }
    }
    result
}
pub(super) fn record_policy_failure(input: PageStateInput, page_out: bool) {
    let _held = Held::acquire();
    if input.page_count == 0 {
        return;
    }
    let Some(index) = ps::index(input.region_id) else {
        return;
    };
    let range = vm_ranges::read(index);
    if !range.active() || range.window != 6 || input.region_offset & (PAGE - 1) != 0 {
        return;
    }
    let first = input.region_offset / PAGE;
    ps::with(|s| {
        if page_out {
            s.summary.pager_failed_page_outs = s.summary.pager_failed_page_outs.wrapping_add(1)
        } else {
            s.summary.pager_failed_page_ins = s.summary.pager_failed_page_ins.wrapping_add(1)
        }
        s.summary.pager_data_preserved_pages = s
            .summary
            .pager_data_preserved_pages
            .wrapping_add(input.page_count);
    });
    let mut dirty_pages = 0u64;
    for offset in 0..input.page_count {
        let page = first + offset;
        if ps::fault(input.region_id, page)
            .is_some_and(|state| state.flags & page_state_flag_dirty != 0)
        {
            dirty_pages = dirty_pages.wrapping_add(1);
            continue;
        }
        let address = range.base + page * PAGE;
        if mapped(address) && dirty(address) {
            dirty_pages = dirty_pages.wrapping_add(1)
        }
    }
    if page_out {
        ps::with(|s| {
            s.summary.pager_dirty_preserved_pages = s
                .summary
                .pager_dirty_preserved_pages
                .wrapping_add(dirty_pages)
        });
    }
}
pub(super) fn commit_covers(id: u32, offset: u64, pages: u64) -> bool {
    let _held = Held::acquire();
    let Ok(index) = lookup(id) else { return false };
    let range = vm_ranges::read(index);
    let Some(bytes) = pages.checked_mul(PAGE) else {
        return false;
    };
    if pages == 0 || offset & (PAGE - 1) != 0 || range.partial_uncommit_len != 0 {
        return false;
    }
    if inside(range, offset, bytes).is_err() {
        return false;
    }
    covered(range, range.base + offset, bytes)
}
fn make_nonresident(id: u32, offset: u64, pages: u64) -> Result<u64, u32> {
    if pages == 0 || offset & (PAGE - 1) != 0 {
        return Err(BAD_ALIGNMENT);
    }
    let bytes = pages.checked_mul(PAGE).ok_or(OVERFLOW)?;
    let index = lookup(id)?;
    let range = vm_ranges::read(index);
    inside(range, offset, bytes)?;
    if !covered(range, range.base + offset, bytes) {
        return Err(NOT_COMMITTED);
    }
    let first = offset / PAGE;
    let result = (|| {
        let mut nonresident = 0u64;
        for i in 0..pages {
            let address = range.base + offset + i * PAGE;
            if !mapped(address) {
                continue;
            }
            uncommit_page(index, address)?;
            nonresident = nonresident.wrapping_add(1);
        }
        ps::set(
            id,
            first,
            pages,
            0,
            page_state_flag_resident | page_state_flag_dirty,
            None,
            true,
        )?;
        Ok(nonresident)
    })();
    blocks::r4os_rust_blocks_coalesce();
    result
}

struct Activity {
    queue: UnsafeCell<sync_types::WaitQueue>,
    sequence: UnsafeCell<u64>,
}
// SAFETY: the native sync/runtime owner admits sequence reads, predicates,
// wake publication and enrollment. Storage is resident for the whole boot.
unsafe impl Sync for Activity {}
static ACTIVITY: Activity = Activity {
    queue: UnsafeCell::new(sync_types::WaitQueue::EMPTY),
    sequence: UnsafeCell::new(0),
};
fn sequence() -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ACTIVITY.queue.get(),
            9,
            ptr::null_mut(),
            0,
            ACTIVITY.sequence.get(),
        )
    }
}
fn wake_activity() {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ACTIVITY.queue.get(),
            8,
            ptr::null_mut(),
            0,
            ACTIVITY.sequence.get(),
        )
    };
}
unsafe extern "C" fn pager_unchanged(raw: *mut c_void) -> bool {
    unsafe { ACTIVITY.sequence.get().read() == raw.cast::<u64>().read() }
}
fn wait_activity(mut observed: u64) -> bool {
    let request = sync_types::WaitRequest {
        timeout: u64::MAX,
        reason: sync_types::Text::literal(b"vm-page-in"),
        predicate: Some(pager_unchanged),
        context: ptr::addr_of_mut!(observed).cast(),
        release: None,
        release_context: ptr::null_mut(),
        flags: 0,
        consume_runtime: false,
    };
    (unsafe { sync_wait::r4os_rust_sync_wait(ACTIVITY.queue.get(), &request) })
        == crate::sched::task::task_types::WaitResult::Signaled as u32
}
unsafe extern "C" {
    fn r4os_zig_vm_file_read(path: *const u8, offset: u64, buffer: *mut u8, bytes: u32) -> i32;
    fn r4os_zig_vm_file_write(path: *const u8, offset: u64, buffer: *const u8, bytes: u32) -> i32;
}
pub(super) fn begin_writeback(
    id: u32,
    offset: u64,
    pages: u64,
    pointer: *const u8,
) -> Result<PageWriteback, u32> {
    let _held = Held::acquire();
    if percpu::current_index() != 0 {
        return Err(BUSY);
    }
    let index = lookup(id)?;
    let range = vm_ranges::read(index);
    let bytes = pages.checked_mul(PAGE).ok_or(OVERFLOW)?;
    if pages == 0 || offset & (PAGE - 1) != 0 {
        return Err(BAD_ALIGNMENT);
    }
    inside(range, offset, bytes)?;
    if range.window != 6 || range.partial_uncommit_len != 0 || range.pager_io_count != 0 {
        return Err(BUSY);
    }
    let start = range.base + offset;
    if pointer.addr() as u64 != start {
        return Err(OUTSIDE_WINDOW);
    }
    if !covered(range, start, bytes) {
        return Err(NOT_COMMITTED);
    }
    let first = offset / PAGE;
    for i in 0..pages {
        let state = ps::with(|s| s.fault_in_range(range, first + i)).ok_or(NOT_COMMITTED)?;
        if state.flags & (page_state_flag_busy | page_state_flag_pinned) != 0 {
            return Err(BUSY);
        }
        if state.flags & page_state_flag_resident == 0 || !mapped(start + i * PAGE) {
            return Err(NOT_COMMITTED);
        }
    }
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        return Err(BUSY);
    }
    if let Err(error) = ps::set(
        id,
        first,
        pages,
        page_state_flag_busy,
        page_state_flag_dirty,
        None,
        true,
    ) {
        unsafe { task_context::leave(unwind) };
        return Err(error);
    }
    for i in 0..pages {
        // A new hardware dirty epoch begins only after its real TLB ACK.
        if !paging::r4os_rust_paging_dirty(start + i * PAGE, true) {
            let _ = ps::set(
                id,
                first,
                pages,
                page_state_flag_dirty,
                page_state_flag_busy,
                None,
                true,
            );
            unsafe { task_context::leave(unwind) };
            return Err(MAP_FAILED);
        }
    }
    vm_ranges::update(index, |range| range.pager_io_count += 1);
    Ok(PageWriteback {
        region_id: id,
        region_offset: offset,
        page_count: pages,
        unwind,
        active: true,
        completed: false,
    })
}
pub(super) fn end_writeback(writeback: &mut PageWriteback) {
    if !writeback.active {
        return;
    }
    let mut held = Held::acquire();
    if let Some(index) = ps::index(writeback.region_id) {
        let range = vm_ranges::read(index);
        let add = if writeback.completed {
            0
        } else {
            page_state_flag_dirty
        };
        let _ = ps::set(
            range.id,
            writeback.region_offset / PAGE,
            writeback.page_count,
            add,
            page_state_flag_busy,
            None,
            true,
        );
        vm_ranges::update(index, |range| {
            assert!(range.pager_io_count != 0);
            range.pager_io_count -= 1;
        });
    }
    writeback.active = false;
    held.release();
    unsafe { task_context::leave(writeback.unwind) };
    wake_activity();
}
pub(super) fn complete_writeback(
    writeback: &mut PageWriteback,
    binding: PageStateInput,
) -> Result<u64, u32> {
    if !writeback.active
        || binding.region_id != writeback.region_id
        || binding.region_offset != writeback.region_offset
        || binding.page_count != writeback.page_count
    {
        return Err(NOT_FOUND);
    }
    let _held = Held::acquire();
    let index = lookup(binding.region_id)?;
    let range = vm_ranges::read(index);
    let first = binding.region_offset / PAGE;
    ps::set(
        range.id,
        first,
        binding.page_count,
        page_state_flag_slot_bound,
        0,
        Some(SlotBinding {
            reservation_id: binding.slot_reservation_id,
            slot_index: binding.slot_index,
            slot_generation: binding.slot_generation,
        }),
        true,
    )?;
    ps::with(|s| s.summary.slot_binds = s.summary.slot_binds.wrapping_add(binding.page_count));
    sync_dirty(range, first, binding.page_count);
    for i in 0..binding.page_count {
        let state = ps::with(|s| s.fault_in_range(range, first + i)).ok_or(NOT_COMMITTED)?;
        if state.flags & page_state_flag_dirty != 0 {
            ps::set(
                range.id,
                first,
                binding.page_count,
                page_state_flag_dirty,
                0,
                None,
                true,
            )?;
            ps::with(|s| {
                s.summary.eviction_redirtied_pages = s
                    .summary
                    .eviction_redirtied_pages
                    .wrapping_add(binding.page_count)
            });
            writeback.completed = true;
            return Ok(0);
        }
    }
    let returned = make_nonresident(range.id, binding.region_offset, binding.page_count)?;
    ps::with(|s| {
        s.summary.page_out_nonresident_pages =
            s.summary.page_out_nonresident_pages.wrapping_add(returned)
    });
    writeback.completed = true;
    Ok(returned)
}
fn page_in_failure(index: usize, page: u64, mark_error: bool) -> bool {
    fault_failure(index);
    ps::with(|s| {
        s.summary.fault_page_in_failures = s.summary.fault_page_in_failures.wrapping_add(1);
        s.summary.pager_failed_page_ins = s.summary.pager_failed_page_ins.wrapping_add(1);
        if mark_error {
            s.summary.pager_data_preserved_pages =
                s.summary.pager_data_preserved_pages.wrapping_add(1)
        }
    });
    let id = vm_ranges::read(index).id;
    let add = if mark_error { page_state_flag_error } else { 0 };
    let _ = ps::set(id, page, 1, add, page_state_flag_busy, None, true);
    false
}
fn page_in_fault(index: usize, address: u64, page: u64, token: owner::Token) -> bool {
    // Exact VM admission is handed over without an unlock gap. Only copied
    // values and the pinned index survive the later deliberate release.
    let mut held = Held::from(token);
    if mapped(address) {
        return true;
    }
    let range = vm_ranges::read(index);
    if !covered(range, address, PAGE) || range.partial_uncommit_len != 0 {
        return false;
    }
    let Some(state) = ps::with(|s| s.fault_in_range(range, page)) else {
        return false;
    };
    if state.slot_reservation_id == 0
        || state.flags & (page_state_flag_busy | page_state_flag_pinned | page_state_flag_error)
            != 0
        || range.owner_id > u64::from(u32::MAX)
    {
        return page_in_failure(index, page, false);
    }
    let Some(backing) = backing_result() else {
        return page_in_failure(index, page, true);
    };
    let active_path = unsafe { bs::r4os_rust_backing_active_path() };
    if active_path.is_null() {
        return page_in_failure(index, page, true);
    }
    let mut path = [0u8; backing::MAX_BACKING_PATH + 1];
    let mut len = 0;
    while unsafe { active_path.add(len).read() } != 0 {
        len += 1
    }
    if len >= backing::MAX_BACKING_PATH {
        return page_in_failure(index, page, true);
    }
    unsafe { ptr::copy_nonoverlapping(active_path, path.as_mut_ptr(), len) };
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        return page_in_failure(index, page, false);
    }
    if ps::set(range.id, page, 1, page_state_flag_busy, 0, None, true).is_err() {
        let result = page_in_failure(index, page, true);
        unsafe { task_context::leave(unwind) };
        return result;
    }
    vm_ranges::update(index, |range| range.pager_io_count += 1);
    let mut private_frame = None;
    let mut published = false;
    let result = (|| {
        let input = backing::PageIoInput {
            operation: backing::page_io_operation_page_in,
            region_id: range.id,
            region_offset: address - range.base,
            reservation_id: state.slot_reservation_id,
            slot_index: state.slot_index,
            page_count: 1,
            owner_kind: backing::slot_owner_kind_vm_region,
            owner_id: u32::try_from(range.owner_id).expect("admitted VM owner"),
            expected_generation: state.slot_generation,
            vm_region_exists: true,
            vm_region_is_r4x: true,
            commit_covered: true,
            committed_bytes: range.committed_bytes,
            resident_bytes: range.resident_bytes,
            backing,
            ..backing::PageIoInput::INIT
        };
        let prepared = io_prepare(input);
        if prepared.status != backing::page_io_status_ready {
            return page_in_failure(index, page, true);
        }
        held.release();
        let allocated = alloc_claimed(range, 3);
        held.reacquire();
        let frame = match allocated {
            Ok(frame) => frame,
            Err(_) => return page_in_failure(index, page, false),
        };
        private_frame = Some(frame);
        let memory =
            ptr::with_exposed_provenance_mut::<u8>(phys::r4os_rust_phys_to_virt(frame) as usize);
        held.release();
        let io_status = unsafe {
            r4os_zig_vm_file_read(
                path.as_ptr(),
                prepared.backing_offset,
                memory,
                u32::try_from(prepared.transfer_bytes).expect("validated page transfer"),
            )
        };
        held.reacquire();
        let completed = io_complete(backing::PageIoInput {
            io_status,
            io_bytes: if io_status > 0 { io_status as u32 } else { 0 },
            ..input
        });
        if completed.status != backing::page_io_status_page_in_ok {
            return page_in_failure(index, page, true);
        }
        if !paging::r4os_rust_paging_map(address, frame, vm_ranges::read(index).flags, false) {
            return page_in_failure(index, page, false);
        }
        published = true;
        fault_success(index);
        ps::with(|s| s.summary.fault_page_ins = s.summary.fault_page_ins.wrapping_add(1));
        let _ = ps::set(
            range.id,
            page,
            1,
            page_state_flag_resident,
            page_state_flag_dirty | page_state_flag_busy | page_state_flag_error,
            None,
            true,
        );
        true
    })();
    if let Some(frame) = private_frame
        && !published
    {
        release_claimed(frame)
    }
    // The live pager count and exact unwind anchor pin this range throughout
    // I/O. Clear the metadata before waking, without entering runtime under VM.
    let _ = ps::set(range.id, page, 1, 0, page_state_flag_busy, None, true);
    vm_ranges::update(index, |range| range.pager_io_count -= 1);
    held.release();
    wake_activity();
    held.reacquire();
    unsafe { task_context::leave(unwind) };
    result
}
pub(super) fn handle_demand_fault(address: u64, error_code: u64, owner: u8, owner_id: u64) -> bool {
    let mut waited = None;
    loop {
        let observed = sequence();
        let mut held = Held::acquire();
        if !ready() || error_code & 1 != 0 {
            return false;
        }
        let address = address & !(PAGE - 1);
        let index = query(4, address);
        if index == usize::MAX {
            return false;
        }
        let range = vm_ranges::read(index);
        if range.window != 6 || range.owner != owner || range.owner_id != owner_id {
            return false;
        }
        if range.kind != 4 || range.partial_uncommit_len != 0 {
            return false;
        }
        if waited.is_some_and(|id| id != range.id) {
            return false;
        }
        if range.guard_len != 0
            && virt_policy::r4os_rust_vm_overlap(address, PAGE, range.guard_base, range.guard_len)
        {
            fault_failure(index);
            return false;
        }
        if !covered(range, address, PAGE) {
            fault_failure(index);
            return false;
        }
        let page = (address - range.base) / PAGE;
        if let Some(state) = ps::with(|s| s.fault_in_range(range, page)) {
            if state.flags & page_state_flag_busy != 0
                && range.pager_io_count != 0
                && !mapped(address)
            {
                if percpu::current_index() != 0 {
                    return false;
                }
                waited = Some(range.id);
                held.release();
                if !wait_activity(observed) {
                    return false;
                }
                continue;
            }
            if mapped(address) {
                return waited.is_some();
            }
            if !can_commit(range) {
                fault_failure(index);
                return false;
            }
            if state.flags & page_state_flag_slot_bound != 0 {
                if percpu::current_index() != 0 {
                    fault_failure(index);
                    return false;
                }
                return page_in_fault(index, address, page, held.take());
            }
        }
        if mapped(address) {
            return waited.is_some();
        }
        if !can_commit(range) {
            fault_failure(index);
            return false;
        }
        let Ok(frame) = alloc_claimed(range, 3) else {
            fault_failure(index);
            return false;
        };
        let memory =
            ptr::with_exposed_provenance_mut::<u8>(phys::r4os_rust_phys_to_virt(frame) as usize);
        unsafe { ptr::write_bytes(memory, 0, PAGE as usize) };
        if !paging::r4os_rust_paging_map(address, frame, range.flags, false) {
            release_claimed(frame);
            fault_failure(index);
            return false;
        }
        fault_success(index);
        let _ = ps::set(
            range.id,
            page,
            1,
            page_state_flag_resident,
            page_state_flag_dirty,
            None,
            true,
        );
        return true;
    }
}

fn pager_page_out_failure(dirty: bool) {
    ps::with(|s| {
        s.summary.pager_failed_page_outs = s.summary.pager_failed_page_outs.wrapping_add(1);
        s.summary.pager_data_preserved_pages = s.summary.pager_data_preserved_pages.wrapping_add(1);
        if dirty {
            s.summary.pager_dirty_preserved_pages =
                s.summary.pager_dirty_preserved_pages.wrapping_add(1)
        }
    });
}
fn clear_eviction_busy(id: u32, page: u64) {
    let _ = ps::set(id, page, 1, 0, page_state_flag_busy, None, true);
}
fn mark_eviction_error(id: u32, page: u64) {
    let _ = ps::set(
        id,
        page,
        1,
        page_state_flag_error,
        page_state_flag_busy,
        None,
        true,
    );
}
fn release_eviction_slot(backing: backing::Result, reservation: u32, owner: u32, id: u32) {
    slot_probe(backing::SlotInput {
        operation: backing::slot_operation_release,
        reservation_id: reservation,
        owner_kind: backing::slot_owner_kind_vm_region,
        owner_id: owner,
        region_id: id,
        backing,
        ..backing::SlotInput::INIT
    });
}
fn evict_clean_bound(
    index: usize,
    page: u64,
    state: FaultPageState,
    backing: backing::Result,
) -> EvictionStep {
    let range = vm_ranges::read(index);
    let owner_id = u32::try_from(range.owner_id).expect("admitted VM owner");
    let input = backing::PageIoInput {
        operation: backing::page_io_operation_page_in,
        region_id: range.id,
        region_offset: page * PAGE,
        reservation_id: state.slot_reservation_id,
        slot_index: state.slot_index,
        page_count: 1,
        owner_kind: backing::slot_owner_kind_vm_region,
        owner_id,
        expected_generation: state.slot_generation,
        vm_region_exists: true,
        vm_region_is_r4x: true,
        commit_covered: covered(range, range.base + page * PAGE, PAGE),
        committed_bytes: range.committed_bytes,
        resident_bytes: range.resident_bytes,
        backing,
        ..backing::PageIoInput::INIT
    };
    if io_prepare(input).status != backing::page_io_status_ready {
        clear_eviction_busy(range.id, page);
        ps::with(|s| {
            s.summary.eviction_slot_failures = s.summary.eviction_slot_failures.wrapping_add(1)
        });
        return EvictionStep {
            outcome: EvictionOutcome::Failed,
            ..EvictionStep::EMPTY
        };
    }
    let before = free_frames();
    match make_nonresident(range.id, page * PAGE, 1) {
        Ok(returned) => ps::with(|s| {
            s.summary.page_out_nonresident_pages =
                s.summary.page_out_nonresident_pages.wrapping_add(returned)
        }),
        Err(_) => {
            clear_eviction_busy(range.id, page);
            ps::with(|s| s.summary.eviction_failures = s.summary.eviction_failures.wrapping_add(1));
            return EvictionStep {
                outcome: EvictionOutcome::Failed,
                ..EvictionStep::EMPTY
            };
        }
    }
    clear_eviction_busy(range.id, page);
    let after = free_frames();
    let returned = u32::try_from(after.saturating_sub(before)).expect("physical frame delta");
    if returned == 0 {
        ps::with(|s| s.summary.eviction_failures = s.summary.eviction_failures.wrapping_add(1));
        return EvictionStep {
            outcome: EvictionOutcome::Failed,
            ..EvictionStep::EMPTY
        };
    }
    EvictionStep {
        outcome: EvictionOutcome::Returned,
        returned_frames: returned,
        returned_bytes: u64::from(returned) * PAGE,
        ..EvictionStep::EMPTY
    }
}
fn evict_resident(id: u32, page: u64, backing: backing::Result, path: *const u8) -> EvictionStep {
    let mut held = Held::acquire();
    let Ok(index) = lookup(id) else {
        return EvictionStep::EMPTY;
    };
    let range = vm_ranges::read(index);
    if range.pager_io_count != 0 || range.partial_uncommit_len != 0 {
        return EvictionStep::EMPTY;
    }
    let Some(state) = ps::with(|s| s.fault_in_range(range, page)) else {
        return EvictionStep::EMPTY;
    };
    if state.flags & (page_state_flag_busy | page_state_flag_pinned | page_state_flag_error) != 0 {
        return EvictionStep::EMPTY;
    }
    let address = range.base + page * PAGE;
    let dirty = state.flags & page_state_flag_dirty != 0 || dirty(address);
    if state.flags & page_state_flag_slot_bound != 0 && !dirty {
        return evict_clean_bound(index, page, state, backing);
    }
    let pointer = ptr::with_exposed_provenance::<u8>(address as usize);
    let Ok(mut writeback) = begin_writeback(id, page * PAGE, 1, pointer) else {
        return EvictionStep {
            outcome: EvictionOutcome::Failed,
            dirty,
            ..EvictionStep::EMPTY
        };
    };
    // Its exact I/O count and unwind anchor retain mappings after releasing
    // the original outer owner. EndWriteback must run after its final release.
    held.release();
    held.reacquire();
    let failed = EvictionStep {
        outcome: EvictionOutcome::Failed,
        dirty,
        ..EvictionStep::EMPTY
    };
    let result = (|| {
        let mut owned_path = [0u8; backing::MAX_BACKING_PATH + 1];
        let mut len = 0;
        while unsafe { path.add(len).read() } != 0 {
            len += 1
        }
        if len >= backing::MAX_BACKING_PATH {
            return failed;
        }
        unsafe { ptr::copy_nonoverlapping(path, owned_path.as_mut_ptr(), len) };
        let owner_id = u32::try_from(vm_ranges::read(index).owner_id).expect("admitted VM owner");
        let mut reservation = state.slot_reservation_id;
        let mut slot_index = state.slot_index;
        let mut generation = state.slot_generation;
        let mut reserved_new = false;
        if state.flags & page_state_flag_slot_bound == 0 {
            let reserved = slot_probe(backing::SlotInput {
                operation: backing::slot_operation_reserve,
                requested_slots: 1,
                owner_kind: backing::slot_owner_kind_vm_region,
                owner_id,
                region_id: id,
                backing,
                ..backing::SlotInput::INIT
            });
            if reserved.status != backing::slot_status_reserved {
                ps::with(|s| {
                    s.summary.eviction_slot_failures =
                        s.summary.eviction_slot_failures.wrapping_add(1)
                });
                pager_page_out_failure(dirty);
                return failed;
            }
            reservation = reserved.reservation_id;
            slot_index = 0;
            generation = reserved.generation;
            reserved_new = true;
        }
        let live = vm_ranges::read(index);
        let input = backing::PageIoInput {
            operation: backing::page_io_operation_page_out,
            region_id: id,
            region_offset: writeback.region_offset,
            reservation_id: reservation,
            slot_index,
            page_count: 1,
            owner_kind: backing::slot_owner_kind_vm_region,
            owner_id,
            expected_generation: generation,
            flags: backing::page_io_flag_eviction_request,
            vm_region_exists: true,
            vm_region_is_r4x: true,
            commit_covered: true,
            committed_bytes: live.committed_bytes,
            resident_bytes: live.resident_bytes,
            backing,
            ..backing::PageIoInput::INIT
        };
        let prepared = io_prepare(input);
        if prepared.status != backing::page_io_status_ready {
            if reserved_new {
                release_eviction_slot(backing, reservation, owner_id, id)
            }
            ps::with(|s| {
                s.summary.eviction_slot_failures = s.summary.eviction_slot_failures.wrapping_add(1)
            });
            pager_page_out_failure(dirty);
            return failed;
        }
        held.release();
        let io_status = unsafe {
            r4os_zig_vm_file_write(
                owned_path.as_ptr(),
                prepared.backing_offset,
                pointer,
                u32::try_from(prepared.transfer_bytes).expect("validated page transfer"),
            )
        };
        held.reacquire();
        let completed = io_complete(backing::PageIoInput {
            io_status,
            io_bytes: if io_status > 0 { io_status as u32 } else { 0 },
            ..input
        });
        if completed.status != backing::page_io_status_page_out_ok {
            if reserved_new {
                release_eviction_slot(backing, reservation, owner_id, id)
            }
            mark_eviction_error(id, page);
            ps::with(|s| {
                s.summary.eviction_io_failures = s.summary.eviction_io_failures.wrapping_add(1)
            });
            pager_page_out_failure(dirty);
            return failed;
        }
        let returned = match complete_writeback(
            &mut writeback,
            PageStateInput {
                region_id: id,
                region_offset: completed.region_offset,
                page_count: 1,
                slot_reservation_id: completed.reservation_id,
                slot_index: completed.slot_index,
                slot_generation: completed.slot_generation,
                ..PageStateInput::INIT
            },
        ) {
            Ok(returned) => returned,
            Err(_) => {
                mark_eviction_error(id, page);
                pager_page_out_failure(true);
                return failed;
            }
        };
        ps::with(|s| s.summary.eviction_page_outs = s.summary.eviction_page_outs.wrapping_add(1));
        EvictionStep {
            outcome: if returned == 0 {
                EvictionOutcome::Skipped
            } else {
                EvictionOutcome::Returned
            },
            returned_frames: u32::try_from(returned).expect("returned pages"),
            returned_bytes: returned * PAGE,
            page_outs: 1,
            dirty,
        }
    })();
    held.release();
    end_writeback(&mut writeback);
    result
}
fn set_cursor_after_page(position: usize, next_page: u64) {
    let slot = ordered(position);
    let pages = vm_ranges::read(slot).len / PAGE;
    if next_page < pages {
        ps::with(|s| {
            s.cursor_slot = Some(slot as u16);
            s.cursor_page = next_page;
        });
        return;
    }
    let mut next = position + 1;
    if next == count() {
        next = 0;
        ps::with(|s| s.hot.reclaim_cursor_wraps = s.hot.reclaim_cursor_wraps.wrapping_add(1));
    }
    ps::with(|s| {
        s.cursor_slot = Some(ordered(next) as u16);
        s.cursor_page = 0;
    });
}
fn scan_span(range_slot: usize, position: usize, slot: u16, page_floor: u64) -> Option<u64> {
    ps::with(|s| s.hot.reclaim_cursor_span_steps = s.hot.reclaim_cursor_span_steps.wrapping_add(1));
    let span = ps::with(|s| s.spans[usize::from(slot)]);
    if span.flags & page_state_flag_committed == 0 {
        return None;
    }
    let end = span.first_page.checked_add(span.page_count)?;
    if end <= page_floor {
        return None;
    }
    let range = vm_ranges::read(range_slot);
    for page in span.first_page.max(page_floor)..end {
        ps::with(|s| {
            s.hot.reclaim_cursor_page_steps = s.hot.reclaim_cursor_page_steps.wrapping_add(1)
        });
        set_cursor_after_page(position, page + 1);
        let address = range.base + page * PAGE;
        if range.guard_len != 0
            && virt_policy::r4os_rust_vm_overlap(address, PAGE, range.guard_base, range.guard_len)
        {
            ps::with(|s| {
                s.summary.pager_disabled_eviction_gates =
                    s.summary.pager_disabled_eviction_gates.wrapping_add(1)
            });
            continue;
        }
        if span.flags & page_state_flag_resident == 0 {
            ps::with(|s| {
                s.summary.eviction_skipped_nonresident =
                    s.summary.eviction_skipped_nonresident.wrapping_add(1)
            });
            continue;
        }
        if span.flags & page_state_flag_pinned != 0 {
            ps::with(|s| {
                s.summary.eviction_skipped_pinned =
                    s.summary.eviction_skipped_pinned.wrapping_add(1);
                s.summary.pager_disabled_eviction_gates =
                    s.summary.pager_disabled_eviction_gates.wrapping_add(1);
            });
            continue;
        }
        if span.flags & page_state_flag_busy != 0 {
            ps::with(|s| {
                s.summary.eviction_skipped_busy = s.summary.eviction_skipped_busy.wrapping_add(1);
                s.summary.pager_disabled_eviction_gates =
                    s.summary.pager_disabled_eviction_gates.wrapping_add(1);
            });
            continue;
        }
        if span.flags & page_state_flag_error != 0 {
            ps::with(|s| {
                s.summary.eviction_skipped_error = s.summary.eviction_skipped_error.wrapping_add(1);
                s.summary.pager_disabled_eviction_gates =
                    s.summary.pager_disabled_eviction_gates.wrapping_add(1);
            });
            continue;
        }
        if !mapped(address) {
            ps::with(|s| {
                s.summary.eviction_skipped_unmapped =
                    s.summary.eviction_skipped_unmapped.wrapping_add(1)
            });
            continue;
        }
        sync_dirty(vm_ranges::read(range_slot), page, 1);
        let Some(state) = ps::with(|s| s.fault_in_range(vm_ranges::read(range_slot), page)) else {
            ps::with(|s| {
                s.summary.eviction_skipped_nonresident =
                    s.summary.eviction_skipped_nonresident.wrapping_add(1)
            });
            continue;
        };
        if state.flags & page_state_flag_resident == 0 {
            ps::with(|s| {
                s.summary.eviction_skipped_nonresident =
                    s.summary.eviction_skipped_nonresident.wrapping_add(1)
            });
            continue;
        }
        if state.flags & (page_state_flag_pinned | page_state_flag_busy | page_state_flag_error)
            != 0
        {
            ps::with(|s| {
                if state.flags & page_state_flag_pinned != 0 {
                    s.summary.eviction_skipped_pinned =
                        s.summary.eviction_skipped_pinned.wrapping_add(1)
                }
                if state.flags & page_state_flag_busy != 0 {
                    s.summary.eviction_skipped_busy =
                        s.summary.eviction_skipped_busy.wrapping_add(1)
                }
                if state.flags & page_state_flag_error != 0 {
                    s.summary.eviction_skipped_error =
                        s.summary.eviction_skipped_error.wrapping_add(1)
                }
                s.summary.pager_disabled_eviction_gates =
                    s.summary.pager_disabled_eviction_gates.wrapping_add(1);
            });
            continue;
        }
        ps::with(|s| s.summary.eviction_candidates = s.summary.eviction_candidates.wrapping_add(1));
        return Some(page);
    }
    None
}
fn evict_one(backing: backing::Result, path: *const u8, owner: Option<u64>) -> EvictionStep {
    let mut held = Held::acquire();
    if count() == 0 {
        return EvictionStep::EMPTY;
    }
    let mut start_position = 0;
    let mut start_page = 0;
    if let Some(slot) = ps::with(|s| s.cursor_slot)
        && let Some(position) = vm_index::position(usize::from(slot))
        && position < count()
    {
        start_position = position;
        start_page = ps::with(|s| s.cursor_page);
    }
    let mut visited = 0;
    let mut position = start_position;
    while visited < count() {
        let range_slot = ordered(position);
        let range = vm_ranges::read(range_slot);
        ps::with(|s| {
            s.hot.reclaim_cursor_range_steps = s.hot.reclaim_cursor_range_steps.wrapping_add(1)
        });
        let page_floor = if visited == 0 { start_page } else { 0 };
        if range.active()
            && range.pager_io_count == 0
            && range.partial_uncommit_len == 0
            && range.window == 6
            && range.owner == 3
            && range.owner_id != 0
            && range.owner_id <= u64::from(u32::MAX)
            && owner.is_none_or(|id| range.owner_id == id)
        {
            let mut cursor = range.page_state_span_head.get();
            while let Some(slot) = cursor {
                if let Some(page) = scan_span(range_slot, position, slot, page_floor) {
                    held.release();
                    return evict_resident(range.id, page, backing, path);
                }
                // Dirty synchronization may split/coalesce the original span.
                // Use its current successor at the same point as the old loop.
                cursor = ps::with(|s| s.spans[usize::from(slot)].next);
            }
        }
        position += 1;
        if position == count() {
            position = 0;
            ps::with(|s| s.hot.reclaim_cursor_wraps = s.hot.reclaim_cursor_wraps.wrapping_add(1));
        }
        ps::with(|s| {
            s.cursor_slot = Some(ordered(position) as u16);
            s.cursor_page = 0;
        });
        visited += 1;
    }
    EvictionStep::EMPTY
}
// Reclaim updates below occur outside I/O and take only bounded VM admission.
fn note(operation: impl FnOnce(&mut PageStateSummary)) {
    let _held = Held::acquire();
    ps::with(|s| operation(&mut s.summary));
}
pub(super) fn reclaim_evict(
    _reason: u32,
    raw_requested: u32,
    owner: Option<u64>,
) -> reclaim::SourceResult {
    let requested = if raw_requested == 0 { 1 } else { raw_requested };
    let mut result = reclaim::SourceResult::INIT;
    if !ready() {
        return result;
    }
    if percpu::current_index() != 0 {
        result.failures = 1;
        return result;
    }
    note(|s| s.eviction_attempts = s.eviction_attempts.wrapping_add(1));
    let selected = {
        let _held = Held::acquire();
        backing_result()
    };
    let Some(backing) = selected else {
        note(|s| {
            s.eviction_no_backing = s.eviction_no_backing.wrapping_add(1);
            s.eviction_failures = s.eviction_failures.wrapping_add(1);
            s.pager_disabled_eviction_gates = s.pager_disabled_eviction_gates.wrapping_add(1);
        });
        result.failures = 1;
        return result;
    };
    let path = backing.path.as_ptr();
    while result.returned_frames < requested {
        let step = evict_one(backing, path, owner);
        match step.outcome {
            EvictionOutcome::Returned => {
                result.returned_frames =
                    result.returned_frames.saturating_add(step.returned_frames);
                result.returned_bytes = result.returned_bytes.wrapping_add(step.returned_bytes);
                result.page_outs = result.page_outs.wrapping_add(step.page_outs);
                note(|s| {
                    if step.dirty {
                        s.eviction_dirty_pages = s
                            .eviction_dirty_pages
                            .wrapping_add(u64::from(step.returned_frames))
                    } else {
                        s.eviction_clean_pages = s
                            .eviction_clean_pages
                            .wrapping_add(u64::from(step.returned_frames))
                    }
                    s.eviction_successes = s.eviction_successes.wrapping_add(1);
                    s.eviction_returned_frames = s
                        .eviction_returned_frames
                        .wrapping_add(u64::from(step.returned_frames));
                });
            }
            EvictionOutcome::Skipped => {
                note(|s| s.eviction_no_candidate = s.eviction_no_candidate.wrapping_add(1));
                break;
            }
            EvictionOutcome::Failed => {
                note(|s| s.eviction_failures = s.eviction_failures.wrapping_add(1));
                result.failures = result.failures.wrapping_add(1);
                break;
            }
        }
    }
    if result.returned_frames == 0 && result.failures == 0 {
        note(|s| s.eviction_failures = s.eviction_failures.wrapping_add(1));
        result.failures = 1;
    }
    result
}
pub(super) fn evictable_bytes(dirty_only: bool) -> u64 {
    let _held = Held::acquire();
    if !ready() || backing_result().is_none() {
        return 0;
    }
    let mut bytes = 0u64;
    let mut position = 0;
    while position < count() {
        let range = vm_ranges::read(ordered(position));
        let mut cursor = range.page_state_span_head.get();
        while let Some(slot) = cursor {
            let span = ps::with(|s| s.spans[usize::from(slot)]);
            let flags = page_state_flag_committed
                | page_state_flag_resident
                | if dirty_only { page_state_flag_dirty } else { 0 };
            if span.flags & flags == flags
                && span.flags
                    & (page_state_flag_pinned | page_state_flag_busy | page_state_flag_error)
                    == 0
            {
                bytes = bytes.wrapping_add(span.page_count * PAGE);
            }
            cursor = span.next;
        }
        position += 1;
    }
    bytes
}

fn commit_change(
    operation: u8,
    id: u32,
    base: u64,
    bytes: u64,
    reserved: *mut OptionalSlot,
) -> Result<(), u32> {
    check(unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_change(operation, id, base, bytes, reserved)
    })
}
fn release_commit_ticket(ticket: OptionalSlot) {
    if let Some(slot) = ticket.get() {
        let _ = commit_change(4, 0, u64::from(slot), 0, ptr::null_mut());
    }
}
fn remove_commit(id: u32, base: u64, bytes: u64) -> Result<(), u32> {
    let mut ticket = OptionalSlot::NONE;
    commit_change(3, id, base, bytes, &mut ticket)?;
    let result = commit_change(1, id, base, bytes, &mut ticket);
    release_commit_ticket(ticket);
    result
}
fn code(result: Result<(), u32>) -> u32 {
    match result {
        Ok(()) => 0,
        Err(code) => code,
    }
}
// SAFETY: these leaves receive the same exact resident range lease while the
// caller retains VM admission. No range reference spans a subordinate owner.
pub(super) unsafe fn demand_commit(pointer: *mut Range, offset: u64, bytes: u64) -> u32 {
    code((|| {
        let range = unsafe { pointer.read() };
        let base = range.base + offset;
        if range.window == 6 {
            // Original by-ID validation performs its own counted ID lookup.
            let index = lookup(range.id)?;
            if overlaps(vm_ranges::read(index), base, bytes) {
                return Err(9);
            }
        } else {
            let mut at = 0;
            while at < bytes {
                if mapped(base + at) {
                    return Err(9);
                }
                at += PAGE;
            }
        }
        let next = range.committed_bytes.checked_add(bytes).ok_or(OVERFLOW)?;
        commit_change(0, range.id, base, bytes, ptr::null_mut())?;
        let flags = page_state_flag_committed
            | if range.eviction_allowed {
                0
            } else {
                page_state_flag_pinned
            };
        if let Err(error) = ps::with(|s| {
            s.add(
                range.id,
                offset / PAGE,
                bytes / PAGE,
                flags,
                SlotBinding::INIT,
            )
        }) {
            let _ = remove_commit(range.id, base, bytes);
            return Err(error);
        }
        unsafe {
            ptr::addr_of_mut!((*pointer).committed_bytes).write(next);
            ptr::addr_of_mut!((*pointer).status).write(2);
        }
        let status =
            unsafe { blocks::r4os_rust_blocks_change(range.block_id, 1, next, ptr::null()) };
        if status != 0 {
            return Err(vm_frames::block_error(status));
        }
        Ok(())
    })())
}
pub(super) unsafe fn resident_page_state(id: u32, first: u64, pages: u64, mark: bool) -> u32 {
    code(if mark {
        ps::set(id, first, pages, page_state_flag_resident, 0, None, true)
    } else {
        ps::with(|s| {
            s.add(
                id,
                first,
                pages,
                page_state_flag_committed | page_state_flag_pinned,
                SlotBinding::INIT,
            )
        })
    })
}
pub(super) unsafe fn uncommit_demand(
    pointer: *mut Range,
    start: u64,
    bytes: u64,
    skip: bool,
) -> u32 {
    code((|| {
        let range = unsafe { pointer.read() };
        if !skip && !covered(range, start, bytes) {
            return Err(NOT_COMMITTED);
        }
        let end = start.checked_add(bytes).ok_or(OVERFLOW)?;
        let mut commit_ticket = OptionalSlot::NONE;
        commit_change(3, range.id, start, bytes, &mut commit_ticket)?;
        let first_page = (start - range.base) / PAGE;
        let end_page = first_page + bytes / PAGE;
        let result = (|| {
            let mut splits = ps::with(|s| s.reserve_splits(range, first_page, end_page))?;
            let result = (|| {
                let mut address = if range.partial_uncommit_cursor >= start
                    && range.partial_uncommit_cursor <= end
                {
                    range.partial_uncommit_cursor
                } else {
                    start
                };
                let mut committed = range.commit_span_head.get();
                while address < end {
                    while let Some(slot) = committed {
                        let span = vm_commit_spans::read(usize::from(slot));
                        if span.base + span.len > address {
                            break;
                        }
                        committed = span.next.get();
                    }
                    let Some(slot) = committed else { break };
                    let span = vm_commit_spans::read(usize::from(slot));
                    address = address.max(span.base);
                    if address >= end {
                        break;
                    }
                    let span_end = end.min(span.base + span.len);
                    let Some(physical) = frame(address) else {
                        address += PAGE;
                        unsafe {
                            vm_transactions::r4os_rust_vm_partial(pointer, 1, address, 0, false)
                        };
                        continue;
                    };
                    let max_pages = ((span_end - address) / PAGE).min(page_batch::MAX_EXTENT_PAGES);
                    let claimed =
                        blocks::r4os_rust_blocks_prefix(physical, max_pages * PAGE, false) / PAGE;
                    if claimed == 0 {
                        return Err(NOT_COMMITTED);
                    }
                    let mut run = 1;
                    while run < claimed {
                        let Some(next) = frame(address + run * PAGE) else {
                            break;
                        };
                        if next != physical + run * PAGE {
                            break;
                        }
                        run += 1;
                    }
                    check(unsafe {
                        vm_frames::r4os_rust_vm_uncommit_extent(
                            pointer, address, physical, run, false, false,
                        )
                    })?;
                    address += run * PAGE;
                    unsafe { vm_transactions::r4os_rust_vm_partial(pointer, 1, address, 0, false) };
                }
                commit_change(1, range.id, start, bytes, &mut commit_ticket)?;
                let current = unsafe { pointer.read() };
                ps::with(|s| s.remove_reserved(current, first_page, end_page, &mut splits))?;
                let _publication = Held::acquire();
                unsafe {
                    if !(*pointer).partial_uncommit_accounted {
                        if skip && bytes > (*pointer).committed_bytes {
                            (*pointer).committed_bytes = 0;
                        } else {
                            (*pointer).committed_bytes =
                                (*pointer).committed_bytes.saturating_sub(bytes);
                        }
                        (*pointer).partial_uncommit_accounted = true;
                    }
                }
                Ok(())
            })();
            ps::with(|s| s.release_reserved(&mut splits));
            result
        })();
        release_commit_ticket(commit_ticket);
        result
    })())
}
fn remove_address(slot: usize) {
    let Some(position) = vm_index::position(slot) else {
        return;
    };
    if position >= count() || ordered(position) != slot {
        return;
    }
    if ps::with(|s| s.cursor_slot) == Some(slot as u16) {
        if count() > 1 {
            let next = if position + 1 < count() {
                position + 1
            } else {
                0
            };
            ps::with(|s| {
                s.cursor_slot = Some(ordered(next) as u16);
                s.cursor_page = 0;
            });
        } else {
            ps::with(|s| {
                s.cursor_slot = None;
                s.cursor_page = 0;
            });
        }
    }
    unsafe { vm_index::r4os_rust_vm_index_change(3, 0, slot) };
}
pub(super) unsafe fn pager_cleanup(pointer: *mut Range, operation: u8, slot: usize) {
    let range = unsafe { pointer.read() };
    match operation {
        0 => {
            let owner = u32::try_from(range.owner_id).expect("admitted VM owner");
            let mut result = MaybeUninit::uninit();
            unsafe {
                bs::r4os_rust_backing_release_vm_region(owner, range.id, result.as_mut_ptr())
            };
        }
        1 => ps::with(|s| s.remove_for_range(range.id)),
        2 => remove_address(slot),
        _ => unreachable!(),
    }
}
unsafe extern "C" {
    fn r4os_zig_vm_reclaimer_address() -> usize;
}
pub(super) unsafe fn init() -> bool {
    unsafe { vm_ranges::r4os_rust_vm_ranges_init() };
    let mut order = ptr::null();
    let mut positions = ptr::null();
    unsafe {
        vm_index::r4os_rust_vm_index_init(&mut order, &mut positions);
        vm_commit_spans::r4os_rust_vm_commit_spans_init();
    }
    ps::with(|s| s.reset());
    unsafe { vm_transactions::r4os_rust_vm_publish_ready() };
    reclaim::r4os_rust_reclaim_register_vm(unsafe { r4os_zig_vm_reclaimer_address() });
    true
}
