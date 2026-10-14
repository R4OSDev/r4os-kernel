//! Original owner-role, directed FIFO, bounded donation/fairness and wake tests.
use super::{helpers::*, owner_snapshots, state::*};
use crate::sched::{
    scheduler,
    scheduler_types::Stats,
    sync_locks, sync_objects,
    task::task_types::{Role, State, Task},
};
use core::ptr;
unsafe fn has_role(id: u32, role: Role) -> bool {
    unsafe {
        if id == 0 {
            return false;
        }
        let t = identity(1, id, 0) as *mut Task;
        if t.is_null() {
            return false;
        }
        let result = view(t).role == role;
        unpin(t);
        result
    }
}
pub unsafe fn owner_roles() -> bool {
    unsafe {
        let o = owner_snapshots::owners();
        let a = o.ipc.started != 0 && has_role(o.ipc.id, Role::ShortCompletion);
        let b = o.work.started != 0 && has_role(o.work.id, Role::ShortCompletion);
        let c = o.block.started != 0 && has_role(o.block.id, Role::Batch);
        let d = o.net.started != 0 && has_role(o.net.id, Role::ShortCompletion);
        let e = if o.input.started != 0 {
            has_role(o.input.id, Role::Input)
        } else {
            !o.input_bound
        };
        if !(a && b && c && d && e) {
            puts(b"TASKREG06933 role-owners");
            for (label, ok) in [
                (b" ipc=".as_slice(), a),
                (b" work=".as_slice(), b),
                (b" block=".as_slice(), c),
                (b" net=".as_slice(), d),
                (b" input=".as_slice(), e),
            ] {
                puts(label);
                puts(if ok { b"OK" } else { b"FAIL" });
            }
            puts(if o.input.started != 0 {
                b"/started\r\n"
            } else {
                b"/absent\r\n"
            });
        }
        a && b && c && d && e
    }
}
unsafe extern "C" fn directed_main() {
    unsafe {
        let t = current();
        if t.is_null() || !(0..DIRECTED).any(|i| (*raw()).directed_tasks[i] == t) {
            (*raw()).directed_failures = (*raw()).directed_failures.wrapping_add(1);
            return;
        }
        if wait(
            ptr::addr_of_mut!((*raw()).directed_queue),
            u64::MAX,
            b"taskreg-role-directed",
        ) == 1
        {
            (*raw()).directed_results = (*raw()).directed_results.wrapping_add(1);
        } else {
            (*raw()).directed_failures = (*raw()).directed_failures.wrapping_add(1);
        }
    }
}
unsafe fn clean_directed() -> bool {
    unsafe {
        queue(ptr::addr_of_mut!((*raw()).directed_queue), 3);
        let mut ok = true;
        for i in 0..DIRECTED {
            let t = (*raw()).directed_tasks[i];
            if t.is_null() {
                continue;
            }
            let id = (*raw()).directed_ids[i];
            let generation = (*raw()).directed_generations[i];
            if alive(id, generation) && !kill(id, generation) {
                ok = false;
            }
            if !unpin(t) {
                ok = false;
            }
            if !release(id, generation) {
                ok = false;
            }
            (*raw()).directed_tasks[i] = ptr::null_mut();
        }
        r4os_rust_task_control(16, 0, 0);
        ok
    }
}
pub unsafe fn directed() -> bool {
    unsafe {
        (*raw()).directed_queue = crate::sched::sync_types::WaitQueue::EMPTY;
        (*raw()).directed_results = 0;
        (*raw()).directed_failures = 0;
        let before = wait_summary();
        let roles = [
            Role::Batch,
            Role::Interactive,
            Role::ShortCompletion,
            Role::Input,
        ];
        for (i, role) in roles.into_iter().enumerate() {
            let t = create(
                b"taskreg-role-directed",
                directed_main,
                State::Blocked,
                role,
            );
            if t.is_null() {
                clean_directed();
                return false;
            }
            if !pin(t) {
                let v = view(t);
                retire(v.id, v.generation);
                clean_directed();
                return false;
            }
            let v = view(t);
            (*raw()).directed_tasks[i] = t;
            (*raw()).directed_ids[i] = v.id;
            (*raw()).directed_generations[i] = v.generation;
            mutate(0, t, now(), 0);
            let mut spins = 0;
            while (view(t).state != State::Blocked
                || !reason_is(view(t).reason, b"taskreg-role-directed"))
                && spins < WAIT_BUDGET
            {
                spins += 1;
                yield_task();
            }
            if view(t).state != State::Blocked {
                clean_directed();
                return false;
            }
        }
        for i in [3, 2, 1, 0] {
            if queue(ptr::addr_of_mut!((*raw()).directed_queue), 0) != (*raw()).directed_ids[i] {
                clean_directed();
                return false;
            }
        }
        let safe_before = structure();
        let switched = scheduler::r4os_rust_scheduler_control(11, 0, 0) != 0;
        let safe_after = structure();
        let mut spins = 0;
        while (*raw()).directed_results + (*raw()).directed_failures < DIRECTED as u32
            && spins < WAIT_BUDGET
        {
            spins += 1;
            yield_task();
        }
        let after = wait_summary();
        let ok = (*raw()).directed_results == DIRECTED as u32
            && (*raw()).directed_failures == 0
            && after.directed_wakes >= before.directed_wakes + DIRECTED as u64
            && after.directed_fifo_bypasses >= before.directed_fifo_bypasses + 3
            && after.directed_scans >= before.directed_scans + DIRECTED as u64
            && switched
            && safe_after.safe_reschedule_switches > safe_before.safe_reschedule_switches
            && queue_valid();
        let cleaned = clean_directed();
        ok && cleaned
    }
}
unsafe extern "C" fn budget_main() {
    unsafe {
        let t = current();
        if t.is_null() {
            return;
        }
        let Some(index) = (0..BUDGET).find(|&i| (*raw()).budget_tasks[i] == t) else {
            return;
        };
        while !(*raw()).budget_stop {
            (*raw()).budget_turns[index] = (*raw()).budget_turns[index].wrapping_add(1);
            if index == 0
                && (*raw()).budget_turns[0] >= INPUT_TURNS
                && (*raw()).budget_turns[1] != 0
                && (*raw()).budget_turns[2] != 0
            {
                (*raw()).budget_stop = true;
            }
            yield_task();
        }
    }
}
unsafe fn clean_budget() -> bool {
    unsafe {
        (*raw()).budget_stop = true;
        let mut ok = true;
        for i in 0..BUDGET {
            let t = (*raw()).budget_tasks[i];
            if t.is_null() {
                continue;
            }
            let id = (*raw()).budget_ids[i];
            let generation = (*raw()).budget_generations[i];
            if alive(id, generation) && !kill(id, generation) {
                ok = false;
            }
            if !unpin(t) {
                ok = false;
            }
            if !release(id, generation) {
                ok = false;
            }
            (*raw()).budget_tasks[i] = ptr::null_mut();
        }
        r4os_rust_task_control(16, 0, 0);
        ok
    }
}
pub unsafe fn fairness() -> bool {
    unsafe {
        (*raw()).budget_turns = [0; BUDGET];
        (*raw()).budget_stop = false;
        let before = role_stats();
        for (i, role) in [Role::Input, Role::Interactive, Role::Batch]
            .into_iter()
            .enumerate()
        {
            let t = create(b"taskreg-role-budget", budget_main, State::Blocked, role);
            if t.is_null() {
                clean_budget();
                return false;
            }
            if !pin(t) {
                let v = view(t);
                retire(v.id, v.generation);
                clean_budget();
                return false;
            }
            let v = view(t);
            (*raw()).budget_tasks[i] = t;
            (*raw()).budget_ids[i] = v.id;
            (*raw()).budget_generations[i] = v.generation;
            mutate(0, t, now(), 0);
        }
        let mut spins = 0;
        while (!(*raw()).budget_stop
            || view((*raw()).budget_tasks[0]).state != State::Dead
            || view((*raw()).budget_tasks[1]).state != State::Dead
            || view((*raw()).budget_tasks[2]).state != State::Dead)
            && spins < WAIT_BUDGET
        {
            spins += 1;
            yield_task();
        }
        let after = role_stats();
        let ok = (*raw()).budget_stop
            && (*raw()).budget_turns[0] >= INPUT_TURNS
            && (*raw()).budget_turns[1] != 0
            && (*raw()).budget_turns[2] != 0
            && view((*raw()).budget_tasks[0]).role_exhaustions != 0
            && after.budget_exhaustions[0] > before.budget_exhaustions[0]
            && queue_valid();
        let cleaned = clean_budget();
        ok && cleaned
    }
}
unsafe extern "C" fn inversion_holder() {
    unsafe {
        let m = ptr::addr_of_mut!((*raw()).inversion_mutex);
        if !sync_locks::r4os_rust_sync_mutex(m, 1, u64::MAX) {
            return;
        }
        (*raw()).inversion_owned = true;
        while !(*raw()).inversion_release {
            yield_task();
        }
        if !sync_locks::r4os_rust_sync_mutex(m, 2, 0) {
            return;
        }
        (*raw()).inversion_holder_done = true;
    }
}
unsafe extern "C" fn inversion_waiter() {
    unsafe {
        let m = ptr::addr_of_mut!((*raw()).inversion_mutex);
        if sync_locks::r4os_rust_sync_mutex(m, 1, 200) {
            (*raw()).inversion_acquired = true;
            sync_locks::r4os_rust_sync_mutex(m, 2, 0);
        }
        (*raw()).inversion_waiter_done = true;
    }
}
unsafe fn clean_inversion(holder: *mut Task, waiter: *mut Task) -> bool {
    unsafe {
        (*raw()).inversion_release = true;
        let mut ok = true;
        for t in [holder, waiter] {
            if t.is_null() {
                continue;
            }
            let v = view(t);
            if alive(v.id, v.generation) && !kill(v.id, v.generation) {
                ok = false;
            }
            if !unpin(t) {
                ok = false;
            }
            if !release(v.id, v.generation) {
                ok = false;
            }
        }
        r4os_rust_task_control(16, 0, 0);
        ok
    }
}
pub unsafe fn inversion() -> bool {
    unsafe {
        (*raw()).inversion_mutex = super::state::Probe::EMPTY.inversion_mutex;
        (*raw()).inversion_owned = false;
        (*raw()).inversion_release = false;
        (*raw()).inversion_holder_done = false;
        (*raw()).inversion_waiter_done = false;
        (*raw()).inversion_acquired = false;
        let before = lock_stats();
        let holder = create(
            b"taskreg-role-holder",
            inversion_holder,
            State::Blocked,
            Role::Batch,
        );
        if holder.is_null() {
            return false;
        }
        if !pin(holder) {
            let v = view(holder);
            retire(v.id, v.generation);
            return false;
        }
        mutate(0, holder, now(), 0);
        let mut spins = 0;
        while !(*raw()).inversion_owned && spins < WAIT_BUDGET {
            spins += 1;
            yield_task();
        }
        if !(*raw()).inversion_owned {
            clean_inversion(holder, ptr::null_mut());
            return false;
        }
        let waiter = create(
            b"taskreg-role-waiter",
            inversion_waiter,
            State::Blocked,
            Role::Input,
        );
        if waiter.is_null() {
            clean_inversion(holder, ptr::null_mut());
            return false;
        }
        if !pin(waiter) {
            let v = view(waiter);
            retire(v.id, v.generation);
            clean_inversion(holder, ptr::null_mut());
            return false;
        }
        mutate(0, waiter, now(), 0);
        spins = 0;
        while (view(waiter).state != State::Blocked || !reason_is(view(waiter).reason, b"mutex"))
            && spins < WAIT_BUDGET
        {
            spins += 1;
            yield_task();
        }
        if view(waiter).state != State::Blocked {
            clean_inversion(holder, waiter);
            return false;
        }
        (*raw()).inversion_release = true;
        spins = 0;
        while (!(*raw()).inversion_holder_done || !(*raw()).inversion_waiter_done)
            && spins < WAIT_BUDGET
        {
            spins += 1;
            yield_task();
        }
        let after = lock_stats();
        let m = ptr::addr_of!((*raw()).inversion_mutex);
        let ok = (*raw()).inversion_holder_done
            && (*raw()).inversion_waiter_done
            && (*raw()).inversion_acquired
            && (*m).owner == 0
            && (*m).owner_generation == 0
            && (*m).depth == 0
            && (*m).donation_rank == 255
            && after.role_inversions > before.role_inversions
            && after.role_donations > before.role_donations
            && after.role_donation_releases > before.role_donation_releases
            && queue_valid();
        let cleaned = clean_inversion(holder, waiter);
        ok && cleaned
    }
}
pub fn latency_distributions() -> bool {
    let mut stats = Stats::EMPTY;
    unsafe { scheduler::r4os_rust_scheduler_stats(&mut stats) };
    for (samples, buckets) in stats
        .ready_latency_role_samples
        .into_iter()
        .zip(stats.ready_latency_role_buckets)
    {
        if samples == 0 {
            return false;
        }
        let total = buckets.into_iter().fold(0u64, |sum, n| sum.wrapping_add(n));
        if total != samples {
            return false;
        }
    }
    true
}
unsafe extern "C" fn wake_main() {
    unsafe {
        (*raw()).wake_entered = true;
        if sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*raw()).wake_event), 2, u64::MAX)
            == 1
        {
            (*raw()).wake_completed = true;
        }
    }
}
pub unsafe fn wake_probe() -> bool {
    unsafe {
        let caller = current();
        if caller.is_null() {
            return false;
        }
        let old_role = view(caller).role;
        let probe = create(
            b"taskreg-wakeup",
            wake_main,
            State::Blocked,
            Role::Interactive,
        );
        if probe.is_null() {
            return false;
        }
        let v = view(probe);
        if mutate(16, probe, 0, 0) == 0 {
            retire(v.id, v.generation);
            return false;
        }
        mutate(0, probe, now(), 0);
        let mut spins = 0;
        while (!(*raw()).wake_entered || view(probe).state != State::Blocked) && spins < WAIT_BUDGET
        {
            spins += 1;
            yield_task();
        }
        if !(*raw()).wake_entered
            || view(probe).state != State::Blocked
            || mutate(16, caller, Role::Interactive as u64, 0) == 0
        {
            if alive(v.id, v.generation) {
                kill(v.id, v.generation);
            }
            retire(v.id, v.generation);
            return false;
        }
        let before = structure();
        sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*raw()).wake_event), 0, 0);
        let after_signal = structure();
        let published = current() == caller
            && after_signal.wakeup_reschedule_requests > before.wakeup_reschedule_requests
            && after_signal.reschedule_pending;
        let switched = scheduler::r4os_rust_scheduler_control(11, 0, 0) != 0;
        let after_safe = structure();
        spins = 0;
        while !(*raw()).wake_completed && spins < WAIT_BUDGET {
            spins += 1;
            yield_task();
        }
        (*raw()).wake_latency = view(probe).ready_latency;
        mutate(16, caller, old_role as u64, 0);
        let bounded = (*raw()).wake_completed && (*raw()).wake_latency <= WAKE_BAR;
        if alive(v.id, v.generation) {
            kill(v.id, v.generation);
        }
        let retired = retire(v.id, v.generation);
        published
            && switched
            && after_safe.safe_reschedule_switches > after_signal.safe_reschedule_switches
            && bounded
            && retired != 1
    }
}
