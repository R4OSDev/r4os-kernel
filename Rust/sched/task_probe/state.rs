//! Original explicit diagnostic loads, never registry capacities. The test
//! caller and all probe workers are BSP-owned; no Rust borrow spans dispatch.
use crate::sched::{
    sync_types::{Event, Mutex, Text, WaitQueue},
    task::task_types::Task,
};
use core::{cell::UnsafeCell, ptr::null_mut};
pub const WORKERS: usize = 160;
pub const RUNNABLE_END: usize = 24;
pub const SLEEP_END: usize = 48;
pub const SIGNAL_END: usize = 80;
pub const CANCEL_END: usize = 112;
pub const TIMEOUT_END: usize = 144;
pub const CHURN: usize = 10_000;
pub const WAIT_BUDGET: usize = 20_000;
pub const NETWORK_PROGRESS: usize = 64;
pub const WAKE_BAR: u64 = 3;
pub const EARLY_TICKS: u64 = 4;
pub const LONG_TICKS: u64 = 20;
pub const IRQ_BAR: u64 = 3;
pub const LATENESS_BAR: u64 = 3;
pub const STORM: usize = 65;
pub const DIRECTED: usize = 4;
pub const BUDGET: usize = 3;
pub const INPUT_TURNS: u32 = 16;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Runnable,
    Sleeping,
    Signal,
    Cancel,
    Timeout,
    Kill,
}
pub struct Probe {
    pub tasks: [*mut Task; WORKERS],
    pub ids: [u32; WORKERS],
    pub generations: [u64; WORKERS],
    pub started: u32,
    pub failures: u32,
    pub signal_results: u32,
    pub cancel_results: u32,
    pub timeout_results: u32,
    pub release_workers: bool,
    pub churn_completed: u32,
    pub short_completed: u32,
    pub wake_entered: bool,
    pub wake_completed: bool,
    pub wake_latency: u64,
    pub deadline_elapsed: u64,
    pub deadline_irqs: u64,
    pub deadline_total_irqs: u64,
    pub deadline_lateness: u64,
    pub deadline_checkpointed: bool,
    pub storm_batches: u32,
    pub directed_tasks: [*mut Task; DIRECTED],
    pub directed_ids: [u32; DIRECTED],
    pub directed_generations: [u64; DIRECTED],
    pub directed_results: u32,
    pub directed_failures: u32,
    pub budget_tasks: [*mut Task; BUDGET],
    pub budget_ids: [u32; BUDGET],
    pub budget_generations: [u64; BUDGET],
    pub budget_turns: [u32; BUDGET],
    pub budget_stop: bool,
    pub inversion_mutex: Mutex,
    pub inversion_owned: bool,
    pub inversion_release: bool,
    pub inversion_holder_done: bool,
    pub inversion_waiter_done: bool,
    pub inversion_acquired: bool,
    pub signal_event: Event,
    pub cancel_queue: WaitQueue,
    pub timeout_queue: WaitQueue,
    pub kill_queue: WaitQueue,
    pub finish_queue: WaitQueue,
    pub wake_event: Event,
    pub directed_queue: WaitQueue,
}
impl Probe {
    pub const EMPTY: Self = Self {
        tasks: [null_mut(); WORKERS],
        ids: [0; WORKERS],
        generations: [0; WORKERS],
        started: 0,
        failures: 0,
        signal_results: 0,
        cancel_results: 0,
        timeout_results: 0,
        release_workers: false,
        churn_completed: 0,
        short_completed: 0,
        wake_entered: false,
        wake_completed: false,
        wake_latency: 0,
        deadline_elapsed: 0,
        deadline_irqs: 0,
        deadline_total_irqs: 0,
        deadline_lateness: 0,
        deadline_checkpointed: false,
        storm_batches: 0,
        directed_tasks: [null_mut(); DIRECTED],
        directed_ids: [0; DIRECTED],
        directed_generations: [0; DIRECTED],
        directed_results: 0,
        directed_failures: 0,
        budget_tasks: [null_mut(); BUDGET],
        budget_ids: [0; BUDGET],
        budget_generations: [0; BUDGET],
        budget_turns: [0; BUDGET],
        budget_stop: false,
        inversion_mutex: Mutex {
            owner: 0,
            owner_generation: 0,
            depth: 0,
            rank: 10,
            mode: 0,
            name: Text::literal(b"taskreg-role-inversion"),
            queue: WaitQueue::EMPTY,
            donation_rank: 255,
            handoff_pending: false,
        },
        inversion_owned: false,
        inversion_release: false,
        inversion_holder_done: false,
        inversion_waiter_done: false,
        inversion_acquired: false,
        signal_event: Event {
            signaled: false,
            mode: 0,
            queue: WaitQueue::EMPTY,
        },
        cancel_queue: WaitQueue::EMPTY,
        timeout_queue: WaitQueue::EMPTY,
        kill_queue: WaitQueue::EMPTY,
        finish_queue: WaitQueue::EMPTY,
        wake_event: Event {
            signaled: false,
            mode: 0,
            queue: WaitQueue::EMPTY,
        },
        directed_queue: WaitQueue::EMPTY,
    };
}
struct Storage(UnsafeCell<Probe>);
// SAFETY: BSP-only probe orchestration/worker admissions own this original
// diagnostic state. No Rust reference survives any yield or callback.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Probe::EMPTY));
pub fn raw() -> *mut Probe {
    STATE.0.get()
}
/// # Safety
/// Every old probe Task is retired and queues drained before reset.
pub unsafe fn reset() {
    unsafe { raw().write(Probe::EMPTY) }
}
pub fn kind(index: usize) -> Kind {
    if index < RUNNABLE_END {
        Kind::Runnable
    } else if index < SLEEP_END {
        Kind::Sleeping
    } else if index < SIGNAL_END {
        Kind::Signal
    } else if index < CANCEL_END {
        Kind::Cancel
    } else if index < TIMEOUT_END {
        Kind::Timeout
    } else {
        Kind::Kill
    }
}
