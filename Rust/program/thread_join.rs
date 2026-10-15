//! Exclusive exact-generation join leases. Completion and enrollment share the
//! existing WaitQueue critical section; no spin/yield loop replaces that wait.
use super::{
    thread_retire,
    threads::{self, Thread},
};
use crate::{
    contract::ProgramJoinHandle as Handle,
    memory::owner::Guard,
    platform::timer,
    sched::{
        scheduler,
        sync_types::{Text, WaitRequest},
        sync_wait,
        task::{task_api, task_registry},
        task_context,
    },
};
use core::{ffi::c_void, ptr};
pub(super) unsafe fn reclaim_stale(target: *mut Thread) {
    unsafe {
        if !(*target).join_lease_active
            || (*target).join_owner_task_id == 0
            || (*target).join_owner_task_generation == 0
        {
            return;
        }
        if task_api::r4os_rust_task_control(
            19,
            u64::from((*target).join_owner_task_id),
            (*target).join_owner_task_generation,
        ) != 0
        {
            return;
        }
        if threads::queue(target, 6) != 0 {
            return;
        }
        (*target).join_owner_task_id = 0;
        (*target).join_owner_task_generation = 0;
        (*target).join_lease_active = false;
        if (*target).join_waiter_refs != 0 {
            (*target).join_waiter_refs -= 1;
        }
        if (*target).pin_count != 0 {
            (*target).pin_count -= 1;
        }
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn claim(
    id: u32,
    expected: Option<u64>,
    instance: u32,
    instance_generation: u64,
    current_thread: u32,
    owner: u32,
    owner_generation: u64,
) -> Result<*mut Thread, i32> {
    let _guard = Guard::acquire(0);
    unsafe {
        let target = threads::by_id(id);
        if target.is_null() {
            return Err(-5);
        }
        if !(*target).used
            || (*target).retire_pending
            || (*target).retire_in_progress
            || (*target).instance_id != instance
            || (*target).instance_generation != instance_generation
            || expected.is_some_and(|g| (*target).generation != g)
        {
            return Err(-5);
        }
        if (*target).id == current_thread {
            return Err(-6);
        }
        if (*target).flags & threads::MAIN != 0 || (*target).flags & threads::JOINABLE == 0 {
            return Err(-9);
        }
        reclaim_stale(target);
        if (*target).join_lease_active
            || (*target).join_owner_task_id != 0
            || (*target).join_owner_task_generation != 0
            || (*target).pin_count == u32::MAX
        {
            return Err(-7);
        }
        (*target).join_owner_task_id = owner;
        (*target).join_owner_task_generation = owner_generation;
        (*target).join_lease_active = true;
        (*target).pin_count += 1;
        Ok(target)
    }
}
pub(super) unsafe fn release_locked(target: *mut Thread, owner: u32, generation: u64) -> bool {
    unsafe {
        if !(*target).join_lease_active
            || (*target).join_owner_task_id != owner
            || (*target).join_owner_task_generation != generation
            || (*target).join_waiter_refs != 0
        {
            return false;
        }
        (*target).join_owner_task_id = 0;
        (*target).join_owner_task_generation = 0;
        (*target).join_lease_active = false;
        if (*target).pin_count != 0 {
            (*target).pin_count -= 1;
        }
        true
    }
}
unsafe fn release(target: *mut Thread, owner: u32, generation: u64) {
    let _guard = Guard::acquire(0);
    unsafe {
        if threads::contains(target) {
            release_locked(target, owner, generation);
        }
    }
}
struct Lease {
    target: *mut Thread,
    owner: u32,
    generation: u64,
    retired: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.retired {
            unsafe { release(self.target, self.owner, self.generation) };
        }
    }
}
unsafe extern "C" fn still_needed(context: *mut c_void) -> bool {
    unsafe {
        let t = context.cast::<Thread>();
        (*t).used && matches!((*t).state, 1 | 2)
    }
}
fn remaining(deadline: u64) -> u64 {
    timer::r4os_rust_timer_control(8, timer::tick_count(), deadline)
}
unsafe fn finish(
    target: *mut Thread,
    owner: u32,
    generation: u64,
    deadline: u64,
    out: *mut i32,
) -> i32 {
    unsafe {
        let target_id = (*target).id;
        let started = timer::tick_count();
        let mut lease = Lease {
            target,
            owner,
            generation,
            retired: false,
        };
        let mut counted = false;
        let mut retries = 0u64;
        loop {
            let token = task_context::enter();
            if !token.admitted() {
                return -7;
            }
            let guard = Guard::acquire(0);
            if !threads::contains(target)
                || !(*target).join_lease_active
                || (*target).join_owner_task_id != owner
                || (*target).join_owner_task_generation != generation
                || (*target).join_waiter_refs != 0
                || threads::queue(target, 6) != 0
                || (*target).pin_count != 1
                || !matches!((*target).state, 3 | 4)
            {
                drop(guard);
                let _ = task_context::leave(token);
                return -7;
            }
            let exit_code = (*target).exit_code;
            if !counted {
                (*target).join_count = (*target).join_count.wrapping_add(1);
                counted = true;
            }
            (*target).retire_pending = true;
            (*target).retire_in_progress = true;
            threads::bump();
            drop(guard);
            if thread_retire::complete(target, token, false, true) {
                lease.retired = true;
                out.write(exit_code);
                let ticks = timer::tick_count().saturating_sub(started);
                if retries != 0 || ticks >= 25 {
                    crate::boot::foundation::puts(b"[R4XTHREAD] join cleanup complete thread=");
                    crate::boot::foundation::dec(u64::from(target_id));
                    crate::boot::foundation::puts(b" retries=");
                    crate::boot::foundation::dec(retries);
                    crate::boot::foundation::puts(b" ticks=");
                    crate::boot::foundation::dec(ticks);
                    crate::boot::foundation::puts(b"\r\n");
                }
                return 0;
            }
            if remaining(deadline) == 0 {
                return -8;
            }
            retries = retries.saturating_add(1);
            threads::yield_task();
        }
    }
}
unsafe fn join(id: u32, exact: Option<Handle>, timeout: u64, out: *mut i32) -> i32 {
    unsafe {
        if out.is_null() {
            return -1;
        }
        out.write(0);
        let deadline = if timeout == u64::MAX {
            u64::MAX
        } else {
            timer::r4os_rust_timer_control(6, timer::tick_count(), timeout)
        };
        let current = threads::current();
        if current.is_null() {
            return -2;
        }
        let instance = (*current).instance_id;
        let instance_generation = (*current).instance_generation;
        if exact.is_some_and(|h| {
            h.instance_id != instance || h.instance_generation != instance_generation
        }) {
            return -5;
        }
        let task = scheduler::current();
        if task.is_null() {
            return -2;
        }
        let core = task_registry::core(task.cast());
        let owner = (*core).id;
        let generation = (*core).generation;
        let target = match claim(
            id,
            exact.map(|h| h.thread_generation),
            instance,
            instance_generation,
            (*current).id,
            owner,
            generation,
        ) {
            Ok(target) => target,
            Err(code) => return code,
        };
        if matches!((*target).state, 1 | 2) {
            let guard = Guard::acquire(0);
            if !threads::contains(target)
                || !(*target).join_lease_active
                || (*target).join_owner_task_id != owner
                || (*target).join_owner_task_generation != generation
                || (*target).join_waiter_refs == u32::MAX
            {
                drop(guard);
                release(target, owner, generation);
                return -7;
            }
            (*target).join_waiter_refs += 1;
            drop(guard);
            let request = WaitRequest {
                timeout: remaining(deadline),
                reason: Text::literal(b"thread_join"),
                predicate: Some(still_needed),
                context: target.cast(),
                release: None,
                release_context: ptr::null_mut(),
                flags: 0,
                consume_runtime: false,
            };
            let result =
                sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*target).join_queue), &request);
            let guard = Guard::acquire(0);
            if !threads::contains(target)
                || !(*target).join_lease_active
                || (*target).join_owner_task_id != owner
                || (*target).join_owner_task_generation != generation
            {
                drop(guard);
                return -5;
            }
            if (*target).join_waiter_refs != 0 {
                (*target).join_waiter_refs -= 1;
            }
            let completed = matches!((*target).state, 3 | 4);
            drop(guard);
            if result == 2 && !completed {
                release(target, owner, generation);
                return -8;
            }
            if result != 1 && result != 4 && !completed {
                release(target, owner, generation);
                return -7;
            }
            return finish(target, owner, generation, deadline, out);
        }
        finish(target, owner, generation, deadline, out)
    }
}
/// # Safety
/// Canonical inputs/outputs retain the original API residency. Exact handle is
/// validated before output reset; the same single join lease spans all waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_join(
    id: u32,
    handle: *const Handle,
    exact: bool,
    timeout: u64,
    out: *mut i32,
) -> i32 {
    unsafe {
        if !exact {
            return join(id, None, timeout, out);
        }
        if handle.is_null() {
            return -1;
        }
        let handle = handle.read();
        if handle.thread_id == 0
            || handle.instance_id == 0
            || handle.thread_generation == 0
            || handle.instance_generation == 0
            || handle.reserved != 0
        {
            return -1;
        }
        join(handle.thread_id, Some(handle), timeout, out)
    }
}
