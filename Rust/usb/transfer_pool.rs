//! Generation-bound xHCI transfer ledger. Timed-out work retains its exact
//! match until endpoint cancellation/recovery; completion is never duplicated.
use super::event_router::{self, Event, Match};
pub const MAX_TRANSFERS: usize = 32;
const FREE: u8 = 0;
const SUBMITTED: u8 = 1;
const COMPLETED: u8 = 2;
const TIMED_OUT: u8 = 3;
const CANCELLED: u8 = 4;
const FAILED: u8 = 5;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub generation: u16,
    pub state: u8,
    pub kind: u8,
    pub expected: Match,
    pub timeout_token: u64,
    pub completion: Event,
}
impl Entry {
    pub const EMPTY: Self = Self {
        generation: 0,
        state: FREE,
        kind: 0,
        expected: Match::EMPTY,
        timeout_token: 0,
        completion: Event::EMPTY,
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub active: u32,
    pub high_water: u32,
    pub submitted: u64,
    pub completed: u64,
    pub timed_out: u64,
    pub cancelled: u64,
    pub failed: u64,
    pub stale_handles: u64,
}
#[repr(C)]
pub struct Pool {
    pub entries: [Entry; MAX_TRANSFERS],
    pub next_generation: u16,
    pub active: u32,
    pub high_water: u32,
    pub submitted: u64,
    pub completed: u64,
    pub timed_out: u64,
    pub cancelled: u64,
    pub failed: u64,
    pub stale_handles: u64,
}
fn encode_handle(index: usize, generation: u16) -> u32 {
    (u32::from(generation) << 8) | u32::try_from(index + 1).expect("USB transfer slot")
}
fn decode_handle(handle: u32) -> Option<(usize, u16)> {
    let index = handle & 255;
    let generation = (handle >> 8) as u16;
    if index == 0 || index > MAX_TRANSFERS as u32 || generation == 0 {
        None
    } else {
        Some(((index - 1) as usize, generation))
    }
}
fn live(state: u8) -> bool {
    state == SUBMITTED || state == TIMED_OUT
}
impl Pool {
    pub const EMPTY: Self = Self {
        entries: [Entry::EMPTY; MAX_TRANSFERS],
        next_generation: 1,
        active: 0,
        high_water: 0,
        submitted: 0,
        completed: 0,
        timed_out: 0,
        cancelled: 0,
        failed: 0,
        stale_handles: 0,
    };
    pub fn begin(&mut self, kind: u8, expected: Match, timeout_token: u64) -> Option<u32> {
        let index = self.entries.iter().position(|entry| entry.state == FREE)?;
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        if self.next_generation == 0 {
            self.next_generation = 1;
        }
        self.entries[index] = Entry {
            generation,
            state: SUBMITTED,
            kind,
            expected,
            timeout_token,
            ..Entry::EMPTY
        };
        self.active += 1;
        self.submitted = self.submitted.wrapping_add(1);
        self.high_water = self.high_water.max(self.active);
        Some(encode_handle(index, generation))
    }
    pub fn matching_handle(&self, event: Event) -> Option<u32> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, entry)| live(entry.state) && event_router::matches(event, entry.expected))
            .map(|(index, entry)| encode_handle(index, entry.generation))
    }
    fn entry_index(&mut self, handle: u32) -> Option<usize> {
        if let Some((index, generation)) = decode_handle(handle) {
            let entry = self.entries[index];
            if entry.generation == generation && entry.state != FREE {
                return Some(index);
            }
        }
        self.stale_handles = self.stale_handles.wrapping_add(1);
        None
    }
    pub fn complete(&mut self, handle: u32, event: Event) -> bool {
        let Some(index) = self.entry_index(handle) else {
            return false;
        };
        let entry = &mut self.entries[index];
        if entry.state != SUBMITTED || !event_router::matches(event, entry.expected) {
            return false;
        }
        entry.completion = event;
        entry.state = if event.code == 1 || event.code == 13 {
            COMPLETED
        } else {
            FAILED
        };
        self.active -= 1;
        if entry.state == COMPLETED {
            self.completed = self.completed.wrapping_add(1);
        } else {
            self.failed = self.failed.wrapping_add(1);
        }
        true
    }
    pub fn mark_timeout(&mut self, handle: u32) -> bool {
        let Some(index) = self.entry_index(handle) else {
            return false;
        };
        if self.entries[index].state != SUBMITTED {
            return false;
        }
        self.entries[index].state = TIMED_OUT;
        self.active -= 1;
        self.timed_out = self.timed_out.wrapping_add(1);
        true
    }
    pub fn cancel(&mut self, handle: u32) -> bool {
        let Some(index) = self.entry_index(handle) else {
            return false;
        };
        let entry = &mut self.entries[index];
        if !live(entry.state) {
            return false;
        }
        if entry.state == SUBMITTED {
            self.active -= 1;
        }
        entry.state = CANCELLED;
        self.cancelled = self.cancelled.wrapping_add(1);
        true
    }
    pub fn release(&mut self, handle: u32) -> bool {
        let Some(index) = self.entry_index(handle) else {
            return false;
        };
        if self.entries[index].state == SUBMITTED {
            return false;
        }
        self.entries[index] = Entry::EMPTY;
        true
    }
    fn entry_for(&self, handle: u32) -> Option<Entry> {
        let (index, generation) = decode_handle(handle)?;
        let entry = self.entries[index];
        (entry.generation == generation && entry.state != FREE).then_some(entry)
    }
    pub fn match_for_handle(&self, handle: u32) -> Option<Match> {
        self.entry_for(handle).map(|entry| entry.expected)
    }
    pub fn state_for_handle(&self, handle: u32) -> Option<u8> {
        self.entry_for(handle).map(|entry| entry.state)
    }
    pub fn timed_out_handle(&self, slot: u8, endpoint: u8) -> Option<u32> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, e)| {
                e.state == TIMED_OUT
                    && e.expected.tag == 0
                    && e.expected.slot_id == slot
                    && e.expected.endpoint_id == endpoint
            })
            .map(|(index, e)| encode_handle(index, e.generation))
    }
    pub fn cancel_endpoint(&mut self, slot: u8, endpoint: u8) -> u32 {
        self.purge_where(|expected| {
            expected.tag == 0 && expected.slot_id == slot && expected.endpoint_id == endpoint
        })
    }
    pub fn purge_slot(&mut self, slot: u8) -> u32 {
        self.purge_where(|expected| expected.tag == 0 && expected.slot_id == slot)
    }
    fn purge_where(&mut self, predicate: impl Fn(Match) -> bool) -> u32 {
        let mut count = 0;
        for entry in &mut self.entries {
            if entry.state == FREE || !predicate(entry.expected) {
                continue;
            }
            if entry.state == SUBMITTED {
                self.active -= 1;
            }
            if live(entry.state) {
                self.cancelled = self.cancelled.wrapping_add(1);
            }
            *entry = Entry::EMPTY;
            count += 1;
        }
        count
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            active: self.active,
            high_water: self.high_water,
            submitted: self.submitted,
            completed: self.completed,
            timed_out: self.timed_out,
            cancelled: self.cancelled,
            failed: self.failed,
            stale_handles: self.stale_handles,
        }
    }
}

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_reset(pool: *mut Pool) {
    unsafe { pool.write(Pool::EMPTY) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_begin(
    pool: *mut Pool,
    kind: u8,
    expected: *const Match,
    timeout: u64,
) -> u32 {
    unsafe { (&mut *pool).begin(kind, *expected, timeout) }.unwrap_or(0)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_owns(pool: *const Pool, event: *const Event) -> bool {
    unsafe { (*pool).matching_handle(*event) }.is_some()
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_matching(
    pool: *const Pool,
    event: *const Event,
) -> u32 {
    unsafe { (*pool).matching_handle(*event) }.unwrap_or(0)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_complete(
    pool: *mut Pool,
    handle: u32,
    event: *const Event,
) -> bool {
    unsafe { (&mut *pool).complete(handle, *event) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_timeout(pool: *mut Pool, handle: u32) -> bool {
    unsafe { (&mut *pool).mark_timeout(handle) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_cancel(pool: *mut Pool, handle: u32) -> bool {
    unsafe { (&mut *pool).cancel(handle) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_release(pool: *mut Pool, handle: u32) -> bool {
    unsafe { (&mut *pool).release(handle) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_match(
    pool: *const Pool,
    handle: u32,
    out: *mut Match,
) -> bool {
    if let Some(value) = unsafe { (*pool).match_for_handle(handle) } {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_state(pool: *const Pool, handle: u32) -> u8 {
    unsafe { (*pool).state_for_handle(handle) }.unwrap_or(255)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_timed_out(
    pool: *const Pool,
    slot: u8,
    endpoint: u8,
) -> u32 {
    unsafe { (*pool).timed_out_handle(slot, endpoint) }.unwrap_or(0)
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_cancel_endpoint(
    pool: *mut Pool,
    slot: u8,
    endpoint: u8,
) -> u32 {
    unsafe { (&mut *pool).cancel_endpoint(slot, endpoint) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_purge_slot(pool: *mut Pool, slot: u8) -> u32 {
    unsafe { (&mut *pool).purge_slot(slot) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_pool_snapshot(pool: *const Pool, out: *mut Snapshot) {
    unsafe { out.write((*pool).snapshot()) }
}
const _: () = {
    assert!(core::mem::size_of::<Entry>() == 304);
    assert!(core::mem::offset_of!(Entry, expected) == 8);
    assert!(core::mem::offset_of!(Entry, completion) == 280);
    assert!(core::mem::size_of::<Pool>() == 9792);
    assert!(core::mem::offset_of!(Pool, next_generation) == 9728);
    assert!(core::mem::offset_of!(Pool, submitted) == 9744);
    assert!(core::mem::size_of::<Snapshot>() == 56);
};
