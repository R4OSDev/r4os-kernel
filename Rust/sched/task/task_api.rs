//! Private C facade for still-Zig kernel consumers. Every pointer is an opaque
//! live task handle or a fixed C projection, never a native Rust task layout.
use super::{
    task_lifecycle as lifetime, task_registry as registry, task_resources as resources,
    task_snapshots as snapshots,
    task_types::{
        Bytes, CreateFailure, Entry, ExecutionOwnerKind, Priority, Role, State, Task, WaitResult,
    },
};
use crate::{
    arch::{fpu, percpu, runtime},
    platform::timer,
};
use core::{
    ffi::c_void,
    ptr::{self, null_mut},
};
fn state(value: u8) -> State {
    match value {
        0 => State::Unused,
        1 => State::Ready,
        2 => State::Running,
        3 => State::Blocked,
        4 => State::Dead,
        _ => panic!("Invalid task state"),
    }
}
fn role(value: u8) -> Role {
    match value {
        0 => Role::Input,
        1 => Role::ShortCompletion,
        2 => Role::Interactive,
        3 => Role::Batch,
        _ => panic!("Invalid task role"),
    }
}
fn owner(value: u8) -> ExecutionOwnerKind {
    match value {
        0 => ExecutionOwnerKind::None,
        1 => ExecutionOwnerKind::ProgramThread,
        2 => ExecutionOwnerKind::AsyncIo,
        3 => ExecutionOwnerKind::DriverThread,
        _ => panic!("Invalid task execution owner"),
    }
}
fn result(value: u64) -> WaitResult {
    match value {
        0 => WaitResult::None,
        1 => WaitResult::Signaled,
        2 => WaitResult::Timeout,
        3 => WaitResult::Cancelled,
        4 => WaitResult::Killed,
        5 => WaitResult::Failed,
        _ => panic!("Invalid task wait result"),
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Span {
    pub pointer: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HeldLock {
    pub object_id: u64,
    pub name: Span,
    pub rank: u16,
    pub mode_no_sleep: bool,
    pub active: bool,
}
#[repr(C)]
pub struct GuardHit {
    pub name: Span,
    pub id: u32,
    pub stack_base: u64,
}
#[repr(C)]
pub struct RoleStats {
    pub activations: [u64; 4],
    pub budget_exhaustions: [u64; 4],
    pub donation_requests: u64,
    pub donations_applied: u64,
    pub donations_released: u64,
    pub donation_budget_exhaustions: u64,
}
#[repr(C)]
pub struct ReapStats {
    pub passes: u64,
    pub candidate_visits: u64,
    pub pending: usize,
}
/// # Safety
/// Exclusive C output; range lookup and copied borrowed name retain the caller
/// snapshot/lifetime contract and the existing runtime admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_guard_hit(address: u64, output: *mut GuardHit) -> bool {
    let flags = runtime::save_runtime();
    let mut found = false;
    unsafe {
        let mut task = (*registry::raw()).head;
        while !task.is_null() {
            let c = registry::core(task);
            if (*c).state != State::Unused
                && (*c).stack_base != 0
                && address >= (*c).stack_base - 16 * 1024
                && address < (*c).stack_base
            {
                output.write(GuardHit {
                    name: Span {
                        pointer: (*c).name.pointer,
                        len: (*c).name.len,
                    },
                    id: (*c).id,
                    stack_base: (*c).stack_base,
                });
                found = true;
                break;
            }
            task = (*c).registry_next;
        }
    }
    runtime::restore_runtime(flags);
    found
}
/// # Safety
/// Exclusive aligned C output; all role/donation counters share runtime owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_roles(output: *mut RoleStats) {
    let flags = runtime::save_runtime();
    unsafe {
        let r = registry::raw();
        output.write(RoleStats {
            activations: (*r).role_activations,
            budget_exhaustions: (*r).role_exhaustions,
            donation_requests: (*r).donation_requests,
            donations_applied: (*r).donation_applied,
            donations_released: (*r).donation_released,
            donation_budget_exhaustions: (*r).donation_exhaustions,
        });
    }
    runtime::restore_runtime(flags);
}
/// # Safety
/// Exclusive aligned C output. Runtime admission covers the complete snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_reap_stats(output: *mut ReapStats) {
    let flags = runtime::save_runtime();
    unsafe {
        let r = registry::raw();
        output.write(ReapStats {
            passes: (*r).reap_passes,
            candidate_visits: (*r).reap_visits,
            pending: (*r).reap_count + (*r).rollback_count,
        });
    }
    runtime::restore_runtime(flags);
}
/// # Safety
/// Caller retains exact task lifetime and lock-tracking admission. Output/input
/// is a fixed C record; no mutable diagnostic mirror is installed in Zig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_held_lock(
    task: *mut c_void,
    index: usize,
    write: bool,
    record: *mut HeldLock,
) {
    unsafe {
        assert!(index < 8);
        let c = registry::core(task.cast());
        if write {
            let v = record.read();
            (*c).held_locks[index] = super::task_types::HeldLockRecord {
                object_id: v.object_id,
                name: Bytes {
                    pointer: v.name.pointer,
                    len: v.name.len,
                },
                rank: v.rank,
                mode_no_sleep: v.mode_no_sleep,
                active: v.active,
            };
        } else {
            let v = (*c).held_locks[index];
            record.write(HeldLock {
                object_id: v.object_id,
                name: Span {
                    pointer: v.name.pointer,
                    len: v.name.len,
                },
                rank: v.rank,
                mode_no_sleep: v.mode_no_sleep,
                active: v.active,
            });
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CreateRequest {
    pub name: *const u8,
    pub name_len: usize,
    pub state: u8,
    pub role: u8,
    pub needs_fpu: bool,
    pub smp_eligible: bool,
    pub smp_r4x: bool,
    pub owner_kind: u8,
    pub owner_context: *mut c_void,
    pub entry: Option<Entry>,
}
/// # Safety
/// Exclusive initialized C request/failure output, borrowed name and callback
/// remain resident for the resulting exact task lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_create(
    request: *const CreateRequest,
    critical: bool,
    failure: *mut u8,
) -> *mut c_void {
    unsafe {
        let r = request.read();
        let mut f = CreateFailure::None;
        let name = Bytes {
            pointer: r.name,
            len: r.name_len,
        };
        let task = if critical {
            resources::create_critical(
                name,
                state(r.state),
                r.entry.expect("Critical task entry"),
                role(r.role),
                &mut f,
            )
        } else {
            resources::create(
                resources::CreateArgs {
                    name,
                    state: state(r.state),
                    needs_fpu: r.needs_fpu,
                    entry: r.entry,
                    role: role(r.role),
                    smp_eligible: r.smp_eligible,
                    smp_r4x: r.smp_r4x,
                    owner_kind: owner(r.owner_kind),
                    owner_context: r.owner_context,
                },
                &mut f,
            )
        };
        failure.write(f as u8);
        task.cast()
    }
}
/// # Safety
/// Opaque task retains its original exact lifetime and caller admission.
/// Yielding operations have no runtime/registry borrow across their call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_mutate(
    operation: u8,
    opaque: *mut c_void,
    a: u64,
    b: u64,
    text: *const u8,
    len: usize,
) -> u64 {
    unsafe {
        let task = opaque.cast::<Task>();
        match operation {
            0 => {
                let f = runtime::save_runtime();
                registry::mark_ready(task, a);
                runtime::restore_runtime(f);
                0
            }
            1 => {
                let f = runtime::save_runtime();
                registry::mark_running(task);
                runtime::restore_runtime(f);
                0
            }
            2 => {
                let f = runtime::save_runtime();
                registry::park_idle(task);
                runtime::restore_runtime(f);
                0
            }
            3 => {
                let f = runtime::save_runtime();
                registry::switched_out(task);
                runtime::restore_runtime(f);
                0
            }
            4 => registry::record_scheduled(task, a),
            5 => {
                (*registry::core(task)).last_yield_tick = a;
                0
            }
            6 => registry::record_run_tick(task, a),
            7 => u64::from(registry::preempt_disable(task)),
            8 => u64::from(registry::preempt_enable(task)),
            9 => {
                let c = registry::core(task);
                (*c).preemption_probe_hits = (*c).preemption_probe_hits.wrapping_add(1);
                0
            }
            10 => {
                let c = registry::core(task);
                (*c).preemption_deferred_ticks = (*c).preemption_deferred_ticks.wrapping_add(1);
                (*c).max_preemption_deferred_ticks = (*c).max_preemption_deferred_ticks.max(a);
                0
            }
            11 => {
                let c = registry::core(task);
                (*c).long_run_warnings = (*c).long_run_warnings.wrapping_add(1);
                (*c).last_long_run_warning_tick = a;
                0
            }
            12 => {
                let c = registry::core(task);
                (*c).starvation_warnings = (*c).starvation_warnings.wrapping_add(1);
                (*c).last_starvation_warning_tick = a;
                0
            }
            13 => {
                let f = runtime::save_runtime();
                registry::begin_wait(
                    task,
                    a,
                    timer::tick_count(),
                    Bytes { pointer: text, len },
                    b,
                );
                runtime::restore_runtime(f);
                0
            }
            14 => {
                let f = runtime::save_runtime();
                let ticks = registry::finish_wait(task, timer::tick_count(), result(a));
                runtime::restore_runtime(f);
                ticks
            }
            15 => u64::from(lifetime::finish_current(task, a != 0)),
            16 => {
                let f = runtime::save_runtime();
                let value = registry::assign_role(task, role(a as u8));
                runtime::restore_runtime(f);
                u64::from(value)
            }
            17 => u64::from(lifetime::pin(task)),
            18 => u64::from(lifetime::unpin(task)),
            19 => {
                let c = registry::core(task);
                if !(*c).uses_fpu {
                    (*registry::raw()).fpu_lazy_skips =
                        (*registry::raw()).fpu_lazy_skips.wrapping_add(1);
                    return 0;
                }
                if (*c).fpu_state_valid
                    && let Some(bytes) = (*c).fpu_state
                {
                    (*registry::raw()).fpu_lazy_saves =
                        (*registry::raw()).fpu_lazy_saves.wrapping_add(1);
                    fpu::r4os_rust_fpu_buffer(1, bytes.pointer.cast_mut(), bytes.len);
                }

                0
            }
            20 => {
                let c = registry::core(task);
                if (*c).uses_fpu
                    && (*c).fpu_state_valid
                    && let Some(bytes) = (*c).fpu_state
                {
                    fpu::r4os_rust_fpu_buffer(2, bytes.pointer.cast_mut(), bytes.len);
                }

                0
            }
            21 => u64::from(crate::sched::wait_node::detach(ptr::addr_of_mut!(
                (*registry::core(task)).wait_node
            ))),
            22 => {
                let f = runtime::save_runtime();
                let value = registry::remove_donation(task, a as u8);
                runtime::restore_runtime(f);
                u64::from(value)
            }
            23 => {
                let f = runtime::save_runtime();
                let c = registry::core(task);
                let accepted = a != 0
                    && registry::contains(task)
                    && (*c).state == State::Blocked
                    && (*c).execution_owner_context.is_null();
                if accepted {
                    (*c).execution_owner_kind = owner(a as u8);
                    (*c).execution_owner_context = ptr::with_exposed_provenance_mut(b as usize);
                }
                runtime::restore_runtime(f);
                u64::from(accepted)
            }
            24 => {
                if a >= registry::MAX_CPUS as u64
                    || percpu::r4os_rust_percpu_control(13, a as u32, 0, 0) == 0
                {
                    return 0;
                }
                let f = runtime::save_runtime();
                let c = registry::core(task);
                let accepted = registry::contains(task)
                    && (*c).state == State::Blocked
                    && (*c).smp_eligible
                    && !(*c).ready_linked
                    && !(*c).timeout_linked
                    && (*c).running_cpu == 0xff;
                if accepted {
                    (*c).home_cpu = a as u8;
                    (*c).home_cpu_bound = true;
                }
                runtime::restore_runtime(f);
                u64::from(accepted)
            }
            25 => {
                let f = runtime::save_runtime();
                let c = registry::core(task);
                let accepted = registry::contains(task)
                    && (*c).state == State::Blocked
                    && (*c).execution_owner_context.addr() == a as usize;
                if accepted {
                    (*c).execution_owner_kind = ExecutionOwnerKind::None;
                    (*c).execution_owner_context = null_mut();
                }
                runtime::restore_runtime(f);
                u64::from(accepted)
            }
            _ => panic!("Invalid private task mutation"),
        }
    }
}
/// # Safety
/// All handles/outputs retain their documented lifetime. Passive queue views
/// retain runtime admission; active release/reclaim has no borrowed state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_control(operation: u8, a: u64, b: u64) -> u64 {
    unsafe {
        match operation {
            0 => u64::from(resources::init()),
            1 => {
                let f = runtime::save_runtime();
                let v = (*registry::raw()).count;
                runtime::restore_runtime(f);
                v as u64
            }
            2 => {
                let f = runtime::save_runtime();
                let v = (*registry::raw()).epoch;
                runtime::restore_runtime(f);
                v
            }
            3 => {
                let f = runtime::save_runtime();
                let v = (*registry::raw()).peak.min(u32::MAX as usize);
                runtime::restore_runtime(f);
                v as u64
            }
            4 => (*registry::raw()).critical_in_use as u64,
            5 => (*registry::raw()).stack_cache_count as u64,
            6 => (*registry::raw()).stack_cache_hits,
            7 => (*registry::raw()).stack_cache_misses,
            8 => (*registry::raw()).stack_cache_reclaims,
            9 => (*registry::raw()).stack_cache_release_failures,
            10 => (*registry::raw()).fpu_lazy_saves,
            11 => (*registry::raw()).fpu_lazy_skips,
            12 => {
                let f = runtime::save_runtime();
                let v = (*registry::raw()).kill_held_lock_deferrals;
                runtime::restore_runtime(f);
                v
            }
            13 => (*registry::raw()).recycle_guard_hits,
            14 => {
                let f = runtime::save_runtime();
                let v = (*registry::raw()).rollback_count.min(u32::MAX as usize);
                runtime::restore_runtime(f);
                v as u64
            }
            15 => {
                let t = (*registry::raw()).timeout_head;
                if t.is_null() {
                    crate::platform::timer::NO_DEADLINE
                } else {
                    (*registry::core(t)).wake_tick
                }
            }
            16 => u64::from(lifetime::reap()),
            17 => u64::from(resources::reclaim_cache(a as u32)),
            18..=20 => {
                let f = runtime::save_runtime();
                let t = if operation == 18 {
                    registry::find_id(a as u32)
                } else {
                    registry::find_identity(a as u32, b)
                };
                let v = !t.is_null()
                    && (*registry::core(t)).state != State::Unused
                    && (operation == 20 || (*registry::core(t)).state != State::Dead);
                runtime::restore_runtime(f);
                u64::from(v)
            }
            21 => (*registry::raw()).head.addr() as u64,
            22 => (*registry::queue(a as u32)).head.addr() as u64,
            23 => (*registry::queue(a as u32)).count as u64,
            24 => (*registry::queue(a as u32)).ranks[b as usize].first.addr() as u64,
            25 => {
                let q = registry::queue(a as u32);
                let task = if !(*q).warning_cursor.is_null() {
                    (*q).warning_cursor
                } else {
                    (*q).head
                };
                if task.is_null() {
                    return 0;
                }
                let next = (*registry::core(task)).ready_next;
                (*q).warning_cursor = if next.is_null() { (*q).head } else { next };
                task.addr() as u64
            }
            26 => (*registry::raw())
                .ready
                .iter()
                .map(|q| q.count as u64)
                .sum(),
            27 => (*registry::raw()).timeout_head.addr() as u64,
            28 => (*registry::raw()).timeout_count as u64,
            29 => ((*registry::raw()).reap_count + (*registry::raw()).rollback_count) as u64,
            30 => u64::from(resources::arm_failure(match a {
                0 => CreateFailure::None,
                1 => CreateFailure::TaskMetadata,
                2 => CreateFailure::Stack,
                3 => CreateFailure::Fpu,
                4 => CreateFailure::Waiter,
                5 => CreateFailure::Memory,
                _ => panic!("Invalid task failure"),
            })),
            31 => {
                let q = registry::queue(a as u32);
                let mut yes = false;
                for rank in 0..(b as usize).min(registry::ROLE_COUNT) {
                    yes |= (*q).dispatch_counts[rank] != 0;
                }
                u64::from(yes)
            }
            32 => {
                if a == 0 || a >= registry::MAX_CPUS as u64 {
                    return 0;
                }
                let mut failure = CreateFailure::None;
                let t = resources::create(
                    resources::CreateArgs {
                        name: Bytes {
                            pointer: b"cpu-idle".as_ptr(),
                            len: 8,
                        },
                        state: State::Blocked,
                        needs_fpu: false,
                        entry: None,
                        role: Role::Batch,
                        smp_eligible: false,
                        smp_r4x: false,
                        owner_kind: ExecutionOwnerKind::None,
                        owner_context: null_mut(),
                    },
                    &mut failure,
                );
                if !t.is_null() {
                    let f = runtime::save_runtime();
                    (*registry::core(t)).home_cpu = a as u8;
                    (*registry::core(t)).cpu_idle = true;
                    runtime::restore_runtime(f);
                }
                t.addr() as u64
            }
            33 => {
                let f = runtime::save_runtime();
                let mut count = 0;
                let mut t = (*registry::raw()).head;
                while !t.is_null() {
                    let c = registry::core(t);
                    if !matches!((*c).state, State::Unused | State::Dead) && (*c).fpu_state_valid {
                        count += 1;
                    }
                    t = (*c).registry_next;
                }
                runtime::restore_runtime(f);
                count
            }
            34 => {
                resources::note_failure(match a {
                    0 => CreateFailure::None,
                    1 => CreateFailure::TaskMetadata,
                    2 => CreateFailure::Stack,
                    3 => CreateFailure::Fpu,
                    4 => CreateFailure::Waiter,
                    5 => CreateFailure::Memory,
                    _ => panic!("Invalid task failure"),
                });
                0
            }
            _ => panic!("Invalid private task control"),
        }
    }
}
/// # Safety
/// Identity operations resolve the exact registry generation before mutation.
/// No raw descriptor can be freed while its owner/pin/release token is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_identity(
    operation: u8,
    id: u32,
    generation: u64,
    context: *mut c_void,
    argument: u64,
) -> u64 {
    unsafe {
        match operation {
            0 => lifetime::pin_identity(id, Some(generation)).addr() as u64,
            1 => lifetime::pin_identity(id, None).addr() as u64,
            2 => u64::from(lifetime::kill(id, None)),
            3 => u64::from(lifetime::kill(id, Some(generation))),
            4 => u64::from(lifetime::release_dead(id, None)),
            5 => u64::from(lifetime::release_dead(id, Some(generation))),
            6 => lifetime::retire(id, generation) as u64,
            7 => {
                let f = runtime::save_runtime();
                let yes = registry::donate(id, generation, argument as u8);
                runtime::restore_runtime(f);
                u64::from(yes)
            }
            8 => {
                let f = runtime::save_runtime();
                let t = registry::find_identity(id, generation);
                let mut accepted = false;
                if !t.is_null() {
                    let c = registry::core(t);
                    accepted = (*c).state == State::Blocked
                        && !(*c).ready_linked
                        && !(*c).timeout_linked
                        && (*c).running_cpu == 0xff
                        && (*c).execution_owner_kind == ExecutionOwnerKind::DriverThread
                        && (*c).execution_owner_context == context
                        && (*c).unwind_guard_count == 1
                        && (*c).held_lock_count == 0;
                    if accepted {
                        (*c).unwind_guard_count = 0;
                        (*c).execution_owner_kind = ExecutionOwnerKind::None;
                        (*c).execution_owner_context = null_mut();
                    }
                }
                runtime::restore_runtime(f);
                u64::from(accepted)
            }
            _ => panic!("Invalid private task identity operation"),
        }
    }
}
/// # Safety
/// Exclusive initialized C output; opaque input retains exact task lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_snapshot(
    operation: u8,
    argument: u64,
    output: *mut c_void,
    _len: usize,
) -> u64 {
    unsafe {
        match operation {
            0 => {
                output
                    .cast::<snapshots::Summary>()
                    .write(snapshots::summary());
                0
            }
            1 => {
                output
                    .cast::<resources::FailureStats>()
                    .write(resources::failure_stats());
                0
            }
            2 => {
                output
                    .cast::<resources::StackTelemetry>()
                    .write(resources::telemetry_for(role(argument as u8)));
                0
            }
            3 => {
                output
                    .cast::<snapshots::QueueSnapshot>()
                    .write(snapshots::queues());
                0
            }
            4 => snapshots::pin_ordinal(argument as usize).addr() as u64,
            5 => snapshots::ordinal(ptr::with_exposed_provenance(argument as usize))
                .map_or(u64::MAX, |n| n as u64),
            _ => panic!("Invalid private task snapshot"),
        }
    }
}
use core::mem::size_of;
const _: () = {
    assert!(size_of::<CreateRequest>() == 40);
    assert!(size_of::<Span>() == 16);
};

