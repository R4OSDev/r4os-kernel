//! Bounded physical sequential read-ahead under the existing cache owner.
//! One worker, four resident pages/device, copied mount identities and one
//! short low-priority request. Physical completion precedes reuse/release.
use super::{page_cache_policy as policy, page_cache_state as c, page_cache_worker as worker};
use crate::{
    arch::pit::DEFAULT_HZ,
    platform::timer,
    sched::scheduler,
    storage::{
        access_runtime::{self, CacheMount},
        access_state::Reference,
        block_io, block_registry,
    },
};
use core::{cell::UnsafeCell, ptr, sync::atomic::Ordering::Relaxed};
const PAGES: usize = 4;
const UNUSED_TICKS: u64 = DEFAULT_HZ as u64 / 4;
#[derive(Clone, Copy)]
struct Context {
    mount: Option<CacheMount>,
    slots: [u16; PAGES],
    last_demand: u64,
}
const EMPTY: Context = Context {
    mount: None,
    slots: [c::NONE; PAGES],
    last_demand: 0,
};
struct Resident(UnsafeCell<[Context; c::DEVICES]>);
// Existing cache mutex protects every context, including after I/O relock.
unsafe impl Sync for Resident {}
static CONTEXTS: Resident = Resident(UnsafeCell::new([EMPTY; c::DEVICES]));
#[repr(align(16))]
struct Buffer(UnsafeCell<[u8; PAGES * c::PAGE]>);
// Only the one permanent cache worker admits this resident buffer; the exact
// block waiter drains physical access before that worker can use it again.
unsafe impl Sync for Buffer {}
static BUFFER: Buffer = Buffer(UnsafeCell::new([0; PAGES * c::PAGE]));
unsafe fn context(dev: usize) -> *mut Context {
    unsafe { CONTEXTS.0.get().cast::<Context>().add(dev) }
}
unsafe fn ahead(dev: usize) -> *mut policy::ReadAhead {
    unsafe {
        ptr::addr_of_mut!((*c::state()).readahead)
            .cast::<policy::ReadAhead>()
            .add(dev)
    }
}
pub(super) unsafe fn reset() {
    unsafe {
        CONTEXTS.0.get().write([EMPTY; c::DEVICES]);
        (*c::state()).prefetch_device = 0;
    }
}
pub(super) unsafe fn forget(dev: usize, index: usize) {
    unsafe {
        for slot in &mut (*context(dev)).slots {
            if usize::from(*slot) == index {
                *slot = c::NONE;
            }
        }
    }
}
pub(super) unsafe fn cancel_pending(dev: usize) {
    unsafe {
        if dev < c::DEVICES && (*ahead(dev)).cancel_all() {
            c::STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
        }
    }
}
pub(super) unsafe fn cancel(dev: usize) {
    unsafe {
        if dev >= c::DEVICES {
            return;
        }
        cancel_pending(dev);
        let slots = (*context(dev)).slots;
        for slot in slots {
            if slot == c::NONE {
                continue;
            }
            let i = usize::from(slot);
            let e = c::entry(i);
            if (*e).valid
                && (*e).device == dev
                && (*e).read_ahead
                && !(*e).io_busy
                && (*e).dirty_mask == 0
            {
                c::clear(i, true);
            }
        }
        (*context(dev)).mount = None;
    }
}
pub(crate) fn cancel_mount(reference: Reference) {
    unsafe {
        let Some(_held) = c::Held::acquire() else {
            return;
        };
        for dev in 0..c::DEVICES {
            if (*context(dev))
                .mount
                .is_some_and(|b| b.reference == reference)
            {
                cancel(dev);
            }
        }
    }
}
pub(super) unsafe fn schedule(dev: usize, lba: u64, count: u32) {
    unsafe {
        if !(*c::state()).worker_started || dev >= c::DEVICES || count < 8 {
            return;
        }
        let Some(end) = lba.checked_add(u64::from(count)) else {
            return;
        };
        // Metadata fragments do not establish a physical sequential stream.
        if !lba.is_multiple_of(8) || !end.is_multiple_of(8) {
            return;
        }
        let cap = c::capacity();
        if cap.read_ahead_pages == 0 || cap.pressure_level != 0 {
            cancel(dev);
            return;
        }
        let Some(info) = block_registry::cache_view(dev) else {
            cancel(dev);
            return;
        };
        if info.sector_bytes != 512 || info.max_sectors < 8 {
            return;
        }
        let Some(mount) = access_runtime::cache_mount(info.reference, lba, u64::from(count)) else {
            cancel(dev);
            return;
        };
        if !(*context(dev)).mount.is_some_and(|b| b.same(mount)) {
            cancel(dev);
            (*context(dev)).mount = Some(mount);
        }
        (*context(dev)).last_demand = timer::tick_count();
        let limit = cap
            .read_ahead_pages
            .min(PAGES as u16)
            .min(info.max_sectors / 8);
        let decision = (*ahead(dev)).sequential.observe(lba / 8, end / 8, limit);
        if decision.random_reset {
            c::STATS.read_ahead_random_resets.fetch_add(1, Relaxed);
            cancel(dev);
            (*context(dev)).mount = Some(mount);
            (*context(dev)).last_demand = timer::tick_count();
            (*ahead(dev)).sequential.observe(lba / 8, end / 8, limit);
            return;
        }
        if decision.pages == 0 {
            return;
        }
        let available = worker::fill_budget(dev);
        let remaining = mount
            .region
            .first
            .saturating_add(mount.region.count)
            .saturating_sub(end)
            / 8;
        let pages = decision
            .pages
            .min(available)
            .min(remaining.min(PAGES as u64) as u16);
        c::STATS.read_ahead_requests.fetch_add(1, Relaxed);
        if pages == 0 || c::find(dev, end).is_some() {
            c::STATS.read_ahead_budget_skips.fetch_add(1, Relaxed);
            return;
        }
        if (*ahead(dev)).schedule(end / 8, pages) {
            c::STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
        }
        c::STATS
            .read_ahead_pages_scheduled
            .fetch_add(u64::from(pages), Relaxed);
        c::signal();
    }
}
pub(super) unsafe fn step() {
    unsafe {
        let Some(_operation) = crate::sched::task_context::Admission::acquire(true) else {
            return;
        };
        let Some(held) = c::Held::acquire() else {
            return;
        };
        work(held.owned);
        // Every early skip also preserves other devices' coalesced wake.
        // Physical I/O and exact mount use returned in work before this edge.
        if (0..c::DEVICES).any(|d| (*ahead(d)).pending) {
            c::signal();
        }
        drop(held);
        drop(_operation);
        scheduler::safe_reschedule();
    }
}
unsafe fn work(owned: bool) {
    unsafe {
        let now = timer::tick_count();
        let cap = c::capacity();
        for dev in 0..c::DEVICES {
            if (*context(dev))
                .mount
                .is_some_and(|b| !access_runtime::cache_mount_current(b))
                || now.saturating_sub((*context(dev)).last_demand) >= UNUSED_TICKS.max(1)
                || cap.pressure_level != 0
            {
                cancel(dev);
            }
        }
        let Some(dev) = (0..c::DEVICES)
            .map(|n| ((*c::state()).prefetch_device + n) % c::DEVICES)
            .find(|&d| (*ahead(d)).pending)
        else {
            return;
        };
        (*c::state()).prefetch_device = (dev + 1) % c::DEVICES;
        let Some(mount) = (*context(dev)).mount else {
            cancel(dev);
            return;
        };
        let budget = worker::fill_budget(dev);
        if budget == 0 {
            cancel_pending(dev);
            return;
        }
        let Some(_mounted_use) = access_runtime::cache_read_use(mount) else {
            cancel(dev);
            return;
        };
        let Some(request) = (*ahead(dev)).begin() else {
            return;
        };
        let mut indices = [c::NONE; PAGES];
        let mut reserved = 0;
        let Some(first) = request.page.checked_mul(8) else {
            (*ahead(dev)).complete(request, false);
            return;
        };
        for position in 0..usize::from(request.pages.min(budget).min(PAGES as u16)) {
            let page = first + position as u64 * 8;
            if c::find(dev, page).is_some() {
                break;
            }
            let Some(i) = worker::create_readahead(dev, page) else {
                break;
            };
            if c::index(6, i, 0, false) == 0 {
                c::clear(i, true);
                break;
            }
            (*c::entry(i)).io_busy = true;
            indices[reserved] = i as u16;
            reserved += 1;
        }
        if reserved == 0 {
            (*ahead(dev)).complete(request, false);
            return;
        }
        let data = BUFFER.0.get().cast::<u8>();
        c::release(owned);
        let result = block_io::read_prefetch_exact(
            mount.region.device,
            first,
            (reserved * 8) as u16,
            data,
            reserved * c::PAGE,
        );
        c::relock(owned);
        let cap = c::capacity();
        let current = cap.pressure_level == 0
            && access_runtime::cache_mount_current(mount)
            && (*context(dev)).mount.is_some_and(|b| b.same(mount));
        let publish = (*ahead(dev)).complete(request, current && result == Some(true));
        if result.is_some() {
            c::STATS
                .read_ahead_issued
                .fetch_add(reserved as u64, Relaxed);
            c::STATS
                .read_ahead_pages_issued
                .fetch_add(reserved as u64, Relaxed);
        } else {
            c::STATS.read_ahead_budget_skips.fetch_add(1, Relaxed);
        }
        for (position, slot) in indices.into_iter().take(reserved).enumerate() {
            let i = usize::from(slot);
            let e = c::entry(i);
            assert!(
                (*e).valid
                    && (*e).device == dev
                    && (*e).page == first + position as u64 * 8
                    && (*e).io_busy,
                "pinned speculative identity"
            );
            if publish {
                let frame = c::payload(i);
                assert!(!frame.is_null() && (*e).dirty_mask == 0);
                ptr::copy_nonoverlapping(data.add(position * c::PAGE), frame, c::PAGE);
                (*e).valid_mask = c::FULL;
                (*e).read_ahead = true;
                (*e).last_use = c::next_clock();
                let target = (*context(dev))
                    .slots
                    .iter_mut()
                    .find(|s| **s == c::NONE)
                    .expect("bounded speculative residency");
                *target = slot;
                (*ahead(dev)).resident_pages += 1;
                c::STATS.fills.fetch_add(1, Relaxed);
            }
            (*e).io_busy = false;
            c::index(7, i, 0, false);
            if !publish {
                c::clear(i, true);
            }
        }
    }
}
