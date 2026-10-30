//! Extend the real placement/FPU/wait fixture: actual first migration, short
//! block/wake, one exception and a second wake with its budget already spent.
use super::*;
use core::sync::atomic::AtomicU64;
static ACTOR: AtomicPtr<Task> = AtomicPtr::new(ptr::null_mut());
static PEERS: [AtomicPtr<Task>; 2] = [const { AtomicPtr::new(ptr::null_mut()) }; 2];
static PHASE: AtomicU32 = AtomicU32::new(0);
static STAGE: AtomicU32 = AtomicU32::new(0);
static BUSY: AtomicU32 = AtomicU32::new(0);
static RELEASE: AtomicBool = AtomicBool::new(false);
static FINISHED: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static CPUS: [AtomicU32; 3] = [const { AtomicU32::new(u32::MAX) }; 3];
static WAKES: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static DELAYS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
struct Signal(UnsafeCell<Event>);
// The exclusive admitted controller resets this only after actual retirement.
unsafe impl Sync for Signal {}
static SIGNAL: Signal = Signal(UnsafeCell::new(Event::AUTO));
unsafe extern "C" fn actor() {
    let t = scheduler::current().cast::<Task>();
    let Some(_admission) = Admission::acquire(true) else {
        ERRORS.fetch_add(1, Ordering::Relaxed);
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
    let expected = [0x8739_0000, identity.1, !identity.1, 0x1234_5678_9abc_def0];
    for round in 0..3 {
        if STOP.load(Ordering::Acquire) {
            break;
        }
        STAGE.store(round as u32, Ordering::Release);
        // The existing store helper executes vzeroupper after observing YMM0.
        // Reload before each new wait so every real migration checks a full
        // fresh four-word vector rather than the prior probe's cleared half.
        unsafe { fpu::load(&expected) };
        let result = unsafe { sync_objects::r4os_rust_sync_event(SIGNAL.0.get(), 2, u64::MAX) };
        let observed = unsafe { fpu::read() };
        if result != 1 || observed != expected {
            ERRORS.fetch_add(1, Ordering::Relaxed);
        }
        CPUS[round].store(percpu::current_index(), Ordering::Release);
        DELAYS[round].store(
            timer::tick_count().saturating_sub(WAKES[round].load(Ordering::Acquire)),
            Ordering::Release,
        );
    }
    let valid = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            let valid = ((*c).id, (*c).generation) == identity
                && token.active
                && token.counter == ptr::addr_of_mut!((*c).unwind_guard_count)
                && (*c).unwind_guard_count == depth + 1;
            {
                let left = task_context::leave(token);
                valid && left && (*c).unwind_guard_count == depth
            }
        }
    };
    if !valid {
        ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    FINISHED.store(true, Ordering::Release);
}
unsafe extern "C" fn peer() {
    let t = scheduler::current().cast::<Task>();
    let Some(index) = PEERS.iter().position(|p| p.load(Ordering::Acquire) == t) else {
        ERRORS.fetch_add(1, Ordering::Relaxed);
        return;
    };
    BUSY.fetch_or(1 << index, Ordering::Release);
    // Real bounded competing Input work. The controller releases it at the
    // control cutoff (or abort); no foreign task/queue facts are fabricated.
    while !RELEASE.load(Ordering::Acquire) && !kernel_check::cancelled() {
        core::hint::spin_loop();
    }
}
fn wait(end: u64, initial: bool, mut ready: impl FnMut() -> bool) -> bool {
    while !ready() {
        if kernel_check::cancelled()
            || timer::tick_count() >= end
            || ERRORS.load(Ordering::Acquire) != 0
        {
            return false;
        }
        if initial {
            tasks::sleep(1, b"spectra-cooldown-enroll");
        } else {
            scheduler::yield_task();
        }
    }
    true
}
fn enrolled(round: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    let t = ACTOR.load(Ordering::Acquire);
    if t.is_null() {
        return false;
    }
    unsafe {
        let c = registry::core(t);
        STAGE.load(Ordering::Acquire) == round
            && (*c).state == State::Blocked
            && (*c).running_cpu == 0xff
            && !(*c).wait_node.queue.is_null()
            && (*SIGNAL.0.get()).queue.core.count == 1
            && (*c).wait_node.queue == ptr::addr_of_mut!((*SIGNAL.0.get()).queue.core)
    }
}
fn start_peer(index: usize, cpu: u32) -> bool {
    let t = tasks::create_on_cpu(b"spectra-cooldown-busy", peer, Role::Input, cpu);
    if t.is_null() {
        return false;
    }
    PEERS[index].store(t, Ordering::Release);
    tasks::start(t)
}
fn cleanup(abort: bool) -> bool {
    STOP.store(abort, Ordering::Release);
    RELEASE.store(true, Ordering::Release);
    unsafe { sync_objects::r4os_rust_sync_event(SIGNAL.0.get(), 0, 0) };
    let mut valid = true;
    for slot in core::iter::once(&ACTOR).chain(PEERS.iter()) {
        let t = slot.load(Ordering::Acquire);
        if !t.is_null() {
            if tasks::finish_task(t, abort) {
                slot.store(ptr::null_mut(), Ordering::Release);
            } else {
                valid = false;
            }
        }
    }
    valid && unsafe { helpers::queue_valid() }
}
fn body(ready_path: bool, end: u64) -> Option<(u32, u32, u64, u64)> {
    let t = ACTOR.load(Ordering::Acquire);
    PHASE.store(1, Ordering::Relaxed);
    if !wait(end, true, || enrolled(0)) {
        return None;
    }
    let source = percpu::current_index();
    {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            // Only this owned saved diagnostic frame loses its temporary pin.
            (*registry::core(t)).cpu_affinity_bound = false;
            crate::sched::task::balance::note_ready_publication(t);
            WAKES[0].store(timer::tick_count(), Ordering::Release);
            sync_objects::r4os_rust_sync_event(SIGNAL.0.get(), 0, 0);
        }
    }
    if !wait(end, false, || enrolled(1)) {
        return None;
    }
    PHASE.store(2, Ordering::Relaxed);
    let first = CPUS[0].load(Ordering::Acquire);
    let first_stamp = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            if first == source
                || (*c).home_cpu != first as u8
                || (*c).last_migration_tick == 0
                || (*c).last_migration_exception_tick != 0
            {
                return None;
            }
            (*c).last_migration_tick
        }
    };
    if !start_peer(0, first) || !wait(end, false, || BUSY.load(Ordering::Acquire) & 1 != 0) {
        return None;
    }
    PHASE.store(3, Ordering::Relaxed);
    let exception_stamp = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            let now = timer::tick_count();
            if now.saturating_sub(first_stamp) >= scheduler::PLACEMENT_COOLDOWN {
                return None;
            }
            if ready_path {
                (*c).cpu_affinity_bound = true;
                crate::sched::task::balance::note_ready_publication(t);
            }
            WAKES[1].store(now, Ordering::Release);
            sync_objects::r4os_rust_sync_event(SIGNAL.0.get(), 0, 0);
            let before = facts(t);
            if ready_path {
                if (*c).home_cpu != first as u8 {
                    return None;
                }
                (*c).cpu_affinity_bound = false;
                crate::sched::task::balance::note_ready_publication(t);
                scheduler::notify_ready(t.cast());
                if facts(t) != before {
                    return None;
                }
            }
            if (*c).home_cpu == first as u8
                || (*c).last_migration_exception_tick == 0
                || (*c).last_migration_exception_tick != (*c).last_migration_tick
                || ((*c).role == Role::ShortCompletion
                    && ((*c).role_budget_ticks_remaining == 0
                        || (*c).role_budget_dispatches_remaining == 0))
            {
                return None;
            }
            (*c).last_migration_exception_tick
        }
    };
    PHASE.store(4, Ordering::Relaxed);
    if !wait(end, false, || enrolled(2)) {
        return None;
    }
    PHASE.store(5, Ordering::Relaxed);
    let second = CPUS[1].load(Ordering::Acquire);
    if second == first
        || !start_peer(1, second)
        || !wait(end, false, || BUSY.load(Ordering::Acquire) == 3)
    {
        return None;
    }
    {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let c = registry::core(t);
            let now = timer::tick_count();
            if now.saturating_sub(exception_stamp) >= scheduler::PLACEMENT_COOLDOWN {
                return None;
            }
            WAKES[2].store(now, Ordering::Release);
            sync_objects::r4os_rust_sync_event(SIGNAL.0.get(), 0, 0);
            if (*c).home_cpu != second as u8
                || (*c).last_migration_exception_tick != exception_stamp
                || (*c).state != State::Ready
                || ((*c).role == Role::ShortCompletion
                    && ((*c).role_budget_ticks_remaining == 0
                        || (*c).role_budget_dispatches_remaining == 0))
                || !helpers::queue_valid()
            {
                return None;
            }
        }
    }
    // Observe the real no-second-exception control under competing work until
    // the ordinary cooldown can expire, then always release our own peers.
    PHASE.store(6, Ordering::Relaxed);
    let cutoff = exception_stamp.saturating_add(scheduler::PLACEMENT_COOLDOWN + 2);
    if !wait(end, false, || {
        timer::tick_count() >= cutoff || FINISHED.load(Ordering::Acquire)
    }) {
        return None;
    }
    RELEASE.store(true, Ordering::Release);
    if !wait(end, false, || FINISHED.load(Ordering::Acquire)) {
        return None;
    }
    let bypass_delay = DELAYS[1].load(Ordering::Acquire);
    let control_delay = DELAYS[2].load(Ordering::Acquire);
    if control_delay <= bypass_delay || ERRORS.load(Ordering::Acquire) != 0 {
        return None;
    }
    Some((first, second, bypass_delay, control_delay))
}
pub(super) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) != 15 {
        helpers::puts(b"COOLDOWN08739 unavailable: four-CPU fixture\r\n");
        return true;
    }
    for (ready_path, role) in [(false, Role::Interactive), (true, Role::ShortCompletion)] {
        if !ACTOR.load(Ordering::Acquire).is_null()
            || PEERS.iter().any(|p| !p.load(Ordering::Acquire).is_null())
        {
            return false;
        }
        unsafe { SIGNAL.0.get().write(Event::AUTO) };
        STAGE.store(u32::MAX, Ordering::Relaxed);
        BUSY.store(0, Ordering::Relaxed);
        RELEASE.store(false, Ordering::Relaxed);
        FINISHED.store(false, Ordering::Relaxed);
        STOP.store(false, Ordering::Relaxed);
        ERRORS.store(0, Ordering::Relaxed);
        for value in &CPUS {
            value.store(u32::MAX, Ordering::Relaxed);
        }
        let t = tasks::create_on_cpu(b"spectra-cooldown", actor, role, percpu::current_index());
        if t.is_null() {
            return false;
        }
        ACTOR.store(t, Ordering::Release);
        if !tasks::start(t) {
            cleanup(true);
            return false;
        }
        let result = body(ready_path, timer::tick_count().saturating_add(2000));
        if result.is_none() {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(t);
                helpers::puts(b"COOLDOWN08739 failure phase=");
                helpers::dec(u64::from(PHASE.load(Ordering::Relaxed)));
                helpers::puts(b" now=");
                helpers::dec(timer::tick_count());
                helpers::puts(b" last=");
                helpers::dec((*c).last_migration_tick);
                helpers::puts(b" exception=");
                helpers::dec((*c).last_migration_exception_tick);
                helpers::puts(b" home=");
                helpers::dec(u64::from((*c).home_cpu));
                helpers::puts(b" state=");
                helpers::dec((*c).state as u64);
                helpers::puts(b" stage=");
                helpers::dec(u64::from(STAGE.load(Ordering::Acquire)));
                helpers::puts(b" role_ticks=");
                helpers::dec(u64::from((*c).role_budget_ticks_remaining));
                helpers::puts(b" role_dispatches=");
                helpers::dec(u64::from((*c).role_budget_dispatches_remaining));
                helpers::puts(b" errors=");
                helpers::dec(u64::from(ERRORS.load(Ordering::Acquire)));
                helpers::puts(b"\r\n");
            }
        }
        let clean = cleanup(result.is_none());
        let Some((first, second, bypass, control)) = result else {
            return helpers::fail(b"cooldown-live-boundary");
        };
        if !clean {
            return helpers::fail(b"cooldown-live-retirement");
        }
        helpers::puts(b"COOLDOWN08739 PASS mode=");
        helpers::puts(if ready_path { b"ready" } else { b"wake" });
        helpers::puts(b" first=");
        helpers::dec(u64::from(first));
        helpers::puts(b" second=");
        helpers::dec(u64::from(second));
        helpers::puts(b" bypass_ticks=");
        helpers::dec(bypass);
        helpers::puts(b" control_ticks=");
        helpers::dec(control);
        helpers::puts(b" exception=1 budget=preserved fpu+unwind=3 own=retired\r\n");
    }
    true
}
