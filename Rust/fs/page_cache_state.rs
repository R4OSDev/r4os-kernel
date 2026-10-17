//! Original resident cache state and exact metadata/pin ownership.
use super::{page_cache_batch as batch, page_cache_policy as policy, page_cache_stats::Counters};
use crate::{
    arch::pit::DEFAULT_HZ,
    memory::{heap, phys},
    platform::timer,
    sched::{
        scheduler, sync_locks, sync_objects,
        sync_types::{Event, Mutex, Text},
        task_context,
    },
};
use core::{cell::UnsafeCell, ptr, sync::atomic::Ordering::Relaxed};
pub(super) const SECTOR: usize = 512;
pub(super) const PAGE: usize = 4096;
pub(super) const COUNT: usize = 512;
pub(super) const DEVICES: usize = 8;
pub(super) const NONE: u16 = 0xffff;
pub(super) const FULL: u8 = 0xff;
pub(super) const LOCK_TICKS: u64 = 3600 * DEFAULT_HZ as u64;
pub(super) const BUSY_TICKS: usize = 5 * DEFAULT_HZ as usize;
pub(super) const MAX_DIRTY_AGE: u64 = 2 * DEFAULT_HZ as u64;
pub(super) const SPECULATION: bool = false;
#[derive(Clone, Copy)]
pub(super) struct Entry {
    pub valid: bool,
    pub io_busy: bool,
    pub device: usize,
    pub page: u64,
    pub valid_mask: u8,
    pub dirty_mask: u8,
    pub last_use: u64,
    pub dirty_sequence: u64,
    pub dirty_since: u64,
    pub owners: [u64; 8],
    pub sequences: [u64; 8],
    pub read_ahead: bool,
    pub phys: u64,
    pub next: u16,
}
impl Entry {
    pub const EMPTY: Self = Self {
        valid: false,
        io_busy: false,
        device: 0,
        page: 0,
        valid_mask: 0,
        dirty_mask: 0,
        last_use: 0,
        dirty_sequence: 0,
        dirty_since: 0,
        owners: [0; 8],
        sequences: [0; 8],
        read_ahead: false,
        phys: 0,
        next: NONE,
    };
}
const fn readahead_empty() -> policy::ReadAhead {
    policy::ReadAhead {
        generation: 1,
        pending: false,
        pending_page: 0,
        pending_pages: 0,
        inflight: false,
        inflight_page: 0,
        inflight_pages: 0,
        resident_pages: 0,
        sequential: policy::Sequential {
            have_last: false,
            last_end_page: 0,
            window_pages: 0,
        },
    }
}
pub(super) struct State {
    pub entries: [Entry; COUNT],
    pub buckets: [u16; 1024],
    pub clock: u64,
    pub dirty_clock: u64,
    pub write_clock: u64,
    pub batch_clock: u64,
    pub dirty_pages: u32,
    pub dirty_sectors: u64,
    pub device_dirty: [u64; DEVICES],
    pub payload_frames: u32,
    pub event: Event,
    pub lock: Mutex,
    pub worker_started: bool,
    pub worker_id: u32,
    pub failure_until: [u64; DEVICES],
    pub readahead: [policy::ReadAhead; DEVICES],
    pub reference_frames: u64,
    pub capacity: policy::Capacity,
    pub window_busy: bool,
}
#[repr(align(16))]
struct Window([u8; 16 * PAGE]);
struct Resident(UnsafeCell<State>);
// Original sleepable cache mutex protects metadata; exclusive BSP reset
// precedes workers. Raw resident pins retain exact identities across I/O.
unsafe impl Sync for Resident {}
static CACHE: Resident = Resident(UnsafeCell::new(State {
    entries: [Entry::EMPTY; COUNT],
    buckets: [NONE; 1024],
    clock: 0,
    dirty_clock: 0,
    write_clock: 0,
    batch_clock: 0,
    dirty_pages: 0,
    dirty_sectors: 0,
    device_dirty: [0; DEVICES],
    payload_frames: 0,
    event: Event::AUTO,
    lock: Mutex::EMPTY,
    worker_started: false,
    worker_id: 0,
    failure_until: [0; DEVICES],
    readahead: [const { readahead_empty() }; DEVICES],
    reference_frames: 0,
    capacity: policy::Capacity {
        ram_pages: 64,
        active_pages: 64,
        pressure_level: 0,
        read_ahead_pages: 1,
    },
    window_busy: false,
}));
struct ResidentWindow(UnsafeCell<Window>);
// One explicit-flush window is admitted by window_busy under the cache lock.
unsafe impl Sync for ResidentWindow {}
static WINDOW: ResidentWindow = ResidentWindow(UnsafeCell::new(Window([0; 16 * PAGE])));
pub(super) static STATS: Counters = Counters::new();
pub(super) fn state() -> *mut State {
    CACHE.0.get()
}
pub(super) fn window() -> *mut u8 {
    unsafe { ptr::addr_of_mut!((*WINDOW.0.get()).0).cast::<u8>() }
}
pub(super) unsafe fn entry(index: usize) -> *mut Entry {
    assert!(index < COUNT);
    unsafe {
        ptr::addr_of_mut!((*state()).entries)
            .cast::<Entry>()
            .add(index)
    }
}
pub(super) fn page(lba: u64) -> u64 {
    lba & !7
}
pub(super) fn bit(lba: u64) -> u8 {
    1u8 << (lba & 7)
}
pub(super) fn offset(lba: u64) -> usize {
    (lba as usize & 7) * SECTOR
}
fn bucket(device: usize, page: u64) -> usize {
    let mut h = page.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= (device as u64)
        .wrapping_add(1)
        .wrapping_mul(0xc2b2ae3d27d4eb4f);
    h ^= h >> 29;
    (h & 1023) as usize
}
// The existing policy owner exposes only this same resident Index pointer;
// no second table or policy instance is created by the cache.
pub(super) unsafe fn index(op: u8, slot: usize, device: usize, flag: bool) -> usize {
    unsafe { policy::r4os_rust_fs_cache_index(policy::global_index(), op, slot, device, flag) }
}
pub(super) fn optional(value: usize) -> Option<usize> {
    if value == usize::MAX {
        None
    } else {
        Some(value)
    }
}
pub(super) unsafe fn next_clock() -> u64 {
    let s = state();
    unsafe {
        (*s).clock = (*s).clock.wrapping_add(1);
        (*s).clock
    }
}
unsafe fn next_dirty() -> u64 {
    let s = state();
    unsafe {
        (*s).dirty_clock = (*s).dirty_clock.wrapping_add(1);
        if (*s).dirty_clock == 0 {
            (*s).dirty_clock = 1;
        }
        (*s).dirty_clock
    }
}
unsafe fn next_write() -> u64 {
    let s = state();
    unsafe {
        (*s).write_clock = (*s).write_clock.wrapping_add(1);
        if (*s).write_clock == 0 {
            (*s).write_clock = 1;
        }
        (*s).write_clock
    }
}
pub(super) unsafe fn find(device: usize, page: u64) -> Option<usize> {
    unsafe {
        let mut cursor = (*state()).buckets[bucket(device, page)];
        while cursor != NONE {
            let i = usize::from(cursor);
            let e = entry(i).read();
            if e.valid && e.device == device && e.page == page {
                return Some(i);
            }
            cursor = e.next;
        }
        None
    }
}
pub(super) unsafe fn link(i: usize) {
    unsafe {
        let e = entry(i);
        let b = bucket((*e).device, (*e).page);
        (*e).next = (*state()).buckets[b];
        (*state()).buckets[b] = i as u16;
    }
}
pub(super) unsafe fn unlink(i: usize) {
    unsafe {
        let e = entry(i);
        let b = bucket((*e).device, (*e).page);
        let mut cursor = (*state()).buckets[b];
        let mut prev = NONE;
        while cursor != NONE {
            if usize::from(cursor) == i {
                if prev == NONE {
                    (*state()).buckets[b] = (*e).next;
                } else {
                    (*entry(usize::from(prev))).next = (*e).next;
                }
                (*e).next = NONE;
                return;
            }
            prev = cursor;
            cursor = (*entry(usize::from(cursor))).next;
        }
    }
}
pub(super) struct Operation(task_context::Unwind);
impl Operation {
    pub unsafe fn enter() -> Option<Self> {
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _ = unsafe { task_context::leave(self.0) };
    }
}
pub(super) unsafe fn acquire() -> Option<bool> {
    if scheduler::current().is_null() {
        return Some(false);
    }
    if !unsafe {
        sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*state()).lock), 1, LOCK_TICKS)
    } {
        STATS.lock_timeouts.fetch_add(1, Relaxed);
        None
    } else {
        Some(true)
    }
}
pub(super) unsafe fn release(owned: bool) {
    if owned {
        unsafe {
            sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*state()).lock), 2, 0);
        }
    }
}

