//! Generation-safe native lifetime and resource release.
//! A failed release remains anchored by the exact dead descriptor; no Rust
//! registry reference or runtime token spans heap/VM teardown.
use super::{
    task_registry as registry, task_resources as resources,
    task_types::{RetireResult, State, Task, WaitResult},
};
use crate::{
    arch::runtime,
    sched::{scheduler, task_context},
};
use core::{ffi::c_void, ptr::null_mut};
pub(super) unsafe fn is_current(task: *mut Task) -> bool {
    unsafe {
        (*registry::core(task)).running_cpu != 0xff || scheduler::current() == task.cast::<c_void>()
    }
}
pub(super) unsafe fn eligible(task: *mut Task) -> bool {
    unsafe {
        let c = registry::core(task);
        (*c).retire_pending
            && (*c).state == State::Dead
            && (*c).running_cpu == 0xff
            && (*c).pin_count == 0
            && !is_current(task)
            && (*c).wait_node.queue.is_null()
            && (*c).held_lock_count == 0
            && (*c).unwind_guard_count == 0
    }
}
pub(super) unsafe fn claim(task: *mut Task) -> Option<task_context::Unwind> {
    unsafe {
        if (*registry::core(task)).release_in_progress || !eligible(task) {
            return None;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return None;
        }
        (*registry::core(task)).release_in_progress = true;
        registry::unlink_reap(task);
        Some(token)
    }
}
unsafe fn clear_claim(task: *mut Task) {
    let flags = runtime::save_runtime();
    unsafe {
        if registry::contains(task) {
            let c = registry::core(task);
            (*c).release_in_progress = false;
            (*c).retire_pending = true;
            registry::link_reap(task);
        }
    }
    runtime::restore_runtime(flags);
}
fn puts(text: &[u8]) {
    unsafe { extern_write(text.as_ptr(), text.len()) };
}
unsafe extern "C" {
    #[link_name = "r4os_zig_kernel_write"]
    fn extern_write(text: *const u8, len: usize);
    #[link_name = "r4os_zig_kernel_dec"]
    fn extern_dec(value: u64);
}
pub(super) unsafe fn complete_release(task: *mut Task, token: task_context::Unwind) -> bool {
    unsafe {
        let result = complete_admitted(task);
        task_context::leave(token);
        result
    }
}
unsafe fn complete_admitted(task: *mut Task) -> bool {
    unsafe {
        let started = resources::timestamp();
        if !resources::release_resources(task) {
            clear_claim(task);
            return false;
        }
        let c = registry::core(task);
        if (*c).critical_reserve_slot != registry::NO_RESERVE {
            resources::measure(task);
            resources::poison(registry::GuardedStack {
                range_id: (*c).stack_range_id,
                base: (*c).stack_base,
                top: (*c).stack_top,
                ..registry::GuardedStack::EMPTY
            });
        }
        let role = (*c).role;
        let high_water = (*c).stack_high_water_bytes;
        let flags = runtime::save_runtime();
        if !registry::contains(task) || !(*c).release_in_progress || !eligible(task) {
            if registry::contains(task) {
                (*c).release_in_progress = false;
                registry::link_reap(task);
            }
            runtime::restore_runtime(flags);
            puts(b"TASK RELEASE INVARIANT: retry anchor changed\r\n");
            return false;
        }
        if (*c).critical_reserve_slot != registry::NO_RESERVE {
            let index = (*c).critical_reserve_slot as usize;
            if index >= registry::CRITICAL_RESERVE_COUNT {
                (*c).release_in_progress = false;
                registry::link_reap(task);
                runtime::restore_runtime(flags);
                puts(b"Critical task reserve slot invalid\r\n");
                return false;
            }
            let bundle = (*registry::raw()).critical[index];
            if bundle.task != task || !bundle.in_use {
                (*c).release_in_progress = false;
                registry::link_reap(task);
                runtime::restore_runtime(flags);
                puts(b"Critical task reserve ownership mismatch\r\n");
                return false;
            }
            registry::unlink_registry(task);
            resources::reset_critical(index);
            (*registry::raw()).critical[index].in_use = false;
            if (*registry::raw()).critical_in_use != 0 {
                (*registry::raw()).critical_in_use -= 1;
            }
            runtime::restore_runtime(flags);
            resources::record_released(
                role,
                high_water,
                resources::timestamp().wrapping_sub(started),
            );
            return true;
        }
        registry::unlink_registry(task);
        runtime::restore_runtime(flags);
        if resources::free_metadata(task) {
            resources::record_released(
                role,
                high_water,
                resources::timestamp().wrapping_sub(started),
            );
            return true;
        }
        let retry = runtime::save_runtime();
        (*c).release_in_progress = false;
        (*c).retire_pending = true;
        registry::link_registry(task);
        runtime::restore_runtime(retry);
        puts(b"Task object release pending retry\r\n");
        false
    }
}
pub(super) unsafe fn pin(task: *mut Task) -> bool {
    let flags = runtime::save_runtime();
    let result = unsafe {
        if !registry::contains(task)
            || (*registry::core(task)).state == State::Unused
            || (*registry::core(task)).release_in_progress
        {
            false
        } else if (*registry::core(task)).pin_count == u32::MAX {
            resources::note_failure(super::task_types::CreateFailure::Memory);
            false
        } else {
            (*registry::core(task)).pin_count += 1;
            true
        }
    };
    runtime::restore_runtime(flags);
    result
}
pub(crate) unsafe fn pin_identity(id: u32, generation: Option<u64>) -> *mut Task {
    let flags = runtime::save_runtime();
    let result = unsafe {
        let task = match generation {
            Some(value) => registry::find_identity(id, value),
            None => registry::find_id(id),
        };
        if task.is_null()
            || (*registry::core(task)).state == State::Unused
            || (*registry::core(task)).release_in_progress
            || (*registry::core(task)).pin_count == u32::MAX
        {
            null_mut()
        } else {
            (*registry::core(task)).pin_count += 1;
            task
        }
    };
    runtime::restore_runtime(flags);
    result
}
pub(crate) unsafe fn unpin(task: *mut Task) -> bool {
    unsafe {
        let flags = runtime::save_runtime();
        if !registry::contains(task) || (*registry::core(task)).pin_count == 0 {
            runtime::restore_runtime(flags);
            return false;
        }
        (*registry::core(task)).pin_count -= 1;
        let token = claim(task);
        runtime::restore_runtime(flags);
        if let Some(token) = token {
            complete_release(task, token);
        }
        true
    }
}
pub(super) unsafe fn finish_current(task: *mut Task, retire: bool) -> bool {
    let flags = runtime::save_runtime();
    let result = unsafe {
        if !registry::contains(task)
            || matches!((*registry::core(task)).state, State::Unused | State::Dead)
            || (*registry::core(task)).held_lock_count != 0
            || (*registry::core(task)).unwind_guard_count != 0
        {
            false
        } else {
            registry::transition_dead(task, WaitResult::Killed, retire);
            true
        }
    };
    runtime::restore_runtime(flags);
    result
}
pub(super) unsafe fn kill(id: u32, generation: Option<u64>) -> bool {
    let flags = runtime::save_runtime();
    let result = unsafe {
        let task = match generation {
            Some(value) => registry::find_identity(id, value),
            None => registry::find_id(id),
        };
        if task.is_null() {
            false
        } else {
            let c = registry::core(task);
            if matches!((*c).state, State::Unused | State::Dead | State::Running)
                || is_current(task)
                || (*c).critical_reserve_slot != registry::NO_RESERVE
            {
                false
            } else if (*c).held_lock_count != 0 || (*c).unwind_guard_count != 0 {
                (*registry::raw()).kill_held_lock_deferrals =
                    (*registry::raw()).kill_held_lock_deferrals.wrapping_add(1);
                false
            } else {
                registry::transition_dead(task, WaitResult::Killed, false);
                true
            }
        }
    };
    runtime::restore_runtime(flags);
    result
}
pub(super) unsafe fn retire(id: u32, generation: u64) -> RetireResult {
    unsafe {
        if id == 0 || generation == 0 {
            return RetireResult::Gone;
        }
        let flags = runtime::save_runtime();
        let task = registry::find_identity(id, generation);
        if task.is_null() {
            runtime::restore_runtime(flags);
            return RetireResult::Gone;
        }
        if is_current(task) {
            runtime::restore_runtime(flags);
            return RetireResult::Pending;
        }
        let c = registry::core(task);
        if (*c).state != State::Dead {
            if (*c).state == State::Unused
                || (*c).critical_reserve_slot != registry::NO_RESERVE
                || (*c).held_lock_count != 0
                || (*c).unwind_guard_count != 0
            {
                if (*c).held_lock_count != 0 || (*c).unwind_guard_count != 0 {
                    (*registry::raw()).kill_held_lock_deferrals =
                        (*registry::raw()).kill_held_lock_deferrals.wrapping_add(1);
                }
                runtime::restore_runtime(flags);
                return RetireResult::Pending;
            }
            registry::transition_dead(task, WaitResult::Killed, true);
        }
        (*c).retire_pending = true;
        registry::link_reap(task);
        let token = claim(task);
        runtime::restore_runtime(flags);
        match token {
            Some(token) if complete_release(task, token) => RetireResult::Released,
            _ => RetireResult::Pending,
        }
    }
}
pub(super) unsafe fn release_dead(id: u32, generation: Option<u64>) -> bool {
    unsafe {
        let flags = runtime::save_runtime();
        let task = match generation {
            Some(value) => registry::find_identity(id, value),
            None => registry::find_id(id),
        };
        if task.is_null() || (*registry::core(task)).state != State::Dead {
            runtime::restore_runtime(flags);
            return false;
        }
        let c = registry::core(task);
        (*c).retire_pending = true;
        registry::link_reap(task);
        if is_current(task) {
            (*registry::raw()).recycle_guard_hits =
                (*registry::raw()).recycle_guard_hits.wrapping_add(1);
            if generation.is_none() {
                puts(b"TASK RECYCLE GUARD: releaseDead deferred for current task (id=");
                extern_dec(u64::from((*c).id));
                puts(b")\r\n");
            }
            runtime::restore_runtime(flags);
            return false;
        }
        let token = claim(task);
        runtime::restore_runtime(flags);
        match token {
            Some(token) => complete_release(task, token),
            None => false,
        }
    }
}
pub(super) unsafe fn reap() -> u32 {
    unsafe {
        let f = runtime::save_runtime();
        (*registry::raw()).reap_passes = (*registry::raw()).reap_passes.wrapping_add(1);
        let mut remaining = (*registry::raw()).reap_count;
        runtime::restore_runtime(f);
        let mut reaped = 0u32;
        while remaining != 0 {
            remaining -= 1;
            let flags = runtime::save_runtime();
            let candidate = (*registry::raw()).reap_head;
            let token = if candidate.is_null() {
                None
            } else {
                (*registry::raw()).reap_visits = (*registry::raw()).reap_visits.wrapping_add(1);
                let token = claim(candidate);
                if token.is_none() {
                    registry::rotate_reap(candidate);
                }
                token
            };
            runtime::restore_runtime(flags);
            if let Some(token) = token
                && complete_release(candidate, token)
            {
                reaped = reaped.wrapping_add(1);
            }
        }
        resources::retry_rollback();
        reaped
    }
}
