//! Native synchronization records. Exact task/runtime owners retain all
//! mutation and task state; ABI projections contain only fixed values.
use crate::sched::wait_node::QueueCore;
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub pointer: *const u8,
    pub len: usize,
}
impl Text {
    pub const fn literal(s: &'static [u8]) -> Self {
        Self {
            pointer: s.as_ptr(),
            len: s.len(),
        }
    }
}
#[repr(C)]
#[derive(Default)]
pub struct WaitQueue {
    pub core: QueueCore,
}
#[repr(C)]
pub struct Event {
    pub signaled: bool,
    pub mode: u8,
    pub queue: WaitQueue,
}
#[repr(C)]
pub struct Completion {
    pub completed: u32,
    pub queue: WaitQueue,
}
#[repr(C)]
pub struct Semaphore {
    pub count: u32,
    pub max_count: u32,
    pub queue: WaitQueue,
}
#[repr(C)]
pub struct Mutex {
    pub owner: u32,
    pub owner_generation: u64,
    pub depth: u32,
    pub rank: u16,
    pub mode: u8,
    pub name: Text,
    pub queue: WaitQueue,
    pub donation_rank: u8,
    pub handoff_pending: bool,
    // Private kernel record. The held-lock count retains this exact owner;
    // waiter nodes retain this mutex until their PI edge has been removed.
    pub(crate) owner_task: *mut super::task::task_types::Task,
    pub(crate) waiter_ranks: [usize; 4],
}
#[repr(C)]
pub struct UnwindGuard {
    pub owner: u32,
    pub owner_generation: u64,
    pub boot_owner: bool,
    pub depth: u32,
    pub name: Text,
    pub queue: WaitQueue,
    pub(crate) owner_task: *mut super::task::task_types::Task,
    pub(crate) waiter_ranks: [usize; 4],
    pub(crate) donation_rank: u8,
    pub(crate) handoff_epoch: u64,
}
pub type Predicate = unsafe extern "C" fn(*mut c_void) -> bool;
pub type Release = unsafe extern "C" fn(*mut c_void);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WaitRequest {
    pub timeout: u64,
    pub reason: Text,
    pub predicate: Option<Predicate>,
    pub context: *mut c_void,
    pub release: Option<Release>,
    pub release_context: *mut c_void,
    pub flags: u64,
    pub consume_runtime: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WaitSummary {
    pub queue_waits: u64,
    pub wake_one: u64,
    pub wake_all: u64,
    pub timeouts: u64,
    pub cancellations: u64,
    pub drops: u64,
    pub total_wait_ticks: u64,
    pub max_wait_ticks: u64,
    pub last_wait_ticks: u64,
    pub directed_wakes: u64,
    pub directed_scans: u64,
    pub directed_fifo_bypasses: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LockSummary {
    pub acquires: u64,
    pub releases: u64,
    pub recursive_acquires: u64,
    pub contention_waits: u64,
    pub contention_timeouts: u64,
    pub order_violations: u64,
    pub sleep_checks: u64,
    pub sleep_under_lock: u64,
    pub sleep_under_no_sleep_lock: u64,
    pub unlock_mismatches: u64,
    pub held_slots_used: u32,
    pub current_depth: u32,
    pub max_depth: u32,
    pub tracking_drops: u32,
    pub role_inversions: u64,
    pub role_donations: u64,
    pub role_donation_releases: u64,
}

impl WaitQueue {
    pub const EMPTY: Self = Self {
        core: QueueCore {
            head: core::ptr::null_mut(),
            tail: core::ptr::null_mut(),
            count: 0,
            closing: false,
            generation: 1,
        },
    };
}
impl Event {
    pub const AUTO: Self = Self {
        signaled: false,
        mode: 1,
        queue: WaitQueue::EMPTY,
    };
}
impl Mutex {
    pub const EMPTY: Self = Self {
        owner: 0,
        owner_generation: 0,
        depth: 0,
        rank: 10,
        mode: 0,
        name: Text::literal(b"local"),
        queue: WaitQueue::EMPTY,
        donation_rank: 255,
        handoff_pending: false,
        owner_task: core::ptr::null_mut(),
        waiter_ranks: [0; 4],
    };
}
impl UnwindGuard {
    pub const EMPTY: Self = Self {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(b"guard"),
        queue: WaitQueue::EMPTY,
        owner_task: core::ptr::null_mut(),
        waiter_ranks: [0; 4],
        donation_rank: 255,
        handoff_epoch: 0,
    };
}
impl WaitRequest {
    pub const fn predicate(
        timeout: u64,
        reason: Text,
        predicate: Predicate,
        context: *mut c_void,
    ) -> Self {
        Self {
            timeout,
            reason,
            predicate: Some(predicate),
            context,
            release: None,
            release_context: core::ptr::null_mut(),
            flags: 0,
            consume_runtime: false,
        }
    }
}
pub(super) fn wait_result(value: u32) -> super::task::task_types::WaitResult {
    use super::task::task_types::WaitResult;
    match value {
        0 => WaitResult::None,
        1 => WaitResult::Signaled,
        2 => WaitResult::Timeout,
        3 => WaitResult::Cancelled,
        4 => WaitResult::Killed,
        5 => WaitResult::Failed,
        _ => panic!("Invalid private wait result"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<WaitQueue>() == 40);
    assert!(core::mem::size_of::<Event>() == 48);
    assert!(core::mem::size_of::<Completion>() == 48);
    assert!(core::mem::size_of::<Semaphore>() == 48);
    assert!(core::mem::size_of::<Mutex>() == 128);
    assert!(core::mem::size_of::<UnwindGuard>() == 136);
    assert!(core::mem::size_of::<WaitRequest>() == 72);
    assert!(core::mem::size_of::<WaitSummary>() == 96);
    assert!(core::mem::size_of::<LockSummary>() == 120);
    assert!(core::mem::offset_of!(Mutex, queue) == 40);
    assert!(core::mem::offset_of!(UnwindGuard, queue) == 40);
    assert!(core::mem::offset_of!(WaitRequest, flags) == 56);
};
