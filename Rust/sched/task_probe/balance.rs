//! One missing live fairness boundary: already-started tasks wake on an
//! uneven queue, migrate with FPU/unwind state, and retire exactly once.
mod occupied;
mod placement;
mod single;
use super::{fpu, helpers};
use crate::{
    arch::{percpu, runtime::RuntimeGuard},
    platform::timer,
    sched::{
        kernel_check::{self, tasks},
        scheduler, sync_objects,
        sync_types::Event,
        task::{
            task_registry as registry,
            task_types::{Role, State, Task, WaitResult},
        },
        task_context::{self, Admission},
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
};
const COUNT: usize = 4;
const ALL: u32 = (1 << COUNT) - 1;
fn wait_capacity(reason: &'static [u8]) {
    // Absolute timer wakes can observe the same ready service/shell peers at
    // each tick boundary. Let that real work execute before observing quiet
    // capacity again; callers keep their original deadline and exact facts.
    tasks::sleep(1, reason);
    scheduler::yield_task();
}
static TASKS: [AtomicPtr<Task>; COUNT] = [const { AtomicPtr::new(ptr::null_mut()) }; COUNT];
static FIRST: [AtomicU32; COUNT] = [const { AtomicU32::new(u32::MAX) }; COUNT];
static LAST: [AtomicU32; COUNT] = [const { AtomicU32::new(u32::MAX) }; COUNT];
static RUNS: [AtomicU32; COUNT] = [const { AtomicU32::new(0) }; COUNT];
static ENTERED: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static FAILURES: AtomicU32 = AtomicU32::new(0);
static ABORT: AtomicBool = AtomicBool::new(false);
static INITIAL_BARRIER: AtomicBool = AtomicBool::new(false);
struct Events(UnsafeCell<[Event; COUNT]>);
// Only this admitted session uses the events; all event metadata transitions
// retain the existing runtime owner, and reset follows actual task retirement.
unsafe impl Sync for Events {}
static EVENTS: Events = Events(UnsafeCell::new([const { Event::AUTO }; COUNT]));
fn event(index: usize) -> *mut Event {
    unsafe { EVENTS.0.get().cast::<Event>().add(index) }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Facts {
    identity: (u32, u64),
    order: u128,
    since: u64,
    role: (u8, u8),
    budgets: (u32, u16, u32, u16),
    inherited: [u16; 4],
    stack: (u64, u64),
    fpu: (usize, u32),
    unwind: (usize, u32),
}
unsafe fn facts(t: *mut Task) -> Facts {
    unsafe {
        let c = registry::core(t);
        Facts {
            identity: ((*c).id, (*c).generation),
            order: (*c).ready_order,
            since: (*c).ready_since_tick,
            role: ((*c).role as u8, (*c).priority as u8),
            budgets: (
                (*c).role_budget_ticks_remaining,
                (*c).role_budget_dispatches_remaining,
                (*c).donation_budget_ticks_remaining,
                (*c).donation_budget_dispatches_remaining,
            ),
            inherited: (*c).inherited_rank_counts,
            stack: ((*c).rsp, (*c).stack_range_id as u64),
            fpu: (
                (*c).fpu_state.map_or(0, |v| v.pointer.addr()),
                (*c).fpu_state_bytes,
            ),
            unwind: (
                ptr::addr_of!((*c).unwind_guard_count).addr(),
                (*c).unwind_guard_count,
            ),
        }
    }
}
unsafe extern "C" fn worker() {
    let t = scheduler::current().cast::<Task>();
    let Some(index) = TASKS.iter().position(|v| v.load(Ordering::Acquire) == t) else {
        FAILURES.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let Some(_admission) = Admission::acquire(true) else {
        FAILURES.fetch_add(1, Ordering::Relaxed);
        DONE.fetch_or(1 << index, Ordering::Release);
        return;
    };
    let (identity, token, depth) = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            let depth = (*c).unwind_guard_count;
            (((*c).id, (*c).generation), task_context::enter(), depth)
        }
    };
    let first = percpu::current_index();
    FIRST[index].store(first, Ordering::Relaxed);
    RUNS[index].fetch_add(1, Ordering::Relaxed);
    let expected = [
        0x57a0_0000_0000_0000 + index as u64,
        identity.1,
        !identity.1,
        0x1234_5678_9abc_def0,
    ];
    unsafe { fpu::load(&expected) };
    ENTERED.fetch_or(1 << index, Ordering::Release);
    while INITIAL_BARRIER.load(Ordering::Acquire)
        && ENTERED.load(Ordering::Acquire) != ALL
        && !ABORT.load(Ordering::Acquire)
        && !kernel_check::cancelled()
    {
        scheduler::yield_task();
    }
    let result = unsafe { sync_objects::r4os_rust_sync_event(event(index), 2, u64::MAX) };
    let observed = unsafe { fpu::read() };
    let last = percpu::current_index();
    let valid = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            let exact = scheduler::current() == t.cast()
                && ((*c).id, (*c).generation) == identity
                && token.active
                && token.counter == ptr::addr_of_mut!((*c).unwind_guard_count)
                && (*c).wait_result == WaitResult::Signaled
                && (*c).wait_node.queue.is_null()
                && (*c).unwind_guard_count == depth + 1;
            let returned = task_context::leave(token);
            exact && returned && (*c).unwind_guard_count == depth
        }
    };
    LAST[index].store(last, Ordering::Relaxed);
    if !ABORT.load(Ordering::Acquire) && (!valid || result != 1 || observed != expected) {
        FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    if DONE.fetch_or(1 << index, Ordering::Release) & (1 << index) != 0 {
        FAILURES.fetch_add(1, Ordering::Relaxed);
    }
}
fn enrolled(count: usize) -> bool {
    let _runtime = RuntimeGuard::acquire();
    if ENTERED.load(Ordering::Acquire) != (1 << count) - 1 {
        return false;
    }
    TASKS[..count].iter().enumerate().all(|(i, p)| unsafe {
        let t = p.load(Ordering::Acquire);
        if t.is_null() {
            return false;
        }
        let c = registry::core(t);
        (*c).state == State::Blocked
            && (*c).running_cpu == 0xff
            && (*event(i)).queue.core.count == 1
            && (*c).wait_node.queue == ptr::addr_of_mut!((*event(i)).queue.core)
    })
}
fn release(source: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let next_order = (*registry::raw()).next_ready_order;
        let mut before = [None; COUNT];
        for (i, slot) in before.iter_mut().enumerate() {
            // First two Ready tasks remain pinned while they form the skewed
            // queue. Only then release the three audited own bindings.
            if i == 2 {
                for p in &TASKS[..3] {
                    (*registry::core(p.load(Ordering::Acquire))).cpu_affinity_bound = false;
                }
            }
            sync_objects::r4os_rust_sync_event(event(i), 0, 0);
            let t = TASKS[i].load(Ordering::Acquire);
            if i < 2 && u32::from((*registry::core(t)).home_cpu) != source {
                return false;
            }
            *slot = Some(facts(t));
        }
        let preserved = before
            .iter()
            .enumerate()
            .all(|(i, value)| *value == Some(facts(TASKS[i].load(Ordering::Acquire))));
        let bound = registry::core(TASKS[3].load(Ordering::Acquire));
        preserved
            && (*registry::raw()).next_ready_order == next_order + COUNT as u128
            && (*bound).cpu_affinity_bound
            && u32::from((*bound).home_cpu) == source
            && helpers::queue_valid()
    }
}
fn finish(abort: bool) -> bool {
    if abort {
        ABORT.store(true, Ordering::Release);
    }
    for i in 0..COUNT {
        unsafe {
            sync_objects::r4os_rust_sync_event(event(i), 0, 0);
        }
    }
    let mut clean = true;
    for p in &TASKS {
        let t = p.load(Ordering::Acquire);
        if !t.is_null() {
            if tasks::finish_task(t, abort) {
                p.store(ptr::null_mut(), Ordering::Release);
            } else {
                clean = false;
            }
        }
    }
    clean
}
fn prepare() -> bool {
    if TASKS.iter().any(|v| !v.load(Ordering::Acquire).is_null()) {
        return false;
    }
    unsafe {
        EVENTS.0.get().write([const { Event::AUTO }; COUNT]);
    }
    ENTERED.store(0, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    FAILURES.store(0, Ordering::Relaxed);
    ABORT.store(false, Ordering::Release);
    INITIAL_BARRIER.store(false, Ordering::Release);
    for i in 0..COUNT {
        FIRST[i].store(u32::MAX, Ordering::Relaxed);
        LAST[i].store(u32::MAX, Ordering::Relaxed);
        RUNS[i].store(0, Ordering::Relaxed);
    }
    true
}
fn many() -> bool {
    if percpu::r4os_rust_percpu_control(15, 0, 0, 0) < 3 {
        helpers::puts(b"TASKREG08427 rebalance unavailable: requires controller, source and idle capacity\r\n");
        return true;
    }
    if !prepare() {
        return false;
    }
    let source = percpu::current_index();
    for (i, role) in [
        Role::Input,
        Role::ShortCompletion,
        Role::Batch,
        Role::Interactive,
    ]
    .into_iter()
    .enumerate()
    {
        let t = tasks::create_on_cpu(b"spectra-rebalance", worker, role, source);
        if t.is_null() {
            finish(true);
            return false;
        }
        TASKS[i].store(t, Ordering::Release);
    }
    for p in &TASKS {
        if !tasks::start(p.load(Ordering::Acquire)) {
            finish(true);
            return false;
        }
    }
    let end = timer::tick_count().saturating_add(2000);
    while !enrolled(COUNT) {
        if kernel_check::cancelled()
            || FAILURES.load(Ordering::Acquire) != 0
            || timer::tick_count() >= end
        {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-rebalance-enrollment");
    }
    let ready = release(source);
    if !ready {
        finish(true);
        return helpers::fail(b"rebalance-ready-facts");
    }
    while DONE.load(Ordering::Acquire) != ALL {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-rebalance-done");
    }
    let migrated = LAST[..3]
        .iter()
        .filter(|p| p.load(Ordering::Acquire) != source)
        .count();
    let valid = FIRST.iter().all(|p| p.load(Ordering::Acquire) == source)
        && LAST[3].load(Ordering::Acquire) == source
        && migrated != 0
        && RUNS.iter().all(|p| p.load(Ordering::Acquire) == 1)
        && FAILURES.load(Ordering::Acquire) == 0;
    let clean = finish(false) && unsafe { helpers::queue_valid() };
    if valid && clean {
        helpers::puts(b"TASKREG08427 rebalance PASS tasks=4 migrated=");
        helpers::dec(migrated as u64);
        helpers::puts(b" pinned=1 fpu=4 unwind=4 ready=preserved own=retired\r\n");
    }
    valid && clean
}
pub(super) fn run() -> bool {
    many() && single::run() && placement::run() && occupied::run()
}
