//! Explicit native cases only; static storage remains until exact own reaps.
use super::*;
use crate::{
    arch::runtime,
    boot::log_route::{dec, puts},
    sched::{
        kernel_check::{self, tasks},
        sync_locks, sync_stats,
        sync_types::{Mutex, WaitSummary},
        task::{
            task_lifecycle as life,
            task_types::{Entry, Role},
        },
    },
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering::*},
};
struct Fixtures {
    guard: UnwindGuard,
    mutex: Mutex,
}
struct Storage(UnsafeCell<Fixtures>);
unsafe impl Sync for Storage {}
static FIX: Storage = Storage(UnsafeCell::new(Fixtures {
    guard: UnwindGuard::EMPTY,
    mutex: Mutex::EMPTY,
}));
static TASKS: [AtomicPtr<Task>; 3] = [const { AtomicPtr::new(ptr::null_mut()) }; 3];
static MODE: AtomicU32 = AtomicU32::new(0);
static OWNED: AtomicU32 = AtomicU32::new(0);
static FINISH: AtomicBool = AtomicBool::new(false);
static FAIL: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static REUSABLE: AtomicBool = AtomicBool::new(true);
fn guard() -> *mut UnwindGuard {
    unsafe { ptr::addr_of_mut!((*FIX.0.get()).guard) }
}
fn mutex() -> *mut Mutex {
    unsafe { ptr::addr_of_mut!((*FIX.0.get()).mutex) }
}
fn task(i: usize) -> *mut Task {
    TASKS[i].load(Acquire)
}
fn check(ok: bool, bit: u32) -> bool {
    if !ok {
        FAIL.fetch_or(bit, Relaxed);
    }
    ok
}
fn until(mut test: impl FnMut() -> bool) -> bool {
    let end = timer::tick_count().saturating_add(2000);
    while timer::tick_count() < end && !kernel_check::cancelled() {
        if test() {
            return true;
        }
        tasks::sleep(1, b"guard-handoff-probe");
    }
    test()
}
fn reset(mode: u32) -> bool {
    if !REUSABLE.load(Acquire) {
        return false;
    }
    unsafe {
        FIX.0.get().write(Fixtures {
            guard: UnwindGuard::EMPTY,
            mutex: Mutex::EMPTY,
        });
    }
    for t in &TASKS {
        t.store(ptr::null_mut(), Release);
    }
    MODE.store(mode, Release);
    OWNED.store(0, Release);
    FINISH.store(false, Release);
    FAIL.store(0, Release);
    DONE.store(0, Release);
    true
}
fn spawn(i: usize, entry: Entry, role: Role) -> bool {
    let t = tasks::create(b"guard-handoff-probe", entry, role);
    TASKS[i].store(t, Release);
    !t.is_null() && tasks::start(t)
}
fn blocked(i: usize, is_guard: bool) -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let t = task(i);
        if t.is_null() {
            return false;
        }
        let c = registry::core(t);
        let queue = if is_guard {
            ptr::addr_of_mut!((*guard()).queue.core)
        } else {
            ptr::addr_of_mut!((*mutex()).queue.core)
        };
        (*c).state == State::Blocked
            && (*c).running_cpu == 0xff
            && (*c).wait_node.queue == queue
            && (*c).wait_node.owner_generation == (*c).generation
            && !(*c).lock_edge.is_empty()
            && (*c).guard_offer.is_empty()
    }
}
fn queues_valid() -> bool {
    let mut snapshot =
        core::mem::MaybeUninit::<crate::sched::task::task_snapshots::QueueSnapshot>::uninit();
    unsafe {
        crate::sched::task::task_api::r4os_rust_task_snapshot(
            3,
            0,
            snapshot.as_mut_ptr().cast(),
            40,
        );
        snapshot.assume_init().valid
    }
}
fn clean() -> bool {
    FINISH.store(true, Release);
    let mut ok = true;
    for t in &TASKS {
        let p = t.load(Acquire);
        if !p.is_null() {
            ok = tasks::finish_task(p, false) && ok;
        }
    }
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let g = guard();
        let m = mutex();
        ok = ok
            && free(g)
            && (*g).owner_task.is_null()
            && (*g).queue.core.count == 0
            && (*g).waiter_ranks == [0; 4]
            && (*g).donation_rank == pi::NO_RANK
            && (*m).owner == 0
            && (*m).owner_task.is_null()
            && (*m).queue.core.count == 0
            && (*m).waiter_ranks == [0; 4]
            && (*m).donation_rank == pi::NO_RANK;
    }
    REUSABLE.store(ok, Release);
    ok
}
fn report(mode: u32, ok: bool, cleaned: bool) {
    puts(b"GUARD08731 case=");
    dec(u64::from(mode));
    puts(b" passed=");
    dec(u64::from(ok));
    puts(b" failures=");
    dec(u64::from(FAIL.load(Acquire)));
    puts(b" done=");
    dec(u64::from(DONE.load(Acquire)));
    puts(b" cleanup=");
    dec(u64::from(cleaned));
    puts(b"\r\n");
}
unsafe fn contender(i: usize) {
    unsafe {
        let mode = MODE.load(Acquire);
        let timeout = if i == 0 && mode == 2 { 40 } else { 2000 };
        let got = guard_enter(guard(), timeout);
        if got {
            check(guard_owned(guard()) && guard_leave(guard()), 1 << i);
        }
        check(got == (i != 0 || mode == 0), 8 << i);
        DONE.fetch_or(1 << i, Release);
    }
}
unsafe extern "C" fn first() {
    unsafe { contender(0) }
}
unsafe extern "C" fn second() {
    unsafe { contender(1) }
}
unsafe extern "C" fn third() {
    unsafe { contender(2) }
}
// 0: exact selected kill, 1: explicit cancellation, 2: real IRQ deadline while
// preemption alone is disabled. No Runtime owner spans the IF-on clock wait.
fn handoff(mode: u32) -> bool {
    if !reset(mode) {
        return false;
    }
    unsafe {
        let g = guard();
        if !guard_enter(g, 0) {
            return false;
        }
        let mut ok = true;
        for (i, entry) in [first as Entry, second, third].into_iter().enumerate() {
            ok = spawn(i, entry, if i == 2 { Role::Input } else { Role::Batch }) && ok;
            ok = until(|| blocked(i, true)) && ok;
            if !ok {
                break;
            }
        }
        let flags = runtime::save_runtime();
        if mode == 2 {
            scheduler::r4os_rust_scheduler_control(6, 0, 0);
        }
        let mut before = WaitSummary::default();
        sync_stats::r4os_rust_sync_wait_summary(&mut before);
        ok = guard_leave(g) && ok;
        let mut after = WaitSummary::default();
        sync_stats::r4os_rust_sync_wait_summary(&mut after);
        if ok {
            let c = registry::core(task(0));
            let other = registry::core(task(1));
            ok = offered(task(0), g)
                && (*g).depth == 0
                && (*c).state == State::Ready
                && (*c).unwind_guard_count == 0
                && (*c).held_lock_count == 0
                && (*c).wait_node.queue.is_null()
                && (*c).lock_edge.is_empty()
                && (*other).state == State::Blocked
                && blocked(2, true)
                && (*g).queue.core.count == 2
                && registry::wake_rank(task(0)) == 0
                && after.wake_all == before.wake_all
                && after.wake_one == before.wake_one + 1
                && after.directed_scans == before.directed_scans
                && queues_valid();
            let id = (*c).id;
            let generation = (*c).generation;
            if mode == 0 {
                ok = !life::kill(id, Some(generation.wrapping_add(1))) && ok;
                ok = life::kill(id, Some(generation)) && ok;
                ok = (*c).state == State::Dead && (*c).guard_offer.is_empty() && ok;
            } else if mode == 1 {
                ok = scheduler::r4os_rust_scheduler_task(
                    2,
                    task(0).cast(),
                    WaitResult::Cancelled as u64,
                    0,
                    ptr::null(),
                    0,
                ) != 0
                    && ok;
                ok = (*c).wait_result == WaitResult::Cancelled && (*c).guard_offer.is_empty() && ok;
            } else {
                ok = (*c).timeout_linked && (*c).wake_tick > timer::tick_count() && ok;
            }
            if mode != 2 {
                ok = offered(task(1), g)
                    && (*g).queue.core.count == 1
                    && blocked(2, true)
                    && (*c).inherited_rank_counts == [0; 4]
                    && ok;
            }
        }
        ok = queues_valid() && ok;
        let irq_before = timer::deadline_stats().timer_irqs;
        runtime::restore_runtime(flags);
        if mode == 2 {
            // Real timer delivery advances time and runs wake_expired. A saved
            // Ready successor cannot run/claim while this controller holds its
            // own preemption depth, but IF and Runtime remain available.
            for _ in 0..50_000_000 {
                let pending = {
                    let _runtime = RuntimeGuard::acquire();
                    !task(0).is_null() && !(*registry::core(task(0))).guard_offer.is_empty()
                };
                if !pending {
                    break;
                }
                core::hint::spin_loop();
            }
            let flags = runtime::save_runtime();
            if !task(0).is_null() {
                let c = registry::core(task(0));
                ok = (*c).guard_offer.is_empty()
                    && (*c).wait_result == WaitResult::Timeout
                    && !(*c).timeout_linked
                    && (*c).wake_tick == 0
                    && (*c).inherited_rank_counts == [0; 4]
                    && offered(task(1), g)
                    && blocked(2, true)
                    && (*g).queue.core.count == 1
                    && timer::deadline_stats().timer_irqs > irq_before
                    && ok;
            }
            ok = queues_valid() && ok;
            scheduler::r4os_rust_scheduler_control(7, 0, 0);
            runtime::restore_runtime(flags);
        }
        let cleaned = clean();
        ok = ok && FAIL.load(Acquire) == 0 && DONE.load(Acquire) == if mode == 0 { 6 } else { 7 };
        report(mode, ok, cleaned);
        ok && cleaned
    }
}
unsafe fn acquire(is_guard: bool) -> bool {
    unsafe {
        if is_guard {
            guard_enter(guard(), 2000)
        } else {
            sync_locks::mutex_lock(mutex(), 2000)
        }
    }
}
unsafe fn release(is_guard: bool) -> bool {
    unsafe {
        if is_guard {
            guard_leave(guard())
        } else {
            sync_locks::mutex_unlock(mutex())
        }
    }
}
unsafe extern "C" fn chain_owner() {
    unsafe {
        let is_guard = MODE.load(Acquire) == 3;
        if !check(acquire(is_guard), 1) {
            return;
        }
        OWNED.fetch_or(1, Release);
        check(until(|| FINISH.load(Acquire)), 2);
        check(release(is_guard), 4);
        DONE.fetch_or(1, Release);
    }
}
unsafe extern "C" fn chain_middle() {
    unsafe {
        let is_guard = MODE.load(Acquire) != 3;
        if !check(acquire(is_guard), 8) {
            return;
        }
        OWNED.fetch_or(2, Release);
        if check(acquire(!is_guard), 16) {
            check(release(!is_guard), 32);
        }
        check(release(is_guard), 64);
        DONE.fetch_or(2, Release);
    }
}
unsafe extern "C" fn chain_root() {
    unsafe {
        let is_guard = MODE.load(Acquire) != 3;
        if check(acquire(is_guard), 128) {
            check(release(is_guard), 256);
        }
        DONE.fetch_or(4, Release);
    }
}
fn chain(mode: u32) -> bool {
    if !reset(mode) {
        return false;
    }
    let mut ok = spawn(0, chain_owner, Role::Batch) && until(|| OWNED.load(Acquire) & 1 != 0);
    ok = spawn(1, chain_middle, Role::Batch) && until(|| blocked(1, mode == 3)) && ok;
    ok = spawn(2, chain_root, Role::Input) && until(|| blocked(2, mode != 3)) && ok;
    ok = until(|| {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            !task(0).is_null()
                && !task(1).is_null()
                && registry::wake_rank(task(0)) == 0
                && registry::wake_rank(task(1)) == 0
        }
    }) && ok;
    FINISH.store(true, Release);
    let cleaned = clean();
    ok = ok && FAIL.load(Acquire) == 0 && DONE.load(Acquire) == 7;
    report(mode, ok, cleaned);
    ok && cleaned
}
unsafe extern "C" fn cycle_guard() {
    unsafe {
        if !check(acquire(true), 1) {
            return;
        }
        OWNED.fetch_or(1, Release);
        check(until(|| OWNED.load(Acquire) & 2 != 0), 2);
        if check(acquire(false), 4) {
            check(release(false), 8);
        }
        check(release(true), 16);
        DONE.fetch_or(1, Release);
    }
}
unsafe extern "C" fn cycle_mutex() {
    unsafe {
        if !check(acquire(false), 32) {
            return;
        }
        OWNED.fetch_or(2, Release);
        check(until(|| blocked(0, false)), 64);
        let start = timer::tick_count();
        let got = acquire(true);
        check(
            !got && timer::tick_count().saturating_sub(start) < 2000,
            128,
        );
        if got {
            release(true);
        }
        check(release(false), 256);
        DONE.fetch_or(2, Release);
    }
}
fn cycle() -> bool {
    if !reset(5) {
        return false;
    }
    let mut ok = spawn(0, cycle_guard, Role::Batch) && until(|| OWNED.load(Acquire) & 1 != 0);
    ok = spawn(1, cycle_mutex, Role::Input) && ok;
    let cleaned = clean();
    ok = ok && FAIL.load(Acquire) == 0 && DONE.load(Acquire) == 3;
    report(5, ok, cleaned);
    ok && cleaned
}
pub(crate) fn run() -> bool {
    let ok = handoff(0) && handoff(1) && handoff(2) && chain(3) && chain(4) && cycle();
    if ok {
        puts(b"GUARD08731 PASS fifo=1 kill=1 cancel=1 irq-timeout=1 mixed=2 cycle=1 tasks=17 own=reaped\r\n");
    }
    ok
}
