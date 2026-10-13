//! Ordered logical-commit spans, their fixed pool, split reservations,
//! rollback anchors and operational counters. No waiting or backing I/O.
use super::vm_ranges::{self, OptionalSlot};
use core::cell::UnsafeCell;
const CAPACITY: usize = 4096;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Span {
    pub slot_used: bool,
    pub range_id: u32,
    pub base: u64,
    pub len: u64,
    pub prev: OptionalSlot,
    pub next: OptionalSlot,
}
impl Span {
    const EMPTY: Self = Self {
        slot_used: false,
        range_id: 0,
        base: 0,
        len: 0,
        prev: OptionalSlot::NONE,
        next: OptionalSlot::NONE,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub commit_span_active: u32,
    pub commit_span_lookups: u64,
    pub commit_span_steps: u64,
    pub commit_span_step_max: u32,
}
impl Stats {
    const EMPTY: Self = Self {
        commit_span_active: 0,
        commit_span_lookups: 0,
        commit_span_steps: 0,
        commit_span_step_max: 0,
    };
}
struct State {
    spans: [Span; CAPACITY],
    free: Option<u16>,
    active: u32,
    stats: Stats,
}
impl State {
    const EMPTY: Self = Self {
        spans: [Span::EMPTY; CAPACITY],
        free: None,
        active: 0,
        stats: Stats::EMPTY,
    };
    fn reset(&mut self) {
        self.free = None;
        self.active = 0;
        self.stats = Stats::EMPTY;
        for (index, span) in self.spans.iter_mut().enumerate() {
            *span = Span {
                next: OptionalSlot::from(self.free),
                ..Span::EMPTY
            };
            self.free = Some(index as u16);
        }
    }
    fn allocated(&mut self) -> Option<u16> {
        let slot = self.free?;
        self.free = self.spans[usize::from(slot)].next.get();
        self.spans[usize::from(slot)] = Span::EMPTY;
        self.active = self.active.saturating_add(1);
        Some(slot)
    }
    fn release(&mut self, slot: u16) {
        self.spans[usize::from(slot)] = Span {
            next: OptionalSlot::from(self.free),
            ..Span::EMPTY
        };
        self.free = Some(slot);
        if self.active != 0 {
            self.active -= 1;
        }
    }
    fn lookup(&mut self) {
        self.stats.commit_span_lookups = self.stats.commit_span_lookups.wrapping_add(1);
    }
    fn steps(&mut self, steps: usize) {
        let value = steps.min(u32::MAX as usize) as u32;
        self.stats.commit_span_steps = self.stats.commit_span_steps.wrapping_add(u64::from(value));
        self.stats.commit_span_step_max = self.stats.commit_span_step_max.max(value);
    }
    fn unlink(&mut self, range_index: usize, slot: u16) {
        let span = self.spans[usize::from(slot)];
        if let Some(previous) = span.prev.get() {
            self.spans[usize::from(previous)].next = span.next;
        } else {
            vm_ranges::update(range_index, |range| range.commit_span_head = span.next);
        }
        if let Some(next) = span.next.get() {
            self.spans[usize::from(next)].prev = span.prev;
        }
    }
    fn add(&mut self, range_id: u32, base: u64, len: u64) -> Result<(), u32> {
        let (range_index, mut current) = range_head(range_id)?;
        let end = base.checked_add(len).ok_or(5u32)?;
        self.lookup();
        let mut previous = None;
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            if self.spans[usize::from(slot)].base >= base {
                break;
            }
            previous = Some(slot);
            current = self.spans[usize::from(slot)].next.get();
        }
        self.steps(steps);
        if let Some(slot) = previous {
            let span = self.spans[usize::from(slot)];
            let previous_end = span.base.checked_add(span.len).ok_or(5u32)?;
            if previous_end > base {
                return Err(7);
            }
            if previous_end == base {
                if let Some(next_slot) = current {
                    let next = self.spans[usize::from(next_slot)];
                    if next.base < end {
                        return Err(7);
                    }
                    if next.base == end {
                        let next_end = next.base.checked_add(next.len).ok_or(5u32)?;
                        self.spans[usize::from(slot)].len = next_end - span.base;
                        self.unlink(range_index, next_slot);
                        self.release(next_slot);
                        return Ok(());
                    }
                }
                self.spans[usize::from(slot)].len = end - span.base;
                return Ok(());
            }
        }
        if let Some(slot) = current {
            let span = self.spans[usize::from(slot)];
            if span.base < end {
                return Err(7);
            }
            if span.base == end {
                let next_end = span.base.checked_add(span.len).ok_or(5u32)?;
                self.spans[usize::from(slot)].base = base;
                self.spans[usize::from(slot)].len = next_end - base;
                return Ok(());
            }
        }
        let slot = self.allocated().ok_or(2u32)?;
        self.spans[usize::from(slot)] = Span {
            slot_used: true,
            range_id,
            base,
            len,
            prev: OptionalSlot::from(previous),
            next: OptionalSlot::from(current),
        };
        if let Some(previous) = previous {
            self.spans[usize::from(previous)].next = OptionalSlot::from(Some(slot));
        } else {
            vm_ranges::update(range_index, |range| {
                range.commit_span_head = OptionalSlot::from(Some(slot))
            });
        }
        if let Some(next) = current {
            self.spans[usize::from(next)].prev = OptionalSlot::from(Some(slot));
        }
        Ok(())
    }
    fn needs_slot(&mut self, range_id: u32, base: u64, len: u64) -> Result<bool, u32> {
        let (_, mut current) = range_head(range_id)?;
        let end = base.checked_add(len).ok_or(5u32)?;
        self.lookup();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            if span.base >= end {
                break;
            }
            let span_end = span.base.checked_add(span.len).ok_or(5u32)?;
            if !(base < span_end && span.base < end) {
                current = span.next.get();
                continue;
            }
            if span.base < base && span_end > end {
                self.steps(steps);
                return Ok(true);
            }
            current = span.next.get();
        }
        self.steps(steps);
        Ok(false)
    }
    fn reserve_split(&mut self, range_id: u32, base: u64, len: u64) -> Result<Option<u16>, u32> {
        if !self.needs_slot(range_id, base, len)? {
            return Ok(None);
        }
        self.allocated().map(Some).ok_or(2)
    }
    fn remove(
        &mut self,
        range_id: u32,
        base: u64,
        len: u64,
        reserved: &mut Option<u16>,
    ) -> Result<(), u32> {
        let (range_index, mut current) = range_head(range_id)?;
        let end = base.checked_add(len).ok_or(5u32)?;
        self.lookup();
        let mut steps = 0;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            if span.base >= end {
                break;
            }
            let span_end = span.base.checked_add(span.len).ok_or(5u32)?;
            if span_end <= base {
                current = span.next.get();
                continue;
            }
            let keep_left = span.base < base;
            let keep_right = span_end > end;
            if keep_left && keep_right {
                let right_slot = reserved.take().ok_or(2u32)?;
                self.spans[usize::from(slot)].len = base - span.base;
                self.spans[usize::from(right_slot)] = Span {
                    slot_used: true,
                    range_id,
                    base: end,
                    len: span_end - end,
                    prev: OptionalSlot::from(Some(slot)),
                    next: span.next,
                };
                self.spans[usize::from(slot)].next = OptionalSlot::from(Some(right_slot));
                if let Some(next) = span.next.get() {
                    self.spans[usize::from(next)].prev = OptionalSlot::from(Some(right_slot));
                }
                break;
            } else if keep_left {
                self.spans[usize::from(slot)].len = base - span.base;
            } else if keep_right {
                self.spans[usize::from(slot)].base = end;
                self.spans[usize::from(slot)].len = span_end - end;
                break;
            } else {
                self.unlink(range_index, slot);
                self.release(slot);
            }
            current = span.next.get();
        }
        self.steps(steps);
        Ok(())
    }
    fn remove_all(&mut self, range_id: u32) {
        let Ok((range_index, mut current)) = range_head(range_id) else {
            return;
        };
        while let Some(slot) = current {
            let next = self.spans[usize::from(slot)].next.get();
            self.release(slot);
            current = next;
        }
        vm_ranges::update(range_index, |range| {
            range.commit_span_head = OptionalSlot::NONE
        });
    }
    fn scan(&mut self, mut current: Option<u16>, base: u64, len: u64, covers: bool) -> bool {
        let Some(end) = base.checked_add(len) else {
            return false;
        };
        self.lookup();
        let mut steps = 0;
        let mut cursor = base;
        while let Some(slot) = current {
            steps += 1;
            let span = self.spans[usize::from(slot)];
            if !covers && span.base >= end {
                break;
            }
            let Some(span_end) = span.base.checked_add(span.len) else {
                self.steps(steps);
                return !covers;
            };
            if covers {
                if span_end <= cursor {
                    current = span.next.get();
                    continue;
                }
                if span.base > cursor {
                    break;
                }
                cursor = span_end.min(end);
                if cursor == end {
                    self.steps(steps);
                    return true;
                }
            } else if span_end > base {
                self.steps(steps);
                return true;
            }
            current = span.next.get();
        }
        self.steps(steps);
        false
    }
}
fn range_head(id: u32) -> Result<(usize, Option<u16>), u32> {
    let index = unsafe { super::vm_index::r4os_rust_vm_index_query(1, u64::from(id)) };
    if index == usize::MAX {
        return Err(8);
    }
    Ok((index, vm_ranges::read(index).commit_span_head.get()))
}
struct Storage(UnsafeCell<State>);
// SAFETY: singleton BSP initializes first; all later C and native operations
// retain the VM owner. The legacy pager receives only a raw read view.
unsafe impl Sync for Storage {}
static SPANS: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn state<T>(operation: impl FnOnce(&mut State) -> T) -> T {
    operation(unsafe { &mut *SPANS.0.get() })
}
/// # Safety
/// Singleton BSP initialization before readers are admitted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_commit_spans_init() -> *const Span {
    state(|state| state.reset());
    unsafe { core::ptr::addr_of!((*SPANS.0.get()).spans).cast() }
}
/// # Safety
/// Caller retains VM owner. op 1/3 requires exclusive live OptionalSlot storage;
/// op 1 contains its exact reserved split ticket. op 4's base is a valid slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_commit_span_change(
    operation: u8,
    range_id: u32,
    base: u64,
    len: u64,
    reserved: *mut OptionalSlot,
) -> u32 {
    let result = state(|state| match operation {
        0 => state.add(range_id, base, len),
        1 => {
            let mut ticket = unsafe { reserved.read() }.get();
            let result = state.remove(range_id, base, len, &mut ticket);
            unsafe {
                reserved.write(OptionalSlot::from(ticket));
            }
            result
        }
        2 => {
            state.remove_all(range_id);
            Ok(())
        }
        3 => {
            let ticket = state.reserve_split(range_id, base, len)?;
            unsafe {
                reserved.write(OptionalSlot::from(ticket));
            }
            Ok(())
        }
        4 => {
            assert!(base < CAPACITY as u64);
            state.release(base as u16);
            Ok(())
        }
        _ => unreachable!(),
    });
    match result {
        Ok(()) => 0,
        Err(code) => code,
    }
}
/// # Safety
/// Caller retains VM owner; head belongs to a current range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_commit_span_scan(
    head: OptionalSlot,
    base: u64,
    len: u64,
    covers: bool,
) -> bool {
    state(|state| state.scan(head.get(), base, len, covers))
}
/// # Safety
/// Caller retains VM owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_commit_span_cursor(free: bool) -> usize {
    state(|state| {
        if free {
            state.free.map_or(usize::MAX, usize::from)
        } else {
            state.active as usize
        }
    })
}
/// # Safety
/// Caller retains VM owner; output is exclusive live C stats storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_commit_span_stats(output: *mut Stats) {
    let stats = state(|state| Stats {
        commit_span_active: state.active,
        ..state.stats
    });
    unsafe {
        output.write(stats);
    }
}
const _: () = {
    assert!(core::mem::size_of::<Span>() == 32);
    assert!(core::mem::offset_of!(Span, prev) == 24);
    assert!(core::mem::size_of::<Stats>() == 32);
};
#[cfg(r4os_owner_fixture)]
#[path = "../../Tests/RustOwners/vm_metadata_cases.rs"]
mod owner_cases;
