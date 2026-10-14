//! Task-owned wait enrollment, exact wake/cancel and ranked selection.
//! Canonical Task storage remains native; only fixed private C records cross the facade.
use super::{
    sync_types::*,
    task::{
        task_registry as registry,
        task_types::{State, Task, WaitResult},
    },
};
use crate::{
    arch::runtime,
    sched::{scheduler, wait_node},
};
use core::{ffi::c_void, ptr};
fn preempt_disable() {
    scheduler::r4os_rust_scheduler_control(6, 0, 0);
}
fn preempt_enable() {
    scheduler::r4os_rust_scheduler_control(7, 0, 0);
}
pub(super) fn enter() -> u64 {
    let flags = runtime::save_runtime();
    preempt_disable();
    flags
}
pub(super) fn leave(flags: u64) {
    preempt_enable();
    runtime::restore_runtime(flags);
}
fn current() -> *mut Task {
    scheduler::current().cast()
}
unsafe fn wake(target: *mut Task, result: WaitResult) -> bool {
    unsafe {
        scheduler::r4os_rust_scheduler_task(2, target.cast(), result as u64, 0, ptr::null(), 0) != 0
    }
}
/// Queue storage is never touched after park: the producer may already close
/// and free it once enrollment has detached. Only the running exact Task lives.
pub(super) unsafe fn wait(queue: *mut WaitQueue, request: &WaitRequest) -> WaitResult {
    unsafe {
        let target = current();
        if !request.consume_runtime && request.release.is_none() && target.is_null() {
            return WaitResult::Failed;
        }
        let flags = if request.consume_runtime {
            if target.is_null() {
                runtime::restore_runtime(request.flags);
                return WaitResult::Failed;
            }
            preempt_disable();
            request.flags
        } else {
            enter()
        };
        if let Some(release) = request.release {
            release(request.release_context);
        }
        if target.is_null() {
            leave(flags);
            return WaitResult::Failed;
        }
        if (*queue).core.closing {
            leave(flags);
            note(super::sync_stats::WaitMetric::Cancellations);
            return WaitResult::Cancelled;
        }
        if let Some(predicate) = request.predicate
            && !predicate(request.context)
        {
            leave(flags);
            return WaitResult::Signaled;
        }
        if request.timeout == 0 {
            leave(flags);
            note(super::sync_stats::WaitMetric::Timeouts);
            return WaitResult::Timeout;
        }
        super::sync_locks::note_sleep();
        let c = registry::core(target);
        if !wait_node::link(
            ptr::addr_of_mut!((*queue).core),
            ptr::addr_of_mut!((*c).wait_node),
            target.cast(),
            (*c).generation,
        ) {
            leave(flags);
            note(super::sync_stats::WaitMetric::Drops);
            return WaitResult::Failed;
        }
        let blocked = scheduler::r4os_rust_scheduler_task(
            0,
            ptr::null_mut(),
            queue.addr() as u64,
            request.timeout,
            request.reason.pointer,
            request.reason.len,
        ) as *mut Task;
        if blocked.is_null() {
            wait_node::detach(ptr::addr_of_mut!((*c).wait_node));
            leave(flags);
            note(super::sync_stats::WaitMetric::Drops);
            return WaitResult::Failed;
        }
        leave(flags);
        note(super::sync_stats::WaitMetric::QueueWaits);
        scheduler::r4os_rust_scheduler_task(1, blocked.cast(), 0, 0, ptr::null(), 0);
        let resumed = registry::core(blocked);
        let result = (*resumed).wait_result;
        let elapsed = (*resumed).last_wait_ticks;
        super::sync_stats::wait_latency(elapsed, result);
        result
    }
}
// Each access remains bounded; no borrow is carried through task dispatch.
pub(super) fn note(metric: super::sync_stats::WaitMetric) {
    super::sync_stats::wait_add(metric, 1);
}
pub(super) unsafe fn pop_waiter(queue: *mut WaitQueue, directed: bool) -> *mut wait_node::Node {
    unsafe {
        let core = ptr::addr_of_mut!((*queue).core);
        if !directed {
            return wait_node::pop_front(core);
        }
        let head = (*core).head;
        if head.is_null() {
            return ptr::null_mut();
        }
        let mut selected = ptr::null_mut();
        let mut best = 255;
        let mut cursor = head;
        while !cursor.is_null() {
            note(super::sync_stats::WaitMetric::DirectedScans);
            let c = registry::core((*cursor).owner.cast());
            if (*c).generation == (*cursor).owner_generation && (*c).state == State::Blocked {
                let rank = registry::wake_rank((*cursor).owner.cast());
                if rank < best {
                    best = rank;
                    selected = cursor;
                    if rank == 0 {
                        break;
                    }
                }
            }
            cursor = (*cursor).next;
        }
        if selected.is_null() {
            return wait_node::pop_front(core);
        }
        if selected != head {
            note(super::sync_stats::WaitMetric::DirectedFifoBypasses);
        }
        wait_node::detach(selected);
        selected
    }
}
pub(super) unsafe fn pop_wake(queue: *mut WaitQueue, result: WaitResult, directed: bool) -> u32 {
    unsafe {
        loop {
            let node = pop_waiter(queue, directed);
            if node.is_null() {
                return 0;
            }
            let target = (*node).owner.cast::<Task>();
            let c = registry::core(target);
            if (*c).generation != (*node).owner_generation {
                continue;
            }
            if wake(target, result) {
                if directed {
                    note(super::sync_stats::WaitMetric::DirectedWakes);
                }
                return (*c).id;
            }
        }
    }
}
pub(super) unsafe fn drain(queue: *mut WaitQueue, result: WaitResult) -> u32 {
    let mut count = 0u32;
    while unsafe { pop_wake(queue, result, false) } != 0 {
        count = count.saturating_add(1);
    }
    count
}
pub(super) unsafe fn handoff_permit(queue: *mut WaitQueue) -> u32 {
    unsafe {
        loop {
            let node = pop_waiter(queue, true);
            if node.is_null() {
                return 0;
            }
            let target = (*node).owner.cast::<Task>();
            let c = registry::core(target);
            if (*c).generation != (*node).owner_generation {
                continue;
            }
            if (*c).wait_handoff_guard_pending || (*c).unwind_guard_count == u32::MAX {
                wake(target, WaitResult::Failed);
                continue;
            }
            (*c).unwind_guard_count += 1;
            (*c).wait_handoff_guard_pending = true;
            if wake(target, WaitResult::Signaled) {
                note(super::sync_stats::WaitMetric::DirectedWakes);
                return (*c).id;
            }
            (*c).wait_handoff_guard_pending = false;
            (*c).unwind_guard_count -= 1;
        }
    }
}
/// # Safety
/// Queue/request are exclusive resident C records during enrollment; optional
/// callbacks keep their original predicate/release ownership. No queue borrow
/// or producer context is retained after the switch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_wait(
    queue: *mut WaitQueue,
    request: *const WaitRequest,
) -> u32 {
    unsafe { wait(queue, &*request) as u32 }
}
/// # Safety
/// Caller retains the exact resident queue and optionally leased task through
/// this bounded operation. Sequence pointers remain resident under runtime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_queue(
    queue: *mut WaitQueue,
    operation: u8,
    target: *mut c_void,
    value: u64,
    sequence: *mut u64,
) -> u64 {
    let flags = enter();
    let result = unsafe {
        match operation {
            0 => {
                let id = pop_wake(queue, WaitResult::Signaled, true);
                if id != 0 {
                    note(super::sync_stats::WaitMetric::WakeOne);
                }
                u64::from(id)
            }
            1 => {
                let n = drain(queue, WaitResult::Signaled);
                if n != 0 {
                    note(super::sync_stats::WaitMetric::WakeAll);
                }
                u64::from(n)
            }
            2 => u64::from(pop_wake(queue, WaitResult::Cancelled, false)),
            3 => u64::from(drain(queue, WaitResult::Cancelled)),
            4 => {
                wait_node::close(ptr::addr_of_mut!((*queue).core));
                u64::from(drain(queue, wait_result(value as u32)))
            }
            5 => u64::from(wait_node::reopen(ptr::addr_of_mut!((*queue).core))),
            6 => u64::from((*queue).core.count != 0),
            7 => {
                let t = target.cast::<Task>();
                let c = registry::core(t);
                u64::from(
                    (*c).generation == value
                        && (*c).state == State::Blocked
                        && (*c).wait_node.queue == ptr::addr_of_mut!((*queue).core)
                        && (*c).wait_node.owner_generation == value
                        && wake(t, WaitResult::Cancelled),
                )
            }
            8 => {
                *sequence = (*sequence).wrapping_add(1);
                let n = drain(queue, WaitResult::Signaled);
                if n != 0 {
                    note(super::sync_stats::WaitMetric::WakeAll);
                }
                u64::from(n)
            }
            9 => *sequence,
            _ => panic!("Invalid private queue operation"),
        }
    };
    leave(flags);
    result
}

/// # Safety
/// Timer queue and reason span retain resident caller lifetime during enrollment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sync_timer(
    queue: *mut WaitQueue,
    ticks: u64,
    text: *const u8,
    len: usize,
) -> bool {
    if ticks == 0 {
        super::sync_locks::note_sleep();
        scheduler::yield_task();
        return true;
    }
    let request = WaitRequest {
        timeout: ticks,
        reason: Text { pointer: text, len },
        predicate: None,
        context: ptr::null_mut(),
        release: None,
        release_context: ptr::null_mut(),
        flags: 0,
        consume_runtime: false,
    };
    unsafe { wait(queue, &request) == WaitResult::Timeout }
}