#[repr(C)]
pub struct Pair {
    pub low: u64,
    pub high: u64,
}
/// # Safety
/// Exact live task under existing admission; exclusive fixed C pair output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_sequence(
    task: *mut c_void,
    timeout: bool,
    output: *mut Pair,
) {
    unsafe {
        let c = registry::core(task.cast());
        let value = if timeout {
            (*c).timeout_sequence
        } else {
            (*c).ready_order
        };
        output.write(Pair {
            low: value as u64,
            high: (value >> 64) as u64,
        });
    }
}
/// # Safety
/// Exact live task and exclusive four-u16 C output under caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_donation_counts(task: *mut c_void, output: *mut [u16; 4]) {
    unsafe {
        output.write((*registry::core(task.cast())).inherited_rank_counts);
    }
}
/// # Safety
/// Copied span retains the prior borrowed-name/buffer ownership contract;
/// caller holds exact task admission and never replaces live FPU ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_set_span(
    task: *mut c_void,
    field: u8,
    span: Span,
    present: bool,
) {
    unsafe {
        let c = registry::core(task.cast());
        let bytes = Bytes {
            pointer: span.pointer,
            len: span.len,
        };
        match field {
            30 => (*c).name = bytes,
            35 => (*c).wait_reason = bytes,
            84 => (*c).fpu_state = if present { Some(bytes) } else { None },
            _ => panic!("Task field is not a writable span"),
        }
    }
}
/// # Safety
/// Exact live task and read admission; computed rank retains native budgets.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_rank(task: *mut c_void, wake: bool) -> u8 {
    unsafe {
        if wake {
            registry::wake_rank(task.cast())
        } else {
            registry::dispatch_rank(task.cast())
        }
    }
}
/// # Safety
/// Exact retained task lifetime/admission. Only the resident task-owned RSP or
/// unwind counter slot is lent; the C caller cannot substitute another owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_slot(task: *mut c_void, unwind: bool) -> *mut c_void {
    unsafe {
        let c = registry::core(task.cast());
        if unwind {
            ptr::addr_of_mut!((*c).unwind_guard_count).cast()
        } else {
            ptr::addr_of_mut!((*c).rsp).cast()
        }
    }
}

