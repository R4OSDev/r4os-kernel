//! Original finite checkpoint/earlier timer and 65-task bounded timeout cases.
use super::{helpers::*, state::*, workers};
use crate::{
    arch::{hpet, lapic, runtime},
    platform::{timer, timer_arm::Stamp},
    sched::{
        scheduler,
        task::task_types::{Role, State, Task},
    },
};
use core::ptr;
const TIMER_OBJECT: u64 = 0x5449_4d45_5741_4954;
fn physical_stamp() -> Option<Stamp> {
    match timer::active_backend() {
        1 => hpet::idle_arm_stamp(),
        2 => lapic::idle_arm_stamp(),
        _ => None,
    }
}
// Caller holds the existing diagnostic Runtime and preemption boundary.
unsafe fn retention_turns() -> (bool, bool, bool, bool) {
    unsafe {
        let mut ok = timer::r4os_rust_timer_control(12, u64::MAX - 1, 0) != 0;
        let long = timer::deadline_stats();
        (*raw()).deadline_checkpointed =
            long.armed_deadline != u64::MAX && long.armed_deadline < u64::MAX - 1;
        let finite_stamp = physical_stamp();
        let finite_width_known = timer::active_backend() != 1 || hpet::idle_counter_wide();
        for _ in 0..4 {
            ok = (timer::r4os_rust_timer_control(12, u64::MAX - 1, 0) != 0) && ok;
        }
        let retained_finite = !finite_width_known
            || finite_stamp.is_some()
                && physical_stamp() == finite_stamp
                && timer::deadline_stats().one_shot_arms == long.one_shot_arms;
        ok = (timer::r4os_rust_timer_control(13, 0, 0) != 0) && ok;
        // Preserve the original long-arm cancellation oracle above. This
        // separate empty turn proves actual disarm identity without telemetry.
        ok = (timer::r4os_rust_timer_control(12, u64::MAX, 0) != 0) && ok;
        let empty_stamp = physical_stamp();
        for _ in 0..4 {
            ok = (timer::r4os_rust_timer_control(12, u64::MAX, 0) != 0) && ok;
        }
        let retained_empty = empty_stamp.is_some() && physical_stamp() == empty_stamp;
        ok = (timer::r4os_rust_timer_control(13, 0, 0) != 0) && ok;
        (ok, retained_finite, retained_empty, finite_width_known)
    }
}

