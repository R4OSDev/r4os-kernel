//! Original coherent execution summary and bounded task/thread pagination.
//! Membership epochs protect the final publication; restart writes no caller
//! rows and never advances the cursor. All records borrow the native owners.
use super::{inventory, threads};
use crate::{
    contract::{
        self, ProgramInventoryCursor as Cursor, ProgramInventoryPageInfo as Page,
        ProgramInventorySummary as Summary, ProgramJoinHandle as JoinHandle,
        ProgramTaskSnapshot as TaskRow, ProgramThreadSnapshot as ThreadRow,
    },
    memory::{heap, owner::Guard},
    sched::task::{task_api, task_resources, task_snapshots},
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr,
};
const MAX: usize = contract::program_inventory_page_max as usize;
#[derive(Default)]
struct ThreadStats {
    epoch: u64,
    total: u32,
    running: u32,
    done: u32,
    joining: u32,
    peak: u32,
    create_failures: u64,
}
struct Sequence(UnsafeCell<u64>);
// SAFETY: same program-state owner as the removed Zig sequence admits updates.
unsafe impl Sync for Sequence {}
static SEQUENCE: Sequence = Sequence(UnsafeCell::new(1));
fn thread_stats() -> ThreadStats {
    let _guard = Guard::acquire(0);
    unsafe {
        let state = threads::state();
        let mut out = ThreadStats {
            epoch: (*state).epoch,
            total: (*state).count.min(u32::MAX as usize) as u32,
            peak: (*state).peak.min(u32::MAX as usize) as u32,
            create_failures: (*state).create_failures,
            ..ThreadStats::default()
        };
        let mut thread = (*state).head;
        while !thread.is_null() {
            match (*thread).state {
                1 | 2 => out.running = out.running.saturating_add(1),
                3 | 4 => out.done = out.done.saturating_add(1),
                0 => {}
                _ => unreachable!(),
            }
            if (*thread).join_lease_active {
                out.joining = out.joining.saturating_add(1);
            }
            thread = (*thread).registry_next;
        }
        out
    }
}
fn generation() -> u64 {
    let _guard = Guard::acquire(0);
    unsafe {
        let out = SEQUENCE.0.get().read();
        let next = out.wrapping_add(1);
        SEQUENCE.0.get().write(if next == 0 { 1 } else { next });
        if out == 0 { 1 } else { out }
    }
}
fn thread_epoch() -> u64 {
    let _guard = Guard::acquire(0);
    unsafe { (*threads::state()).epoch }
}
fn task_epoch() -> u64 {
    unsafe { task_api::r4os_rust_task_control(2, 0, 0) }
}
unsafe fn capture(out: *mut Summary) -> bool {
    unsafe {
        for _ in 0..8 {
            let mut program = MaybeUninit::<inventory::RegistrySnapshot>::uninit();
            if !inventory::r4os_rust_program_inventory_registry(program.as_mut_ptr()) {
                continue;
            }
            let program = program.assume_init();
            let task_before = task_epoch();
            let thread_before = thread_stats();
            let mut tasks = MaybeUninit::<task_snapshots::Summary>::uninit();
            task_api::r4os_rust_task_snapshot(0, 0, tasks.as_mut_ptr().cast(), 0);
            let tasks = tasks.assume_init();
            let mut failures = MaybeUninit::<task_resources::FailureStats>::uninit();
            task_api::r4os_rust_task_snapshot(1, 0, failures.as_mut_ptr().cast(), 0);
            let failures = failures.assume_init();
            let mut heap = MaybeUninit::<heap::Stats>::uninit();
            heap::r4os_rust_heap_stats(heap.as_mut_ptr());
            let heap = heap.assume_init();
            let thread_after = thread_stats();
            let task_after = task_epoch();
            let mut program_after = 0;
            if !inventory::r4os_rust_program_inventory_epoch(&mut program_after) {
                continue;
            }
            if program.epoch != program_after
                || task_before != task_after
                || thread_before.epoch != thread_after.epoch
            {
                continue;
            }
            let failure_total = failures
                .task_metadata
                .saturating_add(failures.stack)
                .saturating_add(failures.fpu)
                .saturating_add(failures.waiter)
                .saturating_add(failures.memory);
            out.write(Summary {
                version: u32::from(contract::program_inventory_version),
                size: mem::size_of::<Summary>() as u32,
                snapshot_generation: generation(),
                program_epoch: program_after,
                task_epoch: task_after,
                thread_epoch: thread_after.epoch,
                program_total: program.counts.total,
                program_active: program.counts.active,
                program_done: program.counts.done,
                program_retiring: program.counts.retiring,
                completion_total: program.completion_total,
                task_total: tasks.total,
                task_running: tasks.running,
                task_ready: tasks.ready,
                task_blocked: tasks.blocked,
                thread_total: thread_after.total,
                thread_running: thread_after.running,
                thread_done: thread_after.done,
                thread_joining: thread_after.joining,
                program_peak: program.peak,
                task_peak: task_api::r4os_rust_task_control(3, 0, 0) as u32,
                thread_peak: thread_after.peak,
                program_create_failures: program.create_failures,
                task_create_failures: failure_total,
                thread_create_failures: thread_after.create_failures,
                rollback_failures: program.rollback_count.saturating_add(u64::from(
                    u32::try_from(task_api::r4os_rust_task_control(14, 0, 0))
                        .expect("Task rollback count"),
                )),
                last_error: program.last_error,
                flags: contract::program_inventory_summary_flag_stable,
                program_reserved: program.counts.reserved,
                heap_active_blocks: u32::try_from(heap.active_blocks)
                    .expect("Kernel heap block count"),
                heap_used_bytes: heap.used_bytes as u64,
            });
            return true;
        }
    }
    false
}
unsafe fn populate(entries: *mut TaskRow, len: usize, expected: u64) -> bool {
    let _guard = Guard::acquire(0);
    unsafe {
        if (*threads::state()).epoch != expected {
            return false;
        }
        let mut thread = (*threads::state()).head;
        while !thread.is_null() {
            let candidate = thread;
            thread = (*candidate).registry_next;
            if !(*candidate).used || (*candidate).task_generation == 0 {
                continue;
            }
            let mut low = 0;
            let mut high = len;
            while low < high {
                let middle = low + (high - low) / 2;
                if (*entries.add(middle)).generation < (*candidate).task_generation {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
            if low < len
                && (*entries.add(low)).generation == (*candidate).task_generation
                && (*entries.add(low)).task_id == (*candidate).task_id
            {
                (*entries.add(low)).owner_instance_id = (*candidate).instance_id;
                (*entries.add(low)).instance_generation = (*candidate).instance_generation;
            }
        }
        (*threads::state()).epoch == expected
    }
}
unsafe fn page_init(cursor: *const Cursor, page: *mut Page, kind: u8) {
    unsafe { inventory::r4os_rust_program_inventory_init_page(cursor, page, u32::from(kind)) };
}
unsafe fn valid(cursor: *const Cursor, capacity: u32) -> bool {
    unsafe {
        inventory::r4os_rust_program_inventory_cursor(cursor)
            && capacity != 0
            && capacity <= u32::from(contract::program_inventory_page_max)
    }
}
unsafe fn restart(page: *mut Page) {
    unsafe { (*page).status = i32::from(contract::program_inventory_status_restart) };
}
/// # Safety
/// Resident exclusive summary. Original bounded admission leaves it unchanged
/// if its eight epoch-stability attempts cannot obtain a coherent snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_execution_summary(out: *mut Summary) -> bool {
    unsafe { capture(out) }
}
/// # Safety
/// Resident disjoint cursor/summary buffers or null, preserving prior restarts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_execution_begin(
    cursor: *mut Cursor,
    out: *mut Summary,
) -> i32 {
    unsafe {
        if cursor.is_null() || out.is_null() {
            return -1;
        }
        let restarts = if (*cursor).version == u32::from(contract::program_inventory_version)
            && (*cursor).size >= mem::size_of::<Cursor>() as u32
        {
            (*cursor).restarts
        } else {
            0
        };
        if !capture(out) {
            return -5;
        }
        cursor.write(Cursor {
            version: u32::from(contract::program_inventory_version),
            size: mem::size_of::<Cursor>() as u32,
            snapshot_generation: (*out).snapshot_generation,
            program_epoch: (*out).program_epoch,
            task_epoch: (*out).task_epoch,
            thread_epoch: (*out).thread_epoch,
            flags: contract::program_inventory_cursor_flag_initialized,
            restarts,
            ..Cursor::default()
        });
        0
    }
}
/// # Safety
/// Original resident disjoint cursor/row capacity/page buffers or null. Only
/// complete staged rows are published after final task and thread epoch checks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_execution_tasks(
    cursor: *mut Cursor,
    out: *mut TaskRow,
    capacity: u32,
    page: *mut Page,
) -> i32 {
    unsafe {
        if cursor.is_null() || out.is_null() || page.is_null() {
            return -1;
        }
        page_init(cursor, page, contract::program_inventory_kind_task);
        if !valid(cursor, capacity) {
            return -1;
        }
        // The existing task-owner slice API requires initialized row storage.
        let mut raw = [task_snapshots::InventoryRow::default(); MAX];
        let mut raw_page = MaybeUninit::<task_snapshots::InventoryPage>::uninit();
        task_api::r4os_rust_task_inventory(
            (*cursor).task_after_generation,
            raw.as_mut_ptr(),
            capacity as usize,
            raw_page.as_mut_ptr(),
        );
        let raw_page = raw_page.assume_init();
        if raw_page.epoch != (*cursor).task_epoch || thread_epoch() != (*cursor).thread_epoch {
            restart(page);
            return 0;
        }
        let mut staged = MaybeUninit::<[TaskRow; MAX]>::uninit();
        let items = staged.as_mut_ptr().cast::<TaskRow>();
        let returned = raw_page.returned as usize;
        for (index, row) in raw[..returned].iter().enumerate() {
            items.add(index).write(TaskRow {
                version: u32::from(contract::program_inventory_version),
                size: mem::size_of::<TaskRow>() as u32,
                task_id: row.task_id,
                state: u32::from(row.state),
                owner_instance_id: 0,
                flags: 0,
                generation: row.generation,
                instance_generation: 0,
                created_tick: row.created_tick,
                last_run_tick: row.last_run_tick,
                wake_tick: row.wake_tick,
                runtime_ticks: row.runtime_ticks,
                ..TaskRow::default()
            });
        }
        if !populate(items, returned, (*cursor).thread_epoch) {
            restart(page);
            return 0;
        }
        let _guard = Guard::acquire(0);
        if task_epoch() != (*cursor).task_epoch
            || (*threads::state()).epoch != (*cursor).thread_epoch
        {
            restart(page);
            return 0;
        }
        for index in 0..returned {
            out.add(index).write(items.add(index).read());
        }
        let after = if returned == 0 {
            (*cursor).task_after_generation
        } else {
            (*items.add(returned - 1)).generation
        };
        (*cursor).task_after_generation = after;
        page.write(Page {
            version: u32::from(contract::program_inventory_version),
            size: mem::size_of::<Page>() as u32,
            snapshot_generation: (*cursor).snapshot_generation,
            next_generation: after,
            total: raw_page.total,
            returned: raw_page.returned,
            has_more: u32::from(raw_page.has_more),
            kind: u32::from(contract::program_inventory_kind_task),
            status: i32::from(if raw_page.has_more {
                contract::program_inventory_status_more
            } else {
                contract::program_inventory_status_complete
            }),
            reserved0: 0,
        });
        0
    }
}
unsafe fn sift_up(items: *mut ThreadRow, start: usize) {
    unsafe {
        let mut index = start;
        while index != 0 {
            let parent = (index - 1) / 2;
            if (*items.add(parent)).handle.thread_generation
                >= (*items.add(index)).handle.thread_generation
            {
                break;
            }
            ptr::swap(items.add(parent), items.add(index));
            index = parent;
        }
    }
}
unsafe fn sift_down(items: *mut ThreadRow, len: usize, start: usize) {
    unsafe {
        let mut index = start;
        loop {
            let left = index * 2 + 1;
            if left >= len {
                return;
            }
            let right = left + 1;
            let mut largest = left;
            if right < len
                && (*items.add(right)).handle.thread_generation
                    > (*items.add(left)).handle.thread_generation
            {
                largest = right;
            }
            if (*items.add(index)).handle.thread_generation
                >= (*items.add(largest)).handle.thread_generation
            {
                return;
            }
            ptr::swap(items.add(index), items.add(largest));
            index = largest;
        }
    }
}
unsafe fn sort(items: *mut ThreadRow, len: usize) {
    unsafe {
        for index in 1..len {
            let value = items.add(index).read();
            let mut insert = index;
            while insert != 0
                && (*items.add(insert - 1)).handle.thread_generation
                    > value.handle.thread_generation
            {
                items.add(insert).write(items.add(insert - 1).read());
                insert -= 1;
            }
            items.add(insert).write(value);
        }
    }
}
/// # Safety
/// Original resident disjoint cursor/row capacity/page buffers or null. Staging
/// keeps only the lowest generations; incomplete/restarted pages publish no rows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_execution_threads(
    cursor: *mut Cursor,
    out: *mut ThreadRow,
    capacity: u32,
    page: *mut Page,
) -> i32 {
    unsafe {
        if cursor.is_null() || out.is_null() || page.is_null() {
            return -1;
        }
        page_init(cursor, page, contract::program_inventory_kind_thread);
        if !valid(cursor, capacity) {
            return -1;
        }
        let mut staged = MaybeUninit::<[ThreadRow; MAX]>::uninit();
        let items = staged.as_mut_ptr().cast::<ThreadRow>();
        let requested = capacity as usize;
        let mut returned = 0usize;
        let mut eligible = 0u32;
        let mut total = 0u32;
        let guard = Guard::acquire(0);
        if (*threads::state()).epoch != (*cursor).thread_epoch {
            restart(page);
            return 0;
        }
        let mut thread = (*threads::state()).head;
        while !thread.is_null() {
            let candidate = thread;
            thread = (*candidate).registry_next;
            if !(*candidate).used {
                continue;
            }
            total = total.saturating_add(1);
            if (*candidate).generation <= (*cursor).thread_after_generation {
                continue;
            }
            eligible = eligible.saturating_add(1);
            if returned == requested && (*candidate).generation >= (*items).handle.thread_generation
            {
                continue;
            }
            let snapshot = ThreadRow {
                version: u32::from(contract::program_inventory_version),
                size: mem::size_of::<ThreadRow>() as u32,
                handle: JoinHandle {
                    thread_id: (*candidate).id,
                    instance_id: (*candidate).instance_id,
                    thread_generation: (*candidate).generation,
                    instance_generation: (*candidate).instance_generation,
                    reserved: 0,
                },
                info: threads::info(candidate),
                state_generation: (*candidate).generation,
            };
            if returned < requested {
                items.add(returned).write(snapshot);
                sift_up(items, returned);
                returned += 1;
            } else {
                items.write(snapshot);
                sift_down(items, returned, 0);
            }
        }
        drop(guard);
        sort(items, returned);
        let _guard = Guard::acquire(0);
        if (*threads::state()).epoch != (*cursor).thread_epoch {
            restart(page);
            return 0;
        }
        for index in 0..returned {
            out.add(index).write(items.add(index).read());
        }
        let after = if returned == 0 {
            (*cursor).thread_after_generation
        } else {
            (*items.add(returned - 1)).handle.thread_generation
        };
        let has_more = eligible > returned as u32;
        (*cursor).thread_after_generation = after;
        page.write(Page {
            version: u32::from(contract::program_inventory_version),
            size: mem::size_of::<Page>() as u32,
            snapshot_generation: (*cursor).snapshot_generation,
            next_generation: after,
            total,
            returned: returned as u32,
            has_more: u32::from(has_more),
            kind: u32::from(contract::program_inventory_kind_thread),
            status: i32::from(if has_more {
                contract::program_inventory_status_more
            } else {
                contract::program_inventory_status_complete
            }),
            reserved0: 0,
        });
        0
    }
}
