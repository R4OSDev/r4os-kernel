//! Runtime paging under the existing ranked page-table owner. Hardware PTEs
//! are raw volatile words; no Rust reference or mutable State borrow survives
//! an allocation, TLB callback, or owner handoff.
use super::{
    blocks,
    owner::{self, Guard},
    page_batch::{self, Translation},
    phys,
};
use core::{cell::UnsafeCell, mem::MaybeUninit};
mod batch;
mod cache;
const PAGE: u64 = 4096;
const MASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const WRITE_THROUGH: u64 = 1 << 3;
const CACHE_DISABLE: u64 = 1 << 4;
const ACCESSED: u64 = 1 << 5;
const DIRTY: u64 = 1 << 6;
const CR0_WP: u64 = 1 << 16;
const PAT: u64 = 1 << 7;
const HUGE: u64 = 1 << 7;
const HUGE_PAT: u64 = 1 << 12;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub initialized: bool,
    pub root_owner: u8,
    pub active_root_phys: u64,
    pub hardware_cr3: u64,
    pub active_root_confirmed: bool,
    pub r4os_root_active: bool,
    pub init_runs: u64,
    pub adopt_runs: u64,
    pub map_pages: u64,
    pub unmap_pages: u64,
    pub write_combining_pages: u64,
    pub invlpg_flushes: u64,
    pub map_failures: u64,
    pub unmap_failures: u64,
    pub map_batches: u64,
    pub map_batch_pages: u64,
    pub map_batch_max_pages: u64,
    pub map_batch_rollbacks: u64,
    pub unmap_batches: u64,
    pub unmap_batch_pages: u64,
    pub unmap_batch_max_pages: u64,
    pub shootdown_failures: u64,
    pub rollback_restores: u64,
    pub rollback_restore_failures: u64,
    pub root_mismatches: u64,
}
impl Stats {
    const EMPTY: Self = Self {
        initialized: false,
        root_owner: 0,
        active_root_phys: 0,
        hardware_cr3: 0,
        active_root_confirmed: false,
        r4os_root_active: false,
        init_runs: 0,
        adopt_runs: 0,
        map_pages: 0,
        unmap_pages: 0,
        write_combining_pages: 0,
        invlpg_flushes: 0,
        map_failures: 0,
        unmap_failures: 0,
        map_batches: 0,
        map_batch_pages: 0,
        map_batch_max_pages: 0,
        map_batch_rollbacks: 0,
        unmap_batches: 0,
        unmap_batch_pages: 0,
        unmap_batch_max_pages: 0,
        shootdown_failures: 0,
        rollback_restores: 0,
        rollback_restore_failures: 0,
        root_mismatches: 0,
    };
}
struct State {
    root: u64,
    initialized: bool,
    owner: u8,
    stats: Stats,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every State access is bounded by the exact page-table owner.
// Nested operations only copy root values or mutate counters in short scopes.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    root: 0,
    initialized: false,
    owner: 0,
    stats: Stats::EMPTY,
}));
unsafe extern "C" {
    fn r4os_read_cr0() -> u64;
    fn r4os_write_cr0(value: u64);
    fn r4os_read_cr3() -> u64;
    fn r4os_invlpg(address: usize);

}
fn state<T>(operation: impl FnOnce(&mut State) -> T) -> T {
    // Callers retain owner 6. No operation closure invokes an external owner.
    operation(unsafe { &mut *STATE.0.get() })
}
fn root() -> u64 {
    state(|state| state.root)
}
fn active_matches() -> bool {
    let (initialized, root) = state(|state| (state.initialized, state.root));
    if !initialized || root == 0 {
        return false;
    }
    let ok = unsafe { r4os_read_cr3() } & MASK == root;
    if !ok {
        state(|state| state.stats.root_mismatches += 1);
    }
    ok
}
fn aligned(address: u64) -> bool {
    address & (PAGE - 1) == 0
}
fn batch_valid(virtual_base: u64, physical_base: u64, pages: u64) -> bool {
    if pages == 0
        || pages > page_batch::MAX_EXTENT_PAGES
        || !aligned(virtual_base)
        || !aligned(physical_base)
    {
        return false;
    }
    let last = (pages - 1) * PAGE;
    virtual_base.checked_add(last).is_some() && physical_base.checked_add(last).is_some()
}
fn table(physical: u64) -> *mut u64 {
    phys::r4os_rust_phys_to_virt(physical) as *mut u64
}
fn index(address: u64, shift: u32) -> usize {
    ((address >> shift) & 511) as usize
}
fn read(entry: *mut u64) -> u64 {
    unsafe { entry.read_volatile() }
}
fn write(entry: *mut u64, value: u64) {
    unsafe {
        entry.write_volatile(value);
    }
}
fn at(table: *mut u64, slot: usize) -> *mut u64 {
    unsafe { table.add(slot) }
}
fn next(table: *mut u64, slot: usize) -> Option<*mut u64> {
    let entry = read(at(table, slot));
    if entry & PRESENT == 0 {
        None
    } else {
        Some(self::table(entry & MASK))
    }
}
/// Only require the AP's optional 1-GB format when the shared live root
/// actually contains such leaves. No page-table borrow crosses AP startup.
pub(crate) fn ap_boot_uses_1g(expected_root: u64) -> Option<bool> {
    let _owner = Guard::acquire(6);
    if !active_matches() || root() != expected_root {
        return None;
    }
    for pml4_index in 0..512 {
        let entry = read(at(table(expected_root), pml4_index));
        if entry & PRESENT == 0 {
            continue;
        }
        let pdpt = table(entry & MASK);
        for pdpt_index in 0..512 {
            if read(at(pdpt, pdpt_index)) & (PRESENT | HUGE) == PRESENT | HUGE {
                return Some(true);
            }
        }
    }
    Some(false)
}
fn next_or_create(table: *mut u64, slot: usize) -> Option<*mut u64> {
    let entry = at(table, slot);
    if read(entry) & PRESENT == 0 {
        let mut frame = 0;
        if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
            return None;
        }
        let request = blocks::Block::physical(frame, PAGE, 3, 0, 0, 2, b"page-table");
        // The block name and enum values are resident kernel constants.
        if unsafe { blocks::claim(request) }.is_err() {
            phys::r4os_rust_phys_free(frame);
            return None;
        }
        let child = self::table(frame);
        // The new page is private until the parent PTE is published.
        unsafe {
            core::ptr::write_bytes(child, 0, 512);
        }
        write(entry, (frame & MASK) | PRESENT | WRITABLE);
    }
    Some(self::table(read(entry) & MASK))
}
fn get_or_create(root: u64, address: u64) -> Option<*mut u64> {
    let pdpt = next_or_create(table(root), index(address, 39))?;
    let pd = next_or_create(pdpt, index(address, 30))?;
    next_or_create(pd, index(address, 21))
}
fn get_table(root: u64, address: u64) -> Option<*mut u64> {
    let pdpt = next(table(root), index(address, 39))?;
    let pd = next(pdpt, index(address, 30))?;
    next(pd, index(address, 21))
}
struct Leaf {
    entry: *mut u64,
    huge: bool,
    bytes: u64,
}
/// Hardware-register windows require confirmed strict UC semantics. An
/// inherited alias is borrowed only when its real physical/cache type agrees.
pub(crate) fn ensure_mmio(address: u64, physical: u64) -> bool {
    ensure_mmio_candidate(address, physical, false)
}
pub(crate) fn ensure_mmio_candidate(address: u64, physical: u64, for_wc: bool) -> bool {
    use crate::arch::{memory_types as types, pat};
    let Some(bits) = pat::cache_bits(types::UC, false) else {
        return false;
    };
    if !aligned(address) || !aligned(physical) {
        return false;
    }
    {
        let _owner = Guard::acquire(6);
        if !active_matches() {
            return false;
        }
        if let Some(leaf) = leaf(root(), address) {
            let value = read(leaf.entry);
            if value & PRESENT != 0 {
                let actual = (value & MASK & !(leaf.bytes - 1)) | (address & (leaf.bytes - 1));
                let kind = pat::effective_at(types::decode(value, leaf.huge), physical);
                return actual == physical
                    && (kind == Some(types::UC) || (for_wc && kind == Some(types::WC)));
            }
        }
    }
    r4os_rust_paging_map(address, physical, WRITABLE | (1 << 63) | bits, false)
}
fn leaf(root: u64, address: u64) -> Option<Leaf> {
    let pml4 = read(at(table(root), index(address, 39)));
    if pml4 & PRESENT == 0 {
        return None;
    }
    let pdpt = at(table(pml4 & MASK), index(address, 30));
    let pdpt_entry = read(pdpt);
    if pdpt_entry & PRESENT == 0 {
        return None;
    }
    if pdpt_entry & HUGE != 0 {
        return Some(Leaf {
            entry: pdpt,
            huge: true,
            bytes: 1 << 30,
        });
    }
    let pd = at(table(pdpt_entry & MASK), index(address, 21));
    let pd_entry = read(pd);
    if pd_entry & PRESENT == 0 {
        return None;
    }
    if pd_entry & HUGE != 0 {
        return Some(Leaf {
            entry: pd,
            huge: true,
            bytes: 1 << 21,
        });
    }
    let pt = at(table(pd_entry & MASK), index(address, 12));
    if read(pt) & PRESENT == 0 {
        None
    } else {
        Some(Leaf {
            entry: pt,
            huge: false,
            bytes: PAGE,
        })
    }
}
fn flush(address: u64) {
    unsafe {
        r4os_invlpg(address as usize);
    }
    state(|state| state.stats.invlpg_flushes += 1);
}
fn fail_map() -> bool {
    state(|state| state.stats.map_failures += 1);
    false
}
fn fail_unmap() -> bool {
    state(|state| state.stats.unmap_failures += 1);
    false
}
fn wrong_owner(mapping: bool) -> bool {
    // Invalid Locked calls still count their failure, but State is never
    // mutably borrowed without admission in the Rust implementation.
    let _guard = Guard::acquire(6);
    if mapping { fail_map() } else { fail_unmap() }
}
fn invalidate(base: u64, pages: u64) -> bool {
    unsafe { paging_shootdown(base, pages) }
}
fn restore_count(pages: u64) {
    state(|state| {
        state.stats.rollback_restores = state.stats.rollback_restores.wrapping_add(pages)
    });
}
fn shootdown_failure() {
    state(|state| state.stats.shootdown_failures = state.stats.shootdown_failures.wrapping_add(1));
}
fn restore_failure(base: u64, pages: u64) {
    if !invalidate(base, pages) {
        state(|state| {
            state.stats.rollback_restore_failures =
                state.stats.rollback_restore_failures.wrapping_add(1)
        });
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_init() -> bool {
    let _guard = Guard::acquire(6);
    // Programs and drivers execute at CPL0. A pager's read-only writeback
    // interval therefore requires WP here as well as in the AP trampoline.
    unsafe { r4os_write_cr0(r4os_read_cr0() | CR0_WP) };
    if unsafe { r4os_read_cr0() } & CR0_WP == 0 {
        return false;
    }
    let root = unsafe { r4os_read_cr3() } & MASK;
    state(|state| {
        state.stats.init_runs += 1;
        state.root = root;
        state.initialized = root != 0;
        state.owner = if state.initialized { 1 } else { 0 };
        state.initialized
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_root() -> u64 {
    let _guard = Guard::acquire(6);
    root()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_adopt(physical: u64, owner: u8) -> bool {
    let _guard = Guard::acquire(6);
    if physical == 0 || !aligned(physical) {
        return false;
    }
    state(|state| {
        state.root = physical & MASK;
        state.owner = owner;
        state.initialized = true;
        state.stats.adopt_runs += 1;
    });
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_map(
    address: u64,
    physical: u64,
    flags: u64,
    locked: bool,
) -> bool {
    let _guard = if locked {
        None
    } else {
        Some(Guard::acquire(6))
    };
    if !owner::named(6).held() {
        return wrong_owner(true);
    }
    if !active_matches() || !aligned(address) || !aligned(physical) {
        return fail_map();
    }
    let Some(table) = get_or_create(root(), address) else {
        return false;
    };
    let entry = at(table, index(address, 12));
    if read(entry) & PRESENT != 0 {
        return fail_map();
    }
    write(entry, (physical & MASK) | flags | PRESENT);
    state(|state| state.stats.map_pages += 1);
    flush(address);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_unmap(address: u64, locked: bool) -> bool {
    unmap(address, locked, core::ptr::null_mut())
}
/// Exact own invalidation result, captured under the same page-table owner.
/// The optional receipt never replaces the existing rollback/retention rules.
pub(crate) fn unmap_with_receipt(address: u64) -> (bool, crate::arch::tlb::Result) {
    let mut receipt = crate::arch::tlb::Result::default();
    let ok = unmap(address, false, core::ptr::addr_of_mut!(receipt));
    (ok, receipt)
}
#[inline(always)]
fn unmap(address: u64, locked: bool, receipt: *mut crate::arch::tlb::Result) -> bool {
    let _guard = if locked {
        None
    } else {
        Some(Guard::acquire(6))
    };
    if !owner::named(6).held() {
        return wrong_owner(false);
    }
    if !active_matches() || !aligned(address) {
        return fail_unmap();
    }
    let Some(table) = get_table(root(), address) else {
        return fail_unmap();
    };
    let entry = at(table, index(address, 12));
    let previous = read(entry);
    if previous & PRESENT == 0 {
        return fail_unmap();
    }
    write(entry, 0);
    let result = paging_shootdown_result(address, 1);
    if !receipt.is_null() {
        unsafe {
            receipt.write(result);
        }
    }
    if !result.ok {
        shootdown_failure();
        write(entry, previous);
        restore_count(1);
        restore_failure(address, 1);
        return fail_unmap();
    }
    state(|state| state.stats.unmap_pages += 1);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_map_batch(
    virtual_base: u64,
    physical_base: u64,
    pages: u64,
    flags: u64,
) -> bool {
    let _guard = Guard::acquire(6);
    if !active_matches() || !batch_valid(virtual_base, physical_base, pages) {
        return fail_map();
    }
    let root = root();
    let mut leaves = batch::Leaves::new(root, virtual_base);
    for page in 0..pages {
        let Some(leaf) = leaves.prepare(page, true) else {
            return fail_map();
        };
        if leaf.huge || read(leaf.entry) & PRESENT != 0 {
            return fail_map();
        }
    }
    for page in 0..pages {
        let address = virtual_base + page * PAGE;
        write(
            leaves.entry(page),
            ((physical_base + page * PAGE) & MASK) | flags | PRESENT,
        );
        state(|state| state.stats.map_pages = state.stats.map_pages.wrapping_add(1));
        flush(address);
    }
    state(|state| {
        state.stats.map_batches = state.stats.map_batches.wrapping_add(1);
        state.stats.map_batch_pages = state.stats.map_batch_pages.wrapping_add(pages);
        state.stats.map_batch_max_pages = state.stats.map_batch_max_pages.max(pages);
    });
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_unmap_batch(
    virtual_base: u64,
    physical_base: u64,
    pages: u64,
    locked: bool,
) -> bool {
    unmap_batch_release(
        virtual_base,
        physical_base,
        pages,
        locked,
        |page| physical_base + page * PAGE,
        || true,
    )
    .is_ok()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReleaseError {
    Mapping,
    Tlb,
    Physical,
}
fn batch_unmap_failure(reason: ReleaseError) -> Result<crate::arch::tlb::Result, ReleaseError> {
    fail_unmap();
    Err(reason)
}
fn unmap_batch_release(
    virtual_base: u64,
    first_physical: u64,
    pages: u64,
    locked: bool,
    expected: impl Fn(u64) -> u64,
    release: impl FnOnce() -> bool,
) -> Result<crate::arch::tlb::Result, ReleaseError> {
    let _guard = if locked {
        None
    } else {
        Some(Guard::acquire(6))
    };
    if !owner::named(6).held() {
        wrong_owner(false);
        return Err(ReleaseError::Mapping);
    }
    if !active_matches() || !batch_valid(virtual_base, first_physical, pages) {
        return batch_unmap_failure(ReleaseError::Mapping);
    }
    let root = root();
    let mut leaves = batch::Leaves::new(root, virtual_base);
    let mut previous = [MaybeUninit::<u64>::uninit(); 64];
    for page in 0..pages {
        let Some(leaf) = leaves.prepare(page, false) else {
            return batch_unmap_failure(ReleaseError::Mapping);
        };
        let value = read(leaf.entry);
        if leaf.huge || value & PRESENT == 0 || value & MASK != expected(page) {
            return batch_unmap_failure(ReleaseError::Mapping);
        }
        previous[page as usize].write(value);
    }
    for page in 0..pages {
        write(leaves.entry(page), 0);
    }
    let receipt = paging_shootdown_result(virtual_base, pages);
    let failure = if !receipt.ok {
        shootdown_failure();
        Some(ReleaseError::Tlb)
    } else if !release() {
        Some(ReleaseError::Physical)
    } else {
        None
    };
    if let Some(reason) = failure {
        for page in 0..pages {
            // Every saved slot was filled during the successful preflight.
            write(leaves.entry(page), unsafe {
                previous[page as usize].assume_init_read()
            });
        }
        restore_count(pages);
        restore_failure(virtual_base, pages);
        return batch_unmap_failure(reason);
    }
    state(|state| {
        state.stats.unmap_pages = state.stats.unmap_pages.wrapping_add(pages);
        state.stats.unmap_batches = state.stats.unmap_batches.wrapping_add(1);
        state.stats.unmap_batch_pages = state.stats.unmap_batch_pages.wrapping_add(pages);
        state.stats.unmap_batch_max_pages = state.stats.unmap_batch_max_pages.max(pages);
    });
    Ok(receipt)
}
/// Read a virtual prefix into caller-owned resident preparation storage.
/// The VM owner and continuously held page-table owner keep these identities
/// bound until release_frames rechecks them before any PTE mutation.
pub(super) fn release_frames_snapshot(
    base: u64,
    frames: &mut [u64],
) -> Result<usize, ReleaseError> {
    if !owner::named(6).held() || !active_matches() || !batch_valid(base, 0, frames.len() as u64) {
        return Err(ReleaseError::Mapping);
    }
    let mut leaves = batch::Leaves::new(root(), base);
    for (page, output) in frames.iter_mut().enumerate() {
        let Some(leaf) = leaves.prepare(page as u64, false) else {
            return Ok(page);
        };
        if leaf.huge {
            return Err(ReleaseError::Mapping);
        }
        let value = read(leaf.entry);
        if value & PRESENT == 0 {
            return Ok(page);
        }
        *output = value & MASK;
    }
    Ok(frames.len())
}
/// The exact page-table owner is already held. The callback retains the
/// prepared physical owner, performs no waiting/reentry and either frees
/// every prepared frame or leaves all of them intact on rejection.
pub(super) fn release_frames(
    base: u64,
    frames: &[u64],
    release: impl FnOnce() -> bool,
) -> Result<crate::arch::tlb::Result, ReleaseError> {
    unmap_batch_release(
        base,
        frames.first().copied().unwrap_or(0),
        frames.len() as u64,
        true,
        |page| frames[page as usize],
        release,
    )
}
/// # Safety
/// output is exclusive C storage. Locked reads are only admitted by owner 6.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_paging_read(
    address: u64,
    query: u8,
    locked: bool,
    output: *mut u64,
) -> bool {
    let _guard = if locked {
        None
    } else {
        Some(Guard::acquire(6))
    };
    if !owner::named(6).held() || !active_matches() {
        return false;
    }
    if (query == 0 || query == 4) && !aligned(address) {
        return false;
    }
    let Some(leaf) = leaf(root(), address) else {
        return false;
    };
    if (query == 0 || query == 4) && leaf.huge {
        return false;
    }
    let value = read(leaf.entry);
    let result = match query {
        0 => value & MASK,
        1 => {
            (if value & (if leaf.huge { HUGE_PAT } else { PAT }) != 0 {
                4
            } else {
                0
            }) | (if value & CACHE_DISABLE != 0 { 2 } else { 0 })
                | (if value & WRITE_THROUGH != 0 { 1 } else { 0 })
        }
        2 => (value & MASK & !(leaf.bytes - 1)) | (address & (leaf.bytes - 1)),
        3 => 1,
        4 => u64::from(value & DIRTY != 0),
        _ => return false,
    };
    unsafe {
        output.write(result);
    }
    true
}
/// # Safety
/// output is exclusive live C storage. The caller retains the virtual range;
/// only root values and translated bytes
/// survive each bounded owner handoff.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_paging_contiguous(
    address: u64,
    bytes: u64,
    output: *mut u64,
) -> bool {
    let Some(mut probe) = Translation::new(address, bytes) else {
        return false;
    };
    let mut initial_root = 0;
    while probe.consumed < probe.bytes {
        let _guard = Guard::acquire(6);
        if !active_matches() {
            return false;
        }
        let root = root();
        if initial_root == 0 {
            initial_root = root;
        } else if initial_root != root {
            return false;
        }
        for _ in 0..page_batch::MAX_EXTENT_PAGES {
            if probe.consumed == probe.bytes {
                break;
            }
            let current = probe.virtual_base + probe.consumed;
            let Some(leaf) = leaf(root, current) else {
                return false;
            };
            let offset = current & (leaf.bytes - 1);
            let physical = (read(leaf.entry) & MASK & !(leaf.bytes - 1)) | offset;
            if !probe.accept(physical, leaf.bytes - offset) {
                return false;
            }
        }
    }
    if let Some(physical) = probe.result() {
        unsafe {
            output.write(physical);
        }
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_dirty(address: u64, clear: bool) -> bool {
    let _guard = Guard::acquire(6);
    if !active_matches() || !aligned(address) {
        return false;
    }
    let Some(leaf) = leaf(root(), address) else {
        return false;
    };
    if leaf.huge {
        return false;
    }
    let previous = read(leaf.entry);
    if !clear {
        write(leaf.entry, previous | DIRTY | ACCESSED);
        flush(address);
        return true;
    }
    write(leaf.entry, previous & !DIRTY);
    if invalidate(address, 1) {
        return true;
    }
    shootdown_failure();
    write(leaf.entry, previous);
    restore_count(1);
    restore_failure(address, 1);
    false
}
/// Caller pins the exact ordinary VM mapping under the VM owner. Only its
/// writable bit changes; physical identity, cache type and other flags stay.
fn update_writable(entry: *mut u64, writable: bool) -> u64 {
    // Hardware may set Accessed/Dirty while the software PT owner is held.
    // A load/store replacement could erase a racing dirty receipt.
    let entry = unsafe { core::sync::atomic::AtomicU64::from_ptr(entry) };
    if writable {
        entry.fetch_or(WRITABLE, core::sync::atomic::Ordering::SeqCst)
    } else {
        entry.fetch_and(!WRITABLE, core::sync::atomic::Ordering::SeqCst)
    }
}
pub(crate) fn set_writable(address: u64, writable: bool) -> bool {
    let _guard = Guard::acquire(6);
    if !active_matches() || !aligned(address) {
        return false;
    }
    let Some(leaf) = leaf(root(), address) else {
        return false;
    };
    if leaf.huge {
        return false;
    }
    let previous = update_writable(leaf.entry, writable);
    if (previous & WRITABLE != 0) == writable {
        return true;
    }
    if invalidate(address, 1) {
        return true;
    }
    shootdown_failure();
    update_writable(leaf.entry, previous & WRITABLE != 0);
    restore_count(1);
    restore_failure(address, 1);
    false
}
/// A write fault can arrive after its writeback interval has already ended.
/// Refresh this CPU only after confirming the current mapping permits retry.
pub(crate) fn retry_writable(address: u64) -> bool {
    let _guard = Guard::acquire(6);
    if !active_matches() || !aligned(address) {
        return false;
    }
    let Some(leaf) = leaf(root(), address) else {
        return false;
    };
    if read(leaf.entry) & WRITABLE == 0 {
        return false;
    }
    flush(address);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_write_combining(address: u64, bytes: u64) -> bool {
    cache::write_combining(address, bytes)
}
/// Nonblocking admission for a permanent kernel MONITOR page. Observe its
/// actual leaf/PAT selector and physical MTRR type without changing mappings.
/// Never wait for a page-table owner while scheduler Runtime is held.
pub(crate) fn resident_writeback_page(address: u64) -> bool {
    use crate::arch::{memory_types as types, pat};
    if !aligned(address) {
        return false;
    }
    let Some(_guard) = Guard::try_acquire(6) else {
        return false;
    };
    if !active_matches() {
        return false;
    }
    let Some(entry) = leaf(root(), address) else {
        return false;
    };
    let value = read(entry.entry);
    if value & (PRESENT | WRITABLE) != PRESENT | WRITABLE {
        return false;
    }
    let physical = (value & MASK & !(entry.bytes - 1)) | (address & (entry.bytes - 1));
    pat::effective_at(types::decode(value, entry.huge), physical) == Some(types::WB)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_paging_active(r4os: bool) -> bool {
    let _guard = Guard::acquire(6);
    if r4os && state(|state| state.owner) != 2 {
        return false;
    }
    active_matches()
}
/// # Safety
/// output is an exclusive live Stats C record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_paging_stats(output: *mut Stats) {
    let _guard = Guard::acquire(6);
    let (mut stats, initialized, root, owner) =
        state(|state| (state.stats, state.initialized, state.root, state.owner));
    stats.initialized = initialized;
    stats.root_owner = owner;
    stats.active_root_phys = root;
    stats.hardware_cr3 = unsafe { r4os_read_cr3() } & MASK;
    stats.active_root_confirmed = initialized && root != 0 && stats.hardware_cr3 == root;
    stats.r4os_root_active = stats.active_root_confirmed && owner == 2;
    unsafe {
        output.write(stats);
    }
}
const _: () = assert!(core::mem::size_of::<Stats>() == 184);

unsafe fn paging_shootdown(base: u64, pages: u64) -> bool {
    paging_shootdown_result(base, pages).ok
}
fn paging_shootdown_result(base: u64, pages: u64) -> crate::arch::tlb::Result {
    let mut result = core::mem::MaybeUninit::uninit();
    unsafe {
        crate::arch::tlb::r4os_rust_tlb_invalidate(base, pages, result.as_mut_ptr());
        result.assume_init()
    }
}
