//! Resident Event, Completion, Semaphore and exact handoff unwind tokens.
use super::{
    sync_types::*,
    sync_wait as wait,
    task::{
        task_registry as registry,
        task_types::{Task, WaitResult},
    },
};
use crate::{
    arch::runtime,
    sched::{scheduler, task_context, wait_node},
};
use core::{ffi::c_void, ptr};
unsafe extern "C" fn event_needed(raw: *mut c_void) -> bool {
    unsafe {
        let event = raw.cast::<Event>();
        if (*event).signaled {
            if (*event).mode == 1 {
                (*event).signaled = false;
            }
            false
        } else {
            true
        }
    }
}
unsafe extern "C" fn completion_needed(raw: *mut c_void) -> bool {
    unsafe {
        let c = raw.cast::<Completion>();
        if (*c).completed == 0 {
            true
        } else {
            (*c).completed -= 1;
            false
        }
    }
}
unsafe fn consume(semaphore: *mut Semaphore) -> bool {
    unsafe {
        if (*semaphore).count == 0 {
            false
        } else {
            (*semaphore).count -= 1;
            true
        }
    }
}
unsafe extern "C" fn semaphore_needed(raw: *mut c_void) -> bool {
    unsafe { !consume(raw.cast()) }
}
fn note(metric: super::sync_stats::WaitMetric) {
    super::sync_stats::wait_add(metric, 1);
}
unsafe fn claim(unwind: *mut task_context::Unwind) -> bool {
    unsafe {
        let t = scheduler::current().cast::<Task>();
        if t.is_null() {
            return false;
        }
        let flags = wait::enter();
        let c = registry::core(t);
        let ok = (*c).wait_handoff_guard_pending && (*c).unwind_guard_count != 0;
        if ok {
            (*c).wait_handoff_guard_pending = false;
            unwind.write(task_context::Unwind {
                counter: ptr::addr_of_mut!((*c).unwind_guard_count),
                context_present: true,
                active: true,
            });
        }
        wait::leave(flags);
        ok
    }
}
unsafe fn finish_unguarded(result: WaitResult) -> WaitResult {
    unsafe {
        if result != WaitResult::Signaled {
            return result;
        }
        let mut unwind = task_context::Unwind::EMPTY;
        if claim(&mut unwind) {
            let flags = runtime::save_runtime();
            let ok = task_context::leave(unwind);
            runtime::restore_runtime(flags);
            assert!(ok, "Semaphore handoff unwind token mismatch");
        }
        result
    }
}
struct GuardedContext {
    semaphore: *mut Semaphore,
    unwind: *mut task_context::Unwind,
    admission_failed: bool,
}
unsafe extern "C" fn guarded_needed(raw: *mut c_void) -> bool {
    unsafe {
        let ctx = raw.cast::<GuardedContext>();
        let semaphore = (*ctx).semaphore;
        if (*semaphore).count == 0 {
            return true;
        }
        let unwind = task_context::enter();
        if !unwind.admitted() {
            (*ctx).admission_failed = true;
            return false;
        }
        (*semaphore).count -= 1;
        (*ctx).unwind.write(unwind);
        false
    }
}
/// # Safety
/// Event is stable resident C storage. Wait uses only its Task result after
/// enrollment; signaling/reset retain the original owner/queue contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_event(
    event: *mut Event,
    operation: u8,
    timeout: u64,
) -> u32 {
    unsafe {
        match operation {
            0 => {
                let flags = wait::enter();
                let queue = ptr::addr_of_mut!((*event).queue);
                if (*event).mode == 1 {
                    // Keep nested wake acquisition: the signaled bit and single selection are
                    // published under the same outer critical section as in the old owner.
                    let woke = wait::pop_wake(queue, WaitResult::Signaled, true);
                    if woke == 0 {
                        (*event).signaled = true;
                    } else {
                        note(super::sync_stats::WaitMetric::WakeOne);
                    }
                    wait::leave(flags);
                    return 0;
                }
                (*event).signaled = true;
                wait::leave(flags);
                wait::r4os_rust_sync_queue(queue, 1, ptr::null_mut(), 0, ptr::null_mut());
                0
            }
            1 => {
                // Serialize the canonical bool with signal and enrollment.
                let flags = wait::enter();
                (*event).signaled = false;
                wait::leave(flags);
                0
            }
            2 => {
                let request = WaitRequest::predicate(
                    timeout,
                    Text::literal(b"event"),
                    event_needed,
                    event.cast(),
                );
                wait::wait(ptr::addr_of_mut!((*event).queue), &request) as u32
            }
            _ => panic!("Invalid private event operation"),
        }
    }
}
/// # Safety
/// Completion and its queue remain resident through explicit admission/close;
/// counter tokens preserve the original single/multiple wake semantics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_completion(
    completion: *mut Completion,
    operation: u8,
    timeout: u64,
) -> u32 {
    unsafe {
        if operation == 4 {
            let request = WaitRequest::predicate(
                timeout,
                Text::literal(b"completion"),
                completion_needed,
                completion.cast(),
            );
            return wait::wait(ptr::addr_of_mut!((*completion).queue), &request) as u32;
        }
        let flags = wait::enter();
        let queue = ptr::addr_of_mut!((*completion).queue);
        let result = match operation {
            0 => {
                if !(*queue).core.closing {
                    if wait::pop_wake(queue, WaitResult::Signaled, true) == 0 {
                        (*completion).completed = (*completion).completed.saturating_add(1);
                    } else {
                        note(super::sync_stats::WaitMetric::WakeOne);
                    }
                }
                0
            }
            1 => {
                if (*queue).core.closing {
                    0
                } else {
                    let n = wait::drain(queue, WaitResult::Signaled);
                    if n == 0 {
                        (*completion).completed = (*completion).completed.saturating_add(1);
                    } else {
                        note(super::sync_stats::WaitMetric::WakeAll);
                    }
                    n
                }
            }
            2 => {
                (*completion).completed = 0;
                wait::drain(queue, WaitResult::Cancelled)
            }
            3 => {
                (*completion).completed = 0;
                wait_node::close(ptr::addr_of_mut!((*queue).core));
                wait::drain(queue, WaitResult::Cancelled)
            }
            _ => panic!("Invalid private completion operation"),
        };
        wait::leave(flags);
        result
    }
}
/// # Safety
/// Semaphore and optional request/out-token are stable resident C records.
/// Release/predicate callbacks run only before enrollment; a successful guarded
/// result owns the exact canonical unwind slot until the original owner leaves.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_semaphore(
    semaphore: *mut Semaphore,
    operation: u8,
    value: u64,
    request: *const WaitRequest,
    unwind: *mut task_context::Unwind,
) -> u64 {
    unsafe {
        let queue = ptr::addr_of_mut!((*semaphore).queue);
        match operation {
            0 => {
                let flags = wait::enter();
                let ok = consume(semaphore);
                wait::leave(flags);
                u64::from(ok)
            }
            1 => {
                if scheduler::current().is_null() {
                    let flags = wait::enter();
                    let ok = consume(semaphore);
                    wait::leave(flags);
                    return if ok {
                        WaitResult::Signaled as u64
                    } else {
                        WaitResult::Failed as u64
                    };
                }
                let req = WaitRequest::predicate(
                    value,
                    Text::literal(b"semaphore"),
                    semaphore_needed,
                    semaphore.cast(),
                );
                finish_unguarded(wait::wait(queue, &req)) as u64
            }
            2 | 3 => {
                let mut req = request.read();
                req.reason = Text::literal(b"semaphore");
                if operation == 2 {
                    req.predicate = Some(semaphore_needed);
                    req.context = semaphore.cast();
                    return finish_unguarded(wait::wait(queue, &req)) as u64;
                }
                unwind.write(task_context::Unwind::EMPTY);
                let mut ctx = GuardedContext {
                    semaphore,
                    unwind,
                    admission_failed: false,
                };
                req.predicate = Some(guarded_needed);
                req.context = ptr::addr_of_mut!(ctx).cast();
                let result = wait::wait(queue, &req);
                if result != WaitResult::Signaled {
                    return result as u64;
                }
                if (*unwind).active {
                    return WaitResult::Signaled as u64;
                }
                if ctx.admission_failed {
                    return WaitResult::Failed as u64;
                }
                if claim(unwind) {
                    WaitResult::Signaled as u64
                } else {
                    WaitResult::Failed as u64
                }
            }
            4 => {
                let flags = wait::enter();
                let mut released = 0u32;
                let count = value as u32;
                for _ in 0..count {
                    if wait::handoff_permit(queue) == 0 {
                        if (*semaphore).count >= (*semaphore).max_count {
                            break;
                        }
                        (*semaphore).count += 1;
                    } else {
                        note(super::sync_stats::WaitMetric::WakeOne);
                    }
                    released += 1;
                }
                wait::leave(flags);
                u64::from(released)
            }
            _ => panic!("Invalid private semaphore operation"),
        }
    }
}