pub(super) unsafe fn relock(owned: bool) {
    unsafe {
        if !owned {
            return;
        }
        let mut slices = 0u64;
        let mut token = 0;
        while !sync_locks::r4os_rust_sync_mutex(
            ptr::addr_of_mut!((*state()).lock),
            1,
            5 * DEFAULT_HZ as u64,
        ) {
            STATS.lock_timeouts.fetch_add(1, Relaxed);
            slices = slices.wrapping_add(1);
            if token == 0 {
                token = crate::fs::page_cache::diagnostic(0, 0, 0, 0);
            }
            crate::fs::page_cache::diagnostic(2, token, slices, 0);
        }
        if token != 0 {
            crate::fs::page_cache::diagnostic(1, token, 0, 0);
        }
    }
}
pub(super) unsafe fn sleep(ticks: u64, reason: &[u8]) {
    unsafe {
        scheduler::r4os_rust_scheduler_task(
            4,
            ptr::null_mut(),
            ticks,
            0,
            reason.as_ptr(),
            reason.len(),
        );
    }
}
pub(super) unsafe fn wait_busy(owned: bool) -> bool {
    unsafe {
        STATS.busy_waits.fetch_add(1, Relaxed);
        if !owned {
            return true;
        }
        release(true);
        sleep(1, b"pgcache-busy");
        relock(true);
        true
    }
}
pub(super) unsafe fn signal() {
    unsafe {
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*state()).event), 0, 0);
    }
}
pub(super) unsafe fn capacity() -> policy::Capacity {
    unsafe {
        let mut stats = phys::Stats::default();
        phys::r4os_rust_phys_stats(&mut stats);
        let s = state();
        let next = policy::capacity_for_memory((*s).reference_frames, stats.free_frames);
        if next.active_pages < (*s).capacity.active_pages {
            STATS.capacity_reductions.fetch_add(1, Relaxed);
        }
        if next.read_ahead_pages < (*s).capacity.read_ahead_pages {
            for device in 0..DEVICES {
                let a = ptr::addr_of_mut!((*s).readahead)
                    .cast::<policy::ReadAhead>()
                    .add(device);
                if (*a).pending || (*a).inflight {
                    if policy::r4os_rust_fs_cache_readahead(
                        a,
                        2,
                        0,
                        0,
                        ptr::null(),
                        false,
                        ptr::null_mut(),
                    ) {
                        STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
                    }
                } else if next.read_ahead_pages == 0 {
                    policy::r4os_rust_fs_cache_sequential(
                        ptr::addr_of_mut!((*a).sequential),
                        1,
                        0,
                        0,
                        0,
                        ptr::null_mut(),
                    );
                }
            }
        }
        STATS
            .capacity_ram_limit_pages
            .store(u32::from(next.ram_pages), Relaxed);
        STATS
            .capacity_active_limit_pages
            .store(u32::from(next.active_pages), Relaxed);
        STATS
            .capacity_pressure_level
            .store(u32::from(next.pressure_level), Relaxed);
        STATS.read_ahead_window_max_pages.store(
            if SPECULATION {
                u32::from(next.read_ahead_pages)
            } else {
                0
            },
            Relaxed,
        );
        (*s).capacity = policy::Capacity {
            ram_pages: next.ram_pages,
            active_pages: next.active_pages,
            pressure_level: next.pressure_level,
            read_ahead_pages: next.read_ahead_pages,
        };
        next
    }
}
pub(super) unsafe fn drop_readahead(i: usize) {
    unsafe {
        let e = entry(i);
        if !(*e).read_ahead {
            return;
        }
        (*e).read_ahead = false;
        let device = (*e).device;
        if device < DEVICES {
            let a = ptr::addr_of_mut!((*state()).readahead)
                .cast::<policy::ReadAhead>()
                .add(device);
            policy::r4os_rust_fs_cache_readahead(a, 5, 0, 0, ptr::null(), false, ptr::null_mut());
        }
    }
}
pub(super) unsafe fn mark_dirty(i: usize, bits: u8, owner: u64) -> bool {
    unsafe {
        if i >= COUNT {
            return false;
        }
        let e = entry(i);
        if !(*e).valid || (*e).device >= DEVICES {
            return false;
        }
        let newly = bits & !(*e).dirty_mask;
        if (*e).dirty_mask == 0 {
            if index(4, i, 0, false) == 0 {
                return false;
            }
            (*e).dirty_sequence = next_dirty();
            (*e).dirty_since = timer::tick_count().max(1);
            (*state()).dirty_pages = (*state()).dirty_pages.wrapping_add(1);
        }
        (*e).dirty_mask |= bits;
        let n = u64::from(newly.count_ones());
        (*state()).dirty_sectors = (*state()).dirty_sectors.wrapping_add(n);
        let dev = (*e).device;
        (*state()).device_dirty[dev] = (*state()).device_dirty[dev].wrapping_add(n);
        for sector in 0..8 {
            if bits & (1 << sector) != 0 {
                (*e).owners[sector] = owner;
                (*e).sequences[sector] = next_write();
            }
        }
        STATS
            .dirty_high_water_entries
            .fetch_max((*state()).dirty_pages, Relaxed);
        STATS
            .writeback_queue_high_water
            .fetch_max((*state()).dirty_pages, Relaxed);
        if (*policy::global_index()).devices[dev].pressure_active {
            signal();
        }
        true
    }
}
pub(super) unsafe fn clear_dirty(i: usize, bits: u8) {
    unsafe {
        if i >= COUNT || bits == 0 {
            return;
        }
        let e = entry(i);
        let cleared = (*e).dirty_mask & bits;
        if cleared == 0 {
            return;
        }
        (*e).dirty_mask &= !cleared;
        let n = u64::from(cleared.count_ones());
        (*state()).dirty_sectors = (*state()).dirty_sectors.saturating_sub(n);
        let dev = (*e).device;
        if dev < DEVICES {
            (*state()).device_dirty[dev] = (*state()).device_dirty[dev].saturating_sub(n);
        }
        batch::r4os_rust_fs_batch_clear(
            ptr::addr_of_mut!((*e).owners).cast(),
            ptr::addr_of_mut!((*e).sequences).cast(),
            cleared,
        );
        if (*e).dirty_mask == 0 {
            (*e).dirty_sequence = 0;
            (*e).dirty_since = 0;
            if (*state()).dirty_pages != 0 {
                (*state()).dirty_pages -= 1;
            }
            index(5, i, 0, false);
        }
    }
}
pub(super) unsafe fn batch_mask(i: usize, owner: u64) -> u8 {
    unsafe {
        let e = entry(i);
        batch::r4os_rust_fs_batch_mask((*e).dirty_mask, ptr::addr_of!((*e).owners).cast(), owner)
    }
}
pub(super) unsafe fn batch_sequence(i: usize, owner: u64) -> Option<u64> {
    unsafe {
        let e = entry(i);
        let mut out = 0;
        if batch::r4os_rust_fs_batch_oldest(
            (*e).dirty_mask,
            ptr::addr_of!((*e).owners).cast(),
            ptr::addr_of!((*e).sequences).cast(),
            owner,
            &mut out,
        ) {
            Some(out)
        } else {
            None
        }
    }
}
pub(super) unsafe fn oldest_for_batch(device: usize, owner: u64) -> Option<usize> {
    unsafe {
        let mut best = None;
        let mut seq = 0;
        let mut cursor = optional(index(11, 0, device, false));
        while let Some(i) = cursor {
            if let Some(s) = batch_sequence(i, owner)
                && (best.is_none() || s < seq)
            {
                best = Some(i);
                seq = s;
            }
            cursor = optional(index(12, i, 0, false));
        }
        best
    }
}
pub(super) unsafe fn payload(i: usize) -> *mut u8 {
    unsafe {
        if i >= COUNT || (*entry(i)).phys == 0 {
            return ptr::null_mut();
        }
        phys::r4os_rust_phys_to_virt((*entry(i)).phys) as *mut u8
    }
}
pub(super) unsafe fn release_payload(i: usize, reclaim: bool) {
    unsafe {
        if i >= COUNT {
            return;
        }
        let e = entry(i);
        let address = (*e).phys;
        if address == 0 {
            return;
        }
        phys::r4os_rust_phys_free(address);
        (*e).phys = 0;
        if (*state()).payload_frames != 0 {
            (*state()).payload_frames -= 1;
        }
        STATS.payload_releases.fetch_add(1, Relaxed);
        if reclaim {
            STATS.reclaim_returned_frames.fetch_add(1, Relaxed);
            STATS.reclaim_returned_bytes.fetch_add(PAGE as u64, Relaxed);
        }
    }
}
pub(super) unsafe fn evict(i: usize) {
    unsafe {
        if i >= COUNT || !(*entry(i)).valid {
            return;
        }
        STATS.evictions.fetch_add(1, Relaxed);
        if (*entry(i)).dirty_mask == 0 {
            STATS.reclaim_clean_entries.fetch_add(1, Relaxed);
        }
        drop_readahead(i);
        unlink(i);
        index(8, i, 0, false);
        let address = (*entry(i)).phys;
        entry(i).write(Entry {
            phys: address,
            ..Entry::EMPTY
        });
    }
}
pub(super) unsafe fn clear(i: usize, reclaim: bool) {
    unsafe {
        if i >= COUNT {
            return;
        }
        if (*entry(i)).valid {
            drop_readahead(i);
            unlink(i);
            index(8, i, 0, false);
        }
        release_payload(i, reclaim);
        entry(i).write(Entry::EMPTY);
        index(9, i, 0, false);
    }
}
pub(super) unsafe fn select_slot(device: usize) -> Option<usize> {
    unsafe {
        let limit = capacity().active_pages;
        if index(14, 0, 0, false) < usize::from(limit)
            && let Some(i) = optional(index(1, 0, 0, false))
        {
            return Some(i);
        }
        STATS.reclaim_scans.fetch_add(1, Relaxed);
        let Some(i) = optional(index(10, 0, device, false)) else {
            STATS.reclaim_failed_drains.fetch_add(1, Relaxed);
            return None;
        };
        evict(i);
        Some(i)
    }
}
pub(super) unsafe fn ensure_payload(i: usize) -> *mut u8 {
    unsafe {
        if i >= COUNT {
            return ptr::null_mut();
        }
        if (*entry(i)).phys == 0 {
            let mut address = 0;
            if !phys::r4os_rust_phys_alloc(&mut address) {
                // Original OOM fallback reclaims one clean payload, never dirty
                // data and never drops this nested metadata owner.
                STATS.reclaim_scans.fetch_add(1, Relaxed);
                if let Some(victim) = optional(index(10, 0, usize::MAX, false)) {
                    STATS.evictions.fetch_add(1, Relaxed);
                    STATS.reclaim_clean_entries.fetch_add(1, Relaxed);
                    clear(victim, true);
                }
                if !phys::r4os_rust_phys_alloc(&mut address) {
                    STATS.payload_allocation_failures.fetch_add(1, Relaxed);
                    return ptr::null_mut();
                }
            }
            (*entry(i)).phys = address;
            (*state()).payload_frames = (*state()).payload_frames.wrapping_add(1);
            STATS.payload_allocations.fetch_add(1, Relaxed);
        }
        payload(i)
    }
}
pub(super) unsafe fn create(device: usize, page: u64) -> Option<usize> {
    unsafe {
        if device >= DEVICES {
            return None;
        }
        let i = select_slot(device)?;
        let phys = (*entry(i)).phys;
        entry(i).write(Entry {
            valid: true,
            device,
            page,
            last_use: next_clock(),
            phys,
            ..Entry::EMPTY
        });
        if ensure_payload(i).is_null() || index(2, i, device, false) == 0 {
            let phys = (*entry(i)).phys;
            entry(i).write(Entry {
                phys,
                ..Entry::EMPTY
            });
            index(9, i, 0, false);
            return None;
        }
        link(i);
        Some(i)
    }
}
// Exclusive original BSP initialization. Reset each bounded entry in resident
// storage; never construct/move the entire >100-KB cache on a 64-KB task stack.
pub(super) unsafe fn reset() {
    unsafe {
        for i in 0..COUNT {
            release_payload(i, false);
            entry(i).write(Entry::EMPTY);
        }
        let s = state();
        for b in 0..1024 {
            (*s).buckets[b] = NONE;
        }
        STATS.clear();
        STATS.enabled.store(true, Relaxed);
        STATS.policy_version.store(2, Relaxed);
        STATS.policy_device_capacity.store(DEVICES as u32, Relaxed);
        STATS.policy_dirty_high_pages.store(64, Relaxed);
        STATS.policy_dirty_low_pages.store(32, Relaxed);
        STATS
            .policy_max_dirty_age_ticks
            .store(MAX_DIRTY_AGE, Relaxed);
        STATS.policy_background_page_budget.store(4, Relaxed);
        (*s).clock = 0;
        (*s).dirty_clock = 0;
        (*s).write_clock = 0;
        (*s).batch_clock = 0;
        (*s).dirty_pages = 0;
        (*s).dirty_sectors = 0;
        (*s).payload_frames = 0;
        (*s).window_busy = false;
        for d in 0..DEVICES {
            (*s).device_dirty[d] = 0;
            (*s).failure_until[d] = 0;
            ptr::addr_of_mut!((*s).readahead)
                .cast::<policy::ReadAhead>()
                .add(d)
                .write(readahead_empty());
        }
        index(0, 0, 0, false);
        ptr::addr_of_mut!((*s).event).write(Event::AUTO);
        ptr::addr_of_mut!((*s).lock).write(Mutex {
            rank: 155,
            name: Text::literal(b"fs-page-cache"),
            ..Mutex::EMPTY
        });
        (*s).worker_started = false;
        (*s).worker_id = 0;
        let mut physical = phys::Stats::default();
        phys::r4os_rust_phys_stats(&mut physical);
        (*s).reference_frames = physical.free_frames;
        let initial = policy::capacity_for_memory(physical.free_frames, physical.free_frames);
        STATS
            .capacity_ram_limit_pages
            .store(u32::from(initial.ram_pages), Relaxed);
        STATS
            .capacity_active_limit_pages
            .store(u32::from(initial.active_pages), Relaxed);
        STATS
            .capacity_pressure_level
            .store(u32::from(initial.pressure_level), Relaxed);
        STATS.read_ahead_window_max_pages.store(
            if SPECULATION {
                u32::from(initial.read_ahead_pages)
            } else {
                0
            },
            Relaxed,
        );
        (*s).capacity = initial;
    }
}

pub(super) struct Held {
    pub owned: bool,
    held: bool,
}
impl Held {
    pub unsafe fn acquire() -> Option<Self> {
        unsafe { acquire().map(|owned| Self { owned, held: true }) }
    }
    pub unsafe fn unlock(&mut self) {
        unsafe {
            release(self.owned);
        }
        self.held = false;
    }
    pub unsafe fn relock(&mut self) {
        unsafe {
            relock(self.owned);
        }
        self.held = true;
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if self.held {
            unsafe { release(self.owned) }
        }
    }
}
pub(super) struct Staging {
    pub data: *mut u8,
    pub len: usize,
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.data.is_null() {
            unsafe {
                heap::r4os_rust_heap_free(self.data, self.len);
            }
        }
    }
}