pub unsafe fn probe() -> bool {
    unsafe {
        let before = timer::deadline_stats();
        if before.flags & 3 != 3 || timer::active_backend() == 0 {
            return false;
        }
        let flags = runtime::save_runtime();
        scheduler::r4os_rust_scheduler_control(6, 0, 0);
        let (mut ok, retained_finite, retained_empty, finite_width_known) = retention_turns();
        let start = now();
        let long_deadline = timer::r4os_rust_timer_control(6, start, LONG_TICKS);
        let early = timer::r4os_rust_timer_control(6, start, EARLY_TICKS);
        ok = (timer::r4os_rust_timer_control(12, long_deadline, 0) != 0) && ok;
        ok = (timer::r4os_rust_timer_control(12, early, 0) != 0) && ok;
        let irq_before = timer::deadline_stats().timer_irqs;
        let one_shot_before = timer::r4os_rust_timer_control(16, 0, 0);
        runtime::restore_runtime(flags);
        // Keep the original real-IRQ/lateness bounds, but isolate hardware
        // delivery from a task switch. Sleep can legally finish through the
        // BSP's due-wait poll after a different IRQ/IPI; restoring productive
        // work then cancels its idle timer before that IRQ is delivered.
        // No runtime owner spans this bounded IF-on hardware interval.
        let mut observed_irq = one_shot_before;
        for _ in 0..1_000_000 {
            let irq = timer::r4os_rust_timer_control(16, 0, 0);
            let current = now();
            if irq > one_shot_before && timer::observed_last_irq_tick() >= early
                || current.saturating_sub(start) >= EARLY_TICKS + LATENESS_BAR
            {
                break;
            }
            if irq > observed_irq && current < early {
                // An old/early count vector is not completion of this due.
                // Exercise the normal idle readmission with the SAME absolute
                // target: retain live hardware or rearm consumed hardware.
                ok = (timer::r4os_rust_timer_control(12, early, 0) != 0) && ok;
            }
            observed_irq = irq;
            core::hint::spin_loop();
        }
        let finish = runtime::save_runtime();
        let armed_end = timer::deadline_stats();
        let one_shot_end = timer::r4os_rust_timer_control(16, 0, 0);
        let elapsed = now() - start;
        ok = (timer::r4os_rust_timer_control(13, 0, 0) != 0) && ok;
        scheduler::r4os_rust_scheduler_control(7, 0, 0);
        runtime::restore_runtime(finish);
        let sleep_start = now();
        sleep(EARLY_TICKS, b"taskreg-deadline-probe");
        let sleep_elapsed = now().saturating_sub(sleep_start);
        let after = timer::deadline_stats();
        (*raw()).deadline_elapsed = elapsed;
        (*raw()).deadline_total_irqs = armed_end.timer_irqs - irq_before;
        (*raw()).deadline_irqs = one_shot_end - one_shot_before;
        (*raw()).deadline_lateness = armed_end.last_lateness_ticks;
        let passed = ok
            && retained_finite
            && retained_empty
            && (*raw()).deadline_checkpointed
            && after.mode == 0
            && after.earlier_reprograms > before.earlier_reprograms
            && after.cancels > before.cancels
            && after.periodic_resumes >= before.periodic_resumes + 2
            && after.runtime_fallbacks == before.runtime_fallbacks
            && elapsed >= EARLY_TICKS
            && sleep_elapsed >= EARLY_TICKS
            && (*raw()).deadline_irqs >= 1
            && (*raw()).deadline_irqs <= IRQ_BAR
            && armed_end.last_irq_tick >= early
            && armed_end.last_lateness_ticks <= LATENESS_BAR;
        if !passed {
            puts(b"TASKREG06932 deadline-failed ok=");
            dec(u64::from(ok));
            puts(b" checkpoint=");
            dec(u64::from((*raw()).deadline_checkpointed));
            puts(b" retained_finite=");
            dec(u64::from(retained_finite));
            puts(b" retained_empty=");
            dec(u64::from(retained_empty));
            puts(b" elapsed=");
            dec(elapsed);
            puts(b" sleep_elapsed=");
            dec(sleep_elapsed);
            puts(b" irqs=");
            dec((*raw()).deadline_irqs);
            puts(b" one_shot_irqs=");
            dec((*raw()).deadline_irqs);
            puts(b" outside_irqs=");
            dec(after.timer_irqs - before.timer_irqs - (*raw()).deadline_total_irqs);
            puts(b" lateness=");
            dec(armed_end.last_lateness_ticks);
            puts(b" last_irq=");
            dec(armed_end.last_irq_tick);
            puts(b" expected_due=");
            dec(early);
            puts(b" earlier=");
            dec(after.earlier_reprograms - before.earlier_reprograms);
            puts(b" cancels=");
            dec(after.cancels - before.cancels);
            puts(b" resumes=");
            dec(after.periodic_resumes - before.periodic_resumes);
            puts(b" fallback=");
            dec(after.runtime_fallbacks - before.runtime_fallbacks);
            puts(b" idle_entries=");
            dec(after.idle_entries - before.idle_entries);
            puts(b" arms=");
            dec(after.one_shot_arms - before.one_shot_arms);
            puts(b" periodic=");
            dec(u64::from(after.mode == 0));
            puts(b"\r\n");
        } else {
            puts(b"TIMER08864 retained PASS finite=");
            puts(if finite_width_known {
                b"4"
            } else {
                b"unavailable-narrow-main"
            });
            puts(b" empty=4 physical-revision=unchanged backend=");
            dec(u64::from(timer::active_backend()));
            puts(b" finite-width-qualified=");
            dec(u64::from(finite_width_known));
            puts(b" checkpoint=finite earlier=real-irq sleep=real cancellation=preserved\r\n");
        }
        passed
    }
}
/// Explicit existing boot selector only. Retention uses the same checked
/// physical turns as probe; actual wake suspends the productive controller.
/// No raw busy-thread count-IRQ limit is reinterpreted as an idle workload.
pub unsafe fn retained_idle() -> bool {
    unsafe {
        let before = timer::deadline_stats();
        if before.flags & 3 != 3 || timer::active_backend() == 0 {
            return false;
        }
        let flags = runtime::save_runtime();
        scheduler::r4os_rust_scheduler_control(6, 0, 0);
        let (ok, finite, empty, width) = retention_turns();
        scheduler::r4os_rust_scheduler_control(7, 0, 0);
        runtime::restore_runtime(flags);
        let start = now();
        sleep(EARLY_TICKS, b"taskreg-retained-idle");
        let elapsed = now().saturating_sub(start);
        let after = timer::deadline_stats();
        let passed = ok
            && finite
            && empty
            && (*raw()).deadline_checkpointed
            && after.mode == 0
            && after.cancels > before.cancels
            && after.periodic_resumes >= before.periodic_resumes + 2
            && after.runtime_fallbacks == before.runtime_fallbacks
            && elapsed >= EARLY_TICKS
            && after.last_irq_tick >= start.saturating_add(EARLY_TICKS);
        if !passed {
            puts(b"TIMER08864 retained-idle FAIL finite=");
            dec(u64::from(finite));
            puts(b" empty=");
            dec(u64::from(empty));
            puts(b" elapsed=");
            dec(elapsed);
            puts(b" last_irq=");
            dec(after.last_irq_tick);
            puts(b" due=");
            dec(start.saturating_add(EARLY_TICKS));
            puts(b"\r\n");
            return false;
        }
        puts(b"TIMER08864 retained PASS finite=");
        puts(if width {
            b"4"
        } else {
            b"unavailable-narrow-main"
        });
        puts(b" empty=4 physical-revision=unchanged backend=");
        dec(u64::from(timer::active_backend()));
        puts(b" finite-width-qualified=");
        dec(u64::from(width));
        puts(b" checkpoint=finite sleep=real cancellation=preserved scope=idle-retention\r\n");
        true
    }
}