// Typed native callers use the same checked Event implementation as the C facade.
/// # Safety
/// Stable resident Event under its existing runtime-owned lifecycle.
pub(crate) unsafe fn event_signal(event: *mut Event) {
    unsafe {
        r4os_rust_sync_event(event, 0, 0);
    }
}
/// # Safety
/// Stable resident Event retained through the existing exact Task wait.
pub(crate) unsafe fn event_wait(event: *mut Event, timeout: u64) -> bool {
    unsafe { r4os_rust_sync_event(event, 2, timeout) == WaitResult::Signaled as u32 }
}

/// # Safety
/// Stable resident Queue and predicate/context through enrollment. Predicate
/// is nonblocking under runtime admission; no context pointer survives sleep.
pub(crate) unsafe fn queue_wait_unless(
    queue: *mut WaitQueue,
    timeout: u64,
    reason: Text,
    predicate: Predicate,
    context: *mut c_void,
) -> WaitResult {
    unsafe {
        wait::wait(
            queue,
            &WaitRequest::predicate(timeout, reason, predicate, context),
        )
    }
}
/// # Safety
/// Stable resident Queue under its existing runtime-owned lifetime.
pub(crate) unsafe fn queue_wake_all(queue: *mut WaitQueue) -> usize {
    unsafe { wait::r4os_rust_sync_queue(queue, 1, ptr::null_mut(), 0, ptr::null_mut()) as usize }
}
