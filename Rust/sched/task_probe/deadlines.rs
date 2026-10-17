//! Original finite checkpoint/earlier timer and 65-task bounded timeout cases.
use super::{helpers::*, state::*, workers};
use crate::{
    arch::runtime,
    platform::timer,
    sched::{
        scheduler,
        task::task_types::{Role, State, Task},
    },
};
use core::ptr;
const TIMER_OBJECT: u64 = 0x5449_4d45_5741_4954;
pub unsafe fn probe() -> bool {
    unsafe {
        let before = timer::deadline_stats();
        if before.flags & 3 != 3 || timer::active_backend() == 0 {
            return false;
        }
        let flags = runtime::save_runtime();
        scheduler::r4os_rust_scheduler_control(6, 0, 0);
        let mut ok = timer::r4os_rust_timer_control(12, u64::MAX - 1, 0) != 0;
        let long = timer::deadline_stats();
        (*raw()).deadline_checkpointed =
            long.armed_deadline != u64::MAX && long.armed_deadline < u64::MAX - 1;
        ok = (timer::r4os_rust_timer_control(13, 0, 0) != 0) && ok;
        let start = now();
        let long_deadline = timer::r4os_rust_timer_control(6, start, LONG_TICKS);
        let early = timer::r4os_rust_timer_control(6, start, EARLY_TICKS);
        ok = (timer::r4os_rust_timer_control(12, long_deadline, 0) != 0) && ok;
        ok = (timer::r4os_rust_timer_control(12, early, 0) != 0) && ok;
        let irq_before = timer::deadline_stats().timer_irqs;
        let one_shot_before = timer::r4os_rust_timer_control(16, 0, 0);
        scheduler::r4os_rust_scheduler_control(7, 0, 0);
        runtime::restore_runtime(flags);
        sleep(EARLY_TICKS, b"taskreg-deadline-probe");
        let finish = runtime::save_runtime();
        let armed_end = timer::deadline_stats();
        let one_shot_end = timer::r4os_rust_timer_control(16, 0, 0);
        let elapsed = now() - start;
        ok = (timer::r4os_rust_timer_control(13, 0, 0) != 0) && ok;
        runtime::restore_runtime(finish);
        let after = timer::deadline_stats();
        (*raw()).deadline_elapsed = elapsed;
        (*raw()).deadline_total_irqs = armed_end.timer_irqs - irq_before;
        (*raw()).deadline_irqs = one_shot_end - one_shot_before;
        (*raw()).deadline_lateness = armed_end.last_lateness_ticks;
        let passed = ok
            && (*raw()).deadline_checkpointed
            && after.mode == 0
            && after.earlier_reprograms > before.earlier_reprograms
            && after.cancels > before.cancels
            && after.periodic_resumes >= before.periodic_resumes + 2
            && after.runtime_fallbacks == before.runtime_fallbacks
            && elapsed >= EARLY_TICKS
            && (*raw()).deadline_irqs >= 1
            && (*raw()).deadline_irqs <= IRQ_BAR
            && armed_end.last_lateness_ticks <= LATENESS_BAR;
        if !passed {
            puts(b"TASKREG06932 deadline-failed ok=");
            dec(u64::from(ok));
            puts(b" checkpoint=");
            dec(u64::from((*raw()).deadline_checkpointed));
            puts(b" elapsed=");
            dec(elapsed);
            puts(b" irqs=");
            dec((*raw()).deadline_irqs);
            puts(b" one_shot_irqs=");
            dec((*raw()).deadline_irqs);
            puts(b" outside_irqs=");
            dec(after.timer_irqs - before.timer_irqs - (*raw()).deadline_total_irqs);
            puts(b" lateness=");
            dec(armed_end.last_lateness_ticks);
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
        }
        passed
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
