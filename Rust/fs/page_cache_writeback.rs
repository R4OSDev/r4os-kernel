//! Original bounded dirty FIFO drains, selective durability and acknowledged
//! writeback. Every I/O releases metadata ownership while exact pins remain.
use super::{page_cache_policy as policy, page_cache_state as c};
use crate::{
    platform::timer,
    storage::{block_io as block, block_registry},
};
use c::{COUNT, DEVICES, FULL, Held, PAGE, SECTOR, STATS};
use core::{mem::MaybeUninit, ptr, sync::atomic::Ordering::Relaxed};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reason {
    Flush,
    Pressure,
    BackgroundPressure,
    BackgroundAge,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Reclaim {
    pub requested_frames: u32,
    pub returned_frames: u32,
    pub returned_bytes: u64,
    pub dirty_drains: u64,
    pub failed_drains: u64,
}
pub(super) unsafe fn oldest() -> Option<usize> {
    unsafe {
        let device = c::optional(c::index(13, 0, 0, false))?;
        c::optional(c::index(11, 0, device, false))
    }
}
pub(super) unsafe fn has_busy() -> bool {
    unsafe { (*policy::global_index()).busy_count != 0 }
}
pub(super) unsafe fn has_busy_dirty() -> bool {
    unsafe {
        (*policy::global_index())
            .devices
            .iter()
            .any(|d| d.busy_dirty != 0)
    }
}
unsafe fn dirty_pages(device: usize) -> u32 {
    if device >= DEVICES {
        0
    } else {
        unsafe { u32::from((*policy::global_index()).devices[device].dirty) }
    }
}
pub(super) unsafe fn wait_device_idle(guard: bool, device: usize) -> bool {
    unsafe {
        if device >= DEVICES {
            return false;
        }
        while (*policy::global_index()).devices[device].busy_dirty != 0 {
            if !guard || !c::wait_busy(guard) {
                return false;
            }
        }
        true
    }
}
unsafe fn wait_all_idle(guard: bool) -> bool {
    unsafe {
        while has_busy_dirty() {
            if !guard || !c::wait_busy(guard) {
                return false;
            }
        }
        true
    }
}
unsafe fn dirty_sectors_batch(device: usize, batch: u64) -> u64 {
    unsafe {
        let mut count = 0u64;
        let mut cursor = c::optional(c::index(11, 0, device, false));
        while let Some(i) = cursor {
            count = count.wrapping_add(u64::from(c::batch_mask(i, batch).count_ones()));
            cursor = c::optional(c::index(12, i, 0, false));
        }
        count
    }
}
unsafe extern "C" {
    fn r4os_zig_pgcache_diagnostic(op: u8, token: u64, value: u64, extra: u64) -> u64;
}
unsafe fn fallback(device: Option<usize>) {
    unsafe {
        STATS.policy_full_scan_fallbacks.fetch_add(1, Relaxed);
        let mut observed = 0u64;
        for i in 0..COUNT {
            let e = c::entry(i);
            if (*e).valid && (*e).dirty_mask != 0 && device.is_none_or(|d| d == (*e).device) {
                observed = observed.wrapping_add(1);
            }
        }
        r4os_zig_pgcache_diagnostic(3, 0, device.map_or(u64::MAX, |d| d as u64), observed);
    }
}
pub(super) fn record(reason: Reason, written: u64, start: u64) {
    if written == 0 {
        return;
    }
    let now = timer::tick_count();
    let elapsed = now.saturating_sub(start);
    STATS.writeback_drains.fetch_add(1, Relaxed);
    match reason {
        Reason::Flush => {
            STATS.writeback_flush_drains.fetch_add(1, Relaxed);
        }
        Reason::Pressure => {
            STATS.writeback_pressure_drains.fetch_add(1, Relaxed);
            STATS.reclaim_dirty_drains.fetch_add(written, Relaxed);
        }
        _ => {}
    }
    STATS.writeback_last_ticks.store(elapsed, Relaxed);
    STATS.writeback_total_ticks.fetch_add(elapsed, Relaxed);
    STATS.writeback_max_ticks.fetch_max(elapsed, Relaxed);
}
pub(super) unsafe fn write_entry(guard: bool, i: usize, batch: Option<u64>, selected: u8) -> bool {
    unsafe {
        if i >= COUNT {
            return true;
        }
        let e = c::entry(i);
        loop {
            if !(*e).valid || !(*e).io_busy {
                break;
            }
            if !c::wait_busy(guard) {
                return false;
            }
        }
        if !(*e).valid {
            return true;
        }
        let dirty =
            batch.map_or_else(|| (*e).dirty_mask, |owner| c::batch_mask(i, owner)) & selected;
        if dirty == 0 {
            return true;
        }
        let device = (*e).device;
        let page = (*e).page;
        let frame = c::payload(i);
        if frame.is_null() || c::index(6, i, 0, false) == 0 {
            STATS.writeback_errors.fetch_add(1, Relaxed);
            return false;
        }
        (*e).io_busy = true;
        c::release(guard);
        let mut ok = true;
        let mut written = 0u8;
        let mut sector = 0usize;
        while sector < 8 {
            if dirty & (1u8 << sector) == 0 {
                sector += 1;
                continue;
            }
            let mut run = 1usize;
            while sector + run < 8 && dirty & (1u8 << (sector + run)) != 0 {
                run += 1;
            }
            let retry = block_registry::cache_view(device).is_some_and(|info| !info.owns_retry);
            let mut retried = false;
            loop {
                let mut transfer = block::Transfer::default();
                block::r4os_rust_block_write(
                    device,
                    page + sector as u64,
                    run as u16,
                    frame.add(sector * SECTOR),
                    run * SECTOR,
                    true,
                    &mut transfer,
                );
                if transfer.error == 0 && usize::from(transfer.sectors_completed) == run {
                    break;
                }
                if !retry || retried {
                    ok = false;
                    break;
                }
                retried = true;
                STATS.writeback_retries.fetch_add(1, Relaxed);
            }
            if !ok {
                break;
            }
            for k in sector..sector + run {
                written |= 1u8 << k;
            }
            sector += run;
        }
        c::relock(guard);
        (*e).io_busy = false;
        c::clear_dirty(i, written);
        c::index(7, i, 0, !ok || (*e).dirty_mask != 0);
        STATS
            .writeback_sectors
            .fetch_add(u64::from(written.count_ones()), Relaxed);
        if !ok {
            STATS.writeback_errors.fetch_add(1, Relaxed);
        }
        ok
    }
}
#[derive(Clone, Copy)]
struct Snapshot {
    index: usize,
    page: u64,
    frame: u64,
    sequences: [u64; 8],
    request: usize,
}
unsafe fn homogeneous(i: usize, batch: Option<u64>) -> Option<u64> {
    unsafe {
        let e = c::entry(i);
        if !(*e).valid || (*e).io_busy || (*e).dirty_mask != FULL {
            return None;
        }
        let owner = batch.unwrap_or((*e).owners[0]);
        if (*e).owners.iter().all(|v| *v == owner) {
            Some(owner)
        } else {
            None
        }
    }
}
unsafe fn window(guard: bool, first: usize, batch: Option<u64>) -> Option<bool> {
    unsafe {
        if first >= COUNT || (*c::state()).window_busy {
            return None;
        }
        let owner = homogeneous(first, batch)?;
        let device = (*c::entry(first)).device;
        let mut limits = MaybeUninit::<block_registry::WriteLimits>::uninit();
        if !block_registry::r4os_rust_block_registry_limits(device, limits.as_mut_ptr()) {
            return None;
        }
        let limits = limits.assume_init();
        let limit = 128u16.min(limits.max_sectors);
        if limit < 8 {
            return None;
        }
        let parallel = batch.is_none() && owner == 0;
        let mut pages = [MaybeUninit::<Snapshot>::uninit(); 16];
        let mut writes = [MaybeUninit::<block::Write>::uninit(); 4];
        let mut n = 0usize;
        let mut requests = 0usize;
        let mut candidate = Some(first);
        (*c::state()).window_busy = true;
        while let Some(i) = candidate {
            if n == 16 {
                break;
            }
            let e = c::entry(i);
            if (*e).device != device || homogeneous(i, batch) != Some(owner) {
                break;
            }
            let frame = c::payload(i);
            if frame.is_null() {
                break;
            }
            let mut extend = false;
            if requests != 0 {
                let last = writes[requests - 1].assume_init_ref();
                extend = last.lba <= u64::MAX - u64::from(last.sectors)
                    && last.lba + u64::from(last.sectors) == (*e).page
                    && u32::from(last.sectors) + 8 <= u32::from(limit);
            }
            if !extend && (requests == 4 || (!parallel && requests != 0)) {
                break;
            }
            if c::index(6, i, 0, false) == 0 {
                break;
            }
            (*e).io_busy = true;
            let offset = n * PAGE;
            ptr::copy_nonoverlapping(frame, c::window().add(offset), PAGE);
            if extend {
                let write = writes[requests - 1].assume_init_mut();
                write.sectors += 8;
                write.len = usize::from(write.sectors) * SECTOR;
            } else {
                writes[requests].write(block::Write {
                    lba: (*e).page,
                    sectors: 8,
                    data: c::window().add(offset),
                    len: PAGE,
                    attempted: false,
                    ok: false,
                });
                requests += 1;
            }
            pages[n].write(Snapshot {
                index: i,
                page: (*e).page,
                frame: (*e).phys,
                sequences: (*e).sequences,
                request: requests - 1,
            });
            n += 1;
            candidate = if let Some(batch) = batch {
                c::oldest_for_batch(device, batch)
            } else {
                c::optional(c::index(11, 0, device, false))
            };
        }
        if n < 2 {
            if n != 0 {
                (*c::entry(first)).io_busy = false;
                c::index(7, first, 0, true);
            }
            (*c::state()).window_busy = false;
            return None;
        }
        STATS.writeback_windows.fetch_add(1, Relaxed);
        STATS.writeback_window_pages.fetch_add(n as u64, Relaxed);
        STATS
            .writeback_window_requests
            .fetch_add(requests as u64, Relaxed);
        for w in &writes[..requests] {
            STATS
                .writeback_window_max_sectors
                .fetch_max(u64::from(w.assume_init_ref().sectors), Relaxed);
        }
        c::release(guard);
        let pending = block::r4os_rust_block_write_batch(
            device,
            &limits.reference,
            writes.as_mut_ptr().cast(),
            requests,
            parallel,
        );
        let mut retries = 0u64;
        if !limits.owns_retry {
            for w in &mut writes[..requests] {
                let job = w.as_mut_ptr();
                if (*job).ok || !(*job).attempted {
                    continue;
                }
                block::r4os_rust_block_write_batch(device, &limits.reference, job, 1, false);
                retries += 1;
            }
        }
        c::relock(guard);
        STATS.writeback_retries.fetch_add(retries, Relaxed);
        STATS
            .writeback_window_pending_max
            .fetch_max(pending as u64, Relaxed);
        let mut ok = true;
        for j in (0..n).rev() {
            let snapshot = pages[j].assume_init();
            let e = c::entry(snapshot.index);
            if !(*e).valid
                || (*e).device != device
                || (*e).page != snapshot.page
                || (*e).phys != snapshot.frame
                || !(*e).io_busy
            {
                ok = false;
                continue;
            }
            let request_ok = writes[snapshot.request].assume_init_ref().ok;
            let mut cleared = 0u8;
            if request_ok {
                for k in 0..8 {
                    if (*e).owners[k] == owner && (*e).sequences[k] == snapshot.sequences[k] {
                        cleared |= 1u8 << k;
                    }
                }
            }
            c::clear_dirty(snapshot.index, cleared);
            STATS
                .writeback_sectors
                .fetch_add(u64::from(cleared.count_ones()), Relaxed);
            (*e).io_busy = false;
            c::index(7, snapshot.index, 0, (*e).dirty_mask != 0);
            ok = ok && request_ok;
        }
        (*c::state()).window_busy = false;
        if !ok {
            STATS.writeback_errors.fetch_add(1, Relaxed);
        }
        Some(ok)
    }
}
unsafe fn drain_entry(guard: bool, i: usize, batch: Option<u64>) -> bool {
    unsafe { window(guard, i, batch).unwrap_or_else(|| write_entry(guard, i, batch, FULL)) }
}
pub(super) unsafe fn drain_device(guard: bool, device: usize, reason: Reason) -> bool {
    unsafe {
        if dirty_pages(device) == 0 {
            return true;
        }
        let start = timer::tick_count();
        STATS.writeback_waits.fetch_add(1, Relaxed);
        let mut written = 0u64;
        while dirty_pages(device) != 0 {
            let Some(i) = c::optional(c::index(11, 0, device, false)) else {
                if device < DEVICES && (*policy::global_index()).devices[device].busy_dirty != 0 {
                    if !wait_device_idle(guard, device) {
                        return false;
                    }
                    continue;
                }
                fallback(Some(device));
                return false;
            };
            let before = STATS.writeback_sectors.load(Relaxed);
            if !drain_entry(guard, i, None) {
                return false;
            }
            written = written.wrapping_add(STATS.writeback_sectors.load(Relaxed) - before);
        }
        record(reason, written, start);
        true
    }
}
unsafe fn drain_batch(guard: bool, device: usize, batch: u64, reason: Reason) -> bool {
    unsafe {
        if !wait_device_idle(guard, device) {
            return false;
        }
        if dirty_sectors_batch(device, batch) == 0 {
            return true;
        }
        let start = timer::tick_count();
        STATS.writeback_waits.fetch_add(1, Relaxed);
        let mut written = 0u64;
        loop {
            let Some(i) = c::oldest_for_batch(device, batch) else {
                if device < DEVICES && (*policy::global_index()).devices[device].busy_dirty != 0 {
                    if !wait_device_idle(guard, device) {
                        return false;
                    }
                    continue;
                }
                break;
            };
            let before = STATS.writeback_sectors.load(Relaxed);
            if !drain_entry(guard, i, Some(batch)) {
                return false;
            }
            written = written.wrapping_add(STATS.writeback_sectors.load(Relaxed) - before);
        }
        STATS
            .selective_writeback_sectors
            .fetch_add(written, Relaxed);
        record(reason, written, start);
        true
    }
}
unsafe fn drain_all(guard: bool, reason: Reason) -> bool {
    unsafe {
        if (*c::state()).dirty_pages == 0 {
            return true;
        }
        let start = timer::tick_count();
        STATS.writeback_waits.fetch_add(1, Relaxed);
        let mut written = 0u64;
        while (*c::state()).dirty_pages != 0 {
            let Some(i) = oldest() else {
                if has_busy_dirty() {
                    if !wait_all_idle(guard) {
                        return false;
                    }
                    continue;
                }
                fallback(None);
                return false;
            };
            let before = STATS.writeback_sectors.load(Relaxed);
            if !drain_entry(guard, i, None) {
                return false;
            }
            written = written.wrapping_add(STATS.writeback_sectors.load(Relaxed) - before);
        }
        record(reason, written, start);
        true
    }
}
pub(super) unsafe fn write_oldest(guard: bool, reason: Reason) -> bool {
    unsafe {
        let Some(i) = oldest() else { return true };
        let start = timer::tick_count();
        STATS.writeback_waits.fetch_add(1, Relaxed);
        let before = STATS.writeback_sectors.load(Relaxed);
        if !write_entry(guard, i, None, FULL) {
            if reason == Reason::Pressure {
                STATS.reclaim_failed_drains.fetch_add(1, Relaxed);
            }
            return false;
        }
        record(
            reason,
            STATS.writeback_sectors.load(Relaxed) - before,
            start,
        );
        true
    }
}
pub(super) unsafe fn reclaim_locked(guard: bool, target: u32, dirty: bool) -> Reclaim {
    unsafe {
        let mut out = Reclaim {
            requested_frames: target,
            ..Reclaim::default()
        };
        if target == 0 {
            return out;
        }
        STATS.reclaim_scans.fetch_add(1, Relaxed);
        while out.returned_frames < target {
            if let Some(i) = c::optional(c::index(10, 0, usize::MAX, false)) {
                STATS.evictions.fetch_add(1, Relaxed);
                STATS.reclaim_clean_entries.fetch_add(1, Relaxed);
                c::clear(i, true);
                out.returned_frames += 1;
                out.returned_bytes = out.returned_bytes.wrapping_add(PAGE as u64);
                continue;
            }
            if !dirty {
                break;
            }
            let before = STATS.reclaim_dirty_drains.load(Relaxed);
            let failures = STATS.reclaim_failed_drains.load(Relaxed);
            if !write_oldest(guard, Reason::Pressure) {
                out.failed_drains = out
                    .failed_drains
                    .wrapping_add(STATS.reclaim_failed_drains.load(Relaxed) - failures);
                break;
            }
            let drained = STATS.reclaim_dirty_drains.load(Relaxed) - before;
            if drained == 0 {
                break;
            }
            out.dirty_drains = out.dirty_drains.wrapping_add(drained);
        }
        out
    }
}
pub(super) unsafe fn reclaim(target: u32, dirty: bool) -> Reclaim {
    unsafe {
        let empty = Reclaim {
            requested_frames: target,
            ..Reclaim::default()
        };
        let Some(held) = Held::acquire() else {
            return empty;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return empty;
        };
        reclaim_locked(held.owned, target, dirty)
    }
}
pub(super) unsafe fn flush_device(device: usize, batch: u64) -> bool {
    unsafe {
        let Some(mut held) = Held::acquire() else {
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return false;
        };
        STATS.flushes.fetch_add(1, Relaxed);
        let ok = if batch == 0 {
            drain_device(held.owned, device, Reason::Flush)
        } else {
            STATS.selective_flushes.fetch_add(1, Relaxed);
            if !wait_device_idle(held.owned, device) {
                return false;
            }
            let all = if device < DEVICES {
                (*c::state()).device_dirty[device]
            } else {
                0
            };
            let own = dirty_sectors_batch(device, batch);
            if all > own {
                STATS
                    .selective_foreign_dirty_sectors_skipped
                    .fetch_add(all - own, Relaxed);
            }
            drain_batch(held.owned, device, batch, Reason::Flush)
        };
        if !ok {
            return false;
        }
        held.unlock();
        if !block::r4os_rust_block_flush(device) {
            STATS.writeback_errors.fetch_add(1, Relaxed);
            return false;
        }
        true
    }
}
pub(super) unsafe fn flush_all() -> bool {
    unsafe {
        let Some(mut held) = Held::acquire() else {
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return false;
        };
        if !drain_all(held.owned, Reason::Flush) {
            return false;
        }
        held.unlock();
        let count = block_registry::r4os_rust_block_registry_query(1, 0);
        for device in 0..count {
            if block_registry::cache_view(device).is_none() {
                continue;
            }
            STATS.flushes.fetch_add(1, Relaxed);
            if !block::r4os_rust_block_flush(device) {
                STATS.writeback_errors.fetch_add(1, Relaxed);
                return false;
            }
        }
        true
    }
}
fn range_mask(page: u64, first: u64, count: u64) -> u8 {
    let mut mask = 0u8;
    for i in 0..8 {
        if page > u64::MAX - i {
            break;
        }
        let lba = page + i;
        if lba >= first && lba - first < count {
            mask |= 1u8 << i;
        }
    }
    mask
}
pub(super) unsafe fn flush_range(device: usize, first: u64, count: u64) -> bool {
    unsafe {
        let Some(info) = block_registry::cache_view(device) else {
            return false;
        };
        if count == 0 || first >= info.sectors || count > info.sectors - first {
            return false;
        }
        let Some(mut held) = Held::acquire() else {
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            return false;
        };
        loop {
            let mut restart = false;
            for i in 0..COUNT {
                let e = c::entry(i);
                if !(*e).valid || (*e).device != device {
                    continue;
                }
                let mask = range_mask((*e).page, first, count);
                if mask == 0 {
                    continue;
                }
                if (*e).io_busy {
                    if !held.owned || !c::wait_busy(held.owned) {
                        return false;
                    }
                    restart = true;
                    break;
                }
                if (*e).dirty_mask & mask != 0 {
                    if !write_entry(held.owned, i, None, mask) {
                        return false;
                    }
                    restart = true;
                    break;
                }
                (*e).valid_mask &= !mask;
                STATS.invalidations.fetch_add(1, Relaxed);
                if (*e).valid_mask == 0 {
                    c::clear(i, false);
                }
            }
            if !restart {
                break;
            }
        }
        held.unlock();
        let durable = block::r4os_rust_block_flush(device);
        held.relock();
        if !durable {
            STATS.writeback_errors.fetch_add(1, Relaxed);
        }
        durable
    }
}
pub(super) unsafe fn invalidate_sector(device: usize, lba: u64) {
    unsafe {
        let Some(held) = Held::acquire() else { return };
        let Some(_unwind) = c::Operation::enter() else {
            return;
        };
        let bit = c::bit(lba);
        loop {
            let Some(i) = c::find(device, c::page(lba)) else {
                return;
            };
            let e = c::entry(i);
            if (*e).io_busy {
                if !c::wait_busy(held.owned) {
                    return;
                }
                continue;
            }
            if (*e).dirty_mask & bit != 0 {
                if !write_entry(held.owned, i, None, FULL) {
                    return;
                }
                continue;
            }
            (*e).valid_mask &= !bit;
            c::clear_dirty(i, bit);
            STATS.invalidations.fetch_add(1, Relaxed);
            if (*e).valid_mask == 0 {
                c::clear(i, false);
            }
            return;
        }
    }
}
pub(super) unsafe fn invalidate_device(device: usize) {
    unsafe {
        let Some(held) = Held::acquire() else { return };
        let Some(_unwind) = c::Operation::enter() else {
            return;
        };
        if device < DEVICES
            && policy::r4os_rust_fs_cache_readahead(
                ptr::addr_of_mut!((*c::state()).readahead)
                    .cast::<policy::ReadAhead>()
                    .add(device),
                2,
                0,
                0,
                ptr::null(),
                false,
                ptr::null_mut(),
            )
        {
            STATS.read_ahead_cancellations.fetch_add(1, Relaxed);
        }
        loop {
            let mut found = None;
            for i in 0..COUNT {
                let e = c::entry(i);
                if (*e).valid && (*e).device == device {
                    found = Some(i);
                    break;
                }
            }
            let Some(i) = found else { return };
            let e = c::entry(i);
            if (*e).io_busy {
                if !c::wait_busy(held.owned) {
                    return;
                }
                continue;
            }
            if (*e).dirty_mask != 0 {
                if !write_entry(held.owned, i, None, FULL) {
                    return;
                }
                continue;
            }
            c::clear(i, false);
            STATS.invalidations.fetch_add(1, Relaxed);
        }
    }
}
