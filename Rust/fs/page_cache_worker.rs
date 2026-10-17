//! Quarter-second dirty writeback and capacity trimming; speculative I/O is disabled.
use super::{page_cache_policy as policy, page_cache_state as c, page_cache_writeback as wb};
use crate::{
    arch::pit::DEFAULT_HZ,
    memory::phys,
    platform::{clock_math, timer},
    sched::{sync_objects, task},
    storage::block_registry,
};
use c::{COUNT, DEVICES, FULL, Held, STATS};
use core::{ffi::c_void, ptr, sync::atomic::Ordering::Relaxed};
#[repr(C)]
struct Create {
    name: *const u8,
    name_len: usize,
    state: u8,
    role: u8,
    needs_fpu: bool,
    smp_eligible: bool,
    smp_r4x: bool,
    owner_kind: u8,
    owner_context: *mut c_void,
    entry: Option<unsafe extern "C" fn()>,
}
unsafe extern "C" {
    fn r4os_rust_task_create(input: *const Create, critical: bool, failure: *mut u8)
    -> *mut c_void;
}
pub(super) unsafe fn start() -> bool {
    unsafe {
        if (*c::state()).worker_started {
            return true;
        }
        let name = b"pgcache-wb";
        let input = Create {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 1,
            role: 3,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(main),
        };
        let mut failure = 0;
        let worker = r4os_rust_task_create(&input, false, &mut failure);
        if worker.is_null() {
            return false;
        }
        let (id, _) = task::immutable_identity(worker);
        (*c::state()).worker_started = true;
        (*c::state()).worker_id = id;
        STATS.policy_worker_started.store(1, Relaxed);
        STATS.policy_worker_task_id.store(id, Relaxed);
        c::signal();
        true
    }
}
unsafe extern "C" fn main() {
    unsafe {
        loop {
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*c::state()).event),
                2,
                (DEFAULT_HZ as u64 / 4).max(1),
            );
            STATS.policy_worker_wakeups.fetch_add(1, Relaxed);
            let mut pages = 0;
            let mut failed = false;
            while pages < 4 {
                match background_one() {
                    Outcome::None => break,
                    Outcome::Wrote => pages += 1,
                    Outcome::Failed => {
                        failed = true;
                        break;
                    }
                }
            }
            // Idle policy trims clean excess capacity without speculative I/O.
            if pages == 0 && !failed {
                trim_one();
            }
            if pages == 4 && prompt_wake() {
                c::sleep((DEFAULT_HZ as u64 / 100).max(1), b"pgcache-policy-yield");
                c::signal();
            }
        }
    }
}
unsafe fn over_capacity() -> bool {
    unsafe { c::index(14, 0, 0, false) > usize::from(c::capacity().active_pages) }
}
unsafe fn trim_one() -> bool {
    unsafe {
        let Some(_held) = Held::acquire() else {
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return false;
        };
        if !over_capacity() {
            return false;
        }
        let Some(i) = c::optional(c::index(10, 0, usize::MAX, false)) else {
            return false;
        };
        STATS.evictions.fetch_add(1, Relaxed);
        STATS.reclaim_clean_entries.fetch_add(1, Relaxed);
        STATS.capacity_trimmed_pages.fetch_add(1, Relaxed);
        c::clear(i, true);
        true
    }
}
enum Outcome {
    None,
    Wrote,
    Failed,
}
unsafe fn select(now: u64) -> Option<(usize, usize, bool)> {
    unsafe {
        let over = over_capacity();
        let mut visited = 0u8;
        for _ in 0..DEVICES {
            let Some(dev) = c::optional(c::index(13, 0, 0, false)) else {
                break;
            };
            let bit = 1u8 << dev;
            if visited & bit != 0 {
                break;
            }
            visited |= bit;
            if now < (*c::state()).failure_until[dev] {
                continue;
            }
            let Some(i) = c::optional(c::index(11, 0, dev, false)) else {
                continue;
            };
            let pressure = (*policy::global_index()).devices[dev].pressure_active || over;
            if pressure
                || policy::r4os_rust_fs_cache_age_due(
                    now,
                    (*c::entry(i)).dirty_since,
                    c::MAX_DIRTY_AGE,
                )
            {
                return Some((i, dev, pressure));
            }
        }
        None
    }
}
unsafe fn background_one() -> Outcome {
    unsafe {
        let Some(held) = Held::acquire() else {
            return Outcome::Failed;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return Outcome::Failed;
        };
        let now = timer::tick_count();
        let Some((i, dev, pressure)) = select(now) else {
            return Outcome::None;
        };
        let before = STATS.writeback_sectors.load(Relaxed);
        STATS.writeback_waits.fetch_add(1, Relaxed);
        if !wb::write_entry(held.owned, i, None, FULL) {
            (*c::state()).failure_until[dev] =
                clock_math::finite_deadline(timer::tick_count(), DEFAULT_HZ as u64);
            STATS.policy_background_errors.fetch_add(1, Relaxed);
            return Outcome::Failed;
        }
        (*c::state()).failure_until[dev] = 0;
        let written = STATS.writeback_sectors.load(Relaxed) - before;
        if written == 0 {
            return Outcome::None;
        }
        STATS.policy_background_drains.fetch_add(1, Relaxed);
        STATS.policy_background_sectors.fetch_add(written, Relaxed);
        if pressure {
            STATS
                .policy_background_pressure_drains
                .fetch_add(1, Relaxed);
            wb::record(wb::Reason::BackgroundPressure, written, now);
        } else {
            STATS.policy_background_age_drains.fetch_add(1, Relaxed);
            wb::record(wb::Reason::BackgroundAge, written, now);
        }
        Outcome::Wrote
    }
}
unsafe fn prompt_wake() -> bool {
    unsafe {
        let Some(_held) = Held::acquire() else {
            return false;
        };
        if over_capacity() {
            return true;
        }
        let now = timer::tick_count();
        for dev in 0..DEVICES {
            if now < (*c::state()).failure_until[dev] {
                continue;
            }
            let Some(i) = c::optional(c::index(11, 0, dev, false)) else {
                continue;
            };
            if (*policy::global_index()).devices[dev].pressure_active
                || policy::r4os_rust_fs_cache_age_due(
                    now,
                    (*c::entry(i)).dirty_since,
                    c::MAX_DIRTY_AGE,
                )
            {
                return true;
            }
        }
        false
    }
}
unsafe fn ahead(dev: usize) -> *mut policy::ReadAhead {
    unsafe {
        ptr::addr_of_mut!((*c::state()).readahead)
            .cast::<policy::ReadAhead>()
            .add(dev)
    }
}
pub(super) unsafe fn demand_start(dev: usize, page: u64) {
    unsafe {
        if dev >= DEVICES {
            return;
        }
        if policy::r4os_rust_fs_cache_readahead(
            ahead(dev),
            1,
            page / 8,
            0,
            ptr::null(),
            false,
            ptr::null_mut(),
        ) {
            STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
        }
    }
}
pub(super) unsafe fn demand_hit(i: usize) {
    unsafe {
        if i >= COUNT || !(*c::entry(i)).valid {
            return;
        }
        c::index(3, i, 0, false);
        let e = c::entry(i);
        if !(*e).read_ahead || (*e).device >= DEVICES {
            return;
        }
        (*e).read_ahead = false;
        policy::r4os_rust_fs_cache_readahead(
            ahead((*e).device),
            5,
            0,
            0,
            ptr::null(),
            false,
            ptr::null_mut(),
        );
        STATS.read_ahead_hits.fetch_add(1, Relaxed);
    }
}
fn resident_budget(capacity: &policy::Capacity) -> u16 {
    if capacity.read_ahead_pages == 0 {
        0
    } else {
        capacity.read_ahead_pages.min(2)
    }
}
fn free_floor(capacity: &policy::Capacity) -> u16 {
    (capacity.active_pages / 8).clamp(8, 32)
}
pub(super) unsafe fn schedule_readahead(dev: usize, lba: u64, count: u32) {
    unsafe {
        if !c::SPECULATION || !(*c::state()).worker_started || dev >= DEVICES || count < 16 {
            return;
        }
        let end = lba.wrapping_add(u64::from(count));
        if end < lba || end == 0 {
            return;
        }
        let last = c::page(end - 1);
        let next = last.wrapping_add(8);
        if next <= last {
            return;
        }
        let Some(info) = block_registry::cache_view(dev) else {
            return;
        };
        if info.sector_bytes != c::SECTOR as u32 {
            return;
        }
        let cap = c::capacity();
        if cap.read_ahead_pages == 0 || (info.sectors != 0 && next >= info.sectors) {
            return;
        }
        let budget = resident_budget(&cap);
        if budget == 0 || (*ahead(dev)).resident_pages >= budget {
            STATS.read_ahead_budget_skips.fetch_add(1, Relaxed);
            return;
        }
        STATS.read_ahead_requests.fetch_add(1, Relaxed);
        if c::find(dev, next).is_some() {
            STATS.read_ahead_budget_skips.fetch_add(1, Relaxed);
            return;
        }
        if policy::r4os_rust_fs_cache_readahead(
            ahead(dev),
            0,
            next / 8,
            1,
            ptr::null(),
            false,
            ptr::null_mut(),
        ) {
            STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
        }
        STATS.read_ahead_pages_scheduled.fetch_add(1, Relaxed);
        c::signal();
    }
}
pub(super) unsafe fn create_readahead(dev: usize, page: u64) -> Option<usize> {
    unsafe {
        let cap = c::capacity();
        if c::index(14, 0, 0, false) >= usize::from(cap.active_pages)
            || (*policy::global_index()).free_count <= free_floor(&cap)
        {
            return None;
        }
        let i = c::optional(c::index(1, 0, 0, false))?;
        let e = c::entry(i);
        let phys = (*e).phys;
        e.write(c::Entry {
            valid: true,
            device: dev,
            page,
            last_use: c::next_clock(),
            phys,
            ..c::Entry::EMPTY
        });
        if (*e).phys == 0 {
            let mut address = 0;
            if !phys::r4os_rust_phys_alloc(&mut address) {
                e.write(c::Entry::EMPTY);
                c::index(9, i, 0, false);
                return None;
            }
            (*e).phys = address;
            (*c::state()).payload_frames = (*c::state()).payload_frames.wrapping_add(1);
            STATS.payload_allocations.fetch_add(1, Relaxed);
        }
        if c::payload(i).is_null() || c::index(2, i, dev, false) == 0 {
            c::release_payload(i, false);
            e.write(c::Entry::EMPTY);
            c::index(9, i, 0, false);
            return None;
        }
        c::link(i);
        Some(i)
    }
}
