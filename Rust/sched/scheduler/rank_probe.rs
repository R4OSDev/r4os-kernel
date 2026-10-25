//! Explicit live-session rank publication checks. Observe the real pending
//! owner here without adding a diagnostic hook to any scheduling hot path.
use super::*;
use crate::{
    arch::runtime::RuntimeGuard,
    sched::{
        kernel_check::{self, tasks},
        task::{
            task_registry as registry,
            task_types::{Role, State as TaskState, Task as Descriptor},
        },
        task_context::Admission,
    },
};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32};
static RUNNER: AtomicPtr<Descriptor> = AtomicPtr::new(ptr::null_mut());
static PEER: AtomicPtr<Descriptor> = AtomicPtr::new(ptr::null_mut());
static MODE: AtomicU32 = AtomicU32::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);
static GO: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU32 = AtomicU32::new(0);
static FAILURES: AtomicU32 = AtomicU32::new(0);
fn check(ok: bool, bit: u32) -> bool {
    if !ok {
        FAILURES.fetch_or(bit, Ordering::Relaxed);
    }
    ok
}
fn before(end: u64) -> bool {
    !STOP.load(Ordering::Acquire) && timer::tick_count() < end
}
// Runtime held. Only descriptors pinned by this session are dereferenced.
unsafe fn exact_pair(running: *mut Descriptor, ready: *mut Descriptor) -> bool {
    unsafe {
        let cpu = percpu::current_index();
        let a = registry::core(running);
        let b = registry::core(ready);
        local().current == running.cast()
            && (*a).state == TaskState::Running
            && u32::from((*a).running_cpu) == cpu
            && (*b).state == TaskState::Ready
            && (*b).ready_linked
            && (*b).running_cpu == 0xff
            && u32::from((*b).home_cpu) == cpu
            && ready_count() == 1
    }
}
unsafe extern "C" fn runner() {
    let mode = MODE.load(Ordering::Relaxed);
    // The elapsed owner case separates actual accounting from asynchronous
    // IRQ/foreign-wake observations. Preserve the original IF through it.
    let flags = if mode == 0 {
        Some(runtime::save_local())
    } else {
        None
    };
    let Some(_admission) = Admission::acquire(true) else {
        if let Some(flags) = flags {
            runtime::restore_local(flags);
        }
        check(false, 1);
        DONE.fetch_or(1, Ordering::Release);
        return;
    };
    let t = RUNNER.load(Ordering::Acquire);
    let peer = PEER.load(Ordering::Acquire);
    let end = timer::tick_count().saturating_add(2000);
    let initial = {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            exact_pair(t, peer)
                && !local().pending
                && registry::dispatch_rank(t) == if mode == 2 { 2 } else { 0 }
        }
    };
    if check(initial, 2) {
        if mode == 0 {
            let clock = crate::platform::clocksource::now_nanoseconds;
            let deadline = clock().map(|v| v.saturating_add(250_000_000));
            // No Runtime borrow while waiting and no synthetic clock input.
            // The BSP and other CPUs keep their normal IRQ/worker progress.
            let scheduled = {
                let _runtime = RuntimeGuard::acquire();
                view(t.cast()).last_scheduled_tick
            };
            while since(tick(), scheduled) < 2
                && deadline.zip(clock()).is_some_and(|(end, now)| now < end)
            {
                core::hint::spin_loop();
            }
            let (published, elapsed) = {
                let _runtime = RuntimeGuard::acquire();
                let now = tick();
                let elapsed = since(now, scheduled);
                unsafe {
                    let before = exact_pair(t, peer)
                        && !local().pending
                        && registry::dispatch_rank(t) == 0
                        && registry::dispatch_rank(peer) == 1
                        && elapsed >= 2;
                    if before {
                        // Same actual owner entry as on_tick, using fresh real
                        // elapsed time. IRQs cannot consume the edge first;
                        // the next IRQ accounts only the remaining delta.
                        let ticks = mutate(21, t.cast(), now, 0);
                        record_run(ticks);
                        record_overrun(ticks);
                    }
                    (
                        before
                            && exact_pair(t, peer)
                            && local().pending
                            && registry::dispatch_rank(t) == 2,
                        elapsed,
                    )
                }
            };
            runtime::restore_local(flags.unwrap());
            if check(published, 4) {
                check(safe_reschedule(), 8);
            }
            crate::boot::log_route::puts(b"RANK08728 elapsed owner ticks=");
            crate::boot::log_route::dec(elapsed);
            crate::boot::log_route::puts(if published {
                b" pending=published\n"
            } else {
                b" pending=FAILED\n"
            });
        } else if mode == 1 {
            // No pending request exists at this ordinary voluntary switch.
            // The selected peer consumes its last dispatch boost before Running.
            yield_task();
        } else {
            ARMED.store(true, Ordering::Release);
            while !GO.load(Ordering::Acquire) && before(end) {
                core::hint::spin_loop();
            }
            if check(GO.load(Ordering::Acquire), 32) {
                check(safe_reschedule(), 64);
            }
        }
        if mode != 1 {
            check(DONE.load(Ordering::Acquire) & 2 != 0, 128);
        }
    } else if let Some(flags) = flags {
        runtime::restore_local(flags);
    }
    DONE.fetch_or(1, Ordering::Release);
}
unsafe extern "C" fn peer() {
    let Some(_admission) = Admission::acquire(true) else {
        check(false, 256);
        DONE.fetch_or(2, Ordering::Release);
        return;
    };
    let t = PEER.load(Ordering::Acquire);
    let runner = RUNNER.load(Ordering::Acquire);
    let mode = MODE.load(Ordering::Relaxed);
    if mode == 1 {
        let observed = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                exact_pair(t, runner)
                    && registry::dispatch_rank(t) == 2
                    && (*registry::core(t)).role_budget_dispatches_remaining == 0
                    && (*registry::core(t)).role_budget_ticks_remaining != 0
                    && local().pending
                    && registry::dispatch_rank(runner) == 0
            }
        };
        if check(observed, 512) {
            check(safe_reschedule(), 1024);
        }
        check(DONE.load(Ordering::Acquire) & 1 != 0, 2048);
    } else if mode == 2 {
        let returned = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                exact_pair(t, runner)
                    && registry::dispatch_rank(t) == 0
                    && (*registry::core(t)).inherited_rank_counts == [1, 0, 0, 0]
                    && registry::remove_donation(t, 0)
                    && (*registry::core(t)).inherited_rank_counts == [0; 4]
                    && registry::dispatch_rank(t) == 3
                    && local().pending
            }
        };
        check(returned, 4096);
    }
    DONE.fetch_or(2, Ordering::Release);
}
fn quiet_cpu() -> Option<u32> {
    let end = timer::tick_count().saturating_add(2000);
    while !kernel_check::cancelled() && timer::tick_count() < end {
        {
            let _runtime = RuntimeGuard::acquire();
            for cpu in 1..MAX_CPUS as u32 {
                let s = cpu_state(cpu);
                if percpu::is_schedulable(cpu)
                    && s.initialized
                    && !s.current.is_null()
                    && view(s.current).cpu_idle
                    && query(3, ptr::null_mut(), u64::from(cpu)) == 0
                {
                    return Some(cpu);
                }
            }
        }
        tasks::sleep(1, b"spectra-rank-capacity");
        yield_task();
    }
    None
}
fn one(mode: u32) -> bool {
    let Some(cpu) = quiet_cpu() else {
        return false;
    };
    MODE.store(mode, Ordering::Relaxed);
    for flag in [&ARMED, &GO, &STOP] {
        flag.store(false, Ordering::Relaxed);
    }
    DONE.store(0, Ordering::Relaxed);
    FAILURES.store(0, Ordering::Relaxed);
    let a = tasks::create_on_cpu(
        b"spectra-rank-running",
        runner,
        if mode == 2 {
            Role::Interactive
        } else {
            Role::Input
        },
        cpu,
    );
    let b = tasks::create_on_cpu(
        b"spectra-rank-ready",
        peer,
        if mode == 2 {
            Role::Batch
        } else {
            Role::ShortCompletion
        },
        cpu,
    );
    RUNNER.store(a, Ordering::Release);
    PEER.store(b, Ordering::Release);
    let admitted = if a.is_null() || b.is_null() {
        false
    } else {
        let _runtime = RuntimeGuard::acquire();
        let ready = tasks::start(a) && tasks::start(b);
        if ready && mode == 1 {
            // Only our not-yet-running fixture budgets: isolate dispatch
            // exhaustion from the separately tested real two-tick boundary.
            unsafe {
                (*registry::core(a)).role_budget_ticks_remaining = 2000;
                (*registry::core(b)).role_budget_ticks_remaining = 2000;
                (*registry::core(b)).role_budget_dispatches_remaining = 1;
            }
        }
        ready
    };
    let end = timer::tick_count().saturating_add(2000);
    let mut donated = false;
    if admitted && mode == 2 {
        while !ARMED.load(Ordering::Acquire)
            && DONE.load(Ordering::Acquire) == 0
            && before(end)
            && !kernel_check::cancelled()
        {
            tasks::sleep(1, b"spectra-rank-promotion-ready");
        }
        let promoted = {
            let _runtime = RuntimeGuard::acquire();
            unsafe {
                let c = registry::core(b);
                let exact = ARMED.load(Ordering::Acquire)
                    && cpu != percpu::current_index()
                    && cpu_state(cpu).current == a.cast()
                    && !cpu_state(cpu).pending
                    && (*c).state == TaskState::Ready
                    && (*c).ready_linked
                    && (*c).running_cpu == 0xff
                    && u32::from((*c).home_cpu) == cpu
                    && registry::dispatch_rank(b) == 3;
                if exact {
                    donated = registry::donate((*c).id, (*c).generation, 0);
                }
                exact
                    && donated
                    && registry::dispatch_rank(b) == 0
                    && (*c).ready_rank == 0
                    && cpu_state(cpu).pending
                    && cpu_state(cpu).current == a.cast()
            }
        };
        check(promoted, 8192);
        GO.store(true, Ordering::Release);
    }
    while admitted && DONE.load(Ordering::Acquire) != 3 && before(end) && !kernel_check::cancelled()
    {
        tasks::sleep(1, b"spectra-rank-join");
    }
    STOP.store(true, Ordering::Release);
    let passed =
        admitted && DONE.load(Ordering::Acquire) == 3 && FAILURES.load(Ordering::Acquire) == 0;
    if donated {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            // Failure cleanup only; normal peer already returned this count.
            if (*registry::core(b)).inherited_rank_counts[0] != 0 {
                registry::remove_donation(b, 0);
            }
        }
    }
    let mut clean = true;
    for t in [a, b] {
        if !t.is_null() {
            clean &= tasks::finish_task(t, !passed);
        }
    }
    if !passed || !clean {
        crate::boot::log_route::puts(b"RANK08728 FAIL mode=");
        crate::boot::log_route::dec(u64::from(mode));
        crate::boot::log_route::puts(b" failures=");
        crate::boot::log_route::dec(u64::from(FAILURES.load(Ordering::Acquire)));
        crate::boot::log_route::puts(b"\n");
    }
    passed && clean
}
pub(crate) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1 == 0 {
        crate::boot::log_route::puts(b"RANK08728 unavailable: schedulable AP required\n");
        return true;
    }
    for mode in 0..3 {
        if !one(mode) {
            return false;
        }
    }
    crate::boot::log_route::puts(b"RANK08728 PASS elapsed-budget=1 dispatch-budget=1 remote-donation=1 returned=1 tasks=6 own=reaped\n");
    true
}
