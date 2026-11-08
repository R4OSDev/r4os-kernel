//! Service registry and endpoint admission. Native records have stable
//! addresses; no registry/endpoint reference survives a wait or foreign call.
use super::registry_types::*;
use super::storage::{Catalog, EndpointNode, Endpoints, Operation, Prepared, ServiceNode};
use crate::{
    arch::runtime::{self, RuntimeGuard},
    build_config::config,
    contract as c,
    fs::vfs_types::Bytes,
    memory::heap,
    platform::monotonic,
    sched::{
        scheduler, sync_locks,
        sync_types::{Mutex, Text},
        task::task_registry,
    },
};
use core::{
    cell::UnsafeCell,
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
const ZERO_STAMP: monotonic::Stamp = monotonic::Stamp {
    raw: 0,
    generation: 0,
    kind: 0,
};
pub(super) struct Registry {
    pub entries: Catalog,
    pub endpoints: Endpoints,
    lock: Mutex,
    pub timing: [LockTiming; FAMILIES],
    pub generation: u64,
    pub inventory_revision: u64,
    pub next_start_generation: u64,
    pub enumeration: RegistryEnumerationPerformance,
    pub retired_lifetime: EndpointLifetimePerformance,
    pub retired_timing: [LockTiming; FAMILIES],
}
struct RegistryStorage(UnsafeCell<Registry>);
// SAFETY: the original service-registry mutex admits every bounded access;
// singleton boot/serialized owner fixtures alone reset the entire catalogue.
unsafe impl Sync for RegistryStorage {}
static REGISTRY: RegistryStorage = RegistryStorage(UnsafeCell::new(Registry {
    entries: Catalog::EMPTY,
    endpoints: Endpoints::EMPTY,
    lock: Mutex {
        rank: 120,
        name: Text::literal(b"service-registry"),
        ..Mutex::EMPTY
    },
    timing: [LockTiming::EMPTY; FAMILIES],
    generation: 1,
    inventory_revision: 1,
    next_start_generation: 0,
    enumeration: RegistryEnumerationPerformance::EMPTY,
    retired_lifetime: EndpointLifetimePerformance::EMPTY,
    retired_timing: [LockTiming::EMPTY; FAMILIES],
}));
pub(super) static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);
pub(super) fn raw() -> *mut Registry {
    REGISTRY.0.get()
}
pub(super) fn entry(slot: usize) -> *mut Entry {
    unsafe {
        let node = (*raw()).entries.get(slot);
        assert!(!node.is_null(), "Missing original service registration");
        ptr::addr_of_mut!((*node).entry)
    }
}
pub(super) fn entry_exists(slot: usize) -> bool {
    unsafe { !(*raw()).entries.get(slot).is_null() }
}
pub(super) fn endpoint(slot: usize) -> *mut Endpoint {
    unsafe {
        let node = (*raw())
            .endpoints
            .get(u32::try_from(slot).expect("Endpoint identity width"));
        assert!(!node.is_null(), "Missing original service endpoint");
        node.cast()
    }
}
unsafe fn atomic<'a>(pointer: *mut u64) -> &'a AtomicU64 {
    // Records are resident for boot or the serialized caller fixture. No
    // non-atomic access occurs concurrently with these short counter views.
    unsafe { AtomicU64::from_ptr(pointer) }
}
pub(super) unsafe fn timing_snapshot(p: *mut LockTiming) -> LockTiming {
    unsafe {
        LockTiming {
            acquisitions: atomic(ptr::addr_of_mut!((*p).acquisitions)).load(Ordering::Relaxed),
            contentions: atomic(ptr::addr_of_mut!((*p).contentions)).load(Ordering::Relaxed),
            timing_samples: atomic(ptr::addr_of_mut!((*p).timing_samples)).load(Ordering::Relaxed),
            wait_ns: atomic(ptr::addr_of_mut!((*p).wait_ns)).load(Ordering::Relaxed),
            wait_max_ns: atomic(ptr::addr_of_mut!((*p).wait_max_ns)).load(Ordering::Relaxed),
            hold_ns: atomic(ptr::addr_of_mut!((*p).hold_ns)).load(Ordering::Relaxed),
            hold_max_ns: atomic(ptr::addr_of_mut!((*p).hold_max_ns)).load(Ordering::Relaxed),
            timing_unavailable: atomic(ptr::addr_of_mut!((*p).timing_unavailable))
                .load(Ordering::Relaxed),
        }
    }
}
unsafe fn note_elapsed(
    t: *mut LockTiming,
    start: monotonic::Stamp,
    end: monotonic::Stamp,
    hold: bool,
) {
    unsafe {
        let Some(elapsed) = monotonic::elapsed(start, end) else {
            atomic(ptr::addr_of_mut!((*t).timing_unavailable)).fetch_add(1, Ordering::Relaxed);
            return;
        };
        let (total, max) = if hold {
            (
                ptr::addr_of_mut!((*t).hold_ns),
                ptr::addr_of_mut!((*t).hold_max_ns),
            )
        } else {
            (
                ptr::addr_of_mut!((*t).wait_ns),
                ptr::addr_of_mut!((*t).wait_max_ns),
            )
        };
        atomic(total).fetch_add(elapsed, Ordering::Relaxed);
        atomic(max).fetch_max(elapsed, Ordering::Relaxed);
    }
}
pub(super) struct TimedLock {
    pub mutex: *mut Mutex,
    timing: *mut LockTiming,
    started: monotonic::Stamp,
    timed: bool,
    locked: bool,
}
impl TimedLock {
    pub unsafe fn acquire(
        mutex: *mut Mutex,
        timing: *mut LockTiming,
        try_only: bool,
    ) -> Option<Self> {
        unsafe {
            if scheduler::current().is_null() {
                return Some(Self {
                    mutex,
                    timing: ptr::null_mut(),
                    started: ZERO_STAMP,
                    timed: false,
                    locked: false,
                });
            }
            let immediate = sync_locks::r4os_rust_sync_mutex(mutex, 0, 0);
            if try_only && !immediate {
                atomic(ptr::addr_of_mut!((*timing).contentions)).fetch_add(1, Ordering::Relaxed);
                return None;
            }
            let wait_started = if config::ENABLE_METRICS && !immediate {
                monotonic::capture()
            } else {
                ZERO_STAMP
            };
            if !immediate && !sync_locks::r4os_rust_sync_mutex(mutex, 1, FOREVER) {
                return None;
            }
            let old =
                atomic(ptr::addr_of_mut!((*timing).acquisitions)).fetch_add(1, Ordering::Relaxed);
            if !immediate {
                atomic(ptr::addr_of_mut!((*timing).contentions)).fetch_add(1, Ordering::Relaxed);
            }
            let sampled = config::ENABLE_METRICS && (!immediate || old % TIMING_STRIDE == 0);
            let acquired = if sampled {
                monotonic::capture()
            } else {
                ZERO_STAMP
            };
            if sampled {
                atomic(ptr::addr_of_mut!((*timing).timing_samples)).fetch_add(1, Ordering::Relaxed);
                if !immediate {
                    note_elapsed(timing, wait_started, acquired, false);
                }
            }
            Some(Self {
                mutex,
                timing,
                started: acquired,
                timed: sampled,
                locked: true,
            })
        }
    }
    pub fn release(&mut self) {
        if self.locked {
            unsafe {
                if self.timed {
                    note_elapsed(self.timing, self.started, monotonic::capture(), true);
                }
                sync_locks::r4os_rust_sync_mutex(self.mutex, 2, 0);
            }
        }
        self.locked = false;
    }
}
impl Drop for TimedLock {
    fn drop(&mut self) {
        self.release();
    }
}
pub(super) unsafe extern "C" fn release_for_wait(raw: *mut core::ffi::c_void) {
    // sync_wait invokes this under the same Runtime admission as predicate
    // evaluation/enrollment. No stack-owned endpoint pin survives a killable
    // queue wait. The real native queue retains its resident backing instead.
    unsafe {
        let lease = &mut *raw.cast::<EndpointLease>();
        lease.guard.release();
        lease.pin.release();
    }
}
pub(super) fn lock_registry(family: usize) -> Option<TimedLock> {
    assert!(family < 3);
    unsafe {
        TimedLock::acquire(
            ptr::addr_of_mut!((*raw()).lock),
            ptr::addr_of_mut!((*raw()).timing)
                .cast::<LockTiming>()
                .add(family),
            false,
        )
    }
}
/// Used only by the lower ranked ProgramRegistry publication transaction.
/// Admission is a bounded try: no wait or allocation under that owner.
pub(super) fn try_lock_registry(family: usize) -> Option<TimedLock> {
    assert!(family < 3);
    unsafe {
        TimedLock::acquire(
            ptr::addr_of_mut!((*raw()).lock),
            ptr::addr_of_mut!((*raw()).timing)
                .cast::<LockTiming>()
                .add(family),
            true,
        )
    }
}
pub(super) unsafe fn endpoint_lock(
    ep: *mut Endpoint,
    family: usize,
    try_only: bool,
) -> Option<TimedLock> {
    assert!((3..FAMILIES).contains(&family));
    unsafe {
        TimedLock::acquire(
            ptr::addr_of_mut!((*ep).lock),
            ptr::addr_of_mut!((*ep).lock_timing)
                .cast::<LockTiming>()
                .add(family),
            try_only,
        )
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Identity {
    pub endpoint: *mut Endpoint,
    pub handle: u32,
    pub generation: u64,
    pub service_slot: usize,
}
pub(super) unsafe fn identity(ep: *mut Endpoint) -> Identity {
    unsafe {
        Identity {
            endpoint: ep,
            handle: (*ep).handle,
            generation: (*ep).generation,
            service_slot: (*ep).service_slot,
        }
    }
}
pub(super) unsafe fn identity_matches(ep: *const Endpoint, id: Identity) -> bool {
    unsafe {
        (*ep).used
            && (*ep).handle == id.handle
            && (*ep).generation == id.generation
            && (*ep).service_slot == id.service_slot
    }
}
pub(super) struct EndpointLease {
    pub endpoint: *mut Endpoint,
    // Field order releases the mutex before the final pin/heap collection.
    pub guard: TimedLock,
    pin: EndpointPin,
}
struct EndpointPin {
    node: *mut EndpointNode,
    operation: Option<Operation>,
}
impl EndpointPin {
    fn release(&mut self) {
        if self.operation.is_some() {
            assert!(unsafe { (*self.node).pins.fetch_sub(1, Ordering::Release) } != 0);
            self.operation.take();
        }
    }
}
impl Drop for EndpointPin {
    fn drop(&mut self) {
        self.release();
        collect();
    }
}
pub(super) fn lock_identity(id: Identity, family: usize) -> Option<EndpointLease> {
    unsafe {
        let operation = Operation::enter()?;
        let pin = {
            let _registry = lock_registry(1)?;
            let node = (*raw()).endpoints.get(id.handle);
            if node.is_null() || node.cast::<Endpoint>() != id.endpoint {
                return None;
            }
            let ep = node.cast::<Endpoint>();
            if !identity_matches(ep, id) {
                return None;
            }
            if (*node)
                .pins
                .try_update(Ordering::Acquire, Ordering::Relaxed, |n| n.checked_add(1))
                .is_err()
            {
                return None;
            }
            EndpointPin {
                node,
                operation: Some(operation),
            }
        };
        let ep = pin.node.cast::<Endpoint>();
        let guard = endpoint_lock(ep, family, false)?;
        (*ep).lifetime.revalidations = (*ep).lifetime.revalidations.wrapping_add(1);
        if !identity_matches(ep, id) {
            (*ep).lifetime.stale_rejections = (*ep).lifetime.stale_rejections.wrapping_add(1);
            return None;
        }
        Some(EndpointLease {
            endpoint: ep,
            guard,
            pin,
        })
    }
}
pub(super) unsafe fn endpoint_for_slot(slot: usize) -> Option<usize> {
    unsafe {
        let node = (*raw()).endpoints.for_service(slot);
        (!node.is_null()).then(|| (*node).endpoint.handle as usize)
    }
}
pub(super) unsafe fn endpoint_for_handle(handle: u32) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    unsafe { (!(*raw()).endpoints.get(handle).is_null()).then_some(handle as usize) }
}
pub(super) fn lookup_identity(handle: u32) -> Option<Identity> {
    let _guard = lock_registry(1)?;
    unsafe { endpoint_for_handle(handle).map(|i| identity(endpoint(i))) }
}
pub(super) fn lock_handle(handle: u32, family: usize) -> Option<EndpointLease> {
    lock_identity(lookup_identity(handle)?, family)
}
pub(super) fn wait_lifecycle(id: Identity) -> bool {
    lock_identity(id, 3).is_some()
}
pub(super) unsafe fn bytes<'a>(value: Bytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(value.data, value.len) }
    }
}
pub(super) fn copy(source: &[u8], out: &mut [u8]) -> usize {
    out.fill(0);
    let len = source.len().min(out.len() - 1);
    out[..len].copy_from_slice(&source[..len]);
    len
}
pub(super) fn count(table: &Catalog) -> usize {
    table.count
}
pub(super) unsafe fn find_name(name: Bytes) -> Option<usize> {
    unsafe {
        let node = (*raw()).entries.find(bytes(name));
        (!node.is_null()).then(|| (*node).index.key as usize)
    }
}
pub(super) fn valid_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() < c::service_name_bytes
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-'))
}
fn value_safe(value: &[u8]) -> bool {
    !value.iter().any(|v| matches!(*v, b';' | b'\r' | b'\n'))
}
pub(super) fn make_entry(
    e: &mut Entry,
    name: &[u8],
    path: &[u8],
    args: &[u8],
    mode: u8,
    description: &[u8],
) -> i32 {
    if !valid_name(name)
        || path.len() < 5
        || !path[path.len() - 4..].eq_ignore_ascii_case(b".R4X")
        || path.len() >= 1024
        || args.len() >= c::service_args_bytes
        || description.len() >= c::service_description_bytes
        || !value_safe(path)
        || !value_safe(args)
        || !value_safe(description)
    {
        return -1;
    }
    *e = Entry {
        used: true,
        state: if mode == MODE_DISABLED {
            STATE_DISABLED
        } else {
            STATE_STOPPED
        },
        start_mode: mode,
        ..Entry::EMPTY
    };
    e.name_len = copy(name, &mut e.name);
    e.path_len = copy(path, &mut e.path);
    e.args_len = copy(args, &mut e.args);
    e.description_len = copy(description, &mut e.description);
    0
}
pub(super) unsafe fn set_state_in(slot: usize, state: u8, id: u32, code: i32, text: &[u8]) -> i32 {
    if state == STATE_EMPTY {
        return -1;
    }
    let e = unsafe { &mut *entry(slot) };
    e.state = state;
    e.start_generation = 0;
    e.instance_id = id;
    e.exit_code = code;
    e.last_error_len = copy(text, &mut e.last_error);
    0
}
pub(super) unsafe fn running_in(slot: usize, id: u32, ticks: u64) -> i32 {
    let e = unsafe { &mut *entry(slot) };
    if e.start_mode == MODE_DISABLED || id == 0 {
        return -1;
    }
    e.state = STATE_RUNNING;
    e.start_generation = 0;
    e.instance_id = id;
    e.exit_code = 0;
    e.start_tick = ticks;
    e.last_error_len = 0;
    0
}
pub(super) unsafe fn structure_changed() {
    unsafe {
        let r = raw();
        (*r).generation = (*r)
            .generation
            .checked_add(1)
            .expect("Service revision exhausted before mutation");
        (*r).inventory_revision = (*r)
            .inventory_revision
            .checked_add(1)
            .expect("Service inventory revision exhausted before mutation");
    }
}
pub(super) unsafe fn api_slot(index: u32) -> Option<usize> {
    unsafe {
        let node = (*raw()).entries.at(index);
        (!node.is_null()).then(|| (*node).index.key as usize)
    }
}
pub(super) unsafe fn target_slot(target: &ApiIndexTarget, require_instance: bool) -> Option<usize> {
    unsafe {
        if target.registry_generation != (*raw()).generation {
            return None;
        }
        let slot = if target.api_index == NAMED_TARGET_INDEX {
            let slot = target.service_slot;
            if !entry_exists(slot) || !(*entry(slot)).used {
                return None;
            }
            slot
        } else {
            api_slot(target.api_index)?
        };
        if slot != target.service_slot {
            return None;
        }
        let e = &*entry(slot);
        if e.name_len != target.name_len
            || !e.name[..e.name_len].eq_ignore_ascii_case(&target.name[..target.name_len])
        {
            return None;
        }
        if require_instance
            && (e.start_generation != target.start_generation
                || !same_binding(e.binding, target.binding))
        {
            return None;
        }
        if require_instance && e.instance_id != target.instance_id {
            // Another observer may already have published this same actual
            // completion. Its retained binding still identifies exactly the
            // old process; a subsequent start must replace that binding.
            if target.binding.id == 0
                || e.instance_id != 0
                || !matches!(e.state, STATE_STOPPED | STATE_DISABLED | STATE_FAILED)
            {
                return None;
            }
        }
        Some(slot)
    }
}
pub(super) fn same_binding(a: c::ProgramObservationHandle, b: c::ProgramObservationHandle) -> bool {
    a.id == b.id
        && a.process.instance_id == b.process.instance_id
        && a.process.generation == b.process.generation
        && a.process.reserved == b.process.reserved
}
/// Registry owner is held. Named queries do not scan the public program
/// inventory or increment the existing indexed-enumeration counters.
pub(super) unsafe fn target_for_slot(slot: usize, index: u32) -> ApiIndexTarget {
    unsafe {
        let e = &*entry(slot);
        ApiIndexTarget {
            registry_generation: (*raw()).generation,
            api_index: index,
            service_slot: slot,
            instance_id: e.instance_id,
            state: e.state,
            name: e.name,
            name_len: e.name_len,
            start_generation: e.start_generation,
            binding: e.binding,
        }
    }
}
pub(super) unsafe fn select_target(index: u32, query: bool) -> Option<ApiIndexTarget> {
    unsafe {
        let r = raw();
        let p = &mut (*r).enumeration;
        if query {
            p.index_queries = p.index_queries.wrapping_add(1);
        }
        let Some(slot) = api_slot(index) else {
            if query {
                p.index_end_markers = p.index_end_markers.wrapping_add(1);
            }
            return None;
        };
        if query {
            p.refresh_requests = p.refresh_requests.wrapping_add(1);
        }
        p.refresh_visits = p.refresh_visits.wrapping_add(1);
        Some(target_for_slot(slot, index))
    }
}