/// # Safety
/// Caller retains exact live opaque task and existing admission. This scalar
/// projection has no relationship to native field offsets or representation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_field(task: *mut c_void, field: u8) -> u64 {
    unsafe {
        let c = registry::core(task.cast());
        match field {
            0 => (*c).registry_prev.addr() as u64,
            1 => (*c).registry_next.addr() as u64,
            2 => (*c).ready_prev.addr() as u64,
            3 => (*c).ready_next.addr() as u64,
            4 => (*c).ready_linked as u64,
            6 => (*c).ready_rank as u64,
            8 => (*c).timeout_prev.addr() as u64,
            9 => (*c).timeout_next.addr() as u64,
            10 => (*c).timeout_linked as u64,
            13 => (*c).reap_prev.addr() as u64,
            14 => (*c).reap_next.addr() as u64,
            15 => (*c).reap_linked as u64,
            16 => (*c).generation,
            17 => (*c).pin_count as u64,
            18 => (*c).retire_pending as u64,
            19 => (*c).release_in_progress as u64,
            20 => (*c).rollback_pending as u64,
            21 => (*c).rollback_next.addr() as u64,
            22 => (*c).critical_reserve_slot as u64,
            23 => (*c).id as u64,
            24 => (*c).home_cpu as u64,
            25 => (*c).home_cpu_bound as u64,
            26 => (*c).running_cpu as u64,
            27 => (*c).smp_eligible as u64,
            28 => (*c).smp_r4x_work as u64,
            29 => (*c).cpu_idle as u64,
            31 => u64::from(ptr::addr_of!((*c).state).cast::<u8>().read()),
            32 => (*c).rsp,
            33 => (*c).wake_tick,
            34 => (*c).blocked_since_tick,
            36 => (*c).wait_object,
            37 => (*c).wait_result as u64,
            38 => ptr::addr_of_mut!((*c).wait_node).addr() as u64,
            39 => (*c).held_lock_count as u64,
            40 => (*c).unwind_guard_count as u64,
            41 => (*c).wait_handoff_guard_pending as u64,
            43 => (*c).stack_base,
            44 => (*c).stack_top,
            45 => (*c).stack_range_id as u64,
            46 => (*c).stack_high_water_bytes,
            47 => (*c).stack_high_water_measured as u64,
            48 => (*c).role as u64,
            49 => (*c).priority as u64,
            50 => (*c).role_budget_ticks_remaining as u64,
            51 => (*c).role_budget_dispatches_remaining as u64,
            52 => (*c).role_budget_exhausted_recorded as u64,
            54 => (*c).donation_budget_ticks_remaining as u64,
            55 => (*c).donation_budget_dispatches_remaining as u64,
            56 => (*c).donation_budget_exhausted_recorded as u64,
            57 => (*c).role_activations,
            58 => (*c).role_budget_exhaustions,
            59 => (*c).donation_budget_exhaustions,
            60 => (*c).execution_owner_kind as u64,
            61 => (*c).execution_owner_context.addr() as u64,
            62 => (*c).entry.map_or(0, |f| f as *const () as u64),
            63 => (*c).created_tick,
            64 => (*c).ready_since_tick,
            65 => (*c).last_scheduled_tick,
            66 => (*c).last_accounted_tick,
            67 => (*c).last_yield_tick,
            68 => (*c).last_ready_latency_ticks,
            69 => (*c).max_ready_latency_ticks,
            70 => (*c).last_wait_ticks,
            71 => (*c).max_wait_ticks,
            72 => (*c).run_ticks,
            73 => (*c).switches_in,
            74 => (*c).max_run_without_switch_ticks,
            75 => (*c).preempt_disable_depth as u64,
            76 => (*c).preempt_disable_max_depth as u64,
            77 => (*c).preemption_probe_hits,
            78 => (*c).preemption_deferred_ticks,
            79 => (*c).max_preemption_deferred_ticks,
            80 => (*c).long_run_warnings,
            81 => (*c).starvation_warnings,
            82 => (*c).last_long_run_warning_tick,
            83 => (*c).last_starvation_warning_tick,
            85 => (*c).fpu_state_bytes as u64,
            86 => (*c).fpu_state_valid as u64,
            87 => (*c).uses_fpu as u64,
            _ => panic!("Task field requires typed projection"),
        }
    }
}
/// # Safety
/// Exact task lifetime, existing field mutation admission, and immutable-ID
/// contract are retained by the trusted kernel caller. No task layout crosses C.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_set_field(task: *mut c_void, field: u8, value: u64) {
    unsafe {
        let c = registry::core(task.cast());
        match field {
            0 => {
                (*c).registry_prev = ptr::with_exposed_provenance_mut(value as usize);
            }
            1 => {
                (*c).registry_next = ptr::with_exposed_provenance_mut(value as usize);
            }
            2 => {
                (*c).ready_prev = ptr::with_exposed_provenance_mut(value as usize);
            }
            3 => {
                (*c).ready_next = ptr::with_exposed_provenance_mut(value as usize);
            }
            4 => {
                (*c).ready_linked = value != 0;
            }
            6 => {
                (*c).ready_rank = value as u8;
            }
            8 => {
                (*c).timeout_prev = ptr::with_exposed_provenance_mut(value as usize);
            }
            9 => {
                (*c).timeout_next = ptr::with_exposed_provenance_mut(value as usize);
            }
            10 => {
                (*c).timeout_linked = value != 0;
            }
            13 => {
                (*c).reap_prev = ptr::with_exposed_provenance_mut(value as usize);
            }
            14 => {
                (*c).reap_next = ptr::with_exposed_provenance_mut(value as usize);
            }
            15 => {
                (*c).reap_linked = value != 0;
            }
            16 => {
                (*c).generation = value;
            }
            17 => {
                (*c).pin_count = value as u32;
            }
            18 => {
                (*c).retire_pending = value != 0;
            }
            19 => {
                (*c).release_in_progress = value != 0;
            }
            20 => {
                (*c).rollback_pending = value != 0;
            }
            21 => {
                (*c).rollback_next = ptr::with_exposed_provenance_mut(value as usize);
            }
            22 => {
                (*c).critical_reserve_slot = value as u8;
            }
            23 => {
                (*c).id = value as u32;
            }
            24 => {
                (*c).home_cpu = value as u8;
            }
            25 => {
                (*c).home_cpu_bound = value != 0;
            }
            26 => {
                (*c).running_cpu = value as u8;
            }
            27 => {
                (*c).smp_eligible = value != 0;
            }
            28 => {
                (*c).smp_r4x_work = value != 0;
            }
            29 => {
                (*c).cpu_idle = value != 0;
            }
            31 => {
                (*c).state = state(value as u8);
            }
            32 => {
                (*c).rsp = value;
            }
            33 => {
                (*c).wake_tick = value;
            }
            34 => {
                (*c).blocked_since_tick = value;
            }
            36 => {
                (*c).wait_object = value;
            }
            37 => {
                (*c).wait_result = result(value);
            }
            39 => {
                (*c).held_lock_count = value as u32;
            }
            40 => {
                (*c).unwind_guard_count = value as u32;
            }
            41 => {
                (*c).wait_handoff_guard_pending = value != 0;
            }
            43 => {
                (*c).stack_base = value;
            }
            44 => {
                (*c).stack_top = value;
            }
            45 => {
                (*c).stack_range_id = value as u32;
            }
            46 => {
                (*c).stack_high_water_bytes = value;
            }
            47 => {
                (*c).stack_high_water_measured = value != 0;
            }
            48 => {
                (*c).role = role(value as u8);
            }
            49 => {
                (*c).priority = match value {
                    0 => Priority::High,
                    1 => Priority::Normal,
                    2 => Priority::Low,
                    _ => panic!("Invalid task priority"),
                };
            }
            50 => {
                (*c).role_budget_ticks_remaining = value as u32;
            }
            51 => {
                (*c).role_budget_dispatches_remaining = value as u16;
            }
            52 => {
                (*c).role_budget_exhausted_recorded = value != 0;
            }
            54 => {
                (*c).donation_budget_ticks_remaining = value as u32;
            }
            55 => {
                (*c).donation_budget_dispatches_remaining = value as u16;
            }
            56 => {
                (*c).donation_budget_exhausted_recorded = value != 0;
            }
            57 => {
                (*c).role_activations = value;
            }
            58 => {
                (*c).role_budget_exhaustions = value;
            }
            59 => {
                (*c).donation_budget_exhaustions = value;
            }
            60 => {
                (*c).execution_owner_kind = owner(value as u8);
            }
            61 => {
                (*c).execution_owner_context = ptr::with_exposed_provenance_mut(value as usize);
            }
            63 => {
                (*c).created_tick = value;
            }
            64 => {
                (*c).ready_since_tick = value;
            }
            65 => {
                (*c).last_scheduled_tick = value;
            }
            66 => {
                (*c).last_accounted_tick = value;
            }
            67 => {
                (*c).last_yield_tick = value;
            }
            68 => {
                (*c).last_ready_latency_ticks = value;
            }
            69 => {
                (*c).max_ready_latency_ticks = value;
            }
            70 => {
                (*c).last_wait_ticks = value;
            }
            71 => {
                (*c).max_wait_ticks = value;
            }
            72 => {
                (*c).run_ticks = value;
            }
            73 => {
                (*c).switches_in = value;
            }
            74 => {
                (*c).max_run_without_switch_ticks = value;
            }
            75 => {
                (*c).preempt_disable_depth = value as u32;
            }
            76 => {
                (*c).preempt_disable_max_depth = value as u32;
            }
            77 => {
                (*c).preemption_probe_hits = value;
            }
            78 => {
                (*c).preemption_deferred_ticks = value;
            }
            79 => {
                (*c).max_preemption_deferred_ticks = value;
            }
            80 => {
                (*c).long_run_warnings = value;
            }
            81 => {
                (*c).starvation_warnings = value;
            }
            82 => {
                (*c).last_long_run_warning_tick = value;
            }
            83 => {
                (*c).last_starvation_warning_tick = value;
            }
            85 => {
                (*c).fpu_state_bytes = value as u32;
            }
            86 => {
                (*c).fpu_state_valid = value != 0;
            }
            87 => {
                (*c).uses_fpu = value != 0;
            }
            _ => panic!("Task field is not scalar writable"),
        }
    }
}
/// # Safety
/// Exact task lifetime and exclusive C Span output; borrowed bytes retain
/// their old owner lifetime. The return bit distinguishes missing FPU storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_span(
    task: *mut c_void,
    field: u8,
    output: *mut Span,
) -> bool {
    unsafe {
        let c = registry::core(task.cast());
        match field {
            30 => {
                let bytes = (*c).name;
                output.write(Span {
                    pointer: bytes.pointer,
                    len: bytes.len,
                });
                true
            }
            35 => {
                let bytes = (*c).wait_reason;
                output.write(Span {
                    pointer: bytes.pointer,
                    len: bytes.len,
                });
                true
            }
            84 => {
                if let Some(bytes) = (*c).fpu_state {
                    output.write(Span {
                        pointer: bytes.pointer,
                        len: bytes.len,
                    });
                    true
                } else {
                    output.write(Span {
                        pointer: ptr::null(),
                        len: 0,
                    });
                    false
                }
            }
            _ => panic!("Task field is not a span"),
        }
    }
}

/// # Safety
/// Exclusive initialized C row/page buffers; lengths describe actual storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_inventory(
    after: u64,
    output: *mut snapshots::InventoryRow,
    len: usize,
    page: *mut snapshots::InventoryPage,
) {
    unsafe {
        let rows = if len == 0 {
            &mut []
        } else {
            core::slice::from_raw_parts_mut(output, len)
        };
        page.write(snapshots::inventory_page(after, rows));
    }
}
/// # Safety
/// Exclusive aligned fixed C Deadman output; snapshot takes runtime admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_deadman(
    now: u64,
    threshold: u64,
    output: *mut snapshots::Deadman,
) {
    unsafe {
        output.write(snapshots::deadman(now, threshold));
    }
}
