//! Physical frame ownership follows the original bitmap/hint/extent policy.
//! The exact bitmap, counters and initialized state have one Rust owner.
use super::{blocks, map, owner::Guard};
use crate::boot::info;
use core::cell::UnsafeCell;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicU64, Ordering};
const FRAME: u64 = 4096;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Extent {
    pub base: u64,
    pub count: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub total_frames: u64,
    pub free_frames: u64,
    pub used_frames: u64,
    pub bitmap_bytes: u64,
    pub bitmap_base: u64,
    pub double_free_errors: u64,
    pub bad_free_errors: u64,
    pub extent_allocations: u64,
    pub extent_allocated_frames: u64,
    pub extent_max_frames: u64,
    pub extent_frees: u64,
    pub extent_freed_frames: u64,
}
struct State {
    bitmap: *mut u8,
    stats: Stats,
    next_hint: u64,
    initialized: bool,
    fault_reported: bool,
}
impl State {
    const EMPTY: Self = Self {
        bitmap: null_mut(),
        stats: Stats {
            total_frames: 0,
            free_frames: 0,
            used_frames: 0,
            bitmap_bytes: 0,
            bitmap_base: 0,
            double_free_errors: 0,
            bad_free_errors: 0,
            extent_allocations: 0,
            extent_allocated_frames: 0,
            extent_max_frames: 0,
            extent_frees: 0,
            extent_freed_frames: 0,
        },
        next_hint: 0,
        initialized: false,
        fault_reported: false,
    };
    fn used(&self, frame: u64) -> bool {
        unsafe { self.bitmap.add((frame / 8) as usize).read() & (1 << (frame % 8)) != 0 }
    }
    fn set_used(&mut self, frame: u64, used: bool) {
        let byte = unsafe { self.bitmap.add((frame / 8) as usize) };
        let mask = 1 << (frame % 8);
        unsafe {
            byte.write(if used {
                byte.read() | mask
            } else {
                byte.read() & !mask
            })
        };
    }
    fn mark_range(&mut self, base: u64, len: u64, used: bool) {
        if len == 0 {
            return;
        }
        let Some(last) = base.checked_add(len) else {
            return;
        };
        let first = if used {
            base & !4095
        } else {
            let Some(value) = align_up(base, FRAME) else {
                return;
            };
            value
        };
        let last = if used {
            let Some(value) = align_up(last, FRAME) else {
                return;
            };
            value
        } else {
            last & !4095
        };
        for frame in first / FRAME..(last / FRAME).min(self.stats.total_frames) {
            if self.used(frame) != used {
                self.set_used(frame, used);
                if used {
                    self.stats.free_frames -= 1;
                } else {
                    self.stats.free_frames += 1;
                }
            }
        }
    }
    fn words(&self) -> Option<*mut u64> {
        if self.bitmap.addr() & 7 == 0 {
            Some(self.bitmap.cast())
        } else {
            None
        }
    }
    fn alloc_one(&mut self) -> Option<u64> {
        if !self.initialized {
            return None;
        }
        let Some(words) = self.words() else {
            return self.alloc_linear();
        };
        let count = self.stats.bitmap_bytes.div_ceil(8);
        let mut scanned = 0;
        let mut index = self.next_hint / 64;
        let mut start = (self.next_hint % 64) as u32;
        while scanned <= count {
            if index >= count {
                index = 0;
                start = 0;
            }
            // The bitmap's final word is covered by its reserved physical
            // page even when the logical bitmap ends in a partial word.
            let pointer = unsafe { words.add(index as usize) };
            let mut word = unsafe { pointer.read() };
            if start != 0 {
                word |= (1u64 << start) - 1;
            }
            if word != u64::MAX {
                let bit = (!word).trailing_zeros();
                let frame = index * 64 + bit as u64;
                if frame == 0 {
                    start = 1;
                    scanned += 1;
                    continue;
                }
                if frame < self.stats.total_frames {
                    unsafe { pointer.write(pointer.read() | (1 << bit)) };
                    self.stats.free_frames -= 1;
                    self.next_hint = frame + 1;
                    return Some(frame * FRAME);
                }
            }
            index += 1;
            start = 0;
            scanned += 1;
        }
        None
    }
    fn alloc_linear(&mut self) -> Option<u64> {
        let mut frame = self.next_hint;
        for _ in 0..self.stats.total_frames {
            if frame >= self.stats.total_frames {
                frame = 0;
            }
            if frame == 0 {
                frame = 1;
                continue;
            }
            if !self.used(frame) {
                self.set_used(frame, true);
                self.stats.free_frames -= 1;
                self.next_hint = frame + 1;
                return Some(frame * FRAME);
            }
            frame += 1;
        }
        None
    }
    fn alloc_contiguous(&mut self, count: u64, maximum: u64) -> Option<u64> {
        if !self.initialized || count == 0 {
            return None;
        }
        let addressable = if maximum == u64::MAX {
            self.stats.total_frames
        } else {
            self.stats.total_frames.min((maximum + 1) / FRAME)
        };
        if count > addressable {
            return None;
        }
        let words = self.words();
        let mut start = 1;
        while start <= addressable - count {
            if let Some(words) = words
                && self.used(start)
                && start & 63 == 0
                && unsafe { words.add((start / 64) as usize).read() } == u64::MAX
            {
                start += 64;
                continue;
            }
            let mut offset = 0;
            while offset < count && !self.used(start + offset) {
                offset += 1;
            }
            if offset == count {
                for offset in 0..count {
                    self.set_used(start + offset, true);
                }
                self.stats.free_frames -= count;
                self.next_hint = start + count;
                return Some(start * FRAME);
            }
            start += offset + 1;
        }
        None
    }
    fn free_contiguous(&mut self, address: u64, count: u64) {
        if !self.initialized || !address.is_multiple_of(FRAME) || count == 0 {
            return;
        }
        let start = address / FRAME;
        if start + count > self.stats.total_frames {
            return;
        }
        for offset in 0..count {
            let frame = start + offset;
            if self.used(frame) {
                self.set_used(frame, false);
                self.stats.free_frames += 1;
            }
        }
        if start < self.next_hint {
            self.next_hint = start;
        }
    }
    fn fault(&mut self, kind: u8) -> u8 {
        if self.fault_reported {
            return 0;
        }
        self.fault_reported = true;
        kind
    }
    fn free_one(&mut self, address: u64) -> u8 {
        if !self.initialized {
            return 0;
        }
        if !address.is_multiple_of(FRAME) {
            self.stats.bad_free_errors = self.stats.bad_free_errors.wrapping_add(1);
            return self.fault(1);
        }
        let frame = address / FRAME;
        if frame >= self.stats.total_frames {
            self.stats.bad_free_errors = self.stats.bad_free_errors.wrapping_add(1);
            return self.fault(2);
        }
        if !self.used(frame) {
            self.stats.double_free_errors = self.stats.double_free_errors.wrapping_add(1);
            return self.fault(3);
        }
        self.set_used(frame, false);
        self.stats.free_frames += 1;
        if frame < self.next_hint {
            self.next_hint = frame;
        }
        0
    }
}
struct Storage(UnsafeCell<State>);
// SAFETY: State and all bitmap bytes are accessed only under the exact
// physical-memory owner. No borrow survives a callback or owner handoff.
unsafe impl Sync for Storage {}
static PMM: Storage = Storage(UnsafeCell::new(State::EMPTY));
// Address conversion remains callable without the mutable PMM boundary.
static HHDM: AtomicU64 = AtomicU64::new(0);

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    Some(value.checked_add(alignment - 1)? & !(alignment - 1))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_phys_init() -> bool {
    let _guard = Guard::acquire(7);
    let current = unsafe { &*info::r4os_rust_boot_info() };
    if current.memory_count == 0 || current.flags & 4 == 0 {
        return false;
    }
    HHDM.store(current.hhdm, Ordering::Relaxed);
    let entries = unsafe { core::slice::from_raw_parts(current.memory, current.memory_count) };
    let summary = map::summarize(entries);
    if summary.largest_usable_len == 0 {
        return false;
    }
    let highest = entries
        .iter()
        .filter(|e| e.kind == 0 && e.usable_len != 0)
        .map(|e| e.usable_base + e.usable_len)
        .max()
        .unwrap_or(0);
    let Some(top) = align_up(highest, FRAME) else {
        return false;
    };
    let frames = top / FRAME;
    let Some(bitmap_bits) = align_up(frames, 8) else {
        return false;
    };
    let bytes = bitmap_bits / 8;
    let base = summary.largest_usable_base;
    let Some(end) = base.checked_add(bytes) else {
        return false;
    };
    let Some(largest_end) = base.checked_add(summary.largest_usable_len) else {
        return false;
    };
    if end > largest_end {
        return false;
    }
    let bitmap = r4os_rust_phys_to_virt(base) as *mut u8;
    {
        let state = unsafe { &mut *PMM.0.get() };
        *state = State::EMPTY;
        state.bitmap = bitmap;
        state.stats.total_frames = frames;
        state.stats.bitmap_bytes = bytes;
        unsafe { core::ptr::write_bytes(bitmap, 0xff, bytes as usize) };
        for entry in entries {
            if entry.kind == 0 {
                state.mark_range(entry.usable_base, entry.usable_len, false);
            }
        }
        state.mark_range(0, FRAME, true);
        state.mark_range(base, bytes, true);
    }
    let Some(claim_end) = align_up(end, FRAME) else {
        return false;
    };
    let claim_base = base & !4095;
    // The name and enum values are resident kernel constants; the bitmap
    // reservation persists for the physical allocator's lifetime.
    if unsafe {
        blocks::claim(blocks::Block::physical(
            claim_base,
            claim_end - claim_base,
            1,
            0,
            0,
            2,
            b"pmm-bitmap",
        ))
    }
    .is_err()
    {
        return false;
    }
    unsafe { (*PMM.0.get()).initialized = true };
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// output owns one writable u64; the physical owner keeps all bitmap storage.
pub unsafe extern "C" fn r4os_rust_phys_alloc(output: *mut u64) -> bool {
    let _guard = Guard::acquire(7);
    let value = unsafe { (&mut *PMM.0.get()).alloc_one() };
    if let Some(value) = value {
        unsafe { output.write(value) };
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// output owns one writable C Extent until return.
pub unsafe extern "C" fn r4os_rust_phys_extent(maximum_count: u64, output: *mut Extent) -> bool {
    let _outer = Guard::acquire(7);
    if maximum_count == 0 {
        return false;
    }
    let _nested = Guard::acquire(7); // preserve original reentrant admission/accounting
    let Some(base) = (unsafe { (&mut *PMM.0.get()).alloc_one() }) else {
        return false;
    };
    // Original allocFrame releases its nested token before the extension.
    drop(_nested);
    let state = unsafe { &mut *PMM.0.get() };
    let start = base / FRAME;
    let mut count = 1;
    while count < maximum_count
        && start + count < state.stats.total_frames
        && !state.used(start + count)
    {
        state.set_used(start + count, true);
        state.stats.free_frames -= 1;
        count += 1;
    }
    state.next_hint = start + count;
    state.stats.extent_allocations = state.stats.extent_allocations.wrapping_add(1);
    state.stats.extent_allocated_frames = state.stats.extent_allocated_frames.wrapping_add(count);
    state.stats.extent_max_frames = state.stats.extent_max_frames.max(count);
    unsafe { output.write(Extent { base, count }) };
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// output owns one writable u64 until return.
pub unsafe extern "C" fn r4os_rust_phys_contiguous(
    count: u64,
    maximum: u64,
    nested: bool,
    output: *mut u64,
) -> bool {
    let _outer = Guard::acquire(7);
    let _nested = if nested {
        Some(Guard::acquire(7))
    } else {
        None
    };
    let value = unsafe { (&mut *PMM.0.get()).alloc_contiguous(count, maximum) };
    if let Some(value) = value {
        unsafe { output.write(value) };
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_phys_free_contiguous(address: u64, count: u64) {
    let _guard = Guard::acquire(7);
    unsafe { (&mut *PMM.0.get()).free_contiguous(address, count) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_phys_free_extent(address: u64, count: u64) {
    let _outer = Guard::acquire(7);
    if count == 0 {
        return;
    }
    {
        let _nested = Guard::acquire(7);
        unsafe { (&mut *PMM.0.get()).free_contiguous(address, count) };
    }
    let state = unsafe { &mut *PMM.0.get() };
    state.stats.extent_frees = state.stats.extent_frees.wrapping_add(1);
    state.stats.extent_freed_frames = state.stats.extent_freed_frames.wrapping_add(count);
}
/// A prepared VM release retains the physical catalogue owner across its TLB
/// acknowledgement. Validate the entire bounded set before changing any bit;
/// a rejected set keeps every frame and reports an operational error.
pub(super) fn validate_release_extents(extents: &[Extent]) -> bool {
    let _owner = Guard::acquire(7);
    release_extents_valid(unsafe { &mut *PMM.0.get() }, extents)
}
fn release_extents_valid(state: &mut State, extents: &[Extent]) -> bool {
    if !state.initialized {
        return false;
    }
    let mut pages = 0;
    if extents.is_empty() || extents.len() > super::page_batch::MAX_EXTENT_PAGES as usize {
        state.stats.bad_free_errors = state.stats.bad_free_errors.wrapping_add(1);
        return false;
    }
    for (index, extent) in extents.iter().copied().enumerate() {
        if extent.base == 0
            || !extent.base.is_multiple_of(FRAME)
            || extent.count == 0
            || extent.count > super::page_batch::MAX_EXTENT_PAGES
        {
            state.stats.bad_free_errors = state.stats.bad_free_errors.wrapping_add(1);
            return false;
        }
        pages += extent.count;
        let first = extent.base / FRAME;
        let Some(end) = first.checked_add(extent.count) else {
            state.stats.bad_free_errors = state.stats.bad_free_errors.wrapping_add(1);
            return false;
        };
        if pages > super::page_batch::MAX_EXTENT_PAGES || end > state.stats.total_frames {
            state.stats.bad_free_errors = state.stats.bad_free_errors.wrapping_add(1);
            return false;
        }
        for earlier in &extents[..index] {
            let earlier_first = earlier.base / FRAME;
            let earlier_end = earlier_first + earlier.count;
            if first < earlier_end && earlier_first < end {
                state.stats.double_free_errors = state.stats.double_free_errors.wrapping_add(1);
                return false;
            }
        }
        for frame in first..end {
            if !state.used(frame) {
                state.stats.double_free_errors = state.stats.double_free_errors.wrapping_add(1);
                return false;
            }
        }
    }
    true
}
pub(super) fn release_extents_checked(extents: &[Extent]) -> bool {
    let _owner = Guard::acquire(7);
    let state = unsafe { &mut *PMM.0.get() };
    if !release_extents_valid(state, extents) {
        return false;
    }
    for extent in extents {
        state.free_contiguous(extent.base, extent.count);
        state.stats.extent_frees = state.stats.extent_frees.wrapping_add(1);
        state.stats.extent_freed_frames =
            state.stats.extent_freed_frames.wrapping_add(extent.count);
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_phys_free(address: u64) {
    let _guard = Guard::acquire(7);
    let fault = unsafe { (&mut *PMM.0.get()).free_one(address) };
    if fault != 0 {
        unsafe { pmm_fault(fault, address) };
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// output owns one writable, aligned C Stats until return.
pub unsafe extern "C" fn r4os_rust_phys_stats(output: *mut Stats) {
    let _guard = Guard::acquire(7);
    let state = unsafe { &*PMM.0.get() };
    let mut result = state.stats;
    result.used_frames = result.total_frames - result.free_frames;
    result.bitmap_base = if state.initialized {
        r4os_rust_virt_to_phys(state.bitmap.addr() as u64)
    } else {
        0
    };
    unsafe { output.write(result) };
}
/// Current allocator truth, without projecting the diagnostic Stats record.
pub(crate) fn free_frames() -> u64 {
    let _guard = Guard::acquire(7);
    unsafe { (&*PMM.0.get()).stats.free_frames }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_phys_to_virt(address: u64) -> u64 {
    address
        .checked_add(HHDM.load(Ordering::Relaxed))
        .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_virt_to_phys(address: u64) -> u64 {
    address.saturating_sub(HHDM.load(Ordering::Relaxed))
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 96 && core::mem::size_of::<Extent>() == 16);
};

unsafe fn pmm_fault(kind: u8, address: u64) {
    use crate::boot::log_route as log;
    log::puts(b"PMM FAULT ");
    log::puts(match kind {
        1 => b"BAD-FREE-ALIGN",
        2 => b"BAD-FREE-RANGE",
        3 => b"DOUBLE-FREE",
        _ => b"UNKNOWN",
    });
    log::puts(b" frame=0x");
    log::hex(address, 16);
    log::puts(b"\r\n");
}