pub(super) fn prepare_entry(value: Entry) -> Option<Prepared<ServiceNode>> {
    let prepared = unsafe { Prepared::<ServiceNode>::new()? };
    unsafe { ptr::addr_of_mut!((*prepared.pointer()).entry).write(value) };
    Some(prepared)
}

pub(super) fn prepare_endpoint() -> Option<Prepared<EndpointNode>> {
    let prepared = unsafe { Prepared::<EndpointNode>::new()? };
    unsafe {
        let ep = prepared.pointer().cast::<Endpoint>();
        (*ep).lock = Mutex {
            rank: 130,
            name: Text::literal(b"service-endpoint"),
            ..Mutex::EMPTY
        };
        super::endpoint::reset_metadata(ep, false);
    }
    Some(prepared)
}

pub(super) unsafe fn retire_endpoint(ep: *mut Endpoint) {
    unsafe {
        crate::program::wait_sources::Channel::close(ptr::addr_of_mut!((*ep).waitset));
        (*raw()).endpoints.remove(ep)
    };
}

fn can_collect() -> bool {
    if runtime::r4os_rust_runtime_control(8, 0) != 0 {
        return false;
    }
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags));
    }
    if flags & 0x200 == 0 {
        return false;
    }
    let current = scheduler::current();
    if current.is_null() {
        return false;
    }
    unsafe { (*task_registry::core(current.cast())).held_lock_count == 0 }
}

