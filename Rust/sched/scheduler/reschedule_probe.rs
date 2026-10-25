//! Explicit owned AP actors observe real sends and vector receipts. No probe
//! hook, foreign queue mutation, fabricated IRQ or altered clock is involved.
use super::*;
use crate::{
    arch::{runtime::RuntimeGuard, smp::reschedule},
    sched::{
        kernel_check::{self, tasks},
        task::task_types::Role,
        task_context::Admission,
    },
};
use core::sync::atomic::{AtomicBool, AtomicU32};
static ARMED: AtomicBool = AtomicBool::new(false);
static GO: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU32 = AtomicU32::new(0);
unsafe extern "C" fn runner() {
    if let Some(_admission) = Admission::acquire(true) {
        let end = timer::tick_count().saturating_add(2000);
        ARMED.store(true, Ordering::Release);
        // Native kernel IP deliberately defers IRQ preemption. The real
        // handler can consume a doorbell while scheduling pending stays set.
        while !GO.load(Ordering::Acquire)
            && !STOP.load(Ordering::Acquire)
            && timer::tick_count() < end
        {
            core::hint::spin_loop();
        }
    }
    DONE.fetch_add(1, Ordering::Release);
}
unsafe extern "C" fn peer() {
    DONE.fetch_add(1, Ordering::Release);
}
fn wait(mut test: impl FnMut() -> bool) -> bool {
    let end = timer::tick_count().saturating_add(2000);
    loop {
        if test() {
            return true;
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            return false;
        }
        tasks::sleep(1, b"spectra-reschedule-receipt");
    }
}
// Runtime held; the caller's session pins all actor identities.
fn quiet(cpu: u32) -> bool {
    let state = cpu_state(cpu);
    !state.current.is_null()
        && view(state.current).cpu_idle
        && query(3, ptr::null_mut(), u64::from(cpu)) == 0
        && !reschedule::snapshot(cpu).in_flight
}
pub(crate) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(14, 0, 0, 0) & !1 == 0 {
        return true;
    }
    let Some(cpu) = super::rank_probe::quiet_cpu() else {
        return false;
    };
    ARMED.store(false, Ordering::Relaxed);
    GO.store(false, Ordering::Relaxed);
    STOP.store(false, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    let mut actors = [ptr::null_mut(); 8];
    for (i, actor) in actors.iter_mut().enumerate() {
        *actor = tasks::create_on_cpu(
            b"spectra-reschedule-actor",
            if i == 0 { runner } else { peer },
            if i == 4 || i == 5 {
                Role::Input
            } else {
                Role::Batch
            },
            cpu,
        );
    }
    let allocated = actors.iter().all(|t| !t.is_null());
    let mut failures = 0u32;
    let mut burst_receipts = 0;
    let burst = allocated
        && wait(|| {
            let _r = RuntimeGuard::acquire();
            quiet(cpu)
        })
        && {
            let _r = RuntimeGuard::acquire();
            let before = reschedule::snapshot(cpu);
            let admitted = quiet(cpu) && actors[..4].iter().all(|&t| tasks::start(t));
            let after = reschedule::snapshot(cpu);
            burst_receipts = before.consumed;
            admitted
                && after.sent == before.sent + 1
                && after.merged == before.merged + 3
                && after.failed == before.failed
                && after.in_flight
        };
    if !burst {
        failures |= 1;
    }
    let armed = burst
        && wait(|| {
            let _r = RuntimeGuard::acquire();
            let status = reschedule::snapshot(cpu);
            ARMED.load(Ordering::Acquire)
                && cpu_state(cpu).current == actors[0].cast()
                && !status.in_flight
                && status.consumed > burst_receipts
        });
    if !armed {
        failures |= 2;
    }
    // Equal-ranked Batch work on this productive CPU needs no IPI.
    let nonurgent = armed && {
        let _r = RuntimeGuard::acquire();
        let before = reschedule::snapshot(cpu);
        let admitted = cpu_state(cpu).current == actors[0].cast() && tasks::start(actors[6]);
        let after = reschedule::snapshot(cpu);
        admitted
            && after.sent == before.sent
            && after.merged == before.merged
            && after.failed == before.failed
    };
    if !nonurgent {
        failures |= 4;
    }
    let mut prior_receipts = 0;
    let first = nonurgent && {
        let _r = RuntimeGuard::acquire();
        let before = reschedule::snapshot(cpu);
        let admitted = !before.in_flight
            && cpu_state(cpu).current == actors[0].cast()
            && tasks::start(actors[4]);
        let after = reschedule::snapshot(cpu);
        prior_receipts = before.consumed;
        admitted
            && cpu_state(cpu).pending
            && after.sent == before.sent + 1
            && after.failed == before.failed
    };
    if !first {
        failures |= 8;
    }
    let consumed = first
        && wait(|| {
            let _r = RuntimeGuard::acquire();
            let status = reschedule::snapshot(cpu);
            status.consumed > prior_receipts
                && !status.in_flight
                && cpu_state(cpu).current == actors[0].cast()
                && cpu_state(cpu).pending
        });
    // A second urgent enqueue after real IRQ consumption must send again,
    // even though the earlier scheduling pending flag remains true.
    let second = consumed && {
        let _r = RuntimeGuard::acquire();
        let before = reschedule::snapshot(cpu);
        let admitted = !before.in_flight
            && cpu_state(cpu).pending
            && cpu_state(cpu).current == actors[0].cast()
            && tasks::start(actors[5]);
        let after = reschedule::snapshot(cpu);
        prior_receipts = before.consumed;
        admitted && after.sent == before.sent + 1 && after.failed == before.failed
    };
    if !second {
        failures |= 16;
    }
    let second_consumed = second && wait(|| reschedule::snapshot(cpu).consumed > prior_receipts);
    if !second_consumed {
        failures |= 32;
    }
    GO.store(true, Ordering::Release);
    STOP.store(true, Ordering::Release);
    let finished = second_consumed && wait(|| DONE.load(Ordering::Acquire) == 7);
    if !finished {
        failures |= 64;
    }
    let mut clean = true;
    for &actor in &actors[..7] {
        if !actor.is_null() {
            clean &= tasks::finish_task(actor, failures != 0);
        }
    }
    let idle = finished
        && clean
        && wait(|| {
            let _r = RuntimeGuard::acquire();
            quiet(cpu)
        });
    let rearmed = idle && {
        let _r = RuntimeGuard::acquire();
        let before = reschedule::snapshot(cpu);
        let admitted = quiet(cpu) && tasks::start(actors[7]);
        let after = reschedule::snapshot(cpu);
        prior_receipts = before.consumed;
        admitted && after.sent == before.sent + 1 && after.failed == before.failed
    };
    let rearm_consumed = rearmed
        && wait(|| {
            DONE.load(Ordering::Acquire) == 8 && reschedule::snapshot(cpu).consumed > prior_receipts
        });
    if !rearm_consumed {
        failures |= 128;
    }
    if !actors[7].is_null() {
        clean &= tasks::finish_task(actors[7], failures != 0);
    }
    let ok = failures == 0 && clean;
    crate::boot::log_route::puts(if ok {
        b"IPI08734 PASS burst=4 sends=1 merges=3 nonurgent=0 pending-independent=1 idle-rearm=1 tasks=8 own=reaped\n"
    } else {
        b"IPI08734 FAILED mask="
    });
    if !ok {
        crate::boot::log_route::dec(u64::from(failures));
        crate::boot::log_route::puts(b"\n");
    }
    ok
}
