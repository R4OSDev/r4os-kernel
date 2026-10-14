//! Original 160-worker mix and exact generation-retaining 10,000-cycle churn.
use super::{helpers::*, owner_snapshots::Baseline, state::*};
use crate::sched::{
    sync_objects,
    task::task_types::{Role, State},
};
use core::ptr;
pub unsafe fn create_workers() -> bool {
    unsafe {
        for i in 0..WORKERS {
            let t = create(
                b"taskreg-worker",
                worker_main,
                State::Blocked,
                Role::Interactive,
            );
            if t.is_null() {
                return false;
            }
            if !pin(t) {
                let v = view(t);
                kill(v.id, v.generation);
                release(v.id, v.generation);
                return false;
            }
            let role = (i % 4) as u64;
            if mutate(16, t, role, 0) == 0 {
                return false;
            }
            let v = view(t);
            (*raw()).tasks[i] = t;
            (*raw()).ids[i] = v.id;
            (*raw()).generations[i] = v.generation;
            mutate(0, t, now(), 0);
        }
        true
    }
}
unsafe extern "C" fn worker_main() {
    unsafe {
        let t = current();
        if t.is_null() {
            (*raw()).failures = (*raw()).failures.wrapping_add(1);
            return;
        }
        let Some(index) = (0..WORKERS).find(|&i| (*raw()).tasks[i] == t) else {
            (*raw()).failures = (*raw()).failures.wrapping_add(1);
            return;
        };
        (*raw()).started = (*raw()).started.wrapping_add(1);
        match kind(index) {
            Kind::Runnable => {
                while !(*raw()).release_workers {
                    yield_task();
                }
            }
            Kind::Sleeping => {
                while !(*raw()).release_workers {
                    sleep(2, b"taskreg-sleep");
                }
            }
            Kind::Signal => {
                let q = ptr::addr_of_mut!((*raw()).signal_event);
                if sync_objects::r4os_rust_sync_event(q, 2, u64::MAX) == 1 {
                    (*raw()).signal_results = (*raw()).signal_results.wrapping_add(1);
                } else {
                    (*raw()).failures = (*raw()).failures.wrapping_add(1);
                }
                park_release();
            }
            Kind::Cancel => {
                if wait(
                    ptr::addr_of_mut!((*raw()).cancel_queue),
                    u64::MAX,
                    b"taskreg-cancel",
                ) == 3
                {
                    (*raw()).cancel_results = (*raw()).cancel_results.wrapping_add(1);
                } else {
                    (*raw()).failures = (*raw()).failures.wrapping_add(1);
                }
                park_release();
            }
            Kind::Timeout => {
                if wait(
                    ptr::addr_of_mut!((*raw()).timeout_queue),
                    5,
                    b"taskreg-timeout",
                ) == 2
                {
                    (*raw()).timeout_results = (*raw()).timeout_results.wrapping_add(1);
                } else {
                    (*raw()).failures = (*raw()).failures.wrapping_add(1);
                }
                park_release();
            }
            Kind::Kill => {
                wait(
                    ptr::addr_of_mut!((*raw()).kill_queue),
                    u64::MAX,
                    b"taskreg-kill",
                );
                (*raw()).failures = (*raw()).failures.wrapping_add(1);
            }
        }
    }
}
unsafe fn park_release() {
    unsafe {
        while !(*raw()).release_workers {
            if wait(
                ptr::addr_of_mut!((*raw()).finish_queue),
                100,
                b"taskreg-finish",
            ) == 5
            {
                (*raw()).failures = (*raw()).failures.wrapping_add(1);
                return;
            }
        }
    }
}
pub unsafe fn stable() -> bool {
    unsafe {
        for i in 0..WORKERS {
            let t = (*raw()).tasks[i];
            if t.is_null() {
                return false;
            }
            let v = view(t);
            if v.id != (*raw()).ids[i] || v.generation != (*raw()).generations[i] {
                return false;
            }
        }
        true
    }
}
pub unsafe fn wait_started() -> bool {
    unsafe {
        let mut spins = 0;
        while (*raw()).started < WORKERS as u32 && spins < WAIT_BUDGET {
            yield_task();
            if spins != 0 && spins % 1000 == 0 {
                log_started(spins);
            }
            spins += 1;
        }
        (*raw()).started == WORKERS as u32
    }
}
unsafe fn log_started(spins: usize) {
    unsafe {
        let mut states = [[0u32; 3]; 3];
        for i in 0..WORKERS {
            let t = (*raw()).tasks[i];
            if t.is_null() {
                continue;
            }
            let v = view(t);
            let selected = match v.state {
                State::Ready => Some(0),
                State::Running => Some(1),
                State::Blocked => Some(2),
                _ => None,
            };
            if let Some(j) = selected {
                states[v.priority as usize][j] += 1;
            }
        }
        puts(b"TASKREG05910 phase=started-wait spins=");
        dec(spins as u64);
        puts(b" started=");
        dec(u64::from((*raw()).started));
        for (label, index) in [
            (b" high=".as_slice(), 0),
            (b" normal=".as_slice(), 1),
            (b" low=".as_slice(), 2),
        ] {
            puts(label);
            for (j, value) in states[index].iter().enumerate() {
                if j != 0 {
                    puts(b"/");
                }
                dec(u64::from(*value));
            }
        }
        puts(b"\r\n");
    }
}
pub unsafe fn wait_kind(wanted: Kind, reason: &[u8]) -> bool {
    unsafe {
        for _ in 0..WAIT_BUDGET {
            let mut ready = true;
            for i in 0..WORKERS {
                if kind(i) != wanted {
                    continue;
                }
                let t = (*raw()).tasks[i];
                if t.is_null() {
                    return false;
                }
                let v = view(t);
                if v.generation != (*raw()).generations[i]
                    || v.state != State::Blocked
                    || !reason_is(v.reason, reason)
                {
                    ready = false;
                    break;
                }
            }
            if ready {
                return true;
            }
            yield_task();
        }
        false
    }
}
pub unsafe fn wait_results() -> bool {
    unsafe {
        for _ in 0..WAIT_BUDGET {
            if (*raw()).signal_results == (SIGNAL_END - SLEEP_END) as u32
                && (*raw()).cancel_results == (CANCEL_END - SIGNAL_END) as u32
                && (*raw()).timeout_results == (TIMEOUT_END - CANCEL_END) as u32
            {
                return true;
            }
            yield_task();
        }
        false
    }
}
#[derive(Default)]
pub struct Mix {
    pub live: u64,
    pub runnable: u64,
    pub blocked: u64,
    pub priorities: u8,
    pub roles: u8,
    pub stable: bool,
}
pub unsafe fn mix() -> Mix {
    unsafe {
        let mut out = Mix {
            stable: true,
            ..Mix::default()
        };
        for i in 0..WORKERS {
            let t = (*raw()).tasks[i];
            if t.is_null() {
                out.stable = false;
                continue;
            }
            let v = view(t);
            if v.generation != (*raw()).generations[i] {
                out.stable = false;
            }
            if matches!(v.state, State::Dead | State::Unused) {
                continue;
            }
            out.live += 1;
            out.priorities |= 1 << v.priority;
            out.roles |= 1 << (v.role as u8);
            match v.state {
                State::Ready | State::Running => out.runnable += 1,
                State::Blocked => out.blocked += 1,
                _ => {}
            }
        }
        out
    }
}
pub unsafe fn wait_dead() -> bool {
    unsafe {
        for _ in 0..WAIT_BUDGET {
            let mut all = true;
            for i in 0..WORKERS {
                let t = (*raw()).tasks[i];
                if t.is_null() {
                    continue;
                }
                let v = view(t);
                if v.generation != (*raw()).generations[i] {
                    return false;
                }
                if v.state != State::Dead {
                    all = false;
                    break;
                }
            }
            if all {
                return true;
            }
            yield_task();
        }
        false
    }
}
pub unsafe fn release_workers() -> bool {
    unsafe {
        let mut ok = true;
        for i in 0..WORKERS {
            let t = (*raw()).tasks[i];
            if !t.is_null() {
                if !unpin(t) {
                    ok = false;
                }
                if !release((*raw()).ids[i], (*raw()).generations[i]) {
                    ok = false;
                }
            }
            (*raw()).tasks[i] = ptr::null_mut();
        }
        ok
    }
}
pub unsafe fn cleanup() {
    unsafe {
        (*raw()).release_workers = true;
        for i in 0..WORKERS {
            let id = (*raw()).ids[i];
            if id != 0 && r4os_rust_task_control(18, u64::from(id), 0) != 0 {
                // Preserve the original ID-based cleanup; pins retain every descriptor.
                super::resources::kill_id(id);
            }
        }
        queue(ptr::addr_of_mut!((*raw()).signal_event.queue), 3);
        queue(ptr::addr_of_mut!((*raw()).cancel_queue), 3);
        queue(ptr::addr_of_mut!((*raw()).timeout_queue), 3);
        queue(ptr::addr_of_mut!((*raw()).kill_queue), 3);
        queue(ptr::addr_of_mut!((*raw()).finish_queue), 3);
        release_workers();
        drain_resources();
    }
}
pub unsafe fn churn(baseline: Baseline) -> bool {
    unsafe {
        for cycle in 0..CHURN {
            if cycle != 0 && cycle % 1000 == 0 {
                puts(b"TASKREG05910 phase=churn progress=");
                dec(cycle as u64);
                puts(b"\r\n");
            }
            let t = create(
                b"taskreg-churn",
                churn_main,
                State::Ready,
                Role::Interactive,
            );
            if t.is_null() {
                return false;
            }
            let v = view(t);
            if !pin(t) {
                kill(v.id, v.generation);
                release(v.id, v.generation);
                return false;
            }
            let mut spins = 0;
            while view(t).state != State::Dead && spins < 4096 {
                spins += 1;
                yield_task();
            }
            if view(t).state != State::Dead {
                kill(v.id, v.generation);
                unpin(t);
                release(v.id, v.generation);
                return false;
            }
            let actual = view(t);
            if actual.id != v.id || actual.generation != v.generation {
                unpin(t);
                return false;
            }
            if !unpin(t)
                || !release(v.id, v.generation)
                || r4os_rust_task_control(1, 0, 0) != baseline.tasks
            {
                return false;
            }
        }
        true
    }
}
unsafe extern "C" fn churn_main() {
    unsafe {
        (*raw()).churn_completed = (*raw()).churn_completed.wrapping_add(1);
    }
}
pub unsafe extern "C" fn short_main() {
    unsafe {
        (*raw()).short_completed = (*raw()).short_completed.wrapping_add(1);
    }
}