unsafe fn endpoint_quiescent(node: *mut EndpointNode) -> bool {
    unsafe {
        if (*node).pins.load(Ordering::Acquire) != 0 {
            return false;
        }
        let ep = &(*node).endpoint;
        if ep.lock.depth != 0
            || ep.lock.handoff_pending
            || ep.lock.queue.core.count != 0
            || ep.requests_available.core.count != 0
            || ep.slots_available.queue.core.count != 0
        {
            return false;
        }
        ep.queue
            .iter()
            .all(|slot| slot.response_available.core.count == 0)
    }
}

pub(super) fn add_lifetime(
    out: &mut EndpointLifetimePerformance,
    value: EndpointLifetimePerformance,
) {
    out.payload_copy_bytes = out
        .payload_copy_bytes
        .wrapping_add(value.payload_copy_bytes);
    out.payload_clear_bytes = out
        .payload_clear_bytes
        .wrapping_add(value.payload_clear_bytes);
    out.slot_metadata_resets = out
        .slot_metadata_resets
        .wrapping_add(value.slot_metadata_resets);
    out.endpoint_metadata_resets = out
        .endpoint_metadata_resets
        .wrapping_add(value.endpoint_metadata_resets);
    out.endpoint_payload_reset_bytes = out
        .endpoint_payload_reset_bytes
        .wrapping_add(value.endpoint_payload_reset_bytes);
    out.queue_scan_passes = out.queue_scan_passes.wrapping_add(value.queue_scan_passes);
    out.queue_scan_slots = out.queue_scan_slots.wrapping_add(value.queue_scan_slots);
    out.revalidations = out.revalidations.wrapping_add(value.revalidations);
    out.stale_rejections = out.stale_rejections.wrapping_add(value.stale_rejections);
}

