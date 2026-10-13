use super::*;
use core::sync::atomic::{AtomicBool, Ordering};

static CASE_OWNER: AtomicBool = AtomicBool::new(false);
static CASE_TABLE: Storage = Storage(UnsafeCell::new(Table::EMPTY));
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
fn fill(table: &mut Table) -> bool {
    for ordinal in 0..MAX_BLOCKS {
        let request = Block {
            virt_base: 0x1000_0000_0000 + ordinal as u64 * 4096,
            virt_len: 4096,
            ..Block::REQUEST
        };
        match table.register(request) {
            Ok(_) => {}
            Err(Error::TableFull) => return true,
            Err(_) => return false,
        }
    }
    false
}
fn claimed(base: u64, len: u64) -> Block {
    let mut request = Block::physical(base, len, 8, 4, 47, COMMITTED, b"fixture-owner");
    request.reserved_bytes = len;
    request.committed_bytes = len;
    request
}
fn late_split_rollback(table: &mut Table) -> u32 {
    let free_id = table.register(free_request(0x1000, 0x3000)).unwrap();
    ensure!(fill(table), 1);
    let next_id = table.next_id;
    ensure!(
        matches!(table.claim(claimed(0x2000, 0x1000)), Err(Error::TableFull)),
        2
    );
    ensure!(table.next_id == next_id && table.invariant(), 3);
    let index = table.by_id(free_id).unwrap();
    ensure!(
        table.entries[index].canonical_free() && table.entries[index].phys_len == 0x3000,
        4
    );
    let summary = table.summary();
    ensure!(
        summary.free_physical_bytes == 0x3000 && summary.active_blocks == MAX_BLOCKS as u64,
        5
    );
    ensure!(
        table.hot.claim_rollbacks == 1 && table.claim(claimed(0x1000, 0x3000)).is_ok(),
        6
    );
    ensure!(
        table.invariant() && table.summary().free_physical_bytes == 0,
        7
    );
    0
}
fn full_release_plan(table: &mut Table) -> u32 {
    let free_id = table.register(free_request(0x1000, 0x1000)).unwrap();
    let claim_id = table.register(claimed(0x2000, 0x3000)).unwrap();
    ensure!(fill(table), 1);
    ensure!(
        matches!(table.prepare_release(0x3000, 0x1000), Err(Error::TableFull)),
        2
    );
    let index = table.by_id(claim_id).unwrap();
    ensure!(
        table.entries[index].phys_len == 0x3000
            && table.summary().free_physical_bytes == 0x1000
            && table.invariant(),
        3
    );
    // An edge release needs no fresh slot: the free neighbor extends, and
    // the old target slot carries the retained claimed suffix.
    let plan = table.prepare_release(0x2000, 0x1000).unwrap();
    ensure!(
        table.entries[index].phys_len == 0x3000 && table.invariant(),
        4
    );
    table.finish_release(&plan, false);
    let free_index = table.by_id(free_id).unwrap();
    ensure!(
        table.entries[free_index].phys_len == 0x2000
            && table.prefix(0x3000, 0x9000, false) == 0x2000,
        5
    );
    ensure!(
        table.invariant() && table.summary().free_physical_bytes == 0x2000,
        6
    );
    let plan = table.prepare_release(0x3000, 0x2000).unwrap();
    table.finish_release(&plan, false);
    ensure!(
        table.invariant() && table.summary().free_physical_bytes == 0x4000,
        7
    );
    0
}
fn merge_overflow_rollback(table: &mut Table) -> u32 {
    let mut previous = claimed(0x1000, 0x1000);
    previous.reserved_bytes = u64::MAX;
    previous.committed_bytes = 0;
    let claimed_id = table.register(previous).unwrap();
    let free_id = table.register(free_request(0x2000, 0x1000)).unwrap();
    let next_id = table.next_id;
    ensure!(
        matches!(table.claim(claimed(0x2000, 0x1000)), Err(Error::Overflow)),
        1
    );
    let index = table.by_id(claimed_id).unwrap();
    ensure!(
        table.entries[index].phys_len == 0x1000 && table.entries[index].reserved_bytes == u64::MAX,
        2
    );
    let free_index = table.by_id(free_id).unwrap();
    ensure!(
        table.entries[free_index].canonical_free() && table.next_id == next_id,
        3
    );
    ensure!(
        table.invariant() && table.hot.claim_rollbacks == 1 && table.summary().active_blocks == 2,
        4
    );
    0
}
fn exhausted_ids_rollback(table: &mut Table) -> u32 {
    let free_id = table.register(free_request(0x1000, 0x3000)).unwrap();
    table.next_id = u32::MAX;
    ensure!(
        matches!(table.claim(claimed(0x2000, 0x1000)), Err(Error::Overflow)),
        1
    );
    let index = table.by_id(free_id).unwrap();
    ensure!(
        table.next_id == u32::MAX
            && table.entries[index].canonical_free()
            && table.entries[index].phys_len == 0x3000,
        2
    );
    ensure!(table.invariant() && table.summary().active_blocks == 1, 3);
    let last_id = table.claim(claimed(0x1000, 0x3000)).unwrap();
    ensure!(
        last_id == u32::MAX && table.next_id == 0 && table.invariant(),
        4
    );
    let plan = table.prepare_release(0x1000, 0x3000);
    ensure!(matches!(plan, Err(Error::Overflow)), 5);
    ensure!(
        table.prefix(0x1000, 0x3000, false) == 0x3000 && table.invariant(),
        6
    );
    0
}
fn collision_reuse(table: &mut Table) -> u32 {
    let first = table.register(claimed(0x1000, 0x1000)).unwrap();
    table.next_id = 16385;
    let second = table.register(claimed(0x3000, 0x1000)).unwrap();
    table.next_id = 32769;
    let third = table.register(claimed(0x5000, 0x1000)).unwrap();
    ensure!(
        id_start(first) == id_start(second) && id_start(second) == id_start(third),
        1
    );
    table.release(first).unwrap();
    ensure!(table.by_id(first).is_some() && table.invariant(), 2);
    let replacement = table.register(claimed(0x7000, 0x1000)).unwrap();
    ensure!(
        table.by_id(first).is_none() && table.by_id(replacement).is_some(),
        3
    );
    ensure!(
        table.by_id(second).is_some() && table.by_id(third).is_some() && table.invariant(),
        4
    );
    ensure!(table.summary().active_blocks == 3 && table.id_count == 3, 5);
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_owner_case(case: u32) -> u32 {
    while CASE_OWNER
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let _guard = CaseGuard;
    let table = unsafe { &mut *CASE_TABLE.0.get() };
    table.reset();
    match case {
        0 => late_split_rollback(table),
        1 => full_release_plan(table),
        2 => merge_overflow_rollback(table),
        3 => exhausted_ids_rollback(table),
        4 => collision_reuse(table),
        _ => u32::MAX,
    }
}
