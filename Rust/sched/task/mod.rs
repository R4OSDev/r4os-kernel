//! Canonical native dynamic tasks. External callers receive opaque handles
//! and fixed records; the scheduler retains exact typed task access.
pub(crate) mod balance;
pub(crate) mod cpu_drain;
pub(crate) mod diagnostic;
mod home;
pub(crate) mod task_api;
pub(crate) mod task_lifecycle;
pub(crate) mod task_registry;
pub(crate) mod task_resources;
pub(crate) mod task_snapshots;
pub(crate) mod task_types;
pub(crate) mod thread_placement;
use crate::sched::scheduler::TaskView;
use core::{ffi::c_void, ptr};
pub(crate) use task_types::Task;

/// # Safety
/// Exact current task and resident reason under uninterrupted Runtime
/// admission. `now` is this enrollment's already resolved deadline base,
/// never a cached raw counter or a value retained through a wait/migration.
pub(crate) unsafe fn begin_wait_at(
    task: *mut Task,
    wake: u64,
    now: u64,
    reason: task_types::Bytes,
    object: u64,
) {
    // Keep the legacy task-mutation admission/IF and accounting contract;
    // this change removes only its redundant clock projection.
    let flags = crate::arch::runtime::save_runtime();
    unsafe { task_registry::begin_wait(task, wake, now, reason, object) };
    crate::arch::runtime::restore_runtime(flags);
}

/// # Safety
/// Exact resident task under uninterrupted Runtime admission. `now` is the
/// resolved cutoff of this bounded wake decision, never carried across a
/// wait, owner release, migration or clock-source transition by the caller.
pub(crate) unsafe fn finish_wait_at(
    task: *mut Task,
    now: u64,
    result: task_types::WaitResult,
) -> u64 {
    let flags = crate::arch::runtime::save_runtime();
    let ticks = unsafe { task_registry::finish_wait(task, now, result) };
    crate::arch::runtime::restore_runtime(flags);
    ticks
}

/// Enroll one owned, pinned, non-running diagnostic task through the real
/// deadline owner. Caller supplies an absolute future tick under runtime admission.
pub(crate) unsafe fn diagnostic_timeout(task: *mut Task, tick: u64, reason: &'static [u8]) {
    let _runtime = crate::arch::runtime::RuntimeGuard::acquire();
    unsafe {
        task_registry::begin_wait(
            task,
            tick,
            crate::platform::timer::tick_count(),
            task_types::Bytes {
                pointer: reason.as_ptr(),
                len: reason.len(),
            },
            0,
        );
    }
}

/// # Safety
/// Non-null exact task remains resident through the call. Identity fields are
/// immutable for that incarnation; no mutable scheduler metadata is sampled.
pub(crate) unsafe fn immutable_identity(task: *mut c_void) -> (u32, u64) {
    let core = unsafe { task_registry::core(task.cast()) };
    unsafe { ((*core).id, (*core).generation) }
}

