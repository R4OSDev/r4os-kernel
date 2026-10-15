//! Bounded program inventory selection and epoch-validated page publication.
//! Stack values have explicit C layout; no inventory or registry state is copied.
use super::{info, registry, registry_state::state};
use crate::{
    contract::{
        self, ProgramInstanceSnapshot as Snapshot, ProgramInventoryCursor as Cursor,
        ProgramInventoryPageInfo as Page,
    },
    sched::sync_locks,
};
use core::{
    mem::{self, MaybeUninit},
    ptr,
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub total: u32,
    pub reserved: u32,
    pub active: u32,
    pub done: u32,
    pub retiring: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Selection {
    pub generation: u64,
    pub snapshot: Snapshot,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Scan {
    pub counts: Counts,
    pub returned: u32,
    pub has_more: bool,
}
#[repr(C)]
pub struct RegistrySnapshot {
    pub epoch: u64,
    pub counts: Counts,
    pub completion_total: u32,
    pub peak: u32,
    pub create_failures: u64,
    pub rollback_count: u64,
    pub last_error: i32,
}
unsafe fn sift_up(items: *mut Selection, start: usize) {
    unsafe {
        let mut index = start;
        while index != 0 {
            let parent = (index - 1) / 2;
            if (*items.add(parent)).generation >= (*items.add(index)).generation {
                break;
            }
            ptr::swap(items.add(parent), items.add(index));
            index = parent;
        }
    }
}
unsafe fn sift_down(items: *mut Selection, len: usize, start: usize) {
    unsafe {
        let mut index = start;
        loop {
            let left = index * 2 + 1;
            if left >= len {
                return;
            }
            let right = left + 1;
            let mut largest = left;
            if right < len && (*items.add(right)).generation > (*items.add(left)).generation {
                largest = right;
            }
            if (*items.add(index)).generation >= (*items.add(largest)).generation {
                return;
            }
            ptr::swap(items.add(index), items.add(largest));
            index = largest;
        }
    }
}
unsafe fn sort(items: *mut Selection, len: usize) {
    unsafe {
        for index in 1..len {
            let value = *items.add(index);
            let mut insert = index;
            while insert != 0 && (*items.add(insert - 1)).generation > value.generation {
                items.add(insert).write(*items.add(insert - 1));
                insert -= 1;
            }
            items.add(insert).write(value);
        }
    }
}
unsafe fn could_enter(
    items: *const Selection,
    len: usize,
    selected: usize,
    generation: u64,
) -> bool {
    len != 0 && (selected < len || generation < unsafe { (*items).generation })
}
unsafe fn consider(
    items: *mut Selection,
    len: usize,
    selected: *mut usize,
    generation: u64,
    snapshot: Snapshot,
) {
    if len == 0 {
        return;
    }
    unsafe {
        if *selected < len {
            items.add(*selected).write(Selection {
                generation,
                snapshot,
            });
            sift_up(items, *selected);
            *selected += 1;
        } else if generation < (*items).generation {
            items.write(Selection {
                generation,
                snapshot,
            });
            sift_down(items, *selected, 0);
        }
    }
}
unsafe fn scan(after: u64, items: *mut Selection, len: usize) -> Scan {
    unsafe {
        let mut counts = Counts::default();
        let mut eligible = 0u32;
        let mut selected = 0usize;
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for slot in &(*chunk).slots {
                match slot.state {
                    0 => {}
                    1 => counts.reserved = counts.reserved.saturating_add(1),
                    2..=5 => {
                        counts.total = counts.total.saturating_add(1);
                        if matches!(slot.state, 2 | 3) {
                            counts.active = counts.active.saturating_add(1);
                        } else if slot.state == 5 {
                            counts.done = counts.done.saturating_add(1);
                        } else {
                            counts.retiring = counts.retiring.saturating_add(1);
                        }
                        if slot.generation <= after {
                            continue;
                        }
                        eligible = eligible.saturating_add(1);
                        if !could_enter(items, len, selected, slot.generation) {
                            continue;
                        }
                        consider(
                            items,
                            len,
                            &mut selected,
                            slot.generation,
                            Snapshot {
                                version: u32::from(contract::program_inventory_version),
                                size: mem::size_of::<Snapshot>() as u32,
                                handle: registry::handle_for_slot(slot),
                                info: info::instance_info(&slot.instance),
                                state_generation: slot.generation,
                            },
                        );
                    }
                    6 | 7 => {
                        let node = slot.completion;
                        if node.is_null() || (*node).state == 3 {
                            continue;
                        }
                        counts.total = counts.total.saturating_add(1);
                        if (*node).state == 2 {
                            counts.done = counts.done.saturating_add(1);
                        } else {
                            counts.retiring = counts.retiring.saturating_add(1);
                        }
                        let h = (*node).handle;
                        if h.generation <= after {
                            continue;
                        }
                        eligible = eligible.saturating_add(1);
                        if !could_enter(items, len, selected, h.generation) {
                            continue;
                        }
                        consider(
                            items,
                            len,
                            &mut selected,
                            h.generation,
                            Snapshot {
                                version: u32::from(contract::program_inventory_version),
                                size: mem::size_of::<Snapshot>() as u32,
                                handle: h,
                                info: info::completion_info(node),
                                state_generation: h.generation,
                            },
                        );
                    }
                    _ => unreachable!(),
                }
            }
            chunk = (*chunk).next;
        }
        let mut node = (*state()).completion_head;
        while !node.is_null() {
            if (*node).state != 2 || (*node).slot_attached {
                node = (*node).next;
                continue;
            }
            counts.total = counts.total.saturating_add(1);
            counts.done = counts.done.saturating_add(1);
            let h = (*node).handle;
            if h.generation > after {
                eligible = eligible.saturating_add(1);
                if could_enter(items, len, selected, h.generation) {
                    consider(
                        items,
                        len,
                        &mut selected,
                        h.generation,
                        Snapshot {
                            version: u32::from(contract::program_inventory_version),
                            size: mem::size_of::<Snapshot>() as u32,
                            handle: h,
                            info: info::completion_info(node),
                            state_generation: h.generation,
                        },
                    );
                }
            }
            node = (*node).next;
        }
        sort(items, selected);
        Scan {
            counts,
            returned: selected as u32,
            has_more: eligible > selected as u32,
        }
    }
}
fn try_lock() -> bool {
    unsafe { sync_locks::r4os_rust_sync_mutex((*state()).mutex, 0, 0) }
}
unsafe fn cursor_valid(cursor: *const Cursor) -> bool {
    unsafe {
        (*cursor).version == u32::from(contract::program_inventory_version)
            && (*cursor).size >= mem::size_of::<Cursor>() as u32
            && (*cursor).snapshot_generation != 0
            && (*cursor).flags & contract::program_inventory_cursor_flag_initialized != 0
    }
}
unsafe fn init_page(cursor: *const Cursor, out: *mut Page, kind: u32) {
    unsafe {
        out.write(Page {
            version: u32::from(contract::program_inventory_version),
            size: mem::size_of::<Page>() as u32,
            snapshot_generation: (*cursor).snapshot_generation,
            kind,
            status: i32::from(contract::program_inventory_status_invalid),
            ..Page::default()
        })
    };
}
/// # Safety
/// Aligned mutable initialized values of len; start obeys original heap bounds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_sort(
    op: u8,
    items: *mut Selection,
    len: usize,
    start: usize,
) {
    unsafe {
        match op {
            0 => sift_up(items, start),
            1 => sift_down(items, len, start),
            2 => sort(items, len),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exclusive aligned selection capacity and selected count. Only the selected
/// prefix is initialized; insertion writes a complete value before reading it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_consider(
    items: *mut Selection,
    len: usize,
    selected: *mut usize,
    generation: u64,
    snapshot: Snapshot,
) {
    unsafe { consider(items, len, selected, generation, snapshot) }
}
/// # Safety
/// Selected prefix is initialized; nonempty full selection has a valid root.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_could_enter(
    items: *const Selection,
    len: usize,
    selected: usize,
    generation: u64,
) -> bool {
    unsafe { could_enter(items, len, selected, generation) }
}
/// # Safety
/// Caller holds registry admission. Exclusive resident selection capacity and
/// result output; rows are admitted through original bounded max heap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_scan(
    after: u64,
    items: *mut Selection,
    len: usize,
    out: *mut Scan,
) {
    unsafe { out.write(scan(after, items, len)) };
}
/// # Safety
/// Exclusive aligned output; try-lock failure leaves it untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_registry(out: *mut RegistrySnapshot) -> bool {
    if !try_lock() {
        return false;
    }
    unsafe {
        let current = scan(u64::MAX, ptr::null_mut(), 0);
        let s = state();
        let stats = (*s).stats;
        out.write(RegistrySnapshot {
            epoch: (*s).epoch,
            counts: current.counts,
            completion_total: (*stats)
                .completion_pending
                .saturating_add((*stats).completion_ready)
                .min(u64::from(u32::MAX)) as u32,
            peak: (*stats).peak_live.min(u64::from(u32::MAX)) as u32,
            create_failures: (*stats).launch_failures,
            rollback_count: (*stats).rollback_count,
            last_error: if (*stats).last_launch_error != 0 {
                (*stats).last_launch_error
            } else {
                (*stats).last_admission_error
            },
        });
    }
    registry::unlock();
    true
}
/// # Safety
/// Exclusive aligned output; original try-lock admission, no owner escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_epoch(out: *mut u64) -> bool {
    if !try_lock() {
        return false;
    }
    unsafe { out.write((*state()).epoch) };
    registry::unlock();
    true
}
/// # Safety
/// Resident aligned cursor, no mutation or registry admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_cursor(cursor: *const Cursor) -> bool {
    unsafe { cursor_valid(cursor) }
}
/// # Safety
/// Resident cursor and exclusive aligned page; caller preserves alias contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_init_page(
    cursor: *const Cursor,
    out: *mut Page,
    kind: u32,
) {
    unsafe { init_page(cursor, out, kind) }
}
/// # Safety
/// Aligned resident cursor/exclusive disjoint row capacity/page or null. Epoch
/// mismatch publishes restart before rows/cursor advancement. Heavy fields are
/// collected only for admitted candidates under the original registry lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_inventory_programs(
    cursor: *mut Cursor,
    out: *mut Snapshot,
    capacity: u32,
    page: *mut Page,
) -> i32 {
    if cursor.is_null() || out.is_null() || page.is_null() {
        return -1;
    }
    unsafe {
        init_page(
            cursor,
            page,
            u32::from(contract::program_inventory_kind_program),
        );
        if !cursor_valid(cursor)
            || capacity == 0
            || capacity > u32::from(contract::program_inventory_page_max)
        {
            return -1;
        }
        if !try_lock() {
            return -5;
        }
        if (*state()).epoch != (*cursor).program_epoch {
            (*page).status = i32::from(contract::program_inventory_status_restart);
            registry::unlock();
            return 0;
        }
        let mut selections =
            MaybeUninit::<[Selection; contract::program_inventory_page_max as usize]>::uninit();
        let items = selections.as_mut_ptr().cast::<Selection>();
        let result = scan((*cursor).program_after_generation, items, capacity as usize);
        for index in 0..result.returned as usize {
            out.add(index).write((*items.add(index)).snapshot);
        }
        let after = if result.returned == 0 {
            (*cursor).program_after_generation
        } else {
            (*items.add(result.returned as usize - 1)).generation
        };
        (*cursor).program_after_generation = after;
        page.write(Page {
            version: u32::from(contract::program_inventory_version),
            size: mem::size_of::<Page>() as u32,
            snapshot_generation: (*cursor).snapshot_generation,
            next_generation: after,
            total: result.counts.total,
            returned: result.returned,
            has_more: u32::from(result.has_more),
            kind: u32::from(contract::program_inventory_kind_program),
            status: i32::from(if result.has_more {
                contract::program_inventory_status_more
            } else {
                contract::program_inventory_status_complete
            }),
            ..Page::default()
        });
        registry::unlock();
        0
    }
}
const _: () = {
    assert!(mem::size_of::<Counts>() == 20);
    assert!(mem::size_of::<Scan>() == 28);
    assert!(mem::offset_of!(Scan, has_more) == 24);
    assert!(mem::size_of::<RegistrySnapshot>() == 64);
    assert!(mem::offset_of!(RegistrySnapshot, create_failures) == 40);
    assert!(mem::size_of::<Selection>() == 152);
    assert!(mem::offset_of!(Selection, snapshot) == 8);
};
