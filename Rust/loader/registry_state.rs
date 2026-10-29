//! Canonical module entries, catalogs and generation/state claims. Existing
//! typed construction peers temporarily write only their exact private claim.
//! Publication and terminal resource/unwind release are owned here.
use super::{
    file::Source,
    r4m::VersionLabels,
    registry_queries::{Export, Section},
    resources::Catalog,
};
use crate::driver::{
    owner_storage,
    owner_table::{Empty, Table},
};
use crate::sched::task_context::{self, Unwind};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
const FREE: u32 = 0;
const RESERVED: u32 = 1;
const PUBLISHED: u32 = 2;
const RETIRING: u32 = 3;
const RETAINED: u32 = 4;
#[repr(C)]
pub struct OptionalSource {
    pub present: bool,
    pub value: MaybeUninit<Source>,
}
#[repr(C)]
pub struct Entry {
    pub version_labels: VersionLabels,
    pub used: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub path: [u8; 96],
    pub path_len: usize,
    pub kind: u16,
    pub state: u8,
    pub flags: u32,
    pub base: u64,
    pub size: usize,
    pub section_count: u32,
    pub entry_count: u32,
    pub import_count: u32,
    pub export_count: u32,
    pub reloc_count: u32,
    pub reloc_abs64_count: u32,
    pub reloc_rel32_count: u32,
    pub reloc_base_rel64_count: u32,
    pub reloc_import_slot64_count: u32,
    pub resolved_import_count: u32,
    pub resolver_status: u8,
    pub bss_zeroed: bool,
    pub generation: u32,
    pub pinned: bool,
    pub resource_source: OptionalSource,
    pub resource_file_offset: usize,
    pub sections: [Section; 16],
    pub exports: [Export; 16],
}
impl Entry {
    pub(crate) const EMPTY: Self = {
        // Every initialized field is a scalar C integer/bool/array; the
        // absent Source payload is explicitly MaybeUninit, never a Rust ref.
        let mut value: Self = unsafe { core::mem::zeroed() };
        value.kind = 1;
        value.bss_zeroed = true;
        value
    };
}
struct ModuleRow {
    entry: Entry,
    catalog: Catalog,
    claim: AtomicU64,
}
impl Empty for ModuleRow {
    const EMPTY: Self = Self {
        entry: Entry::EMPTY,
        catalog: unsafe { core::mem::zeroed() },
        claim: AtomicU64::new(0),
    };
}
struct Rows(UnsafeCell<Table<ModuleRow>>);
unsafe impl Sync for Rows {}
static ROWS: Rows = Rows(UnsafeCell::new(Table::new()));
pub(crate) fn capacity() -> usize {
    unsafe { (*ROWS.0.get()).capacity() }
}
fn row(index: usize) -> *mut ModuleRow {
    unsafe { Table::get(ROWS.0.get(), index) }
}
fn slot_claim(index: usize) -> &'static AtomicU64 {
    let p = row(index);
    assert!(!p.is_null());
    unsafe { &(*p).claim }
}
#[repr(C)]
pub struct Prepared {
    pub slot: usize,
    pub generation: u32,
    pub scratch: *mut c_void,
    pub unwind: Unwind,
}
#[repr(C)]
pub struct Peers {
    pub scratch_size: usize,
    pub scratch_align: usize,
    pub alloc: unsafe extern "C" fn(usize, usize) -> *mut u8,
    pub free: unsafe extern "C" fn(*mut u8, usize) -> u8,
    pub publication: unsafe extern "C" fn(*const Entry),
    pub log: unsafe extern "C" fn(*const u8, usize),
}
pub(crate) fn entry(index: usize) -> *mut Entry {
    let p = row(index);
    if p.is_null() {
        ptr::null_mut()
    } else {
        unsafe { ptr::addr_of_mut!((*p).entry) }
    }
}
pub const fn claim(generation: u32, state: u32) -> u64 {
    (generation as u64) << 32 | state as u64
}
pub(crate) fn catalog(index: usize) -> *const Catalog {
    let p = row(index);
    if p.is_null() {
        ptr::null()
    } else {
        unsafe { ptr::addr_of!((*p).catalog) }
    }
}
unsafe fn log(peers: &Peers, text: &[u8]) {
    unsafe { (peers.log)(text.as_ptr(), text.len()) };
}
/// # Safety
/// Original linear BSP bootstrap or an isolated owner fixture; no live load,
/// published consumer or retained image may exist while resetting the owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_reset() {
    for i in 0..capacity() {
        let slot_claim = slot_claim(i);
        unsafe { ptr::write(entry(i), Entry::EMPTY) };
        slot_claim.store(0, Ordering::Release);
    }
    // Original init does not reset catalogs; the next exact R4D load owns its
    // count reset and parse. Reset entries revoke every previous source view.
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_mod_published(index: usize) -> bool {
    index < capacity() && slot_claim(index).load(Ordering::Acquire) as u32 == PUBLISHED
}
/// # Safety
/// Load is a live caller-owned handle. The packed claim validates ownership;
/// no scratch or Entry pointer is borrowed or dereferenced before this check.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_owns(load: *const Prepared) -> bool {
    let slot = unsafe { (*load).slot };
    let generation = unsafe { (*load).generation };
    slot < capacity() && slot_claim(slot).load(Ordering::Acquire) == claim(generation, RESERVED)
}
unsafe fn reserve(out: *mut Prepared, peers: &Peers) -> bool {
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        return false;
    }
    loop {
        for i in 0..capacity() {
            let slot_claim = slot_claim(i);
            let before = slot_claim.load(Ordering::Acquire);
            if before as u32 != FREE {
                continue;
            }
            let mut generation = (before >> 32) as u32;
            generation = generation.wrapping_add(1);
            if generation == 0 {
                generation = 1;
            }
            if slot_claim
                .compare_exchange(
                    before,
                    claim(generation, RESERVED),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                continue;
            }
            unsafe {
                ptr::write(
                    entry(i),
                    Entry {
                        generation,
                        ..Entry::EMPTY
                    },
                )
            };
            unsafe {
                ptr::write(
                    out,
                    Prepared {
                        slot: i,
                        generation,
                        scratch: ptr::null_mut(),
                        unwind,
                    },
                )
            };
            return true;
        }
        if !owner_storage::runtime(ROWS.0.get(), capacity()) {
            break;
        }
    }
    unsafe { task_context::leave(unwind) };
    unsafe { log(peers, b"[MOD] registry metadata memory exhausted\r\n") };
    false
}
unsafe fn release_scratch(load: *mut Prepared, peers: &Peers) {
    let scratch = unsafe { (*load).scratch };
    if !scratch.is_null() {
        let _ = unsafe { (peers.free)(scratch.cast(), peers.scratch_size) };
        unsafe { ptr::addr_of_mut!((*load).scratch).write(ptr::null_mut()) };
    }
}
/// # Safety
/// Out is aligned writable handle storage; caller retains current-task unwind
/// admission. Immutable peers obey the existing heap/log contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_reserve(out: *mut Prepared, peers: &Peers) -> bool {
    unsafe { reserve(out, peers) }
}
/// # Safety
/// Same admission/storage as reserve; scratch layout is the exact existing
/// private LoadScratch. A failed allocation aborts its exact claim/unwind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_begin(out: *mut Prepared, peers: &Peers) -> bool {
    if !unsafe { reserve(out, peers) } {
        return false;
    }
    let scratch = unsafe { (peers.alloc)(peers.scratch_size, peers.scratch_align) };
    if scratch.is_null() {
        unsafe { r4os_rust_mod_abort(out, peers) };
        return false;
    }
    unsafe { ptr::addr_of_mut!((*out).scratch).write(scratch.cast()) };
    true
}
/// # Safety
/// The exact reserved image has completed validation and consumer admission.
/// Load/peers stay resident through logging, heap and unwind-release callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_commit(load: *mut Prepared, peers: &Peers) -> usize {
    let slot = unsafe { (*load).slot };
    let generation = unsafe { (*load).generation };
    if slot_claim(slot)
        .compare_exchange(
            claim(generation, RESERVED),
            claim(generation, RETIRING),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return usize::MAX;
    }
    let target = entry(slot);
    unsafe { ptr::addr_of_mut!((*target).used).write(true) };
    slot_claim(slot).store(claim(generation, PUBLISHED), Ordering::Release);
    if unsafe { (*target).kind } != 5 {
        unsafe { (peers.publication)(target) };
    }
    unsafe { release_scratch(load, peers) };
    let unwind = unsafe { (*load).unwind };
    let _ = unsafe { task_context::leave(unwind) };
    slot
}
/// # Safety
/// Caller retains its exact private image and unwind. Failed physical release
/// keeps image metadata and claim retained; no reusable slot is published.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mod_abort(load: *mut Prepared, peers: &Peers) {
    let slot = unsafe { (*load).slot };
    let generation = unsafe { (*load).generation };
    if slot_claim(slot)
        .compare_exchange(
            claim(generation, RESERVED),
            claim(generation, RETIRING),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    let target = entry(slot);
    let base = unsafe { (*target).base };
    let released = base == 0 || unsafe { (peers.free)(base as *mut u8, (*target).size) } == 0;
    if released {
        unsafe { ptr::write(target, Entry::EMPTY) };
    } else {
        unsafe { ptr::addr_of_mut!((*target).state).write(3) };
        unsafe {
            log(
                peers,
                b"[MOD] rejected image release failed; private slot retained\r\n",
            )
        };
    }
    unsafe { release_scratch(load, peers) };
    slot_claim(slot).store(
        claim(generation, if released { FREE } else { RETAINED }),
        Ordering::Release,
    );
    let unwind = unsafe { (*load).unwind };
    let _ = unsafe { task_context::leave(unwind) };
}
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<AtomicU64>() == 8 && align_of::<AtomicU64>() == 8);
    assert!(size_of::<OptionalSource>() == 944 && offset_of!(OptionalSource, value) == 8);
    assert!(size_of::<Entry>() == 3296);
    assert!(offset_of!(Entry, version_labels) == 0);
    assert!(offset_of!(Entry, used) == 64);
    assert!(offset_of!(Entry, name) == 65);
    assert!(offset_of!(Entry, name_len) == 104);
    assert!(offset_of!(Entry, path) == 112);
    assert!(offset_of!(Entry, path_len) == 208);
    assert!(offset_of!(Entry, kind) == 216);
    assert!(offset_of!(Entry, state) == 218);
    assert!(offset_of!(Entry, flags) == 220);
    assert!(offset_of!(Entry, base) == 224);
    assert!(offset_of!(Entry, size) == 232);
    assert!(offset_of!(Entry, section_count) == 240);
    assert!(offset_of!(Entry, entry_count) == 244);
    assert!(offset_of!(Entry, import_count) == 248);
    assert!(offset_of!(Entry, export_count) == 252);
    assert!(offset_of!(Entry, reloc_count) == 256);
    assert!(offset_of!(Entry, reloc_abs64_count) == 260);
    assert!(offset_of!(Entry, reloc_rel32_count) == 264);
    assert!(offset_of!(Entry, reloc_base_rel64_count) == 268);
    assert!(offset_of!(Entry, reloc_import_slot64_count) == 272);
    assert!(offset_of!(Entry, resolved_import_count) == 276);
    assert!(offset_of!(Entry, resolver_status) == 280);
    assert!(offset_of!(Entry, bss_zeroed) == 281);
    assert!(offset_of!(Entry, generation) == 284);
    assert!(offset_of!(Entry, pinned) == 288);
    assert!(offset_of!(Entry, resource_source) == 296);
    assert!(offset_of!(Entry, resource_file_offset) == 1240);
    assert!(offset_of!(Entry, sections) == 1248);
    assert!(offset_of!(Entry, exports) == 2144);
    assert!(size_of::<Prepared>() == 40);
    assert!(offset_of!(Prepared, slot) == 0);
    assert!(offset_of!(Prepared, generation) == 8);
    assert!(offset_of!(Prepared, scratch) == 16);
    assert!(offset_of!(Prepared, unwind) == 24);
    assert!(size_of::<Peers>() == 48);
    assert!(offset_of!(Peers, scratch_size) == 0);
    assert!(offset_of!(Peers, scratch_align) == 8);
    assert!(offset_of!(Peers, alloc) == 16);
    assert!(offset_of!(Peers, free) == 24);
    assert!(offset_of!(Peers, publication) == 32);
    assert!(offset_of!(Peers, log) == 40);
};
