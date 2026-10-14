//! Resident VM range storage and reservation owner.
//! During the pager migration, Zig holds only raw C leases into this table.
//! A lease is admitted by the VM owner; Rust references end before returning
//! or calling any path that may wait/reenter a VM operation.
use super::{blocks, virt_policy, vm_index};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
pub const RANGES: usize = 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OptionalSlot {
    pub value: u16,
    pub present: bool,
}
impl OptionalSlot {
    pub const NONE: Self = Self {
        value: 0,
        present: false,
    };
    pub fn get(self) -> Option<u16> {
        self.present.then_some(self.value)
    }
    pub fn from(value: Option<u16>) -> Self {
        value.map_or(Self::NONE, |value| Self {
            value,
            present: true,
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Range {
    pub slot_used: bool,
    pub id: u32,
    pub eviction_allowed: bool,
    pub window: u8,
    pub kind: u8,
    pub owner: u8,
    pub owner_id: u64,
    pub status: u8,
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub flags: u64,
    pub base: u64,
    pub len: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub guard_base: u64,
    pub guard_len: u64,
    pub block_id: u32,
    pub pager_io_count: u32,
    pub resident_commit_active: bool,
    pub resident_commit_failed: bool,
    pub resident_commit_base: u64,
    pub resident_commit_bytes: u64,
    pub resident_prepared: super::phys::Extent,
    pub partial_uncommit_base: u64,
    pub partial_uncommit_len: u64,
    pub partial_uncommit_cursor: u64,
    pub partial_uncommit_accounted: bool,
    pub commit_span_head: OptionalSlot,
    pub page_state_span_head: OptionalSlot,
}
impl Range {
    pub const EMPTY: Self = Self {
        slot_used: false,
        id: 0,
        eviction_allowed: true,
        window: 0,
        kind: 4,
        owner: 0,
        owner_id: 0,
        status: 1,
        name_ptr: b"".as_ptr(),
        name_len: 0,
        flags: 2 | (1 << 63), // original writable/NX default
        base: 0,
        len: 0,
        committed_bytes: 0,
        resident_bytes: 0,
        peak_resident_bytes: 0,
        fault_count: 0,
        failed_faults: 0,
        guard_base: 0,
        guard_len: 0,
        block_id: 0,
        pager_io_count: 0,
        resident_commit_active: false,
        resident_commit_failed: false,
        resident_commit_base: 0,
        resident_commit_bytes: 0,
        resident_prepared: super::phys::Extent { base: 0, count: 0 },
        partial_uncommit_base: 0,
        partial_uncommit_len: 0,
        partial_uncommit_cursor: 0,
        partial_uncommit_accounted: false,
        commit_span_head: OptionalSlot::NONE,
        page_state_span_head: OptionalSlot::NONE,
    };
    pub fn active(self) -> bool {
        self.slot_used && self.status != 5
    }
}
struct Storage(UnsafeCell<MaybeUninit<[Range; RANGES]>>);
// SAFETY: singleton BSP initializes every entry before publication. VM owner
// admits every later access, including the bounded legacy pager's C leases.
unsafe impl Sync for Storage {}
static TABLE: Storage = Storage(UnsafeCell::new(MaybeUninit::uninit()));
pub(super) fn slot(index: usize) -> *mut Range {
    assert!(index < RANGES);
    unsafe { TABLE.0.get().cast::<Range>().add(index) }
}
// Keep scalar projections from materializing the entire C snapshot in loops.
// Callers that cross an owner boundary still receive their independent value.
#[inline(always)]
pub(super) fn read(index: usize) -> Range {
    unsafe { slot(index).read() }
}
pub(super) fn update<T>(index: usize, operation: impl FnOnce(&mut Range) -> T) -> T {
    // No pointer/reference to a different owner is retained by this closure.
    operation(unsafe { &mut *slot(index) })
}
/// # Safety
/// Singleton BSP initialization before any reader or C lease is admitted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_ranges_init() -> *mut Range {
    for index in 0..RANGES {
        unsafe {
            slot(index).write(Range::EMPTY);
        }
    }
    slot(0)
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reserve {
    pub eviction_allowed: bool,
    pub window: u8,
    pub kind: u8,
    pub owner: u8,
    pub owner_id: u64,
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub flags: u64,
    pub base: u64,
    pub len: u64,
}
fn query(operation: u8, argument: u64) -> usize {
    unsafe { vm_index::r4os_rust_vm_index_query(operation, argument) }
}
fn change(operation: u8, id: u32, slot: usize) -> bool {
    unsafe { vm_index::r4os_rust_vm_index_change(operation, id, slot) }
}
fn reserve(request: Reserve) -> Result<u32, u32> {
    let mut definition = MaybeUninit::<virt_policy::Window>::uninit();
    unsafe {
        virt_policy::r4os_rust_vm_window(request.window, definition.as_mut_ptr());
    }
    let definition = unsafe { definition.assume_init() };
    let end = request.base.checked_add(request.len).ok_or(5u32)?;
    let window_end = definition.base.checked_add(definition.len).ok_or(5u32)?;
    if request.base < definition.base || end > window_end {
        return Err(15);
    }
    let position = query(3, request.base);
    let (previous, next) = vm_index::neighbors(position);
    if let Some(slot) = previous {
        let previous = read(slot);
        if previous
            .base
            .checked_add(previous.len)
            .is_none_or(|end| end > request.base)
        {
            return Err(7);
        }
    }
    if next.is_some_and(|slot| read(slot).base < end) {
        return Err(7);
    }
    let slot = query(0, 0);
    if slot == usize::MAX {
        return Err(2);
    }
    let id = query(8, 0) as u32;
    if id == 0 {
        return Err(5);
    }
    let block = blocks::Block {
        slot_used: false,
        id: 0,
        kind: request.kind,
        owner: request.owner,
        status: 1,
        owner_id: request.owner_id,
        name_ptr: request.name_ptr,
        name_len: request.name_len,
        phys_base: 0,
        phys_len: 0,
        virt_base: request.base,
        virt_len: request.len,
        reserved_bytes: request.len,
        committed_bytes: 0,
    };
    let mut block_id = 0;
    // This call ends before publishing a range; no table reference spans it.
    let code = unsafe { blocks::r4os_rust_blocks_register(&block, &mut block_id) };
    if code != 0 {
        return Err(match code {
            1..=3 => code,
            4 | 6 => 7,
            5 => 8,
            7 | 9 => 5,
            8 => 15,
            _ => unreachable!(),
        });
    }
    let range = Range {
        slot_used: true,
        id,
        window: request.window,
        kind: request.kind,
        owner: request.owner,
        owner_id: request.owner_id,
        status: 1,
        name_ptr: request.name_ptr,
        name_len: request.name_len,
        flags: request.flags,
        eviction_allowed: request.eviction_allowed,
        base: request.base,
        len: request.len,
        block_id,
        ..Range::EMPTY
    };
    update(slot, |current| *current = range);
    if !change(0, id, slot) {
        update(slot, |current| *current = Range::EMPTY);
        let _ = unsafe { blocks::r4os_rust_blocks_change(block_id, 4, 0, ptr::null()) };
        change(4, 0, slot);
        return Err(2);
    }
    change(2, 0, slot);
    Ok(id)
}
/// # Safety
/// VM owner is retained. Request contains valid enum values and a resident
/// borrowed name whose lifetime covers the resulting range/catalogue entry.
/// Input/output are exclusive live C records; request is already normalized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_reserve_at(request: *const Reserve, output: *mut u32) -> u32 {
    match reserve(unsafe { request.read() }) {
        Ok(id) => {
            unsafe {
                output.write(id);
            }
            0
        }
        Err(code) => code,
    }
}
const _: () = {
    assert!(core::mem::size_of::<OptionalSlot>() == 4);
    assert!(core::mem::size_of::<Reserve>() == 56);
    assert!(core::mem::size_of::<Range>() == 216);
    assert!(core::mem::offset_of!(Range, commit_span_head) == 202);
    assert!(core::mem::offset_of!(Range, page_state_span_head) == 206);
};