// Keep scalar native projections visible to the caller; a single field
// access must not materialize the complete temporary TaskView.
#[inline(always)]
pub(crate) unsafe fn scheduler_view(task: *mut c_void) -> TaskView {
    unsafe {
        let t = task.cast::<Task>();
        let c = task_registry::core(t);
        let rank = task_registry::dispatch_rank(t);
        TaskView {
            rsp_slot: ptr::addr_of_mut!((*c).rsp),
            unwind_slot: ptr::addr_of_mut!((*c).unwind_guard_count),
            entry: (*c).entry,
            generation: (*c).generation,
            rsp: (*c).rsp,
            wake_tick: (*c).wake_tick,
            wait_object: (*c).wait_object,
            last_scheduled_tick: (*c).last_scheduled_tick,
            ready_since_tick: (*c).ready_since_tick,
            created_tick: (*c).created_tick,
            last_long_warning: (*c).last_long_run_warning_tick,
            last_starvation_warning: (*c).last_starvation_warning_tick,
            max_ready_latency: (*c).max_ready_latency_ticks,
            id: (*c).id,
            home_cpu: u32::from((*c).home_cpu),
            preempt_depth: (*c).preempt_disable_depth,
            held_locks: (*c).held_lock_count,
            unwind_count: (*c).unwind_guard_count,
            state: (*c).state as u8,
            role: (*c).role as u8,
            priority: match rank {
                0 | 1 => 0,
                2 => 1,
                _ => 2,
            },
            dispatch_rank: rank,
            cpu_idle: (*c).cpu_idle,
            smp_eligible: (*c).smp_eligible,
            smp_r4x: (*c).smp_r4x_work,
            uses_fpu: (*c).uses_fpu,
            fpu_valid: (*c).fpu_state_valid,
            fpu_present: (*c).fpu_state.is_some(),
            handoff_pending: (*c).wait_handoff_guard_pending || !(*c).guard_offer.is_empty(),
        }
    }
}
pub(crate) unsafe fn scheduler_query(operation: u8, target: *mut c_void, value: u64) -> u64 {
    unsafe {
        let control = task_api::r4os_rust_task_control;
        match operation {
            0 => control(1, 0, 0),
            1 => control(21, 0, 0),
            2 => control(32, value, 0),
            3 => control(23, value, 0),
            4 => control(22, value, 0),
            5 => control(24, value as u32 as u64, value >> 32),
            6 => control(25, value, 0),
            7 => control(
                31,
                value,
                u64::from(task_registry::dispatch_rank(target.cast())),
            ),
            8 => control(15, 0, 0),
            9 => control(27, 0, 0),
            10 => task_snapshots::ordinal(target.cast()).unwrap_or(0) as u64,
            11 => {
                let mut failure = task_types::CreateFailure::None;
                task_resources::create(
                    task_resources::CreateArgs {
                        name: task_types::Bytes {
                            pointer: b"st-prio-busy".as_ptr(),
                            len: 12,
                        },
                        state: task_types::State::Ready,
                        needs_fpu: true,
                        entry: Some(core::mem::transmute::<usize, task_types::Entry>(
                            value as usize,
                        )),
                        role: task_types::Role::Interactive,
                        smp_eligible: false,
                        smp_r4x: false,
                        owner_kind: task_types::ExecutionOwnerKind::None,
                        owner_context: ptr::null_mut(),
                    },
                    &mut failure,
                )
                .addr() as u64
            }
            _ => panic!("Invalid native scheduler task query"),
        }
    }
}
pub(crate) unsafe fn scheduler_mutate(
    operation: u8,
    target: *mut c_void,
    a: u64,
    b: u64,
    text: *const u8,
    len: usize,
) -> u64 {
    unsafe {
        if operation == 17 {
            return task_api::r4os_rust_task_control(16, 0, 0);
        }
        if operation == 19 {
            (*task_registry::core(target.cast())).max_ready_latency_ticks = 0;
            return 0;
        }
        let mapped = match operation {
            0 => 1,
            1 => 5,
            2 => 0,
            3 => 2,
            4 => 3,
            5 => 4,
            6 => 19,
            7 => 20,
            8 => 7,
            9 => 8,
            10 => 9,
            11 => 10,
            12 => 11,
            13 => 12,
            14 => 13,
            15 => 14,
            16 => 15,
            18 => 16,
            21 => 6,
            _ => panic!("Invalid native scheduler task mutation"),
        };
        task_api::r4os_rust_task_mutate(mapped, target, a, b, text, len)
    }
}

/// Bounded passive counters with the same runtime admission as task scans.
pub(crate) fn observer_fpu_counts() -> (u64, u64, u32) {
    let _owner = crate::arch::runtime::RuntimeGuard::acquire();
    unsafe {
        let s = task_registry::raw();
        let mut count = 0u32;
        let mut t = (*s).head;
        while !t.is_null() {
            let c = task_registry::core(t);
            if !matches!(
                (*c).state,
                task_types::State::Unused | task_types::State::Dead
            ) && (*c).fpu_state_valid
            {
                count += 1
            }
            t = (*c).registry_next;
        }
        ((*s).fpu_lazy_saves, (*s).fpu_lazy_skips, count)
    }
}
