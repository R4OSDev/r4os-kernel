//! Native bounded R4D Work runtime. The original Work owner protects resident
//! metadata only; callbacks, completion publication/waits and task construction
//! remain outside it with exact original lifetime barriers.
use crate::{
    build_bridge::config,
    contract as a,
    driver::{irq_router, work_queue as q},
    memory::owner,
    platform::{monotonic, timer},
    sched::{
        scheduler, sync_objects,
        sync_types::{Completion, Event, WaitQueue},
        task::{
            task_registry as registry,
            task_resources::{self, CreateArgs},
            task_types::{Bytes, CreateFailure, ExecutionOwnerKind, Role, State as TaskState},
        },
    },
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
};
pub type Summary = a::DriverWorkSummary;
pub type CompletionStatus = a::DriverCompletionStatus;
pub type Performance = a::ProgramDriverWorkPerformanceInfo;
pub type Metrics = a::ProgramDriverWorkPerformanceMetrics;
pub type Request = a::DriverWorkRequest;
pub type Handler = unsafe extern "C" fn(usize) -> i32;
const CAP: usize = 16;
const OWNERS: usize = 16;
pub(crate) const OWNER_CAPACITY: u32 = OWNERS as u32;
const NONE: u8 = 255;
const FREE: u8 = 0;
const QUEUED: u8 = 1;
const RUNNING: u8 = 2;
const COMPLETED: u8 = 3;
const CANCELLED: u8 = 4;
const TASK: u8 = 0;
const IRQ: u8 = 1;
const DEADLINE: u8 = 2;
const CANCELLED_RESULT: i32 = -7;
const CLEANUP_TICKS: u64 = 1000;
const LONG_CALLBACK_NS: u64 = 1_000_000;
const SUMMARY_EMPTY: a::DriverWorkSummary = a::DriverWorkSummary {
    version: 1,
    size: 248,
    initialized: 0,
    worker_started: 0,
    worker_task_id: 0,
    queue_capacity: 16,
    queue_depth: 0,
    queue_high_water: 0,
    active_workers: 0,
    reserved0: 0,
    submitted: 0,
    submitted_from_irq: 0,
    submitted_from_task: 0,
    started: 0,
    completed: 0,
    failed: 0,
    cancelled: 0,
    dropped: 0,
    waits: 0,
    wait_timeouts: 0,
    wait_denied_irq: 0,
    wait_total_ticks: 0,
    wait_max_ticks: 0,
    wait_last_ticks: 0,
    queue_total_ticks: 0,
    queue_max_ticks: 0,
    queue_last_ticks: 0,
    run_total_ticks: 0,
    run_max_ticks: 0,
    run_last_ticks: 0,
    releases: 0,
    invalid_handles: 0,
    cleanup_cancelled: 0,
    sleep_waits: 0,
    sleep_denied_irq: 0,
    sleep_total_ticks: 0,
};
const METRICS_EMPTY: a::ProgramDriverWorkPerformanceMetrics =
    a::ProgramDriverWorkPerformanceMetrics {
        submitted: 0,
        submitted_actual_irq: 0,
        submitted_actual_task: 0,
        submitted_irq_class: 0,
        submitted_task_class: 0,
        started: 0,
        started_irq_class: 0,
        started_task_class: 0,
        completed: 0,
        completed_irq_class: 0,
        completed_task_class: 0,
        failed: 0,
        cancelled: 0,
        dropped: 0,
        full_rejections: 0,
        retained_full_rejections: 0,
        waits: 0,
        wait_timeouts: 0,
        wait_denied_irq: 0,
        wait_failed: 0,
        wait_total_ns: 0,
        wait_max_ns: 0,
        wait_last_ns: 0,
        wake_publications: 0,
        wake_waiters: 0,
        wake_misses: 0,
        releases: 0,
        release_busy: 0,
        release_wakes: 0,
        invalid_handles: 0,
        stale_handles: 0,
        publication_pending_releases: 0,
        waiter_blocked_releases: 0,
        claimed_releases: 0,
        queue_total_ns: 0,
        queue_max_ns: 0,
        queue_last_ns: 0,
        run_total_ns: 0,
        run_max_ns: 0,
        run_last_ns: 0,
        e2e_total_ns: 0,
        e2e_max_ns: 0,
        e2e_last_ns: 0,
        timing_unavailable: 0,
        completion_age_current_ns: 0,
        completion_age_max_ns: 0,
        selection_irq: 0,
        selection_task: 0,
        selection_irq_preferred: 0,
        selection_task_fairness: 0,
        selection_empty: 0,
        scan_passes: 0,
        scan_slots: 0,
        critical_sections: 0,
        critical_from_irq: 0,
        critical_timing_samples: 0,
        critical_timing_unavailable: 0,
        critical_total_ns: 0,
        critical_max_ns: 0,
        critical_last_ns: 0,
        cleanup_calls: 0,
        cleanup_quiesced: 0,
        cleanup_failed_context: 0,
        cleanup_queued_cancelled: 0,
        cleanup_waits: 0,
        cleanup_wait_timeouts: 0,
        cleanup_wait_failures: 0,
        cleanup_wait_total_ns: 0,
        cleanup_wait_max_ns: 0,
        cleanup_released: 0,
        cleanup_late_finishes: 0,
        cleanup_scan_passes: 0,
        cleanup_scan_slots: 0,
        long_callbacks: 0,
        waiter_enrollments: 0,
        waiter_wake_returns: 0,
        waiter_cancel_returns: 0,
        sleep_waits: 0,
        sleep_denied_irq: 0,
        sleep_total_ticks: 0,
    };
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DeadlineMetrics {
    submitted: u64,
    started: u64,
    completed: u64,
    misses: u64,
    budget_overruns: u64,
    queue_rejections: u64,
    queue_total_ticks: u64,
    queue_max_ticks: u64,
    lateness_total_ticks: u64,
    lateness_max_ticks: u64,
}
const DEADLINE_EMPTY: DeadlineMetrics = DeadlineMetrics {
    submitted: 0,
    started: 0,
    completed: 0,
    misses: 0,
    budget_overruns: 0,
    queue_rejections: 0,
    queue_total_ticks: 0,
    queue_max_ticks: 0,
    lateness_total_ticks: 0,
    lateness_max_ticks: 0,
};
const OWNER_EMPTY: q::OwnerBookkeeping = q::OwnerBookkeeping {
    used: 0,
    queued: 0,
    running: 0,
    deadline_running: 0,
    completed: 0,
    cancelled: 0,
    irq_queued: 0,
    task_queued: 0,
    deadline_queued: 0,
    waiters: 0,
    waiters_max: 0,
    used_high_water: 0,
    retained_high_water: 0,
    deadline_queue_high_water: 0,
};
const BOOK_EMPTY: q::Bookkeeping = q::Bookkeeping {
    states: [0; CAP],
    classes: [0; CAP],
    queue_prev: [NONE; CAP],
    queue_next: [NONE; CAP],
    final_prev: [NONE; CAP],
    final_next: [NONE; CAP],
    irq_head: NONE,
    irq_tail: NONE,
    task_head: NONE,
    task_tail: NONE,
    deadline_head: NONE,
    deadline_tail: NONE,
    final_head: NONE,
    final_tail: NONE,
    free_mask: u16::MAX,
    free_count: CAP as u32,
    queued_count: 0,
    running_count: 0,
    deadline_running_count: 0,
    completed_count: 0,
    cancelled_count: 0,
    irq_queued_count: 0,
    task_queued_count: 0,
    deadline_queued_count: 0,
    queue_high_water: 0,
    used_high_water: 0,
    retained_high_water: 0,
    deadline_queue_high_water: 0,
    current_irq_burst: 0,
};
struct Item {
    owner: u32,
    owned_slice: bool,
    handle: u32,
    generation: u32,
    flags: u32,
    // Retained declared device identity; the original single deadline lane
    // serializes callbacks without reading this key after request admission.
    _serial_key: u64,
    deadline_tick: u64,
    budget_ticks: u64,
    handler: Option<Handler>,
    context: usize,
    result: i32,
    submitted_tick: u64,
    started_tick: u64,
    completed_tick: u64,
    submitted_at: monotonic::Stamp,
    started_at: monotonic::Stamp,
    completed_at: monotonic::Stamp,
    completion_published: bool,
    release_claimed: bool,
    cleanup_waiting: bool,
    waiter_count: u32,
    owner_final_prev: u8,
    owner_final_next: u8,
    completion: Completion,
}
const STAMP_EMPTY: monotonic::Stamp = monotonic::Stamp {
    raw: 0,
    kind: 0,
    generation: 0,
};
impl Item {
    const EMPTY: Self = Self {
        owner: 0,
        owned_slice: false,
        handle: 0,
        generation: 0,
        flags: 0,
        _serial_key: 0,
        deadline_tick: 0,
        budget_ticks: 0,
        handler: None,
        context: 0,
        result: 0,
        submitted_tick: 0,
        started_tick: 0,
        completed_tick: 0,
        submitted_at: STAMP_EMPTY,
        started_at: STAMP_EMPTY,
        completed_at: STAMP_EMPTY,
        completion_published: false,
        release_claimed: false,
        cleanup_waiting: false,
        waiter_count: 0,
        owner_final_prev: NONE,
        owner_final_next: NONE,
        completion: Completion {
            completed: 0,
            queue: WaitQueue::EMPTY,
        },
    };
}
struct State {
    initialized: bool,
    worker_started: bool,
    worker_task_id: u32,
    deadline_worker_started: bool,
    deadline_worker_task_id: u32,
    items: [Item; CAP],
    deadlines: [u64; CAP],
    book: q::Bookkeeping,
    queue_event: Event,
    deadline_event: Event,
    summary: Summary,
    metrics: Metrics,
    owner_metrics: [Metrics; OWNERS],
    deadline_metrics: DeadlineMetrics,
    owner_deadline_metrics: [DeadlineMetrics; OWNERS],
    owner_current: [q::OwnerBookkeeping; OWNERS],
    owner_final_head: [u8; OWNERS],
    owner_final_tail: [u8; OWNERS],
    waiters: u32,
    waiters_max: u32,
    last_submitted_owner: u32,
    last_started_owner: u32,
    last_completed_owner: u32,
    last_cleanup_owner: u32,
    normal_callback_owner: u32,
    deadline_callback_owner: u32,
}
struct Storage(UnsafeCell<State>);
// SAFETY: the original Work owner class3 serializes all canonical metadata.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    initialized: false,
    worker_started: false,
    worker_task_id: 0,
    deadline_worker_started: false,
    deadline_worker_task_id: 0,
    items: [const { Item::EMPTY }; CAP],
    deadlines: [0; CAP],
    book: BOOK_EMPTY,
    queue_event: Event {
        signaled: false,
        mode: 1,
        queue: WaitQueue::EMPTY,
    },
    deadline_event: Event {
        signaled: false,
        mode: 1,
        queue: WaitQueue::EMPTY,
    },
    summary: SUMMARY_EMPTY,
    metrics: METRICS_EMPTY,
    owner_metrics: [METRICS_EMPTY; OWNERS],
    deadline_metrics: DEADLINE_EMPTY,
    owner_deadline_metrics: [DEADLINE_EMPTY; OWNERS],
    owner_current: [const { OWNER_EMPTY }; OWNERS],
    owner_final_head: [NONE; OWNERS],
    owner_final_tail: [NONE; OWNERS],
    waiters: 0,
    waiters_max: 0,
    last_submitted_owner: 0,
    last_started_owner: 0,
    last_completed_owner: 0,
    last_cleanup_owner: 0,
    normal_callback_owner: 0,
    deadline_callback_owner: 0,
}));
fn raw() -> *mut State {
    STATE.0.get()
}
fn index(id: u32) -> Option<usize> {
    if id == 0 || id as usize > OWNERS {
        None
    } else {
        Some((id - 1) as usize)
    }
}
fn capture() -> monotonic::Stamp {
    if config::ENABLE_METRICS {
        monotonic::capture()
    } else {
        STAMP_EMPTY
    }
}
fn elapsed(start: u64, end: u64) -> u64 {
    end.saturating_sub(start)
}
fn from_irq() -> bool {
    irq_router::r4os_rust_irq_router_local(false) != 0
}
fn current_id() -> Option<u32> {
    let task = scheduler::current();
    if task.is_null() {
        None
    } else {
        Some(unsafe { (*registry::core(task.cast())).id })
    }
}
struct Critical {
    lock: owner::Guard,
    started: monotonic::Stamp,
    from_irq: bool,
    timed: bool,
}
impl Critical {
    fn enter() -> Self {
        let from_irq = from_irq();
        let lock = owner::Guard::acquire(3);
        let timed = config::ENABLE_METRICS
            && unsafe { (*raw()).metrics.critical_sections.is_multiple_of(64) };
        Self {
            lock,
            started: if timed { capture() } else { STAMP_EMPTY },
            from_irq,
            timed,
        }
    }
    fn leave(self, id: u32) {
        let s = unsafe { &mut *raw() };
        add(s, id, |m| &mut m.critical_sections, 1);
        if self.from_irq {
            add(s, id, |m| &mut m.critical_from_irq, 1)
        }
        if self.timed {
            if let Some(v) = monotonic::elapsed(self.started, capture()) {
                add(s, id, |m| &mut m.critical_timing_samples, 1);
                add(s, id, |m| &mut m.critical_total_ns, v);
                max(s, id, |m| &mut m.critical_max_ns, v);
                set(s, id, |m| &mut m.critical_last_ns, v);
            } else {
                add(s, id, |m| &mut m.critical_timing_unavailable, 1)
            }
        }
        drop(self.lock);
    }
}
fn add(s: &mut State, id: u32, field: fn(&mut Metrics) -> &mut u64, value: u64) {
    let f = field(&mut s.metrics);
    *f = f.wrapping_add(value);
    if let Some(i) = index(id) {
        let f = field(&mut s.owner_metrics[i]);
        *f = f.wrapping_add(value)
    }
}
fn set(s: &mut State, id: u32, field: fn(&mut Metrics) -> &mut u64, value: u64) {
    *field(&mut s.metrics) = value;
    if let Some(i) = index(id) {
        *field(&mut s.owner_metrics[i]) = value
    }
}
fn max(s: &mut State, id: u32, field: fn(&mut Metrics) -> &mut u64, value: u64) {
    let f = field(&mut s.metrics);
    *f = (*f).max(value);
    if let Some(i) = index(id) {
        let f = field(&mut s.owner_metrics[i]);
        *f = (*f).max(value)
    }
}
fn add_deadline(s: &mut State, id: u32, field: fn(&mut DeadlineMetrics) -> &mut u64, value: u64) {
    let f = field(&mut s.deadline_metrics);
    *f = f.wrapping_add(value);
    if let Some(i) = index(id) {
        let f = field(&mut s.owner_deadline_metrics[i]);
        *f = f.wrapping_add(value)
    }
}
fn max_deadline(s: &mut State, id: u32, field: fn(&mut DeadlineMetrics) -> &mut u64, value: u64) {
    let f = field(&mut s.deadline_metrics);
    *f = (*f).max(value);
    if let Some(i) = index(id) {
        let f = field(&mut s.owner_deadline_metrics[i]);
        *f = (*f).max(value)
    }
}
fn queue(s: &mut State, op: u8, value: usize) -> usize {
    unsafe { q::r4os_rust_work_queue(&mut s.book, op, value) }
}
fn query(s: &State, op: u8, slot: usize) -> usize {
    unsafe { q::r4os_rust_work_queue_query(&s.book, op, slot) }
}
fn ledger(s: &mut State, id: u32, op: u8, value: u8) {
    if let Some(i) = index(id) {
        unsafe { q::r4os_rust_work_owner_ledger(&mut s.owner_current[i], op, value) };
    }
}
fn sync_legacy(s: &mut State) {
    s.summary.queue_depth = s.book.queued_count;
    s.summary.active_workers = s.book.running_count;
    s.summary.queue_high_water = s.book.queue_high_water;
}
unsafe extern "C" {
    fn r4os_zig_work_display_init() -> bool;
    fn r4os_zig_work_invoke_owned(owner: u32, handler: Handler, context: usize) -> i32;
}
fn append_final(s: &mut State, id: u32, slot: usize) {
    let Some(i) = index(id) else { return };
    let tail = s.owner_final_tail[i];
    s.items[slot].owner_final_prev = tail;
    s.items[slot].owner_final_next = NONE;
    if tail == NONE {
        s.owner_final_head[i] = slot as u8
    } else {
        s.items[tail as usize].owner_final_next = slot as u8
    }
    s.owner_final_tail[i] = slot as u8;
}
fn remove_final(s: &mut State, id: u32, slot: usize) {
    let Some(i) = index(id) else { return };
    let prev = s.items[slot].owner_final_prev;
    let next = s.items[slot].owner_final_next;
    if prev == NONE {
        s.owner_final_head[i] = next
    } else {
        s.items[prev as usize].owner_final_next = next
    }
    if next == NONE {
        s.owner_final_tail[i] = prev
    } else {
        s.items[next as usize].owner_final_prev = prev
    }
    s.items[slot].owner_final_prev = NONE;
    s.items[slot].owner_final_next = NONE;
}
fn waiter_add(s: &mut State, id: u32) {
    s.waiters = s.waiters.saturating_add(1);
    s.waiters_max = s.waiters_max.max(s.waiters);
    ledger(s, id, 5, 0)
}
fn waiter_remove(s: &mut State, id: u32) {
    s.waiters = s.waiters.saturating_sub(1);
    ledger(s, id, 6, 0)
}
fn lookup(s: &State, handle: u32) -> Result<usize, bool> {
    let slot = q::r4os_rust_work_handle(1, handle as usize, 0);
    if slot == usize::MAX {
        return Err(false);
    }
    let generation = (handle >> 8) & 0x00ff_ffff;
    if s.items[slot].generation != generation {
        return Err(true);
    }
    if query(s, 4, slot) == FREE as usize || s.items[slot].handle != handle {
        return Err(false);
    }
    Ok(slot)
}
fn valid(s: &mut State, handle: u32) -> Option<usize> {
    match lookup(s, handle) {
        Ok(i) => Some(i),
        Err(stale) => {
            if stale {
                add(s, 0, |m| &mut m.stale_handles, 1)
            } else {
                add(s, 0, |m| &mut m.invalid_handles, 1)
            }
            s.summary.invalid_handles = s.summary.invalid_handles.wrapping_add(1);
            None
        }
    }
}
fn duration(
    s: &mut State,
    id: u32,
    total: fn(&mut Metrics) -> &mut u64,
    maximum: fn(&mut Metrics) -> &mut u64,
    last: Option<fn(&mut Metrics) -> &mut u64>,
    start: monotonic::Stamp,
    end: monotonic::Stamp,
) {
    if !config::ENABLE_METRICS {
        return;
    }
    let Some(v) = monotonic::elapsed(start, end) else {
        add(s, id, |m| &mut m.timing_unavailable, 1);
        return;
    };
    add(s, id, total, v);
    max(s, id, maximum, v);
    if let Some(f) = last {
        set(s, id, f, v)
    }
}
fn note_wait(s: &mut State, id: u32, start_tick: u64, start: monotonic::Stamp, timed_out: bool) {
    let ticks = elapsed(start_tick, timer::tick_count());
    add(s, id, |m| &mut m.waits, 1);
    if timed_out {
        add(s, id, |m| &mut m.wait_timeouts, 1)
    }
    duration(
        s,
        id,
        |m| &mut m.wait_total_ns,
        |m| &mut m.wait_max_ns,
        Some(|m| &mut m.wait_last_ns),
        start,
        capture(),
    );
    s.summary.waits = s.summary.waits.wrapping_add(1);
    if timed_out {
        s.summary.wait_timeouts = s.summary.wait_timeouts.wrapping_add(1)
    }
    s.summary.wait_total_ticks = s.summary.wait_total_ticks.wrapping_add(ticks);
    s.summary.wait_last_ticks = ticks;
    s.summary.wait_max_ticks = s.summary.wait_max_ticks.max(ticks);
}
fn note_age(s: &mut State, id: u32, start: monotonic::Stamp) {
    if !config::ENABLE_METRICS {
        return;
    }
    let Some(age) = monotonic::elapsed(start, monotonic::capture()) else {
        add(s, id, |m| &mut m.timing_unavailable, 1);
        return;
    };
    max(s, id, |m| &mut m.completion_age_max_ns, age)
}
fn completion_pointer(slot: usize) -> *mut Completion {
    unsafe { ptr::addr_of_mut!((*raw()).items[slot].completion) }
}
fn event_pointer(deadline: bool) -> *mut Event {
    unsafe {
        if deadline {
            ptr::addr_of_mut!((*raw()).deadline_event)
        } else {
            ptr::addr_of_mut!((*raw()).queue_event)
        }
    }
}
fn publish(slot: usize, handle: u32) {
    let count = unsafe { sync_objects::r4os_rust_sync_completion(completion_pointer(slot), 1, 0) };
    let critical = Critical::enter();
    let s = unsafe { &mut *raw() };
    let mut owner = 0;
    if let Ok(i) = lookup(s, handle)
        && i == slot
        && matches!(query(s, 4, i) as u8, COMPLETED | CANCELLED)
        && !s.items[i].completion_published
    {
        owner = s.items[i].owner;
        s.items[i].completion_published = true;
        add(s, owner, |m| &mut m.wake_publications, 1);
        if count == 0 {
            add(s, owner, |m| &mut m.wake_misses, 1)
        } else {
            add(s, owner, |m| &mut m.wake_waiters, u64::from(count))
        }
    }
    critical.leave(owner);
    scheduler::r4os_rust_scheduler_control(11, 0, 0);
}
fn create_worker(deadline: bool) -> u32 {
    let mut failure = CreateFailure::None;
    let name: &[u8] = if deadline { b"r4d-audio" } else { b"r4d-work" };
    let entry = if deadline {
        deadline_worker
    } else {
        normal_worker
    };
    let task = unsafe {
        task_resources::create(
            CreateArgs {
                name: Bytes {
                    pointer: name.as_ptr(),
                    len: name.len(),
                },
                state: TaskState::Ready,
                needs_fpu: true,
                entry: Some(entry),
                role: Role::ShortCompletion,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            &mut failure,
        )
    };
    if task.is_null() {
        0
    } else {
        unsafe { (*registry::core(task)).id }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_work_init() -> bool {
    if !unsafe { r4os_zig_work_display_init() } {
        return false;
    }
    if unsafe {
        (*raw()).initialized && (*raw()).worker_started && (*raw()).deadline_worker_started
    } {
        return true;
    }
    unsafe {
        (*raw()).initialized = true;
        (*raw()).summary.initialized = 1;
        (*raw()).summary.queue_capacity = CAP as u32;
    }
    if !unsafe { (*raw()).worker_started } {
        let id = create_worker(false);
        if id == 0 {
            unsafe { (*raw()).summary.worker_started = 0 };
            return false;
        }
        unsafe {
            (*raw()).worker_task_id = id;
            (*raw()).worker_started = true;
            (*raw()).summary.worker_task_id = id;
        }
    }
    if !unsafe { (*raw()).deadline_worker_started } {
        let id = create_worker(true);
        if id == 0 {
            unsafe { (*raw()).summary.worker_started = 0 };
            return false;
        }
        unsafe {
            (*raw()).deadline_worker_task_id = id;
            (*raw()).deadline_worker_started = true;
        }
    }
    unsafe {
        (*raw()).summary.worker_started = 1;
        (*raw()).worker_started && (*raw()).deadline_worker_started
    }
}
struct Submit {
    owner: u32,
    handler: Handler,
    context: usize,
    flags: u32,
    source: u8,
    serial_key: u64,
    deadline_tick: u64,
    budget_ticks: u64,
    actual_irq: bool,
    owned_slice: bool,
}
unsafe fn submit(r: Submit, out: *mut u32) -> i32 {
    unsafe { *out = 0 };
    let ready = unsafe {
        if r.source == DEADLINE {
            (*raw()).deadline_worker_started
        } else {
            (*raw()).worker_started
        }
    };
    if !unsafe { (*raw()).initialized } || !ready {
        let c = Critical::enter();
        let s = unsafe { &mut *raw() };
        add(s, r.owner, |m| &mut m.dropped, 1);
        if r.source == DEADLINE {
            add_deadline(s, r.owner, |m| &mut m.queue_rejections, 1)
        }
        s.summary.dropped = s.summary.dropped.wrapping_add(1);
        c.leave(r.owner);
        return -1;
    }
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let slot = queue(s, 0, r.source as usize);
    if slot == usize::MAX {
        add(s, r.owner, |m| &mut m.dropped, 1);
        add(s, r.owner, |m| &mut m.full_rejections, 1);
        if r.source == DEADLINE {
            add_deadline(s, r.owner, |m| &mut m.queue_rejections, 1)
        }
        if query(s, 7, 0) != 0 {
            add(s, r.owner, |m| &mut m.retained_full_rejections, 1)
        }
        s.summary.dropped = s.summary.dropped.wrapping_add(1);
        sync_legacy(s);
        c.leave(r.owner);
        return -2;
    }
    let generation = q::r4os_rust_work_handle(2, 0, s.items[slot].generation) as u32;
    let handle = q::r4os_rust_work_handle(0, slot, generation) as u32;
    s.items[slot] = Item {
        owner: r.owner,
        owned_slice: r.owned_slice,
        handle,
        generation,
        flags: r.flags | u32::from(r.actual_irq),
        _serial_key: r.serial_key,
        deadline_tick: r.deadline_tick,
        budget_ticks: r.budget_ticks,
        handler: Some(r.handler),
        context: r.context,
        submitted_tick: timer::tick_count(),
        submitted_at: capture(),
        ..Item::EMPTY
    };
    s.deadlines[slot] = r.deadline_tick;
    ledger(s, r.owner, 0, r.source);
    unsafe { *out = handle };
    s.last_submitted_owner = r.owner;
    add(s, r.owner, |m| &mut m.submitted, 1);
    if r.actual_irq {
        add(s, r.owner, |m| &mut m.submitted_actual_irq, 1)
    } else {
        add(s, r.owner, |m| &mut m.submitted_actual_task, 1)
    }
    if r.actual_irq || r.flags & 1 != 0 {
        add(s, r.owner, |m| &mut m.submitted_irq_class, 1)
    } else {
        add(s, r.owner, |m| &mut m.submitted_task_class, 1)
    }
    if r.source == DEADLINE {
        add_deadline(s, r.owner, |m| &mut m.submitted, 1)
    }
    s.summary.submitted = s.summary.submitted.wrapping_add(1);
    if r.actual_irq {
        s.summary.submitted_from_irq = s.summary.submitted_from_irq.wrapping_add(1)
    } else {
        s.summary.submitted_from_task = s.summary.submitted_from_task.wrapping_add(1)
    }
    sync_legacy(s);
    c.leave(r.owner);
    unsafe { sync_objects::r4os_rust_sync_event(event_pointer(r.source == DEADLINE), 0, 0) };
    0
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_submit(
    owner: u32,
    handler: Handler,
    context: usize,
    flags: u32,
    owned: bool,
    out: *mut u32,
) -> i32 {
    let irq = from_irq();
    if owned {
        unsafe { *out = 0 };
        if owner == 0 || irq {
            return -3;
        }
    }
    unsafe {
        submit(
            Submit {
                owner,
                handler,
                context,
                flags: if owned { 0 } else { flags },
                source: if !owned && (irq || flags & 1 != 0) {
                    IRQ
                } else {
                    TASK
                },
                serial_key: 0,
                deadline_tick: 0,
                budget_ticks: 0,
                actual_irq: irq,
                owned_slice: owned,
            },
            out,
        )
    }
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_request(
    owner: u32,
    request: *const Request,
    out: *mut u32,
) -> i32 {
    unsafe { *out = 0 };
    let r = unsafe { ptr::read(request) };
    if r.flags & !1 != 0
        || r.version != a::driver_work_request_version
        || r.size < size_of::<Request>() as u32
        || r.work_class != a::driver_work_class_audio_refill
        || r.serial_key == 0
        || r.deadline_tick == 0
        || r.budget_ticks == 0
        || r.budget_ticks > a::driver_work_deadline_max_budget_ticks
    {
        return -3;
    }
    unsafe {
        submit(
            Submit {
                owner,
                handler: r.handler,
                context: r.context,
                flags: r.flags,
                source: DEADLINE,
                serial_key: r.serial_key,
                deadline_tick: r.deadline_tick,
                budget_ticks: r.budget_ticks,
                actual_irq: from_irq(),
                owned_slice: false,
            },
            out,
        )
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_work_cancel(handle: u32) -> i32 {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(slot) = valid(s, handle) else {
        c.leave(0);
        return -1;
    };
    let id = s.items[slot].owner;
    if query(s, 4, slot) != QUEUED as usize {
        c.leave(id);
        return -2;
    }
    let source = query(s, 5, slot) as u8;
    if queue(s, 1, slot) == 0 {
        c.leave(id);
        return -2;
    }
    s.items[slot].result = CANCELLED_RESULT;
    s.items[slot].completed_tick = timer::tick_count();
    s.items[slot].completed_at = capture();
    s.items[slot].completion_published = false;
    ledger(s, id, 2, source);
    append_final(s, id, slot);
    add(s, id, |m| &mut m.cancelled, 1);
    s.summary.cancelled = s.summary.cancelled.wrapping_add(1);
    sync_legacy(s);
    c.leave(id);
    publish(slot, handle);
    0
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_wait(handle: u32, ticks: u64, out: *mut i32) -> i32 {
    unsafe { *out = 0 };
    let start_tick = timer::tick_count();
    let start = capture();
    if from_irq() {
        let c = Critical::enter();
        let s = unsafe { &mut *raw() };
        let Some(slot) = valid(s, handle) else {
            c.leave(0);
            return -1;
        };
        let id = s.items[slot].owner;
        add(s, id, |m| &mut m.wait_denied_irq, 1);
        s.summary.wait_denied_irq = s.summary.wait_denied_irq.wrapping_add(1);
        c.leave(id);
        return -6;
    }
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(slot) = valid(s, handle) else {
        c.leave(0);
        return -1;
    };
    let id = s.items[slot].owner;
    let state = query(s, 4, slot) as u8;
    match state {
        COMPLETED | CANCELLED => {
            unsafe { *out = s.items[slot].result };
            note_wait(s, id, start_tick, start, false);
            c.leave(id);
            return if state == CANCELLED {
                CANCELLED_RESULT
            } else {
                0
            };
        }
        QUEUED | RUNNING => {
            s.items[slot].waiter_count = s.items[slot].waiter_count.saturating_add(1);
            waiter_add(s, id);
            add(s, id, |m| &mut m.waiter_enrollments, 1)
        }
        FREE => {
            c.leave(id);
            return -2;
        }
        _ => unreachable!(),
    }
    c.leave(id);
    let result =
        unsafe { sync_objects::r4os_rust_sync_completion(completion_pointer(slot), 4, ticks) };
    let timed_out = result == 2;
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(slot) = valid(s, handle) else {
        c.leave(0);
        return -1;
    };
    let id = s.items[slot].owner;
    if s.items[slot].waiter_count != 0 {
        s.items[slot].waiter_count -= 1;
        waiter_remove(s, id)
    }
    note_wait(s, id, start_tick, start, timed_out);
    if timed_out {
        c.leave(id);
        return 1;
    }
    if result != 1 {
        add(s, id, |m| &mut m.wait_failed, 1);
        add(s, id, |m| &mut m.waiter_cancel_returns, 1);
        c.leave(id);
        return -5;
    }
    add(s, id, |m| &mut m.waiter_wake_returns, 1);
    unsafe { *out = s.items[slot].result };
    let state = query(s, 4, slot) as u8;
    c.leave(id);
    if state == CANCELLED {
        CANCELLED_RESULT
    } else {
        0
    }
}
fn status(s: &State, slot: usize) -> CompletionStatus {
    let item = &s.items[slot];
    let state = query(s, 4, slot) as u8;
    let queue_end = match state {
        RUNNING | COMPLETED => item.started_tick,
        CANCELLED => item.completed_tick,
        FREE | QUEUED => 0,
        _ => unreachable!(),
    };
    let queue_ticks = if queue_end == 0 {
        0
    } else {
        elapsed(item.submitted_tick, queue_end)
    };
    let run_ticks = match state {
        COMPLETED => elapsed(item.started_tick, item.completed_tick),
        RUNNING => elapsed(item.started_tick, timer::tick_count()),
        FREE | QUEUED | CANCELLED => 0,
        _ => unreachable!(),
    };
    CompletionStatus {
        handle: item.handle,
        state: u32::from(state),
        owner: item.owner,
        flags: item.flags,
        result: item.result,
        submitted_tick: item.submitted_tick,
        started_tick: item.started_tick,
        completed_tick: item.completed_tick,
        queue_ticks,
        run_ticks,
        ..CompletionStatus::default()
    }
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_status(
    handle: u32,
    out: *mut CompletionStatus,
) -> i32 {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(slot) = valid(s, handle) else {
        unsafe { ptr::write(out, CompletionStatus::default()) };
        c.leave(0);
        return -1;
    };
    let id = s.items[slot].owner;
    unsafe { ptr::write(out, status(s, slot)) };
    c.leave(id);
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_work_release(handle: u32) -> i32 {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(slot) = valid(s, handle) else {
        c.leave(0);
        return -1;
    };
    let id = s.items[slot].owner;
    let state = query(s, 4, slot) as u8;
    let decision = q::r4os_rust_work_release_decision(
        state,
        s.items[slot].completion_published,
        s.items[slot].waiter_count,
        s.items[slot].release_claimed,
    );
    match decision {
        5 => {
            s.items[slot].release_claimed = true;
            add(s, id, |m| &mut m.claimed_releases, 1)
        }
        0 => {
            c.leave(id);
            return -3;
        }
        1 | 4 => {
            add(s, id, |m| &mut m.release_busy, 1);
            c.leave(id);
            return -2;
        }
        2 => {
            add(s, id, |m| &mut m.release_busy, 1);
            add(s, id, |m| &mut m.publication_pending_releases, 1);
            c.leave(id);
            return -2;
        }
        3 => {
            add(s, id, |m| &mut m.release_busy, 1);
            add(s, id, |m| &mut m.waiter_blocked_releases, 1);
            c.leave(id);
            return -2;
        }
        _ => unreachable!(),
    }
    let completed = s.items[slot].completed_at;
    c.leave(id);
    let wakes = unsafe { sync_objects::r4os_rust_sync_completion(completion_pointer(slot), 3, 0) };
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let Some(checked) = valid(s, handle) else {
        c.leave(0);
        return -1;
    };
    if checked != slot || !s.items[slot].release_claimed {
        c.leave(id);
        return -1;
    }
    if wakes != 0 {
        add(s, id, |m| &mut m.release_wakes, u64::from(wakes))
    }
    note_age(s, id, completed);
    if queue(s, 3, slot) == 0 {
        s.items[slot].release_claimed = false;
        c.leave(id);
        return -2;
    }
    remove_final(s, id, slot);
    ledger(s, id, 4, state);
    let generation = s.items[slot].generation;
    s.items[slot] = Item {
        generation,
        ..Item::EMPTY
    };
    s.deadlines[slot] = 0;
    add(s, id, |m| &mut m.releases, 1);
    s.summary.releases = s.summary.releases.wrapping_add(1);
    sync_legacy(s);
    c.leave(id);
    0
}
fn take(deadline: bool) -> Option<usize> {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let mut selection = MaybeUninit::uninit();
    let ready = unsafe {
        q::r4os_rust_work_queue_select(
            &mut s.book,
            deadline,
            s.deadlines.as_ptr(),
            CAP,
            selection.as_mut_ptr(),
        )
    };
    if !ready {
        if !deadline {
            add(s, 0, |m| &mut m.selection_empty, 1)
        }
        sync_legacy(s);
        c.leave(0);
        return None;
    }
    let selected = unsafe { selection.assume_init() };
    let slot = selected.slot;
    let id = s.items[slot].owner;
    s.items[slot].started_tick = timer::tick_count();
    s.items[slot].started_at = capture();
    ledger(s, id, 1, selected.source);
    s.last_started_owner = id;
    add(s, id, |m| &mut m.started, 1);
    if s.items[slot].flags & 1 != 0 {
        add(s, id, |m| &mut m.started_irq_class, 1)
    } else {
        add(s, id, |m| &mut m.started_task_class, 1)
    }
    match selected.source {
        IRQ => add(s, id, |m| &mut m.selection_irq, 1),
        TASK => add(s, id, |m| &mut m.selection_task, 1),
        DEADLINE => {
            add_deadline(s, id, |m| &mut m.started, 1);
            let ticks = elapsed(s.items[slot].submitted_tick, s.items[slot].started_tick);
            add_deadline(s, id, |m| &mut m.queue_total_ticks, ticks);
            max_deadline(s, id, |m| &mut m.queue_max_ticks, ticks);
            if s.items[slot].started_tick > s.items[slot].deadline_tick {
                let late = s.items[slot].started_tick - s.items[slot].deadline_tick;
                add_deadline(s, id, |m| &mut m.misses, 1);
                add_deadline(s, id, |m| &mut m.lateness_total_ticks, late);
                max_deadline(s, id, |m| &mut m.lateness_max_ticks, late)
            }
        }
        _ => unreachable!(),
    }
    if selected.irq_preferred {
        add(s, id, |m| &mut m.selection_irq_preferred, 1)
    }
    if selected.task_fairness {
        add(s, id, |m| &mut m.selection_task_fairness, 1)
    }
    duration(
        s,
        id,
        |m| &mut m.queue_total_ns,
        |m| &mut m.queue_max_ns,
        Some(|m| &mut m.queue_last_ns),
        s.items[slot].submitted_at,
        s.items[slot].started_at,
    );
    let ticks = elapsed(s.items[slot].submitted_tick, s.items[slot].started_tick);
    s.summary.started = s.summary.started.wrapping_add(1);
    s.summary.queue_total_ticks = s.summary.queue_total_ticks.wrapping_add(ticks);
    s.summary.queue_last_ticks = ticks;
    s.summary.queue_max_ticks = s.summary.queue_max_ticks.max(ticks);
    sync_legacy(s);
    c.leave(id);
    Some(slot)
}
fn finish(slot: usize, result: i32) {
    let tick = timer::tick_count();
    let now = capture();
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    if slot >= CAP || query(s, 4, slot) != RUNNING as usize {
        c.leave(0);
        return;
    }
    let id = s.items[slot].owner;
    let handle = s.items[slot].handle;
    let source = query(s, 5, slot) as u8;
    if queue(s, 2, slot) == 0 {
        c.leave(id);
        return;
    }
    s.items[slot].result = result;
    s.items[slot].completed_tick = tick;
    s.items[slot].completed_at = now;
    s.items[slot].completion_published = false;
    ledger(s, id, 3, source);
    append_final(s, id, slot);
    s.last_completed_owner = id;
    add(s, id, |m| &mut m.completed, 1);
    if s.items[slot].flags & 1 != 0 {
        add(s, id, |m| &mut m.completed_irq_class, 1)
    } else {
        add(s, id, |m| &mut m.completed_task_class, 1)
    }
    if result != 0 {
        add(s, id, |m| &mut m.failed, 1)
    }
    if s.items[slot].cleanup_waiting {
        add(s, id, |m| &mut m.cleanup_late_finishes, 1)
    }
    duration(
        s,
        id,
        |m| &mut m.run_total_ns,
        |m| &mut m.run_max_ns,
        Some(|m| &mut m.run_last_ns),
        s.items[slot].started_at,
        now,
    );
    duration(
        s,
        id,
        |m| &mut m.e2e_total_ns,
        |m| &mut m.e2e_max_ns,
        Some(|m| &mut m.e2e_last_ns),
        s.items[slot].submitted_at,
        now,
    );
    if config::ENABLE_METRICS
        && monotonic::elapsed(s.items[slot].started_at, now).is_some_and(|v| v >= LONG_CALLBACK_NS)
    {
        add(s, id, |m| &mut m.long_callbacks, 1)
    }
    let ticks = elapsed(s.items[slot].started_tick, tick);
    if source == DEADLINE {
        add_deadline(s, id, |m| &mut m.completed, 1);
        if ticks > s.items[slot].budget_ticks {
            add_deadline(s, id, |m| &mut m.budget_overruns, 1)
        }
    }
    s.summary.completed = s.summary.completed.wrapping_add(1);
    if result != 0 {
        s.summary.failed = s.summary.failed.wrapping_add(1)
    }
    s.summary.run_total_ticks = s.summary.run_total_ticks.wrapping_add(ticks);
    s.summary.run_last_ticks = ticks;
    s.summary.run_max_ticks = s.summary.run_max_ticks.max(ticks);
    sync_legacy(s);
    c.leave(id);
    publish(slot, handle)
}
fn run(slot: usize, deadline: bool) {
    let handler = unsafe { (*raw()).items[slot].handler };
    let Some(handler) = handler else {
        finish(slot, -1);
        return;
    };
    let id = unsafe { (*raw()).items[slot].owner };
    let context = unsafe { (*raw()).items[slot].context };
    let owned = unsafe { (*raw()).items[slot].owned_slice };
    unsafe {
        if deadline {
            (*raw()).deadline_callback_owner = id
        } else {
            (*raw()).normal_callback_owner = id
        }
    }
    let result = if owned {
        unsafe { r4os_zig_work_invoke_owned(id, handler, context) }
    } else {
        unsafe { handler(context) }
    };
    finish(slot, result);
    unsafe {
        if deadline {
            (*raw()).deadline_callback_owner = 0
        } else {
            (*raw()).normal_callback_owner = 0
        }
    }
}
unsafe extern "C" fn normal_worker() {
    loop {
        if let Some(slot) = take(false) {
            run(slot, false);
            continue;
        }
        unsafe { sync_objects::r4os_rust_sync_event(event_pointer(false), 2, u64::MAX) };
    }
}
unsafe extern "C" fn deadline_worker() {
    loop {
        if let Some(slot) = take(true) {
            run(slot, true);
            continue;
        }
        unsafe { sync_objects::r4os_rust_sync_event(event_pointer(true), 2, u64::MAX) };
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_work_current() -> u32 {
    let Some(id) = current_id() else { return 0 };
    unsafe {
        if id == (*raw()).worker_task_id {
            (*raw()).normal_callback_owner
        } else if id == (*raw()).deadline_worker_task_id {
            (*raw()).deadline_callback_owner
        } else {
            0
        }
    }
}
#[repr(C)]
pub struct Cleanup {
    pub removed: u32,
    pub quiesced: bool,
}
fn find_owner(s: &State, id: u32) -> (Option<usize>, u32) {
    let mut scanned = 0;
    let mut running = None;
    let mut final_slot = None;
    for i in 0..CAP {
        scanned += 1;
        if s.items[i].owner != id {
            continue;
        }
        match query(s, 4, i) as u8 {
            QUEUED => return (Some(i), scanned),
            RUNNING => {
                if running.is_none() {
                    running = Some(i)
                }
            }
            COMPLETED | CANCELLED => {
                if final_slot.is_none() {
                    final_slot = Some(i)
                }
            }
            FREE => {}
            _ => unreachable!(),
        }
    }
    (running.or(final_slot), scanned)
}
fn cleanup(id: u32) -> Cleanup {
    if id == 0 {
        return Cleanup {
            removed: 0,
            quiesced: true,
        };
    }
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    add(s, id, |m| &mut m.cleanup_calls, 1);
    s.last_cleanup_owner = id;
    let current = current_id();
    let is_worker =
        current.is_some_and(|v| v == s.worker_task_id || v == s.deadline_worker_task_id);
    if index(id).is_none() || from_irq() || current.is_none() || is_worker {
        add(s, id, |m| &mut m.cleanup_failed_context, 1);
        c.leave(id);
        return Cleanup {
            removed: 0,
            quiesced: false,
        };
    }
    c.leave(id);
    let started = timer::tick_count();
    let mut removed: u32 = 0;
    loop {
        let c = Critical::enter();
        let s = unsafe { &mut *raw() };
        let (found, scanned) = find_owner(s, id);
        add(s, id, |m| &mut m.scan_passes, 1);
        add(s, id, |m| &mut m.scan_slots, u64::from(scanned));
        add(s, id, |m| &mut m.cleanup_scan_passes, 1);
        add(s, id, |m| &mut m.cleanup_scan_slots, u64::from(scanned));
        let Some(slot) = found else {
            add(s, id, |m| &mut m.cleanup_quiesced, 1);
            s.summary.cleanup_cancelled =
                s.summary.cleanup_cancelled.wrapping_add(u64::from(removed));
            c.leave(id);
            return Cleanup {
                removed,
                quiesced: true,
            };
        };
        let handle = s.items[slot].handle;
        let state = query(s, 4, slot) as u8;
        if elapsed(started, timer::tick_count()) >= CLEANUP_TICKS {
            add(s, id, |m| &mut m.cleanup_wait_timeouts, 1);
            c.leave(id);
            return Cleanup {
                removed,
                quiesced: false,
            };
        }
        if state == RUNNING {
            s.items[slot].cleanup_waiting = true;
            s.items[slot].waiter_count = s.items[slot].waiter_count.saturating_add(1);
            waiter_add(s, id);
            add(s, id, |m| &mut m.waiter_enrollments, 1);
            add(s, id, |m| &mut m.cleanup_waits, 1)
        }
        c.leave(id);
        match state {
            QUEUED => {
                if r4os_rust_driver_work_cancel(handle) == 0 {
                    let c = Critical::enter();
                    let s = unsafe { &mut *raw() };
                    add(s, id, |m| &mut m.cleanup_queued_cancelled, 1);
                    c.leave(id)
                }
            }
            RUNNING => {
                let remaining = CLEANUP_TICKS.saturating_sub(elapsed(started, timer::tick_count()));
                let start = capture();
                let result = unsafe {
                    sync_objects::r4os_rust_sync_completion(completion_pointer(slot), 4, remaining)
                };
                let end = capture();
                let c = Critical::enter();
                let s = unsafe { &mut *raw() };
                if let Ok(slot) = lookup(s, handle)
                    && s.items[slot].waiter_count != 0
                {
                    s.items[slot].waiter_count -= 1;
                    waiter_remove(s, id)
                }
                duration(
                    s,
                    id,
                    |m| &mut m.cleanup_wait_total_ns,
                    |m| &mut m.cleanup_wait_max_ns,
                    None,
                    start,
                    end,
                );
                if result == 2 {
                    add(s, id, |m| &mut m.cleanup_wait_timeouts, 1);
                    c.leave(id);
                    return Cleanup {
                        removed,
                        quiesced: false,
                    };
                }
                if result != 1 {
                    add(s, id, |m| &mut m.cleanup_wait_failures, 1);
                    c.leave(id);
                    return Cleanup {
                        removed,
                        quiesced: false,
                    };
                }
                add(s, id, |m| &mut m.waiter_wake_returns, 1);
                c.leave(id)
            }
            COMPLETED | CANCELLED => {
                if r4os_rust_driver_work_release(handle) == 0 {
                    removed = removed.saturating_add(1);
                    let c = Critical::enter();
                    let s = unsafe { &mut *raw() };
                    add(s, id, |m| &mut m.cleanup_released, 1);
                    c.leave(id)
                } else {
                    scheduler::yield_task()
                }
            }
            FREE => {}
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_cleanup(owner: u32, out: *mut Cleanup) {
    unsafe { ptr::write(out, cleanup(owner)) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_work_sleep_note(denied: bool, ticks: u64) {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    if denied {
        add(s, 0, |m| &mut m.sleep_denied_irq, 1);
        s.summary.sleep_denied_irq = s.summary.sleep_denied_irq.wrapping_add(1)
    } else {
        add(s, 0, |m| &mut m.sleep_waits, 1);
        add(s, 0, |m| &mut m.sleep_total_ticks, ticks);
        s.summary.sleep_waits = s.summary.sleep_waits.wrapping_add(1);
        s.summary.sleep_total_ticks = s.summary.sleep_total_ticks.wrapping_add(ticks)
    }
    c.leave(0)
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_summary(out: *mut Summary) {
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    sync_legacy(s);
    let mut value = s.summary;
    value.initialized = u32::from(s.initialized);
    value.worker_started = u32::from(s.worker_started && s.deadline_worker_started);
    value.worker_task_id = s.worker_task_id;
    value.queue_capacity = CAP as u32;
    unsafe { ptr::write(out, value) };
    c.leave(0)
}
/// # Safety
/// C inputs/outputs are aligned caller storage. Canonical Work tickets,
/// publication barriers and waiters retain the exact resident completion identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_work_performance(id: u32, out: *mut Performance) {
    let clock = monotonic::snapshot();
    let now = capture();
    let c = Critical::enter();
    let s = unsafe { &mut *raw() };
    let oi = index(id);
    let metrics = if id == 0 {
        s.metrics
    } else {
        oi.map(|i| s.owner_metrics[i]).unwrap_or(METRICS_EMPTY)
    };
    let d = if id == 0 {
        s.deadline_metrics
    } else {
        oi.map(|i| s.owner_deadline_metrics[i])
            .unwrap_or(DEADLINE_EMPTY)
    };
    let current = oi
        .map(|i| unsafe { ptr::read(&s.owner_current[i]) })
        .unwrap_or(OWNER_EMPTY);
    let present = id == 0
        || (oi.is_some()
            && (current.used != 0
                || current.used_high_water != 0
                || metrics.submitted != 0
                || d.submitted != 0
                || metrics.cleanup_calls != 0));
    let mut value = Performance {
        selected_owner: id,
        owner_present: u32::from(present),
        initialized: u32::from(s.initialized),
        worker_started: u32::from(s.worker_started),
        worker_task_id: s.worker_task_id,
        worker_count: 2,
        free_slots: s.book.free_count,
        used_slots: query(s, 6, 0) as u32,
        queued_slots: s.book.queued_count,
        running_slots: s.book.running_count,
        completed_slots: s.book.completed_count,
        cancelled_slots: s.book.cancelled_count,
        irq_queued_slots: s.book.irq_queued_count,
        task_queued_slots: s.book.task_queued_count,
        queue_high_water: s.book.queue_high_water,
        used_high_water: s.book.used_high_water,
        retained_high_water: s.book.retained_high_water,
        current_irq_burst: s.book.current_irq_burst,
        waiters_current: s.waiters,
        waiters_max: s.waiters_max,
        last_submitted_owner: s.last_submitted_owner,
        last_started_owner: s.last_started_owner,
        last_completed_owner: s.last_completed_owner,
        last_cleanup_owner: s.last_cleanup_owner,
        owner_used_slots: current.used,
        owner_queued_slots: current.queued,
        owner_running_slots: current.running,
        owner_completed_slots: current.completed,
        owner_cancelled_slots: current.cancelled,
        owner_irq_queued_slots: current.irq_queued,
        owner_task_queued_slots: current.task_queued,
        owner_used_high_water: current.used_high_water,
        owner_retained_high_water: current.retained_high_water,
        monotonic_clock_flags: clock.flags,
        owner_waiters_current: current.waiters,
        owner_waiters_max: current.waiters_max,
        metrics,
        deadline_worker_started: u32::from(s.deadline_worker_started),
        deadline_worker_task_id: s.deadline_worker_task_id,
        deadline_queued_slots: s.book.deadline_queued_count,
        deadline_running_slots: s.book.deadline_running_count,
        deadline_queue_high_water: s.book.deadline_queue_high_water,
        owner_deadline_queued_slots: current.deadline_queued,
        owner_deadline_running_slots: current.deadline_running,
        owner_deadline_queue_high_water: current.deadline_queue_high_water,
        deadline_submitted: d.submitted,
        deadline_started: d.started,
        deadline_completed: d.completed,
        deadline_misses: d.misses,
        deadline_budget_overruns: d.budget_overruns,
        deadline_queue_rejections: d.queue_rejections,
        deadline_queue_total_ticks: d.queue_total_ticks,
        deadline_queue_max_ticks: d.queue_max_ticks,
        deadline_lateness_total_ticks: d.lateness_total_ticks,
        deadline_lateness_max_ticks: d.lateness_max_ticks,
        ..Performance::default()
    };
    let final_slot = if config::ENABLE_METRICS {
        if id == 0 {
            s.book.final_head
        } else {
            oi.map(|i| s.owner_final_head[i]).unwrap_or(NONE)
        }
    } else {
        NONE
    };
    if final_slot != NONE {
        if let Some(age) = monotonic::elapsed(s.items[final_slot as usize].completed_at, now) {
            value.metrics.completion_age_current_ns = age;
            value.metrics.completion_age_max_ns = value.metrics.completion_age_max_ns.max(age)
        } else {
            value.metrics.timing_unavailable = value.metrics.timing_unavailable.wrapping_add(1)
        }
    }
    unsafe { ptr::write(out, value) };
    c.leave(id)
}
