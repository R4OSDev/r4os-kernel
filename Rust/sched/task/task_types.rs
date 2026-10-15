//! Canonical native task types; no native layout crosses C.
use crate::sched::{task_index::Links, wait_node::Node};
use core::{cell::UnsafeCell, ffi::c_void, ptr::null_mut};

#[derive(Clone, Copy)]
pub struct Bytes {
    pub pointer: *const u8,
    pub len: usize,
}
impl Default for Bytes {
    fn default() -> Self {
        Self {
            pointer: core::ptr::NonNull::<u8>::dangling().as_ptr(),
            len: 0,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct HeldLockRecord {
    pub object_id: u64,
    pub name: Bytes,
    pub rank: u16,
    pub mode_no_sleep: bool,
    pub active: bool,
}
pub type Entry = unsafe extern "C" fn();

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Unused = 0,
    Ready = 1,
    Running = 2,
    Blocked = 3,
    Dead = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High = 0,
    Normal = 1,
    Low = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Input = 0,
    ShortCompletion = 1,
    Interactive = 2,
    Batch = 3,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    None = 0,
    Signaled = 1,
    Timeout = 2,
    Cancelled = 3,
    Killed = 4,
    Failed = 5,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOwnerKind {
    None = 0,
    ProgramThread = 1,
    AsyncIo = 2,
    DriverThread = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CreateFailure {
    None = 0,
    TaskMetadata = 1,
    Stack = 2,
    Fpu = 3,
    Waiter = 4,
    Memory = 5,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RetireResult {
    Gone = 0,
    Pending = 1,
    Released = 2,
}

pub struct Task {
    pub core: UnsafeCell<TaskCore>,
}

pub struct TaskCore {
    pub registry_prev: *mut Task,
    pub registry_next: *mut Task,
    pub ready_prev: *mut Task,
    pub ready_next: *mut Task,
    pub ready_linked: bool,
    pub ready_order: u128,
    pub ready_rank: u8,
    pub ready_index: Links,
    pub timeout_prev: *mut Task,
    pub timeout_next: *mut Task,
    pub timeout_linked: bool,
    pub timeout_sequence: u128,
    pub timeout_index: Links,
    pub reap_prev: *mut Task,
    pub reap_next: *mut Task,
    pub reap_linked: bool,
    pub generation: u64,
    pub pin_count: u32,
    pub retire_pending: bool,
    pub release_in_progress: bool,
    pub rollback_pending: bool,
    pub rollback_next: *mut Task,
    pub critical_reserve_slot: u8,
    pub id: u32,
    pub home_cpu: u8,
    pub home_cpu_bound: bool,
    pub running_cpu: u8,
    pub smp_eligible: bool,
    pub smp_r4x_work: bool,
    pub cpu_idle: bool,
    pub name: Bytes,
    pub state: State,
    pub rsp: u64,
    pub wake_tick: u64,
    pub blocked_since_tick: u64,
    pub wait_reason: Bytes,
    pub wait_object: u64,
    pub wait_result: WaitResult,
    pub wait_node: Node,
    pub held_lock_count: u32,
    pub unwind_guard_count: u32,
    pub wait_handoff_guard_pending: bool,
    pub held_locks: [HeldLockRecord; 8],
    pub stack_base: u64,
    pub stack_top: u64,
    pub stack_range_id: u32,
    pub stack_high_water_bytes: u64,
    pub stack_high_water_measured: bool,
    pub role: Role,
    pub priority: Priority,
    pub role_budget_ticks_remaining: u32,
    pub role_budget_dispatches_remaining: u16,
    pub role_budget_exhausted_recorded: bool,
    pub inherited_rank_counts: [u16; 4],
    pub donation_budget_ticks_remaining: u32,
    pub donation_budget_dispatches_remaining: u16,
    pub donation_budget_exhausted_recorded: bool,
    pub role_activations: u64,
    pub role_budget_exhaustions: u64,
    pub donation_budget_exhaustions: u64,
    pub execution_owner_kind: ExecutionOwnerKind,
    pub execution_owner_context: *mut c_void,
    pub entry: Option<Entry>,
    pub created_tick: u64,
    pub ready_since_tick: u64,
    pub last_scheduled_tick: u64,
    pub last_accounted_tick: u64,
    pub last_yield_tick: u64,
    pub last_ready_latency_ticks: u64,
    pub max_ready_latency_ticks: u64,
    pub last_wait_ticks: u64,
    pub max_wait_ticks: u64,
    pub run_ticks: u64,
    pub switches_in: u64,
    pub max_run_without_switch_ticks: u64,
    pub preempt_disable_depth: u32,
    pub preempt_disable_max_depth: u32,
    pub preemption_probe_hits: u64,
    pub preemption_deferred_ticks: u64,
    pub max_preemption_deferred_ticks: u64,
    pub long_run_warnings: u64,
    pub starvation_warnings: u64,
    pub last_long_run_warning_tick: u64,
    pub last_starvation_warning_tick: u64,
    pub fpu_state: Option<Bytes>,
    pub fpu_state_bytes: u32,
    pub fpu_state_valid: bool,
    pub uses_fpu: bool,
}

impl Default for TaskCore {
    fn default() -> Self {
        Self {
            registry_prev: null_mut(),
            registry_next: null_mut(),
            ready_prev: null_mut(),
            ready_next: null_mut(),
            ready_linked: false,
            ready_order: 0,
            ready_rank: 0,
            ready_index: Links::default(),
            timeout_prev: null_mut(),
            timeout_next: null_mut(),
            timeout_linked: false,
            timeout_sequence: 0,
            timeout_index: Links::default(),
            reap_prev: null_mut(),
            reap_next: null_mut(),
            reap_linked: false,
            generation: 0,
            pin_count: 0,
            retire_pending: false,
            release_in_progress: false,
            rollback_pending: false,
            rollback_next: null_mut(),
            critical_reserve_slot: 0xff,
            id: 0,
            home_cpu: 0,
            home_cpu_bound: false,
            running_cpu: 0xFF,
            smp_eligible: false,
            smp_r4x_work: false,
            cpu_idle: false,
            name: Bytes::default(),
            state: State::Unused,
            rsp: 0,
            wake_tick: 0,
            blocked_since_tick: 0,
            wait_reason: Bytes::default(),
            wait_object: 0,
            wait_result: WaitResult::None,
            wait_node: Node::default(),
            held_lock_count: 0,
            unwind_guard_count: 0,
            wait_handoff_guard_pending: false,
            held_locks: [HeldLockRecord::default(); 8],
            stack_base: 0,
            stack_top: 0,
            stack_range_id: 0,
            stack_high_water_bytes: 0,
            stack_high_water_measured: false,
            role: Role::Interactive,
            priority: Priority::Normal,
            role_budget_ticks_remaining: 0,
            role_budget_dispatches_remaining: 0,
            role_budget_exhausted_recorded: false,
            inherited_rank_counts: [0; 4],
            donation_budget_ticks_remaining: 0,
            donation_budget_dispatches_remaining: 0,
            donation_budget_exhausted_recorded: false,
            role_activations: 0,
            role_budget_exhaustions: 0,
            donation_budget_exhaustions: 0,
            execution_owner_kind: ExecutionOwnerKind::None,
            execution_owner_context: null_mut(),
            entry: None,
            created_tick: 0,
            ready_since_tick: 0,
            last_scheduled_tick: 0,
            last_accounted_tick: 0,
            last_yield_tick: 0,
            last_ready_latency_ticks: 0,
            max_ready_latency_ticks: 0,
            last_wait_ticks: 0,
            max_wait_ticks: 0,
            run_ticks: 0,
            switches_in: 0,
            max_run_without_switch_ticks: 0,
            preempt_disable_depth: 0,
            preempt_disable_max_depth: 0,
            preemption_probe_hits: 0,
            preemption_deferred_ticks: 0,
            max_preemption_deferred_ticks: 0,
            long_run_warnings: 0,
            starvation_warnings: 0,
            last_long_run_warning_tick: 0,
            last_starvation_warning_tick: 0,
            fpu_state: None,
            fpu_state_bytes: 0,
            fpu_state_valid: false,
            uses_fpu: false,
        }
    }
}
