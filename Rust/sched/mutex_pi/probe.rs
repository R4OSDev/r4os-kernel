//! Explicit live kernel-check cases. Real locks, waits, deadlines, kills and
//! scheduler continuations; resident fixtures outlive every exact task receipt.
use super::*;
use crate::{
    arch::runtime::RuntimeGuard,
    boot::log_route::{dec, puts},
    platform::timer,
    sched::{
        kernel_check::{self, tasks},
        scheduler, sync_locks, sync_objects,
        sync_types::Event,
        sync_wait,
        task::{
            task_lifecycle as life,
            task_types::{Role, WaitResult},
            thread_placement,
        },
    },
};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
const COUNT: usize = 12;
struct Fixtures {
    locks: [Mutex; COUNT],
    gates: [Event; COUNT],
}
struct Storage(UnsafeCell<Fixtures>);
// The explicit kernel-check gate owns this storage until all exact ledger
// tasks are reaped. Reset is forbidden following an incomplete cleanup.
unsafe impl Sync for Storage {}
static FIXTURES: Storage = Storage(UnsafeCell::new(Fixtures {
    locks: [const { Mutex::EMPTY }; COUNT],
    gates: [const { Event::AUTO }; COUNT],
}));
static TASKS: [AtomicPtr<Task>; COUNT] = [const { AtomicPtr::new(ptr::null_mut()) }; COUNT];
static OWNED: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static FAILURES: AtomicU32 = AtomicU32::new(0);
static ORDER: AtomicU32 = AtomicU32::new(0);
static FIRST: AtomicU32 = AtomicU32::new(0);
static MODE: AtomicU32 = AtomicU32::new(0);
static STAGE: AtomicU32 = AtomicU32::new(0);
static MASKED: AtomicBool = AtomicBool::new(false);
static EXHAUSTED: AtomicBool = AtomicBool::new(false);
static REUSABLE: AtomicBool = AtomicBool::new(true);
fn check(ok: bool, bit: u32) -> bool {
    if !ok {
        FAILURES.fetch_or(bit, Ordering::Relaxed);
    }
    ok
}
fn lock(i: usize) -> *mut Mutex {
    unsafe { ptr::addr_of_mut!((*FIXTURES.0.get()).locks[i]) }
}
fn gate(i: usize) -> *mut Event {
    unsafe { ptr::addr_of_mut!((*FIXTURES.0.get()).gates[i]) }
}
fn task(i: usize) -> *mut Task {
    TASKS[i].load(Ordering::Acquire)
}
fn signal(i: usize) {
    unsafe {
        sync_objects::r4os_rust_sync_event(gate(i), 0, 0);
    }
}
fn await_gate(i: usize) -> bool {
    unsafe { sync_objects::r4os_rust_sync_event(gate(i), 2, 2000) == WaitResult::Signaled as u32 }
}
fn until(mut predicate: impl FnMut() -> bool) -> bool {
    let end = timer::tick_count().saturating_add(2000);
    while timer::tick_count() < end {
        if predicate() {
            return true;
        }
        tasks::sleep(1, b"mutex-chain-probe");
    }
    predicate()
}
fn blocked(i: usize, m: *mut Mutex) -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let c = registry::core(task(i));
        (*c).state == State::Blocked
            && (*c).running_cpu == 0xff
            && (*c).wait_node.queue == ptr::addr_of_mut!((*m).queue.core)
            && (*c).wait_node.owner_generation == (*c).generation
            && (*c).lock_edge.lock == Lock::Mutex(m)
            && valid_edge(task(i), (*c).lock_edge)
    }
}
fn dead(i: usize) -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe { (*registry::core(task(i))).state == State::Dead }
}
fn create(i: usize, entry: super::super::task::task_types::Entry, role: Role) -> bool {
    let t = if MODE.load(Ordering::Acquire) == 3 && i == 3 {
        tasks::create_on_cpu(b"mutex-chain-08730", entry, role, 0)
    } else {
        tasks::create(b"mutex-chain-08730", entry, role)
    };
    TASKS[i].store(t, Ordering::Release);
    !t.is_null() && tasks::start(t)
}
fn reset(mode: u32) -> bool {
    if !REUSABLE.load(Ordering::Acquire) {
        return false;
    }
    REUSABLE.store(false, Ordering::Release);
    unsafe {
        FIXTURES.0.get().write(Fixtures {
            locks: [const { Mutex::EMPTY }; COUNT],
            gates: [const { Event::AUTO }; COUNT],
        });
    }
    for t in &TASKS {
        t.store(ptr::null_mut(), Ordering::Relaxed);
    }
    for v in [&OWNED, &DONE, &FAILURES, &ORDER, &FIRST] {
        v.store(0, Ordering::Relaxed);
    }
    MASKED.store(false, Ordering::Relaxed);
    EXHAUSTED.store(false, Ordering::Relaxed);
    MODE.store(mode, Ordering::Release);
    STAGE.store(0, Ordering::Release);
    true
}
fn cleanup() -> bool {
    for i in 0..COUNT {
        signal(i);
    }
    let mut ok = true;
    for (i, item) in TASKS.iter().enumerate() {
        let t = task(i);
        if t.is_null() {
            continue;
        }
        ok &= until(|| dead(i));
        let exact = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(t);
                (*c).state == State::Dead
                    && (*c).held_lock_count == 0
                    && (*c).inherited_rank_counts == [0; 4]
                    && (*c).lock_edge.is_empty()
                    && !(*c).pi_queued
                    && (*c).wait_node.queue.is_null()
            }
        };
        ok &= exact;
        if tasks::finish_task(t, true) {
            item.store(ptr::null_mut(), Ordering::Release);
        } else {
            ok = false;
        }
    }
    let _runtime = RuntimeGuard::acquire();
    for i in 0..COUNT {
        unsafe {
            let m = lock(i);
            ok &= (*m).owner == 0
                && (*m).owner_generation == 0
                && (*m).depth == 0
                && (*m).owner_task.is_null()
                && (*m).donation_rank == NO_RANK
                && (*m).waiter_ranks == [0; 4]
                && (*m).queue.core.count == 0;
        }
    }
    REUSABLE.store(ok, Ordering::Release);
    ok
}
fn acquired(index: u32) {
    if ORDER.fetch_add(1, Ordering::AcqRel) == 0 {
        FIRST.store(index, Ordering::Release);
    }
}
unsafe extern "C" fn pair_actor() {
    let running = scheduler::current().cast::<Task>();
    let Some(i) = (0..5).find(|&i| task(i) == running) else {
        check(false, 1);
        return;
    };
    let mode = MODE.load(Ordering::Acquire);
    unsafe {
        match i {
            0 => {
                if check(sync_locks::mutex_lock(lock(1), 2000), 2) {
                    OWNED.fetch_or(1, Ordering::Release);
                    check(await_gate(0), 4);
                    if mode == 0 {
                        // Four real redispatches consume the unchanged inherited
                        // dispatch budget even while the real mutex remains held.
                        for _ in 0..4 {
                            tasks::sleep(1, b"mutex-donation-dispatch");
                        }
                        let _runtime = RuntimeGuard::acquire();
                        let c = registry::core(running);
                        EXHAUSTED.store(
                            (*c).donation_budget_dispatches_remaining == 0
                                && (*c).donation_budget_exhaustions != 0
                                && registry::dispatch_rank(running) == 3
                                && registry::wake_rank(running) == 3,
                            Ordering::Release,
                        );
                    }
                    check(sync_locks::mutex_unlock(lock(1)), 8);
                }
            }
            1 => {
                if check(sync_locks::mutex_lock(lock(1), 2000), 16) {
                    acquired(1);
                    check(sync_locks::mutex_unlock(lock(1)), 32);
                }
            }
            2 => {
                if check(sync_locks::mutex_lock(lock(0), 2000), 64) {
                    OWNED.fetch_or(4, Ordering::Release);
                    if check(sync_locks::mutex_lock(lock(1), 2000), 128) {
                        acquired(2);
                        if mode == 0 || mode == 3 {
                            let _runtime = RuntimeGuard::acquire();
                            let c = registry::core(running);
                            check(
                                (*c).inherited_rank_counts == [1, 0, 1, 0]
                                    && (*lock(1)).donation_rank == 2,
                                256,
                            );
                        }
                        // Remove the stronger M0 donation while still owning
                        // M1. The masked Interactive contribution must survive.
                        check(sync_locks::mutex_unlock(lock(0)), 512);
                        if mode == 0 || mode == 3 {
                            let _runtime = RuntimeGuard::acquire();
                            let c = registry::core(running);
                            MASKED.store(
                                (*c).inherited_rank_counts == [0, 0, 1, 0]
                                    && registry::dispatch_rank(running) == 2
                                    && (*c).donation_budget_dispatches_remaining <= 3,
                                Ordering::Release,
                            );
                        }
                        check(sync_locks::mutex_unlock(lock(1)), 1024);
                    } else {
                        check(sync_locks::mutex_unlock(lock(0)), 2048);
                    }
                }
            }
            _ => {
                let got = sync_locks::mutex_lock(lock(0), if mode == 1 { 200 } else { 2000 });
                if got {
                    check(sync_locks::mutex_unlock(lock(0)), 4096);
                }
                let expected = mode == 0 || (mode == 3 && i == 4);
                check(got == expected, 8192);
                if !got {
                    let _runtime = RuntimeGuard::acquire();
                    check(
                        (*registry::core(running)).wait_result
                            == if mode == 1 {
                                WaitResult::Timeout
                            } else {
                                WaitResult::Cancelled
                            },
                        16384,
                    );
                }
            }
        }
    }
    DONE.fetch_or(1 << i, Ordering::Release);
}
fn pair(mode: u32) -> bool {
    if !reset(mode) {
        return false;
    }
    let result = (|| {
        STAGE.store(1, Ordering::Release);
        if !create(0, pair_actor, Role::Batch) || !until(|| OWNED.load(Ordering::Acquire) & 1 != 0)
        {
            return false;
        }
        STAGE.store(2, Ordering::Release);
        if !create(1, pair_actor, Role::Interactive) || !until(|| blocked(1, lock(1))) {
            return false;
        }
        STAGE.store(3, Ordering::Release);
        if !create(2, pair_actor, Role::Batch) || !until(|| blocked(2, lock(1))) {
            return false;
        }
        STAGE.store(4, Ordering::Release);
        let initial = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                (*registry::core(task(0))).inherited_rank_counts == [0, 0, 1, 0]
                    && registry::wake_rank(task(2)) == 3
            }
        };
        if !initial || !create(3, pair_actor, Role::Input) || !until(|| blocked(3, lock(0))) {
            return false;
        }
        STAGE.store(5, Ordering::Release);
        let promoted = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                registry::wake_rank(task(2)) == 0
                    && registry::dispatch_rank(task(0)) == 0
                    && (*lock(0)).donation_rank == 0
                    && (*lock(1)).donation_rank == 0
                    && (*registry::core(task(2))).inherited_rank_counts == [1, 0, 0, 0]
            }
        };
        if !promoted {
            return false;
        }
        STAGE.store(6, Ordering::Release);
        if mode == 3 {
            // Use the actual placement owner on this saved, lock-free waiter.
            // It configures roles without the ordinary Ready adjust_rank path.
            let (id, generation) = tasks::identity(task(3));
            for (class, rank, tail) in [(1, 3, 2), (2, 1, 1)] {
                if thread_placement::set_probe_task(
                    id,
                    generation,
                    thread_placement::Policy {
                        cpus: 1,
                        preferred: None,
                        class,
                    },
                ) != crate::contract::thread_ok
                {
                    return false;
                }
                if !until(|| {
                    let _runtime = RuntimeGuard::acquire();
                    unsafe {
                        registry::wake_rank(task(2)) == rank
                            && registry::dispatch_rank(task(0)) == tail
                    }
                }) {
                    return false;
                }
            }
        }
        STAGE.store(7, Ordering::Release);
        if mode != 0 {
            if mode == 2 {
                let (id, generation) = tasks::identity(task(3));
                if !unsafe { life::kill(id, Some(generation)) } {
                    return false;
                }
            } else if mode == 3 {
                let closed = unsafe {
                    sync_wait::r4os_rust_sync_queue(
                        ptr::addr_of_mut!((*lock(0)).queue),
                        4,
                        ptr::null_mut(),
                        WaitResult::Cancelled as u64,
                        ptr::null_mut(),
                    )
                };
                if closed != 1 {
                    return false;
                }
            }
            STAGE.store(8, Ordering::Release);
            if !until(|| dead(3)) {
                return false;
            }
            STAGE.store(9, Ordering::Release);
            if !until(|| {
                let _runtime = RuntimeGuard::acquire();
                unsafe {
                    registry::wake_rank(task(2)) == 3
                        && registry::dispatch_rank(task(0)) == 2
                        && (*registry::core(task(2))).inherited_rank_counts == [0; 4]
                        && (*registry::core(task(0))).inherited_rank_counts == [0, 0, 1, 0]
                        && (*lock(0)).donation_rank == NO_RANK
                        && (*lock(1)).donation_rank == 2
                }
            }) {
                return false;
            }
            if mode == 3 {
                let reopened = unsafe {
                    sync_wait::r4os_rust_sync_queue(
                        ptr::addr_of_mut!((*lock(0)).queue),
                        5,
                        ptr::null_mut(),
                        0,
                        ptr::null_mut(),
                    )
                };
                if reopened != 1
                    || !create(4, pair_actor, Role::Input)
                    || !until(|| blocked(4, lock(0)))
                {
                    return false;
                }
            }
        }
        STAGE.store(10, Ordering::Release);
        signal(0);
        let expected = if mode == 3 {
            31
        } else if mode == 2 {
            7
        } else {
            15
        };
        until(|| DONE.load(Ordering::Acquire) == expected)
            && FIRST.load(Ordering::Acquire) == if mode == 0 || mode == 3 { 2 } else { 1 }
            && (mode != 0 || EXHAUSTED.load(Ordering::Acquire))
            && (!(mode == 0 || mode == 3) || MASKED.load(Ordering::Acquire))
    })();
    let clean = cleanup();
    report(mode, result, clean)
}
unsafe extern "C" fn cycle_actor() {
    let i = usize::from(scheduler::current() == task(1).cast());
    unsafe {
        if check(sync_locks::mutex_lock(lock(i), 2000), 1) {
            OWNED.fetch_or(1 << i, Ordering::Release);
            check(await_gate(i), 2);
            let got = sync_locks::mutex_lock(lock(1 - i), 2000);
            check(got == (i == 0), 4);
            if got {
                check(sync_locks::mutex_unlock(lock(1 - i)), 8);
            } else {
                let _runtime = RuntimeGuard::acquire();
                let c = registry::core(task(i));
                check(
                    (*c).wait_result == WaitResult::Failed
                        && (*c).lock_edge.is_empty()
                        && (*c).wait_node.queue.is_null()
                        && !(*c).pi_queued,
                    16,
                );
            }
            check(sync_locks::mutex_unlock(lock(i)), 32);
        }
    }
    DONE.fetch_or(1 << i, Ordering::Release);
}
fn cycle() -> bool {
    if !reset(4) {
        return false;
    }
    let result = (|| {
        if !create(0, cycle_actor, Role::Input)
            || !create(1, cycle_actor, Role::Batch)
            || !until(|| OWNED.load(Ordering::Acquire) == 3)
        {
            return false;
        }
        signal(0);
        if !until(|| blocked(0, lock(1))) {
            return false;
        }
        signal(1);
        until(|| DONE.load(Ordering::Acquire) == 3)
    })();
    let clean = cleanup();
    report(4, result, clean)
}
unsafe extern "C" fn chain_actor() {
    let current = scheduler::current().cast::<Task>();
    let Some(i) = (0..COUNT).find(|&i| task(i) == current) else {
        check(false, 1);
        return;
    };
    unsafe {
        if check(sync_locks::mutex_lock(lock(i), 2000), 2) {
            OWNED.fetch_or(1 << i, Ordering::Release);
            check(await_gate(i), 4);
            if i + 1 < COUNT && check(sync_locks::mutex_lock(lock(i + 1), 2000), 8) {
                check(sync_locks::mutex_unlock(lock(i + 1)), 16);
            }
            check(sync_locks::mutex_unlock(lock(i)), 32);
        }
    }
    DONE.fetch_or(1 << i, Ordering::Release);
}
fn chain() -> bool {
    if !reset(5) {
        return false;
    }
    let result = (|| {
        for i in 0..COUNT {
            if !create(i, chain_actor, Role::Batch) {
                return false;
            }
        }
        if !until(|| OWNED.load(Ordering::Acquire) == (1 << COUNT) - 1) {
            return false;
        }
        // Grow from the tail: bounded forward admission is deliberately not
        // misused as a claim that the entire component fits one pump batch.
        for i in 0..COUNT - 1 {
            signal(i);
            if !until(|| blocked(i, lock(i + 1))) {
                return false;
            }
        }
        let bounded = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(task(0));
                registry::donate((*c).id, (*c).generation, 0)
                    && pump()
                    && registry::dispatch_rank(task(COUNT - 1)) == 3
            }
        };
        if !bounded {
            return false;
        }
        // No direct pump here: normal sleep/dispatch/tick must continue it.
        if !until(|| {
            let _runtime = RuntimeGuard::acquire();
            unsafe { registry::dispatch_rank(task(COUNT - 1)) == 0 }
        }) {
            return false;
        }
        let withdrawing = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                registry::remove_donation(task(0), 0)
                    && pump()
                    && registry::dispatch_rank(task(COUNT - 1)) == 0
            }
        };
        if !withdrawing {
            return false;
        }
        if !until(|| {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                (0..COUNT).all(|i| (*registry::core(task(i))).inherited_rank_counts == [0; 4])
            }
        }) {
            return false;
        }
        signal(COUNT - 1);
        until(|| DONE.load(Ordering::Acquire) == (1 << COUNT) - 1)
    })();
    // A failed observation still returns any exact diagnostic contribution.
    if !task(0).is_null() {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            if (*registry::core(task(0))).inherited_rank_counts[0] != 0 {
                registry::remove_donation(task(0), 0);
            }
        }
    }
    let clean = cleanup();
    report(5, result, clean)
}
fn report(mode: u32, result: bool, clean: bool) -> bool {
    let ok = result && clean;
    let failures = FAILURES.load(Ordering::Acquire);
    puts(if ok && failures == 0 {
        b"MUTEX08730 case=PASS mode="
    } else {
        b"MUTEX08730 case=FAIL mode="
    });
    dec(u64::from(mode));
    puts(b" failures=");
    dec(u64::from(failures));
    for (label, value) in [
        (b" stage=".as_slice(), STAGE.load(Ordering::Acquire)),
        (b" result=", u32::from(result)),
        (b" clean=", u32::from(clean)),
        (b" first=", FIRST.load(Ordering::Acquire)),
        (b" masked=", u32::from(MASKED.load(Ordering::Acquire))),
        (b" exhausted=", u32::from(EXHAUSTED.load(Ordering::Acquire))),
        (b" owned=", OWNED.load(Ordering::Acquire)),
        (b" done=", DONE.load(Ordering::Acquire)),
    ] {
        puts(label);
        dec(u64::from(value));
    }

    puts(b"\n");
    ok && failures == 0
}
pub(crate) fn run() -> bool {
    assert!(kernel_check::live());
    for mode in 0..4 {
        if !pair(mode) {
            return false;
        }
    }
    if !cycle() || !chain() {
        return false;
    }
    puts(b"MUTEX08730 PASS late=1 handoff=1 masked=1 budgets=1 timeout=1 kill=1 close-reopen=1 cycle=1 deferred=12 tasks=31 own=reaped role-change=1\n");
    true
}