unsafe fn begin_wait(t: *mut Task, tick: u64, reason: &[u8]) {
    unsafe {
        // The real native wait mutator keeps the exact timeout sequence/tree.
        unsafe extern "C" {
            fn r4os_rust_task_mutate(
                op: u8,
                t: *mut core::ffi::c_void,
                a: u64,
                b: u64,
                text: *const u8,
                len: usize,
            ) -> u64;
        }
        r4os_rust_task_mutate(
            13,
            t.cast(),
            tick,
            TIMER_OBJECT,
            reason.as_ptr(),
            reason.len(),
        );
    }
}
unsafe fn first() -> *mut Task {
    unsafe { r4os_rust_task_control(27, 0, 0) as *mut Task }
}
unsafe fn clean(probes: &mut [*mut Task; STORM]) -> bool {
    unsafe {
        let mut ok = true;
        for t in probes.iter_mut() {
            if t.is_null() {
                continue;
            }
            let v = view(*t);
            if alive(v.id, v.generation) && !kill(v.id, v.generation) {
                ok = false;
            }
            if !unpin(*t) {
                ok = false;
            }
            if !release(v.id, v.generation) {
                ok = false;
            }
            *t = ptr::null_mut();
        }
        drain_resources();
        ok
    }
}
pub(super) unsafe fn allocate(
    probes: &mut [*mut Task; STORM],
    entry: crate::sched::task::task_types::Entry,
) -> bool {
    unsafe {
        for i in 0..STORM {
            let t = create(
                b"taskreg-deadline-storm",
                entry,
                State::Blocked,
                Role::Interactive,
            );
            if t.is_null() {
                clean(probes);
                return false;
            }
            if !pin(t) {
                let v = view(t);
                retire(v.id, v.generation);
                clean(probes);
                return false;
            }
            probes[i] = t;
        }
        true
    }
}
pub unsafe fn storm() -> bool {
    unsafe {
        let mut probes = [ptr::null_mut(); STORM];
        if !allocate(&mut probes, workers::short_main) {
            return false;
        }
        let flags = runtime::save_runtime();
        for t in probes {
            begin_wait(t, 1, b"taskreg-deadline-storm");
        }
        let enrolled = first() == probes[0] && queue_valid();
        let before = structure();
        scheduler::r4os_rust_scheduler_tick(now(), false, false);
        let fifo = first() == probes[STORM - 1];
        let ready = probes[..STORM - 1]
            .iter()
            .filter(|&&t| view(t).state == State::Ready)
            .count();
        let blocked = view(probes[STORM - 1]).state == State::Blocked;
        let deferred = structure().timeout_storm_deferrals == before.timeout_storm_deferrals + 1;
        let irq_before = timer::deadline_stats().timer_irqs;
        let idle_tail = scheduler::r4os_rust_scheduler_control(14, 0, 0) != 0;
        let drained = view(probes[STORM - 1]).state == State::Ready;
        let tail = probes[STORM - 1];
        begin_wait(
            tail,
            timer::r4os_rust_timer_control(7, 1000, 0),
            b"taskreg-idle-future",
        );
        let future_due = scheduler::r4os_rust_scheduler_control(14, 0, 0) != 0;
        let future_held = view(tail).state == State::Blocked;
        begin_wait(tail, 1, b"taskreg-idle-due");
        let final_tail = scheduler::r4os_rust_scheduler_control(14, 0, 0) != 0;
        let woken =
            view(tail).state == State::Ready && timer::deadline_stats().timer_irqs == irq_before;
        let valid = queue_valid();
        runtime::restore_runtime(flags);
        let bounded = enrolled
            && fifo
            && ready == STORM - 1
            && blocked
            && deferred
            && drained
            && !idle_tail
            && !future_due
            && future_held
            && !final_tail
            && woken
            && valid;
        if !bounded {
            puts(b"TASKREG06932 storm-failed enrolled=");
            dec(u64::from(enrolled));
            puts(b" fifo_head=");
            dec(u64::from(fifo));
            puts(b" ready_first=");
            dec(ready as u64);
            puts(b" tail_blocked=");
            dec(u64::from(blocked));
            puts(b" deferral=");
            dec(u64::from(deferred));
            puts(b" drained=");
            dec(u64::from(drained));
            puts(b" projection=");
            dec(u64::from(valid));
            puts(b"\r\n");
        }
        (*raw()).storm_batches = 2;
        let cleaned = clean(&mut probes);
        bounded && cleaned
    }
}
