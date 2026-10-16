//! Service registry and endpoint admission. Native records have stable
//! addresses; no registry/endpoint reference survives a wait or foreign call.
use super::registry_types::*;
use crate::{
    build_bridge::config,
    contract as c,
    fs::vfs_types::Bytes,
    platform::monotonic,
    sched::{
        scheduler, sync_locks,
        sync_types::{Mutex, Text},
    },
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
const ZERO_STAMP: monotonic::Stamp = monotonic::Stamp {
    raw: 0,
    generation: 0,
    kind: 0,
};
pub(super) struct Registry {
    pub entries: [Entry; MAX_SERVICES],
    lock: Mutex,
    pub timing: [LockTiming; FAMILIES],
    pub api_slots: [u8; MAX_SERVICES],
    pub api_count: u32,
    pub generation: u64,
    pub next_start_generation: u64,
    pub enumeration: RegistryEnumerationPerformance,
    pub next_endpoint_handle: u32,
}
struct RegistryStorage(UnsafeCell<Registry>);
// SAFETY: the original service-registry mutex admits every bounded access;
// singleton boot/serialized owner fixtures alone reset the entire catalogue.
unsafe impl Sync for RegistryStorage {}
static REGISTRY: RegistryStorage = RegistryStorage(UnsafeCell::new(Registry {
    entries: [Entry::EMPTY; MAX_SERVICES],
    lock: Mutex {
        rank: 120,
        name: Text::literal(b"service-registry"),
        ..Mutex::EMPTY
    },
    timing: [LockTiming::EMPTY; FAMILIES],
    api_slots: [INVALID_SLOT; MAX_SERVICES],
    api_count: 0,
    generation: 1,
    next_start_generation: 0,
    enumeration: RegistryEnumerationPerformance::EMPTY,
    next_endpoint_handle: 1,
}));
struct EndpointStorage(UnsafeCell<[Endpoint; MAX_ENDPOINTS]>);
// SAFETY: lifecycle fields change under registry+exact endpoint admission;
// payload/queue metadata use the exact endpoint. Slot addresses never move.
unsafe impl Sync for EndpointStorage {}
static ENDPOINTS: EndpointStorage =
    EndpointStorage(UnsafeCell::new([const { Endpoint::EMPTY }; MAX_ENDPOINTS]));
pub(super) static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);
pub(super) fn raw() -> *mut Registry {
    REGISTRY.0.get()
}
pub(super) fn entry(slot: usize) -> *mut Entry {
    assert!(slot < MAX_SERVICES);
    unsafe {
        ptr::addr_of_mut!((*raw()).entries)
            .cast::<Entry>()
            .add(slot)
    }
}
pub(super) fn endpoint(slot: usize) -> *mut Endpoint {
    assert!(slot < MAX_ENDPOINTS);
    ENDPOINTS.0.get().cast::<Endpoint>().wrapping_add(slot)
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
    unsafe { (*raw.cast::<TimedLock>()).release() };
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
    pub _identity: Identity,
    pub guard: TimedLock,
}
pub(super) fn lock_identity(id: Identity, family: usize) -> Option<EndpointLease> {
    unsafe {
        let guard = endpoint_lock(id.endpoint, family, false)?;
        let ep = id.endpoint;
        (*ep).lifetime.revalidations = (*ep).lifetime.revalidations.wrapping_add(1);
        if !identity_matches(ep, id) {
            (*ep).lifetime.stale_rejections = (*ep).lifetime.stale_rejections.wrapping_add(1);
            return None;
        }
        Some(EndpointLease {
            endpoint: ep,
            _identity: id,
            guard,
        })
    }
}
pub(super) unsafe fn endpoint_for_slot(slot: usize) -> Option<usize> {
    (0..MAX_ENDPOINTS)
        .find(|&i| unsafe { (*endpoint(i)).used && (*endpoint(i)).service_slot == slot })
}
pub(super) unsafe fn endpoint_for_handle(handle: u32) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    (0..MAX_ENDPOINTS).find(|&i| unsafe { (*endpoint(i)).used && (*endpoint(i)).handle == handle })
}
pub(super) unsafe fn free_endpoint() -> Option<usize> {
    (0..MAX_ENDPOINTS).find(|&i| unsafe { !(*endpoint(i)).used })
}
pub(super) fn lookup_identity(handle: u32) -> Option<Identity> {
    let _guard = lock_registry(1)?;
    unsafe { endpoint_for_handle(handle).map(|i| identity(endpoint(i))) }
}
pub(super) fn lock_handle(handle: u32, family: usize) -> Option<EndpointLease> {
    lock_identity(lookup_identity(handle)?, family)
}
pub(super) fn wait_lifecycle(ep: *mut Endpoint) -> bool {
    unsafe { endpoint_lock(ep, 3, false) }.is_some()
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
pub(super) fn find(table: &[Entry; MAX_SERVICES], name: &[u8]) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    table
        .iter()
        .position(|e| e.used && e.name[..e.name_len].eq_ignore_ascii_case(name))
}
pub(super) fn count(table: &[Entry; MAX_SERVICES]) -> usize {
    table.iter().filter(|e| e.used).count()
}
pub(super) unsafe fn find_name(name: Bytes) -> Option<usize> {
    unsafe { find(&(*raw()).entries, bytes(name)) }
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
pub(super) fn register_in(
    table: &mut [Entry; MAX_SERVICES],
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
    if find(table, name).is_some() {
        return -3;
    }
    let Some(slot) = table.iter().position(|e| !e.used) else {
        return -2;
    };
    let e = &mut table[slot];
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
    slot as i32
}
pub(super) fn unregister_in(table: &mut [Entry; MAX_SERVICES], name: &[u8]) -> i32 {
    let Some(slot) = find(table, name) else {
        return -4;
    };
    table[slot] = Entry::EMPTY;
    0
}
pub(super) fn set_state_in(
    table: &mut [Entry; MAX_SERVICES],
    name: &[u8],
    state: u8,
    id: u32,
    code: i32,
    text: &[u8],
) -> i32 {
    if state == STATE_EMPTY {
        return -1;
    }
    let Some(slot) = find(table, name) else {
        return -4;
    };
    let e = &mut table[slot];
    e.state = state;
    e.start_generation = 0;
    e.instance_id = id;
    e.exit_code = code;
    e.last_error_len = copy(text, &mut e.last_error);
    0
}
pub(super) fn running_in(
    table: &mut [Entry; MAX_SERVICES],
    name: &[u8],
    id: u32,
    ticks: u64,
) -> i32 {
    let Some(slot) = find(table, name) else {
        return -4;
    };
    let e = &mut table[slot];
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
        (*r).generation = (*r).generation.wrapping_add(1);
        if (*r).generation == 0 {
            (*r).generation = 1;
        }
        (*r).api_slots.fill(INVALID_SLOT);
        (*r).api_count = 0;
        for i in 0..MAX_SERVICES {
            if (*entry(i)).used {
                (*r).api_slots[(*r).api_count as usize] = i as u8;
                (*r).api_count += 1;
            }
        }
    }
}
pub(super) unsafe fn api_slot(index: u32) -> Option<usize> {
    unsafe {
        let r = raw();
        if index >= (*r).api_count {
            return None;
        }
        let slot = (*r).api_slots[index as usize] as usize;
        (slot < MAX_SERVICES && (*entry(slot)).used).then_some(slot)
    }
}
pub(super) unsafe fn target_slot(target: &ApiIndexTarget, require_instance: bool) -> Option<usize> {
    unsafe {
        if target.registry_generation != (*raw()).generation {
            return None;
        }
        let slot = api_slot(target.api_index)?;
        if slot != target.service_slot {
            return None;
        }
        let e = &*entry(slot);
        if e.name_len != target.name_len
            || !e.name[..e.name_len].eq_ignore_ascii_case(&target.name[..target.name_len])
        {
            return None;
        }
        if require_instance && e.instance_id != target.instance_id {
            return None;
        }
        Some(slot)
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
        let e = &*entry(slot);
        let mut result = ApiIndexTarget {
            registry_generation: (*r).generation,
            api_index: index,
            service_slot: slot,
            instance_id: e.instance_id,
            state: e.state,
            name_len: e.name_len,
            ..ApiIndexTarget::EMPTY
        };
        result.name[..e.name_len].copy_from_slice(&e.name[..e.name_len]);
        Some(result)
    }
}

