//! Sole native RX queue/task runtime. Preserve original queue/task/poll
//! policy; every borrowed frame given to another owner is a bounded owned copy.
use super::{adapters, backend_admission::CallbackGuard, backend_contract, rx_handoff, timing};
use crate::{
    arch::runtime::RuntimeGuard,
    boot::foundation,
    build_config::config,
    driver::irq_router,
    memory::owner::Guard,
    platform::monotonic,
    sched::{
        scheduler, sync_locks, sync_objects,
        sync_types::{Event, Text, UnwindGuard, WaitQueue},
        task::{self, task_resources, task_types},
    },
};
use core::{cell::UnsafeCell, ptr};
const MAX_ADAPTERS: usize = 8;
const BATCH_BUDGET: usize = 32;
const ALL_ADAPTERS: u16 = (1 << MAX_ADAPTERS) - 1;
const NESTED_LIMIT: u8 = 4;
const FALLBACK_TICKS: u64 = timing::ms_to_ticks(10);
const LOG_INTERVAL: u64 = 2000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RxTaskSummary {
    pub started: bool,
    pub task_id: u32,
    pub task_generation: u64,
    pub polls: u64,
    pub iterations: u64,
    pub guard_skips: u64,
    pub nested_polls: u64,
    pub queue_capacity: u32,
    pub queue_ready: u32,
    pub queue_occupied: u32,
    pub queue_high_water: u32,
    pub accepted: u64,
    pub processed: u64,
    pub cancelled: u64,
    pub queue_busy: u64,
    pub protocol_success: u64,
    pub protocol_failures: u64,
    pub schedules: u64,
    pub schedules_from_irq: u64,
    pub schedule_coalesced: u64,
    pub event_wakeups: u64,
    pub fallback_timeouts: u64,
    pub batches: u64,
    pub batch_max: u32,
    pub budget_exhaustions: u64,
    pub handoff_latency_samples: u64,
    pub handoff_latency_last_ns: u64,
    pub handoff_latency_max_ns: u64,
    pub schedule_latency_samples: u64,
    pub schedule_latency_last_ns: u64,
    pub schedule_latency_max_ns: u64,
    pub irq_frame_rejects: u64,
    pub release_failures: u64,
    pub metadata_submissions: u64,
    pub metadata_selected: u64,
    pub metadata_fallbacks: u64,
    pub l4_offload_used: u64,
    pub l4_software_checks: u64,
    pub ownership_balanced: bool,
}
struct State {
    rx_handoff_queue: rx_handoff::Queue,
    rx_schedule_mask: u16,
    rx_schedule_stamps: [monotonic::Stamp; MAX_ADAPTERS],
    rx_schedule_calls: u64,
    rx_schedule_from_irq: u64,
    rx_schedule_coalesced: u64,
    rx_schedule_invalid: u64,
    rx_event_wakeups: u64,
    rx_event_timeouts: u64,
    rx_protocol_success: u64,
    rx_protocol_failures: u64,
    rx_processing_depth: u8,
    rx_release_failures: u64,
    rx_batches: u64,
    rx_batch_max: u32,
    rx_budget_exhaustions: u64,
    rx_handoff_latency_samples: u64,
    rx_handoff_latency_total_ns: u64,
    rx_handoff_latency_last_ns: u64,
    rx_handoff_latency_max_ns: u64,
    rx_schedule_latency_samples: u64,
    rx_schedule_latency_total_ns: u64,
    rx_schedule_latency_last_ns: u64,
    rx_schedule_latency_max_ns: u64,
    rx_latency_unavailable: u64,
    rx_irq_frame_rejects: u64,
    rx_metadata_submissions: u64,
    rx_metadata_selected: u64,
    rx_metadata_fallbacks: u64,
    rx_l4_offload_used: u64,
    rx_l4_software_checks: u64,
    adapter_poll_rounds: u64,
    tcp_service_poll_requests: u64,
    tcp_service_poll_skips: u64,
    net_loss_counter: u64,
    net_loss_drops: u64,
    net_poll_skips: u64,
    net_poll_nested: u64,
    rx_task_started: bool,
    rx_task_id: u32,
    rx_task_generation: u64,
    rx_task_polls: u64,
    rx_task_iterations: u64,
}
impl State {
    const EMPTY: Self = Self {
        rx_handoff_queue: rx_handoff::Queue::EMPTY,
        rx_schedule_mask: 0,
        rx_schedule_stamps: [const {
            monotonic::Stamp {
                raw: 0,
                generation: 0,
                kind: 0,
            }
        }; MAX_ADAPTERS],
        rx_schedule_calls: 0,
        rx_schedule_from_irq: 0,
        rx_schedule_coalesced: 0,
        rx_schedule_invalid: 0,
        rx_event_wakeups: 0,
        rx_event_timeouts: 0,
        rx_protocol_success: 0,
        rx_protocol_failures: 0,
        rx_processing_depth: 0,
        rx_release_failures: 0,
        rx_batches: 0,
        rx_batch_max: 0,
        rx_budget_exhaustions: 0,
        rx_handoff_latency_samples: 0,
        rx_handoff_latency_total_ns: 0,
        rx_handoff_latency_last_ns: 0,
        rx_handoff_latency_max_ns: 0,
        rx_schedule_latency_samples: 0,
        rx_schedule_latency_total_ns: 0,
        rx_schedule_latency_last_ns: 0,
        rx_schedule_latency_max_ns: 0,
        rx_latency_unavailable: 0,
        rx_irq_frame_rejects: 0,
        rx_metadata_submissions: 0,
        rx_metadata_selected: 0,
        rx_metadata_fallbacks: 0,
        rx_l4_offload_used: 0,
        rx_l4_software_checks: 0,
        adapter_poll_rounds: 0,
        tcp_service_poll_requests: 0,
        tcp_service_poll_skips: 0,
        net_loss_counter: 0,
        net_loss_drops: 0,
        net_poll_skips: 0,
        net_poll_nested: 0,
        rx_task_started: false,
        rx_task_id: 0,
        rx_task_generation: 0,
        rx_task_polls: 0,
        rx_task_iterations: 0,
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: queue/metrics use short RX metadata admission; all callbacks,
// waits and output writes follow admission release. The exact claimed slot
// remains retained until protocol completion even though its bytes are copied.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn state() -> *mut State {
    STATE.0.get()
}
struct EventStorage(UnsafeCell<Event>);
// SAFETY: native Event owns its runtime admission; bootstrap alone resets it.
unsafe impl Sync for EventStorage {}
static EVENT: EventStorage = EventStorage(UnsafeCell::new(Event::AUTO));
struct PollOwner(UnsafeCell<UnwindGuard>);
// SAFETY: exact native task guard operations own its fields and generation.
unsafe impl Sync for PollOwner {}
const POLL_INITIAL: UnwindGuard = UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"net-poll"),
    queue: WaitQueue::EMPTY,
};
static POLL: PollOwner = PollOwner(UnsafeCell::new(POLL_INITIAL));
struct Critical {
    owner: Option<Guard>,
}
impl Critical {
    fn acquire() -> Self {
        let owner = Guard::acquire(2);
        scheduler::preempt_disable();
        Self { owner: Some(owner) }
    }
}
impl Drop for Critical {
    fn drop(&mut self) {
        scheduler::preempt_enable();
        drop(self.owner.take());
    }
}
struct PollGuard;
impl PollGuard {
    fn acquire() -> Option<Self> {
        crate::memory::owner::prepare_stack();
        if unsafe { sync_locks::guard_try(POLL.0.get()) } {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for PollGuard {
    fn drop(&mut self) {
        assert!(
            unsafe { sync_locks::guard_leave(POLL.0.get()) },
            "Exact network poll guard release"
        );
    }
}
#[repr(C)]
pub struct RxPeers {
    pub process: unsafe extern "C" fn(usize, *const u8, usize, *const rx_handoff::Metadata) -> bool,
    pub maintenance: unsafe extern "C" fn(u64),
    pub log: unsafe extern "C" fn(),
}
struct PeerStorage(UnsafeCell<*const RxPeers>);
// SAFETY: bootstrap sets one immutable resident callback table before starting
// the worker. Runtime calls retain that same kernel-lifetime table.
unsafe impl Sync for PeerStorage {}
static PEERS: PeerStorage = PeerStorage(UnsafeCell::new(ptr::null()));
fn peers() -> &'static RxPeers {
    unsafe {
        (*PEERS.0.get())
            .as_ref()
            .expect("Published network RX peers")
    }
}
/// # Safety
/// Original singleton boot reset; no active queue/Event/poll borrowers exist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_reset() {
    let _critical = Critical::acquire();
    unsafe {
        (*state()).rx_handoff_queue.reset();
        (*state()).rx_schedule_mask = 0;
        (*state()).rx_schedule_stamps = [const {
            monotonic::Stamp {
                raw: 0,
                generation: 0,
                kind: 0,
            }
        }; MAX_ADAPTERS];
        (*state()).rx_schedule_calls = 0;
        (*state()).rx_schedule_from_irq = 0;
        (*state()).rx_schedule_coalesced = 0;
        (*state()).rx_schedule_invalid = 0;
        (*state()).rx_event_wakeups = 0;
        (*state()).rx_event_timeouts = 0;
        (*state()).rx_protocol_success = 0;
        (*state()).rx_protocol_failures = 0;
        (*state()).rx_processing_depth = 0;
        (*state()).rx_release_failures = 0;
        (*state()).rx_batches = 0;
        (*state()).rx_batch_max = 0;
        (*state()).rx_budget_exhaustions = 0;
        (*state()).rx_handoff_latency_samples = 0;
        (*state()).rx_handoff_latency_total_ns = 0;
        (*state()).rx_handoff_latency_last_ns = 0;
        (*state()).rx_handoff_latency_max_ns = 0;
        (*state()).rx_schedule_latency_samples = 0;
        (*state()).rx_schedule_latency_total_ns = 0;
        (*state()).rx_schedule_latency_last_ns = 0;
        (*state()).rx_schedule_latency_max_ns = 0;
        (*state()).rx_latency_unavailable = 0;
        (*state()).rx_irq_frame_rejects = 0;
        (*state()).rx_metadata_submissions = 0;
        (*state()).rx_metadata_selected = 0;
        (*state()).rx_metadata_fallbacks = 0;
        (*state()).rx_l4_offload_used = 0;
        (*state()).rx_l4_software_checks = 0;
        (*state()).adapter_poll_rounds = 0;
        (*state()).tcp_service_poll_requests = 0;
        (*state()).tcp_service_poll_skips = 0;
        (*state()).net_poll_skips = 0;
        (*state()).net_poll_nested = 0;

        EVENT.0.get().write(Event::AUTO);
        POLL.0.get().write(POLL_INITIAL);
    }
}

fn signal() {
    unsafe {
        sync_objects::event_signal(EVENT.0.get());
    }
}
fn now_ns() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
fn in_irq() -> bool {
    irq_router::in_dispatch()
}
fn started() -> bool {
    let _critical = Critical::acquire();
    unsafe { (*state()).rx_task_started }
}
fn is_current() -> bool {
    let current = scheduler::current();
    if current.is_null() {
        return false;
    }
    let identity = unsafe { task::immutable_identity(current) };
    let _critical = Critical::acquire();
    unsafe { identity == ((*state()).rx_task_id, (*state()).rx_task_generation) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_schedule(index: usize) -> bool {
    let Some(_callback) = CallbackGuard::acquire() else {
        return false;
    };
    if index >= adapters::r4os_rust_net_adapter_count() {
        let _critical = Critical::acquire();
        unsafe {
            (*state()).rx_schedule_invalid = (*state()).rx_schedule_invalid.wrapping_add(1);
        }
        return false;
    }
    let stamp = monotonic::capture();
    let bit = 1u16 << index;
    let should_signal = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            (*s).rx_schedule_calls = (*s).rx_schedule_calls.wrapping_add(1);
            if in_irq() {
                (*s).rx_schedule_from_irq = (*s).rx_schedule_from_irq.wrapping_add(1);
            }
            if (*s).rx_schedule_mask & bit != 0 {
                (*s).rx_schedule_coalesced = (*s).rx_schedule_coalesced.wrapping_add(1);
                false
            } else {
                (*s).rx_schedule_mask |= bit;
                (*s).rx_schedule_stamps[index] = stamp;
                true
            }
        }
    };
    if should_signal {
        signal();
    }
    true
}
/// # Safety
/// Immutable resident driver-owned frame through the bounded copy. The IRQ
/// path rejects the request without reading the bytes, retaining driver retry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_receive(
    index: usize,
    queue: u16,
    packet_flags: u64,
    data: *const u8,
    len: usize,
    metadata_api: bool,
) -> u8 {
    if in_irq() {
        let _critical = Critical::acquire();
        unsafe {
            (*state()).rx_irq_frame_rejects = (*state()).rx_irq_frame_rejects.wrapping_add(1);
        }
        return 6;
    }
    if len == 0 || len > 1536 {
        return 3;
    }
    let Some(_callback) = CallbackGuard::acquire() else {
        return 4;
    };
    let Some(adapter) = adapters::snapshot(index) else {
        return 2;
    };
    if queue >= adapter.backend.rx_queue_count {
        return 3;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    let decision = if metadata_api {
        backend_contract::select_rx_metadata(adapter.backend.accepted, packet_flags, bytes)
    } else {
        backend_contract::RxMetadataDecision::Software
    };
    let metadata = rx_handoff::Metadata {
        l4_checksum_valid: matches!(
            decision,
            backend_contract::RxMetadataDecision::L4ChecksumValid
        ),
        software_fallback: matches!(
            decision,
            backend_contract::RxMetadataDecision::SoftwareFallback
        ),
    };
    let stamp = now_ns();
    let (result, should_signal) = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            let was_empty = (*s).rx_handoff_queue.queued_count() == 0;
            let result = (*s).rx_handoff_queue.enqueue(index, bytes, stamp, metadata);
            let should_signal = match result {
                rx_handoff::EnqueueResult::Accepted => {
                    if metadata_api && packet_flags != 0 {
                        (*s).rx_metadata_submissions = (*s).rx_metadata_submissions.wrapping_add(1);
                    }
                    if metadata.l4_checksum_valid {
                        (*s).rx_metadata_selected = (*s).rx_metadata_selected.wrapping_add(1);
                    }
                    if metadata.software_fallback {
                        (*s).rx_metadata_fallbacks = (*s).rx_metadata_fallbacks.wrapping_add(1);
                    }
                    was_empty
                }
                rx_handoff::EnqueueResult::Busy => true,
                rx_handoff::EnqueueResult::InvalidFrame => false,
            };
            (result, should_signal)
        }
    };
    if should_signal {
        signal();
    }
    match result {
        rx_handoff::EnqueueResult::Accepted => {
            if metadata.software_fallback {
                1
            } else {
                0
            }
        }
        rx_handoff::EnqueueResult::InvalidFrame => 3,
        rx_handoff::EnqueueResult::Busy => 5,
    }
}
unsafe fn process_one(peers: &RxPeers) -> bool {
    let Some(_callback) = CallbackGuard::acquire() else {
        return false;
    };
    // Keep the exact queue claim occupied through protocol completion. Copy its
    // bytes while admitted so another producer never mutates a whole Queue while
    // protocol code holds a Rust reference into one of that Queue's payloads.
    let mut frame = [0u8; 1536];
    let claim = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            let Some(claim) = (*s).rx_handoff_queue.claim() else {
                return false;
            };
            let Some(bytes) = (*s).rx_handoff_queue.frame(claim) else {
                (*s).rx_release_failures = (*s).rx_release_failures.wrapping_add(1);
                (*s).rx_handoff_queue.release(claim);
                return false;
            };
            frame[..bytes.len()].copy_from_slice(bytes);
            claim
        }
    };
    let process_start = now_ns();
    {
        let _critical = Critical::acquire();
        unsafe {
            (*state()).rx_processing_depth = (*state()).rx_processing_depth.wrapping_add(1);
        }
    }
    let protocol_ok = unsafe {
        (peers.process)(
            claim.adapter_index,
            frame.as_ptr(),
            usize::from(claim.len),
            &claim.metadata,
        )
    };
    {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            (*s).rx_processing_depth -= 1;
            if protocol_ok {
                (*s).rx_protocol_success = (*s).rx_protocol_success.wrapping_add(1);
            } else {
                (*s).rx_protocol_failures = (*s).rx_protocol_failures.wrapping_add(1);
            }
            if !(*s).rx_handoff_queue.release(claim) {
                (*s).rx_release_failures = (*s).rx_release_failures.wrapping_add(1);
            }
            if claim.enqueued_ns != 0 && process_start >= claim.enqueued_ns {
                let latency = process_start - claim.enqueued_ns;
                (*s).rx_handoff_latency_samples = (*s).rx_handoff_latency_samples.wrapping_add(1);
                (*s).rx_handoff_latency_total_ns =
                    (*s).rx_handoff_latency_total_ns.wrapping_add(latency);
                (*s).rx_handoff_latency_last_ns = latency;
                (*s).rx_handoff_latency_max_ns = (*s).rx_handoff_latency_max_ns.max(latency);
            } else {
                (*s).rx_latency_unavailable = (*s).rx_latency_unavailable.wrapping_add(1);
            }
        }
    }
    true
}
unsafe fn process_batch(budget: usize, peers: &RxPeers, cooperative: bool) -> usize {
    if budget == 0 {
        return 0;
    }
    let mut processed = 0;
    while processed < budget && unsafe { process_one(peers) } {
        processed += 1;
        // Only the task-owned outer batch opts in. A frame's exact claim and
        // protocol callback have returned; no RX Critical or PollGuard spans
        // this point. Nested/polled callers retain their original no-switch path.
        if cooperative && processed.is_multiple_of(8) && processed < budget {
            scheduler::safe_reschedule();
        }
    }
    if processed == 0 {
        return 0;
    }
    let _critical = Critical::acquire();
    unsafe {
        let s = state();
        (*s).rx_batches = (*s).rx_batches.wrapping_add(1);
        (*s).rx_batch_max = (*s).rx_batch_max.max(processed as u32);
        if processed == budget && (*s).rx_handoff_queue.queued_count() != 0 {
            (*s).rx_budget_exhaustions = (*s).rx_budget_exhaustions.wrapping_add(1);
        }
    }
    processed
}
fn has_ready() -> bool {
    let _critical = Critical::acquire();
    unsafe { (*state()).rx_handoff_queue.queued_count() != 0 }
}
fn take_scheduled() -> u16 {
    let mut stamps = [monotonic::Stamp {
        raw: 0,
        kind: 0,
        generation: 0,
    }; MAX_ADAPTERS];
    let mask = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            let mask = (*s).rx_schedule_mask;
            (*s).rx_schedule_mask = 0;
            for (index, stamp) in stamps.iter_mut().enumerate() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                *stamp = (*s).rx_schedule_stamps[index];
                (*s).rx_schedule_stamps[index] = monotonic::Stamp {
                    raw: 0,
                    kind: 0,
                    generation: 0,
                };
            }
            mask
        }
    };
    let (mut samples, mut total, mut last, mut max, mut unavailable) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    for (index, stamp) in stamps.into_iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        let Some(latency) = monotonic::elapsed(stamp, monotonic::capture()) else {
            unavailable += 1;
            continue;
        };
        samples += 1;
        total = total.wrapping_add(latency);
        last = latency;
        max = max.max(latency);
    }
    if samples != 0 || unavailable != 0 {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            (*s).rx_schedule_latency_samples =
                (*s).rx_schedule_latency_samples.wrapping_add(samples);
            (*s).rx_schedule_latency_total_ns =
                (*s).rx_schedule_latency_total_ns.wrapping_add(total);
            (*s).rx_schedule_latency_last_ns = last;
            (*s).rx_schedule_latency_max_ns = (*s).rx_schedule_latency_max_ns.max(max);
            (*s).rx_latency_unavailable = (*s).rx_latency_unavailable.wrapping_add(unavailable);
        }
    }
    mask
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_cancel() -> usize {
    let _critical = Critical::acquire();
    unsafe {
        let s = state();
        let cancelled = (*s).rx_handoff_queue.cancel_all();
        (*s).rx_schedule_mask = 0;
        (*s).rx_schedule_stamps = [monotonic::Stamp {
            raw: 0,
            kind: 0,
            generation: 0,
        }; MAX_ADAPTERS];
        cancelled
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_l4(protocol: u8, valid: bool) {
    let _critical = Critical::acquire();
    unsafe {
        let s = state();
        if valid {
            (*s).rx_l4_offload_used = (*s).rx_l4_offload_used.wrapping_add(1);
        } else if matches!(protocol, 6 | 17) {
            (*s).rx_l4_software_checks = (*s).rx_l4_software_checks.wrapping_add(1);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_loss_drop() -> bool {
    if !config::ENABLE_NET_LOSS_TEST {
        return false;
    }
    let _critical = Critical::acquire();
    unsafe {
        let s = state();
        (*s).net_loss_counter = (*s).net_loss_counter.wrapping_add(1);
        if (*s).net_loss_counter.is_multiple_of(64) {
            (*s).net_loss_drops = (*s).net_loss_drops.wrapping_add(1);
            true
        } else {
            false
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_loss_drops() -> u64 {
    let _critical = Critical::acquire();
    unsafe { (*state()).net_loss_drops }
}

unsafe fn poll_inner(mask: u16) {
    let mut index = 0;
    while index < adapters::r4os_rust_net_adapter_count() {
        if mask & (1 << index) != 0
            && let Some(adapter) = adapters::snapshot(index)
            && let Some(poll) = adapter.ops.poll
        {
            {
                let _critical = Critical::acquire();
                unsafe {
                    (*state()).adapter_poll_rounds = (*state()).adapter_poll_rounds.wrapping_add(1);
                }
            }
            unsafe {
                poll(adapter.ops.poll_context, index);
            }
        }
        index += 1;
    }
}
unsafe fn poll(_rounds: usize, mask: u16, peers: &RxPeers) {
    let Some(_callback) = CallbackGuard::acquire() else {
        return;
    };
    if scheduler::current().is_null() {
        unsafe {
            poll_inner(mask);
        }
        return;
    }
    let nested = unsafe { sync_locks::guard_owned(POLL.0.get()) };
    let Some(_poll) = PollGuard::acquire() else {
        let _critical = Critical::acquire();
        unsafe {
            (*state()).net_poll_skips = (*state()).net_poll_skips.wrapping_add(1);
        }
        return;
    };
    if nested {
        let _critical = Critical::acquire();
        unsafe {
            (*state()).net_poll_nested = (*state()).net_poll_nested.wrapping_add(1);
        }
    }
    unsafe {
        poll_inner(mask);
    }
    let depth = {
        let _critical = Critical::acquire();
        unsafe { (*state()).rx_processing_depth }
    };
    if depth != 0 && depth < NESTED_LIMIT && is_current() {
        unsafe {
            process_batch(1, peers, false);
        }
    }
}
/// # Safety
/// Immutable resident owner callbacks retained through all nested frame work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_poll(
    rounds: usize,
    mask: u16,
    peers: *const RxPeers,
) {
    unsafe {
        poll(rounds, mask, &*peers);
    }
}
/// # Safety
/// Immutable resident owner callbacks retained through all nested frame work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_poll_without_task(
    rounds: usize,
    peers: *const RxPeers,
) {
    if !started() {
        unsafe {
            poll(rounds, ALL_ADAPTERS, &*peers);
        }
    }
}
/// # Safety
/// Immutable resident owner callbacks retained through all nested frame work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_service_poll(
    rounds: usize,
    peers: *const RxPeers,
) {
    let skip = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            (*s).tcp_service_poll_requests = (*s).tcp_service_poll_requests.wrapping_add(1);
            if (*s).rx_task_started {
                (*s).tcp_service_poll_skips = (*s).tcp_service_poll_skips.wrapping_add(1);
                true
            } else {
                false
            }
        }
    };
    if !skip {
        unsafe {
            poll(rounds, ALL_ADAPTERS, &*peers);
        }
    }
}
/// # Safety
/// Immutable resident owner callbacks retained through claimed frame processing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_process_one(peers: *const RxPeers) -> bool {
    unsafe { process_one(&*peers) }
}
/// # Safety
/// Immutable resident owner callbacks retained through every bounded activation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_process_batch(
    budget: usize,
    peers: *const RxPeers,
) -> usize {
    unsafe { process_batch(budget, &*peers, false) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_has_ready() -> bool {
    has_ready()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_take_scheduled() -> u16 {
    take_scheduled()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_rx_runtime_is_current() -> bool {
    is_current()
}
unsafe extern "C" fn task_main() {
    loop {
        let event_wakeup = unsafe { sync_objects::event_wait(EVENT.0.get(), FALLBACK_TICKS) };
        {
            let _critical = Critical::acquire();
            unsafe {
                let s = state();
                if event_wakeup {
                    (*s).rx_event_wakeups = (*s).rx_event_wakeups.wrapping_add(1);
                } else {
                    (*s).rx_event_timeouts = (*s).rx_event_timeouts.wrapping_add(1);
                }
            }
        }
        let scheduled = take_scheduled();
        let peers = peers();
        let mut processed = unsafe { process_batch(BATCH_BUDGET, peers, true) };
        let first = {
            let _critical = Critical::acquire();
            unsafe { (*state()).rx_task_iterations == 0 }
        };
        if adapters::r4os_rust_net_adapter_count() > 0 && (scheduled != 0 || !event_wakeup || first)
        {
            {
                let _critical = Critical::acquire();
                unsafe {
                    (*state()).rx_task_polls = (*state()).rx_task_polls.wrapping_add(1);
                }
            }
            unsafe {
                poll(
                    1,
                    if scheduled != 0 {
                        scheduled
                    } else {
                        ALL_ADAPTERS
                    },
                    peers,
                );
                if processed < BATCH_BUDGET {
                    processed += process_batch(BATCH_BUDGET - processed, peers, true);
                }
            }
        }
        if has_ready() {
            signal();
        }
        let iterations = {
            let _critical = Critical::acquire();
            unsafe {
                let s = state();
                (*s).rx_task_iterations = (*s).rx_task_iterations.wrapping_add(1);
                (*s).rx_task_iterations
            }
        };
        unsafe {
            (peers.maintenance)(timing::now_ticks());
            if iterations.is_multiple_of(LOG_INTERVAL) {
                (peers.log)();
            }
        }
        // Maintenance/log callbacks and all queue/adapter owners have returned.
        // A signaled Event can return immediately under sustained traffic, so
        // waiting at the next loop alone does not guarantee competing progress.
        scheduler::safe_reschedule();
        let _ = processed;
    }
}
/// # Safety
/// Singleton boot start with one immutable kernel-lifetime callback table. Task
/// construction retains the original FPU, BSP and short-completion role policy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_start(input: *const RxPeers) -> bool {
    if started() {
        return true;
    }
    {
        let _guard = RuntimeGuard::acquire();
        unsafe {
            PEERS.0.get().write(input);
        }
    }
    let mut failure = task_types::CreateFailure::None;
    let worker = unsafe {
        task_resources::create(
            task_resources::CreateArgs {
                name: task_types::Bytes {
                    pointer: b"net-rx".as_ptr(),
                    len: 6,
                },
                state: task_types::State::Ready,
                needs_fpu: true,
                entry: Some(task_main),
                role: task_types::Role::ShortCompletion,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: task_types::ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            &mut failure,
        )
    };
    if worker.is_null() {
        foundation::puts(b"NET rx-task create failed\r\n");
        return false;
    }
    let (id, generation) = unsafe { task::immutable_identity(worker.cast()) };
    {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            (*s).rx_task_started = true;
            (*s).rx_task_id = id;
            (*s).rx_task_generation = generation;
        }
    }
    foundation::puts(b"NET rx-task started id=");
    foundation::dec(u64::from(id));
    foundation::puts(b"\r\nNETRX handoff queue=");
    foundation::dec(rx_handoff::CAPACITY as u64);
    foundation::puts(b" batch=");
    foundation::dec(BATCH_BUDGET as u64);
    foundation::puts(b" irq-inline=blocked wake=event fallback-ms=10\r\n");
    let mut index = 0;
    while index < adapters::r4os_rust_net_adapter_count() {
        r4os_rust_net_rx_runtime_schedule(index);
        index += 1;
    }
    if adapters::r4os_rust_net_adapter_count() == 0 {
        signal();
    }
    true
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RxPollStatus {
    pub adapter_poll_rounds: u64,
    pub service_poll_requests: u64,
    pub service_poll_skips: u64,
}
/// # Safety
/// Writable exact paired output; no caller output spans RX metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_poll_status(out: *mut RxPollStatus) {
    let value = {
        let _critical = Critical::acquire();
        unsafe {
            let s = state();
            RxPollStatus {
                adapter_poll_rounds: (*s).adapter_poll_rounds,
                service_poll_requests: (*s).tcp_service_poll_requests,
                service_poll_skips: (*s).tcp_service_poll_skips,
            }
        }
    };
    unsafe {
        out.write(value);
    }
}

/// # Safety
/// Writable exact paired snapshot; no output write spans RX metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_runtime_summary(out: *mut RxTaskSummary) {
    let value = {
        let _critical = Critical::acquire();
        unsafe {
            let s = &*state();
            let q = &s.rx_handoff_queue;
            let occupied = q.occupied_count();
            RxTaskSummary {
                started: s.rx_task_started,
                task_id: s.rx_task_id,
                task_generation: s.rx_task_generation,
                polls: s.rx_task_polls,
                iterations: s.rx_task_iterations,
                guard_skips: s.net_poll_skips,
                nested_polls: s.net_poll_nested,
                queue_capacity: rx_handoff::CAPACITY as u32,
                queue_ready: q.queued_count() as u32,
                queue_occupied: occupied as u32,
                queue_high_water: q.high_water as u32,
                accepted: q.accepted,
                processed: q.released,
                cancelled: q.cancelled,
                queue_busy: q.busy,
                protocol_success: s.rx_protocol_success,
                protocol_failures: s.rx_protocol_failures,
                schedules: s.rx_schedule_calls,
                schedules_from_irq: s.rx_schedule_from_irq,
                schedule_coalesced: s.rx_schedule_coalesced,
                event_wakeups: s.rx_event_wakeups,
                fallback_timeouts: s.rx_event_timeouts,
                batches: s.rx_batches,
                batch_max: s.rx_batch_max,
                budget_exhaustions: s.rx_budget_exhaustions,
                handoff_latency_samples: s.rx_handoff_latency_samples,
                handoff_latency_last_ns: s.rx_handoff_latency_last_ns,
                handoff_latency_max_ns: s.rx_handoff_latency_max_ns,
                schedule_latency_samples: s.rx_schedule_latency_samples,
                schedule_latency_last_ns: s.rx_schedule_latency_last_ns,
                schedule_latency_max_ns: s.rx_schedule_latency_max_ns,
                irq_frame_rejects: s.rx_irq_frame_rejects,
                release_failures: s.rx_release_failures,
                metadata_submissions: s.rx_metadata_submissions,
                metadata_selected: s.rx_metadata_selected,
                metadata_fallbacks: s.rx_metadata_fallbacks,
                l4_offload_used: s.rx_l4_offload_used,
                l4_software_checks: s.rx_l4_software_checks,
                ownership_balanced: q.accepted == q.released + q.cancelled + occupied as u64,
            }
        }
    };
    unsafe {
        out.write(value);
    }
}

const _: () = {
    assert!(core::mem::size_of::<RxTaskSummary>() == 288);
    assert!(core::mem::align_of::<RxTaskSummary>() == 8);
    assert!(core::mem::offset_of!(RxTaskSummary, started) == 0);
    assert!(core::mem::offset_of!(RxTaskSummary, task_id) == 4);
    assert!(core::mem::offset_of!(RxTaskSummary, task_generation) == 8);
    assert!(core::mem::offset_of!(RxTaskSummary, polls) == 16);
    assert!(core::mem::offset_of!(RxTaskSummary, iterations) == 24);
    assert!(core::mem::offset_of!(RxTaskSummary, guard_skips) == 32);
    assert!(core::mem::offset_of!(RxTaskSummary, nested_polls) == 40);
    assert!(core::mem::offset_of!(RxTaskSummary, queue_capacity) == 48);
    assert!(core::mem::offset_of!(RxTaskSummary, queue_ready) == 52);
    assert!(core::mem::offset_of!(RxTaskSummary, queue_occupied) == 56);
    assert!(core::mem::offset_of!(RxTaskSummary, queue_high_water) == 60);
    assert!(core::mem::offset_of!(RxTaskSummary, accepted) == 64);
    assert!(core::mem::offset_of!(RxTaskSummary, processed) == 72);
    assert!(core::mem::offset_of!(RxTaskSummary, cancelled) == 80);
    assert!(core::mem::offset_of!(RxTaskSummary, queue_busy) == 88);
    assert!(core::mem::offset_of!(RxTaskSummary, protocol_success) == 96);
    assert!(core::mem::offset_of!(RxTaskSummary, protocol_failures) == 104);
    assert!(core::mem::offset_of!(RxTaskSummary, schedules) == 112);
    assert!(core::mem::offset_of!(RxTaskSummary, schedules_from_irq) == 120);
    assert!(core::mem::offset_of!(RxTaskSummary, schedule_coalesced) == 128);
    assert!(core::mem::offset_of!(RxTaskSummary, event_wakeups) == 136);
    assert!(core::mem::offset_of!(RxTaskSummary, fallback_timeouts) == 144);
    assert!(core::mem::offset_of!(RxTaskSummary, batches) == 152);
    assert!(core::mem::offset_of!(RxTaskSummary, batch_max) == 160);
    assert!(core::mem::offset_of!(RxTaskSummary, budget_exhaustions) == 168);
    assert!(core::mem::offset_of!(RxTaskSummary, handoff_latency_samples) == 176);
    assert!(core::mem::offset_of!(RxTaskSummary, handoff_latency_last_ns) == 184);
    assert!(core::mem::offset_of!(RxTaskSummary, handoff_latency_max_ns) == 192);
    assert!(core::mem::offset_of!(RxTaskSummary, schedule_latency_samples) == 200);
    assert!(core::mem::offset_of!(RxTaskSummary, schedule_latency_last_ns) == 208);
    assert!(core::mem::offset_of!(RxTaskSummary, schedule_latency_max_ns) == 216);
    assert!(core::mem::offset_of!(RxTaskSummary, irq_frame_rejects) == 224);
    assert!(core::mem::offset_of!(RxTaskSummary, release_failures) == 232);
    assert!(core::mem::offset_of!(RxTaskSummary, metadata_submissions) == 240);
    assert!(core::mem::offset_of!(RxTaskSummary, metadata_selected) == 248);
    assert!(core::mem::offset_of!(RxTaskSummary, metadata_fallbacks) == 256);
    assert!(core::mem::offset_of!(RxTaskSummary, l4_offload_used) == 264);
    assert!(core::mem::offset_of!(RxTaskSummary, l4_software_checks) == 272);
    assert!(core::mem::offset_of!(RxTaskSummary, ownership_balanced) == 280);
};
const _: () = {
    assert!(core::mem::size_of::<RxPollStatus>() == 24);
    assert!(core::mem::align_of::<RxPollStatus>() == 8);
    assert!(core::mem::offset_of!(RxPollStatus, adapter_poll_rounds) == 0);
    assert!(core::mem::offset_of!(RxPollStatus, service_poll_requests) == 8);
    assert!(core::mem::offset_of!(RxPollStatus, service_poll_skips) == 16);
};
const _: () = {
    assert!(core::mem::size_of::<RxPeers>() == 24);
    assert!(core::mem::align_of::<RxPeers>() == 8);
    assert!(core::mem::offset_of!(RxPeers, process) == 0);
    assert!(core::mem::offset_of!(RxPeers, maintenance) == 8);
    assert!(core::mem::offset_of!(RxPeers, log) == 16);
};