pub(super) fn add_timing(out: &mut LockTiming, value: LockTiming) {
    out.acquisitions = out.acquisitions.wrapping_add(value.acquisitions);
    out.contentions = out.contentions.wrapping_add(value.contentions);
    out.timing_samples = out.timing_samples.wrapping_add(value.timing_samples);
    out.wait_ns = out.wait_ns.wrapping_add(value.wait_ns);
    out.wait_max_ns = out.wait_max_ns.max(value.wait_max_ns);
    out.hold_ns = out.hold_ns.wrapping_add(value.hold_ns);
    out.hold_max_ns = out.hold_max_ns.max(value.hold_max_ns);
    out.timing_unavailable = out
        .timing_unavailable
        .wrapping_add(value.timing_unavailable);
}

/// A retired endpoint is absent from lookup. Zero short pins and empty actual
/// queues under Runtime prove that no admission/predicate/native waiter can
/// still touch its allocation. A woken caller resolves only its original ID.
pub(crate) fn collect() {
    if !can_collect() {
        return;
    }
    let Some(_operation) = Operation::enter() else {
        return;
    };
    loop {
        let (service, endpoint) = {
            let Some(_registry) = lock_registry(2) else {
                return;
            };
            unsafe {
                let r = raw();
                if !(*r).entries.retired.is_null() {
                    let node = (*r).entries.retired;
                    (*r).entries.retired = (*node).retired_next;
                    (node, ptr::null_mut())
                } else {
                    let _runtime = RuntimeGuard::acquire();
                    let mut link = ptr::addr_of_mut!((*r).endpoints.retired);
                    while !(*link).is_null() && !endpoint_quiescent(*link) {
                        link = ptr::addr_of_mut!((**link).retired_next);
                    }
                    let node = *link;
                    if !node.is_null() {
                        *link = (*node).retired_next;
                    }
                    (ptr::null_mut(), node)
                }
            }
        };
        if service.is_null() && endpoint.is_null() {
            return;
        }
        unsafe {
            let lifetime = if endpoint.is_null() {
                EndpointLifetimePerformance::EMPTY
            } else {
                (*endpoint).endpoint.lifetime
            };
            let timing = if endpoint.is_null() {
                [LockTiming::EMPTY; FAMILIES]
            } else {
                core::array::from_fn(|i| {
                    timing_snapshot(ptr::addr_of_mut!((*endpoint).endpoint.lock_timing[i]))
                })
            };
            let released = if !service.is_null() {
                heap::r4os_rust_heap_free(service.cast(), size_of::<ServiceNode>())
            } else {
                heap::r4os_rust_heap_free(endpoint.cast(), size_of::<EndpointNode>())
            };
            let _registry = lock_registry(2).expect("Service collection registry admission");
            let r = raw();
            if released != 0 {
                if !service.is_null() {
                    (*service).retired_next = (*r).entries.retired;
                    (*r).entries.retired = service;
                } else {
                    (*endpoint).retired_next = (*r).endpoints.retired;
                    (*r).endpoints.retired = endpoint;
                }
                return;
            }
            if !endpoint.is_null() {
                (*r).endpoints.allocated -= 1;
                add_lifetime(&mut (*r).retired_lifetime, lifetime);
                for (out, value) in (*r).retired_timing.iter_mut().zip(timing) {
                    add_timing(out, value);
                }
            }
        }
    }
}

/// Singleton boot or completely empty serialized metadata fixtures. Never
/// forget live records, native waits or the failed-release retry lists.
pub(super) unsafe fn reset() {
    unsafe {
        let r = raw();
        assert_eq!((*r).entries.count, 0);
        assert!((*r).entries.retired.is_null());
        assert_eq!((*r).endpoints.allocated, 0);
        assert!((*r).endpoints.retired.is_null());
        (*r).lock = Mutex {
            rank: 120,
            name: Text::literal(b"service-registry"),
            ..Mutex::EMPTY
        };
        (*r).timing = [LockTiming::EMPTY; FAMILIES];
        (*r).enumeration = RegistryEnumerationPerformance::EMPTY;
    }
}
