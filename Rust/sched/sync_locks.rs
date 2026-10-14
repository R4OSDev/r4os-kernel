//! Generation-safe Mutex/UnwindGuard; transfer precedes Ready publication.
use super::{
    sync_types::*,
    sync_wait as wait,
    task::{
        task_lifecycle as lifetime, task_registry as registry,
        task_types::{Bytes, HeldLockRecord, State, Task, WaitResult},
    },
};
use crate::{arch::runtime, build_bridge::config, platform::timer, sched::scheduler};
use core::{ffi::c_void, ptr};
const NO_RANK: u8 = 255;
fn current() -> *mut Task {
    scheduler::current().cast()
}
fn note(f: impl FnOnce(&mut LockSummary)) {
    super::sync_stats::lock_change(f);
}
unsafe fn wake(target: *mut Task, result: WaitResult) -> bool {
    unsafe {
        scheduler::r4os_rust_scheduler_task(2, target.cast(), result as u64, 0, ptr::null(), 0) != 0
    }
}
pub(super) fn note_sleep() {
    if !config::ENABLE_METRICS {
        return;
    }
    let t = current();
    if t.is_null() {
        return;
    }
    let flags = runtime::save_runtime();
    unsafe {
        let c = registry::core(t);
        let mut any = false;
        let mut no_sleep = false;
        for held in &(*c).held_locks {
            if held.active {
                any = true;
                if held.mode_no_sleep {
                    no_sleep = true;
                }
            }
        }
        note(|s| {
            s.sleep_checks = s.sleep_checks.wrapping_add(1);
            if any {
                s.sleep_under_lock = s.sleep_under_lock.wrapping_add(1);
            }
            if no_sleep {
                s.sleep_under_no_sleep_lock = s.sleep_under_no_sleep_lock.wrapping_add(1);
            }
        });
    }
    runtime::restore_runtime(flags);
}
pub(super) unsafe fn held_depth(t: *mut Task) -> u32 {
    unsafe {
        (*registry::core(t))
            .held_locks
            .iter()
            .filter(|h| h.active)
            .count() as u32
    }
}
unsafe fn record_order(t: *mut Task, id: u64, rank: u16) {
    if !config::ENABLE_METRICS {
        return;
    }
    unsafe {
        for held in &(*registry::core(t)).held_locks {
            if held.active && held.object_id != id && held.rank >= rank {
                note(|s| s.order_violations = s.order_violations.wrapping_add(1));
                return;
            }
        }
    }
}
unsafe fn record_acquire(t: *mut Task, id: u64, name: Text, rank: u16, mode: u8) {
    note(|s| s.acquires = s.acquires.wrapping_add(1));
    unsafe {
        let c = registry::core(t);
        let mut free = None;
        for (i, h) in (*c).held_locks.iter().enumerate() {
            if h.active && h.object_id == id {
                return;
            }
            if !h.active && free.is_none() {
                free = Some(i);
            }
        }
        let Some(slot) = free else {
            note(|s| s.tracking_drops = s.tracking_drops.wrapping_add(1));
            return;
        };
        (*c).held_locks[slot] = HeldLockRecord {
            object_id: id,
            name: Bytes {
                pointer: name.pointer,
                len: name.len,
            },
            rank,
            mode_no_sleep: mode == 1,
            active: true,
        };
        let depth = held_depth(t);
        note(|s| {
            s.held_slots_used = s.held_slots_used.saturating_add(1);
            s.max_depth = s.max_depth.max(depth);
        });
    }
}
unsafe fn record_release(t: *mut Task, id: u64) {
    unsafe {
        let c = registry::core(t);
        for h in &mut (*c).held_locks {
            if h.active && h.object_id == id {
                *h = HeldLockRecord::default();
                note(|s| s.held_slots_used = s.held_slots_used.saturating_sub(1));
                return;
            }
        }
        note(|s| s.unlock_mismatches = s.unlock_mismatches.wrapping_add(1));
    }
}
unsafe fn mutex_try(m: *mut Mutex) -> bool {
    unsafe {
        let t = current();
        if t.is_null() {
            return false;
        }
        let flags = wait::enter();
        let c = registry::core(t);
        let result = if (*m).owner == 0 && (*m).owner_generation == 0 {
            if (*c).held_lock_count == u32::MAX {
                false
            } else {
                record_order(t, m.addr() as u64, (*m).rank);
                (*m).owner = (*c).id;
                (*m).owner_generation = (*c).generation;
                (*m).depth = 1;
                (*c).held_lock_count += 1;
                record_acquire(t, m.addr() as u64, (*m).name, (*m).rank, (*m).mode);
                true
            }
        } else if (*m).owner == (*c).id && (*m).owner_generation == (*c).generation {
            if (*m).handoff_pending {
                (*m).handoff_pending = false;
                true
            } else if (*m).depth == u32::MAX {
                false
            } else {
                (*m).depth += 1;
                note(|s| {
                    s.acquires = s.acquires.wrapping_add(1);
                    s.recursive_acquires = s.recursive_acquires.wrapping_add(1);
                });
                true
            }
        } else {
            false
        };
        wait::leave(flags);
        result
    }
}
unsafe fn mutex_permanent(m: *const Mutex) -> bool {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe {
        let t = current();
        if t.is_null() {
            return true;
        }
        let c = registry::core(t);
        if (*m).owner == 0 && (*m).owner_generation == 0 {
            return (*c).held_lock_count == u32::MAX;
        }
        (*m).owner == (*c).id && (*m).owner_generation == (*c).generation && (*m).depth == u32::MAX
    }
}
unsafe extern "C" fn mutex_owned(raw: *mut c_void) -> bool {
    unsafe {
        let m = raw.cast::<Mutex>();
        (*m).owner != 0 || (*m).owner_generation != 0
    }
}
unsafe fn donate(m: *mut Mutex) {
    unsafe {
        let t = current();
        if t.is_null() {
            return;
        }
        let desired = registry::dispatch_rank(t);
        let flags = wait::enter();
        let c = registry::core(t);
        if (*m).owner == 0
            || (*m).owner_generation == 0
            || ((*m).owner == (*c).id && (*m).owner_generation == (*c).generation)
            || desired >= (*m).donation_rank
        {
            wait::leave(flags);
            return;
        }
        let owner = lifetime::pin_identity((*m).owner, Some((*m).owner_generation));
        if owner.is_null() {
            wait::leave(flags);
            return;
        }
        if desired < registry::dispatch_rank(owner) {
            note(|s| s.role_inversions = s.role_inversions.wrapping_add(1));
            if (*m).donation_rank != NO_RANK {
                registry::remove_donation(owner, (*m).donation_rank);
                (*m).donation_rank = NO_RANK;
            }
            let o = registry::core(owner);
            if registry::donate((*o).id, (*o).generation, desired) {
                (*m).donation_rank = desired;
                note(|s| s.role_donations = s.role_donations.wrapping_add(1));
            }
        }
        lifetime::unpin(owner);
        wait::leave(flags);
    }
}
unsafe fn mutex_lock(m: *mut Mutex, timeout: u64) -> bool {
    unsafe {
        if mutex_try(m) {
            return true;
        }
        if mutex_permanent(m) || timeout == 0 {
            return false;
        }
        note(|s| s.contention_waits = s.contention_waits.wrapping_add(1));
        let bounded = timeout != u64::MAX;
        let deadline = if bounded {
            timer::r4os_rust_timer_control(7, timeout, 0)
        } else {
            0
        };
        let mut remaining = timeout;
        loop {
            donate(m);
            let request =
                WaitRequest::predicate(remaining, Text::literal(b"mutex"), mutex_owned, m.cast());
            if wait::wait(ptr::addr_of_mut!((*m).queue), &request) != WaitResult::Signaled {
                note(|s| s.contention_timeouts = s.contention_timeouts.wrapping_add(1));
                return false;
            }
            if mutex_try(m) {
                return true;
            }
            if mutex_permanent(m) {
                return false;
            }
            if bounded {
                let now = timer::tick_count();
                if now >= deadline {
                    note(|s| s.contention_timeouts = s.contention_timeouts.wrapping_add(1));
                    return false;
                }
                remaining = deadline - now;
            }
        }
    }
}
unsafe fn mutex_handoff(m: *mut Mutex) {
    unsafe {
        loop {
            let node = wait::pop_waiter(ptr::addr_of_mut!((*m).queue), true);
            if node.is_null() {
                return;
            }
            let t = (*node).owner.cast::<Task>();
            let c = registry::core(t);
            if (*c).generation != (*node).owner_generation || (*c).state != State::Blocked {
                continue;
            }
            if (*c).held_lock_count == u32::MAX {
                wake(t, WaitResult::Cancelled);
                continue;
            }
            record_order(t, m.addr() as u64, (*m).rank);
            (*m).owner = (*c).id;
            (*m).owner_generation = (*c).generation;
            (*m).depth = 1;
            (*m).handoff_pending = true;
            (*c).held_lock_count += 1;
            record_acquire(t, m.addr() as u64, (*m).name, (*m).rank, (*m).mode);
            if wake(t, WaitResult::Signaled) {
                super::sync_stats::wait_add(super::sync_stats::WaitMetric::WakeOne, 1);
                super::sync_stats::wait_add(super::sync_stats::WaitMetric::DirectedWakes, 1);
                return;
            }
            record_release(t, m.addr() as u64);
            (*c).held_lock_count -= 1;
            (*m).owner = 0;
            (*m).owner_generation = 0;
            (*m).depth = 0;
            (*m).handoff_pending = false;
        }
    }
}
unsafe fn mutex_unlock(m: *mut Mutex) -> bool {
    unsafe {
        let t = current();
        if t.is_null() {
            return false;
        }
        let c = registry::core(t);
        let flags = wait::enter();
        if (*m).owner != (*c).id || (*m).owner_generation != (*c).generation || (*m).depth == 0 {
            note(|s| s.unlock_mismatches = s.unlock_mismatches.wrapping_add(1));
            wait::leave(flags);
            return false;
        }
        (*m).depth -= 1;
        note(|s| s.releases = s.releases.wrapping_add(1));
        if (*m).depth == 0 {
            record_release(t, m.addr() as u64);
            if (*c).held_lock_count == 0 {
                note(|s| s.unlock_mismatches = s.unlock_mismatches.wrapping_add(1));
            } else {
                (*c).held_lock_count -= 1;
            }
            (*m).owner = 0;
            (*m).owner_generation = 0;
            (*m).handoff_pending = false;
            if (*m).donation_rank != NO_RANK {
                if registry::remove_donation(t, (*m).donation_rank) {
                    note(|s| s.role_donation_releases = s.role_donation_releases.wrapping_add(1));
                }
                (*m).donation_rank = NO_RANK;
            }
            mutex_handoff(m);
        }
        wait::leave(flags);
        true
    }
}
unsafe fn guard_free(g: *const UnwindGuard) -> bool {
    unsafe { (*g).owner == 0 && (*g).owner_generation == 0 && !(*g).boot_owner && (*g).depth == 0 }
}
unsafe fn guard_owned(g: *const UnwindGuard) -> bool {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe {
        let t = current();
        if t.is_null() {
            return (*g).boot_owner && (*g).depth != 0;
        }
        let c = registry::core(t);
        (*g).owner == (*c).id && (*g).owner_generation == (*c).generation && (*g).depth != 0
    }
}
unsafe fn guard_try(g: *mut UnwindGuard) -> bool {
    unsafe {
        let t = current();
        if t.is_null() {
            if guard_free(g) {
                (*g).boot_owner = true;
                (*g).depth = 1;
                return true;
            }
            if (*g).boot_owner && (*g).depth != u32::MAX {
                (*g).depth += 1;
                return true;
            }
            return false;
        }
        let flags = wait::enter();
        let c = registry::core(t);
        let result = if guard_free(g) {
            if (*c).unwind_guard_count == u32::MAX {
                false
            } else {
                (*g).owner = (*c).id;
                (*g).owner_generation = (*c).generation;
                (*g).depth = 1;
                (*c).unwind_guard_count += 1;
                true
            }
        } else if (*g).owner == (*c).id && (*g).owner_generation == (*c).generation {
            if (*g).depth == u32::MAX {
                false
            } else {
                (*g).depth += 1;
                true
            }
        } else {
            false
        };
        wait::leave(flags);
        result
    }
}
unsafe fn guard_permanent(g: *const UnwindGuard) -> bool {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe {
        let t = current();
        if t.is_null() {
            return (*g).boot_owner && (*g).depth == u32::MAX;
        }
        let c = registry::core(t);
        if guard_free(g) {
            return (*c).unwind_guard_count == u32::MAX;
        }
        (*g).owner == (*c).id && (*g).owner_generation == (*c).generation && (*g).depth == u32::MAX
    }
}
unsafe extern "C" fn guard_needed(raw: *mut c_void) -> bool {
    unsafe { !guard_free(raw.cast()) }
}
unsafe fn guard_enter(g: *mut UnwindGuard, timeout: u64) -> bool {
    unsafe {
        if guard_try(g) {
            return true;
        }
        if guard_permanent(g) || timeout == 0 {
            return false;
        }
        let bounded = timeout != u64::MAX;
        let deadline = if bounded {
            timer::r4os_rust_timer_control(7, timeout, 0)
        } else {
            0
        };
        let mut remaining = timeout;
        loop {
            let request = WaitRequest::predicate(remaining, (*g).name, guard_needed, g.cast());
            if wait::wait(ptr::addr_of_mut!((*g).queue), &request) != WaitResult::Signaled {
                return false;
            }
            if guard_try(g) {
                return true;
            }
            if guard_permanent(g) {
                return false;
            }
            if bounded {
                let now = timer::tick_count();
                if now >= deadline {
                    return false;
                }
                remaining = deadline - now;
            }
        }
    }
}
unsafe fn guard_leave(g: *mut UnwindGuard) -> bool {
    unsafe {
        let t = current();
        if t.is_null() {
            if !(*g).boot_owner || (*g).depth == 0 {
                return false;
            }
            (*g).depth -= 1;
            if (*g).depth == 0 {
                (*g).boot_owner = false;
                super::sync_wait::r4os_rust_sync_queue(
                    ptr::addr_of_mut!((*g).queue),
                    1,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                );
            }
            return true;
        }
        let c = registry::core(t);
        let flags = wait::enter();
        if (*g).owner != (*c).id || (*g).owner_generation != (*c).generation || (*g).depth == 0 {
            wait::leave(flags);
            return false;
        }
        (*g).depth -= 1;
        if (*g).depth == 0 {
            if (*c).unwind_guard_count != 0 {
                (*c).unwind_guard_count -= 1;
            }
            (*g).owner = 0;
            (*g).owner_generation = 0;
            // Wake all contenders while the outer owner is still held. Selection does
            // not grant this guard: a killed waiter must not strand the free guard.
            super::sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*g).queue),
                1,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            );
        }
        wait::leave(flags);
        true
    }
}
/// # Safety
/// Mutex is stable exclusive C storage under its original caller lifetime;
/// blocking calls retain no Rust borrow or runtime guard across dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_mutex(m: *mut Mutex, operation: u8, timeout: u64) -> bool {
    unsafe {
        match operation {
            0 => mutex_try(m),
            1 => mutex_lock(m, timeout),
            2 => mutex_unlock(m),
            _ => panic!("Invalid private mutex operation"),
        }
    }
}
/// # Safety
/// Guard retains its generation-safe owner storage until exact leave; this
/// explicit operation is not a Drop-based external-resource completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_guard(
    g: *mut UnwindGuard,
    operation: u8,
    timeout: u64,
) -> bool {
    unsafe {
        match operation {
            0 => guard_owned(g),
            1 => guard_try(g),
            2 => guard_enter(g, timeout),
            3 => guard_leave(g),
            _ => panic!("Invalid private unwind guard operation"),
        }
    }
}
