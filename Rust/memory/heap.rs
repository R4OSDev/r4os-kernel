//! Boundary-tag kernel allocator: original bins, coalescence, optional poison,
//! growth serialization and exact partial-tail retention.
mod release;

use super::{
    heap_policy as policy,
    owner::{self, Token},
    phys,
};
#[cfg(not(test))]
use core::alloc::{GlobalAlloc, Layout};
use core::{
    cell::UnsafeCell,
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
const BASE: u64 = 0xffff_ff10_0000_0000;
const PAGE: usize = 4096;
const WINDOW: usize = 1024 * 1024 * 1024;
const CAP_MIN: usize = 32 * 1024 * 1024;
const GRANULE: usize = 16;
const HEADER: usize = 16;
const FOOTER: usize = 8;
const OVERHEAD: usize = HEADER + FOOTER;
const MIN_BLOCK: usize = 64;
const BINS: usize = 24;
const NONE: u64 = u64::MAX;
const FREE_MAGIC: u64 = 0xf6ee_b10c_f6ee_b10c;
const SCAN_LIMIT: usize = 16;
pub(crate) const FULL_POISON: bool = crate::build_config::config::HEAP_POISON;

/// Only the allocator's exact private live/retiring block may reach this fill.
/// Boundary tags and free-list metadata are independent of this diagnostic.
fn poison(offset: usize, size: usize) -> u64 {
    if !FULL_POISON || size < OVERHEAD + GRANULE {
        return 0;
    }
    unsafe {
        core::ptr::write_bytes(payload(offset), 0xdd, size - OVERHEAD);
    }
    (size - OVERHEAD) as u64
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub base: u64,
    pub pages: usize,
    pub capacity_bytes: usize,
    pub reserved_bytes: usize,
    pub committed_bytes: usize,
    pub used_bytes: usize,
    pub free_bytes: usize,
    pub active_blocks: usize,
    pub free_blocks: usize,
    pub largest_free: usize,
    pub allocation_errors: u64,
    pub invalid_free_errors: u64,
    pub double_free_errors: u64,
    pub size_mismatch_errors: u64,
    pub oom_errors: u64,
    pub reentry_errors: u64,
    pub next_growth_pages: usize,
    pub pending_uncommit_bytes: usize,
    pub commit_calls: u64,
    pub commit_failures: u64,
    pub committed_pages_total: u64,
    pub uncommit_calls: u64,
    pub uncommit_failures: u64,
    pub uncommitted_pages_total: u64,
    pub release_suppressed: u64,
    pub pressure_releases: u64,
    pub poison_bytes: u64,
    pub retained_tail_pages: usize,
    pub fragmentation_hint: bool,
    pub bootstrap_bump_active: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Metadata {
    pub base: u64,
    pub len: u64,
}
pub use crate::sched::task_context::Unwind;
struct Control {
    bins: [u64; BINS],
    heap_top: usize,
    committed_pages: usize,
    cap_pages: usize,
    used_bytes: usize,
    active_blocks: usize,
    free_blocks: usize,
    free_bytes: usize,
    range_id: u32,
    in_heap: bool,
    growth_cleanup_pending: bool,
    reentry_reported: bool,
    allocation_errors: u64,
    invalid_free_errors: u64,
    double_free_errors: u64,
    size_mismatch_errors: u64,
    oom_errors: u64,
    reentry_errors: u64,
    next_growth_pages: usize,
    pending_uncommit_start: usize,
    pending_uncommit_bytes: usize,
    pending_uncommit_pressure: bool,
    next_release_id: u64,
    commit_calls: u64,
    commit_failures: u64,
    committed_pages_total: u64,
    uncommit_calls: u64,
    uncommit_failures: u64,
    uncommitted_pages_total: u64,
    release_suppressed: u64,
    pressure_releases: u64,
    poison_bytes: u64,
    corruption_reported: bool,
    corruption_count: u64,
    pending_corruption: u8,
    pending_corruption_offset: usize,
}
impl Control {
    const EMPTY: Self = Self {
        bins: [NONE; BINS],
        heap_top: 0,
        committed_pages: 0,
        cap_pages: 0,
        used_bytes: 0,
        active_blocks: 0,
        free_blocks: 0,
        free_bytes: 0,
        range_id: 0,
        in_heap: false,
        growth_cleanup_pending: false,
        reentry_reported: false,
        allocation_errors: 0,
        invalid_free_errors: 0,
        double_free_errors: 0,
        size_mismatch_errors: 0,
        oom_errors: 0,
        reentry_errors: 0,
        next_growth_pages: policy::MIN_GROWTH_PAGES,
        pending_uncommit_start: 0,
        pending_uncommit_bytes: 0,
        pending_uncommit_pressure: false,
        next_release_id: 0,
        commit_calls: 0,
        commit_failures: 0,
        committed_pages_total: 0,
        uncommit_calls: 0,
        uncommit_failures: 0,
        uncommitted_pages_total: 0,
        release_suppressed: 0,
        pressure_releases: 0,
        poison_bytes: 0,
        corruption_reported: false,
        corruption_count: 0,
        pending_corruption: 0,
        pending_corruption_offset: 0,
    };
    fn committed(&self) -> usize {
        self.committed_pages * PAGE
    }
    fn corruption(&mut self, kind: u8, offset: usize) {
        self.corruption_count += 1;
        if self.corruption_reported {
            return;
        }
        self.corruption_reported = true;
        self.pending_corruption = kind;
        self.pending_corruption_offset = offset;
    }
    fn insert_free(&mut self, offset: usize, size: usize) {
        write(offset, size as u64);
        write(offset + 8, FREE_MAGIC);
        write(offset + size - FOOTER, size as u64);
        let bin = bin(size);
        let head = self.bins[bin];
        write(offset + 16, head);
        write(offset + 24, NONE);
        if head != NONE {
            write((head + 24) as usize, offset as u64);
        }
        self.bins[bin] = offset as u64;
        self.free_blocks += 1;
        self.free_bytes += size;
    }
    fn plausible(&self, offset: u64) -> bool {
        offset & (GRANULE as u64 - 1) == 0 && offset as usize + MIN_BLOCK <= self.heap_top
    }
    fn unlink_free(&mut self, offset: usize, size: usize) {
        let header = read(offset);
        if header & 1 != 0 || block_size(header) != size || read(offset + 8) != FREE_MAGIC {
            self.corruption(1, offset);
        }
        let next = read(offset + 16);
        let previous = read(offset + 24);
        if next != NONE && !self.plausible(next) {
            self.corruption(2, offset);
        } else if previous != NONE && !self.plausible(previous) {
            self.corruption(3, offset);
        } else {
            if previous != NONE {
                write((previous + 16) as usize, next);
            } else {
                self.bins[bin(size)] = next;
            }
            if next != NONE {
                write((next + 24) as usize, previous);
            }
        }
        self.free_blocks -= 1;
        self.free_bytes -= size;
    }
    fn release_block(&mut self, offset: usize, size: usize) {
        self.poison_bytes = self.poison_bytes.wrapping_add(poison(offset, size));
        self.publish_free(offset, size);
    }
    fn publish_free(&mut self, mut offset: usize, mut size: usize) {
        let next_offset = offset + size;
        if next_offset < self.heap_top {
            let next = read(next_offset);
            if next & 1 == 0 {
                let next_size = block_size(next);
                self.unlink_free(next_offset, next_size);
                size += next_size;
            }
        }
        if offset > 0 {
            let previous = read(offset - FOOTER);
            if previous & 1 == 0 {
                let previous_size = block_size(previous);
                let previous_offset = offset - previous_size;
                self.unlink_free(previous_offset, previous_size);
                // This formerly used header becomes interior free payload.
                // Do not leave a plausible used header/footer pair behind.
                write(offset, 0);
                write(offset + 8, 0);
                offset = previous_offset;
                size += previous_size;
            }
        }
        self.insert_free(offset, size);
    }
    fn find(&mut self, need: usize) -> Option<usize> {
        let start = bin(need);
        let mut cursor = self.bins[start];
        let mut scanned = 0;
        while cursor != NONE && scanned < SCAN_LIMIT {
            let offset = cursor as usize;
            let size = block_size(read(offset));
            if size >= need {
                self.unlink_free(offset, size);
                return Some(offset);
            }
            cursor = read(offset + 16);
            scanned += 1;
        }
        for index in start + 1..BINS {
            let head = self.bins[index];
            if head == NONE {
                continue;
            }
            let offset = head as usize;
            let size = block_size(read(offset));
            if size >= need {
                self.unlink_free(offset, size);
                return Some(offset);
            }
            let mut cursor = read(offset + 16);
            while cursor != NONE {
                let offset = cursor as usize;
                let size = block_size(read(offset));
                if size >= need {
                    self.unlink_free(offset, size);
                    return Some(offset);
                }
                cursor = read(offset + 16);
            }
        }
        None
    }
    fn find_aligned(&mut self, need: usize, alignment: usize) -> Option<(usize, usize)> {
        for index in bin(need)..BINS {
            let mut cursor = self.bins[index];
            let mut scanned = 0;
            while cursor != NONE && scanned < SCAN_LIMIT {
                let offset = cursor as usize;
                let size = block_size(read(offset));
                if let Some(payload) = aligned_payload(offset, size, need, alignment) {
                    self.unlink_free(offset, size);
                    return Some((offset, payload));
                }
                cursor = read(offset + 16);
                scanned += 1;
            }
        }
        None
    }
    fn place(
        &mut self,
        offset: usize,
        need: usize,
        requested: usize,
        aligned: Option<usize>,
    ) -> *mut u8 {
        let total = block_size(read(offset));
        let mut used_offset = offset;
        let mut used_size = total;
        if let Some(payload) = aligned {
            let prefix = payload - HEADER - offset;
            if prefix != 0 {
                self.insert_free(offset, prefix);
                used_offset += prefix;
                used_size -= prefix;
            }
        }
        if used_size - need >= MIN_BLOCK {
            write_used(used_offset, need, requested);
            self.insert_free(used_offset + need, used_size - need);
        } else {
            write_used(used_offset, used_size, requested);
        }
        self.active_blocks += 1;
        self.used_bytes += requested;
        payload(used_offset)
    }
    fn validate(&self, pointer: *mut u8, len: usize) -> Option<Validation> {
        let address = pointer as u64;
        if address < BASE + HEADER as u64 {
            return None;
        }
        let payload_offset = (address - BASE) as usize;
        if payload_offset >= self.heap_top || payload_offset & (GRANULE - 1) != 0 {
            return None;
        }
        let offset = payload_offset - HEADER;
        let header = read(offset);
        let size = block_size(header);
        let plausible = size >= MIN_BLOCK
            && size <= self.heap_top
            && offset + size <= self.heap_top
            && read(offset + size - FOOTER) == header;
        if !plausible {
            return self.classify(offset);
        }
        if header & 1 == 0 {
            if read(offset + 8) == FREE_MAGIC {
                return Some(Validation {
                    kind: 1,
                    offset,
                    size,
                    requested: 0,
                });
            }
            return self.classify(offset);
        }
        if header & release::RETIRING != 0 {
            return Some(Validation {
                kind: 1,
                offset,
                size,
                requested: 0,
            });
        }
        let requested = read(offset + 8) as usize;
        if requested > size - OVERHEAD {
            return self.classify(offset);
        }
        Some(Validation {
            kind: if requested == len { 0 } else { 2 },
            offset,
            size,
            requested,
        })
    }
    fn classify(&self, target: usize) -> Option<Validation> {
        let mut offset = 0;
        while offset < self.heap_top {
            let header = read(offset);
            let size = block_size(header);
            if size < MIN_BLOCK || offset + size > self.heap_top {
                return None;
            }
            if target < offset + size {
                return if header & 1 == 0 {
                    Some(Validation {
                        kind: 1,
                        offset,
                        size,
                        requested: 0,
                    })
                } else {
                    None
                };
            }
            offset += size;
        }
        None
    }
    fn largest(&self) -> usize {
        for index in (0..BINS).rev() {
            let mut cursor = self.bins[index];
            if cursor == NONE {
                continue;
            }
            let mut largest = 0;
            while cursor != NONE {
                let offset = cursor as usize;
                largest = largest.max(block_size(read(offset)));
                cursor = read(offset + 16);
            }
            return largest;
        }
        0
    }
    fn retire_tail(&mut self, pressure: bool) -> bool {
        if GROWTH.load(Ordering::Acquire) != 0
            || self.growth_cleanup_pending
            || self.heap_top <= PAGE
        {
            return false;
        }
        let footer = read(self.heap_top - FOOTER);
        if footer & 1 != 0 {
            return false;
        }
        let size = block_size(footer);
        let start = self.heap_top - size;
        let Some(first) = align_up(start, PAGE) else {
            return false;
        };
        if first >= self.heap_top {
            return false;
        }
        let free_pages = (self.heap_top - first) / PAGE;
        let retained =
            policy::r4os_rust_heap_retained(self.next_growth_pages, self.cap_pages, pressure);
        if !policy::r4os_rust_heap_release_tail(free_pages, retained, pressure) {
            self.release_suppressed = self.release_suppressed.wrapping_add(1);
            return false;
        }
        let Some(retained_end) = start.checked_add(retained * PAGE) else {
            return false;
        };
        let Some(mut uncommit) = align_up(retained_end, PAGE) else {
            return false;
        };
        let mut remainder = uncommit - start;
        if remainder != 0 && remainder < MIN_BLOCK {
            uncommit += PAGE;
            remainder += PAGE;
        }
        if uncommit < PAGE {
            uncommit = PAGE;
            remainder = uncommit - start;
            if remainder != 0 && remainder < MIN_BLOCK {
                uncommit += PAGE;
                remainder += PAGE;
            }
        }
        if uncommit >= self.heap_top {
            return false;
        }
        self.unlink_free(start, size);
        let bytes = self.heap_top - uncommit;
        self.committed_pages = uncommit / PAGE;
        self.heap_top = uncommit;
        if remainder != 0 {
            self.insert_free(start, remainder);
        }
        self.pending_uncommit_start = uncommit;
        self.pending_uncommit_bytes = bytes;
        self.pending_uncommit_pressure = pressure;
        true
    }
}
struct Validation {
    kind: u8,
    offset: usize,
    size: usize,
    requested: usize,
}
fn read(offset: usize) -> u64 {
    unsafe { ((BASE as usize + offset) as *const u64).read() }
}
fn write(offset: usize, value: u64) {
    unsafe {
        ((BASE as usize + offset) as *mut u64).write(value);
    }
}
fn payload(offset: usize) -> *mut u8 {
    (BASE as usize + offset + HEADER) as *mut u8
}
fn block_size(header: u64) -> usize {
    (header & !(GRANULE as u64 - 1)) as usize
}
fn block_size_for(requested: usize) -> Option<usize> {
    Some(align_up(requested.checked_add(OVERHEAD)?, GRANULE)?.max(MIN_BLOCK))
}
fn align_up(value: usize, alignment: usize) -> Option<usize> {
    Some(value.checked_add(alignment - 1)? & !(alignment - 1))
}
fn normalize_alignment(alignment: usize) -> Option<usize> {
    let value = alignment.max(1);
    if value.is_power_of_two() {
        Some(value)
    } else {
        None
    }
}
fn bin(size: usize) -> usize {
    if size < 128 {
        0
    } else {
        (63 - (size as u64).leading_zeros() as usize - 6).min(BINS - 1)
    }
}
fn aligned_payload(offset: usize, size: usize, need: usize, alignment: usize) -> Option<usize> {
    let mut payload = align_up(offset + HEADER, alignment)?;
    loop {
        let prefix = payload - HEADER - offset;
        if prefix == 0 || prefix >= MIN_BLOCK {
            break;
        }
        payload = payload.checked_add(alignment)?;
    }
    let prefix = payload - HEADER - offset;
    if prefix.checked_add(need)? > size {
        None
    } else {
        Some(payload)
    }
}
fn write_used(offset: usize, size: usize, requested: usize) {
    write(offset, size as u64 | 1);
    write(offset + 8, requested as u64);
    write(offset + size - FOOTER, size as u64 | 1);
}
struct Storage(UnsafeCell<Control>);
// SAFETY: singleton BSP initialization precedes publication. Subsequently
// Control and boundary tags are admitted by the exact heap owner.
// Each access scope ends before task/VM/log/wait C calls.
unsafe impl Sync for Storage {}
static CONTROL: Storage = Storage(UnsafeCell::new(Control::EMPTY));
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static GROWTH: AtomicU8 = AtomicU8::new(0);
fn control<T>(operation: impl FnOnce(&mut Control) -> T) -> T {
    operation(unsafe { &mut *CONTROL.0.get() })
}

struct HeapGuard {
    token: Token,
    unwind: Unwind,
    wake_growth: bool,
}
impl HeapGuard {
    fn enter() -> Option<Self> {
        let lock = owner::named(4);
        let token = lock.acquire();
        let unwind = unsafe { crate::sched::task_context::enter() };
        if unwind.context_present && !unwind.active {
            control(|control| control.reentry_errors += 1);
            lock.release(token);
            return None;
        }
        let (reentry, report) = control(|control| {
            if control.in_heap {
                control.reentry_errors += 1;
                let report = !control.reentry_reported;
                control.reentry_reported = true;
                (true, report)
            } else {
                control.in_heap = true;
                (false, false)
            }
        });
        if reentry {
            if report {
                unsafe {
                    heap_fault(0, 0);
                }
            }
            lock.release(token);
            unsafe {
                crate::sched::task_context::leave(unwind);
            }
            return None;
        }
        Some(Self {
            token,
            unwind,
            wake_growth: false,
        })
    }
    fn can_drop(&self) -> bool {
        self.token.previous_rank == 0 && unsafe { owner_runtime_depth() } == 0
    }
    fn can_wait(&self) -> bool {
        self.can_drop() && self.token.flags & (1 << 9) != 0 && unsafe { heap_can_wait() }
    }
    fn suspend(&self) {
        control(|control| control.in_heap = false);
        owner::named(4).release(self.token);
    }
    fn resume(&mut self) {
        self.token = owner::named(4).acquire();
        control(|control| control.in_heap = true);
    }
}
impl Drop for HeapGuard {
    fn drop(&mut self) {
        flush_diagnostics();
        control(|control| control.in_heap = false);
        if self.wake_growth {
            owner::named(4).release(self.token);
            unsafe {
                heap_wake_growth();
            }
            // Kill admission stays closed until every sleeper can observe the
            // actual completion. Reacquire before releasing the exact token.
            let token = owner::named(4).acquire();
            unsafe {
                crate::sched::task_context::leave(self.unwind);
            }
            owner::named(4).release(token);
        } else {
            unsafe {
                crate::sched::task_context::leave(self.unwind);
            }
            owner::named(4).release(self.token);
        }
    }
}
fn flush_diagnostics() {
    let (kind, offset) = control(|control| {
        let result = (
            control.pending_corruption,
            control.pending_corruption_offset,
        );
        control.pending_corruption = 0;
        result
    });
    if kind != 0 {
        unsafe {
            heap_fault(kind, offset as u64);
        }
    }
}
fn pressure() -> bool {
    let mut frames = phys::Stats::default();
    unsafe {
        phys::r4os_rust_phys_stats(&mut frames);
    }
    frames.total_frames != 0 && frames.free_frames <= (frames.total_frames / 32).max(512)
}
fn finish_tail(guard: &mut HeapGuard) -> bool {
    release::finish_tail(guard)
}
fn release_tail(guard: &mut HeapGuard) {
    if control(|control| GROWTH.load(Ordering::Acquire) != 0 || control.growth_cleanup_pending) {
        return;
    }
    if !finish_tail(guard) {
        return;
    }
    let pressure = pressure();
    if control(|control| control.retire_tail(pressure)) {
        let _ = finish_tail(guard);
    }
}
fn commit_pages(start: usize, pages: usize, guard: &mut HeapGuard) -> bool {
    if pages == 0 {
        return true;
    }
    let id = control(|control| {
        control.commit_calls = control.commit_calls.wrapping_add(1);
        control.range_id
    });
    let drop_owner = guard.can_drop();
    if drop_owner {
        guard.suspend();
    }
    let committed = unsafe { heap_commit(id, start as u64, (pages * PAGE) as u64) };
    if drop_owner {
        guard.resume();
    }
    control(|control| {
        if !committed {
            control.commit_failures = control.commit_failures.wrapping_add(1);
            control.growth_cleanup_pending = true;
        } else {
            control.growth_cleanup_pending = false;
            control.committed_pages_total =
                control.committed_pages_total.wrapping_add(pages as u64);
        }
    });
    committed
}
fn grow(min_extra: usize, guard: &mut HeapGuard) -> bool {
    if GROWTH.load(Ordering::Acquire) != 0 {
        if !guard.can_wait() {
            return false;
        }
        guard.suspend();
        let result = unsafe { heap_wait_growth() };
        guard.resume();
        return result;
    }
    if !finish_tail(guard) {
        return false;
    }
    // Preserve the original overflow rejection before page division.
    let minimum = min_extra
        .checked_add(PAGE - 1)
        .expect("heap page rounding overflow")
        / PAGE;
    if minimum == 0 {
        return true;
    }
    let prepared = control(|control| {
        if control.committed_pages + minimum > control.cap_pages {
            return None;
        }
        Some((
            control.committed(),
            policy::r4os_rust_heap_growth(
                minimum,
                control.next_growth_pages,
                control.cap_pages - control.committed_pages,
            ),
        ))
    });
    let Some((start, mut pages)) = prepared else {
        return false;
    };
    GROWTH.store(1, Ordering::Release);
    let result = (|| {
        if !commit_pages(start, pages, guard) {
            if pages == minimum || !commit_pages(start, minimum, guard) {
                return false;
            }
            pages = minimum;
        }
        let bytes = pages * PAGE;
        control(|control| {
            control.committed_pages += pages;
            control.heap_top = control.committed();
            control.next_growth_pages = policy::r4os_rust_heap_next_hint(pages);
            let mut offset = start;
            let mut size = bytes;
            if offset > 0 {
                let previous = read(offset - FOOTER);
                if previous & 1 == 0 {
                    let previous_size = block_size(previous);
                    let previous_offset = offset - previous_size;
                    control.unlink_free(previous_offset, previous_size);
                    offset = previous_offset;
                    size += previous_size;
                }
            }
            control.insert_free(offset, size);
        });
        true
    })();
    GROWTH.store(0, Ordering::Release);
    guard.wake_growth |= guard.can_drop() && guard.unwind.context_present;
    result
}
fn alloc_failure(oom: bool) -> *mut u8 {
    // Failed admission may have returned the owner; use a short admitted
    // scope instead of an unlocked mutable counter access.
    let _owner = super::owner::Guard::acquire(4);
    control(|control| {
        control.allocation_errors += 1;
        if oom {
            control.oom_errors += 1;
        }
    });
    null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_init() -> bool {
    INITIALIZED.store(false, Ordering::Release);
    control(|control| {
        let reported = control.corruption_reported;
        let count = control.corruption_count;
        *control = Control::EMPTY;
        control.corruption_reported = reported;
        control.corruption_count = count;
    });
    let id = unsafe { heap_reserve(BASE, WINDOW as u64) };
    control(|control| control.range_id = id);
    if id == 0 {
        return false;
    }
    let current = unsafe { &*crate::boot::info::r4os_rust_boot_info() };
    let mut summary = super::map::Summary::default();
    unsafe {
        super::map::r4os_rust_memory_summary(current.memory, current.memory_count, &mut summary);
    }
    control(|control| {
        control.cap_pages = (summary.usable_page_bytes as usize / 4).clamp(CAP_MIN, WINDOW) / PAGE
    });
    INITIALIZED.store(true, Ordering::Release);
    let Some(mut guard) = HeapGuard::enter() else {
        return false;
    };
    if !grow(PAGE, &mut guard) {
        INITIALIZED.store(false, Ordering::Release);
        return false;
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_alloc(requested: usize, alignment: usize) -> *mut u8 {
    let Some(mut guard) = HeapGuard::enter() else {
        return alloc_failure(false);
    };
    if !INITIALIZED.load(Ordering::Acquire) || requested == 0 {
        return alloc_failure(false);
    }
    let Some(alignment) = normalize_alignment(alignment) else {
        return alloc_failure(false);
    };
    let Some(need) = block_size_for(requested) else {
        return alloc_failure(true);
    };
    if control(|control| need > control.cap_pages * PAGE) {
        return alloc_failure(true);
    }
    loop {
        let found = control(|control| {
            if alignment <= GRANULE {
                control
                    .find(need)
                    .map(|offset| control.place(offset, need, requested, None))
            } else {
                control
                    .find_aligned(need, alignment)
                    .map(|(offset, payload)| control.place(offset, need, requested, Some(payload)))
            }
        });
        if let Some(pointer) = found {
            return pointer;
        }
        let padding = if alignment <= GRANULE {
            Some(MIN_BLOCK)
        } else {
            alignment.checked_add(MIN_BLOCK)
        };
        let Some(extra) = padding.and_then(|padding| need.checked_add(padding)) else {
            return alloc_failure(true);
        };
        if !grow(extra, &mut guard) {
            return alloc_failure(true);
        }
    }
}
/// # Safety
/// pointer describes readable allocator metadata for the caller's retained
/// allocation (or a still-mapped invalid/free diagnostic pointer).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_heap_free(pointer: *mut u8, len: usize) -> u8 {
    let Some(mut guard) = HeapGuard::enter() else {
        return 1;
    };
    let value = match control(|control| validate_free(control, pointer, len)) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let retiring = if release::can_unlock(&guard, value.size) {
        control(|control| {
            release::prepare(control, value.offset, value.size, Some(value.requested))
        })
    } else {
        None
    };
    if let Some(retiring) = retiring {
        release::finish(retiring, &mut guard);
    } else {
        control(|control| {
            control.active_blocks -= 1;
            control.used_bytes -= value.requested;
            control.release_block(value.offset, value.size);
        });
    }
    release_tail(&mut guard);
    0
}
fn validate_free(control: &mut Control, pointer: *mut u8, len: usize) -> Result<Validation, u8> {
    if !INITIALIZED.load(Ordering::Acquire) || len == 0 {
        control.invalid_free_errors += 1;
        return Err(1);
    }
    let Some(value) = control.validate(pointer, len) else {
        control.invalid_free_errors += 1;
        return Err(1);
    };
    match value.kind {
        1 => {
            control.double_free_errors += 1;
            return Err(2);
        }
        2 => {
            control.size_mismatch_errors += 1;
            return Err(3);
        }
        _ => {}
    }
    Ok(value)
}
fn free_locked(control: &mut Control, pointer: *mut u8, len: usize) -> u8 {
    let value = match validate_free(control, pointer, len) {
        Ok(value) => value,
        Err(error) => return error,
    };
    control.active_blocks -= 1;
    control.used_bytes -= value.requested;
    control.release_block(value.offset, value.size);
    0
}
/// Retain the allocator owner across rustc's void dealloc and the exact
/// resulting block state. No foreign allocation can reuse this address.
pub(super) unsafe fn rust_deallocate_probe(pointer: *mut u8, len: usize, alignment: usize) -> bool {
    let _owner = super::owner::Guard::acquire(4);
    if !control(|state| state.validate(pointer, len).is_some_and(|v| v.kind == 0)) {
        return false;
    }
    let Ok(layout) = core::alloc::Layout::from_size_align(len, alignment) else {
        return false;
    };
    unsafe {
        crate::alloc::alloc::dealloc(pointer, layout);
    }
    control(|state| {
        pointer.addr() as u64 >= BASE + state.heap_top as u64
            || state
                .classify((pointer.addr() as u64 - BASE) as usize - HEADER)
                .is_some_and(|v| v.kind == 1)
    })
}
/// The retained live allocation belongs exclusively to this diagnostic. Keep
/// both frees before tail uncommit or another allocator user can reuse it.
pub(super) unsafe fn duplicate_free_probe(pointer: *mut u8, len: usize) -> (bool, bool) {
    let Some(mut guard) = HeapGuard::enter() else {
        return (false, false);
    };
    let (released, rejected) = control(|control| {
        let before = control.double_free_errors;
        let released = free_locked(control, pointer, len) == 0;
        let rejected = released
            && before != u64::MAX
            && free_locked(control, pointer, len) == 2
            && before.checked_add(1) == Some(control.double_free_errors);
        (released, rejected)
    });
    if released {
        release_tail(&mut guard);
    }
    (released, rejected)
}
enum Resize {
    Done,
    Failed,
    Copy,
    Trim,
}
fn resize(pointer: *mut u8, len: usize, new_size: usize, alignment: usize) -> Resize {
    let Some(mut guard) = HeapGuard::enter() else {
        return Resize::Failed;
    };
    let allow_unlocked = guard.can_drop();
    let mut retiring = None;
    let result = control(|control| {
        let Some(value) = control.validate(pointer, len) else {
            control.invalid_free_errors += 1;
            return Resize::Failed;
        };
        match value.kind {
            1 => {
                control.invalid_free_errors += 1;
                return Resize::Failed;
            }
            2 => {
                control.size_mismatch_errors += 1;
                return Resize::Failed;
            }
            _ => {}
        }
        if pointer.addr() & (alignment - 1) != 0 {
            return Resize::Copy;
        }
        let Some(need) = block_size_for(new_size) else {
            control.allocation_errors += 1;
            control.oom_errors += 1;
            return Resize::Failed;
        };
        let offset = value.offset;
        let mut size = value.size;
        if need <= size {
            let trim = size - need >= MIN_BLOCK;
            if trim {
                write_used(offset, need, new_size);
                if allow_unlocked && size - need >= PAGE {
                    retiring = release::prepare(control, offset + need, size - need, None);
                }
                if retiring.is_none() {
                    control.release_block(offset + need, size - need);
                }
            } else {
                write_used(offset, size, new_size);
            }
            control.used_bytes = control.used_bytes - value.requested + new_size;
            return if trim { Resize::Trim } else { Resize::Done };
        }
        let next_offset = offset + size;
        if next_offset < control.heap_top {
            let next = read(next_offset);
            if next & 1 == 0 {
                let next_size = block_size(next);
                if size + next_size >= need {
                    control.unlink_free(next_offset, next_size);
                    size += next_size;
                    if size - need >= MIN_BLOCK {
                        write_used(offset, need, new_size);
                        control.insert_free(offset + need, size - need);
                    } else {
                        write_used(offset, size, new_size);
                    }
                    control.used_bytes = control.used_bytes - value.requested + new_size;
                    return Resize::Done;
                }
            }
        }
        Resize::Copy
    });
    if let Some(retiring) = retiring {
        release::finish(retiring, &mut guard);
    }
    if matches!(result, Resize::Trim) {
        release_tail(&mut guard);
    }
    result
}
/// # Safety
/// pointer/len are the caller's live retained allocation, readable for len
/// bytes until this operation returns. Ownership follows the original realloc contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_heap_realloc(
    pointer: *mut u8,
    len: usize,
    new_size: usize,
    alignment: usize,
) -> *mut u8 {
    if !INITIALIZED.load(Ordering::Acquire) {
        return null_mut();
    }
    if new_size == 0 {
        unsafe {
            r4os_rust_heap_free(pointer, len);
        }
        return null_mut();
    }
    if len == 0 {
        return r4os_rust_heap_alloc(new_size, alignment);
    }
    let Some(alignment) = normalize_alignment(alignment) else {
        return alloc_failure(false);
    };
    match resize(pointer, len, new_size, alignment) {
        Resize::Done | Resize::Trim => return pointer,
        Resize::Failed => return null_mut(),
        Resize::Copy => {}
    }
    let new_pointer = r4os_rust_heap_alloc(new_size, alignment);
    if new_pointer.is_null() {
        return null_mut();
    }
    unsafe {
        core::ptr::copy_nonoverlapping(pointer, new_pointer, len.min(new_size));
    }
    if unsafe { r4os_rust_heap_free(pointer, len) } != 0 {
        return null_mut();
    }
    new_pointer
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_growing() -> bool {
    GROWTH.load(Ordering::Acquire) != 0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
/// # Safety
/// output is exclusive C storage. The getter only supplies the resident
/// control identity; it does not lend a reference to the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_heap_metadata(output: *mut Metadata) {
    unsafe {
        output.write(Metadata {
            base: CONTROL.0.get() as u64,
            len: core::mem::size_of::<Control>() as u64,
        });
    }
}
/// # Safety
/// output is exclusive live Stats C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_heap_stats(output: *mut Stats) {
    let _owner = super::owner::Guard::acquire(4);
    let pressure = pressure();
    let stats = control(|control| {
        let largest = control.largest();
        Stats {
            base: BASE,
            pages: control.committed_pages,
            capacity_bytes: control.committed(),
            reserved_bytes: control.cap_pages * PAGE,
            committed_bytes: control.committed(),
            used_bytes: control.used_bytes,
            free_bytes: control.free_bytes,
            active_blocks: control.active_blocks,
            free_blocks: control.free_blocks,
            largest_free: largest,
            allocation_errors: control.allocation_errors,
            invalid_free_errors: control.invalid_free_errors,
            double_free_errors: control.double_free_errors,
            size_mismatch_errors: control.size_mismatch_errors,
            oom_errors: control.oom_errors,
            reentry_errors: control.reentry_errors,
            next_growth_pages: control.next_growth_pages,
            pending_uncommit_bytes: control.pending_uncommit_bytes,
            commit_calls: control.commit_calls,
            commit_failures: control.commit_failures,
            committed_pages_total: control.committed_pages_total,
            uncommit_calls: control.uncommit_calls,
            uncommit_failures: control.uncommit_failures,
            uncommitted_pages_total: control.uncommitted_pages_total,
            release_suppressed: control.release_suppressed,
            pressure_releases: control.pressure_releases,
            poison_bytes: control.poison_bytes,
            retained_tail_pages: policy::r4os_rust_heap_retained(
                control.next_growth_pages,
                control.cap_pages,
                pressure,
            ),
            fragmentation_hint: control.free_blocks > 1 && control.free_bytes > largest,
            bootstrap_bump_active: false,
        }
    });
    unsafe {
        output.write(stats);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_boot_invariant() -> bool {
    let _owner = super::owner::Guard::acquire(4);
    control(|control| {
        if !INITIALIZED.load(Ordering::Acquire)
            || control.range_id == 0
            || control.committed_pages < 1
            || control.committed_pages > control.cap_pages
            || control.heap_top != control.committed()
            || control.heap_top < MIN_BLOCK
            || control.active_blocks != 0
            || control.used_bytes != 0
            || control.free_blocks != 1
            || control.free_bytes != control.heap_top
        {
            return false;
        }
        let first = read(0);
        first & 1 == 0
            && block_size(first) == control.heap_top
            && read(8) == FREE_MAGIC
            && read(control.heap_top - FOOTER) == first
            && read(16) == NONE
            && read(24) == NONE
            && control.bins[bin(control.heap_top)] == 0
    })
}
#[cfg(not(test))]
struct KernelAllocator;
// Host owner fixtures never install a kernel allocator into the host runtime.
#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator;
#[cfg(not(test))]
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        r4os_rust_heap_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe {
            r4os_rust_heap_free(pointer, layout.size());
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 232);
    assert!(core::mem::size_of::<Metadata>() == 16);
    assert!(core::mem::size_of::<Unwind>() == 16);
};

unsafe fn owner_runtime_depth() -> u32 {
    unsafe { crate::arch::percpu::r4os_rust_percpu_depth().read() }
}

unsafe fn heap_can_wait() -> bool {
    let current = crate::sched::scheduler::current();
    if current.is_null() {
        return false;
    }
    let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
    let core = unsafe { crate::sched::task::scheduler_view(current) };
    core.preempt_depth == 0 && core.held_locks == 0
}

unsafe fn heap_commit(id: u32, offset: u64, bytes: u64) -> bool {
    crate::memory::vm_transactions::r4os_rust_vm_commit(id, offset, bytes) == 0
}

unsafe fn heap_uncommit(id: u32, offset: u64, bytes: u64) -> bool {
    crate::memory::vm_transactions::r4os_rust_vm_uncommit(id, offset, bytes) == 0
}

unsafe fn heap_reserve(base: u64, bytes: u64) -> u32 {
    if base != BASE {
        return 0;
    }
    let request = super::vm_ranges::Reserve {
        eviction_allowed: true,
        window: 1,
        kind: 2,
        owner: 0,
        owner_id: 0,
        name_ptr: b"kernel-heap".as_ptr(),
        name_len: 11,
        flags: 2,
        base,
        len: bytes,
    };
    let mut id = 0;
    if unsafe { super::vm_pager_api::r4os_rust_vm_reserve_fixed(&request, &mut id) } == 0 {
        id
    } else {
        0
    }
}

unsafe fn heap_fault(kind: u8, offset: u64) {
    use crate::boot::log_route as log;
    if kind == 0 {
        log::puts(b"HEAP REENTRY detected (non-preempt invariant violated)\r\n");
        return;
    }
    log::puts(b"HEAP CORRUPT kind=");
    log::puts(match kind {
        1 => b"unlink-header",
        2 => b"unlink-next",
        3 => b"unlink-prev",
        _ => b"unknown",
    });
    log::puts(b" off=0x");
    log::hex(offset, 12);
    log::puts(b"\r\n");
}

unsafe fn heap_wait_growth() -> bool {
    use crate::sched::{
        sync_types::{Text, WaitRequest},
        sync_wait,
    };
    let request = WaitRequest {
        timeout: u64::MAX,
        reason: Text::literal(b"heap-growth"),
        predicate: Some(growth_active),
        context: core::ptr::addr_of!(GROWTH_CONTEXT).cast_mut().cast(),
        release: None,
        release_context: core::ptr::null_mut(),
        flags: 0,
        consume_runtime: false,
    };
    (unsafe { sync_wait::r4os_rust_sync_wait(GROWTH_WAITERS.0.get(), &request) })
        == crate::sched::task::task_types::WaitResult::Signaled as u32
}

unsafe fn heap_wake_growth() {
    let _ = unsafe {
        crate::sched::sync_wait::r4os_rust_sync_queue(
            GROWTH_WAITERS.0.get(),
            1,
            core::ptr::null_mut(),
            0,
            core::ptr::null_mut(),
        )
    };
}

struct GrowthWaiters(UnsafeCell<crate::sched::sync_types::WaitQueue>);
// SAFETY: existing runtime admission owns enrollment/wakes; no queue reference
// crosses a wait or is shared with a second queue implementation.
unsafe impl Sync for GrowthWaiters {}
static GROWTH_WAITERS: GrowthWaiters =
    GrowthWaiters(UnsafeCell::new(crate::sched::sync_types::WaitQueue::EMPTY));
static GROWTH_CONTEXT: u8 = 0;
unsafe extern "C" fn growth_active(_: *mut core::ffi::c_void) -> bool {
    r4os_rust_heap_growing()
}
