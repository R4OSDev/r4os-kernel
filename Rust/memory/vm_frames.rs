//! Bounded physical preparation and acknowledged VM page release.
//! The original natural extent, discrepancy retention and reclaim admission
//! remain unchanged. No range reference spans a callback, mapping or TLB wait.
use super::{
    blocks,
    layout::PAGE_SIZE as PAGE,
    owner::{self, Guard},
    page_batch::MAX_EXTENT_PAGES,
    paging, phys,
    vm_ranges::Range,
};
use core::mem::MaybeUninit;
const OUT_OF_MEMORY: u32 = 12;
unsafe extern "C" {
    fn r4os_zig_vm_reclaim(reason: u32, frames: u32) -> u32;
}
pub(super) fn block_error(code: u32) -> u32 {
    match code {
        1..=3 => code,
        4 | 6 => 7,
        5 => 8,
        7 | 9 => 5,
        8 => 15,
        _ => unreachable!(),
    }
}
fn claim(range: Range, extent: phys::Extent) -> Result<(), u32> {
    let request = blocks::Block {
        slot_used: false,
        id: 0,
        kind: range.kind,
        owner: range.owner,
        status: 1,
        owner_id: range.owner_id,
        name_ptr: range.name_ptr,
        name_len: range.name_len,
        phys_base: extent.base,
        phys_len: extent.count * PAGE,
        virt_base: 0,
        virt_len: 0,
        reserved_bytes: 0,
        committed_bytes: 0,
    };
    let mut id = 0;
    let code = unsafe { blocks::r4os_rust_blocks_range(&request, false, &mut id) };
    if code == 0 { Ok(()) } else { Err(code) }
}
pub(super) fn can_commit(range: Range, pages: u64) -> bool {
    if range.owner != 3 && range.kind != 7 && range.kind != 5 {
        return true;
    }
    let mut stats = MaybeUninit::<phys::Stats>::uninit();
    unsafe {
        phys::r4os_rust_phys_stats(stats.as_mut_ptr());
    }
    unsafe { stats.assume_init() }.free_frames > pages + 384
}
fn unclaimed(extent: phys::Extent) {
    let mut cursor = extent.base;
    let end = extent.base + extent.count * PAGE;
    while cursor < end {
        let free_bytes = blocks::r4os_rust_blocks_prefix(cursor, end - cursor, true);
        if free_bytes >= PAGE {
            let pages = free_bytes / PAGE;
            phys::r4os_rust_phys_free_extent(cursor, pages);
            cursor += pages * PAGE;
        } else {
            cursor += PAGE;
        }
    }
}
fn extent(range: Range, requested: u64, reason: u32) -> Result<phys::Extent, u32> {
    let wanted = requested.min(MAX_EXTENT_PAGES);
    if wanted == 0 {
        return Err(OUT_OF_MEMORY);
    }
    let mut attempts = 0u64;
    let mut reclaimed = false;
    while attempts < MAX_EXTENT_PAGES {
        let mut acquired = MaybeUninit::<phys::Extent>::uninit();
        if !unsafe { phys::r4os_rust_phys_extent(wanted, acquired.as_mut_ptr()) } {
            if !reclaimed {
                reclaimed = true;
                if owner::named(5).held() {
                    return Err(OUT_OF_MEMORY);
                }
                if unsafe { r4os_zig_vm_reclaim(reason, wanted as u32) } != 0 {
                    continue;
                }
            }
            return Err(OUT_OF_MEMORY);
        }
        let acquired = unsafe { acquired.assume_init() };
        attempts = attempts.saturating_add(acquired.count);
        let free_bytes =
            blocks::r4os_rust_blocks_prefix(acquired.base, acquired.count * PAGE, true);
        let claim_pages = free_bytes / PAGE;
        if claim_pages == 0 {
            unclaimed(acquired);
            continue;
        }
        let claimed = phys::Extent {
            base: acquired.base,
            count: claim_pages,
        };
        if claim_pages < acquired.count {
            unclaimed(phys::Extent {
                base: acquired.base + claim_pages * PAGE,
                count: acquired.count - claim_pages,
            });
        }
        if let Err(code) = claim(range, claimed) {
            unclaimed(claimed);
            if code == 6 {
                continue;
            }
            return Err(block_error(code));
        }
        return Ok(claimed);
    }
    Err(OUT_OF_MEMORY)
}
fn frame(range: Range, reason: u32) -> Result<phys::Extent, u32> {
    let mut stats = MaybeUninit::<phys::Stats>::uninit();
    unsafe {
        phys::r4os_rust_phys_stats(stats.as_mut_ptr());
    }
    let maximum = unsafe { stats.assume_init() }.total_frames;
    let mut attempts = 0;
    let mut reclaimed = false;
    while attempts < maximum {
        let mut frame = 0;
        if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
            if !reclaimed {
                reclaimed = true;
                if owner::named(5).held() {
                    return Err(OUT_OF_MEMORY);
                }
                if unsafe { r4os_zig_vm_reclaim(reason, 1) } != 0 {
                    attempts = 1;
                    continue;
                }
            }
            return Err(OUT_OF_MEMORY);
        }
        let claimed = phys::Extent {
            base: frame,
            count: 1,
        };
        if let Err(code) = claim(range, claimed) {
            if code == 6 {
                attempts += 1;
                continue;
            }
            phys::r4os_rust_phys_free(frame);
            return Err(block_error(code));
        }
        return Ok(claimed);
    }
    Err(OUT_OF_MEMORY)
}
struct Release {
    plan: blocks::ReleasePlan,
}
impl Release {
    fn prepare(extent: phys::Extent) -> Result<Self, u32> {
        let mut plan = MaybeUninit::<blocks::ReleasePlan>::uninit();
        let code = unsafe {
            blocks::r4os_rust_blocks_prepare(extent.base, extent.count * PAGE, plan.as_mut_ptr())
        };
        if code != 0 {
            return Err(block_error(code));
        }
        Ok(Self {
            plan: unsafe { plan.assume_init() },
        })
    }
    fn commit(&mut self) {
        unsafe {
            blocks::r4os_rust_blocks_finish(&mut self.plan, false, false);
        }
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        unsafe {
            blocks::r4os_rust_blocks_finish(&mut self.plan, false, true);
        }
    }
}
pub(super) fn release_prepared(extent: phys::Extent, single: bool) -> Result<(), u32> {
    if extent.count == 0 {
        return Ok(());
    }
    let mut release = Release::prepare(extent)?;
    if single {
        phys::r4os_rust_phys_free(extent.base);
    } else {
        phys::r4os_rust_phys_free_extent(extent.base, extent.count);
    }
    release.commit();
    Ok(())
}
pub(super) fn mapped(address: u64, locked: bool) -> Option<u64> {
    let mut value = 0;
    if unsafe { paging::r4os_rust_paging_read(address, 0, locked, &mut value) } {
        Some(value)
    } else {
        None
    }
}
/// # Safety
/// range points to its exact admitted resident C lease, retaining VM owner.
/// The extent is unpublished; mapped extents use acknowledged uncommit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_allocate(
    range: *const Range,
    reason: u32,
    requested: u64,
    single: bool,
    output: *mut phys::Extent,
) -> u32 {
    let range = unsafe { range.read() };
    let result = if single {
        frame(range, reason)
    } else {
        extent(range, requested, reason)
    };
    match result {
        Ok(extent) => {
            unsafe {
                output.write(extent);
            }
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// range is a live immutable C snapshot; its enum values are valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_can_commit(range: *const Range, pages: u64) -> bool {
    can_commit(unsafe { range.read() }, pages)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_release_prepared(base: u64, count: u64, single: bool) {
    let _ = release_prepared(phys::Extent { base, count }, single);
}
/// # Safety
/// Caller retains VM owner and the exact range lease. base/count identify
/// mapped contiguous frames (count == 1 for a single page). References end
/// before TLB work; free/metadata publication follows exact acknowledgements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_uncommit_extent(
    range: *mut Range,
    address: u64,
    base: u64,
    count: u64,
    logical: bool,
    single: bool,
) -> u32 {
    if count == 0 {
        return 10;
    }
    let _page_owner = Guard::acquire(6);
    let extent = if single {
        let Some(base) = mapped(address, true) else {
            return 10;
        };
        phys::Extent { base, count: 1 }
    } else {
        phys::Extent { base, count }
    };
    let mut release = match Release::prepare(extent) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let unmapped = if single {
        paging::r4os_rust_paging_unmap(address, true)
    } else {
        paging::r4os_rust_paging_unmap_batch(address, extent.base, extent.count, true)
    };
    if !unmapped {
        return 13;
    }
    if single {
        phys::r4os_rust_phys_free(extent.base);
    } else {
        phys::r4os_rust_phys_free_extent(extent.base, extent.count);
    }
    let bytes = extent.count * PAGE;
    {
        let range = unsafe { &mut *range };
        range.resident_bytes = range.resident_bytes.saturating_sub(bytes);
        if logical {
            range.committed_bytes = range.committed_bytes.saturating_sub(bytes);
        }
    }
    release.commit();
    0
}
