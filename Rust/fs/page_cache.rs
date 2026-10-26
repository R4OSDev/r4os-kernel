//! Cache API owner. Fixed caller records and raw buffers bind the native
//! read/write, policy and writeback owners.
use super::{
    page_cache_policy as policy, page_cache_read as read, page_cache_state as c,
    page_cache_stats::Summary, page_cache_worker as worker, page_cache_write as write,
    page_cache_writeback as wb,
};
unsafe fn snapshot() -> Summary {
    unsafe {
        let Some(_held) = c::Held::acquire() else {
            return c::STATS.snapshot();
        };
        let mut out = c::STATS.snapshot();
        let used = c::index(14, 0, 0, false) as u32;
        let s = c::state();
        let dirty = (*s).dirty_pages;
        let index = policy::global_index();
        let busy_dirty: u16 = (*index)
            .devices
            .iter()
            .map(|device| device.busy_dirty)
            .sum();
        let busy_clean = u32::from((*index).busy_count - busy_dirty);
        let clean = (used - dirty).saturating_sub(busy_clean);
        let frames = (*s).payload_frames;
        out.enabled = true;
        out.sector_bytes = c::SECTOR as u32;
        out.capacity = c::COUNT as u32;
        out.entries_used = used;
        out.dirty_entries = dirty;
        out.dirty_bytes = (*s).dirty_sectors * c::SECTOR as u64;
        out.writeback_queue_depth = dirty;
        out.clean_reclaimable_entries = clean;
        out.dirty_non_reclaimable_entries = dirty;
        out.clean_reclaimable_bytes = u64::from(clean) * c::PAGE as u64;
        out.dirty_non_reclaimable_bytes = u64::from(dirty) * c::PAGE as u64;
        out.payload_frame_bytes = c::PAGE as u32;
        out.payload_frames = frames;
        out.payload_bytes = u64::from(frames) * c::PAGE as u64;
        out.pmm_reclaimable_bytes = u64::from(clean) * c::PAGE as u64;
        out.pmm_dirty_bytes = u64::from(dirty) * c::PAGE as u64;
        out.policy_worker_started = u32::from((*s).worker_started);
        out.policy_worker_task_id = (*s).worker_id;
        out.policy_clean_device_probes = (*index).clean_device_probes;
        out.policy_dirty_device_probes = (*index).dirty_device_probes;
        let cap = c::capacity();
        out.capacity_ram_limit_pages = u32::from(cap.ram_pages);
        out.capacity_active_limit_pages = u32::from(cap.active_pages);
        out.capacity_pressure_level = u32::from(cap.pressure_level);
        out.read_ahead_window_max_pages = if c::SPECULATION {
            u32::from(cap.read_ahead_pages)
        } else {
            0
        };
        let mut window = 0;
        if c::SPECULATION {
            for a in &(*s).readahead {
                window = window.max(a.sequential.window_pages);
            }
        }
        out.read_ahead_window_pages = u32::from(window);
        out.policy_device_dirty_high_water = u32::from(
            (*index)
                .devices
                .iter()
                .map(|d| d.dirty_high_water)
                .max()
                .unwrap_or(0),
        );
        out
    }
}
/// # Safety
/// Exclusive BSP reset before cache workers or live requests exist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_init() {
    unsafe {
        c::reset();
    }
}
/// # Safety
/// Existing boot worker-start admission; task entry/FPU and role stay native.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_start() -> bool {
    unsafe { worker::start() }
}
pub(crate) fn worker_cpus() -> u64 {
    worker::execution_cpus()
}
/// # Safety
/// Aligned writable output receives a batch only on successful cache admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_batch(out: *mut u64) -> bool {
    unsafe {
        let Some(_held) = c::Held::acquire() else {
            return false;
        };
        let s = c::state();
        (*s).batch_clock = (*s).batch_clock.wrapping_add(1);
        if (*s).batch_clock == 0 {
            (*s).batch_clock = 1;
        }
        out.write((*s).batch_clock);
        true
    }
}
/// # Safety
/// Exclusive aligned C output; timeout returns advisory scalar counters only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_summary(out: *mut Summary) {
    unsafe {
        out.write(snapshot());
    }
}
/// # Safety
/// Caller owns len writable bytes across real synchronous I/O and publication.
/// Pageable output is touched only after releasing all cache metadata owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_read_sector(
    device: usize,
    lba: u64,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe { read::read_sector(device, lba, out, len) }
}
/// # Safety
/// Same caller buffer lifetime as read_sector; count/overflow are checked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_read_sectors(
    device: usize,
    lba: u64,
    count: u32,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe { read::read_sectors(device, lba, count, out, len) }
}
/// # Safety
/// Caller owns len readable bytes. Input is staged before cache/pin admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_write_sector(
    device: usize,
    lba: u64,
    data: *const u8,
    len: usize,
    batch: u64,
) -> bool {
    unsafe { write::write_sector(device, lba, data, len, batch) }
}
/// # Safety
/// Exact caller bytes survive initial resident heap staging; pins/backing stay
/// owned until every real backend completion and acknowledged-prefix cleanup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_write_direct(
    device: usize,
    lba: u64,
    sectors: u16,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe { write::write_direct(device, lba, sectors, data, len) }
}
/// # Safety
/// Existing mounted-device and explicit dirty-batch lifetime through durability.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_flush_device(device: usize, batch: u64) -> bool {
    unsafe { wb::flush_device(device, batch) }
}
/// # Safety
/// Existing global flush context retains all device registrations through drain.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_flush_all() -> bool {
    unsafe { wb::flush_all() }
}
/// # Safety
/// Storage maintenance has closed admission for the exact authorized range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_flush_range(
    device: usize,
    first: u64,
    count: u64,
) -> bool {
    unsafe { wb::flush_range(device, first, count) }
}
/// # Safety
/// Exact reclaim context and exclusive aligned C output through admitted drain.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_reclaim(
    target: u32,
    dirty: bool,
    out: *mut wb::Reclaim,
) {
    unsafe {
        out.write(wb::reclaim(target, dirty));
    }
}
/// # Safety
/// Existing device owner retains the registration across coherence maintenance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_invalidate_sector(device: usize, lba: u64) {
    unsafe {
        wb::invalidate_sector(device, lba);
    }
}
/// # Safety
/// Existing unmount/retirement owner has closed new admission for this device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pagecache_invalidate_device(device: usize) {
    unsafe {
        wb::invalidate_device(device);
    }
}

pub(crate) fn diagnostic(op: u8, token: u64, value: u64, extra: u64) -> u64 {
    use crate::display::diag_screen as d;
    match op {
        0 => d::begin_resolvable(),
        1 => u64::from(d::resolve(token)),
        2 => {
            d::text(b"[PGCACHE] relock stalled slice=");
            d::number(value, false);
            d::end_line();
            0
        }
        3 => {
            let incident = d::begin_resolvable();
            d::text(b"[PGCACHE] dirty index fallback dev=");
            if value == u64::MAX {
                d::text(b"all")
            } else {
                d::number(value, false)
            }
            d::text(b" observed=");
            d::number(extra, false);
            d::end_line();
            let _ = d::resolve(incident);
            0
        }
        4 | 5 => {
            let incident = d::begin_resolvable();
            d::text(if op == 4 {
                b"[PGCACHE] busy read failed dev="
            } else {
                b"[PGCACHE] busy bulk read failed dev="
            });
            d::number(value, false);
            d::text(b" page=");
            d::number(extra, false);
            d::end_line();
            let _ = d::resolve(incident);
            0
        }
        _ => unreachable!(),
    }
}
