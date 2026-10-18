//! Confirmed PAT selectors, effective MTRRs and canonical device aliases.
//! Cacheable RAM is never silently converted under live users. Existing MMIO
//! window owners separately retain their exact extent/overlap/release guards.
use super::*;
use crate::arch::{memory_types as types, pat};

fn device_range(physical: u64, bytes: u64) -> bool {
    let Some(end) = physical.checked_add(bytes) else {
        return false;
    };
    let boot = unsafe { &*crate::boot::info::r4os_rust_boot_info() };
    for index in 0..boot.memory_count {
        let region = unsafe { boot.memory.add(index).read() };
        if region.valid
            && physical < region.end
            && region.base < end
            && !matches!(region.kind, 1 | 7)
        {
            return false;
        }
    }
    true
}
fn invalidate_types(base: u64, pages: u64) -> bool {
    let mut done = 0;
    while done < pages {
        // Below the existing CR3-reload threshold, the local and remote
        // owners use INVLPG, including global entries/large translations.
        let count = (pages - done).min(63);
        if !invalidate(base + done * PAGE, count) {
            return false;
        }
        done += count;
    }
    true
}
fn aliases(
    table_physical: u64,
    level: u32,
    prefix: u64,
    first: u64,
    end: u64,
    hhdm: u64,
    change_canonical: bool,
) -> bool {
    let shift = 12 + level * 9;
    for slot in 0..512 {
        let value = read(at(table(table_physical), slot));
        if value & PRESENT == 0 {
            continue;
        }
        let mut address = prefix | ((slot as u64) << shift);
        if level == 3 && slot >= 256 {
            address |= 0xffff_0000_0000_0000;
        }
        if level != 0 && (level == 3 || value & HUGE == 0) {
            if !aliases(
                value & MASK,
                level - 1,
                address,
                first,
                end,
                hhdm,
                change_canonical,
            ) {
                return false;
            }
            continue;
        }
        let bytes = 1u64 << shift;
        let physical = value & MASK & !(bytes - 1);
        if physical >= end || physical + bytes <= first {
            continue;
        }
        // This is the single canonical mapping being changed. Other aliases
        // must already have the requested effective type, not merely a
        // superficially similar PAT index or an untracked WB alias.
        if change_canonical && hhdm.checked_add(physical) == Some(address) {
            continue;
        }
        let mut current = physical.max(first);
        while current < end.min(physical + bytes) {
            if pat::effective_at(types::decode(value, level != 0), current) != Some(types::WC) {
                return false;
            }
            current += PAGE;
        }
    }
    true
}
pub(super) fn write_combining(address: u64, bytes: u64) -> bool {
    let Some(table) = pat::local_table() else {
        return false;
    };
    let Some(wc) = types::selector(table, types::WC) else {
        return false;
    };
    let Some(end) = address
        .checked_add(bytes)
        .and_then(|n| n.checked_add(PAGE - 1))
        .map(|n| n & !(PAGE - 1))
    else {
        return false;
    };
    if bytes == 0 {
        return false;
    }
    let start = address & !(PAGE - 1);
    let _owner = Guard::acquire(6);
    if !active_matches() {
        return false;
    }
    let boot = unsafe { &*crate::boot::info::r4os_rust_boot_info() };
    // An already confirmed WC window needs no retyping. This includes the
    // private framebuffer alias established coherently by the boot root.
    if let Some(first) = leaf(root(), start) {
        let first_value = read(first.entry);
        let physical = (first_value & MASK & !(first.bytes - 1)) | (start & (first.bytes - 1));
        if first_value & PRESENT != 0 && device_range(physical, end - start) {
            let mut matching = true;
            for current in (start..end).step_by(PAGE as usize) {
                let Some(entry) = leaf(root(), current) else {
                    matching = false;
                    break;
                };
                let value = read(entry.entry);
                let actual = (value & MASK & !(entry.bytes - 1)) | (current & (entry.bytes - 1));
                if value & PRESENT == 0
                    || actual != physical + current - start
                    || pat::effective_at(types::decode(value, entry.huge), actual)
                        != Some(types::WC)
                {
                    matching = false;
                    break;
                }
            }
            if matching
                && aliases(
                    root(),
                    3,
                    0,
                    physical,
                    physical + end - start,
                    boot.hhdm,
                    false,
                )
            {
                return true;
            }
        }
    }
    if boot.flags & 4 == 0 || start < boot.hhdm {
        return false;
    }
    let physical = start - boot.hhdm;
    let Some(physical_end) = physical.checked_add(end - start) else {
        return false;
    };
    if !device_range(physical, end - start)
        || !aliases(root(), 3, 0, physical, physical_end, boot.hhdm, true)
    {
        return false;
    }
    // Preflight the entire requested range before any mutation. A partial
    // huge-leaf change may not retag neighboring resources without ownership.
    for current in (start..end).step_by(PAGE as usize) {
        let Some(entry) = leaf(root(), current) else {
            return false;
        };
        let value = read(entry.entry);
        if value & PRESENT == 0 {
            return false;
        }
        let actual = (value & MASK & !(entry.bytes - 1)) | (current & (entry.bytes - 1));
        if actual != current - boot.hhdm {
            return false;
        }
        let Some(kind) = pat::effective_at(types::decode(value, entry.huge), actual) else {
            return false;
        };
        if types::cacheable(kind) {
            return false;
        }
        if entry.huge
            && kind != types::WC
            && (current & !(entry.bytes - 1) < start
                || (current & !(entry.bytes - 1))
                    .checked_add(entry.bytes)
                    .is_none_or(|n| n > end))
        {
            return false;
        }
    }
    let mut base = start;
    while base < end {
        let pages = ((end - base) / PAGE).min(page_batch::MAX_EXTENT_PAGES);
        let mut leaves = batch::Leaves::new(root(), base);
        let mut previous = [MaybeUninit::<u64>::uninit(); 64];
        for page in 0..pages {
            let Some(entry) = leaves.prepare(page, false) else {
                return false;
            };
            previous[page as usize].write(read(entry.entry));
        }
        for page in 0..pages {
            let entry = leaves.leaf(page);
            let mask = WRITE_THROUGH | CACHE_DISABLE | if entry.huge { HUGE_PAT } else { PAT };
            let value = (read(entry.entry) & !mask)
                | types::encode(wc, entry.huge).expect("validated PAT selector");
            write(entry.entry, value);
        }
        if !invalidate_types(base, pages) {
            shootdown_failure();
            for page in 0..pages {
                write(leaves.entry(page), unsafe {
                    previous[page as usize].assume_init_read()
                });
            }
            restore_count(pages);
            if !invalidate_types(base, pages) {
                state(|s| s.stats.rollback_restore_failures += 1);
            }
            return false;
        }
        state(|s| {
            s.stats.write_combining_pages = s.stats.write_combining_pages.wrapping_add(pages)
        });
        base += pages * PAGE;
    }
    true
}
