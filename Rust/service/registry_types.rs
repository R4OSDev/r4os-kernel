//! Fixed private service records; original fixtures lend the same layouts.
use crate::sched::sync_types::{Mutex, Semaphore, Text, WaitQueue};
use crate::{build_config::config, contract as c};
pub const MAX_SERVICES: usize = 16;
pub const MAX_ENDPOINTS: usize = MAX_SERVICES;
pub const DEPTH: usize = crate::contract::service_api_endpoint_queue_depth;
pub const PAYLOAD: usize = crate::contract::service_api_max_payload;
pub const FAMILIES: usize = 7;
pub const TIMING_STRIDE: u64 = 64;
pub const REQUEST_BLOCK: u32 = 256;
pub const REQUEST_MAX: u32 = 0x7fff_fffe;
pub const INVALID_SLOT: u8 = 255;
pub const NAMED_TARGET_INDEX: u32 = u32::MAX;
pub const FOREVER: u64 = u64::MAX;
pub const STATE_EMPTY: u8 = 0;
pub const STATE_STOPPED: u8 = 1;
pub const STATE_STARTING: u8 = 2;
pub const STATE_RUNNING: u8 = 3;
pub const STATE_STOPPING: u8 = 4;
pub const STATE_FAILED: u8 = 5;
pub const STATE_DISABLED: u8 = 6;
pub const MODE_DISABLED: u8 = 2;
pub const REQUEST_FREE: u8 = 0;
pub const REQUEST_QUEUED: u8 = 1;
pub const REQUEST_DELIVERED: u8 = 2;
pub const REQUEST_RESPONDED: u8 = 3;
pub const EMPTY_BINDING: c::ProgramObservationHandle = c::ProgramObservationHandle {
    process: c::ProgramProcessHandle {
        instance_id: 0,
        reserved: 0,
        generation: 0,
    },
    id: 0,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LockTiming {
    pub acquisitions: u64,
    pub contentions: u64,
    pub timing_samples: u64,
    pub wait_ns: u64,
    pub wait_max_ns: u64,
    pub hold_ns: u64,
    pub hold_max_ns: u64,
    pub timing_unavailable: u64,
}
impl LockTiming {
    pub const EMPTY: Self = Self {
        acquisitions: 0,
        contentions: 0,
        timing_samples: 0,
        wait_ns: 0,
        wait_max_ns: 0,
        hold_ns: 0,
        hold_max_ns: 0,
        timing_unavailable: 0,
    };
}
impl Default for LockTiming {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EndpointLifetimePerformance {
    pub payload_copy_bytes: u64,
    pub payload_clear_bytes: u64,
    pub slot_metadata_resets: u64,
    pub endpoint_metadata_resets: u64,
    pub endpoint_payload_reset_bytes: u64,
    pub queue_scan_passes: u64,
    pub queue_scan_slots: u64,
    pub revalidations: u64,
    pub stale_rejections: u64,
}
impl EndpointLifetimePerformance {
    pub const EMPTY: Self = Self {
        payload_copy_bytes: 0,
        payload_clear_bytes: 0,
        slot_metadata_resets: 0,
        endpoint_metadata_resets: 0,
        endpoint_payload_reset_bytes: 0,
        queue_scan_passes: 0,
        queue_scan_slots: 0,
        revalidations: 0,
        stale_rejections: 0,
    };
}
impl Default for EndpointLifetimePerformance {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub used: bool,
    pub state: u8,
    pub start_mode: u8,
    pub instance_id: u32,
    pub start_generation: u64,
    pub exit_code: i32,
    pub start_tick: u64,
    pub restart_count: u32,
    pub name: [u8; 32],
    pub name_len: usize,
    pub path: [u8; 1024],
    pub path_len: usize,
    pub args: [u8; 96],
    pub args_len: usize,
    pub description: [u8; 80],
    pub description_len: usize,
    pub last_error: [u8; 64],
    pub last_error_len: usize,
    // The actual launch identity and its detached completion lease survive
    // starter exit/owner reap. A stopped slot keeps its last real evidence
    // until the next start/remove releases it outside this registry owner.
    pub binding: c::ProgramObservationHandle,
}
impl Entry {
    pub const EMPTY: Self = Self {
        used: false,
        state: 0,
        start_mode: 0,
        instance_id: 0,
        start_generation: 0,
        exit_code: 0,
        start_tick: 0,
        restart_count: 0,
        name: [0; 32],
        name_len: 0,
        path: [0; 1024],
        path_len: 0,
        args: [0; 96],
        args_len: 0,
        description: [0; 80],
        description_len: 0,
        last_error: [0; 64],
        last_error_len: 0,
        binding: EMPTY_BINDING,
    };
}
impl Default for Entry {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiIndexTarget {
    pub registry_generation: u64,
    pub api_index: u32,
    pub service_slot: usize,
    pub instance_id: u32,
    pub state: u8,
    pub name: [u8; 32],
    pub name_len: usize,
    pub start_generation: u64,
    pub binding: c::ProgramObservationHandle,
}
impl ApiIndexTarget {
    pub const EMPTY: Self = Self {
        registry_generation: 0,
        api_index: 0,
        service_slot: 0,
        instance_id: 0,
        state: 0,
        name: [0; 32],
        name_len: 0,
        start_generation: 0,
        binding: EMPTY_BINDING,
    };
}
impl Default for ApiIndexTarget {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
pub struct RequestSlot {
    pub state: u8,
    pub request_id: u32,
    pub client_id: u32,
    pub op: u16,
    pub flags: u32,
    pub request_len: u16,
    pub response_len: u16,
    pub response_status: i32,
    pub response_available: WaitQueue,
    pub request_payload: [u8; 4096],
    pub response_payload: [u8; 4096],
}
impl RequestSlot {
    pub const EMPTY: Self = Self {
        state: 0,
        request_id: 0,
        client_id: 0,
        op: 0,
        flags: 0,
        request_len: 0,
        response_len: 0,
        response_status: 0,
        response_available: WaitQueue::EMPTY,
        request_payload: [0; 4096],
        response_payload: [0; 4096],
    };
}
impl Default for RequestSlot {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
pub struct Endpoint {
    pub lock: Mutex,
    pub generation: u64,
    pub request_id_next: u32,
    pub request_id_limit: u32,
    pub lifetime: EndpointLifetimePerformance,
    pub lock_timing: [LockTiming; 7],
    pub used: bool,
    pub service_slot: usize,
    pub handle: u32,
    pub flags: u32,
    pub queue_high_water: u32,
    pub max_active_workers: u32,
    pub open_handles: u32,
    pub requests: u64,
    pub responses: u64,
    pub drops: u64,
    pub busy_rejections: u64,
    pub timeouts: u64,
    pub cancellations: u64,
    pub completion_waits: u64,
    pub completion_timeouts: u64,
    pub completion_wait_rounds: u64,
    pub targeted_response_wakes: u64,
    pub targeted_response_wake_misses: u64,
    pub admission_waits: u64,
    pub admission_timeouts: u64,
    pub requests_available: WaitQueue,
    pub slots_available: Semaphore,
    pub queued_slots: [usize; 8],
    pub queued_count: usize,
    pub queue: [RequestSlot; 8],
}
impl Endpoint {
    pub const EMPTY: Self = Self {
        lock: Mutex {
            rank: 130,
            name: Text::literal(b"service-endpoint"),
            ..Mutex::EMPTY
        },
        generation: 0,
        request_id_next: 0,
        request_id_limit: 0,
        lifetime: EndpointLifetimePerformance::EMPTY,
        lock_timing: [const { LockTiming::EMPTY }; 7],
        used: false,
        service_slot: 0,
        handle: 0,
        flags: 0,
        queue_high_water: 0,
        max_active_workers: 0,
        open_handles: 0,
        requests: 0,
        responses: 0,
        drops: 0,
        busy_rejections: 0,
        timeouts: 0,
        cancellations: 0,
        completion_waits: 0,
        completion_timeouts: 0,
        completion_wait_rounds: 0,
        targeted_response_wakes: 0,
        targeted_response_wake_misses: 0,
        admission_waits: 0,
        admission_timeouts: 0,
        requests_available: WaitQueue::EMPTY,
        slots_available: Semaphore {
            count: DEPTH as u32,
            max_count: DEPTH as u32,
            queue: WaitQueue::EMPTY,
        },
        queued_slots: [0; 8],
        queued_count: 0,
        queue: [const { RequestSlot::EMPTY }; 8],
    };
}
impl Default for Endpoint {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PerformanceSummary {
    pub max_services: u32,
    pub used_services: u32,
    pub running_services: u32,
    pub endpoints_used: u32,
    pub request_pending: u32,
    pub response_pending: u32,
    pub queue_depth_total: u32,
    pub queue_used_total: u32,
    pub queue_high_water_total: u32,
    pub active_workers: u32,
    pub max_active_workers: u32,
    pub open_handles: u32,
    pub requests: u64,
    pub responses: u64,
    pub drops: u64,
    pub busy_rejections: u64,
    pub timeouts: u64,
    pub cancellations: u64,
    pub completion_waits: u64,
    pub completion_timeouts: u64,
    pub completion_wait_rounds: u64,
    pub targeted_response_wakes: u64,
    pub targeted_response_wake_misses: u64,
    pub admission_waits: u64,
    pub admission_timeouts: u64,
    pub payload_copy_bytes: u64,
    pub payload_clear_bytes: u64,
    pub slot_metadata_resets: u64,
    pub endpoint_metadata_resets: u64,
    pub endpoint_payload_reset_bytes: u64,
    pub queue_scan_passes: u64,
    pub queue_scan_slots: u64,
    pub endpoint_revalidations: u64,
    pub endpoint_stale_rejections: u64,
    pub lock_family_count: u32,
    pub lock_reserved0: u32,
    pub lock_acquisitions: [u64; 7],
    pub lock_contentions: [u64; 7],
    pub lock_wait_ns: [u64; 7],
    pub lock_wait_max_ns: [u64; 7],
    pub lock_hold_ns: [u64; 7],
    pub lock_hold_max_ns: [u64; 7],
    pub lock_timing_unavailable: [u64; 7],
    pub lock_timing_stride: u32,
    pub lock_timing_reserved0: u32,
    pub lock_timing_samples: [u64; 7],
    pub registry_index_queries: u64,
    pub registry_refresh_requests: u64,
    pub registry_refresh_visits: u64,
    pub registry_instance_lookups: u64,
    pub registry_index_end_markers: u64,
}
impl PerformanceSummary {
    pub const EMPTY: Self = Self {
        max_services: MAX_SERVICES as u32,
        used_services: 0,
        running_services: 0,
        endpoints_used: 0,
        request_pending: 0,
        response_pending: 0,
        queue_depth_total: 0,
        queue_used_total: 0,
        queue_high_water_total: 0,
        active_workers: 0,
        max_active_workers: 0,
        open_handles: 0,
        requests: 0,
        responses: 0,
        drops: 0,
        busy_rejections: 0,
        timeouts: 0,
        cancellations: 0,
        completion_waits: 0,
        completion_timeouts: 0,
        completion_wait_rounds: 0,
        targeted_response_wakes: 0,
        targeted_response_wake_misses: 0,
        admission_waits: 0,
        admission_timeouts: 0,
        payload_copy_bytes: 0,
        payload_clear_bytes: 0,
        slot_metadata_resets: 0,
        endpoint_metadata_resets: 0,
        endpoint_payload_reset_bytes: 0,
        queue_scan_passes: 0,
        queue_scan_slots: 0,
        endpoint_revalidations: 0,
        endpoint_stale_rejections: 0,
        lock_family_count: FAMILIES as u32,
        lock_reserved0: 0,
        lock_acquisitions: [0; 7],
        lock_contentions: [0; 7],
        lock_wait_ns: [0; 7],
        lock_wait_max_ns: [0; 7],
        lock_hold_ns: [0; 7],
        lock_hold_max_ns: [0; 7],
        lock_timing_unavailable: [0; 7],
        lock_timing_stride: if config::ENABLE_METRICS {
            TIMING_STRIDE as u32
        } else {
            0
        },
        lock_timing_reserved0: 0,
        lock_timing_samples: [0; 7],
        registry_index_queries: 0,
        registry_refresh_requests: 0,
        registry_refresh_visits: 0,
        registry_instance_lookups: 0,
        registry_index_end_markers: 0,
    };
}
impl Default for PerformanceSummary {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RegistryEnumerationPerformance {
    pub index_queries: u64,
    pub refresh_requests: u64,
    pub refresh_visits: u64,
    pub instance_lookups: u64,
    pub index_end_markers: u64,
}
impl RegistryEnumerationPerformance {
    pub const EMPTY: Self = Self {
        index_queries: 0,
        refresh_requests: 0,
        refresh_visits: 0,
        instance_lookups: 0,
        index_end_markers: 0,
    };
}
impl Default for RegistryEnumerationPerformance {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StartReservation {
    pub slot: usize,
    pub generation: u64,
    pub entry: Entry,
    pub registry_generation: u64,
}
impl StartReservation {
    pub const EMPTY: Self = Self {
        slot: 0,
        generation: 0,
        entry: Entry::EMPTY,
        registry_generation: 0,
    };
}
impl Default for StartReservation {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct QueueCounts {
    pub queued: u32,
    pub delivered: u32,
    pub responded: u32,
    pub used: u32,
}
impl QueueCounts {
    pub const EMPTY: Self = Self {
        queued: 0,
        delivered: 0,
        responded: 0,
        used: 0,
    };
}
impl Default for QueueCounts {
    fn default() -> Self {
        Self::EMPTY
    }
}

const _: () = {
    assert!(core::mem::offset_of!(LockTiming, acquisitions) == 0);
    assert!(core::mem::offset_of!(LockTiming, contentions) == 8);
    assert!(core::mem::offset_of!(LockTiming, timing_samples) == 16);
    assert!(core::mem::offset_of!(LockTiming, wait_ns) == 24);
    assert!(core::mem::offset_of!(LockTiming, wait_max_ns) == 32);
    assert!(core::mem::offset_of!(LockTiming, hold_ns) == 40);
    assert!(core::mem::offset_of!(LockTiming, hold_max_ns) == 48);
    assert!(core::mem::offset_of!(LockTiming, timing_unavailable) == 56);
    assert!(core::mem::size_of::<LockTiming>() == 64);
    assert!(core::mem::align_of::<LockTiming>() == 8);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, payload_copy_bytes) == 0);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, payload_clear_bytes) == 8);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, slot_metadata_resets) == 16);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, endpoint_metadata_resets) == 24);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, endpoint_payload_reset_bytes) == 32);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, queue_scan_passes) == 40);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, queue_scan_slots) == 48);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, revalidations) == 56);
    assert!(core::mem::offset_of!(EndpointLifetimePerformance, stale_rejections) == 64);
    assert!(core::mem::size_of::<EndpointLifetimePerformance>() == 72);
    assert!(core::mem::align_of::<EndpointLifetimePerformance>() == 8);
    assert!(core::mem::offset_of!(Entry, used) == 0);
    assert!(core::mem::offset_of!(Entry, state) == 1);
    assert!(core::mem::offset_of!(Entry, start_mode) == 2);
    assert!(core::mem::offset_of!(Entry, instance_id) == 4);
    assert!(core::mem::offset_of!(Entry, start_generation) == 8);
    assert!(core::mem::offset_of!(Entry, exit_code) == 16);
    assert!(core::mem::offset_of!(Entry, start_tick) == 24);
    assert!(core::mem::offset_of!(Entry, restart_count) == 32);
    assert!(core::mem::offset_of!(Entry, name) == 36);
    assert!(core::mem::offset_of!(Entry, name_len) == 72);
    assert!(core::mem::offset_of!(Entry, path) == 80);
    assert!(core::mem::offset_of!(Entry, path_len) == 1104);
    assert!(core::mem::offset_of!(Entry, args) == 1112);
    assert!(core::mem::offset_of!(Entry, args_len) == 1208);
    assert!(core::mem::offset_of!(Entry, description) == 1216);
    assert!(core::mem::offset_of!(Entry, description_len) == 1296);
    assert!(core::mem::offset_of!(Entry, last_error) == 1304);
    assert!(core::mem::offset_of!(Entry, last_error_len) == 1368);
    assert!(core::mem::offset_of!(Entry, binding) == 1376);
    assert!(core::mem::size_of::<Entry>() == 1400);
    assert!(core::mem::align_of::<Entry>() == 8);
    assert!(core::mem::offset_of!(ApiIndexTarget, registry_generation) == 0);
    assert!(core::mem::offset_of!(ApiIndexTarget, api_index) == 8);
    assert!(core::mem::offset_of!(ApiIndexTarget, service_slot) == 16);
    assert!(core::mem::offset_of!(ApiIndexTarget, instance_id) == 24);
    assert!(core::mem::offset_of!(ApiIndexTarget, state) == 28);
    assert!(core::mem::offset_of!(ApiIndexTarget, name) == 29);
    assert!(core::mem::offset_of!(ApiIndexTarget, name_len) == 64);
    assert!(core::mem::offset_of!(ApiIndexTarget, start_generation) == 72);
    assert!(core::mem::offset_of!(ApiIndexTarget, binding) == 80);
    assert!(core::mem::size_of::<ApiIndexTarget>() == 104);
    assert!(core::mem::align_of::<ApiIndexTarget>() == 8);
    assert!(core::mem::offset_of!(RequestSlot, state) == 0);
    assert!(core::mem::offset_of!(RequestSlot, request_id) == 4);
    assert!(core::mem::offset_of!(RequestSlot, client_id) == 8);
    assert!(core::mem::offset_of!(RequestSlot, op) == 12);
    assert!(core::mem::offset_of!(RequestSlot, flags) == 16);
    assert!(core::mem::offset_of!(RequestSlot, request_len) == 20);
    assert!(core::mem::offset_of!(RequestSlot, response_len) == 22);
    assert!(core::mem::offset_of!(RequestSlot, response_status) == 24);
    assert!(core::mem::offset_of!(RequestSlot, response_available) == 32);
    assert!(core::mem::offset_of!(RequestSlot, request_payload) == 112);
    assert!(core::mem::offset_of!(RequestSlot, response_payload) == 4208);
    assert!(core::mem::size_of::<RequestSlot>() == 8304);
    assert!(core::mem::align_of::<RequestSlot>() == 8);
    assert!(core::mem::offset_of!(Endpoint, lock) == 0);
    assert!(core::mem::offset_of!(Endpoint, generation) == 168);
    assert!(core::mem::offset_of!(Endpoint, request_id_next) == 176);
    assert!(core::mem::offset_of!(Endpoint, request_id_limit) == 180);
    assert!(core::mem::offset_of!(Endpoint, lifetime) == 184);
    assert!(core::mem::offset_of!(Endpoint, lock_timing) == 256);
    assert!(core::mem::offset_of!(Endpoint, used) == 704);
    assert!(core::mem::offset_of!(Endpoint, service_slot) == 712);
    assert!(core::mem::offset_of!(Endpoint, handle) == 720);
    assert!(core::mem::offset_of!(Endpoint, flags) == 724);
    assert!(core::mem::offset_of!(Endpoint, queue_high_water) == 728);
    assert!(core::mem::offset_of!(Endpoint, max_active_workers) == 732);
    assert!(core::mem::offset_of!(Endpoint, open_handles) == 736);
    assert!(core::mem::offset_of!(Endpoint, requests) == 744);
    assert!(core::mem::offset_of!(Endpoint, responses) == 752);
    assert!(core::mem::offset_of!(Endpoint, drops) == 760);
    assert!(core::mem::offset_of!(Endpoint, busy_rejections) == 768);
    assert!(core::mem::offset_of!(Endpoint, timeouts) == 776);
    assert!(core::mem::offset_of!(Endpoint, cancellations) == 784);
    assert!(core::mem::offset_of!(Endpoint, completion_waits) == 792);
    assert!(core::mem::offset_of!(Endpoint, completion_timeouts) == 800);
    assert!(core::mem::offset_of!(Endpoint, completion_wait_rounds) == 808);
    assert!(core::mem::offset_of!(Endpoint, targeted_response_wakes) == 816);
    assert!(core::mem::offset_of!(Endpoint, targeted_response_wake_misses) == 824);
    assert!(core::mem::offset_of!(Endpoint, admission_waits) == 832);
    assert!(core::mem::offset_of!(Endpoint, admission_timeouts) == 840);
    assert!(core::mem::offset_of!(Endpoint, requests_available) == 848);
    assert!(core::mem::offset_of!(Endpoint, slots_available) == 928);
    assert!(core::mem::offset_of!(Endpoint, queued_slots) == 1016);
    assert!(core::mem::offset_of!(Endpoint, queued_count) == 1080);
    assert!(core::mem::offset_of!(Endpoint, queue) == 1088);
    assert!(core::mem::size_of::<Endpoint>() == 67520);
    assert!(core::mem::align_of::<Endpoint>() == 8);
    assert!(core::mem::offset_of!(PerformanceSummary, max_services) == 0);
    assert!(core::mem::offset_of!(PerformanceSummary, used_services) == 4);
    assert!(core::mem::offset_of!(PerformanceSummary, running_services) == 8);
    assert!(core::mem::offset_of!(PerformanceSummary, endpoints_used) == 12);
    assert!(core::mem::offset_of!(PerformanceSummary, request_pending) == 16);
    assert!(core::mem::offset_of!(PerformanceSummary, response_pending) == 20);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_depth_total) == 24);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_used_total) == 28);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_high_water_total) == 32);
    assert!(core::mem::offset_of!(PerformanceSummary, active_workers) == 36);
    assert!(core::mem::offset_of!(PerformanceSummary, max_active_workers) == 40);
    assert!(core::mem::offset_of!(PerformanceSummary, open_handles) == 44);
    assert!(core::mem::offset_of!(PerformanceSummary, requests) == 48);
    assert!(core::mem::offset_of!(PerformanceSummary, responses) == 56);
    assert!(core::mem::offset_of!(PerformanceSummary, drops) == 64);
    assert!(core::mem::offset_of!(PerformanceSummary, busy_rejections) == 72);
    assert!(core::mem::offset_of!(PerformanceSummary, timeouts) == 80);
    assert!(core::mem::offset_of!(PerformanceSummary, cancellations) == 88);
    assert!(core::mem::offset_of!(PerformanceSummary, completion_waits) == 96);
    assert!(core::mem::offset_of!(PerformanceSummary, completion_timeouts) == 104);
    assert!(core::mem::offset_of!(PerformanceSummary, completion_wait_rounds) == 112);
    assert!(core::mem::offset_of!(PerformanceSummary, targeted_response_wakes) == 120);
    assert!(core::mem::offset_of!(PerformanceSummary, targeted_response_wake_misses) == 128);
    assert!(core::mem::offset_of!(PerformanceSummary, admission_waits) == 136);
    assert!(core::mem::offset_of!(PerformanceSummary, admission_timeouts) == 144);
    assert!(core::mem::offset_of!(PerformanceSummary, payload_copy_bytes) == 152);
    assert!(core::mem::offset_of!(PerformanceSummary, payload_clear_bytes) == 160);
    assert!(core::mem::offset_of!(PerformanceSummary, slot_metadata_resets) == 168);
    assert!(core::mem::offset_of!(PerformanceSummary, endpoint_metadata_resets) == 176);
    assert!(core::mem::offset_of!(PerformanceSummary, endpoint_payload_reset_bytes) == 184);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_scan_passes) == 192);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_scan_slots) == 200);
    assert!(core::mem::offset_of!(PerformanceSummary, endpoint_revalidations) == 208);
    assert!(core::mem::offset_of!(PerformanceSummary, endpoint_stale_rejections) == 216);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_family_count) == 224);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_reserved0) == 228);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_acquisitions) == 232);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_contentions) == 288);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_wait_ns) == 344);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_wait_max_ns) == 400);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_hold_ns) == 456);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_hold_max_ns) == 512);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_timing_unavailable) == 568);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_timing_stride) == 624);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_timing_reserved0) == 628);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_timing_samples) == 632);
    assert!(core::mem::offset_of!(PerformanceSummary, registry_index_queries) == 688);
    assert!(core::mem::offset_of!(PerformanceSummary, registry_refresh_requests) == 696);
    assert!(core::mem::offset_of!(PerformanceSummary, registry_refresh_visits) == 704);
    assert!(core::mem::offset_of!(PerformanceSummary, registry_instance_lookups) == 712);
    assert!(core::mem::offset_of!(PerformanceSummary, registry_index_end_markers) == 720);
    assert!(core::mem::size_of::<PerformanceSummary>() == 728);
    assert!(core::mem::align_of::<PerformanceSummary>() == 8);
    assert!(core::mem::offset_of!(RegistryEnumerationPerformance, index_queries) == 0);
    assert!(core::mem::offset_of!(RegistryEnumerationPerformance, refresh_requests) == 8);
    assert!(core::mem::offset_of!(RegistryEnumerationPerformance, refresh_visits) == 16);
    assert!(core::mem::offset_of!(RegistryEnumerationPerformance, instance_lookups) == 24);
    assert!(core::mem::offset_of!(RegistryEnumerationPerformance, index_end_markers) == 32);
    assert!(core::mem::size_of::<RegistryEnumerationPerformance>() == 40);
    assert!(core::mem::align_of::<RegistryEnumerationPerformance>() == 8);
    assert!(core::mem::offset_of!(StartReservation, slot) == 0);
    assert!(core::mem::offset_of!(StartReservation, generation) == 8);
    assert!(core::mem::offset_of!(StartReservation, entry) == 16);
    assert!(core::mem::offset_of!(StartReservation, registry_generation) == 1416);
    assert!(core::mem::size_of::<StartReservation>() == 1424);
    assert!(core::mem::align_of::<StartReservation>() == 8);
    assert!(core::mem::offset_of!(QueueCounts, queued) == 0);
    assert!(core::mem::offset_of!(QueueCounts, delivered) == 4);
    assert!(core::mem::offset_of!(QueueCounts, responded) == 8);
    assert!(core::mem::offset_of!(QueueCounts, used) == 12);
    assert!(core::mem::size_of::<QueueCounts>() == 16);
    assert!(core::mem::align_of::<QueueCounts>() == 4);
};
