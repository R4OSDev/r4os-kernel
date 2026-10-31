//! Coherent VM range/window/owner snapshots under the existing VM owner.
use super::{
    owner::Guard,
    phys, virt_policy, vm_budget, vm_index,
    vm_ranges::{self, Range},
    vm_transactions,
};
use core::mem::MaybeUninit;
const APP_SYSTEM_RESERVE_FRAMES: u64 = 384;

/// Narrow admission views under the same VM owner as the range transitions.
/// Public diagnostic snapshots and their owner/window/kind filters stay below.
pub(crate) fn admission_application(owner: u64) -> vm_budget::Usage {
    let _owner = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() {
        return vm_budget::Usage::default();
    }
    vm_ranges::budget_application(owner)
}
pub(crate) fn admission_system() -> (vm_budget::Usage, u64) {
    let _owner = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() {
        return (vm_budget::Usage::default(), 0);
    }
    (
        vm_ranges::budget_global(),
        phys::free_frames().saturating_sub(APP_SYSTEM_RESERVE_FRAMES),
    )
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub active_ranges: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub guard_bytes: u64,
    pub largest_free_virtual_base: u64,
    pub largest_free_virtual_len: u64,
    pub app_system_reserve_frames: u64,
    pub app_available_frames: u64,
    pub overflow: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OwnerStats {
    pub active_ranges: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub fault_count: u64,
    pub failed_faults: u64,
    pub overflow: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub active_ranges: u32,
    pub committed_bytes: u64,
    pub reserved_bytes: u64,
}
fn add(target: &mut u64, value: u64, overflow: &mut bool) {
    if let Some(next) = target.checked_add(value) {
        *target = next;
    } else {
        *target = u64::MAX;
        *overflow = true;
    }
}
fn largest(window: u8) -> (u64, u64) {
    let mut definition = MaybeUninit::<virt_policy::Window>::uninit();
    unsafe {
        virt_policy::r4os_rust_vm_window(window, definition.as_mut_ptr());
    }
    let definition = unsafe { definition.assume_init() };
    let mut best = (definition.base, 0);
    let mut cursor = definition.base;
    let Some(end) = definition.base.checked_add(definition.len) else {
        return best;
    };
    let mut position = unsafe { vm_index::r4os_rust_vm_index_query(3, definition.base) };
    let count = unsafe { vm_index::r4os_rust_vm_index_query(5, 0) };
    while position < count {
        let (_, slot) = vm_index::neighbors(position);
        let range = vm_ranges::read(slot.unwrap());
        if range.base >= end {
            break;
        }
        if range.window != window {
            position += 1;
            continue;
        }
        if range.base > cursor && range.base - cursor > best.1 {
            best = (cursor, range.base - cursor);
        }
        let Some(range_end) = range.base.checked_add(range.len) else {
            return best;
        };
        cursor = cursor.max(range_end);
        position += 1;
    }
    if cursor < end && end - cursor > best.1 {
        best = (cursor, end - cursor);
    }
    best
}
fn stats() -> Stats {
    let _owner = Guard::acquire(5);
    let mut stats = Stats {
        app_system_reserve_frames: APP_SYSTEM_RESERVE_FRAMES,
        ..Stats::default()
    };
    if !vm_transactions::r4os_rust_vm_initialized() {
        return stats;
    }
    for slot in 0..vm_ranges::RANGES {
        let range = vm_ranges::read(slot);
        if !range.active() {
            continue;
        }
        stats.active_ranges += 1;
        add(&mut stats.reserved_bytes, range.len, &mut stats.overflow);
        add(
            &mut stats.committed_bytes,
            range.committed_bytes,
            &mut stats.overflow,
        );
        add(
            &mut stats.resident_bytes,
            range.resident_bytes,
            &mut stats.overflow,
        );
        add(
            &mut stats.fault_count,
            range.fault_count,
            &mut stats.overflow,
        );
        add(
            &mut stats.failed_faults,
            range.failed_faults,
            &mut stats.overflow,
        );
        add(&mut stats.guard_bytes, range.guard_len, &mut stats.overflow);
    }
    let (peak, overflow) = vm_ranges::global_resident_peak();
    stats.peak_resident_bytes = peak;
    stats.overflow |= overflow;
    for window in 0..9 {
        let (base, len) = largest(window);
        if len > stats.largest_free_virtual_len {
            stats.largest_free_virtual_base = base;
            stats.largest_free_virtual_len = len;
        }
    }
    let mut physical = MaybeUninit::<phys::Stats>::uninit();
    unsafe {
        phys::r4os_rust_phys_stats(physical.as_mut_ptr());
    }
    let physical = unsafe { physical.assume_init() };
    stats.app_available_frames = physical
        .free_frames
        .saturating_sub(APP_SYSTEM_RESERVE_FRAMES);
    stats
}
fn owner_stats_with_stack(
    owner: u8,
    owner_id: u64,
    window: u8,
    kind: u8,
    main_stack: u32,
) -> (OwnerStats, [u64; 2]) {
    let _owner = Guard::acquire(5);
    let mut stats = OwnerStats::default();
    let mut stack = [0; 2];
    if !vm_transactions::r4os_rust_vm_initialized() {
        return (stats, stack);
    }
    for slot in 0..vm_ranges::RANGES {
        let range = vm_ranges::read(slot);
        if !range.active() || range.owner != owner || range.owner_id != owner_id {
            continue;
        }
        if window != u8::MAX && range.window != window {
            continue;
        }
        if kind != u8::MAX && range.kind != kind {
            continue;
        }
        if main_stack != 0 && range.id == main_stack {
            stack = [range.len, range.committed_bytes];
        }
        stats.active_ranges += 1;
        add(&mut stats.reserved_bytes, range.len, &mut stats.overflow);
        add(
            &mut stats.committed_bytes,
            range.committed_bytes,
            &mut stats.overflow,
        );
        add(
            &mut stats.resident_bytes,
            range.resident_bytes,
            &mut stats.overflow,
        );
        add(
            &mut stats.fault_count,
            range.fault_count,
            &mut stats.overflow,
        );
        add(
            &mut stats.failed_faults,
            range.failed_faults,
            &mut stats.overflow,
        );
        stats.peak_resident_bytes = stats.peak_resident_bytes.max(range.peak_resident_bytes);
    }
    if owner == crate::contract::memory_owner_r4x_instance && window == u8::MAX && kind == u8::MAX {
        let (peak, overflow) = vm_ranges::process_resident_peak(owner_id);
        stats.peak_resident_bytes = peak;
        stats.overflow |= overflow;
    }
    (stats, stack)
}
fn owner_stats(owner: u8, owner_id: u64, window: u8, kind: u8) -> OwnerStats {
    owner_stats_with_stack(owner, owner_id, window, kind, 0).0
}
/// Process totals and its main-stack subset share the same VM-owner snapshot.
pub(crate) fn process_stats(owner: u64, main_stack: u32) -> (OwnerStats, [u64; 2]) {
    owner_stats_with_stack(
        crate::contract::memory_owner_r4x_instance,
        owner,
        u8::MAX,
        u8::MAX,
        main_stack,
    )
}
/// Narrow lifetime-retention view; no range walk or budget admission changes.
pub(crate) fn process_resident_peak(owner: u64) -> u64 {
    let _owner = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() {
        return 0;
    }
    vm_ranges::process_resident_peak(owner).0
}
/// # Safety
/// output is exclusive live C stats storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_stats(output: *mut Stats) {
    unsafe {
        output.write(stats());
    }
}
/// # Safety
/// output is exclusive live C stats storage. Enum values are valid; 255 means
/// no window/kind filter. The caller keeps the unchanged public field meaning.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_owner_stats(
    owner: u8,
    owner_id: u64,
    window: u8,
    kind: u8,
    output: *mut OwnerStats,
) {
    unsafe {
        output.write(owner_stats(owner, owner_id, window, kind));
    }
}
/// # Safety
/// window is valid and output is exclusive live C Usage storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_window_usage(window: u8, output: *mut Usage) {
    let _owner = Guard::acquire(5);
    let mut result = Usage::default();
    if vm_transactions::r4os_rust_vm_initialized() {
        for slot in 0..vm_ranges::RANGES {
            let range = vm_ranges::read(slot);
            if !range.active() || range.window != window {
                continue;
            }
            result.active_ranges += 1;
            result.committed_bytes += range.committed_bytes;
            result.reserved_bytes += range.len;
        }
    }
    unsafe {
        output.write(result);
    }
}
/// # Safety
/// output is exclusive live C Range storage. The copied borrowed name retains
/// its original caller lifetime; no mutable range pointer is returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_range_info(id: u32, output: *mut Range) -> bool {
    let _owner = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() {
        return false;
    }
    let slot = unsafe { vm_index::r4os_rust_vm_index_query(1, u64::from(id)) };
    if slot == usize::MAX {
        return false;
    }
    let range = vm_ranges::read(slot);
    if !range.active() {
        return false;
    }
    unsafe {
        output.write(range);
    }
    true
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 104);
    assert!(core::mem::size_of::<OwnerStats>() == 64);
    assert!(core::mem::size_of::<Usage>() == 24);
};
