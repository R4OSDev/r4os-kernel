//! Original deferred sector writes and acknowledged-prefix direct writes.
use super::{page_cache_state as c, page_cache_writeback as wb};
use crate::{memory::heap, storage::block_io as block};
use c::{COUNT, Held, SECTOR, STATS, Staging};
use core::{mem::MaybeUninit, ptr, sync::atomic::Ordering::Relaxed};
pub(super) unsafe fn write_sector(
    device: usize,
    lba: u64,
    data: *const u8,
    len: usize,
    batch: u64,
) -> bool {
    unsafe {
        if len < SECTOR {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        let mut snapshot = [MaybeUninit::<u8>::uninit(); SECTOR];
        ptr::copy_nonoverlapping(data, snapshot.as_mut_ptr().cast::<u8>(), SECTOR);
        let Some(held) = Held::acquire() else {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        };
        let Some(_unwind) = c::Operation::enter() else {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        };
        let bit = c::bit(lba);
        let page = c::page(lba);
        let mut busy = 0usize;
        loop {
            let chosen = c::find(device, page).or_else(|| c::create(device, page));
            let Some(i) = chosen else {
                if wb::oldest().is_some() {
                    if !wb::write_oldest(held.owned, wb::Reason::Pressure) {
                        STATS.write_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    busy = 0;
                    continue;
                }
                if wb::has_busy() {
                    busy += 1;
                    if busy > c::BUSY_TICKS || !c::wait_busy(held.owned) {
                        STATS.write_errors.fetch_add(1, Relaxed);
                        return false;
                    }
                    continue;
                }
                STATS.write_errors.fetch_add(1, Relaxed);
                return false;
            };
            let e = c::entry(i);
            if (*e).io_busy {
                if !c::wait_busy(held.owned) {
                    STATS.write_errors.fetch_add(1, Relaxed);
                    return false;
                }
                continue;
            }
            let frame = c::payload(i);
            if frame.is_null() {
                STATS.write_errors.fetch_add(1, Relaxed);
                return false;
            }
            c::drop_readahead(i);
            if !c::mark_dirty(i, bit, batch) {
                STATS.write_errors.fetch_add(1, Relaxed);
                return false;
            }
            ptr::copy_nonoverlapping(
                snapshot.as_ptr().cast::<u8>(),
                frame.add(c::offset(lba)),
                SECTOR,
            );
            (*e).valid_mask |= bit;
            (*e).last_use = c::next_clock();
            STATS.dirty_sector_updates.fetch_add(1, Relaxed);
            STATS.deferred_write_requests.fetch_add(1, Relaxed);
            return true;
        }
    }
}
unsafe fn release_pins(owned: &mut [u64; COUNT / 64], reserved: &mut [u64; COUNT / 64]) {
    unsafe {
        for i in 0..COUNT {
            let bit = 1u64 << (i % 64);
            if owned[i / 64] & bit == 0 {
                continue;
            }
            let e = c::entry(i);
            if (*e).io_busy {
                (*e).io_busy = false;
                c::index(7, i, 0, (*e).dirty_mask != 0);
            }
            if reserved[i / 64] & bit != 0
                && (*e).valid
                && (*e).valid_mask == 0
                && (*e).dirty_mask == 0
            {
                c::clear(i, false);
            }
            owned[i / 64] &= !bit;
            reserved[i / 64] &= !bit;
        }
    }
}
pub(super) unsafe fn write_direct(
    device: usize,
    lba: u64,
    sectors: u16,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        if sectors == 0 {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        let count = usize::from(sectors);
        let bytes = count * SECTOR;
        if len < bytes {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        if count > 1 {
            STATS.bulk_write_requests.fetch_add(1, Relaxed);
            STATS.bulk_write_sectors.fetch_add(count as u64, Relaxed);
        }
        let Some(_unwind) = c::Operation::enter() else {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        };
        let staging = Staging {
            data: heap::r4os_rust_heap_alloc(bytes, 16),
            len: bytes,
        };
        if staging.data.is_null() {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        // Resident heap for every transfer size, including a single sector.
        // Never restore the historical 4-KB stack fallback in deep async I/O.
        ptr::copy_nonoverlapping(data, staging.data, bytes);
        let Some(mut held) = Held::acquire() else {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        };
        let mut owned = [0u64; COUNT / 64];
        let mut reserved = [0u64; COUNT / 64];
        let mut offset = 0usize;
        while offset < count {
            let cur = lba + offset as u64;
            let page = c::page(cur);
            let existing = c::find(device, page);
            let mut created = false;
            let i = if let Some(i) = existing {
                i
            } else {
                let Some(i) = c::create(device, page) else {
                    release_pins(&mut owned, &mut reserved);
                    STATS.write_errors.fetch_add(1, Relaxed);
                    return false;
                };
                created = true;
                i
            };
            let e = c::entry(i);
            if (*e).io_busy {
                if !c::wait_busy(held.owned) {
                    release_pins(&mut owned, &mut reserved);
                    STATS.write_errors.fetch_add(1, Relaxed);
                    return false;
                }
                continue;
            }
            c::drop_readahead(i);
            if c::index(6, i, 0, false) == 0 {
                release_pins(&mut owned, &mut reserved);
                STATS.write_errors.fetch_add(1, Relaxed);
                return false;
            }
            (*e).io_busy = true;
            let bit = 1u64 << (i % 64);
            owned[i / 64] |= bit;
            if created {
                reserved[i / 64] |= bit;
            }
            offset += (8 - (cur - page) as usize).min(count - offset);
        }
        STATS.write_through_requests.fetch_add(1, Relaxed);
        held.unlock();
        let mut result = block::Transfer::default();
        block::r4os_rust_block_write(device, lba, sectors, staging.data, bytes, true, &mut result);
        let completed = usize::from(result.sectors_completed).min(count);
        let ok = result.error == 0 && completed == count;
        held.relock();
        offset = 0;
        while offset < count {
            let cur = lba + offset as u64;
            let page = c::page(cur);
            let first = (cur - page) as usize;
            let span = (8 - first).min(count - offset);
            if let Some(i) = c::find(device, page) {
                let bit = 1u64 << (i % 64);
                let e = c::entry(i);
                let own = owned[i / 64] & bit != 0
                    && (*e).valid
                    && (*e).device == device
                    && (*e).page == page
                    && (*e).io_busy;
                let done = if completed > offset {
                    span.min(completed - offset)
                } else {
                    0
                };
                if own && done != 0 {
                    for sector in first..first + done {
                        let mask = 1u8 << sector;
                        (*e).valid_mask &= !mask;
                        c::clear_dirty(i, mask);
                        STATS.invalidations.fetch_add(1, Relaxed);
                    }
                }
                if own {
                    (*e).io_busy = false;
                    c::index(7, i, 0, (*e).dirty_mask != 0);
                    owned[i / 64] &= !bit;
                    let was_reserved = reserved[i / 64] & bit != 0;
                    reserved[i / 64] &= !bit;
                    if (done != 0 || was_reserved) && (*e).valid_mask == 0 && (*e).dirty_mask == 0 {
                        c::clear(i, false);
                    }
                }
            }
            offset += span;
        }
        // Unknown identities remain pinned. Never release a possibly foreign slot.
        if owned.iter().any(|w| *w != 0) || reserved.iter().any(|w| *w != 0) {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        STATS
            .write_through_updates
            .fetch_add(completed as u64, Relaxed);
        if !ok {
            STATS.write_errors.fetch_add(1, Relaxed);
            return false;
        }
        true
    }
}
