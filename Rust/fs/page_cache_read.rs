//! Original demand-read/fill paths, with resident caller staging and exact pins.
use super::{
    page_cache_policy as policy, page_cache_state as c, page_cache_worker as worker,
    page_cache_writeback as wb,
};
use crate::{
    memory::heap,
    storage::{block_io as block, block_registry},
};
use c::{COUNT, FULL, Held, PAGE, SECTOR, STATS, Staging};
use core::{mem::MaybeUninit, ptr, sync::atomic::Ordering::Relaxed};
pub(super) unsafe fn fill(guard: bool, i: usize) -> bool {
    unsafe {
        if fill_once(guard, i) {
            return true;
        }
        let dev = (*c::entry(i)).device;
        let Some(info) = block_registry::cache_view(dev) else {
            return false;
        };
        if info.owns_retry {
            return false;
        }
        fill_once(guard, i)
    }
}
unsafe fn fill_once(guard: bool, i: usize) -> bool {
    unsafe {
        let e = c::entry(i);
        if (*e).valid_mask == FULL {
            return true;
        }
        let device = (*e).device;
        let page = (*e).page;
        let before = (*e).valid_mask;
        let frame = c::payload(i);
        if frame.is_null() {
            return false;
        }
        let Some(info) = block_registry::cache_view(device) else {
            return false;
        };
        if info.sector_bytes != SECTOR as u32 {
            return false;
        }
        let readable = if info.sectors == 0 {
            8
        } else {
            if page >= info.sectors {
                return false;
            }
            (info.sectors - page).min(8) as usize
        };
        let mask = readable_mask(readable);
        if mask & !before == 0 {
            return false;
        }
        if c::index(6, i, 0, false) == 0 {
            return false;
        }
        (*e).io_busy = true;
        c::release(guard);
        let mut ok = true;
        if before == 0 {
            ok = block::r4os_rust_block_read(
                device,
                page,
                readable as u16,
                frame,
                readable * SECTOR,
                true,
            );
        } else {
            let mut buffer = [MaybeUninit::<u8>::uninit(); SECTOR];
            for sector in 0..readable {
                if before & (1 << sector) != 0 {
                    continue;
                }
                if !block::r4os_rust_block_read(
                    device,
                    page + sector as u64,
                    1,
                    buffer.as_mut_ptr().cast(),
                    SECTOR,
                    true,
                ) {
                    ok = false;
                    break;
                }
                ptr::copy_nonoverlapping(
                    buffer.as_ptr().cast::<u8>(),
                    frame.add(sector * SECTOR),
                    SECTOR,
                );
            }
        }
        c::relock(guard);
        (*e).io_busy = false;
        c::index(7, i, 0, (*e).dirty_mask != 0);
        if ok {
            (*e).valid_mask = before | mask;
            (*e).last_use = c::next_clock();
            STATS.fills.fetch_add(1, Relaxed);
        }
        ok
    }
}
#[derive(Default)]
pub(super) struct FillOutcome {
    pub ok: bool,
    pub pages: u16,
    #[allow(dead_code)] // Original dormant model retains the completed-sector count.
    pub sectors: u16,
}
pub(super) unsafe fn run_limit(device: usize, requested: u16) -> u16 {
    unsafe {
        let Some(info) = block_registry::cache_view(device) else {
            return 0;
        };
        if info.sector_bytes != SECTOR as u32 {
            return 0;
        }
        let capacity = c::capacity();
        policy::fill_run_page_limit(requested.min(2), info.max_sectors, 8, capacity.active_pages)
    }
}
pub(super) fn readable_mask(sectors: usize) -> u8 {
    if sectors >= 8 {
        FULL
    } else {
        ((1u16 << sectors) - 1) as u8
    }
}
pub(super) unsafe fn fill_run(
    guard: bool,
    device: usize,
    first: u64,
    requested: u16,
    staging: *mut u8,
    len: usize,
    speculative: bool,
) -> FillOutcome {
    unsafe {
        let Some(info) = block_registry::cache_view(device) else {
            return FillOutcome::default();
        };
        if info.sector_bytes != SECTOR as u32 {
            return FillOutcome::default();
        }
        let sector_count = info.sectors;
        let limit = ((len / PAGE).min(COUNT) as u16).min(run_limit(device, requested));
        if limit < 2 {
            return FillOutcome::default();
        }
        let mut indices = [MaybeUninit::<u16>::uninit(); COUNT];
        let mut masks = [MaybeUninit::<u8>::uninit(); COUNT];
        let mut reserved = 0usize;
        let mut total = 0u16;
        let mut page = first;
        while reserved < usize::from(limit) {
            if sector_count != 0 && page >= sector_count {
                break;
            }
            let readable = if sector_count == 0 {
                8
            } else {
                (sector_count - page).min(8) as usize
            };
            let mask = readable_mask(readable);
            let existing = c::find(device, page);
            if speculative && existing.is_some() {
                break;
            }
            let chosen = existing.or_else(|| {
                if speculative {
                    worker::create_readahead(device, page)
                } else {
                    c::create(device, page)
                }
            });
            let Some(i) = chosen else { break };
            let e = c::entry(i);
            if (*e).io_busy || (*e).valid_mask & mask == mask {
                break;
            }
            if c::index(6, i, 0, false) == 0 {
                break;
            }
            (*e).io_busy = true;
            if !speculative {
                c::drop_readahead(i);
            }
            indices[reserved].write(i as u16);
            masks[reserved].write((*e).valid_mask);
            reserved += 1;
            total += readable as u16;
            if readable != 8 || page > u64::MAX - 8 {
                break;
            }
            page += 8;
        }
        if reserved < 2 {
            if reserved == 0 {
                return FillOutcome::default();
            }
            let i = usize::from(indices[0].assume_init());
            let e = c::entry(i);
            (*e).io_busy = false;
            c::index(7, i, 0, (*e).dirty_mask != 0);
            return FillOutcome {
                ok: fill(guard, i),
                pages: 1,
                sectors: total.min(8),
            };
        }
        let bytes = usize::from(total) * SECTOR;
        STATS.fill_run_requests.fetch_add(1, Relaxed);
        STATS.fill_run_pages.fetch_add(reserved as u64, Relaxed);
        STATS.fill_run_sectors.fetch_add(u64::from(total), Relaxed);
        STATS.fill_run_bytes.fetch_add(bytes as u64, Relaxed);
        STATS.fill_run_max_pages.fetch_max(reserved as u64, Relaxed);
        STATS.fill_lock_drops.fetch_add(1, Relaxed);
        c::release(guard);
        let mut ok = false;
        let mut calls = 0u64;
        let mut retries = 0u64;
        let attempts = policy::r4os_rust_fs_cache_fill_attempts(info.owns_retry);
        for attempt in 0..attempts {
            calls = calls.wrapping_add(1);
            if block::r4os_rust_block_read(device, first, total, staging, bytes, true) {
                ok = true;
                break;
            }
            if attempt + 1 < attempts {
                retries = retries.wrapping_add(1);
            }
        }
        c::relock(guard);
        STATS.fill_run_backend_requests.fetch_add(calls, Relaxed);
        STATS.fill_run_retries.fetch_add(retries, Relaxed);
        for (slot, index) in indices.iter().enumerate().take(reserved) {
            let i = usize::from(index.assume_init());
            let e = c::entry(i);
            if !(*e).valid
                || (*e).device != device
                || (*e).page != first + slot as u64 * 8
                || !(*e).io_busy
                || c::payload(i).is_null()
            {
                ok = false;
            }
        }
        if !ok {
            STATS.fill_run_failures.fetch_add(1, Relaxed);
        }
        let mut offset = 0usize;
        for (slot, index) in indices.iter().enumerate().take(reserved) {
            let i = usize::from(index.assume_init());
            let e = c::entry(i);
            let page = first + slot as u64 * 8;
            let identity = (*e).valid && (*e).device == device && (*e).page == page && (*e).io_busy;
            if !identity {
                ok = false;
                continue;
            }
            let readable = if sector_count == 0 {
                8
            } else {
                (sector_count - page).min(8) as usize
            };
            let mask = readable_mask(readable);
            if ok {
                let frame = c::payload(i);
                let before = masks[slot].assume_init();
                for sector in 0..readable {
                    if (mask & !before) & (1 << sector) == 0 {
                        continue;
                    }
                    ptr::copy_nonoverlapping(
                        staging.add((offset + sector) * SECTOR),
                        frame.add(sector * SECTOR),
                        SECTOR,
                    );
                    STATS
                        .fill_scatter_copy_bytes
                        .fetch_add(SECTOR as u64, Relaxed);
                }
                (*e).valid_mask = before | mask;
                (*e).last_use = c::next_clock();
                STATS.fills.fetch_add(1, Relaxed);
            }
            offset += readable;
            (*e).io_busy = false;
            c::index(7, i, 0, !ok || (*e).dirty_mask != 0);
            if !ok && (*e).valid_mask == 0 && (*e).dirty_mask == 0 {
                c::clear(i, false);
            }
        }
        FillOutcome {
            ok,
            pages: reserved as u16,
            sectors: total,
        }
    }
}
unsafe extern "C" {
    fn r4os_zig_pgcache_diagnostic(op: u8, token: u64, value: u64, extra: u64) -> u64;
}
pub(super) unsafe fn read_sector(device: usize, lba: u64, out: *mut u8, len: usize) -> bool {
    unsafe {
        if len < SECTOR {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        }
        let mut snapshot = [MaybeUninit::<u8>::uninit(); SECTOR];
        let Some(mut held) = Held::acquire() else {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        };
        STATS.reads.fetch_add(1, Relaxed);
        let bit = c::bit(lba);
        let page = c::page(lba);
        worker::demand_start(device, page);
        let mut missed = false;
        let mut busy = 0usize;
        loop {
            let chosen = c::find(device, page).or_else(|| c::create(device, page));
            let Some(i) = chosen else {
                if !missed {
                    STATS.misses.fetch_add(1, Relaxed);
                    missed = true;
                }
                if wb::oldest().is_some() {
                    if !wb::write_oldest(held.owned, wb::Reason::Pressure) {
                        STATS.read_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    busy = 0;
                    continue;
                }
                if wb::has_busy() {
                    busy += 1;
                    if busy > c::BUSY_TICKS || !c::wait_busy(held.owned) {
                        STATS.read_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    continue;
                }
                STATS.read_errors.fetch_add(1, Relaxed);
                return false;
            };
            let e = c::entry(i);
            if (*e).io_busy {
                busy += 1;
                if busy > c::BUSY_TICKS {
                    STATS.read_errors.fetch_add(1, Relaxed);
                    r4os_zig_pgcache_diagnostic(4, 0, device as u64, page);
                    return false;
                }
                if !c::wait_busy(held.owned) {
                    STATS.read_errors.fetch_add(1, Relaxed);
                    return false;
                }
                continue;
            }
            if (*e).valid_mask & bit != 0 {
                let frame = c::payload(i);
                if frame.is_null() {
                    STATS.read_errors.fetch_add(1, Relaxed);
                    return false;
                }
                (*e).last_use = c::next_clock();
                worker::demand_hit(i);
                ptr::copy_nonoverlapping(
                    frame.add(c::offset(lba)),
                    snapshot.as_mut_ptr().cast::<u8>(),
                    SECTOR,
                );
                STATS
                    .read_staging_copy_bytes
                    .fetch_add(SECTOR as u64, Relaxed);
                STATS
                    .read_caller_copy_bytes
                    .fetch_add(SECTOR as u64, Relaxed);
                STATS.read_publish_lock_drops.fetch_add(1, Relaxed);
                if !missed {
                    STATS.hits.fetch_add(1, Relaxed);
                }
                held.unlock();
                ptr::copy_nonoverlapping(snapshot.as_ptr().cast::<u8>(), out, SECTOR);
                return true;
            }
            if !missed {
                STATS.misses.fetch_add(1, Relaxed);
                missed = true;
            }
            if !fill(held.owned, i) {
                if (*e).valid && (*e).valid_mask == 0 && !(*e).io_busy {
                    c::clear(i, false);
                }
                STATS.read_errors.fetch_add(1, Relaxed);
                return false;
            }
        }
    }
}
pub(super) fn covers(mask: u8, first: usize, len: usize) -> bool {
    for sector in first..first + len {
        if mask & (1u8 << sector) == 0 {
            return false;
        }
    }
    true
}
pub(super) unsafe fn read_sectors(
    device: usize,
    lba: u64,
    count: u32,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        if count == 0 {
            return true;
        }
        let total = count as usize * SECTOR;
        if len < total {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        }
        let end = lba.wrapping_add(u64::from(count));
        if end <= lba {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        }
        let final_page = c::page(end - 1);
        let mut snapshot = [MaybeUninit::<u8>::uninit(); PAGE];
        let mut staging = Staging {
            data: ptr::null_mut(),
            len: 0,
        };
        let mut unavailable = false;
        let Some(mut held) = Held::acquire() else {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            STATS.read_errors.fetch_add(1, Relaxed);
            return false;
        };
        worker::demand_start(device, c::page(lba));
        let mut done = 0u32;
        while done < count {
            let cur = lba + u64::from(done);
            let page = c::page(cur);
            let first = (cur - page) as usize;
            let span = (count - done) as usize;
            let span = span.min(8 - first);
            STATS.reads.fetch_add(span as u64, Relaxed);
            let mut busy = 0usize;
            let mut missed = false;
            loop {
                let chosen = c::find(device, page).or_else(|| c::create(device, page));
                let Some(i) = chosen else {
                    if wb::oldest().is_some() {
                        if !wb::write_oldest(held.owned, wb::Reason::Pressure) {
                            STATS.read_errors.fetch_add(1, Relaxed);
                            return false;
                        }
                        busy = 0;
                        continue;
                    }
                    if wb::has_busy() {
                        busy += 1;
                        if busy > c::BUSY_TICKS || !c::wait_busy(held.owned) {
                            STATS.read_errors.fetch_add(1, Relaxed);
                            return false;
                        }
                        continue;
                    }
                    STATS.read_errors.fetch_add(1, Relaxed);
                    return false;
                };
                let e = c::entry(i);
                if (*e).io_busy {
                    busy += 1;
                    if busy > c::BUSY_TICKS {
                        r4os_zig_pgcache_diagnostic(5, 0, device as u64, page);
                        STATS.read_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    if !c::wait_busy(held.owned) {
                        STATS.read_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    continue;
                }
                if !covers((*e).valid_mask, first, span) {
                    if !missed {
                        STATS.misses.fetch_add(span as u64, Relaxed);
                        missed = true;
                    }
                    let requested = ((final_page - page) / 8 + 1).min(COUNT as u64) as u16;
                    let run = run_limit(device, requested);
                    if run >= 2 && staging.data.is_null() && !unavailable {
                        held.unlock();
                        staging.len = usize::from(run) * PAGE;
                        staging.data = heap::r4os_rust_heap_alloc(staging.len, 16);
                        if staging.data.is_null() {
                            unavailable = true
                        }
                        held.relock();
                        continue;
                    }
                    let ok = if run >= 2 && !staging.data.is_null() {
                        fill_run(
                            held.owned,
                            device,
                            page,
                            run,
                            staging.data,
                            staging.len,
                            false,
                        )
                        .ok
                    } else {
                        fill(held.owned, i)
                    };
                    if !ok || !covers((*e).valid_mask, first, span) {
                        STATS.read_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                }
                let frame = c::payload(i);
                if frame.is_null() {
                    STATS.read_errors.fetch_add(1, Relaxed);
                    return false;
                }
                let bytes = span * SECTOR;
                ptr::copy_nonoverlapping(
                    frame.add(first * SECTOR),
                    snapshot.as_mut_ptr().cast::<u8>(),
                    bytes,
                );
                STATS
                    .read_staging_copy_bytes
                    .fetch_add(bytes as u64, Relaxed);
                STATS
                    .read_caller_copy_bytes
                    .fetch_add(bytes as u64, Relaxed);
                STATS.read_publish_lock_drops.fetch_add(1, Relaxed);
                (*e).last_use = c::next_clock();
                worker::demand_hit(i);
                if !missed {
                    STATS.hits.fetch_add(span as u64, Relaxed);
                }
                held.unlock();
                ptr::copy_nonoverlapping(
                    snapshot.as_ptr().cast::<u8>(),
                    out.add(done as usize * SECTOR),
                    bytes,
                );
                held.relock();
                break;
            }
            done += span as u32;
        }
        worker::schedule_readahead(device, lba, count);
        true
    }
}
