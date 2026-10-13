use super::*;
use core::sync::atomic::{AtomicBool, Ordering};
static CASE_OWNER: AtomicBool = AtomicBool::new(false);
struct CaseGuard;
impl Drop for CaseGuard {
    fn drop(&mut self) {
        CASE_OWNER.store(false, Ordering::Release);
    }
}
macro_rules! ensure {
    ($condition:expr, $step:expr) => {
        if !$condition {
            return $step;
        }
    };
}
fn seed(slot: usize, id: u32, base: u64, len: u64) -> bool {
    vm_ranges::update(slot, |range| {
        *range = vm_ranges::Range {
            slot_used: true,
            id,
            window: 6,
            base,
            len,
            ..vm_ranges::Range::EMPTY
        }
    });
    unsafe {
        if !super::super::vm_index::r4os_rust_vm_index_change(0, id, slot) {
            return false;
        }
        super::super::vm_index::r4os_rust_vm_index_change(2, id, slot)
    }
}
fn query(operation: u8, argument: u64) -> usize {
    unsafe { super::super::vm_index::r4os_rust_vm_index_query(operation, argument) }
}
fn change(operation: u8, id: u32, slot: usize) {
    unsafe {
        super::super::vm_index::r4os_rust_vm_index_change(operation, id, slot);
    }
}
fn release_range(slot: usize, id: u32) {
    change(3, 0, slot);
    change(1, id, 0);
    vm_ranges::update(slot, |range| range.status = 5);
    change(4, 0, slot);
}
fn collision_reuse() -> u32 {
    ensure!(
        seed(0, 1, 0x5000, 0x1000) && seed(1, 2049, 0x1000, 0x1000),
        1
    );
    ensure!(
        query(1, 1) == 0 && query(1, 2049) == 1 && query(4, 0x1800) == 1,
        2
    );
    release_range(0, 1);
    ensure!(query(1, 1) == usize::MAX && query(1, 2049) == 1, 3);
    ensure!(query(0, 0) == 0 && seed(0, 4097, 0x9000, 0x1000), 4);
    ensure!(query(1, 1) == usize::MAX && query(1, 4097) == 0, 5);
    ensure!(query(4, 0x5000) == usize::MAX && query(4, 0x9000) == 0, 6);
    ensure!(
        query(3, 0x5000) == 1 && query(5, 0) == 2 && query(6, 0) == 2,
        7
    );
    0
}
fn full_ranges() -> u32 {
    for slot in 0..vm_ranges::RANGES {
        ensure!(
            seed(slot, slot as u32 + 1, 0x1000 + slot as u64 * 0x2000, 0x1000),
            1
        );
    }
    ensure!(
        query(0, 0) == usize::MAX && query(5, 0) == vm_ranges::RANGES,
        2
    );
    release_range(511, 512);
    ensure!(query(1, 512) == usize::MAX && query(0, 0) == 511, 3);
    ensure!(seed(511, 2560, 0x1000 + 511 * 0x2000, 0x1000), 4);
    ensure!(query(1, 2560) == 511 && query(1, 512) == usize::MAX, 5);
    for position in 0..vm_ranges::RANGES {
        let (previous, next) = super::super::vm_index::neighbors(position);
        ensure!(
            next == Some(position) && previous == position.checked_sub(1),
            6
        );
    }
    ensure!(query(0, 0) == usize::MAX, 7);
    0
}
fn merge_split() -> u32 {
    ensure!(seed(0, 1, 0x1000, 0x8000), 1);
    state(|table| {
        ensure!(
            table.add(1, 0x1000, 0x1000).is_ok() && table.add(1, 0x3000, 0x1000).is_ok(),
            2
        );
        ensure!(table.add(1, 0x2000, 0x1000).is_ok() && table.active == 1, 3);
        let mut reserved = match table.reserve_split(1, 0x2000, 0x1000) {
            Ok(value) => value,
            Err(_) => return 4,
        };
        ensure!(reserved.is_some() && table.active == 2, 5);
        ensure!(
            table.remove(1, 0x2000, 0x1000, &mut reserved).is_ok() && reserved.is_none(),
            6
        );
        let head = vm_ranges::read(0).commit_span_head.get();
        ensure!(
            table.active == 2 && !table.scan(head, 0x1000, 0x3000, true),
            7
        );
        ensure!(
            !table.scan(head, 0x2000, 0x1000, false) && table.scan(head, 0x3000, 0x1000, true),
            8
        );
        table.remove_all(1);
        ensure!(
            vm_ranges::read(0).commit_span_head.get().is_none() && table.active == 0,
            9
        );
        let mut current = table.free;
        let mut count = 0;
        while let Some(slot) = current {
            count += 1;
            ensure!(count <= CAPACITY, 10);
            current = table.spans[usize::from(slot)].next.get();
        }
        ensure!(count == CAPACITY, 11);
        0
    })
}
fn full_split_preflight() -> u32 {
    ensure!(seed(0, 1, 0x1000, 64 * 1024 * 1024), 1);
    state(|table| {
        ensure!(table.add(1, 0x1000, 0x3000).is_ok(), 2);
        for ordinal in 1..CAPACITY {
            ensure!(
                table
                    .add(1, 0x10000 + ordinal as u64 * 0x2000, 0x1000)
                    .is_ok(),
                3
            );
        }
        let head = vm_ranges::read(0).commit_span_head.get().unwrap();
        ensure!(table.active == CAPACITY as u32 && table.free.is_none(), 4);
        ensure!(table.reserve_split(1, 0x2000, 0x1000) == Err(2), 5);
        let mut missing = None;
        ensure!(table.remove(1, 0x2000, 0x1000, &mut missing) == Err(2), 6);
        ensure!(
            table.spans[usize::from(head)].base == 0x1000
                && table.spans[usize::from(head)].len == 0x3000,
            7
        );
        ensure!(
            table.active == CAPACITY as u32 && table.scan(Some(head), 0x1000, 0x3000, true),
            8
        );
        table.remove_all(1);
        ensure!(
            table.active == 0 && vm_ranges::read(0).commit_span_head.get().is_none(),
            9
        );
        0
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_metadata_case(case: u32) -> u32 {
    while CASE_OWNER
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let _guard = CaseGuard;
    unsafe {
        vm_ranges::r4os_rust_vm_ranges_init();
        let mut order = core::ptr::null();
        let mut positions = core::ptr::null();
        super::super::vm_index::r4os_rust_vm_index_init(&mut order, &mut positions);
    }
    state(|table| table.reset());
    match case {
        0 => collision_reuse(),
        1 => full_ranges(),
        2 => merge_split(),
        3 => full_split_preflight(),
        _ => u32::MAX,
    }
}
