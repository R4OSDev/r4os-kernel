//! Ordered thread/task/I/O/namespace/stack retirement. A failed acknowledgement
//! retains the precise thread, task identity, join lease and original retry edge.
use super::{
    registry, retire_progress,
    retire_wait::Wait,
    stack, thread_join,
    threads::{self, Thread},
};
use crate::{
    contract::ProgramProcessHandle as Handle,
    memory::owner::Guard,
    platform::timer,
    sched::{
        scheduler,
        task::{task_api, task_registry},
        task_context,
    },
    storage::operations,
};
use core::{mem, ptr};

fn report(enabled: bool, detail: &[u8]) {
    super::entry::report_stage(enabled, detail, false);
}
const DETAILS: [&[u8]; 6] = [
    b"SERVMAN: Spawn wartet",
    b"SERVMAN: Task-Reaper wartet",
    b"SERVMAN: Reaper wartet",
    b"SERVMAN: Join wartet",
    b"SERVMAN: Thread-Pin wartet",
    b"SERVMAN: Schutz wartet",
];
unsafe fn terminate(handle: Handle, code: i32, skip: Option<u32>) {
    let now = timer::tick_count();
    let _guard = Guard::acquire(0);
    unsafe {
        let mut changed = false;
        let mut cursor = (*threads::state()).head;
        while !cursor.is_null() {
            let thread = cursor;
            cursor = (*thread).registry_next;
            if !(*thread).used
                || (*thread).instance_id != handle.instance_id
                || (*thread).instance_generation != handle.generation
            {
                continue;
            }
            if skip.is_some_and(|id| (*thread).task_id == id) {
                continue;
            }
            if threads::natural_epilogue(thread) {
                continue;
            }
            if !matches!((*thread).state, 3 | 4) {
                (*thread).exit_code = code;
                (*thread).state = 4;
                (*thread).finished_tick = now;
                changed = true;
                threads::queue(thread, 1);
            }
            if (*thread).spawn_transaction_depth != 0 {
                (*thread).exit_deferred = true;
                continue;
            }
            if (*thread).task_id != 0 {
                task_api::r4os_rust_task_identity(
                    3,
                    (*thread).task_id,
                    (*thread).task_generation,
                    ptr::null_mut(),
                    0,
                );
            }
        }
        if changed {
            threads::bump();
        }
    }
}
enum Claim {
    Done,
    Pending(usize),
    Claimed(*mut Thread, task_context::Unwind),
}
unsafe fn pause(thread: *mut Thread, wait: Wait) {
    unsafe {
        (*thread).retire_retry.begin(retire_progress::round());
        (*thread).retire_retry.defer(timer::tick_count(), wait);
    }
}
unsafe fn observe(thread: *mut Thread, reason: usize) -> Wait {
    unsafe {
        match reason {
            // leave_spawn already publishes the exact deferred exit.
            0 => Wait::Progress,
            1 => {
                if task_api::observe_retirement((*thread).task_id, (*thread).task_generation) {
                    Wait::Progress
                } else {
                    Wait::Retry
                }
            }
            2 | 4 => {
                (*thread).retire_notice = true;
                Wait::Progress
            }
            3 => {
                (*thread).retire_notice = true;
                // A killed join owner may never run its join epilogue. Its
                // native task edge must also be covered before parking.
                if task_api::observe_retirement(
                    (*thread).join_owner_task_id,
                    (*thread).join_owner_task_generation,
                ) {
                    Wait::Progress
                } else {
                    Wait::Retry
                }
            }
            _ => Wait::Retry,
        }
    }
}
unsafe fn claim(handle: Handle, gated: bool) -> Claim {
    let _guard = Guard::acquire(0);
    unsafe {
        let mut pending = None;
        let mut changed = false;
        let mut cursor = (*threads::state()).head;
        while !cursor.is_null() {
            let thread = cursor;
            cursor = (*thread).registry_next;
            if !(*thread).used
                || (*thread).instance_id != handle.instance_id
                || (*thread).instance_generation != handle.generation
            {
                continue;
            }
            if gated
                && !(*thread)
                    .retire_retry
                    .ready(retire_progress::round(), timer::tick_count())
            {
                pending.get_or_insert(2);
                continue;
            }
            (*thread).retire_pending = true;
            (*thread).retire_for_instance = true;
            if !matches!((*thread).state, 3 | 4) {
                (*thread).exit_code = -9;
                (*thread).state = 4;
                (*thread).finished_tick = timer::tick_count();
                changed = true;
            }
            thread_join::reclaim_stale(thread);
            if threads::queue(thread, 6) != 0 {
                threads::queue(thread, 3);
            }
            let reason = if (*thread).spawn_transaction_depth != 0 {
                (*thread).exit_deferred = true;
                Some(0)
            } else if threads::natural_epilogue(thread) {
                Some(1)
            } else if (*thread).retire_in_progress {
                Some(2)
            } else if (*thread).join_lease_active
                || (*thread).join_waiter_refs != 0
                || threads::queue(thread, 6) != 0
            {
                Some(3)
            } else if (*thread).pin_count != 0 {
                Some(4)
            } else {
                None
            };
            if let Some(reason) = reason {
                if gated {
                    let wait = observe(thread, reason);
                    pause(thread, wait);
                }
                if pending.is_none() {
                    pending = Some(reason);
                }
                continue;
            }
            let token = task_context::enter();
            if !token.admitted() {
                if gated {
                    pause(thread, Wait::Retry);
                }
                if pending.is_none() {
                    pending = Some(5);
                }
                continue;
            }
            (*thread).retire_in_progress = true;
            (*thread).pin_count = 1;
            if gated {
                (*thread).retire_retry.begin(retire_progress::round());
            }
            if changed {
                threads::bump();
            }
            return Claim::Claimed(thread, token);
        }
        if changed {
            threads::bump();
        }
        pending.map_or(Claim::Done, Claim::Pending)
    }
}
unsafe fn clear_fields(thread: *mut Thread, keep_join: bool) {
    unsafe {
        (*thread).retire_in_progress = false;
        if !(*thread).retire_for_instance {
            (*thread).retire_pending = false;
        }
        if keep_join {
            assert!((*thread).join_lease_active);
        } else if (*thread).join_lease_active {
            thread_join::release_locked(
                thread,
                (*thread).join_owner_task_id,
                (*thread).join_owner_task_generation,
            );
        } else if (*thread).pin_count != 0 {
            (*thread).pin_count -= 1;
        }
    }
}
unsafe fn clear_claim(thread: *mut Thread, keep_join: bool) {
    let _guard = Guard::acquire(0);
    unsafe {
        if !threads::contains(thread) || !(*thread).retire_in_progress {
            return;
        }
        clear_fields(thread, keep_join);
        threads::notify_retire(thread);
    }
}
pub(super) unsafe fn release_task(
    id: u32,
    generation: u64,
    current: u32,
    current_generation: u64,
) -> bool {
    if id == 0 {
        return true;
    }
    if id == current && generation == current_generation {
        return false;
    }
    match unsafe { task_api::r4os_rust_task_identity(6, id, generation, ptr::null_mut(), 0) } {
        0 | 2 => {
            unsafe { program_thread_task_retired(id, generation) };
            true
        }
        1 => false,
        _ => unreachable!(),
    }
}
fn bump() {
    let _guard = Guard::acquire(0);
    unsafe { threads::bump() };
}
pub(super) unsafe fn complete(
    thread: *mut Thread,
    token: task_context::Unwind,
    report_boot: bool,
    keep_join: bool,
) -> bool {
    let _operation = threads::Operation(token);
    unsafe {
        let task_id = (*thread).task_id;
        let task_generation = (*thread).task_generation;
        let handle = Handle {
            instance_id: (*thread).instance_id,
            generation: (*thread).instance_generation,
            ..Handle::default()
        };
        if !(*thread).task_detached {
            if keep_join && threads::natural_epilogue(thread) {
                clear_claim(thread, keep_join);
                return false;
            }
            report(report_boot, b"SERVMAN: Task freigeben");
            let current = scheduler::current();
            let (id, generation) = if current.is_null() {
                (0, 0)
            } else {
                let c = task_registry::core(current.cast());
                ((*c).id, (*c).generation)
            };
            if !release_task(task_id, task_generation, id, generation) {
                report(report_boot, b"SERVMAN: Task wartet");
                clear_claim(thread, keep_join);
                return false;
            }
            (*thread).task_detached = true;
            bump();
            report(report_boot, b"SERVMAN: Task frei");
        }
        if !threads::release_input(thread) {
            clear_claim(thread, keep_join);
            return false;
        }
        report(report_boot, b"SERVMAN: I/O abbrechen");
        if !super::async_io::cancel(handle, Some((task_id, task_generation))) {
            report(report_boot, b"SERVMAN: I/O wartet");
            clear_claim(thread, keep_join);
            return false;
        }
        report(report_boot, b"SERVMAN: I/O entfernen");
        if !super::async_io::purge(handle, Some((task_id, task_generation))) {
            report(report_boot, b"SERVMAN: I/O-Abbau wartet");
            clear_claim(thread, keep_join);
            return false;
        }
        report(report_boot, b"SERVMAN: Streams loesen");
        if !program_thread_retire_peer(2, handle, task_id, task_generation) {
            report(report_boot, b"SERVMAN: Streams warten");
            clear_claim(thread, keep_join);
            return false;
        }
        let mut args: operations::Args = mem::zeroed();
        args.program = (*thread).instance_id;
        args.program_generation = (*thread).instance_generation;
        args.task = task_id;
        args.task_generation = task_generation;
        if operations::r4os_rust_storage_operations(15, &args, ptr::null_mut()) == 0 {
            clear_claim(thread, keep_join);
            return false;
        }
        if (*thread).task_id != 0 || (*thread).task_generation != 0 {
            (*thread).task_id = 0;
            (*thread).task_generation = 0;
            bump();
        }
        if !(*thread).stack_released {
            report(report_boot, b"SERVMAN: Stack freigeben");
            if (*thread).stack.range_id != 0
                && !stack::r4os_rust_program_stack_free(ptr::addr_of_mut!((*thread).stack))
            {
                report(report_boot, b"SERVMAN: Stack wartet");
                clear_claim(thread, keep_join);
                return false;
            }
            (*thread).stack_released = true;
            bump();
        }
        threads::unpin_execution(thread);
        if (*thread).execution_pinned {
            report(report_boot, b"SERVMAN: Pin wartet");
            clear_claim(thread, keep_join);
            return false;
        }
        let guard = Guard::acquire(0);
        if !threads::contains(thread)
            || !(*thread).retire_pending
            || !(*thread).retire_in_progress
            || !(*thread).task_detached
            || !(*thread).stack_released
            || (*thread).pin_count != 1
            || (*thread).join_waiter_refs != 0
            || threads::queue(thread, 6) != 0
        {
            drop(guard);
            report(report_boot, b"SERVMAN: Thread wartet");
            clear_claim(thread, keep_join);
            return false;
        }
        (*thread).flags |= threads::JOINED;
        let notice = mem::replace(&mut (*thread).retire_notice, false);
        threads::unlink(thread);
        drop(guard);
        if threads::free(thread) {
            if notice {
                retire_progress::changed();
            }
            report(report_boot, b"SERVMAN: Thread frei");
            return true;
        }
        report(report_boot, b"SERVMAN: Speicher wartet");
        let _guard = Guard::acquire(0);
        threads::link(thread);
        clear_fields(thread, keep_join);
        threads::notify_retire(thread);
        if notice {
            retire_progress::changed();
        }
        false
    }
}
unsafe fn release(handle: Handle, report_boot: bool, gated: bool) -> bool {
    loop {
        match unsafe { claim(handle, gated) } {
            Claim::Done => return true,
            Claim::Pending(reason) => {
                report(report_boot, DETAILS[reason]);
                return false;
            }
            Claim::Claimed(thread, token) => {
                report(report_boot, b"SERVMAN: Thread freigeben");
                // The claim pins this record. Keep only a value identity after
                // complete returns: another retire caller may then free it.
                let identity = unsafe {
                    crate::contract::ProgramJoinHandle {
                        thread_id: (*thread).id,
                        instance_id: (*thread).instance_id,
                        thread_generation: (*thread).generation,
                        instance_generation: (*thread).instance_generation,
                        ..Default::default()
                    }
                };
                if !unsafe { complete(thread, token, report_boot, false) } {
                    if !gated {
                        return false;
                    }
                    let _guard = Guard::acquire(0);
                    unsafe {
                        let current = threads::by_handle(identity);
                        if !current.is_null() {
                            let wait = if (*current).retire_in_progress {
                                (*current).retire_notice = true;
                                Wait::Progress
                            } else if !(*current).task_detached
                                && task_api::observe_retirement(
                                    (*current).task_id,
                                    (*current).task_generation,
                                )
                            {
                                Wait::Progress
                            } else {
                                Wait::Retry
                            };
                            // Preserve an in-flight round reset performed by
                            // the main reaper; defer never overwrites begin.
                            (*current).retire_retry.defer(timer::tick_count(), wait);
                        }
                    }
                }
            }
        }
    }
}
pub(super) unsafe fn release_ready(handle: Handle, report_boot: bool) -> bool {
    unsafe { release(handle, report_boot, true) }
}
/// Only scheduling metadata is inspected here. A lack of records or an
/// unclassified/in-flight failure needs a bounded retry, never a false ACK.
pub(super) unsafe fn waiting(handle: Handle) -> Wait {
    let _guard = Guard::acquire(0);
    unsafe {
        let mut found = false;
        let mut cursor = (*threads::state()).head;
        while !cursor.is_null() {
            let thread = cursor;
            cursor = (*thread).registry_next;
            if !(*thread).used
                || (*thread).instance_id != handle.instance_id
                || (*thread).instance_generation != handle.generation
            {
                continue;
            }
            found = true;
            if (*thread).retire_retry.deadline().is_some()
                || (*thread)
                    .retire_retry
                    .ready(retire_progress::round(), timer::tick_count())
            {
                return Wait::Retry;
            }
        }
        if found { Wait::Progress } else { Wait::Retry }
    }
}
pub(super) fn reset_retire_rounds() -> bool {
    let _guard = Guard::acquire(0);
    unsafe {
        let mut thread = (*threads::state()).head;
        while !thread.is_null() {
            (*thread).retire_retry.reset_round();
            thread = (*thread).registry_next;
        }
    }
    true
}
/// # Safety
/// No registry mutex held. Exact handle and optional skip identity are values;
/// each thread remains with its original task/instance lifetime through kill.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_threads_terminate(
    handle: Handle,
    code: i32,
    has_skip: bool,
    skip: u32,
) {
    unsafe { terminate(handle, code, has_skip.then_some(skip)) };
}
/// # Safety
/// Original retirement caller owns no program registry mutex. Every async peer
/// and successful subrelease retains its exact original acknowledgement edge.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_threads_release(
    handle: Handle,
    report_boot: bool,
) -> bool {
    unsafe { release(handle, report_boot, false) }
}
/// # Safety
/// Original cooperative by-ID lookup, then exact generation retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_threads_instance(
    id: u32,
    terminate_threads: bool,
    code: i32,
    has_skip: bool,
    skip: u32,
) -> bool {
    unsafe {
        let mut handle = Handle::default();
        if !registry::r4os_rust_program_registry_handle(3, ptr::null(), id, &mut handle) {
            return true;
        }
        if terminate_threads {
            terminate(handle, code, has_skip.then_some(skip));
            true
        } else {
            release(handle, false, false)
        }
    }
}
/// # Safety
/// Exact task identities under the original cleanup lifetime; successful task
/// release notifies existing graphics owners before acknowledging completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_release_task(
    id: u32,
    generation: u64,
    current: u32,
    current_generation: u64,
) -> bool {
    unsafe { release_task(id, generation, current, current_generation) }
}
/// # Safety
/// Exact created Task remains resident at the original rollback edge.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_release_created(
    task: *mut core::ffi::c_void,
) -> bool {
    unsafe {
        let current = scheduler::current();
        let (id, generation) = if current.is_null() {
            (0, 0)
        } else {
            let c = task_registry::core(current.cast());
            ((*c).id, (*c).generation)
        };
        let c = task_registry::core(task.cast());
        release_task((*c).id, (*c).generation, id, generation)
    }
}

unsafe fn program_thread_retire_peer(
    operation: u8,
    handle: Handle,
    task: u32,
    generation: u64,
) -> bool {
    assert!(operation == 2);
    unsafe {
        crate::program::system::stream::r4os_rust_sys_stream_cleanup(
            handle.instance_id,
            handle.generation,
            task,
            generation,
            true,
        )
    }
}

unsafe fn program_thread_task_retired(task: u32, generation: u64) {
    crate::display::queue::retired_task(task, generation);
    crate::gfx::allocations::retired_task(task, generation);
    crate::gfx::virtual_runtime::retired_task(task, generation);
}
