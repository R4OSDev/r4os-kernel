//! Resident VM transactions. Exact range leases survive owner handoffs;
//! no Rust table/range reference survives zeroing, reschedule or pager calls.
use super::{
    blocks,
    heap::Unwind,
    layout::PAGE_SIZE as PAGE,
    owner::{self, Guard},
    page_batch::MAX_EXTENT_PAGES as BATCH,
    paging, phys, virt_policy, vm_commit_spans, vm_frames, vm_index,
    vm_ranges::{self, Range},
};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
static READY: AtomicBool = AtomicBool::new(false);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub resident_commit_batches: u64,
    pub resident_commit_max_batch_pages: u64,
    pub resident_commit_unlocked_zero_bytes: u64,
    pub resident_rollback_batches: u64,
}
impl Stats {
    const EMPTY: Self = Self {
        resident_commit_batches: 0,
        resident_commit_max_batch_pages: 0,
        resident_commit_unlocked_zero_bytes: 0,
        resident_rollback_batches: 0,
    };
}
struct Counters(UnsafeCell<Stats>);
// SAFETY: singleton BSP initializes; VM owner admits every later counter access.
unsafe impl Sync for Counters {}
static COUNTERS: Counters = Counters(UnsafeCell::new(Stats::EMPTY));
fn counters(operation: impl FnOnce(&mut Stats)) {
    operation(unsafe { &mut *COUNTERS.0.get() });
}
unsafe extern "C" {
    fn r4os_zig_heap_unwind_enter(output: *mut Unwind);
    fn r4os_zig_heap_unwind_leave(token: *const Unwind) -> bool;
    fn r4os_zig_vm_reschedule();
    fn r4os_zig_vm_demand_commit(range: *mut Range, offset: u64, len: u64) -> u32;
    fn r4os_zig_vm_uncommit_demand(range: *mut Range, base: u64, len: u64, skip: bool) -> u32;
    fn r4os_zig_vm_pager_cleanup(range: *mut Range, operation: u8, slot: usize);
    fn r4os_zig_vm_resident_page_state(id: u32, first_page: u64, pages: u64, mark: bool) -> u32;
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_initialized() -> bool {
    READY.load(Ordering::Acquire)
}
/// # Safety
/// Singleton BSP completes the range/index/span and legacy pager initialization.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_publish_ready() {
    counters(|stats| *stats = Stats::EMPTY);
    READY.store(true, Ordering::Release);
}
fn lookup(id: u32) -> Result<usize, u32> {
    let slot = unsafe { vm_index::r4os_rust_vm_index_query(1, u64::from(id)) };
    if slot == usize::MAX { Err(8) } else { Ok(slot) }
}
fn normalized(value: u64) -> Result<u64, u32> {
    let mut len = 0;
    let code = unsafe { virt_policy::r4os_rust_vm_normalize(value, false, &mut len) };
    check(code)?;
    Ok(len)
}
fn check(code: u32) -> Result<(), u32> {
    if code == 0 { Ok(()) } else { Err(code) }
}
fn inside(range: Range, offset: u64, len: u64) -> Result<(), u32> {
    check(virt_policy::r4os_rust_vm_inside(range.len, offset, len))
}
fn no_guard(range: Range, base: u64, len: u64) -> Result<(), u32> {
    check(virt_policy::r4os_rust_vm_guard(
        base,
        len,
        range.guard_base,
        range.guard_len,
    ))
}
fn mapped(address: u64) -> bool {
    let mut value = 0;
    unsafe { paging::r4os_rust_paging_read(address, 3, false, &mut value) }
}
fn committed(range: Range, base: u64, len: u64, want: bool) -> Result<(), u32> {
    if range.window == 6 {
        let slot = lookup(range.id)?;
        let head = vm_ranges::read(slot).commit_span_head;
        let present =
            unsafe { vm_commit_spans::r4os_rust_vm_commit_span_scan(head, base, len, want) };
        return if want {
            if present { Ok(()) } else { Err(10) }
        } else if present {
            Err(9)
        } else {
            Ok(())
        };
    }
    let mut offset = 0;
    while offset < len {
        if mapped(base + offset) != want {
            return Err(if want { 10 } else { 9 });
        }
        offset += PAGE;
    }
    Ok(())
}
fn block_committed(range: Range) -> Result<(), u32> {
    let code = unsafe {
        blocks::r4os_rust_blocks_change(range.block_id, 1, range.committed_bytes, ptr::null())
    };
    if code == 0 {
        Ok(())
    } else {
        Err(vm_frames::block_error(code))
    }
}
unsafe fn update<T>(range: *mut Range, operation: impl FnOnce(&mut Range) -> T) -> T {
    operation(unsafe { &mut *range })
}
unsafe fn snapshot(range: *const Range) -> Range {
    unsafe { range.read() }
}
unsafe fn partial(range: *mut Range, operation: u8, base: u64, len: u64, reset: bool) {
    let _owner = Guard::acquire(5);
    unsafe {
        update(range, |range| match operation {
            0 => {
                range.partial_uncommit_base = base;
                range.partial_uncommit_len = len;
                if reset {
                    range.partial_uncommit_cursor = base;
                    range.partial_uncommit_accounted = false;
                }
            }
            1 => range.partial_uncommit_cursor = base,
            2 => {
                range.partial_uncommit_base = 0;
                range.partial_uncommit_len = 0;
                range.partial_uncommit_cursor = 0;
                range.partial_uncommit_accounted = false;
            }
            _ => unreachable!(),
        });
    }
}
unsafe fn uncommit_span(range: *mut Range, start: u64, len: u64, skip: bool) -> Result<(), u32> {
    let current = unsafe { snapshot(range) };
    if current.window == 6 {
        return check(unsafe { r4os_zig_vm_uncommit_demand(range, start, len, skip) });
    }
    let end = start.checked_add(len).ok_or(5u32)?;
    let mut address =
        if current.partial_uncommit_cursor >= start && current.partial_uncommit_cursor <= end {
            current.partial_uncommit_cursor
        } else {
            start
        };
    while address < end {
        let Some(frame) = vm_frames::mapped(address, false) else {
            if !skip {
                return Err(10);
            }
            address += PAGE;
            unsafe {
                partial(range, 1, address, 0, false);
            }
            continue;
        };
        let maximum = ((end - address) / PAGE).min(BATCH);
        let claimed = blocks::r4os_rust_blocks_prefix(frame, maximum * PAGE, false) / PAGE;
        if claimed == 0 {
            return Err(10);
        }
        let mut pages = 1;
        while pages < claimed {
            if vm_frames::mapped(address + pages * PAGE, false) != Some(frame + pages * PAGE) {
                break;
            }
            pages += 1;
        }
        check(unsafe {
            vm_frames::r4os_rust_vm_uncommit_extent(range, address, frame, pages, true, false)
        })?;
        address += pages * PAGE;
        unsafe {
            partial(range, 1, address, 0, false);
        }
    }
    Ok(())
}
struct Lease {
    range: *mut Range,
    unwind: Unwind,
}
impl Lease {
    unsafe fn admit(range: *mut Range) -> Result<Self, u32> {
        let mut token = MaybeUninit::<Unwind>::uninit();
        unsafe {
            r4os_zig_heap_unwind_enter(token.as_mut_ptr());
        }
        let unwind = unsafe { token.assume_init() };
        if unwind.context_present && !unwind.active {
            return Err(14);
        }
        unsafe {
            update(range, |range| range.resident_commit_active = true);
        }
        Ok(Self { range, unwind })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Owner is reacquired before any handoff returns. Close the exact
        // range lease before allowing the pinned task's terminal unwind.
        unsafe {
            update(self.range, |range| range.resident_commit_active = false);
            r4os_zig_heap_unwind_leave(&self.unwind);
        }
    }
}
fn reschedule(owner: &mut Option<Guard>) {
    drop(owner.take());
    unsafe {
        r4os_zig_vm_reschedule();
    }
    *owner = Some(Guard::acquire(5));
}
unsafe fn drain(range: *mut Range, owner: &mut Option<Guard>) -> Result<(), u32> {
    let result = (|| {
        let prepared = unsafe { snapshot(range) }.resident_prepared;
        if prepared.count != 0 {
            vm_frames::release_prepared(prepared, false)?;
            unsafe {
                update(range, |range| {
                    range.resident_prepared = phys::Extent { base: 0, count: 0 }
                });
            }
        }
        let current = unsafe { snapshot(range) };
        let end = current.resident_commit_base + current.resident_commit_bytes;
        while unsafe { snapshot(range) }.partial_uncommit_cursor < end {
            let start = unsafe { snapshot(range) }.partial_uncommit_cursor;
            unsafe { uncommit_span(range, start, (end - start).min(BATCH * PAGE), true) }?;
            counters(|stats| {
                stats.resident_rollback_batches = stats.resident_rollback_batches.wrapping_add(1)
            });
            if unsafe { snapshot(range) }.partial_uncommit_cursor < end {
                reschedule(owner);
            }
        }
        unsafe {
            partial(range, 2, 0, 0, false);
            update(range, |range| {
                range.resident_commit_failed = false;
                range.resident_commit_base = 0;
                range.resident_commit_bytes = 0;
            });
        }
        Ok(())
    })();
    // Original defer publishes the retained count on every error as well.
    unsafe {
        update(range, |range| {
            range.status = if range.committed_bytes == 0 { 1 } else { 2 }
        });
    }
    let _ = block_committed(unsafe { snapshot(range) });
    result
}
fn finish_rollback(id: u32) -> Result<(), u32> {
    let outer = !owner::fault_allowed();
    let mut owner = Some(Guard::acquire(5));
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.resident_commit_active {
        return Err(14);
    }
    if !current.resident_commit_failed {
        return Ok(());
    }
    if outer && current.resident_commit_bytes > BATCH * PAGE {
        return Err(14);
    }
    let range = vm_ranges::slot(slot);
    let _lease = unsafe { Lease::admit(range) }?;
    unsafe { drain(range, &mut owner) }
}
fn commit(id: u32, offset: u64, raw_len: u64) -> Result<(), u32> {
    finish_rollback(id)?;
    let outer = !owner::fault_allowed();
    let mut owner = Some(Guard::acquire(5));
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let len = normalized(raw_len)?;
    if !offset.is_multiple_of(PAGE) {
        return Err(4);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.resident_commit_active || current.pager_io_count != 0 {
        return Err(14);
    }
    if current.partial_uncommit_len != 0 {
        return Err(10);
    }
    inside(current, offset, len)?;
    no_guard(current, current.base + offset, len)?;
    let range = vm_ranges::slot(slot);
    if current.window == 6 {
        return check(unsafe { r4os_zig_vm_demand_commit(range, offset, len) });
    }
    let pages = len / PAGE;
    if outer && pages > BATCH {
        return Err(14);
    }
    if !vm_frames::can_commit(current, pages) {
        return Err(12);
    }
    let _lease = unsafe { Lease::admit(range) }?;
    unsafe {
        update(range, |range| {
            range.resident_commit_base = range.base + offset;
            range.resident_commit_bytes = 0;
        });
    }
    let result = (|| {
        let mut done = 0;
        while done < pages {
            let current = unsafe { snapshot(range) };
            let address = current.resident_commit_base + done * PAGE;
            let maximum = (pages - done).min(BATCH);
            committed(current, address, maximum * PAGE, false)?;
            if !vm_frames::can_commit(current, maximum) {
                return Err(12);
            }
            let mut extent = MaybeUninit::<phys::Extent>::uninit();
            check(unsafe {
                vm_frames::r4os_rust_vm_allocate(&current, 2, maximum, false, extent.as_mut_ptr())
            })?;
            let extent = unsafe { extent.assume_init() };
            let bytes = extent.count * PAGE;
            unsafe {
                update(range, |range| range.resident_prepared = extent);
            }
            drop(owner.take());
            unsafe {
                ptr::write_bytes(
                    phys::r4os_rust_phys_to_virt(extent.base) as *mut u8,
                    0,
                    bytes as usize,
                );
            }
            owner = Some(Guard::acquire(5));
            if !paging::r4os_rust_paging_map_batch(
                address,
                extent.base,
                extent.count,
                current.flags,
            ) {
                return Err(13);
            }
            unsafe {
                update(range, |range| {
                    range.resident_prepared = phys::Extent { base: 0, count: 0 };
                    range.committed_bytes += bytes;
                    range.resident_commit_bytes += bytes;
                    range.resident_bytes = range.resident_bytes.saturating_add(bytes);
                    range.peak_resident_bytes = range.peak_resident_bytes.max(range.resident_bytes);
                });
            }
            counters(|stats| {
                stats.resident_commit_batches = stats.resident_commit_batches.wrapping_add(1);
                stats.resident_commit_max_batch_pages =
                    stats.resident_commit_max_batch_pages.max(extent.count);
                if !outer {
                    stats.resident_commit_unlocked_zero_bytes = stats
                        .resident_commit_unlocked_zero_bytes
                        .wrapping_add(bytes);
                }
            });
            done += extent.count;
            if done < pages {
                reschedule(&mut owner);
            }
        }
        unsafe {
            update(range, |range| range.status = 2);
        }
        block_committed(unsafe { snapshot(range) })?;
        unsafe {
            update(range, |range| {
                range.resident_commit_base = 0;
                range.resident_commit_bytes = 0;
            });
        }
        Ok(())
    })();
    if result.is_err() {
        unsafe {
            update(range, |range| range.resident_commit_failed = true);
            let current = snapshot(range);
            partial(
                range,
                0,
                current.resident_commit_base,
                current.resident_commit_bytes,
                true,
            );
            let _ = drain(range, &mut owner);
        }
    }
    result
}
fn uncommit(id: u32, offset: u64, raw_len: u64) -> Result<(), u32> {
    let _owner = Guard::acquire(5);
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let len = normalized(raw_len)?;
    if !offset.is_multiple_of(PAGE) {
        return Err(4);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.resident_commit_active
        || current.resident_commit_failed
        || current.pager_io_count != 0
    {
        return Err(14);
    }
    inside(current, offset, len)?;
    let start = current.base + offset;
    no_guard(current, start, len)?;
    let resume = current.partial_uncommit_len != 0;
    let range = vm_ranges::slot(slot);
    if resume {
        if current.partial_uncommit_base != start || current.partial_uncommit_len != len {
            return Err(10);
        }
    } else {
        committed(current, start, len, true)?;
        unsafe {
            partial(range, 0, start, len, true);
        }
    }
    let result = unsafe { uncommit_span(range, start, len, resume) };
    unsafe {
        update(range, |range| {
            range.status = if range.committed_bytes == 0 { 1 } else { 2 }
        });
    }
    block_committed(unsafe { snapshot(range) })?;
    result?;
    unsafe {
        partial(range, 2, 0, 0, false);
    }
    Ok(())
}
fn guard(id: u32, offset: u64, raw_len: u64, clear: bool) -> Result<(), u32> {
    let _owner = Guard::acquire(5);
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let len = if clear { 0 } else { normalized(raw_len)? };
    if !clear && !offset.is_multiple_of(PAGE) {
        return Err(4);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.resident_commit_active || current.resident_commit_failed {
        return Err(14);
    }
    if clear {
        vm_ranges::update(slot, |range| {
            range.guard_base = 0;
            range.guard_len = 0;
        });
        return Ok(());
    }
    inside(current, offset, len)?;
    let base = current.base + offset;
    let mut position = 0;
    while position < len {
        if mapped(base + position) {
            return Err(9);
        }
        position += PAGE;
    }
    let again = lookup(current.id)?;
    if unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_scan(
            vm_ranges::read(again).commit_span_head,
            base,
            len,
            false,
        )
    } {
        return Err(9);
    }
    vm_ranges::update(slot, |range| {
        range.guard_base = base;
        range.guard_len = len;
    });
    Ok(())
}
fn code(result: Result<(), u32>) -> u32 {
    result.err().unwrap_or(0)
}
fn remove_commit_span(id: u32, base: u64, len: u64) -> Result<(), u32> {
    let mut reserved = vm_ranges::OptionalSlot::NONE;
    check(unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_change(3, id, base, len, &mut reserved)
    })?;
    let result = check(unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_change(1, id, base, len, &mut reserved)
    });
    if let Some(slot) = reserved.get() {
        unsafe {
            vm_commit_spans::r4os_rust_vm_commit_span_change(
                4,
                0,
                u64::from(slot),
                0,
                ptr::null_mut(),
            );
        }
    }
    result
}
fn commit_resident(id: u32, offset: u64, raw_len: u64) -> Result<(), u32> {
    let _owner = Guard::acquire(5);
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let len = normalized(raw_len)?;
    if len > BATCH * PAGE {
        return Err(15);
    }
    if !offset.is_multiple_of(PAGE) {
        return Err(4);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.window != 6 || current.kind != 4 {
        return Err(15);
    }
    if current.partial_uncommit_len != 0 {
        return Err(10);
    }
    inside(current, offset, len)?;
    let start = current.base + offset;
    no_guard(current, start, len)?;
    committed(current, start, len, false)?;
    let pages = len / PAGE;
    if !vm_frames::can_commit(current, pages) {
        return Err(12);
    }
    let next_committed = current.committed_bytes.checked_add(len).ok_or(5u32)?;
    check(unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_change(0, id, start, len, ptr::null_mut())
    })?;
    let page_code = unsafe { r4os_zig_vm_resident_page_state(id, offset / PAGE, pages, false) };
    if page_code != 0 {
        let _ = remove_commit_span(id, start, len);
        return Err(page_code);
    }
    let range = vm_ranges::slot(slot);
    unsafe {
        update(range, |range| {
            range.committed_bytes = next_committed;
            range.status = 2;
        });
    }
    let result = (|| {
        let mut done = 0;
        while done < pages {
            let address = start + done * PAGE;
            let current = unsafe { snapshot(range) };
            let mut extent = MaybeUninit::<phys::Extent>::uninit();
            check(unsafe {
                vm_frames::r4os_rust_vm_allocate(
                    &current,
                    2,
                    pages - done,
                    false,
                    extent.as_mut_ptr(),
                )
            })?;
            let extent = unsafe { extent.assume_init() };
            if !paging::r4os_rust_paging_map_batch(
                address,
                extent.base,
                extent.count,
                current.flags,
            ) {
                let _ = vm_frames::release_prepared(extent, false);
                return Err(13);
            }
            let bytes = extent.count * PAGE;
            unsafe {
                ptr::write_bytes(address as *mut u8, 0, bytes as usize);
                update(range, |range| {
                    range.resident_bytes = range.resident_bytes.saturating_add(bytes);
                    range.peak_resident_bytes = range.peak_resident_bytes.max(range.resident_bytes);
                });
            }
            done += extent.count;
        }
        check(unsafe { r4os_zig_vm_resident_page_state(id, offset / PAGE, pages, true) })?;
        block_committed(unsafe { snapshot(range) })
    })();
    if result.is_err() {
        unsafe {
            partial(range, 0, start, len, true);
            if uncommit_span(range, start, len, false).is_ok() {
                partial(range, 2, 0, 0, false);
            }
            update(range, |range| {
                range.status = if range.committed_bytes == 0 { 1 } else { 2 }
            });
        }
        let _ = block_committed(unsafe { snapshot(range) });
    }
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_commit_resident(id: u32, offset: u64, len: u64) -> u32 {
    code(commit_resident(id, offset, len))
}
fn release(id: u32) -> Result<(), u32> {
    finish_rollback(id)?;
    let _owner = Guard::acquire(5);
    if !r4os_rust_vm_initialized() {
        return Err(1);
    }
    let slot = lookup(id)?;
    let current = vm_ranges::read(slot);
    if current.resident_commit_active
        || current.resident_commit_failed
        || current.pager_io_count != 0
    {
        return Err(14);
    }
    let range = vm_ranges::slot(slot);
    if current.window == 6 && current.owner == 3 && current.owner_id <= u64::from(u32::MAX) {
        unsafe {
            r4os_zig_vm_pager_cleanup(range, 0, slot);
        }
    }
    let resume = current.partial_uncommit_len != 0
        && current.partial_uncommit_base == current.base
        && current.partial_uncommit_len == current.len;
    if !resume {
        unsafe {
            partial(range, 0, current.base, current.len, true);
        }
    }
    if current.committed_bytes != 0 {
        let result = unsafe { uncommit_span(range, current.base, current.len, true) };
        if let Err(error) = result {
            block_committed(unsafe { snapshot(range) })?;
            return Err(error);
        }
    }
    // Retain the retry identity through block release. Close spans, indices
    // and the range in the same original IRQ-atomic publication boundary.
    let _publication = Guard::acquire(5);
    let code = unsafe { blocks::r4os_rust_blocks_change(current.block_id, 4, 0, ptr::null()) };
    if code != 0 {
        return Err(vm_frames::block_error(code));
    }
    unsafe {
        vm_commit_spans::r4os_rust_vm_commit_span_change(2, current.id, 0, 0, ptr::null_mut());
        r4os_zig_vm_pager_cleanup(range, 1, slot);
        vm_index::r4os_rust_vm_index_change(1, current.id, 0);
        r4os_zig_vm_pager_cleanup(range, 2, slot);
        update(range, |range| {
            range.status = 5;
            range.committed_bytes = 0;
            range.resident_bytes = 0;
            range.peak_resident_bytes = 0;
            range.fault_count = 0;
            range.failed_faults = 0;
            range.guard_base = 0;
            range.guard_len = 0;
            range.partial_uncommit_base = 0;
            range.partial_uncommit_len = 0;
            range.partial_uncommit_cursor = 0;
            range.partial_uncommit_accounted = false;
        });
        vm_index::r4os_rust_vm_index_change(4, 0, slot);
    }
    Ok(())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_release(id: u32) -> u32 {
    code(release(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_commit(id: u32, offset: u64, len: u64) -> u32 {
    code(commit(id, offset, len))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_uncommit(id: u32, offset: u64, len: u64) -> u32 {
    code(uncommit(id, offset, len))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_finish_rollback(id: u32) -> u32 {
    code(finish_rollback(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_protect_guard(id: u32, offset: u64, len: u64, clear: bool) -> u32 {
    code(guard(id, offset, len, clear))
}
/// # Safety
/// range is the exact admitted C lease under VM owner, pinned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_partial(
    range: *mut Range,
    operation: u8,
    base: u64,
    len: u64,
    reset: bool,
) {
    unsafe {
        partial(range, operation, base, len, reset);
    }
}
/// # Safety
/// Caller retains VM owner and the exact range lease; waiting pager callbacks
/// receive raw C values only and retain their existing I/O admission contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_uncommit_span(
    range: *mut Range,
    start: u64,
    len: u64,
    skip: bool,
) -> u32 {
    code(unsafe { uncommit_span(range, start, len, skip) })
}
/// # Safety
/// Caller retains VM owner; output is exclusive live C stats storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_transaction_stats(output: *mut Stats) {
    let stats = unsafe { *COUNTERS.0.get() };
    unsafe {
        output.write(stats);
    }
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 32);
};
