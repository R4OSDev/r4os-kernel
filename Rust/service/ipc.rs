//! Stable channel queues, five BSP lanes plus bounded reviewed AP workers and exact
//! admission/completion publication. No channel borrow spans wait or handler.
use super::{ipc_types::*, queue};
use crate::{
    build_config::config,
    driver::irq_router,
    fs::vfs_types::Bytes,
    platform::{clock_math, monotonic, timer},
    sched::{scheduler, sync_locks, sync_objects, sync_types::*, task, task_context},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
pub const MAX_CHANNELS: usize = 16;
pub const QUEUE_DEPTH: usize = 8;
pub const MAX_MESSAGE_SIZE: usize = 4096;
const WORKER_COUNT: usize = 5;
const MAX_AP_WORKERS: usize = 4;
const MAX_WORKERS: usize = WORKER_COUNT + MAX_AP_WORKERS;
const FOREVER: u64 = u64::MAX;
const STAMP: monotonic::Stamp = monotonic::Stamp {
    raw: 0,
    generation: 0,
    kind: 0,
};
struct Message {
    meta: queue::SlotMeta,
    channel_generation: u64,
    handler: Option<Handler>,
    parallel: bool,
    completion: Completion,
    submitted_at: monotonic::Stamp,
    started_at: monotonic::Stamp,
    data: [u8; MAX_MESSAGE_SIZE],
}
impl Message {
    const EMPTY: Self = Self {
        meta: queue::SlotMeta {
            state: 0,
            mode: 0,
            generation: 0,
            sequence: 0,
            len: 0,
            result: 0,
            abandoned: false,
            completion_published: false,
        },
        channel_generation: 0,
        handler: None,
        parallel: false,
        completion: Completion {
            completed: 0,
            queue: WaitQueue::EMPTY,
        },
        submitted_at: STAMP,
        started_at: STAMP,
        data: [0; MAX_MESSAGE_SIZE],
    };
}
struct Channel {
    lock: Mutex,
    slots_available: Semaphore,
    generation: u64,
    next_sequence: u64,
    active: bool,
    id: u32,
    opens: u64,
    closes: u64,
    sends: u64,
    receives: u64,
    drops: u64,
    service_name: Bytes,
    handler: Option<Handler>,
    parallel: bool,
    handler_owner: u32,
    queue: [Message; QUEUE_DEPTH],
    lifetime: Lifetime,
}
impl Channel {
    const EMPTY: Self = Self {
        lock: Mutex {
            rank: 130,
            name: Text::literal(b"service-ipc-channel"),
            ..Mutex::EMPTY
        },
        slots_available: Semaphore {
            count: QUEUE_DEPTH as u32,
            max_count: QUEUE_DEPTH as u32,
            queue: WaitQueue::EMPTY,
        },
        generation: 1,
        next_sequence: 1,
        active: false,
        id: 0,
        opens: 0,
        closes: 0,
        sends: 0,
        receives: 0,
        drops: 0,
        service_name: Bytes {
            data: ptr::null(),
            len: 0,
        },
        handler: None,
        parallel: false,
        handler_owner: 0,
        queue: [const { Message::EMPTY }; QUEUE_DEPTH],
        lifetime: Lifetime::EMPTY,
    };
}
struct Worker {
    task_id: AtomicU32,
    cursor: usize,
    event: Event,
    response: [u8; MAX_MESSAGE_SIZE],
}
impl Worker {
    const fn new() -> Self {
        Self {
            task_id: AtomicU32::new(0),
            cursor: 0,
            event: Event::AUTO,
            response: [0; MAX_MESSAGE_SIZE],
        }
    }
}
struct Channels(UnsafeCell<[Channel; MAX_CHANNELS]>);
// SAFETY: each stable channel's original mutex admits metadata and payload;
// boot is single BSP before tasks. No whole-channel assignment/recycling.
unsafe impl Sync for Channels {}
static CHANNELS: Channels = Channels(UnsafeCell::new([const { Channel::EMPTY }; MAX_CHANNELS]));
struct Workers(UnsafeCell<[Worker; MAX_WORKERS]>);
// SAFETY: boot publishes immutable task IDs before markReady. Each exact
// worker owns cursor/response; Event has its independent scheduler admission.
unsafe impl Sync for Workers {}
static WORKERS: Workers = Workers(UnsafeCell::new([const { Worker::new() }; MAX_WORKERS]));
static AP_WORKERS: AtomicU32 = AtomicU32::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static WORKER_TASK_ID: AtomicU32 = AtomicU32::new(0);
static SENDS: AtomicU64 = AtomicU64::new(0);
static RECEIVES: AtomicU64 = AtomicU64::new(0);
static DROPS: AtomicU64 = AtomicU64::new(0);
static ERRORS: AtomicU64 = AtomicU64::new(0);
static ECHO_TESTS: AtomicU64 = AtomicU64::new(0);
static IRQ_DENIED: AtomicU64 = AtomicU64::new(0);
static IDLE_WAITS: AtomicU64 = AtomicU64::new(0);
static WAKES: AtomicU64 = AtomicU64::new(0);
static EXECUTED_CPUS: AtomicU64 = AtomicU64::new(0);
pub(crate) fn execution_cpus() -> u64 {
    EXECUTED_CPUS.load(Ordering::Relaxed)
}
pub type Handler = unsafe fn(u32, Bytes, *mut u8, usize) -> i32;

fn channel(index: usize) -> *mut Channel {
    assert!(index < MAX_CHANNELS);
    CHANNELS.0.get().cast::<Channel>().wrapping_add(index)
}
fn worker(index: usize) -> *mut Worker {
    assert!(index < MAX_WORKERS);
    WORKERS.0.get().cast::<Worker>().wrapping_add(index)
}
fn index(id: u32) -> Option<usize> {
    (id != 0 && id as usize <= MAX_CHANNELS).then(|| (id - 1) as usize)
}
fn increment(value: &AtomicU64, amount: u64) {
    value.fetch_add(amount, Ordering::Relaxed);
}
fn fail() -> i32 {
    increment(&ERRORS, 1);
    -1
}
fn deny_irq() -> bool {
    if irq_router::r4os_rust_irq_router_local(false) == 0 {
        return false;
    }
    increment(&IRQ_DENIED, 1);
    true
}
fn next(value: u64) -> u64 {
    let n = value.wrapping_add(1);
    if n == 0 { 1 } else { n }
}
fn stamp() -> monotonic::Stamp {
    if config::ENABLE_METRICS {
        monotonic::capture()
    } else {
        STAMP
    }
}
fn remaining(deadline: u64, forever: bool) -> u64 {
    if forever {
        FOREVER
    } else {
        deadline.saturating_sub(timer::tick_count())
    }
}
fn deadline(ticks: u64) -> u64 {
    clock_math::finite_deadline(timer::tick_count(), ticks)
}
unsafe fn same_text(a: Bytes, b: Bytes) -> bool {
    if a.len != b.len {
        return false;
    }
    for i in 0..a.len {
        if unsafe { a.data.add(i).read() != b.data.add(i).read() } {
            return false;
        }
    }
    true
}
struct ChannelGuard {
    channel: *mut Channel,
    locked: bool,
}
impl ChannelGuard {
    fn acquire(index: usize) -> Option<Self> {
        let ch = channel(index);
        if scheduler::current().is_null() {
            return Some(Self {
                channel: ch,
                locked: false,
            });
        }
        unsafe {
            let mutex = ptr::addr_of_mut!((*ch).lock);
            let immediate = sync_locks::r4os_rust_sync_mutex(mutex, 0, 0);
            if !immediate && !sync_locks::r4os_rust_sync_mutex(mutex, 1, FOREVER) {
                return None;
            }
            if !immediate {
                (*ch).lifetime.lock_contentions = (*ch).lifetime.lock_contentions.wrapping_add(1);
            }
        }
        Some(Self {
            channel: ch,
            locked: true,
        })
    }
    fn release(&mut self) {
        if self.locked {
            unsafe {
                sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*self.channel).lock), 2, 0)
            };
        }
        self.locked = false;
    }
}
impl Drop for ChannelGuard {
    fn drop(&mut self) {
        self.release();
    }
}
unsafe extern "C" fn release_channel(raw: *mut c_void) {
    unsafe { (*raw.cast::<ChannelGuard>()).release() };
}
unsafe fn select(ch: *const Channel, operation: u8, capacity: usize) -> queue::Decision {
    let mut result = queue::Decision::default();
    let view = queue::View {
        base: unsafe { ptr::addr_of!((*ch).queue).cast() },
        len: QUEUE_DEPTH,
        stride: mem::size_of::<Message>(),
        meta_offset: mem::offset_of!(Message, meta),
    };
    unsafe { queue::r4os_rust_service_queue_query(view, operation, capacity, &mut result) };
    result
}
unsafe fn message(ch: *mut Channel, slot: usize) -> *mut Message {
    assert!(slot < QUEUE_DEPTH);
    unsafe { ptr::addr_of_mut!((*ch).queue).cast::<Message>().add(slot) }
}
unsafe fn release_permit(ch: *mut Channel) {
    unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*ch).slots_available),
            4,
            1,
            ptr::null(),
            ptr::null_mut(),
        )
    };
}
unsafe fn try_permit(ch: *mut Channel) -> bool {
    unsafe {
        sync_objects::r4os_rust_sync_semaphore(
            ptr::addr_of_mut!((*ch).slots_available),
            0,
            0,
            ptr::null(),
            ptr::null_mut(),
        ) != 0
    }
}
unsafe fn activate(ch: *mut Channel, id: u32) {
    unsafe {
        (*ch).active = true;
        (*ch).id = id;
        (*ch).generation = next((*ch).generation);
    }
}
unsafe fn reset(slot: *mut Message) {
    unsafe {
        let generation = (*slot).meta.generation;
        (*slot).meta = queue::SlotMeta {
            generation,
            ..queue::SlotMeta::default()
        };
        (*slot).channel_generation = 0;
        (*slot).handler = None;
        (*slot).parallel = false;
        (*slot).submitted_at = STAMP;
        (*slot).started_at = STAMP;
    }
}
unsafe fn prepare(ch: *mut Channel, slot: *mut Message, mode: u8, len: usize) {
    unsafe {
        let generation = next((*slot).meta.generation);
        let sequence = (*ch).next_sequence;
        (*ch).next_sequence = next(sequence);
        (*slot).meta = queue::SlotMeta {
            mode,
            generation,
            sequence,
            len: len as u16,
            ..queue::SlotMeta::default()
        };
        (*slot).channel_generation = (*ch).generation;
        (*slot).handler = None;
        (*slot).parallel = false;
        (*slot).completion = Completion {
            completed: 0,
            queue: WaitQueue::EMPTY,
        };
        (*slot).submitted_at = STAMP;
        (*slot).started_at = STAMP;
    }
}
fn parallel_workers_available() -> bool {
    AP_WORKERS.load(Ordering::Acquire) != 0 && crate::net::adapters::parallel_callbacks()
}
fn signal(index: usize) {
    unsafe { sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*worker(index)).event), 0, 0) };
    let count = AP_WORKERS.load(Ordering::Acquire) as usize;
    if count != 0 && parallel_workers_available() {
        unsafe {
            sync_objects::r4os_rust_sync_event(
                ptr::addr_of_mut!((*worker(WORKER_COUNT + index % count)).event),
                0,
                0,
            )
        };
    }
}
fn current_worker() -> Option<usize> {
    let current = scheduler::current();
    if current.is_null() {
        return None;
    }
    let (id, _) = unsafe { task::immutable_identity(current) };
    (0..MAX_WORKERS).find(|&i| unsafe { (*worker(i)).task_id.load(Ordering::Acquire) == id })
}
fn start_workers() -> bool {
    if STARTED.load(Ordering::Acquire) {
        return true;
    }
    let online = crate::arch::percpu::r4os_rust_percpu_control(14, 0, 0, 0).count_ones() as usize;
    let capacity = WORKER_COUNT + online.saturating_sub(1).min(MAX_AP_WORKERS);
    for lane in 0..capacity {
        if unsafe { (*worker(lane)).task_id.load(Ordering::Acquire) } != 0 {
            continue;
        }
        let parallel = lane >= WORKER_COUNT;
        let name: &[u8] = if parallel { b"ipc-ap" } else { b"ipc-worker" };
        let input = task::task_api::CreateRequest {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 3,
            role: 1,
            needs_fpu: true,
            smp_eligible: parallel,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(worker_main),
        };
        let mut failure = 0;
        let created = unsafe { task::task_api::r4os_rust_task_create(&input, false, &mut failure) };
        if created.is_null() {
            if parallel {
                break;
            }
            return false;
        }
        let (id, _) = unsafe { task::immutable_identity(created) };
        unsafe {
            (*worker(lane)).task_id.store(id, Ordering::Release);
        }
        scheduler::publish_created(created);
    }
    let ap_count = (WORKER_COUNT..MAX_WORKERS)
        .filter(|&i| unsafe { (*worker(i)).task_id.load(Ordering::Acquire) != 0 })
        .count();
    AP_WORKERS.store(ap_count as u32, Ordering::Release);
    WORKER_TASK_ID.store(
        unsafe { (*worker(0)).task_id.load(Ordering::Acquire) },
        Ordering::Release,
    );
    STARTED.store(true, Ordering::Release);
    true
}
pub(crate) fn open(id: u32) -> i32 {
    if deny_irq() {
        return fail();
    }
    let Some(i) = index(id) else {
        return fail();
    };
    let Some(guard) = ChannelGuard::acquire(i) else {
        return fail();
    };
    unsafe {
        let ch = guard.channel;
        if !(*ch).active {
            activate(ch, id);
        }
        (*ch).opens = (*ch).opens.wrapping_add(1);
    }
    id as i32
}
fn control(id: u32, operation: u8) -> i32 {
    if deny_irq() {
        return fail();
    }
    let Some(i) = index(id) else {
        return fail();
    };
    let Some(guard) = ChannelGuard::acquire(i) else {
        return fail();
    };
    unsafe {
        let ch = guard.channel;
        if !(*ch).active {
            return fail();
        }
        if operation == 0 {
            (*ch).closes = (*ch).closes.wrapping_add(1);
            0
        } else {
            select(ch, 4, 0).selection.index as i32
        }
    }
}
unsafe fn publish_ready(
    id: u32,
    payload: Bytes,
    state: u8,
    count_call: bool,
    count_send: bool,
) -> i32 {
    unsafe {
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(mut guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        if !(*ch).active {
            activate(ch, id);
        }
        if count_call {
            (*ch).lifetime.send_calls = (*ch).lifetime.send_calls.wrapping_add(1);
        }
        if !try_permit(ch) {
            (*ch).lifetime.queue_full = (*ch).lifetime.queue_full.wrapping_add(1);
            (*ch).drops = (*ch).drops.wrapping_add(1);
            increment(&DROPS, 1);
            guard.release();
            return fail();
        }
        let selection = select(ch, 0, 0);
        if selection.kind == 0 {
            guard.release();
            release_permit(ch);
            return fail();
        }
        let slot = message(ch, selection.selection.index);
        prepare(ch, slot, queue::RAW, payload.len);
        if payload.len != 0 {
            ptr::copy_nonoverlapping(
                payload.data,
                ptr::addr_of_mut!((*slot).data).cast(),
                payload.len,
            );
            (*ch).lifetime.payload_copy_bytes = (*ch)
                .lifetime
                .payload_copy_bytes
                .wrapping_add(payload.len as u64);
        }
        (*ch).lifetime.request_bytes = (*ch)
            .lifetime
            .request_bytes
            .wrapping_add(payload.len as u64);
        (*slot).meta.state = state;
        if count_send {
            (*ch).sends = (*ch).sends.wrapping_add(1);
            increment(&SENDS, 1);
        }
        payload.len as i32
    }
}
pub(crate) unsafe fn recv(id: u32, output: *mut u8, capacity: usize) -> i32 {
    unsafe {
        if deny_irq() {
            return fail();
        }
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(mut guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        if !(*ch).active {
            guard.release();
            return fail();
        }
        (*ch).lifetime.recv_calls = (*ch).lifetime.recv_calls.wrapping_add(1);
        (*ch).lifetime.response_searches = (*ch).lifetime.response_searches.wrapping_add(1);
        let decision = select(ch, 3, capacity);
        let mut release = false;
        let result = match decision.kind {
            0 => {
                (*ch).lifetime.queue_empty = (*ch).lifetime.queue_empty.wrapping_add(1);
                0
            }
            2 => {
                (*ch).lifetime.response_search_slots = (*ch)
                    .lifetime
                    .response_search_slots
                    .wrapping_add(decision.selection.visits as u64);
                (*ch).lifetime.recv_buffer_small = (*ch).lifetime.recv_buffer_small.wrapping_add(1);
                (*ch).drops = (*ch).drops.wrapping_add(1);
                increment(&DROPS, 1);
                reset(message(ch, decision.selection.index));
                release = true;
                -1
            }
            1 => {
                (*ch).lifetime.response_search_slots = (*ch)
                    .lifetime
                    .response_search_slots
                    .wrapping_add(decision.selection.visits as u64);
                let slot = message(ch, decision.selection.index);
                let len = usize::from((*slot).meta.len);
                if len != 0 {
                    ptr::copy_nonoverlapping(ptr::addr_of!((*slot).data).cast(), output, len);
                    (*ch).lifetime.payload_copy_bytes =
                        (*ch).lifetime.payload_copy_bytes.wrapping_add(len as u64);
                }
                (*ch).receives = (*ch).receives.wrapping_add(1);
                increment(&RECEIVES, 1);
                reset(slot);
                release = true;
                len as i32
            }
            _ => unreachable!(),
        };
        guard.release();
        if release {
            release_permit(ch);
        }
        if result < 0 {
            increment(&ERRORS, 1);
        }
        result
    }
}

#[derive(Clone, Copy)]
struct HandlerTarget {
    channel: usize,
    _channel_generation: u64,
    slot: usize,
    slot_generation: u64,
    completion: *mut Completion,
    request_len: usize,
    deadline: u64,
    forever: bool,
}
struct Admission {
    target: HandlerTarget,
    unwind: task_context::Unwind,
}
fn enter_unwind() -> task_context::Unwind {
    let _guard = crate::arch::runtime::RuntimeGuard::acquire();
    unsafe { task_context::enter() }
}
fn leave_unwind(unwind: task_context::Unwind) {
    if unwind.active {
        let _guard = crate::arch::runtime::RuntimeGuard::acquire();
        unsafe { task_context::leave(unwind) };
    }
}
// Preserve the original independently timed admission and completion contracts.
#[allow(clippy::too_many_arguments)]
unsafe fn admit_handler(
    id: u32,
    payload: Bytes,
    mode: u8,
    admission_timeout: u64,
    completion_deadline: u64,
    completion_forever: bool,
) -> Option<Admission> {
    unsafe {
        let i = index(id)?;
        let admission_forever = admission_timeout == FOREVER;
        let admission_deadline = if admission_forever {
            0
        } else {
            deadline(admission_timeout)
        };
        let mut guard = ChannelGuard::acquire(i)?;
        let mut ch = guard.channel;
        if !(*ch).active {
            activate(ch, id);
        }
        let generation = (*ch).generation;
        let handler = (*ch).handler;
        let parallel = (*ch).parallel;
        handler?;
        if mode == queue::QUEUED_RESPONSE {
            (*ch).lifetime.send_calls = (*ch).lifetime.send_calls.wrapping_add(1);
        } else {
            (*ch).lifetime.request_calls = (*ch).lifetime.request_calls.wrapping_add(1);
        }
        let mut unwind = enter_unwind();
        if !unwind.admitted() {
            return None;
        }
        if !try_permit(ch) {
            leave_unwind(unwind);
            unwind = task_context::Unwind::EMPTY;
            if admission_timeout == 0 {
                (*ch).lifetime.queue_full = (*ch).lifetime.queue_full.wrapping_add(1);
                (*ch).drops = (*ch).drops.wrapping_add(1);
                increment(&DROPS, 1);
                return None;
            }
            (*ch).lifetime.admission_waits = (*ch).lifetime.admission_waits.wrapping_add(1);
            let request = WaitRequest {
                timeout: remaining(admission_deadline, admission_forever),
                reason: Text::literal(b"semaphore"),
                predicate: None,
                context: ptr::null_mut(),
                release: Some(release_channel),
                release_context: ptr::from_mut(&mut guard).cast(),
                flags: 0,
                consume_runtime: false,
            };
            let result = sync_objects::r4os_rust_sync_semaphore(
                ptr::addr_of_mut!((*ch).slots_available),
                3,
                0,
                &request,
                &mut unwind,
            );
            let Some(next_guard) = ChannelGuard::acquire(i) else {
                release_permit(channel(i));
                leave_unwind(unwind);
                return None;
            };
            guard = next_guard;
            ch = guard.channel;
            if result != 1 {
                if result == 2 {
                    (*ch).lifetime.admission_timeouts =
                        (*ch).lifetime.admission_timeouts.wrapping_add(1);
                }
                guard.release();
                leave_unwind(unwind);
                return None;
            }
            if !(*ch).active
                || (*ch).generation != generation
                || (*ch).handler.map(|f| f as usize) != handler.map(|f| f as usize)
            {
                guard.release();
                release_permit(ch);
                leave_unwind(unwind);
                return None;
            }
        }
        let selection = select(ch, 0, 0);
        if selection.kind == 0 {
            guard.release();
            release_permit(ch);
            leave_unwind(unwind);
            return None;
        }
        let slot = message(ch, selection.selection.index);
        prepare(ch, slot, mode, payload.len);
        (*slot).channel_generation = (*ch).generation;
        (*slot).handler = handler;
        (*slot).parallel = parallel;
        (*slot).submitted_at = stamp();
        if payload.len != 0 {
            ptr::copy_nonoverlapping(
                payload.data,
                ptr::addr_of_mut!((*slot).data).cast(),
                payload.len,
            );
            (*ch).lifetime.payload_copy_bytes = (*ch)
                .lifetime
                .payload_copy_bytes
                .wrapping_add(payload.len as u64);
        }
        (*ch).lifetime.request_bytes = (*ch)
            .lifetime
            .request_bytes
            .wrapping_add(payload.len as u64);
        (*ch).lifetime.handler_queued = (*ch).lifetime.handler_queued.wrapping_add(1);
        (*ch).lifetime.handler_waits = (*ch).lifetime.handler_waits.wrapping_add(1);
        (*slot).meta.state = queue::HANDLER_QUEUED;
        let target = HandlerTarget {
            channel: i,
            _channel_generation: (*ch).generation,
            slot: selection.selection.index,
            slot_generation: (*slot).meta.generation,
            completion: ptr::addr_of_mut!((*slot).completion),
            request_len: payload.len,
            deadline: completion_deadline,
            forever: completion_forever,
        };
        guard.release();
        Some(Admission { target, unwind })
    }
}
unsafe fn finish_handler(
    target: HandlerTarget,
    wait_result: u32,
    output: *mut u8,
    capacity: Option<usize>,
) -> i32 {
    unsafe {
        let Some(mut guard) = ChannelGuard::acquire(target.channel) else {
            return fail();
        };
        let ch = guard.channel;
        let slot = message(ch, target.slot);
        let mut release = false;
        let mut result = -1;
        if (*slot).meta.generation != target.slot_generation {
            (*ch).lifetime.stale_drops = (*ch).lifetime.stale_drops.wrapping_add(1);
        } else if wait_result != 1 {
            if wait_result == 2 {
                (*ch).lifetime.handler_wait_timeouts =
                    (*ch).lifetime.handler_wait_timeouts.wrapping_add(1);
            }
            match queue::timeout((*slot).meta, target.slot_generation) {
                1 => {
                    reset(slot);
                    release = true;
                }
                2 => (*slot).meta.abandoned = true,
                0 => {}
                _ => unreachable!(),
            }
        } else {
            match queue::completion((*slot).meta, target.slot_generation, capacity) {
                1 => {
                    reset(slot);
                    release = true;
                }
                2 => {
                    (*slot).meta.state = queue::HANDLER_READY;
                    (*ch).sends = (*ch).sends.wrapping_add(1);
                    increment(&SENDS, 1);
                    result = target.request_len as i32;
                }
                4 => {
                    (*ch).lifetime.recv_buffer_small =
                        (*ch).lifetime.recv_buffer_small.wrapping_add(1);
                    reset(slot);
                    release = true;
                }
                3 => {
                    assert!(capacity.is_some());
                    let len = usize::from((*slot).meta.len);
                    if len != 0 {
                        ptr::copy_nonoverlapping(ptr::addr_of!((*slot).data).cast(), output, len);
                        (*ch).lifetime.payload_copy_bytes =
                            (*ch).lifetime.payload_copy_bytes.wrapping_add(len as u64);
                    }
                    (*ch).sends = (*ch).sends.wrapping_add(1);
                    (*ch).receives = (*ch).receives.wrapping_add(1);
                    increment(&SENDS, 1);
                    increment(&RECEIVES, 1);
                    result = len as i32;
                    reset(slot);
                    release = true;
                }
                0 => (*ch).lifetime.stale_drops = (*ch).lifetime.stale_drops.wrapping_add(1),
                _ => unreachable!(),
            }
        }
        guard.release();
        if release {
            release_permit(ch);
        }
        if result < 0 {
            increment(&ERRORS, 1);
        }
        result
    }
}
unsafe fn submit_handler(
    id: u32,
    payload: Bytes,
    output: *mut u8,
    capacity: Option<usize>,
    mode: u8,
    admission_timeout: u64,
    completion_timeout: u64,
) -> i32 {
    unsafe {
        if !STARTED.load(Ordering::Acquire) {
            return fail();
        }
        let forever = completion_timeout == FOREVER;
        let end = if forever {
            0
        } else {
            deadline(completion_timeout)
        };
        let Some(admission) = admit_handler(id, payload, mode, admission_timeout, end, forever)
        else {
            return fail();
        };
        signal(admission.target.channel % WORKER_COUNT);
        let result = sync_objects::r4os_rust_sync_completion(
            admission.target.completion,
            4,
            remaining(admission.target.deadline, admission.target.forever),
        );
        let result = finish_handler(admission.target, result, output, capacity);
        leave_unwind(admission.unwind);
        result
    }
}
enum Timing {
    Queue,
    Run,
    EndToEnd,
}
unsafe fn note_duration(
    ch: *mut Channel,
    start: monotonic::Stamp,
    end: monotonic::Stamp,
    kind: Timing,
) {
    if !config::ENABLE_METRICS {
        return;
    }
    unsafe {
        let lifetime = &mut (*ch).lifetime;
        let Some(elapsed) = monotonic::elapsed(start, end) else {
            lifetime.timing_unavailable = lifetime.timing_unavailable.wrapping_add(1);
            return;
        };
        let (total, max) = match kind {
            Timing::Queue => (
                &mut lifetime.handler_queue_ns,
                &mut lifetime.handler_queue_max_ns,
            ),
            Timing::Run => (
                &mut lifetime.handler_run_ns,
                &mut lifetime.handler_run_max_ns,
            ),
            Timing::EndToEnd => (
                &mut lifetime.handler_e2e_ns,
                &mut lifetime.handler_e2e_max_ns,
            ),
        };
        *total = total.wrapping_add(elapsed);
        *max = (*max).max(elapsed);
    }
}
unsafe fn invoke_direct(
    id: u32,
    payload: Bytes,
    response: *mut u8,
    capacity: usize,
    count_request: bool,
) -> i32 {
    unsafe {
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(mut guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        let handler = (*ch).handler;
        if handler.is_none() {
            guard.release();
            return fail();
        }
        let current = scheduler::current();
        let caller = if current.is_null() {
            0
        } else {
            task::immutable_identity(current).0
        };
        let previous = (*ch).handler_owner;
        if previous != 0 && previous != caller {
            guard.release();
            return fail();
        }
        (*ch).handler_owner = caller;
        let generation = (*ch).generation;
        (*ch).lifetime.handler_direct = (*ch).lifetime.handler_direct.wrapping_add(1);
        (*ch).lifetime.handler_started = (*ch).lifetime.handler_started.wrapping_add(1);
        (*ch).lifetime.request_bytes = (*ch)
            .lifetime
            .request_bytes
            .wrapping_add(payload.len as u64);
        if count_request {
            (*ch).lifetime.request_calls = (*ch).lifetime.request_calls.wrapping_add(1);
        }
        guard.release();
        let started = stamp();
        let produced = handler.unwrap()(id, payload, response, capacity);
        let completed = stamp();
        let Some(mut guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        (*ch).handler_owner = previous;
        (*ch).lifetime.handler_completed = (*ch).lifetime.handler_completed.wrapping_add(1);
        note_duration(ch, started, completed, Timing::Run);
        note_duration(ch, started, completed, Timing::EndToEnd);
        let valid =
            (*ch).generation == generation && produced >= 0 && produced as usize <= capacity;
        if valid {
            (*ch).lifetime.response_bytes =
                (*ch).lifetime.response_bytes.wrapping_add(produced as u64);
        } else {
            (*ch).lifetime.handler_failures = (*ch).lifetime.handler_failures.wrapping_add(1);
        }
        guard.release();
        if previous == 0 {
            signal(i % WORKER_COUNT);
        }
        if valid { produced } else { fail() }
    }
}
unsafe fn direct_send(id: u32, payload: Bytes) -> i32 {
    unsafe {
        let mut response = MaybeUninit::<[u8; MAX_MESSAGE_SIZE]>::uninit();
        let data = response.as_mut_ptr().cast();
        let produced = invoke_direct(id, payload, data, MAX_MESSAGE_SIZE, false);
        if produced < 0 {
            return produced;
        }
        let queued = publish_ready(
            id,
            Bytes {
                data,
                len: produced as usize,
            },
            queue::HANDLER_READY,
            true,
            true,
        );
        if queued < 0 {
            queued
        } else {
            payload.len as i32
        }
    }
}
unsafe fn direct_request(id: u32, payload: Bytes, output: *mut u8, capacity: usize) -> i32 {
    unsafe {
        let mut response = MaybeUninit::<[u8; MAX_MESSAGE_SIZE]>::uninit();
        let data = response.as_mut_ptr().cast::<u8>();
        let produced = invoke_direct(id, payload, data, MAX_MESSAGE_SIZE, true);
        if produced < 0 {
            return produced;
        }
        let len = produced as usize;
        if capacity < len {
            return fail();
        }
        if len != 0 {
            ptr::copy_nonoverlapping(data, output, len);
        }
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        (*ch).lifetime.payload_copy_bytes =
            (*ch).lifetime.payload_copy_bytes.wrapping_add(len as u64);
        (*ch).sends = (*ch).sends.wrapping_add(1);
        (*ch).receives = (*ch).receives.wrapping_add(1);
        increment(&SENDS, 1);
        increment(&RECEIVES, 1);
        produced
    }
}
pub(crate) unsafe fn send(id: u32, payload: Bytes) -> i32 {
    unsafe {
        if payload.len > MAX_MESSAGE_SIZE || deny_irq() {
            return fail();
        }
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(mut guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        if !(*ch).active {
            activate(ch, id);
        }
        let handler = (*ch).handler.is_some();
        let on_worker = current_worker().is_some();
        guard.release();
        if !handler {
            return publish_ready(id, payload, queue::RAW_READY, true, true);
        }
        if on_worker {
            return direct_send(id, payload);
        }
        submit_handler(
            id,
            payload,
            ptr::null_mut(),
            None,
            queue::QUEUED_RESPONSE,
            0,
            FOREVER,
        )
    }
}
pub(crate) unsafe fn request(
    id: u32,
    payload: Bytes,
    output: *mut u8,
    capacity: usize,
    timeout: u64,
) -> i32 {
    unsafe {
        if payload.len > MAX_MESSAGE_SIZE || capacity > MAX_MESSAGE_SIZE || deny_irq() {
            return fail();
        }
        if current_worker().is_some() {
            return direct_request(id, payload, output, capacity);
        }
        submit_handler(
            id,
            payload,
            output,
            Some(capacity),
            queue::DIRECT_RESPONSE,
            timeout,
            timeout,
        )
    }
}

#[derive(Clone, Copy)]
struct WorkerTarget {
    channel: usize,
    channel_generation: u64,
    slot: usize,
    slot_generation: u64,
    handler: Option<Handler>,
    parallel: bool,
}
unsafe fn take_next(lane: usize) -> Option<WorkerTarget> {
    let on_ap_worker = lane >= WORKER_COUNT;
    // Native handlers still validate their exact backend at each callback;
    // topology changes may make a previously accepted operation return Busy.
    let parallel_backends = parallel_workers_available();
    if on_ap_worker && !parallel_backends {
        return None;
    }
    unsafe {
        let cursor = (*worker(lane)).cursor;
        for offset in 0..MAX_CHANNELS {
            let i = (cursor + offset) % MAX_CHANNELS;
            if !on_ap_worker && i % WORKER_COUNT != lane {
                continue;
            }
            let Some(mut guard) = ChannelGuard::acquire(i) else {
                continue;
            };
            let ch = guard.channel;
            if (*ch).handler_owner != 0 {
                continue;
            }
            let selection = select(ch, 2, 0);
            if selection.kind == 0 {
                continue;
            }
            let slot = message(ch, selection.selection.index);
            let parallel = (*slot).parallel && parallel_backends;
            // Original BSP lanes may help qualified work as well. A bounded
            // AP pool must not reduce the independent waiting capacity when
            // several DNS/TCP handlers are already blocked.
            if on_ap_worker && !parallel {
                continue;
            }
            let handler = (*slot).handler;
            if handler.is_none() {
                (*slot).meta.result = -1;
                (*slot).meta.state = queue::HANDLER_DONE;
                let completion = ptr::addr_of_mut!((*slot).completion);
                guard.release();
                sync_objects::r4os_rust_sync_completion(completion, 0, 0);
                continue;
            }
            (*slot).meta.state = queue::HANDLER_RUNNING;
            (*slot).started_at = stamp();
            (*ch).lifetime.handler_started = (*ch).lifetime.handler_started.wrapping_add(1);
            note_duration(ch, (*slot).submitted_at, (*slot).started_at, Timing::Queue);
            (*worker(lane)).cursor = (i + 1) % MAX_CHANNELS;
            (*ch).handler_owner = (*worker(lane)).task_id.load(Ordering::Acquire);
            let target = WorkerTarget {
                channel: i,
                channel_generation: (*slot).channel_generation,
                slot: selection.selection.index,
                slot_generation: (*slot).meta.generation,
                handler,
                parallel: (*slot).parallel,
            };
            guard.release();
            return Some(target);
        }
        None
    }
}
unsafe fn publish_completion(target: WorkerTarget) {
    unsafe {
        let Some(mut guard) = ChannelGuard::acquire(target.channel) else {
            return;
        };
        let ch = guard.channel;
        let slot = message(ch, target.slot);
        if (*slot).meta.generation != target.slot_generation
            || (*slot).meta.state != queue::HANDLER_DONE
        {
            return;
        }
        (*slot).meta.completion_published = true;
        let abandoned = (*slot).meta.abandoned;
        if abandoned {
            reset(slot);
            (*ch).lifetime.stale_drops = (*ch).lifetime.stale_drops.wrapping_add(1);
        }
        guard.release();
        if abandoned {
            release_permit(ch);
        }
    }
}
unsafe fn run_target(lane: usize, target: WorkerTarget) {
    unsafe {
        let ch = channel(target.channel);
        let slot = message(ch, target.slot);
        let payload = Bytes {
            data: ptr::addr_of!((*slot).data).cast(),
            len: usize::from((*slot).meta.len),
        };
        let response = ptr::addr_of_mut!((*worker(lane)).response).cast();
        let cpu = crate::arch::percpu::current_index();
        let produced = target.handler.unwrap()((*ch).id, payload, response, MAX_MESSAGE_SIZE);
        if target.parallel && produced >= 0 {
            EXECUTED_CPUS.fetch_or(1u64 << cpu, Ordering::Relaxed);
        }
        let completed = stamp();
        let Some(mut guard) = ChannelGuard::acquire(target.channel) else {
            return;
        };
        let ch = guard.channel;
        (*ch).handler_owner = 0;
        let slot = message(ch, target.slot);
        if (*slot).meta.generation != target.slot_generation
            || (*slot).meta.state != queue::HANDLER_RUNNING
        {
            (*ch).lifetime.stale_drops = (*ch).lifetime.stale_drops.wrapping_add(1);
            return;
        }
        (*ch).lifetime.handler_completed = (*ch).lifetime.handler_completed.wrapping_add(1);
        note_duration(ch, (*slot).started_at, completed, Timing::Run);
        note_duration(ch, (*slot).submitted_at, completed, Timing::EndToEnd);
        let valid = (*ch).generation == target.channel_generation
            && produced >= 0
            && produced as usize <= MAX_MESSAGE_SIZE;
        (*slot).meta.result = if valid { produced } else { -1 };
        if valid {
            let len = produced as usize;
            if len != 0 {
                ptr::copy_nonoverlapping(response, ptr::addr_of_mut!((*slot).data).cast(), len);
                (*ch).lifetime.payload_copy_bytes =
                    (*ch).lifetime.payload_copy_bytes.wrapping_add(len as u64);
            }
            (*slot).meta.len = len as u16;
            (*ch).lifetime.response_bytes = (*ch).lifetime.response_bytes.wrapping_add(len as u64);
        } else {
            (*slot).meta.len = 0;
            (*ch).lifetime.handler_failures = (*ch).lifetime.handler_failures.wrapping_add(1);
        }
        if (*slot).meta.abandoned {
            reset(slot);
            (*ch).lifetime.stale_drops = (*ch).lifetime.stale_drops.wrapping_add(1);
            guard.release();
            release_permit(ch);
            return;
        }
        (*slot).meta.state = queue::HANDLER_DONE;
        let completion = ptr::addr_of_mut!((*slot).completion);
        guard.release();
        sync_objects::r4os_rust_sync_completion(completion, 0, 0);
        publish_completion(target);
    }
}
unsafe extern "C" fn worker_main() {
    let Some(lane) = current_worker() else {
        return;
    };
    loop {
        if let Some(target) = unsafe { take_next(lane) } {
            unsafe { run_target(lane, target) };
            // A previous BSP owner can finish after AP admission was published.
            // Wake its queued successor only after the exact handler settled.
            let pending = if let Some(guard) = ChannelGuard::acquire(target.channel) {
                unsafe { select(guard.channel, 2, 0).kind != 0 }
            } else {
                false
            };
            if pending {
                signal(target.channel % WORKER_COUNT);
            }
            // Every handler outcome has relinquished its ChannelGuard and
            // settled its exact message/completion/permit before this point.
            // This also covers abandoned or stale early-return outcomes.
            scheduler::safe_reschedule();
            continue;
        }
        increment(&IDLE_WAITS, 1);
        let result = unsafe {
            sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*worker(lane)).event), 2, FOREVER)
        };
        if result == 1 {
            increment(&WAKES, 1);
        }
    }
}
unsafe fn echo_smoke() -> bool {
    unsafe {
        let payload = b"R4IPC-ECHO";
        let mut output = MaybeUninit::<[u8; MAX_MESSAGE_SIZE]>::uninit();
        if send(
            1,
            Bytes {
                data: payload.as_ptr(),
                len: payload.len(),
            },
        ) != payload.len() as i32
        {
            return false;
        }
        if recv(1, output.as_mut_ptr().cast(), MAX_MESSAGE_SIZE) != payload.len() as i32 {
            return false;
        }
        let same = same_text(
            Bytes {
                data: output.as_ptr().cast(),
                len: payload.len(),
            },
            Bytes {
                data: payload.as_ptr(),
                len: payload.len(),
            },
        );
        if same {
            increment(&ECHO_TESTS, 1);
        }
        same
    }
}
/// # Safety
/// Boot init/worker start retain their original singleton BSP admission.
/// Remaining operations use the stable channel ID and native channel mutex.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ipc_control(operation: u8, id: u32) -> i32 {
    match operation {
        0 => {
            if !INITIALIZED.swap(true, Ordering::AcqRel) {
                open(1);
            }
            0
        }
        1 => i32::from(start_workers()),
        2 => open(id),
        3 => control(id, 0),
        4 => control(id, 1),
        5 => i32::from(unsafe { echo_smoke() }),
        _ => panic!("Invalid private IPC operation"),
    }
}
/// # Safety
/// Name and typed resident handler retain their kernel lifetime.
/// `parallel` additionally requires an audited complete native handler path;
/// it never admits a foreign callback missing its own backend declaration.
pub unsafe fn register(id: u32, name: Bytes, handler: Option<Handler>, parallel: bool) -> i32 {
    unsafe {
        if deny_irq() {
            return fail();
        }
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        if !(*ch).active {
            activate(ch, id);
        }
        if (*ch).handler.map(|f| f as usize) != handler.map(|f| f as usize)
            || !same_text((*ch).service_name, name)
            || (*ch).parallel != parallel
        {
            (*ch).generation = next((*ch).generation);
        }
        (*ch).service_name = name;
        (*ch).handler = handler;
        (*ch).parallel = parallel;
        0
    }
}
/// # Safety
/// Input/output buffers retain the original caller lifetime through the call,
/// including any admitted handler wait. No mutable channel borrow crosses it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ipc_io(
    operation: u8,
    id: u32,
    input: Bytes,
    output: *mut u8,
    capacity: usize,
    timeout: u64,
) -> i32 {
    unsafe {
        match operation {
            0 => send(id, input),
            1 => request(id, input, output, capacity, timeout),
            2 => recv(id, output, capacity),
            _ => panic!("Invalid private IPC I/O operation"),
        }
    }
}
/// # Safety
/// Output is an exclusive writable snapshot for this call; channels are each
/// admitted separately, preserving the original bounded aggregate semantics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ipc_summary(output: *mut Summary) -> i32 {
    if deny_irq() {
        return fail();
    }
    let mut active = 0;
    for i in 0..MAX_CHANNELS {
        if let Some(guard) = ChannelGuard::acquire(i)
            && unsafe { (*guard.channel).active }
        {
            active += 1;
        }
    }
    let result = Summary {
        max_channels: MAX_CHANNELS as u32,
        active_channels: active,
        max_message_size: MAX_MESSAGE_SIZE as u32,
        queue_depth: QUEUE_DEPTH as u32,
        sends: SENDS.load(Ordering::Relaxed),
        receives: RECEIVES.load(Ordering::Relaxed),
        drops: DROPS.load(Ordering::Relaxed),
        errors: ERRORS.load(Ordering::Relaxed),
        echo_tests: ECHO_TESTS.load(Ordering::Relaxed),
    };
    unsafe { output.write(result) };
    1
}
/// # Safety
/// Output is exclusive caller storage and channel names retain the original
/// static registered lifetime. The channel mutex admits this bounded copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ipc_channel(id: u32, output: *mut ChannelInfo) -> i32 {
    unsafe {
        if deny_irq() {
            return fail();
        }
        let Some(i) = index(id) else {
            return fail();
        };
        let Some(guard) = ChannelGuard::acquire(i) else {
            return fail();
        };
        let ch = guard.channel;
        let mut result = ChannelInfo {
            id,
            active: u32::from((*ch).active),
            queued: select(ch, 4, 0).selection.index as u32,
            queue_depth: QUEUE_DEPTH as u32,
            max_message_size: MAX_MESSAGE_SIZE as u32,
            has_handler: u32::from((*ch).handler.is_some()),
            opens: (*ch).opens,
            closes: (*ch).closes,
            sends: (*ch).sends,
            receives: (*ch).receives,
            drops: (*ch).drops,
            ..ChannelInfo::EMPTY
        };
        let name = if (*ch).service_name.len != 0 {
            (*ch).service_name
        } else if id == 1 {
            Bytes {
                data: b"echo".as_ptr(),
                len: 4,
            }
        } else {
            Bytes {
                data: ptr::null(),
                len: 0,
            }
        };
        let len = name.len.min(result.name.len() - 1);
        if len != 0 {
            ptr::copy_nonoverlapping(name.data, result.name.as_mut_ptr(), len);
        }
        output.write(result);
        i32::from((*ch).active)
    }
}
fn performance_for(id: Option<u32>) -> Option<PerformanceSummary> {
    let aggregate = id.is_none();
    let mut result = PerformanceSummary {
        worker_started: u32::from(STARTED.load(Ordering::Acquire)),
        worker_task_id: WORKER_TASK_ID.load(Ordering::Acquire),
        queue_limit: if aggregate {
            (MAX_CHANNELS * QUEUE_DEPTH) as u32
        } else {
            QUEUE_DEPTH as u32
        },
        worker_idle_waits: IDLE_WAITS.load(Ordering::Relaxed),
        worker_wakes: WAKES.load(Ordering::Relaxed),
        irq_denied: if aggregate {
            IRQ_DENIED.load(Ordering::Relaxed)
        } else {
            0
        },
        ..PerformanceSummary::EMPTY
    };
    let first = if let Some(id) = id { index(id)? } else { 0 };
    let end = if aggregate { MAX_CHANNELS } else { first + 1 };
    for i in first..end {
        let Some(guard) = ChannelGuard::acquire(i) else {
            if aggregate {
                continue;
            } else {
                return None;
            }
        };
        unsafe {
            let ch = guard.channel;
            if (*ch).active {
                result.active_channels += 1;
            }
            result.queue_used = result
                .queue_used
                .saturating_add(select(ch, 5, 0).selection.index as u32);
            result.queue_ready = result
                .queue_ready
                .saturating_add(select(ch, 4, 0).selection.index as u32);
            result.queue_running = result
                .queue_running
                .saturating_add(select(ch, 6, 0).selection.index as u32);
            result.add_lifetime((*ch).lifetime);
        }
    }
    Some(result)
}
/// # Safety
/// Output is an exclusive snapshot. `check_irq` distinguishes the two existing
/// callers: passive raw performanceSummary and IRQ-rejecting summaryFor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ipc_performance(
    id: u32,
    check_irq: bool,
    output: *mut PerformanceSummary,
) -> i32 {
    if check_irq && deny_irq() {
        return fail();
    }
    let Some(result) = performance_for((id != 0).then_some(id)) else {
        return fail();
    };
    unsafe { output.write(result) };
    1
}

// Typed native callers retain the same channel control admission and policy.
pub(crate) fn close_channel(id: u32) -> i32 {
    control(id, 0)
}
pub(crate) fn poll_channel(id: u32) -> i32 {
    control(id, 1)
}
