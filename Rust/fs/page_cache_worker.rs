//! Bounded dirty writeback, clean trimming and low-priority sequential prefetch.
use super::{page_cache_policy as policy, page_cache_state as c, page_cache_writeback as wb};
use crate::{
    arch::pit::DEFAULT_HZ,
    memory::phys,
    platform::{clock_math, timer},
    sched::{scheduler, sync_objects, task},
};
use c::{COUNT, DEVICES, FULL, Held, STATS};
use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
static EXECUTED_CPUS: AtomicU64 = AtomicU64::new(0);
pub(super) fn execution_cpus() -> u64 {
    EXECUTED_CPUS.load(Relaxed)
}
pub(super) unsafe fn start() -> bool {
    unsafe {
        if (*c::state()).worker_started {
            return true;
        }
        let name = b"pgcache-wb";
        let input = task::task_api::CreateRequest {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 3,
            role: 3,
            needs_fpu: true,
            // Original Held/Operation and queued block owners retain every
            // entry/window/backend lifetime; no direct unaudited AP callback.
            smp_eligible: true,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(main),
        };
        let mut failure = 0;
        let worker = task::task_api::r4os_rust_task_create(&input, false, &mut failure);
        if worker.is_null() {
            return false;
        }
        let (id, _) = task::immutable_identity(worker);
        (*c::state()).worker_started = true;
        (*c::state()).worker_id = id;
        STATS.policy_worker_started.store(1, Relaxed);
        STATS.policy_worker_task_id.store(id, Relaxed);
        task::task_api::r4os_rust_task_mutate(0, worker, timer::tick_count(), 0, ptr::null(), 0);
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
                    Outcome::Wrote => {
                        pages += 1;
                        // Exact writeback and its Held/Operation admissions
                        // have returned; no cache metadata loan spans a switch.
                        scheduler::safe_reschedule();
                    }
                    Outcome::Failed => {
                        failed = true;
                        break;
                    }
                }
            }
            // Idle policy trims clean excess capacity without speculative I/O.
            let mut trimmed = 0;
            if pages == 0 && !failed {
                while trimmed < policy::TRIM_PAGES && trim_one() {
                    trimmed += 1;
                    // One exact clear has returned both owners before switch.
                    scheduler::safe_reschedule();
                }
            }
            if pages == 0 && !failed && trimmed == 0 {
                super::page_cache_prefetch::step();
            }
            if (pages == 4 || trimmed == policy::TRIM_PAGES) && prompt_wake() {
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
        let selected = select(now);
        // Capture the CPU of the actual owned selection, before any backend
        // wait or safe reschedule. Empty policy scans are explicitly included.
        EXECUTED_CPUS.fetch_or(1u64 << crate::arch::percpu::current_index(), Relaxed);
        let Some((i, dev, pressure)) = selected else {
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
pub(super) unsafe fn demand_hit(i: usize, streaming: bool) {
    unsafe {
        if i >= COUNT || !(*c::entry(i)).valid {
            return;
        }
        if !streaming {
            c::index(3, i, 0, false);
        }
        let e = c::entry(i);
        if !(*e).read_ahead || (*e).device >= DEVICES {
            return;
        }
        c::drop_readahead(i);
        STATS.read_ahead_hits.fetch_add(1, Relaxed);
    }
}
fn resident_budget(capacity: &policy::Capacity) -> u16 {
    if capacity.read_ahead_pages == 0 {
        0
    } else {
        capacity.read_ahead_pages.min(policy::MAX_FILL_PAGES)
    }
}
fn free_floor(capacity: &policy::Capacity) -> u16 {
    (capacity.active_pages / 8).clamp(8, 32)
}
// Shared fill admission retains the dormant speculative path's free floor
// and per-device resident budget. This does not enable speculative I/O.
pub(super) unsafe fn fill_budget(dev: usize) -> u16 {
    unsafe {
        if dev >= DEVICES {
            return 0;
        }
        let cap = c::capacity();
        let index = &*policy::global_index();
        let used = c::COUNT as u16 - index.free_count;
        if cap.pressure_level != 0 {
            return 0;
        }
        let spare = cap
            .active_pages
            .saturating_sub(used)
            .saturating_sub(free_floor(&cap));
        let cold = if cold_victim(dev).is_some() { 4 } else { 0 };
        resident_budget(&cap)
            .min(4)
            .saturating_sub((*ahead(dev)).resident_pages)
            .min(spare.saturating_add(cold))
    }
}
pub(super) unsafe fn schedule_readahead(dev: usize, lba: u64, count: u32) {
    unsafe {
        super::page_cache_prefetch::schedule(dev, lba, count);
    }
}
pub(super) unsafe fn create_readahead(dev: usize, page: u64) -> Option<usize> {
    unsafe {
        let cap = c::capacity();
        if cap.pressure_level != 0 {
            return None;
        }
        if c::index(14, 0, 0, false)
            >= usize::from(cap.active_pages.saturating_sub(free_floor(&cap)))
        {
            let victim = cold_victim(dev)?;
            c::clear(victim, false);
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
        if c::payload(i).is_null() || !(*policy::global_index()).attach_class(i, dev, true) {
            c::release_payload(i, false);
            e.write(c::Entry::EMPTY);
            c::index(9, i, 0, false);
            return None;
        }
        c::link(i);
        Some(i)
    }
}

// At most8 heads, never a cache-wide scan and never evict a hot/dirty/busy page.
pub(super) unsafe fn cold_victim(preferred: usize) -> Option<usize> {
    unsafe {
        let index = &*policy::global_index();
        for n in 0..DEVICES {
            let dev = (preferred + n) % DEVICES;
            let i = index.devices[dev].clean_head;
            if i != policy::NO_INDEX && index.links[usize::from(i)].cold {
                return Some(usize::from(i));
            }
        }
        None
    }
}