/// Singleton boot or serialized metadata fixtures, with no live service task.
pub(super) unsafe fn reset() {
    unsafe {
        let r = raw();
        for i in 0..MAX_SERVICES {
            reg_entry_reset(i);
        }
        (*r).lock = Mutex {
            rank: 120,
            name: Text::literal(b"service-registry"),
            ..Mutex::EMPTY
        };
        (*r).timing = [LockTiming::EMPTY; FAMILIES];
        (*r).api_slots = [INVALID_SLOT; MAX_SERVICES];
        (*r).api_count = 0;
        (*r).generation = 1;
        (*r).next_start_generation = 0;
        (*r).enumeration = RegistryEnumerationPerformance::EMPTY;
        for i in 0..MAX_ENDPOINTS {
            let ep = endpoint(i);
            if (*ep).used {
                super::endpoint::wake_waiters(ep, 3);
            }
            super::endpoint::reset_metadata(ep, false);
            (*ep).lock = Mutex {
                rank: 130,
                name: Text::literal(b"service-endpoint"),
                ..Mutex::EMPTY
            };
            (*ep).generation = 0;
            (*ep).request_id_next = 0;
            (*ep).request_id_limit = 0;
            (*ep).lifetime = EndpointLifetimePerformance::EMPTY;
            (*ep).lock_timing = [LockTiming::EMPTY; FAMILIES];
        }
        (*r).next_endpoint_handle = 1;
        NEXT_REQUEST.store(1, Ordering::Relaxed);
    }
}
unsafe fn reg_entry_reset(index: usize) {
    unsafe { entry(index).write(Entry::EMPTY) };
}
